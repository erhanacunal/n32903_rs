//! SD card host for N32903 (W55FA93) — the SIC's SD engine
//! Ported from the N3290x Non-OS BSP SD driver (`Loader/SDLoader/sd.c`:
//! `fmiInitDevice`, `fmiInitSDDevice`, `fmiSD_Init`, `fmiSelectCard`,
//! `fmiGet_SD_info`, `fmiSD_Read`, `fmiSD_Write`).
//!
//! SD memory cards only (SDSC v1.x/v2.0 and SDHC/SDXC); MMC and the
//! high-speed switch are not ported. Transfers run at the default-speed
//! clock with a 4-bit bus, by DMA, polled.
//!
//! Differences from the C driver:
//! - Every wait is bounded (the C code spins until the card is pulled), so a
//!   missing card makes [`sd_init_device`] fail instead of hanging.
//! - The FMI itself is not reset: the SD engine shares it (and the DMA
//!   channel) with the NAND controller, and an FMI reset drops the NAND
//!   configuration. Each operation selects the SD function in FMICR, as
//!   [`crate::nand`] selects SM.
//!
//! # Safety
//!
//! All functions in this module are `unsafe` because they program the
//! hardware directly. Don't run them concurrently with NAND operations.

use crate::cache;
use crate::clock::{self, ClockNode};
use crate::mmu::UNCACHED_ALIAS;
use crate::registers::*;

// ============================================================================
// Public Types
// ============================================================================

/// SD sector size.
pub const SD_SECTOR_SIZE: u32 = 512;

/// DMA buffers must start on a cache line.
pub const BUF_ALIGN: usize = cache::CACHE_LINE as usize;

/// SD host port (pin group)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SdPort {
    /// GPE2–GPE7
    Port0,
    /// GPB0–GPB5
    Port1,
    /// GPE8–GPE9 (DAT0–1) + GPD5–GPD8 (CLK, CMD, DAT2–3)
    Port2,
}

/// Card type (`FMI_TYPE_*`)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SdCardType {
    /// Standard capacity (SDSC, byte addressed): v1.x or v2.0
    SdLow,
    /// High capacity (SDHC/SDXC, block addressed)
    SdHigh,
}

/// An initialised card (`FMI_SD_INFO_T`)
#[derive(Clone, Copy, Debug)]
pub struct SdInfo {
    pub card_type: SdCardType,
    pub port: SdPort,
    /// Relative card address, already shifted into argument bits 31..16
    pub rca: u32,
    /// Capacity in 512-byte sectors
    pub total_sectors: u32,
}

/// SD error
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SdError {
    /// No response / transfer did not finish (also: no card)
    Timeout,
    /// CRC error on a response or data block
    Crc,
    /// The card is not an SD memory card (e.g. MMC)
    Unsupported,
    /// Buffer not [`BUF_ALIGN`]-aligned, or zero sectors
    BadBuffer,
}

// ============================================================================
// Configuration
// ============================================================================

/// Identification clock (vendor: 200 kHz).
const INIT_KHZ: u32 = 200;
/// Transfer clock (vendor `SD_FREQ`).
const SD_KHZ: u32 = 24_000;

// Poll bounds, in register reads (a few seconds at most at 200 MHz).
const CMD_POLLS: u32 = 0x0010_0000;
const DATA_POLLS: u32 = 0x0100_0000;
const BUSY_POLLS: u32 = 0x0001_0000;
/// ACMD41 attempts; each takes ~1 ms at the identification clock.
const READY_TRIES: u32 = 1000;

/// SDCR bits kept when issuing a command / a data command.
const SDCR_KEEP_CMD: u32 = 0xFFFF_C080;
const SDCR_KEEP_DATA: u32 = 0xFF00_C080;

// ============================================================================
// Host Control
// ============================================================================

/// Physical (cacheable-view) address of a buffer, accepting either view.
fn phys(ptr: u32) -> u32 {
    ptr & !UNCACHED_ALIAS
}

/// Poll until `done` holds, at most `polls` times.
#[inline(always)]
unsafe fn wait(polls: u32, done: impl Fn() -> bool) -> Result<(), SdError> {
    for _ in 0..polls {
        if done() {
            return Ok(());
        }
    }
    Err(SdError::Timeout)
}

/// Reset the SD engine (after a timeout), keeping the port and bus width.
unsafe fn reset_engine() {
    let keep = reg_read32(REG_SDCR) & (SDCR_SDPORT | SDCR_DBW);
    reg_write32(REG_SDCR, reg_read32(REG_SDCR) | SDCR_SWRST);
    let _ = wait(CMD_POLLS, || reg_read32(REG_SDCR) & SDCR_SWRST == 0);
    reg_write32(REG_SDCR, (reg_read32(REG_SDCR) & !(SDCR_SDPORT | SDCR_DBW)) | keep);
    reg_write32(REG_SDISR, 0xFFFF);
}

/// Wait for the engine to clear `bits` in SDCR; reset it on timeout.
unsafe fn wait_sdcr(bits: u32, polls: u32) -> Result<(), SdError> {
    let r = wait(polls, || reg_read32(REG_SDCR) & bits == 0);
    if r.is_err() {
        reset_engine();
    }
    r
}

/// Set the SD clock from UPLL to at most `khz`.
unsafe fn set_clock(khz: u32) {
    let src = clock::get_clock(ClockNode::UPll);
    let total = src.div_ceil(khz).max(2);
    // Pre-divide by at least 2, as the vendor code does.
    let n0 = (2..=8).find(|n| total.div_ceil(*n) <= 256).unwrap_or(8);
    let n1 = total.div_ceil(n0).clamp(1, 256);
    let v = reg_read32(REG_CLKDIV2) & !(SD_N1 | SD_S | SD_N0);
    reg_write32(REG_CLKDIV2, v | ((n1 - 1) << 24) | (3 << 19) | ((n0 - 1) << 16));
    for _ in 0..1000 {
        core::hint::spin_loop();
    }
}

/// `fmiSDCommand`: command without response.
unsafe fn sd_command(cmd: u32, arg: u32) -> Result<(), SdError> {
    reg_write32(REG_SDARG, arg);
    reg_write32(REG_SDCR, (reg_read32(REG_SDCR) & SDCR_KEEP_CMD) | (cmd << 8) | SDCR_CO_EN);
    wait_sdcr(SDCR_CO_EN, CMD_POLLS)
}

/// `fmiSDCmdAndRsp`: command with a 48-bit response. R3 carries no valid
/// CRC, so `check_crc` is false for it.
unsafe fn sd_cmd_rsp(cmd: u32, arg: u32, check_crc: bool) -> Result<(), SdError> {
    reg_write32(REG_SDARG, arg);
    reg_write32(
        REG_SDCR,
        (reg_read32(REG_SDCR) & SDCR_KEEP_CMD) | (cmd << 8) | SDCR_CO_EN | SDCR_RI_EN,
    );
    wait_sdcr(SDCR_RI_EN, CMD_POLLS)?;
    if !check_crc {
        reg_write32(REG_SDISR, SDISR_CRC_IF);
    } else if reg_read32(REG_SDISR) & SDISR_CRC_7 == 0 {
        reg_write32(REG_SDISR, SDISR_CRC_IF);
        return Err(SdError::Crc);
    }
    Ok(())
}

/// `fmiSDCmdAndRsp2`: command with a 136-bit response (CID / CSD), as four
/// words, most significant first (bits 127..8; the CRC byte is dropped).
unsafe fn sd_cmd_rsp2(cmd: u32, arg: u32) -> Result<[u32; 4], SdError> {
    reg_write32(REG_SDARG, arg);
    reg_write32(
        REG_SDCR,
        (reg_read32(REG_SDCR) & SDCR_KEEP_CMD) | (cmd << 8) | SDCR_CO_EN | SDCR_R2_EN,
    );
    wait_sdcr(SDCR_R2_EN, CMD_POLLS)?;
    if reg_read32(REG_SDISR) & SDISR_CRC_7 == 0 {
        return Err(SdError::Crc);
    }
    let mut tmp = [0u32; 5];
    for (i, t) in tmp.iter_mut().enumerate() {
        *t = reg_read32(REG_FB_0 + i as u32 * 4).swap_bytes();
    }
    let mut out = [0u32; 4];
    for i in 0..4 {
        out[i] = ((tmp[i] & 0x00FF_FFFF) << 8) | ((tmp[i + 1] & 0xFF00_0000) >> 24);
    }
    Ok(out)
}

/// `fmiCheckRB`: clock the card until it releases DAT0 (end of busy).
unsafe fn sd_check_rb() -> Result<(), SdError> {
    for _ in 0..BUSY_POLLS {
        reg_write32(REG_SDCR, reg_read32(REG_SDCR) | SDCR_8CLK_OE);
        wait_sdcr(SDCR_8CLK_OE, CMD_POLLS)?;
        if reg_read32(REG_SDISR) & SDISR_SD_DATA0 != 0 {
            return Ok(());
        }
    }
    Err(SdError::Timeout)
}

/// Deselect the card and give it the 8 clocks the spec asks for after the
/// last transaction (end of `fmiSD_Read_in` / `fmiSelectCard`).
unsafe fn sd_deselect() {
    let _ = sd_command(7, 0);
    reg_write32(REG_SDCR, reg_read32(REG_SDCR) | SDCR_8CLK_OE);
}

/// Mux the port's pins to SD and select the port in SDCR.
unsafe fn select_port(port: SdPort) {
    let sdcr = reg_read32(REG_SDCR) & !SDCR_SDPORT;
    match port {
        SdPort::Port0 => {
            reg_write32(REG_GPEFUN, (reg_read32(REG_GPEFUN) & !0x0000_FFF0) | 0x0000_AAA0);
            reg_write32(REG_SDCR, sdcr | SDCR_SDPORT_0);
        }
        SdPort::Port1 => {
            reg_write32(REG_GPBFUN, (reg_read32(REG_GPBFUN) & !0x0000_0FFF) | 0x0000_0AAA);
            reg_write32(REG_SDCR, sdcr | SDCR_SDPORT_1);
        }
        SdPort::Port2 => {
            reg_write32(REG_GPEFUN, (reg_read32(REG_GPEFUN) & !0x000F_0000) | 0x0005_0000);
            reg_write32(REG_GPDFUN, (reg_read32(REG_GPDFUN) & !0x0003_FC00) | 0x0001_5400);
            reg_write32(REG_SDCR, sdcr | SDCR_SDPORT_2);
        }
    }
}

/// Make the SD engine the FMI's active function (the NAND driver may have
/// selected SM) and wait for the DMA channel.
unsafe fn claim_fmi() -> Result<(), SdError> {
    reg_write32(REG_FMICR, FMI_SD_EN);
    wait(CMD_POLLS, || reg_read32(REG_DMACCSR) & FMI_BUSY == 0)?;
    reg_write32(REG_DMACCSR, reg_read32(REG_DMACCSR) | DMAC_EN);
    Ok(())
}

// ============================================================================
// Card Initialisation
// ============================================================================

/// `fmiSD_Init`: reset and identify the card at the identification clock.
/// Returns the card type and RCA.
unsafe fn sd_identify() -> Result<(SdCardType, u32), SdError> {
    set_clock(INIT_KHZ);

    // Power-on: 74 clocks, then reset all cards.
    reg_write32(REG_SDCR, reg_read32(REG_SDCR) | SDCR_74CLK_OE);
    wait_sdcr(SDCR_74CLK_OE, CMD_POLLS)?;
    sd_command(0, 0)?;
    for _ in 0..0x100 {
        core::hint::spin_loop();
    }

    // CMD8 (2.7-3.6 V, check pattern 0x55): only v2.0 cards answer.
    let v2 = sd_cmd_rsp(8, 0x0000_0155, true).is_ok() && reg_read32(REG_SDRSP1) & 0xFF == 0x55;
    let hcs = if v2 { 0x4000_0000 } else { 0 };

    // ACMD41 until the card is ready. A card that ignores CMD55 is MMC (or
    // there is none).
    let mut tries = 0;
    loop {
        sd_cmd_rsp(55, 0, true).map_err(|e| match e {
            SdError::Timeout => SdError::Unsupported,
            e => e,
        })?;
        sd_cmd_rsp(41, hcs | 0x00FF_8000, false)?; // 2.7-3.6 V
        if reg_read32(REG_SDRSP0) & 0x0080_0000 != 0 {
            break;
        }
        tries += 1;
        if tries >= READY_TRIES {
            return Err(SdError::Timeout);
        }
    }
    let card_type = if v2 && reg_read32(REG_SDRSP0) & 0x0040_0000 != 0 {
        SdCardType::SdHigh
    } else {
        SdCardType::SdLow
    };

    sd_cmd_rsp2(2, 0)?; // ALL_SEND_CID: moves the card to identification
    sd_cmd_rsp(3, 0, true)?; // SEND_RELATIVE_ADDR
    let rca = (reg_read32(REG_SDRSP0) << 8) & 0xFFFF_0000;
    Ok((card_type, rca))
}

/// `fmiGet_SD_info`: capacity from the CSD.
unsafe fn sd_get_info(rca: u32) -> Result<u32, SdError> {
    let csd = sd_cmd_rsp2(9, rca)?;
    let sectors = if csd[0] & 0xC000_0000 != 0 {
        // CSD v2.0: (C_SIZE + 1) * 512 KB
        let c_size = ((csd[1] & 0x0000_003F) << 16) | ((csd[2] & 0xFFFF_0000) >> 16);
        (c_size + 1) * 1024
    } else {
        // CSD v1.0: (C_SIZE + 1) * 2^(C_SIZE_MULT + 2) * 2^READ_BL_LEN bytes
        let r_len = (csd[1] & 0x000F_0000) >> 16;
        let c_size = ((csd[1] & 0x0000_03FF) << 2) | ((csd[2] & 0xC000_0000) >> 30);
        let mult = (csd[2] & 0x0003_8000) >> 15;
        let bytes = (c_size as u64 + 1) << (mult + 2 + r_len);
        (bytes / SD_SECTOR_SIZE as u64) as u32
    };
    Ok(sectors)
}

/// `fmiSelectCard`: 4-bit bus and 512-byte blocks, then deselect.
unsafe fn sd_select_card(rca: u32) -> Result<(), SdError> {
    sd_cmd_rsp(7, rca, true)?;
    sd_check_rb()?;
    sd_cmd_rsp(55, rca, true)?;
    sd_cmd_rsp(6, 0x02, true)?; // SET_BUS_WIDTH: 4 bits
    reg_write32(REG_SDCR, reg_read32(REG_SDCR) | SDCR_DBW);
    sd_cmd_rsp(16, SD_SECTOR_SIZE, true)?; // SET_BLOCKLEN (fixed on SDHC)
    sd_deselect();
    reg_write32(REG_SDISR, 0xFFFF);
    Ok(())
}

/// `fmiInitDevice` + `fmiInitSDDevice`: turn on the SD host on `port` and
/// bring up the card in it.
///
/// Enables the SIC / SD engine clocks and the DMA channel, and takes over
/// the port's pins.
pub unsafe fn sd_init_device(port: SdPort) -> Result<SdInfo, SdError> {
    reg_write32(REG_AHBCLK, reg_read32(REG_AHBCLK) | SIC_CKE | SD_CKE);
    reg_write32(REG_DMACCSR, reg_read32(REG_DMACCSR) | DMAC_EN);
    reg_write32(REG_DMACISR, TABORT_IF);

    reg_write32(REG_FMICR, FMI_SD_EN);
    reg_write32(REG_SDCR, reg_read32(REG_SDCR) & !0xFF); // SD clock output off
    reg_write32(REG_SDCR, reg_read32(REG_SDCR) & !SDCR_DBW);
    reg_write32(REG_SDCR, reg_read32(REG_SDCR) | SDCR_SWRST);
    wait(CMD_POLLS, || reg_read32(REG_SDCR) & SDCR_SWRST == 0)?;

    select_port(port);

    reg_write32(REG_FMIIER, 0);
    reg_write32(REG_SDIER, 0x0000_0A00); // vendor default; interrupts unused
    reg_write32(REG_SDISR, 0x0000_FFFF);
    reg_write32(REG_SDTMOUT, 0x00FF_FFFF);

    let (card_type, rca) = sd_identify()?;
    set_clock(SD_KHZ);
    let total_sectors = sd_get_info(rca)?;
    sd_select_card(rca)?;

    Ok(SdInfo { card_type, port, rca, total_sectors })
}

// ============================================================================
// Data Transfer
// ============================================================================

/// Card address of `sector` (byte address on SDSC).
fn card_addr(sd: &SdInfo, sector: u32) -> u32 {
    match sd.card_type {
        SdCardType::SdHigh => sector,
        SdCardType::SdLow => sector * SD_SECTOR_SIZE,
    }
}

/// Common start of a transfer: claim the FMI, select the card, block length
/// and address, DMA address.
unsafe fn begin_transfer(sd: &SdInfo, sector: u32, buf: u32, count: u32) -> Result<(), SdError> {
    if count == 0 || buf as usize % BUF_ALIGN != 0 {
        return Err(SdError::BadBuffer);
    }
    claim_fmi()?;
    sd_cmd_rsp(7, sd.rca, true)?;
    sd_check_rb()?;
    reg_write32(REG_SDBLEN, SD_SECTOR_SIZE - 1);
    reg_write32(REG_SDARG, card_addr(sd, sector));
    reg_write32(REG_DMACSAR, phys(buf));
    Ok(())
}

/// Common end of a transfer: STOP_TRANSMISSION, wait out busy, deselect.
unsafe fn end_transfer() -> Result<(), SdError> {
    sd_cmd_rsp(12, 0, true)?;
    sd_check_rb()?;
    sd_deselect();
    Ok(())
}

/// `fmiSD_Read`: read `count` sectors from `sector` into `buf`
/// (`count * 512` bytes, [`BUF_ALIGN`]-aligned, in SDRAM).
///
/// # Safety
/// `buf` must be valid for `count * 512` bytes and not touched until return.
pub unsafe fn sd_read(sd: &SdInfo, sector: u32, count: u32, buf: *mut u8) -> Result<(), SdError> {
    // No dirty line may be written back over the DMA data later, and the
    // buffer's own lines must go.
    cache::dcache_clean();
    cache::dcache_invalidate_range(phys(buf as u32), count * SD_SECTOR_SIZE);
    begin_transfer(sd, sector, buf as u32, count)?;

    // READ_MULTIPLE_BLOCK in chunks of up to 255 blocks.
    let mut left = count;
    let mut first = true;
    while left > 0 {
        let blocks = left.min(255);
        let base = (reg_read32(REG_SDCR) & SDCR_KEEP_DATA) | (blocks << 16);
        if first {
            reg_write32(REG_SDCR, base | (18 << 8) | SDCR_CO_EN | SDCR_RI_EN | SDCR_DI_EN);
            first = false;
        } else {
            reg_write32(REG_SDCR, base | SDCR_DI_EN);
        }
        let done = wait(DATA_POLLS, || {
            reg_read32(REG_SDISR) & SDISR_BLKD_IF != 0 && reg_read32(REG_SDCR) & SDCR_DI_EN == 0
        });
        if done.is_err() {
            reset_engine();
            return Err(SdError::Timeout);
        }
        reg_write32(REG_SDISR, SDISR_BLKD_IF);

        let isr = reg_read32(REG_SDISR);
        if isr & SDISR_CRC_7 == 0 || isr & SDISR_CRC_16 == 0 {
            reg_write32(REG_SDISR, SDISR_CRC_IF);
            let _ = end_transfer();
            return Err(SdError::Crc);
        }
        left -= blocks;
    }
    end_transfer()
}

/// `fmiSD_Write`: write `count` sectors from `buf` to `sector`
/// (`count * 512` bytes, [`BUF_ALIGN`]-aligned, in SDRAM).
///
/// # Safety
/// `buf` must be valid for `count * 512` bytes.
pub unsafe fn sd_write(sd: &SdInfo, sector: u32, count: u32, buf: *const u8) -> Result<(), SdError> {
    cache::dcache_clean();
    begin_transfer(sd, sector, buf as u32, count)?;

    // WRITE_MULTIPLE_BLOCK in chunks of up to 255 blocks.
    let mut left = count;
    let mut first = true;
    while left > 0 {
        let blocks = left.min(255);
        let base = (reg_read32(REG_SDCR) & SDCR_KEEP_DATA) | (blocks << 16);
        if first {
            reg_write32(REG_SDCR, base | (25 << 8) | SDCR_CO_EN | SDCR_RI_EN | SDCR_DO_EN);
            first = false;
        } else {
            reg_write32(REG_SDCR, base | SDCR_DO_EN);
        }
        if wait(DATA_POLLS, || reg_read32(REG_SDISR) & SDISR_BLKD_IF != 0).is_err() {
            reset_engine();
            return Err(SdError::Timeout);
        }
        reg_write32(REG_SDISR, SDISR_BLKD_IF);

        if reg_read32(REG_SDISR) & SDISR_CRC_IF != 0 {
            reg_write32(REG_SDISR, SDISR_CRC_IF);
            let _ = end_transfer();
            return Err(SdError::Crc);
        }
        left -= blocks;
    }
    reg_write32(REG_SDISR, SDISR_CRC_IF);
    end_transfer()
}
