//! UART driver for N32903 (W55FA93)
//! Ported from wb_uart.c / wb_uart0.c and the N32903 NAND bootloader.
//!
//! Supports both UART0 (high-speed on GPD[2:1]) and UART1 (normal on GPA[11:10]).
//!
//! # Safety
//!
//! All hardware-access functions are `unsafe`.  The module uses static
//! mutable state for the active port — only one port is active at a time
//! in the simple API.  For concurrent use prefer instantiating `Uart` directly.

use crate::registers::*;

// ============================================================================
// Public Types
// ============================================================================

/// UART port selection
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UartPort {
    /// High-speed UART (pins GPD2=RX, GPD1=TX)
    Uart0,
    /// Normal-speed UART (pins GPA10=RX, GPA11=TX) — default console
    Uart1,
}

impl UartPort {
    /// Base offset within the UART register block
    fn offset(self) -> u32 {
        match self {
            UartPort::Uart0 => 0x000,
            UartPort::Uart1 => 0x100,
        }
    }
}

/// Parity selection
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UartParity {
    None = 0,
    Odd = 0x08,  // PBE
    Even = 0x18, // PBE | EPE
}

/// Data bits
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UartDataBits {
    Bits5 = 0x00,
    Bits6 = 0x01,
    Bits7 = 0x02,
    Bits8 = 0x03,
}

/// Stop bits
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum UartStopBits {
    Bits1 = 0x00,
    Bits2 = 0x04, // NSB
}

/// UART configuration
#[derive(Clone, Copy, Debug)]
pub struct UartConfig {
    /// Which UART peripheral to use
    pub port: UartPort,
    /// Baud rate (e.g. 115200, 9600)
    pub baud_rate: u32,
    /// Data bits (5-8)
    pub data_bits: UartDataBits,
    /// Stop bits (1 or 2)
    pub stop_bits: UartStopBits,
    /// Parity
    pub parity: UartParity,
    /// Reference clock in Hz (typically external crystal = 12 MHz)
    pub ref_clock_hz: u32,
    /// RX FIFO trigger level (1, 4, 8, or 14 bytes)
    pub rx_fifo_trigger: u8,
}

impl Default for UartConfig {
    fn default() -> Self {
        Self {
            port: UartPort::Uart1,
            baud_rate: 115_200,
            data_bits: UartDataBits::Bits8,
            stop_bits: UartStopBits::Bits1,
            parity: UartParity::None,
            ref_clock_hz: EXTERNAL_CRYSTAL_CLOCK,
            rx_fifo_trigger: 1,
        }
    }
}

/// Argument type for `uart_printf`
#[derive(Clone, Copy)]
pub enum UartArg<'a> {
    U32(u32),
    Str(&'a str),
}

// ============================================================================
// Uart driver struct
// ============================================================================

/// An initialised UART instance.
pub struct Uart {
    port: UartPort,
    #[allow(dead_code)]
    ref_clock: u32,
}

impl Uart {
    /// Create and initialise the UART hardware.
    ///
    /// # Safety
    ///
    /// Modifies hardware registers and multifunction pin configuration.
    pub unsafe fn init(config: &UartConfig) -> Self {
        let port = config.port;
        let off = port.offset();

        // --- Configure multifunction pins ---
        match port {
            UartPort::Uart0 => {
                // UART0 on GPD2 (RX), GPD1 (TX): set MF = 0x5
                let val = reg_read32(REG_GPDFUN);
                reg_write32(
                    REG_GPDFUN,
                    (val & !(MF_GPD1 | MF_GPD2)) | (0x5 | (0x5 << 2)),
                );
                // Clear UART0 clock divider
                reg_write32(REG_CLKDIV3, reg_read32(REG_CLKDIV3) & !UART0_N);
                // Enable UART0 clock
                reg_write32(REG_APBCLK, reg_read32(REG_APBCLK) | UART0_CKE);
            }
            UartPort::Uart1 => {
                // UART1 on GPA10 (RX), GPA11 (TX)
                reg_write32(REG_GPAFUN, reg_read32(REG_GPAFUN) | MF_GPA10 | MF_GPA11);
                // Clear UART1 clock divider
                reg_write32(REG_CLKDIV3, reg_read32(REG_CLKDIV3) & !UART1_N);
                // Enable UART1 clock
                reg_write32(REG_APBCLK, reg_read32(REG_APBCLK) | UART1_CKE);
            }
        }

        // --- Reset TX/RX FIFOs ---
        reg_write32(REG_UART0_FCR + off, 0x07);

        // --- Setup baud rate (mode 3: divisor = clock / baud - 2) ---
        let baud_div = config.ref_clock_hz / config.baud_rate - 2;
        reg_write32(
            REG_UART0_BAUD + off,
            (0x30 << 24) | (baud_div & BAUD_RATE_DIVISOR),
        );

        // --- Setup line control ---
        let lcr = config.parity as u32 | config.data_bits as u32 | config.stop_bits as u32;
        reg_write32(REG_UART0_LCR + off, lcr);

        // --- Timeout register ---
        reg_write32(REG_UART0_TOR + off, 0x80 + 0x20);

        // --- Setup FIFO trigger level and enable FIFO ---
        let fcr_val = match config.rx_fifo_trigger {
            1 => UART_FIFO_TRIG_1BYTE,
            4 => UART_FIFO_TRIG_4BYTE,
            8 => UART_FIFO_TRIG_8BYTE,
            _ => UART_FIFO_TRIG_14BYTE,
        };
        reg_write32(REG_UART0_FCR + off, (fcr_val << 4) | 0x01);

        Uart {
            port,
            ref_clock: config.ref_clock_hz,
        }
    }

    /// Transmit a single byte (blocking — polls TX-empty).
    pub unsafe fn putchar(&self, c: u8) {
        let off = self.port.offset();
        // Wait until TX FIFO is not full
        while reg_read32(REG_UART0_FSR + off) & TX_EMPTY == 0 {}
        reg_write8(REG_UART0_THR + off, c);

        // Auto \n -> \r\n expansion
        if c == b'\n' {
            while reg_read32(REG_UART0_FSR + off) & TX_EMPTY == 0 {}
            reg_write8(REG_UART0_THR + off, b'\r');
        }
    }

    /// Try to receive a byte (non-blocking). Returns `None` if no data.
    pub unsafe fn getchar(&self) -> Option<u8> {
        let off = self.port.offset();
        if reg_read32(REG_UART0_FSR + off) & RX_NOT_EMPTY != 0 {
            Some(reg_read8(REG_UART0_RBR + off))
        } else {
            None
        }
    }

    /// Blocking write of a byte slice.
    pub unsafe fn write(&self, data: &[u8]) {
        for &byte in data {
            self.putchar(byte);
        }
    }

    /// Write a string (blocking).
    pub unsafe fn puts(&self, s: &str) {
        self.write(s.as_bytes());
    }

    /// Formatted print (supports `%s`, `%d`, `%x`, `%X`, `%c`).
    pub unsafe fn printf(&self, fmt: &str, args: &[UartArg]) {
        let bytes = fmt.as_bytes();
        let mut arg_idx = 0usize;
        let mut i = 0usize;

        while i < bytes.len() {
            if bytes[i] == b'%' && i + 1 < bytes.len() {
                i += 1;
                if arg_idx >= args.len() {
                    break;
                }
                match bytes[i] {
                    b's' => {
                        if let UartArg::Str(s) = &args[arg_idx] {
                            self.puts(s);
                        }
                        arg_idx += 1;
                    }
                    b'd' => {
                        if let UartArg::U32(val) = args[arg_idx] {
                            self.print_dec(val);
                        }
                        arg_idx += 1;
                    }
                    b'x' | b'X' => {
                        if let UartArg::U32(val) = args[arg_idx] {
                            self.print_hex(val, bytes[i] == b'X');
                        }
                        arg_idx += 1;
                    }
                    b'c' => {
                        if let UartArg::U32(val) = args[arg_idx] {
                            self.putchar(val as u8);
                        }
                        arg_idx += 1;
                    }
                    _ => {
                        self.putchar(b'%');
                        self.putchar(bytes[i]);
                    }
                }
            } else {
                self.putchar(bytes[i]);
            }
            i += 1;
        }
    }

    // --- Internal formatting helpers ---

    unsafe fn print_hex(&self, mut value: u32, upper: bool) {
        let mut buf: [u8; 8] = [0; 8];
        let mut idx = 0usize;

        if value == 0 {
            self.putchar(b'0');
            return;
        }

        let a = if upper { b'A' } else { b'a' };

        while value != 0 {
            let digit = (value & 0xF) as u8;
            buf[idx] = if digit < 10 { b'0' + digit } else { a + digit - 10 };
            idx += 1;
            value >>= 4;
        }

        while idx > 0 {
            idx -= 1;
            self.putchar(buf[idx]);
        }
    }

    unsafe fn print_dec(&self, mut value: u32) {
        let mut buf: [u8; 12] = [0; 12];
        let mut idx = 0usize;

        if value == 0 {
            self.putchar(b'0');
            return;
        }

        while value != 0 {
            buf[idx] = b'0' + (value % 10) as u8;
            idx += 1;
            value /= 10;
        }

        while idx > 0 {
            idx -= 1;
            self.putchar(buf[idx]);
        }
    }
}

// ============================================================================
// Global UART convenience (stateless, matches the C BSP pattern)
// ============================================================================

/// Active port offset; initialised by `uart_port_select`.
static mut UART_PORT_OFFSET: u32 = 0x100;

/// Select the UART port for the global convenience functions.
///
/// # Safety
///
/// Must be called once before any other global UART function.
pub unsafe fn uart_port_select(port: UartPort) {
    UART_PORT_OFFSET = port.offset();
}

/// Initialise the globally-selected UART with a default 115200-8N1 config.
pub unsafe fn uart_init_default() {
    let off = UART_PORT_OFFSET;

    // UART1: enable multifunction pins
    if off == 0x100 {
        reg_write32(REG_GPAFUN, reg_read32(REG_GPAFUN) | MF_GPA10 | MF_GPA11);
        reg_write32(REG_CLKDIV3, reg_read32(REG_CLKDIV3) & !UART1_N);
        reg_write32(REG_APBCLK, reg_read32(REG_APBCLK) | UART1_CKE);
    } else {
        // UART0
        let val = reg_read32(REG_GPDFUN);
        reg_write32(
            REG_GPDFUN,
            (val & !(MF_GPD1 | MF_GPD2)) | (0x5 | (0x5 << 2)),
        );
        reg_write32(REG_CLKDIV3, reg_read32(REG_CLKDIV3) & !UART0_N);
        reg_write32(REG_APBCLK, reg_read32(REG_APBCLK) | UART0_CKE);
    }

    // Reset FIFOs
    reg_write32(REG_UART0_FCR + off, 0x07);

    // Baud rate: 115200
    let baud_val = EXTERNAL_CRYSTAL_CLOCK / 115_200 - 2;
    reg_write32(REG_UART0_BAUD + off, (0x30 << 24) | baud_val);

    // 8N1
    reg_write32(REG_UART0_LCR + off, WL_8BIT);

    // Timeout
    reg_write32(REG_UART0_TOR + off, 0x80 + 0x20);

    // FIFO: 1-byte trigger, enable
    reg_write32(REG_UART0_FCR + off, 0x01);
}

/// Output a single character via the global UART (blocking).
pub unsafe fn uart_putchar(c: u8) {
    let off = UART_PORT_OFFSET;
    while reg_read32(REG_UART0_FSR + off) & TX_EMPTY == 0 {}
    reg_write8(REG_UART0_THR + off, c);

    if c == b'\n' {
        while reg_read32(REG_UART0_FSR + off) & TX_EMPTY == 0 {}
        reg_write8(REG_UART0_THR + off, b'\r');
    }
}

/// Print a nul-terminated string via the global UART.
pub unsafe fn uart_puts(s: &str) {
    for byte in s.bytes() {
        uart_putchar(byte);
    }
}

/// Try to read a byte (non-blocking). `None` if no data available.
pub unsafe fn uart_getchar() -> Option<u8> {
    let off = UART_PORT_OFFSET;
    if reg_read32(REG_UART0_FSR + off) & RX_NOT_EMPTY != 0 {
        Some(reg_read8(REG_UART0_RBR + off))
    } else {
        None
    }
}

/// Low-level formatted print via the global UART.
pub unsafe fn uart_printf(fmt: &str, args: &[UartArg]) {
    let bytes = fmt.as_bytes();
    let mut arg_idx = 0usize;
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i] == b'%' && i + 1 < bytes.len() {
            i += 1;
            if arg_idx >= args.len() {
                break;
            }
            match bytes[i] {
                b's' => {
                    if let UartArg::Str(s) = &args[arg_idx] {
                        uart_puts(s);
                    }
                    arg_idx += 1;
                }
                b'd' => {
                    if let UartArg::U32(val) = args[arg_idx] {
                        uart_print_dec(val);
                    }
                    arg_idx += 1;
                }
                b'x' | b'X' => {
                    if let UartArg::U32(val) = args[arg_idx] {
                        uart_print_hex(val);
                    }
                    arg_idx += 1;
                }
                b'c' => {
                    if let UartArg::U32(val) = args[arg_idx] {
                        uart_putchar(val as u8);
                    }
                    arg_idx += 1;
                }
                _ => {
                    uart_putchar(b'%');
                    uart_putchar(bytes[i]);
                }
            }
        } else {
            uart_putchar(bytes[i]);
        }
        i += 1;
    }
}

// Internal formatting helpers for the global UART
unsafe fn uart_print_hex(mut value: u32) {
    let mut buf: [u8; 8] = [0; 8];
    let mut idx = 0usize;
    if value == 0 {
        uart_putchar(b'0');
        return;
    }
    while value != 0 {
        let digit = (value & 0xF) as u8;
        buf[idx] = if digit < 10 { b'0' + digit } else { b'A' + digit - 10 };
        idx += 1;
        value >>= 4;
    }
    while idx > 0 {
        idx -= 1;
        uart_putchar(buf[idx]);
    }
}

unsafe fn uart_print_dec(mut value: u32) {
    let mut buf: [u8; 12] = [0; 12];
    let mut idx = 0usize;
    if value == 0 {
        uart_putchar(b'0');
        return;
    }
    while value != 0 {
        buf[idx] = b'0' + (value % 10) as u8;
        idx += 1;
        value /= 10;
    }
    while idx > 0 {
        idx -= 1;
        uart_putchar(buf[idx]);
    }
}

// ============================================================================
// Convenience Macros
// ============================================================================

/// Print formatted string with u32 args via the global UART.
/// Usage: `uart_printf_u32!("val=%d hex=%x\n", 42, 0xABCD);`
#[macro_export]
macro_rules! uart_printf_u32 {
    ($uart:expr, $fmt:literal $(, $arg:expr)*) => {{
        unsafe {
            $uart.printf($fmt, &[$($crate::uart::UartArg::U32($arg as u32)),*]);
        }
    }};
}

/// Print formatted string with u32 args (global UART version).
#[macro_export]
macro_rules! sysprintf {
    ($fmt:literal $(, $arg:expr)*) => {{
        unsafe {
            $crate::uart::uart_printf($fmt, &[$($crate::uart::UartArg::U32($arg as u32)),*]);
        }
    }};
}

/// Print formatted string with mixed u32 and &str args (global UART version).
#[macro_export]
macro_rules! sysprintf_mixed {
    ($fmt:literal $(, $arg:expr)*) => {{
        unsafe {
            $crate::uart::uart_printf($fmt, &[$($arg),*]);
        }
    }};
}
