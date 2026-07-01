//! System-level utilities for N32903 (W55FA93)
//! Ported from wblib.h, wberrcode.h, wb_uartdev.c
//!
//! Provides DateTime, error code constants, wake-up source definitions,
//! and UART device registration.

use crate::registers::*;

// ============================================================================
// Return Codes
// ============================================================================

/// Standard BSP return value
pub const SUCCESS: i32 = 0;
pub const FAIL: i32 = 1;

// ============================================================================
// Error Code Module IDs
// ============================================================================

pub const FMI_ERR_ID: u32 = 0xFFFF_0100;
pub const APU_ERR_ID: u32 = 0xFFFF_0200;
pub const USB_ERR_ID: u32 = 0xFFFF_0300;
pub const GDMA_ERR_ID: u32 = 0xFFFF_0400;
pub const JPG_ERR_ID: u32 = 0xFFFF_0500;
pub const DMAC_ERR_ID: u32 = 0xFFFF_0600;
pub const TMR_ERR_ID: u32 = 0xFFFF_0700;
pub const GE_ERR_ID: u32 = 0xFFFF_0800;
pub const AIC_ERR_ID: u32 = 0xFFFF_0900;
pub const SYSLIB_ERR_ID: u32 = 0xFFFF_0A00;
pub const USBO_ERR_ID: u32 = 0xFFFF_0C00;
pub const USBH_ERR_ID: u32 = 0xFFFF_0D00;
pub const RTC_ERR_ID: u32 = 0xFFFF_0E00;
pub const GPIO_ERR_ID: u32 = 0xFFFF_0F00;
pub const VIN_ERR_ID: u32 = 0xFFFF_1000;
pub const I2C_ERR_ID: u32 = 0xFFFF_1100;
pub const SPI_ERR_ID: u32 = 0xFFFF_1200;
pub const PWM_ERR_ID: u32 = 0xFFFF_1300;
pub const BLT_ERR_ID: u32 = 0xFFFF_1500;
pub const UART_ERR_ID: u32 = 0xFFFF_1700;
pub const LCD_ERR_ID: u32 = 0xFFFF_1800;
pub const ADC_ERR_ID: u32 = 0xFFFF_1A00;
pub const FAT_ERR_ID: u32 = 0xFFFF_8200;

// ============================================================================
// DateTime
// ============================================================================

/// Date / time representation
#[derive(Clone, Copy, Debug)]
pub struct DateTime {
    pub year: u32,
    pub mon: u32,
    pub day: u32,
    pub hour: u32,
    pub min: u32,
    pub sec: u32,
}

// ============================================================================
// Wake-Up Sources
// ============================================================================

/// Wake-up source bit indices (for REG_MISCR wake-up configuration)
pub const WAKEUP_GPIO: u32 = 0;
pub const WAKEUP_RTC: u32 = 1;
pub const WAKEUP_SDH: u32 = 2;
pub const WAKEUP_UART: u32 = 3;
pub const WAKEUP_UDC: u32 = 4;
pub const WAKEUP_UHC: u32 = 5;
pub const WAKEUP_ADC: u32 = 6;
pub const WAKEUP_KPI: u32 = 7;

// ============================================================================
// System Constant: External Crystal
// ============================================================================

pub const EXTERNAL_XTAL_HZ: u32 = EXTERNAL_CRYSTAL_CLOCK;

// ============================================================================
// UART Device Registration
// ============================================================================

/// UART device function table (virtual methods for UART operations).
/// Mirrors `UARTDEV_T` from wblib.h.
pub struct UartDevice {
    pub uart_port: unsafe extern "C" fn(port: u32),
    pub uart_install_callback: unsafe extern "C" fn(int_type: u32, callback: *const ()),
    pub uart_initialize: unsafe extern "C" fn(cfg: *const ()) -> i32,
    pub uart_enable_int: unsafe extern "C" fn(int_type: i32),
    pub uart_transfer: unsafe extern "C" fn(buf: *const u8, len: u32),
    pub uart_put_char: unsafe extern "C" fn(ch: u8),
    pub uart_get_char: unsafe extern "C" fn() -> i8,
    pub uart_transfer_int: unsafe extern "C" fn(buf: *const u8, len: u32) -> i32,
    pub uart_get_char_nonblock: unsafe extern "C" fn() -> i8,
}

/// Register a UART device by port number.
///
/// In the full BSP this copies a pre-built device table.  In this
/// library use [`crate::uart::Uart::init`] directly.
pub unsafe fn register_uart_device(port: u32, _dev: *mut UartDevice) -> i32 {
    match port {
        0 | 1 => SUCCESS,
        _ => -1,
    }
}
