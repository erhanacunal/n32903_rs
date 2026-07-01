//! AIC (Advanced Interrupt Controller) driver for N32903 (W55FA93)
//! Ported from wb_aic.c and wblib.h
//!
//! Supports 32 interrupt sources with configurable priority levels (0–7),
//! edge/level trigger types, and IRQ/FIQ routing.
//!
//! # Safety
//!
//! All functions are `unsafe` — they manipulate hardware registers and
//! the ARM exception vector table directly.

use crate::registers::*;

// ============================================================================
// Public Types
// ============================================================================

/// Interrupt source numbers (matches hardware IRQ assignment)
pub type IntSource = u32;

/// Interrupt priority / level
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum IntLevel {
    Fiq = 0,
    Irq1 = 1,
    Irq2 = 2,
    Irq3 = 3,
    Irq4 = 4,
    Irq5 = 5,
    Irq6 = 6,
    Irq7 = 7,
}

/// Interrupt trigger type
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum IntType {
    LowLevel = 0,
    HighLevel = 1,
    FallingEdge = 2,
    RisingEdge = 3,
}

/// Exception type for sysInstallExceptionHandler
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ExceptionType {
    Swi = 0,
    DataAbort = 1,
    PrefetchAbort = 2,
    Undefined = 3,
}

/// IRQ/FIQ enable/disable constants
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum LocalIntState {
    EnableIrq = 0x7F,
    EnableFiq = 0xBF,
    EnableBoth = 0x3F,
    DisableIrq = 0x80,
    DisableFiq = 0x40,
    DisableBoth = 0xC0,
}

/// Interrupt handler function pointer
pub type IntHandler = unsafe extern "C" fn();

// ============================================================================
// Constants
// ============================================================================

const MIN_INT_SOURCE: u32 = 1;
const MAX_INT_SOURCE: u32 = 31;
const NUM_AIC_REGS: u32 = 32;

/// AIC register stride (each SCRn is 4 bytes apart)
const AIC_REG_OFFSET: u32 = 0x4;

// ============================================================================
// Static State
// ============================================================================

static mut AIC_INITIALIZED: bool = false;
static mut IS_HW_MODE: bool = true;

/// Default interrupt shell (no-op handler filled in for unused sources)
extern "C" fn interrupt_shell() {}

/// IRQ handler table (32 entries, indexed by interrupt number)
static mut IRQ_HANDLER_TABLE: [IntHandler; 32] = [interrupt_shell as IntHandler; 32];

/// FIQ handler table
static mut FIQ_HANDLER_TABLE: [IntHandler; 32] = [interrupt_shell as IntHandler; 32];

// ============================================================================
// Initialization
// ============================================================================

/// Initialise the AIC.  Installs IRQ/FIQ handler trampolines in the
/// ARM exception vector table (at 0x18 and 0x1C).
pub unsafe fn aic_initialize() {
    if AIC_INITIALIZED {
        return;
    }

    // Set vector table entries: LDR PC,[PC,#offset] opcodes at 0x18, 0x1C
    core::ptr::write_volatile(0x18 as *mut u32, 0xe59f_f018);
    core::ptr::write_volatile(0x1C as *mut u32, 0xe59f_f018);

    // Install IRQ handler trampoline at 0x38
    core::ptr::write_volatile(0x38 as *mut u32, aic_irq_handler as *const () as u32);

    // Install FIQ handler trampoline at 0x3C
    core::ptr::write_volatile(0x3C as *mut u32, aic_fiq_handler as *const () as u32);

    AIC_INITIALIZED = true;
}

// ============================================================================
// Public API — Interrupt Management
// ============================================================================

/// Enable an interrupt source.
pub unsafe fn aic_enable_interrupt(e_int_no: IntSource) -> bool {
    if e_int_no > MAX_INT_SOURCE || e_int_no < MIN_INT_SOURCE {
        return false;
    }
    reg_write32(REG_AIC_MECR, 1 << e_int_no);
    true
}

/// Disable an interrupt source.
pub unsafe fn aic_disable_interrupt(e_int_no: IntSource) -> bool {
    if e_int_no > MAX_INT_SOURCE || e_int_no < MIN_INT_SOURCE {
        return false;
    }
    reg_write32(REG_AIC_MDCR, 1 << e_int_no);
    true
}

/// Read the current interrupt mask (enabled interrupt bits).
pub unsafe fn aic_get_interrupt_enable_status() -> u32 {
    reg_read32(REG_AIC_IMR)
}

/// Install an interrupt service routine.
///
/// Returns the previous handler for the given source.
pub unsafe fn aic_install_isr(level: IntLevel, e_int_no: IntSource, handler: IntHandler) -> IntHandler {
    if !AIC_INITIALIZED {
        aic_initialize();
    }

    let reg_index = e_int_no / 4;
    let shift = (e_int_no % 4) * 8;

    // Write priority level to SCR
    let scr_addr = REG_AIC_SCR1 + reg_index * AIC_REG_OFFSET;
    let mut reg_val = reg_read32(scr_addr);
    // Clear the 3-bit level field for this source and set new value
    reg_val &= !(0x7 << shift);
    reg_val |= (level as u32) << shift;
    reg_write32(scr_addr, reg_val);

    // Install handler
    let old;
    match level {
        IntLevel::Fiq => {
            old = FIQ_HANDLER_TABLE[e_int_no as usize];
            FIQ_HANDLER_TABLE[e_int_no as usize] = handler;
        }
        _ => {
            old = IRQ_HANDLER_TABLE[e_int_no as usize];
            IRQ_HANDLER_TABLE[e_int_no as usize] = handler;
        }
    }
    old
}

/// Set interrupt priority level for a source.
pub unsafe fn aic_set_priority(e_int_no: IntSource, level: IntLevel) -> bool {
    if e_int_no > MAX_INT_SOURCE || e_int_no < MIN_INT_SOURCE {
        return false;
    }

    let reg_index = e_int_no / 4;
    let shift = (e_int_no % 4) * 8;
    let scr_addr = REG_AIC_SCR1 + reg_index * AIC_REG_OFFSET;

    let mut reg_val = reg_read32(scr_addr);
    reg_val &= !(0x7 << shift);
    reg_val |= (level as u32) << shift;
    reg_write32(scr_addr, reg_val);
    true
}

/// Set interrupt trigger type (level/edge, high/low, falling/rising).
pub unsafe fn aic_set_interrupt_type(e_int_no: IntSource, int_type: IntType) -> bool {
    if e_int_no > MAX_INT_SOURCE || e_int_no < MIN_INT_SOURCE {
        return false;
    }

    let reg_index = e_int_no / 4;
    let shift = (e_int_no % 4) * 8 + 6;
    let scr_addr = REG_AIC_SCR1 + reg_index * AIC_REG_OFFSET;

    let mut reg_val = reg_read32(scr_addr);
    reg_val &= !(0x3 << shift);
    reg_val |= (int_type as u32) << shift;
    reg_write32(scr_addr, reg_val);
    true
}

/// Enable/disable all interrupts globally.
pub unsafe fn aic_set_global_interrupt(enable: bool) {
    if enable {
        reg_write32(REG_AIC_MECR, 0xFFFF_FFFF);
    } else {
        reg_write32(REG_AIC_MDCR, 0xFFFF_FFFF);
    }
}

/// Enable or disable IRQ/FIQ at the CPU (CPSR) level.
///
/// Uses inline ARM assembly to manipulate the CPSR I/F bits.
pub unsafe fn aic_set_local_interrupt(state: LocalIntState) {
    match state {
        LocalIntState::EnableIrq | LocalIntState::EnableFiq | LocalIntState::EnableBoth => {
            core::arch::asm!(
                "mrs r0, CPSR",
                "bic r0, r0, #0x80",
                "msr CPSR_c, r0",
                out("r0") _,
            );
        }
        LocalIntState::DisableIrq | LocalIntState::DisableFiq | LocalIntState::DisableBoth => {
            core::arch::asm!(
                "mrs r0, CPSR",
                "orr r0, r0, #0x80",
                "msr CPSR_c, r0",
                out("r0") _,
            );
        }
    }
}

/// Check whether IRQs are enabled at the CPU level.
pub unsafe fn aic_get_ibit_state() -> bool {
    let cpsr: u32;
    core::arch::asm!(
        "mrs {0}, CPSR",
        out(reg) cpsr,
    );
    cpsr & 0x80 == 0
}

/// Switch AIC to software mode (disable hardware priority encoding).
pub unsafe fn aic_set_sw_mode() {
    IS_HW_MODE = false;
}

// ============================================================================
// Exception Vector Handlers
// ============================================================================

/// Install a handler for a standard ARM exception (SWI, Data Abort,
/// Prefetch Abort, Undefined Instruction).
///
/// Returns the previous handler (raw pointer).
pub unsafe fn aic_install_exception_handler(
    except_type: ExceptionType,
    handler: *const (),
) -> *const () {
    let vec_addr = match except_type {
        ExceptionType::Swi => 0x28,
        ExceptionType::DataAbort => 0x30,
        ExceptionType::PrefetchAbort => 0x2C,
        ExceptionType::Undefined => 0x24,
    };

    let old = core::ptr::read_volatile(vec_addr as *const *const ());
    core::ptr::write_volatile(vec_addr as *mut *const (), handler);
    old
}

/// Install a custom IRQ handler trampoline (replaces the AIC dispatch).
pub unsafe fn aic_install_irq_handler(handler: *const ()) -> *const () {
    let old = core::ptr::read_volatile(0x38 as *const *const ());
    core::ptr::write_volatile(0x38 as *mut *const (), handler);
    old
}

/// Install a custom FIQ handler trampoline.
pub unsafe fn aic_install_fiq_handler(handler: *const ()) -> *const () {
    let old = core::ptr::read_volatile(0x3C as *const *const ());
    core::ptr::write_volatile(0x3C as *mut *const (), handler);
    old
}

// ============================================================================
// IRQ / FIQ Dispatchers (called from exception vectors)
// ============================================================================

/// IRQ handler — dispatched from the ARM IRQ exception vector.
/// Reads the priority-encoded source number and calls the registered handler.
#[no_mangle]
unsafe extern "C" fn aic_irq_handler() {
    if IS_HW_MODE {
        let iper = reg_read32(REG_AIC_IPER) >> 2;
        let isnr = reg_read32(REG_AIC_ISNR);

        if isnr != 0 && iper == isnr {
            IRQ_HANDLER_TABLE[iper as usize]();
        }
        reg_write32(REG_AIC_EOSCR, 1);
    } else {
        let isr = reg_read32(REG_AIC_ISR);
        for i in 1..NUM_AIC_REGS {
            if isr & (1 << i) != 0 {
                IRQ_HANDLER_TABLE[i as usize]();
            }
        }
    }
}

/// FIQ handler — dispatched from the ARM FIQ exception vector.
#[no_mangle]
unsafe extern "C" fn aic_fiq_handler() {
    if IS_HW_MODE {
        let iper = reg_read32(REG_AIC_IPER) >> 2;
        let isnr = reg_read32(REG_AIC_ISNR);

        if isnr != 0 && iper == isnr {
            FIQ_HANDLER_TABLE[iper as usize]();
        }
        reg_write32(REG_AIC_EOSCR, 1);
    } else {
        let isr = reg_read32(REG_AIC_ISR);
        for i in 1..NUM_AIC_REGS {
            if isr & (1 << i) != 0 {
                FIQ_HANDLER_TABLE[i as usize]();
            }
        }
    }
}
