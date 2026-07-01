//! EBI (External Bus Interface) driver for N32903 (W55FA93)
//! Ported from wb_ebi.c
//!
//! Manages up to 3 external chip-select regions (EXT0, EXT1, EXT2)
//! with configurable base address, size, bus width, and access timing.
//!
//! # Safety
//!
//! All functions are `unsafe` — they write to EBI control registers.

use crate::registers::*;

// ============================================================================
// Public Types
// ============================================================================

/// External chip-select index
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ExtIo {
    Ext0 = 0,
    Ext1 = 1,
    Ext2 = 2,
}

/// External bus width
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ExtBusWidth {
    Disable = 12,
    Bits8 = 13,
    Bits16 = 14,
    Bits32 = 15,
}

/// External region size
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum ExtSize {
    Size256K = 4,
    Size512K = 5,
    Size1M = 6,
    Size2M = 7,
    Size4M = 8,
    Size8M = 9,
    Size16M = 10,
    Size32M = 11,
}

// ============================================================================
// Register Lookup
// ============================================================================

fn ext_reg(ext: ExtIo) -> u32 {
    match ext {
        ExtIo::Ext0 => REG_EXT0CON,
        ExtIo::Ext1 => REG_EXT1CON,
        ExtIo::Ext2 => REG_EXT2CON,
    }
}

// ============================================================================
// Public API
// ============================================================================

/// Configure an external chip-select region.
///
/// Sets base address, region size, and data bus width.
///
/// `base_addr` is left-shifted by 1 before writing (the hardware uses
/// bits [19:3] of the register for the base address).
pub unsafe fn ebi_set_external_io(
    ext: ExtIo,
    base_addr: u32,
    size: ExtSize,
    bus_width: ExtBusWidth,
) -> i32 {
    let reg = ext_reg(ext);
    let mut val = reg_read32(reg);

    // Bus width (bits 1:0)
    match bus_width {
        ExtBusWidth::Disable => val &= !0x03,
        ExtBusWidth::Bits8 => val = (val & !0x03) | 0x01,
        ExtBusWidth::Bits16 => val = (val & !0x03) | 0x02,
        ExtBusWidth::Bits32 => val = (val & !0x03) | 0x03,
    }

    // Size (bits 18:16)
    val &= !0x0007_0000;
    val |= ((size as u32 - 4) & 0x7) << 16;

    // Base address (bits 31:19)
    let addr_field = (base_addr << 1) & 0xFFF8_0000;
    val = (val & 0x0007_FFFF) | addr_field;

    reg_write32(reg, val);
    0
}

/// Set access timing for an external region.
///
/// `t_acc` — access time (0–15, written to bits 14:11)
/// `t_acs` — access setup time (0–7, written to bits 7:5)
pub unsafe fn ebi_set_timing1(ext: ExtIo, t_acc: u32, t_acs: u32) -> i32 {
    let reg = ext_reg(ext);
    let mut val = reg_read32(reg);

    if t_acc <= 0xF {
        val = (val & !0x0000_7800) | ((t_acc & 0xF) << 11);
    }

    if t_acs <= 0x7 {
        val = (val & !0x0000_00E0) | ((t_acs & 0x7) << 5);
    }

    reg_write32(reg, val);
    0
}

/// Set output hold and setup timing for an external region.
///
/// `t_coh` — output hold time (0–7, written to bits 10:8)
/// `t_cos` — output setup time (0–7, written to bits 2:0)
pub unsafe fn ebi_set_timing2(ext: ExtIo, t_coh: u32, t_cos: u32) -> i32 {
    let reg = ext_reg(ext);
    let mut val = reg_read32(reg);

    if t_coh <= 0x7 {
        val = (val & !0x0000_0700) | ((t_coh & 0x7) << 8);
    }

    if t_cos <= 0x7 {
        val = (val & !0x0000_0007) | (t_cos & 0x7);
    }

    reg_write32(reg, val);
    0
}
