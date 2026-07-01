//! Power management for N32903 (W55FA93)
//! Ported from wb_power.c
//!
//! Provides system suspend (power-down) with configurable wake-up sources.
//! The core power-down sequence runs from SRAM while SDRAM is in self-refresh.
//!
//! # Safety
//!
//! All functions are `unsafe`.  Power-down saves/restores SRAM content and
//! manipulates the interrupt controller, DRAM controller, and PLLs.

use crate::registers::*;

// ============================================================================
// Public Types
// ============================================================================

/// Wake-up source bitmask (matches WAKEUP_SOURCE_E and REG_MISCR wake-up bits)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WakeUpSource {
    Gpio = 0x01,
    Rtc = 0x02,
    Sdh = 0x04,
    Uart = 0x08,
    Udc = 0x10,
    Uhc = 0x20,
    Adc = 0x40,
    Kpi = 0x80,
}

// ============================================================================
// Constants
// ============================================================================

/// SRAM base used for power-down code
const PD_RAM_BASE: u32 = 0xFF00_0000;
const PD_RAM_START: u32 = 0xFF00_1000;
const PD_RAM_SIZE: usize = 0x2000;

/// Scratch variable address in SRAM
// const SRAM_VAR_ADDR: u32 = 0xFF00_1FF0;

// ============================================================================
// Static State
// ============================================================================

/// Backup buffer for SRAM contents during power-down
static mut TMP_BUF: [u8; PD_RAM_SIZE] = [0u8; PD_RAM_SIZE];

// ============================================================================
// Power-Down Sequence (runs from SRAM)
// ============================================================================

/// The core power-down routine — copied to SRAM and executed there.
///
/// Sequence:
/// 1. Put DRAM in self-refresh
/// 2. Switch system clock to external crystal
/// 3. Optionally power-down PLLs (based on GPAFUN[26] flag)
/// 4. Set wake-up pre-scaler
/// 5. Stop external clock (enter power-down)
/// 6. On wake-up: re-enable PLLs, wait for lock, switch back
/// 7. Exit DRAM self-refresh
unsafe fn sample_power_down() {
    // Delay
    let mut delay: u32;
    // DDR self-refresh
    reg_write32(REG_SDOPM, reg_read32(REG_SDOPM) & !OPMODE);
    reg_write32(
        REG_SDCMD,
        (reg_read32(REG_SDCMD) & !(AUTOEXSELFREF | CKE_H)) | SELF_REF,
    );

    // Switch to external crystal
    reg_write32(REG_CLKDIV0, reg_read32(REG_CLKDIV0) & !SYSTEM_S);

    // Check PLL power-down flag (stored in GPAFUN[26] by caller)
    if reg_read32(REG_GPAFUN) & 0x0400_0000 != 0 {
        reg_write32(REG_UPLLCON, reg_read32(REG_UPLLCON) | PD);
        reg_write32(REG_APLLCON, reg_read32(REG_APLLCON) | PD);
    } else {
        reg_write32(REG_UPLLCON, reg_read32(REG_UPLLCON) & !PD);
        reg_write32(REG_APLLCON, reg_read32(REG_APLLCON) & !PD);
    }

    // Set wake-up pre-scaler
    if reg_read32(REG_GPAFUN) & 0x0400_0000 != 0 {
        // PLL off: ~25-75 ms wake time
        reg_write32(REG_PWRCON, (reg_read32(REG_PWRCON) & !0xFFFF00) | 0xFF02);
    } else {
        // PLL on: ~3 ms wake time
        reg_write32(REG_PWRCON, (reg_read32(REG_PWRCON) & !0xFFFF00) | 0x0002);
    }

    // Enter power-down: clear CPU_CKE in PWRCON
    let pwrc = reg_read32(REG_PWRCON);
    reg_write32(REG_PWRCON, pwrc & !0x01);

    // --- CPU stops here; resumes after wake-up event ---

    // Re-enable PLLs
    reg_write32(REG_UPLLCON, reg_read32(REG_UPLLCON) & !PD);
    reg_write32(REG_APLLCON, reg_read32(REG_APLLCON) & !PD);

    // If PLLs were off, wait for lock
    if reg_read32(REG_GPAFUN) & 0x0400_0000 != 0 {
        delay = 500;
        while delay != 0 { delay -= 1; }
    }

    // Switch back to PLL
    reg_write32(REG_CLKDIV0, reg_read32(REG_CLKDIV0) | SYSTEM_S);

    delay = 500;
    while delay != 0 { delay -= 1; }

    // Exit self-refresh
    reg_write32(REG_SDCMD, 0x20); // CKE low, exit self-refresh

    delay = 100;
    while delay != 0 { delay -= 1; }
}

// ============================================================================
// Public API
// ============================================================================

/// Set whether PLLs should be powered down during system suspend.
///
/// Stores the flag in an unused GPAFUN register bit, which the SRAM-resident
/// power-down sequence reads.
pub unsafe fn power_down_pll_during_suspend(power_down: bool) {
    if power_down {
        reg_write32(REG_GPAFUN, reg_read32(REG_GPAFUN) | 0x0400_0000); // BIT26
    } else {
        reg_write32(REG_GPAFUN, reg_read32(REG_GPAFUN) & !0x0400_0000);
    }
}

/// Enter system power-down (suspend) mode.
///
/// Saves IRQ state, backs up SRAM, copies the power-down sequence into SRAM,
/// enables the specified wake-up source, and suspends.  On wake-up, restores
/// SRAM and interrupt state.
pub unsafe fn power_down(wake_up_src: WakeUpSource) -> i32 {
    use crate::aic;

    let b_was_irq_enabled = aic::aic_get_ibit_state();
    if b_was_irq_enabled {
        aic::aic_set_local_interrupt(crate::aic::LocalIntState::DisableIrq);
    }

    let u32_ram_base = PD_RAM_BASE;

    // Save SRAM
    core::ptr::copy_nonoverlapping(
        (u32_ram_base as usize | 0x8000_0000) as *const u8,
        TMP_BUF.as_mut_ptr(),
        PD_RAM_SIZE,
    );

    // Copy power-down routine to SRAM
    let func_bytes_ptr = sample_power_down as *const u8;
    let func_base = (func_bytes_ptr as usize & !0x8000_0000)
        .wrapping_sub((PD_RAM_START - PD_RAM_BASE) as usize);
    core::ptr::copy_nonoverlapping(
        (func_base | 0x8000_0000) as *const u8,
        (u32_ram_base as usize | 0x8000_0000) as *mut u8,
        PD_RAM_SIZE,
    );

    // Flush I-cache so SRAM copy is seen
    crate::cache::cache_flush(crate::cache::CacheType::ICache);

    // Save and mask all interrupts
    let u32_int_enable = reg_read32(REG_AIC_IMR);
    reg_write32(REG_AIC_MDCR, 0xFFFF_FFFE);
    reg_write32(REG_AIC_MECR, 0x0000_0000);

    // Enable wake-up source
    let src = wake_up_src as u32;
    reg_write32(REG_MISCR, (src << 24) | (src << 16));

    // Jump to SRAM power-down routine
    let pd_func: unsafe extern "C" fn() = core::mem::transmute(PD_RAM_START as *const ());
    pd_func();

    // Restore wake-up source settings
    reg_write32(REG_MISCR, (src << 24) | (src << 16));

    // Restore SRAM
    core::ptr::copy_nonoverlapping(
        TMP_BUF.as_ptr(),
        u32_ram_base as *mut u8,
        PD_RAM_SIZE,
    );

    // Restore interrupt mask
    reg_write32(REG_AIC_MECR, u32_int_enable);

    if b_was_irq_enabled {
        aic::aic_set_local_interrupt(crate::aic::LocalIntState::EnableIrq);
    }

    0
}
