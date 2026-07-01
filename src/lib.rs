//! # N32903 Rust BSP
//!
//! Board Support Package for the Nuvoton N32903 (W55FA93) ARM926EJ-S SoC.
//!
//! Provides low-level peripheral drivers for:
//!
//! - **`clock`** — System clock, PLL, and DDR SDRAM initialisation
//! - **`uart`**   — UART0 (high-speed) and UART1 (console) with formatted output
//! - **`gpio`**   — GPIO port A–E: direction, output, input, pull-up, interrupts
//! - **`timer`**  — Two 32-bit timers with event scheduler, delay, and watchdog
//! - **`registers`** — Complete MMIO register map and access helpers
//!
//! # Quick Start
//!
//! ```rust,ignore
//! #![no_std]
//! #![no_main]
//!
//! use n32903_rs::clock::{init_clock, ClockConfig};
//! use n32903_rs::uart::{Uart, UartConfig};
//!
//! #[no_mangle]
//! pub extern "C" fn rust_main() -> ! {
//!     // 1. Set up clocks and DDR
//!     unsafe { init_clock(ClockConfig::DEFAULT).unwrap(); }
//!
//!     // 2. Initialise UART
//!     let uart = unsafe { Uart::init(&UartConfig::default()) };
//!
//!     // 3. Print something
//!     unsafe { uart.puts("Hello from N32903!\n"); }
//!
//!     loop {}
//! }
//! ```
//!
//! # Safety
//!
//! Nearly every public function in this crate is `unsafe` because it
//! directly accesses memory-mapped I/O registers.  Callers must ensure:
//!
//! - The hardware is present (N32903 / W55FA93 silicon)
//! - No conflicting access from interrupts or other cores
//! - Correct register addresses (verified against `W55FA93_reg.h`)

#![no_std]
#![no_main]
#![feature(linkage)]
// Suppress warnings normal in bare-metal embedded Rust:
// - static_mut_refs: reading/writing static mut is the standard pattern
//   for memory-mapped peripheral state in no_std embedded code.
// - non_upper_case_globals: register names match the C header for
//   traceability (e.g. REG_LCM_LCDCCtl, REG_GPIOA_OMD).
#![allow(static_mut_refs, non_upper_case_globals)]

pub mod aic;
pub mod cache;
pub mod clock;
pub mod ebi;
pub mod gpio;
pub mod i2c;
pub mod mmu;
pub mod power;
pub mod pwm;
pub mod registers;
pub mod spu;
pub mod sys;
pub mod timer;
pub mod uart;
pub mod vpost;

// ============================================================================
// Default entry point (weak — may be overridden by the application)
// ============================================================================

/// Default `rust_main` — initialises clock + UART and prints a banner.
///
/// Applications **should** override this with their own `#[no_mangle]`
/// `pub extern "C" fn rust_main()`.
///
/// The startup assembly (`asm/startup.s`) sets up stacks and exception
/// vectors, then calls `rust_main`.  The linker picks the strongest
/// definition — provide your own to replace this default.
#[no_mangle]
#[linkage = "weak"]
pub extern "C" fn rust_main() -> ! {
    // Default: initialise with safe defaults and print banner
    unsafe {
        // Basic clock init (UPLL 96 MHz)
        let _ = clock::init_clock(clock::ClockConfig::DEFAULT);

        // Select UART1 and init at 115200-8N1
        uart::uart_port_select(uart::UartPort::Uart1);
        uart::uart_init_default();

        uart::uart_puts("\n========================================\n");
        uart::uart_puts("  N32903 Rust BSP - Default Boot\n");
        uart::uart_puts("  Clock: UPLL 96 MHz, HCLK 48 MHz\n");
        uart::uart_puts("  UART:  115200 8N1\n");
        uart::uart_puts("========================================\n\n");
    }

    loop {}
}

// ============================================================================
// Panic Handler
// ============================================================================

use core::panic::PanicInfo;

/// Panic handler — prints file/line info via UART if possible, then loops.
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    unsafe {
        // Try to output panic info via UART
        // (UART may or may not be initialised — we try anyway)
        uart::uart_puts("\n*** PANIC ***\n");

        if let Some(loc) = info.location() {
            uart::uart_puts("  at ");
            uart::uart_puts(loc.file());
            uart::uart_puts(":");
            // Print line number as decimal
            let line = loc.line();
            uart::uart_printf(" %d\n", &[uart::UartArg::U32(line)]);
        }

        if let Some(msg) = info.message().as_str() {
            uart::uart_puts("  ");
            uart::uart_puts(msg);
            uart::uart_puts("\n");
        }
    }

    loop {}
}
