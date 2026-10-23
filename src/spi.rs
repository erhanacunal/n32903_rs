//! SPI master for N32903 (W55FA93) — SPI0 / SPI1
//! Ported from the N3290x Non-OS BSP SPI driver (`SPI/Src/spi.c`:
//! `spiOpen`, `spiClose`, `spiSetClock`, `spiTxLen`, `spiActive`,
//! `spiSSEnable`, `spiSSDisable`, `spiRead`, `spiWrite`, `spiIsBusy`).
//!
//! Pins: SPI0 on GPD12 (CLK), GPD13 (SS0), GPD14 (DI), GPD15 (DO);
//! SPI1 on GPB9–GPB12.
//!
//! Differences from the C driver:
//! - Every wait is bounded, so a stuck controller returns
//!   [`SpiError::Timeout`] instead of hanging.
//! - Polled only: the interrupt callbacks, `spiTransfer` (multi-word bursts)
//!   and the FreeRTOS chip-select semaphores are not ported.
//! - [`spi_set_clock`] takes the APB clock in kHz ([`crate::clock`] reports
//!   kHz) instead of MHz.
//!
//! # Safety
//!
//! All functions in this module are `unsafe` because they program the
//! hardware directly. Don't run them concurrently on the same port.

use crate::registers::*;

// ============================================================================
// Public Types
// ============================================================================

/// SPI controller.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SpiPort {
    Spi0,
    Spi1,
}

impl SpiPort {
    #[inline(always)]
    fn base(self) -> u32 {
        match self {
            SpiPort::Spi0 => 0,
            SpiPort::Spi1 => SPI_PORT_STRIDE,
        }
    }
    #[inline(always)]
    fn cntrl(self) -> u32 {
        REG_SPI0_CNTRL + self.base()
    }
    #[inline(always)]
    fn divider(self) -> u32 {
        REG_SPI0_DIVIDER + self.base()
    }
    #[inline(always)]
    fn ssr(self) -> u32 {
        REG_SPI0_SSR + self.base()
    }
    /// TX0 / RX0 (same address).
    #[inline(always)]
    fn data0(self) -> u32 {
        REG_SPI0_TX0 + self.base()
    }
    #[inline(always)]
    fn index(self) -> usize {
        self as usize
    }
}

/// Port configuration (`SPI_INFO_T`).
#[derive(Clone, Copy, Debug)]
pub struct SpiInfo {
    pub port: SpiPort,
    pub slave_mode: bool,
    pub clock_idle_high: bool,
    pub lsb_first: bool,
    pub auto_select: bool,
    pub active_low: bool,
    /// Transmit on the falling edge, receive on the rising edge (else the
    /// other way round).
    pub tx_negative: bool,
    pub level_trigger: bool,
}

/// SPI error
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SpiError {
    /// The port is already open (`spiOpen`: "pin is unavailable").
    InUse,
    /// The controller stayed busy.
    Timeout,
    /// Transfer count / bit length out of range (`spiTxLen`).
    BadLength,
}

// ============================================================================
// Configuration
// ============================================================================

/// GO_BUSY polls per transfer (vendor `spiClose` timeout). One 32-bit word
/// at a few MHz takes far fewer.
const BUSY_POLLS: u32 = 0x0001_0000;

static mut OPEN: [bool; 2] = [false; 2];

#[inline(always)]
unsafe fn set_bits(reg: u32, bits: u32, on: bool) {
    let v = reg_read32(reg);
    reg_write32(reg, if on { v | bits } else { v & !bits });
}

// ============================================================================
// Port Control
// ============================================================================

/// `spiOpen`: enable the clock, reset the controller, mux the pins and apply
/// `info`. Fails if the port is already open.
pub unsafe fn spi_open(info: &SpiInfo) -> Result<(), SpiError> {
    let port = info.port;
    if OPEN[port.index()] {
        return Err(SpiError::InUse);
    }

    let (cke, rst) = match port {
        SpiPort::Spi0 => (SPIMS0_CKE, SPI0RST),
        SpiPort::Spi1 => (SPIMS1_CKE, SPI1RST),
    };
    reg_write32(REG_APBCLK, reg_read32(REG_APBCLK) | cke);
    reg_write32(REG_APBIPRST, reg_read32(REG_APBIPRST) | rst);
    reg_write32(REG_APBIPRST, reg_read32(REG_APBIPRST) & !rst);
    spin(200);

    match port {
        // GPD12–GPD15: function 3 (SPI0)
        SpiPort::Spi0 => reg_write32(REG_GPDFUN, reg_read32(REG_GPDFUN) | 0xFF00_0000),
        // GPB9–GPB12: function 1 (SPI1)
        SpiPort::Spi1 => reg_write32(
            REG_GPBFUN,
            (reg_read32(REG_GPBFUN) & !0x03FC_0000) | 0x0154_0000,
        ),
    }

    let cntrl = port.cntrl();
    let ssr = port.ssr();
    set_bits(cntrl, SPI_SLAVE, info.slave_mode);
    set_bits(cntrl, SPI_CLKP, info.clock_idle_high);
    set_bits(cntrl, SPI_LSB, info.lsb_first);
    set_bits(ssr, SPI_ASS, info.auto_select);
    set_bits(ssr, SPI_SS_LVL, !info.active_low);
    let edge = if info.tx_negative { SPI_TX_NEG } else { SPI_RX_NEG };
    reg_write32(cntrl, (reg_read32(cntrl) & !(SPI_TX_NEG | SPI_RX_NEG)) | edge);
    set_bits(ssr, SPI_SS_LTRIG, info.level_trigger);

    OPEN[port.index()] = true;
    Ok(())
}

/// `spiClose`: wait for the controller, reset it and stop its clock.
pub unsafe fn spi_close(port: SpiPort) -> Result<(), SpiError> {
    wait_idle(port)?;
    OPEN[port.index()] = false;
    let (cke, rst) = match port {
        SpiPort::Spi0 => (SPIMS0_CKE, SPI0RST),
        SpiPort::Spi1 => (SPIMS1_CKE, SPI1RST),
    };
    reg_write32(REG_APBIPRST, reg_read32(REG_APBIPRST) | rst);
    reg_write32(REG_APBIPRST, reg_read32(REG_APBIPRST) & !rst);
    reg_write32(REG_APBCLK, reg_read32(REG_APBCLK) & !cke);
    Ok(())
}

/// `spiSetClock`: SCLK = APB / (2 × (divider + 1)).
pub unsafe fn spi_set_clock(port: SpiPort, apb_khz: u32, out_khz: u32) {
    let divider = (apb_khz / (2 * out_khz.max(1))).saturating_sub(1);
    reg_write32(port.divider(), divider);
}

/// `spiTxLen`: `count` + 1 words of `bit_len` bits (1..=32) per transfer.
pub unsafe fn spi_tx_len(port: SpiPort, count: u32, bit_len: u32) -> Result<(), SpiError> {
    if count > 3 || bit_len == 0 || bit_len > 32 {
        return Err(SpiError::BadLength);
    }
    let len = if bit_len == 32 { 0 } else { bit_len << 3 };
    let reg = port.cntrl();
    let v = reg_read32(reg) & !(SPI_TX_BIT_LEN | SPI_TX_NUM);
    reg_write32(reg, v | len | (count << 8));
    Ok(())
}

/// `spiIsBusy`
pub unsafe fn spi_is_busy(port: SpiPort) -> bool {
    reg_read32(port.cntrl()) & SPI_GO_BUSY != 0
}

unsafe fn wait_idle(port: SpiPort) -> Result<(), SpiError> {
    for _ in 0..BUSY_POLLS {
        if !spi_is_busy(port) {
            return Ok(());
        }
    }
    Err(SpiError::Timeout)
}

/// `spiActive`: start the transfer set up in TX0..TX3 and wait for it.
pub unsafe fn spi_active(port: SpiPort) -> Result<(), SpiError> {
    let reg = port.cntrl();
    reg_write32(reg, reg_read32(reg) | SPI_GO_BUSY);
    wait_idle(port)
}

/// `spiSSEnable`: assert slave select `ss` (0 or 1) after setting SPI clock
/// `mode` 0–3 (CPOL/CPHA).
pub unsafe fn spi_ss_enable(port: SpiPort, ss: u32, mode: u32) {
    let reg = port.cntrl();
    let v = reg_read32(reg) & !(SPI_CLKP | SPI_TX_NEG | SPI_RX_NEG);
    let bits = match mode {
        0 => SPI_TX_NEG,
        1 => SPI_RX_NEG,
        2 => SPI_CLKP | SPI_RX_NEG,
        _ => SPI_CLKP | SPI_TX_NEG,
    };
    reg_write32(reg, v | bits);
    reg_write32(port.ssr(), reg_read32(port.ssr()) | (1 << ss));
}

/// `spiSSDisable`: release slave select `ss`.
pub unsafe fn spi_ss_disable(port: SpiPort, ss: u32) {
    reg_write32(port.ssr(), reg_read32(port.ssr()) & !(1 << ss));
}

// ============================================================================
// Data Transfer
// ============================================================================

/// One transfer of `bit_len` bits (8, 16, 24 or 32): send the low `bit_len`
/// bits of `tx`, MSB first, and return what came back.
pub unsafe fn spi_xfer(port: SpiPort, bit_len: u32, tx: u32) -> Result<u32, SpiError> {
    reg_write32(port.data0(), tx);
    spi_tx_len(port, 0, bit_len)?;
    spi_active(port)?;
    Ok(reg_read32(port.data0()))
}

/// `spiRead` (8-bit words): clock out 0xFF and store each received byte.
pub unsafe fn spi_read(port: SpiPort, dst: &mut [u8]) -> Result<(), SpiError> {
    for b in dst {
        *b = spi_xfer(port, 8, 0xFF)? as u8;
    }
    Ok(())
}

/// `spiWrite` (8-bit words).
pub unsafe fn spi_write(port: SpiPort, src: &[u8]) -> Result<(), SpiError> {
    for &b in src {
        spi_xfer(port, 8, b as u32)?;
    }
    Ok(())
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
