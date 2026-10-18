//! CPU-level helpers for the ARM926EJ-S core (CPSR interrupt masking, WFI).
//!
//! Used by the [`thread`](crate::thread) scheduler and [`ipc`](crate::ipc)
//! primitives for their critical sections.

use core::arch::asm;

pub const NOINT: u32 = 0xC0;
pub const I_BIT: u32 = 0x80;
pub const F_BIT: u32 = 0x40;

/// Read the CPSR register.
#[inline]
pub fn read_cpsr() -> u32 {
    let val: u32;
    unsafe {
        asm!("mrs {0}, cpsr", out(reg) val);
    }
    val
}

/// Disable IRQ and FIQ; returns the previous CPSR for [`interrupt_enable`].
#[inline]
pub fn interrupt_disable() -> u32 {
    let cpsr: u32;
    unsafe {
        asm!(
            "mrs {0}, cpsr",
            "orr {1}, {0}, #{no_int}",
            "msr cpsr_c, {1}",
            out(reg) cpsr,
            out(reg) _,
            no_int = const NOINT,
        );
    }
    cpsr
}

/// Restore the interrupt state saved by [`interrupt_disable`].
#[inline]
pub fn interrupt_enable(cpsr: u32) {
    unsafe {
        asm!("msr cpsr_c, {0}", in(reg) cpsr);
    }
}

/// True if IRQs are currently unmasked.
#[inline]
pub fn interrupts_enabled() -> bool {
    read_cpsr() & I_BIT == 0
}

/// Wait for interrupt (CP15 c7, c0, 4).
#[inline]
pub fn wfi() {
    unsafe {
        asm!("mcr p15, 0, {0}, c7, c0, 4", in(reg) 0u32);
    }
}
