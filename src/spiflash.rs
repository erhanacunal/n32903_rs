//! SPI NOR flash on SPI0 / SS0 (W55FA93 boards: e.g. BY25Q40, W25Qxx)
//! Ported from the N3290x Non-OS BSP SPI flash driver (`SPI/Src/spiflash.c`:
//! `spiFlashInit`, `usiReadID`, `usiCheckBusy`, `usiWriteEnable`,
//! `usiWriteDisable`, `usiStatusWrite`, `spiFlashReset`, `spiFlashEraseBlock`,
//! `spiFlashEraseAll`, `spiFlashWrite`, `spiFlashRead`).
//!
//! Pins: GPD12 (CLK), GPD13 (CS = SS0), GPD14 (DI = flash SO),
//! GPD15 (DO = flash SI). SCLK 10 MHz, SPI mode 0.
//!
//! Differences from the C driver:
//! - Every wait is bounded: [`SpiFlashError::Timeout`] instead of a hang
//!   when the chip is missing or stuck busy.
//! - [`spi_flash_erase_sector`] (4 KB, command 0x20) is an addition. The
//!   vendor's `spiFlashEraseSector` erases 64 KB blocks (0xD8), the same as
//!   `spiFlashEraseBlock`, which is ported as [`spi_flash_erase_block`].
//! - [`spi_flash_write`] splits at 256-byte page boundaries itself, so the
//!   start address doesn't have to be page aligned (the C code assumes it is).
//! - Byte slices instead of `u32` buffers; 4-byte address mode is not ported
//!   (only needed above 16 MB).
//!
//! # Safety
//!
//! All functions in this module are `unsafe` because they program the
//! hardware directly. Don't run them concurrently with other SPI0 users.

use crate::clock;
use crate::registers::*;
use crate::spi::{self, SpiPort};

// ============================================================================
// Public Types
// ============================================================================

/// Page program size.
pub const PAGE_SIZE: u32 = 256;
/// Smallest erase unit ([`spi_flash_erase_sector`]).
pub const SECTOR_SIZE: u32 = 4 * 1024;
/// [`spi_flash_erase_block`] unit.
pub const BLOCK_SIZE: u32 = 64 * 1024;

/// SPI flash error
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SpiFlashError {
    /// Read ID returned 0x0000 / 0xFFFF: no chip answering.
    NoDevice,
    /// The chip stayed busy, or the SPI controller did.
    Timeout,
    /// Address not aligned to the erase unit.
    Unaligned,
}

impl From<spi::SpiError> for SpiFlashError {
    fn from(_: spi::SpiError) -> Self {
        SpiFlashError::Timeout
    }
}

// ============================================================================
// Configuration
// ============================================================================

const PORT: SpiPort = SpiPort::Spi0;
const SCLK_KHZ: u32 = 10_000;

const CMD_WRITE_STATUS: u8 = 0x01;
const CMD_PAGE_PROGRAM: u8 = 0x02;
const CMD_READ: u8 = 0x03;
const CMD_WRITE_DISABLE: u8 = 0x04;
const CMD_READ_STATUS: u8 = 0x05;
const CMD_WRITE_ENABLE: u8 = 0x06;
const CMD_SECTOR_ERASE: u8 = 0x20;
const CMD_RESET_ENABLE: u8 = 0x66;
const CMD_READ_ID: u8 = 0x90;
const CMD_RESET: u8 = 0x99;
const CMD_CHIP_ERASE: u8 = 0xC7;
const CMD_BLOCK_ERASE: u8 = 0xD8;

const STATUS_BUSY: u8 = 0x01;

/// Status polls while the chip is busy. One poll is an 8-bit transfer
/// (~1-2 µs at 10 MHz), so this allows several seconds: enough for a 64 KB
/// block erase (≤ 2 s on typical parts). A chip erase can take longer.
const BUSY_POLLS: u32 = 5_000_000;

// ============================================================================
// Low Level (usi*)
// ============================================================================

#[inline(always)]
unsafe fn cs_low() {
    reg_write32(REG_SPI0_SSR, reg_read32(REG_SPI0_SSR) | 0x01);
}

#[inline(always)]
unsafe fn cs_high() {
    reg_write32(REG_SPI0_SSR, reg_read32(REG_SPI0_SSR) & !0x01);
}

/// One command byte in its own chip-select frame.
unsafe fn command(cmd: u8) -> Result<(), SpiFlashError> {
    cs_low();
    let r = spi::spi_xfer(PORT, 8, cmd as u32);
    cs_high();
    r.map(|_| ()).map_err(Into::into)
}

/// Command byte + 24-bit address, leaving chip select asserted.
unsafe fn command_addr(cmd: u8, addr: u32) -> Result<(), SpiFlashError> {
    cs_low();
    let r = spi::spi_xfer(PORT, 8, cmd as u32).and_then(|_| spi::spi_xfer(PORT, 24, addr));
    if r.is_err() {
        cs_high();
    }
    r.map(|_| ()).map_err(Into::into)
}

/// `usiCheckBusy`: wait until the status register's BUSY bit clears.
pub unsafe fn spi_flash_wait_ready() -> Result<(), SpiFlashError> {
    cs_low();
    let r = (|| {
        spi::spi_xfer(PORT, 8, CMD_READ_STATUS as u32)?;
        for _ in 0..BUSY_POLLS {
            if spi::spi_xfer(PORT, 8, 0xFF)? as u8 & STATUS_BUSY == 0 {
                return Ok(());
            }
        }
        Err(SpiFlashError::Timeout)
    })();
    cs_high();
    r
}

/// `usiReadID`: manufacturer (high byte) and device ID (command 0x90).
pub unsafe fn spi_flash_read_id() -> Result<u16, SpiFlashError> {
    command_addr(CMD_READ_ID, 0)?;
    let r = spi::spi_xfer(PORT, 16, 0xFFFF);
    cs_high();
    Ok(r? as u16)
}

/// `usiWriteEnable`
pub unsafe fn spi_flash_write_enable() -> Result<(), SpiFlashError> {
    command(CMD_WRITE_ENABLE)
}

/// `usiWriteDisable`
pub unsafe fn spi_flash_write_disable() -> Result<(), SpiFlashError> {
    command(CMD_WRITE_DISABLE)
}

/// `usiStatusWrite`: write status register 1 (0x00 clears block protect).
pub unsafe fn spi_flash_status_write(data: u8) -> Result<(), SpiFlashError> {
    spi_flash_write_enable()?;
    cs_low();
    let r = spi::spi_xfer(PORT, 8, CMD_WRITE_STATUS as u32)
        .and_then(|_| spi::spi_xfer(PORT, 8, data as u32));
    cs_high();
    r?;
    spi_flash_wait_ready()
}

// ============================================================================
// Flash API (spiFlash*)
// ============================================================================

/// `spiFlashInit`: start SPI0 at 10 MHz, read the ID, clear block protect.
/// Returns the ID (manufacturer << 8 | device).
pub unsafe fn spi_flash_init() -> Result<u16, SpiFlashError> {
    reg_write32(REG_APBCLK, reg_read32(REG_APBCLK) | SPIMS0_CKE);
    reg_write32(REG_APBIPRST, reg_read32(REG_APBIPRST) | SPI0RST);
    reg_write32(REG_APBIPRST, reg_read32(REG_APBIPRST) & !SPI0RST);
    spin(500);

    spi::spi_set_clock(PORT, clock::get_apb_clock(), SCLK_KHZ);

    // GPD12–GPD15: function 3 (SPI0), chip select = SS0
    reg_write32(REG_GPDFUN, reg_read32(REG_GPDFUN) | 0xFF00_0000);

    reg_write32(REG_SPI0_SSR, 0x00); // CS active low
    reg_write32(REG_SPI0_CNTRL, SPI_TX_NEG); // Tx: falling edge, Rx: rising edge

    let id = spi_flash_read_id()?;
    if id == 0x0000 || id == 0xFFFF {
        return Err(SpiFlashError::NoDevice);
    }
    spi_flash_status_write(0x00)?; // clear block protect
    spi_flash_wait_ready()?;
    Ok(id)
}

/// `spiFlashReset`: software reset (0x66, 0x99), then clear block protect.
pub unsafe fn spi_flash_reset() -> Result<(), SpiFlashError> {
    spi_flash_wait_ready()?;
    command(CMD_RESET_ENABLE)?;
    command(CMD_RESET)?;
    spin(100_000); // tRST (vendor: sysDelay(5))
    spi_flash_status_write(0x00)
}

unsafe fn erase(cmd: u8, addr: u32) -> Result<(), SpiFlashError> {
    spi_flash_write_enable()?;
    command_addr(cmd, addr)?;
    cs_high();
    spi_flash_wait_ready()
}

/// Erase `count` 4 KB sectors from `addr` (sector aligned). Addition, see
/// the module notes.
pub unsafe fn spi_flash_erase_sector(addr: u32, count: u32) -> Result<(), SpiFlashError> {
    if addr % SECTOR_SIZE != 0 {
        return Err(SpiFlashError::Unaligned);
    }
    for i in 0..count {
        erase(CMD_SECTOR_ERASE, addr + i * SECTOR_SIZE)?;
    }
    Ok(())
}

/// `spiFlashEraseBlock`: erase `count` 64 KB blocks from `addr` (aligned).
pub unsafe fn spi_flash_erase_block(addr: u32, count: u32) -> Result<(), SpiFlashError> {
    if addr % BLOCK_SIZE != 0 {
        return Err(SpiFlashError::Unaligned);
    }
    for i in 0..count {
        erase(CMD_BLOCK_ERASE, addr + i * BLOCK_SIZE)?;
    }
    Ok(())
}

/// `spiFlashEraseAll`: chip erase. Can outlast the busy bound on large parts.
pub unsafe fn spi_flash_erase_all() -> Result<(), SpiFlashError> {
    spi_flash_write_enable()?;
    command(CMD_CHIP_ERASE)?;
    spi_flash_wait_ready()
}

/// Big-endian word from up to 4 bytes (missing bytes 0xFF), for 32-bit
/// transfers (the C code byte-swaps a `u32` buffer the same way).
#[inline(always)]
fn word(b: &[u8]) -> u32 {
    let mut w = [0xFF; 4];
    w[..b.len()].copy_from_slice(b);
    u32::from_be_bytes(w)
}

/// `spiFlashWrite`: page program `data` at `addr`. The target must be erased
/// (programming only clears bits).
pub unsafe fn spi_flash_write(mut addr: u32, mut data: &[u8]) -> Result<(), SpiFlashError> {
    while !data.is_empty() {
        let room = (PAGE_SIZE - addr % PAGE_SIZE) as usize;
        let (page, rest) = data.split_at(room.min(data.len()));
        spi_flash_write_enable()?;
        command_addr(CMD_PAGE_PROGRAM, addr)?;
        let r = (|| {
            let mut words = page.chunks_exact(4);
            for w in &mut words {
                spi::spi_xfer(PORT, 32, word(w))?;
            }
            for &b in words.remainder() {
                spi::spi_xfer(PORT, 8, b as u32)?;
            }
            Ok::<(), spi::SpiError>(())
        })();
        cs_high();
        r?;
        spi_flash_wait_ready()?;
        addr += page.len() as u32;
        data = rest;
    }
    Ok(())
}

/// `spiFlashRead`: read `buf.len()` bytes at `addr`.
pub unsafe fn spi_flash_read(addr: u32, buf: &mut [u8]) -> Result<(), SpiFlashError> {
    command_addr(CMD_READ, addr)?;
    let r = (|| {
        let mut words = buf.chunks_exact_mut(4);
        for w in &mut words {
            w.copy_from_slice(&spi::spi_xfer(PORT, 32, 0xFFFF_FFFF)?.to_be_bytes());
        }
        for b in words.into_remainder() {
            *b = spi::spi_xfer(PORT, 8, 0xFF)? as u8;
        }
        Ok::<(), spi::SpiError>(())
    })();
    cs_high();
    r.map_err(Into::into)
}

fn spin(n: u32) {
    if n == 0 {
        return;
    }
    unsafe {
        core::arch::asm!(
            "1:",
            "subs {0}, {0}, #1",
            "bne 1b",
            inout(reg) n => _,
            options(nomem, nostack),
        );
    }
}
