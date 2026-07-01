# n32903_rs

A bare-metal Rust Board Support Package (BSP) / peripheral HAL for the
**Nuvoton N32903 (W55FA93)** ARM926EJ-S SoC.

This crate provides low-level, `#![no_std]` drivers for the chip's core
peripherals, plus the startup assembly and linker infrastructure needed to
boot a Rust application on the silicon.

## Features

Peripheral modules exposed by the library:

| Module      | Description                                                       |
|-------------|-------------------------------------------------------------------|
| `clock`     | System clock, PLL, and DDR SDRAM initialisation                   |
| `uart`      | UART0 (high-speed) and UART1 (console) with formatted output      |
| `gpio`      | GPIO ports A–E: direction, output, input, pull-up, interrupts     |
| `timer`     | Two 32-bit timers with event scheduler, delay, and watchdog       |
| `aic`       | Advanced Interrupt Controller                                     |
| `cache`     | Instruction/data cache control                                    |
| `mmu`       | Memory Management Unit setup                                      |
| `ebi`       | External Bus Interface                                            |
| `i2c`       | I²C controller                                                    |
| `pwm`       | PWM generation                                                    |
| `power`     | Power management                                                  |
| `spu`       | Sound Processing Unit                                             |
| `vpost`     | Video post-processor / LCD output                                 |
| `sys`       | System control helpers                                            |
| `registers` | Complete MMIO register map and access helpers                     |

## Target

The crate builds for a custom bare-metal ARMv5TE target defined in
[`arm926ej-s.json`](arm926ej-s.json):

- **CPU:** ARM926EJ-S (`armv5te`, soft-float, strict-align)
- **Environment:** `no_std`, `no_main`, `panic = "abort"`
- **Toolchain:** Rust **nightly** (uses `build-std`, `feature(linkage)`)

## Prerequisites

- Rust nightly with the `rust-src` component (pinned via
  [`rust-toolchain.toml`](rust-toolchain.toml))
- `arm-none-eabi-gcc` — used by [`build.rs`](build.rs) to assemble
  [`asm/startup.s`](asm/startup.s) into `startup.o`

## Building

```sh
cargo build            # debug
cargo build --release  # release
```

The target and `build-std` settings are preconfigured in
[`.cargo/config.toml`](.cargo/config.toml), so no extra flags are required.

> Note: this is a `staticlib`/`rlib`. The **application** crate is responsible
> for the final link — supply [`linker.ld`](linker.ld) with `-Tlinker.ld` and
> `-nostartfiles`, and link against the generated `startup.o`.

## Usage

Applications override the weak default `rust_main` with their own entry point.
The startup assembly sets up stacks and exception vectors, then calls
`rust_main`.

```rust
#![no_std]
#![no_main]

use n32903_rs::clock::{init_clock, ClockConfig};
use n32903_rs::uart::{Uart, UartConfig};

#[no_mangle]
pub extern "C" fn rust_main() -> ! {
    // 1. Set up clocks and DDR
    unsafe { init_clock(ClockConfig::DEFAULT).unwrap(); }

    // 2. Initialise UART
    let uart = unsafe { Uart::init(&UartConfig::default()) };

    // 3. Print something
    unsafe { uart.puts("Hello from N32903!\n"); }

    loop {}
}
```

If not overridden, the built-in default `rust_main` initialises the clock
(UPLL 96 MHz, HCLK 48 MHz) and UART1 (115200 8N1), then prints a boot banner.

## Safety

Nearly every public function is `unsafe` because it directly accesses
memory-mapped I/O registers. Callers must ensure:

- The hardware is present (N32903 / W55FA93 silicon)
- No conflicting access from interrupts or other execution contexts
- Correct register addresses (verified against `W55FA93_reg.h`)

## License

_No license specified yet._
