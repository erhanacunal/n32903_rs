//! I2C (Inter-Integrated Circuit) bus driver for N32903 (W55FA93)
//! Ported from W55FA93_I2C.h, DrvI2CH.c, and i2c.c
//!
//! Provides both low-level hardware access (start/stop/read/write/ack)
//! and a high-level API (open/close/read/write/ioctl) for I2C
//! communication with sub-addressed devices.
//!
//! Pins: GPB13 = SDA, GPB14 = SCL
//!
//! # Safety
//!
//! All functions are `unsafe` — they manipulate hardware registers directly.

use crate::registers::*;

// ============================================================================
// Constants
// ============================================================================

/// Maximum I2C transfer buffer size
pub const I2C_MAX_BUF_LEN: usize = 450;

/// Default input clock in KHz
pub const I2C_INPUT_CLOCK: u32 = 33000;

// ============================================================================
// Public Types
// ============================================================================

/// I2C bus speed
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum I2cSpeed {
    Standard100k = 100,
    Fast400k = 400,
}

/// I2C error codes
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum I2cError {
    Success = 0,
    LostArbitration = 0xFFFF1101,
    BusBusy = 0xFFFF1102,
    Nack = 0xFFFF1103,
    SlaveNack = 0xFFFF1104,
    NoDev = 0xFFFF1105,
    Busy = 0xFFFF1106,
    Io = 0xFFFF1107,
    NotSupported = 0xFFFF1108,
    Timeout = 0xFFFF1109,
    WrongLength = 0xFFFF1100,
}

/// I2C device state
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[allow(dead_code)]
enum I2cState {
    Nop = 0,
    Read = 1,
    Write = 2,
    Probe = 3,
}

// ============================================================================
// Static State
// ============================================================================

/// I2C device context
pub struct I2cDevice {
    open: bool,
    state: I2cState,
    addr: i32,
    last_error: u32,
    subaddr: u32,
    subaddr_len: i32,
    buffer: [u8; I2C_MAX_BUF_LEN],
    pos: usize,
    len: usize,
}

static mut I2C_DEVICE: I2cDevice = I2cDevice {
    open: false,
    state: I2cState::Nop,
    addr: -1,
    last_error: 0,
    subaddr: 0,
    subaddr_len: 0,
    buffer: [0u8; I2C_MAX_BUF_LEN],
    pos: 0,
    len: 0,
};

static mut I2C_SPEED: i32 = 100;

/// Interrupt callback
static mut I2C_CALLBACK: Option<unsafe extern "C" fn()> = None;

// ============================================================================
// Low-Level Hardware Functions (DrvI2CH layer)
// ============================================================================

/// Check if I2C hardware is busy (transfer in progress).
pub unsafe fn i2c_hw_is_busy() -> bool {
    reg_read32(REG_I2C_CSR) & I2C_TIP != 0
}

/// Check if I2C bus is busy (START detected, STOP not yet sent).
pub unsafe fn i2c_hw_is_bus_busy() -> bool {
    reg_read32(REG_I2C_CSR) & I2C_BUSY != 0
}

/// Check if bus is free (both SDA and SCK high, no BUSY flag).
pub unsafe fn i2c_is_bus_free() -> bool {
    (reg_read32(REG_I2C_SWR) & 0x18 == 0x18) && (reg_read32(REG_I2C_CSR) & I2C_BUSY == 0)
}

/// Check for arbitration lost. If lost, sends START+STOP to recover.
pub unsafe fn i2c_hw_is_arbit_lost() -> bool {
    if reg_read32(REG_I2C_CSR) & I2C_AL == 0 {
        return false;
    }
    // Recovery: send START then STOP
    reg_write32(REG_I2C_CMDR, I2C_START);
    reg_write32(REG_I2C_CMDR, I2C_STOP);
    true
}

/// Set burst transfer count (1–4).
pub unsafe fn i2c_hw_set_burst_cnt(cnt: u8) -> Result<(), I2cError> {
    if cnt == 0 || cnt > 4 {
        return Err(I2cError::WrongLength);
    }
    let mut csr = reg_read32(REG_I2C_CSR);
    csr &= !TX_NUM;
    csr |= ((cnt - 1) as u32) << 4;
    reg_write32(REG_I2C_CSR, csr);
    Ok(())
}

/// Write data to the transmit register.
pub unsafe fn i2c_hw_set_tx_data(data: u32) {
    reg_write32(REG_I2C_TxR, data);
}

/// Send a command to the I2C hardware.
pub unsafe fn i2c_hw_send_cmd(cmd: u32) {
    reg_write32(REG_I2C_CMDR, cmd);
}

/// Check if ACK was received from slave (RXACK=0 means ACK).
pub unsafe fn i2c_hw_is_ack() -> bool {
    reg_read32(REG_I2C_CSR) & I2C_RXACK == 0
}

/// Read received data.
pub unsafe fn i2c_hw_get_rx_data() -> u8 {
    (reg_read32(REG_I2C_RxR) & 0xFF) as u8
}

/// Enable I2C interrupt.
pub unsafe fn i2c_hw_enable_int() {
    reg_write32(REG_I2C_CSR, reg_read32(REG_I2C_CSR) | CSR_IE);
}

/// Disable I2C interrupt.
pub unsafe fn i2c_hw_disable_int() {
    reg_write32(REG_I2C_CSR, reg_read32(REG_I2C_CSR) & !CSR_IE);
}

/// Check if I2C interrupt is enabled.
pub unsafe fn i2c_hw_is_int_enabled() -> bool {
    reg_read32(REG_I2C_CSR) & CSR_IE != 0
}

/// Poll the interrupt flag.
pub unsafe fn i2c_hw_poll_int() -> bool {
    reg_read32(REG_I2C_CSR) & CSR_IF != 0
}

/// Clear the interrupt flag.
pub unsafe fn i2c_hw_clear_int() {
    reg_write32(REG_I2C_CSR, reg_read32(REG_I2C_CSR) | CSR_IF);
}

/// Initialise SDA/SCL pins and enable I2C core with interrupts.
pub unsafe fn i2c_hw_init_sda_sck() {
    let mut csr = reg_read32(REG_I2C_CSR);
    csr &= !(I2C_EN | TX_NUM);
    reg_write32(REG_I2C_CSR, csr);
    reg_write32(REG_I2C_CSR, reg_read32(REG_I2C_CSR) | CSR_IF | CSR_IE | I2C_EN);
}

/// Open the I2C hardware and configure bus clock.
pub unsafe fn i2c_hw_open(bus_clock_hz: u32) {
    // Enable I2C engine clock
    reg_write32(REG_APBCLK, reg_read32(REG_APBCLK) | I2C_CKE);

    // Reset I2C engine
    reg_write32(REG_APBIPRST, reg_read32(REG_APBIPRST) | I2CRST);
    reg_write32(REG_APBIPRST, reg_read32(REG_APBIPRST) & !I2CRST);

    let apb_khz = crate::clock::get_apb_clock();

    let mut divider = apb_khz / (5 * bus_clock_hz / 1000) - 1;
    if divider <= 1 {
        divider = 1;
    }

    reg_write32(REG_I2C_DIVIDER, divider);

    let csr = reg_read32(REG_I2C_CSR) & !TX_NUM;
    reg_write32(REG_I2C_CSR, csr | I2C_EN);
}

/// Close the I2C hardware.
pub unsafe fn i2c_hw_close() {
    reg_write32(REG_APBIPRST, reg_read32(REG_APBIPRST) | I2CRST);
    reg_write32(REG_APBIPRST, reg_read32(REG_APBIPRST) & !I2CRST);
    reg_write32(REG_APBCLK, reg_read32(REG_APBCLK) & !I2C_CKE);
}

/// Wait for I2C hardware to become ready.
pub unsafe fn i2c_hw_wait_ready() -> Result<(), I2cError> {
    if i2c_hw_is_arbit_lost() {
        return Err(I2cError::LostArbitration);
    }

    let mut delay = 0u32;
    while i2c_hw_is_busy() && delay != 500 {
        // ~10us delay via simple loop
        for _ in 0..10 {
            core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
        }
        delay += 1;
    }

    if delay == 500 {
        return Err(I2cError::Timeout);
    }
    Ok(())
}

/// Send START condition.
pub unsafe fn i2c_hw_send_start() -> Result<(), I2cError> {
    i2c_hw_send_cmd(I2C_START);
    i2c_hw_wait_ready()
}

/// Send STOP condition.
pub unsafe fn i2c_hw_send_stop() -> Result<(), I2cError> {
    i2c_hw_send_cmd(I2C_STOP);
    i2c_hw_wait_ready()
}

/// Write a byte to the I2C bus.
pub unsafe fn i2c_hw_write_byte(
    start: bool,
    data: u8,
    check_ack: bool,
    stop: bool,
) -> Result<(), I2cError> {
    if i2c_hw_is_arbit_lost() {
        return Err(I2cError::LostArbitration);
    }

    i2c_hw_set_burst_cnt(1)?;
    i2c_hw_set_tx_data(data as u32);

    let mut cmd = I2C_WRITE;
    if start {
        cmd |= I2C_START;
    }
    if stop {
        cmd |= I2C_STOP;
    }
    i2c_hw_send_cmd(cmd);

    i2c_hw_wait_ready()?;

    if check_ack && !i2c_hw_is_ack() {
        return Err(I2cError::Nack);
    }
    Ok(())
}

/// Read a byte from the I2C bus.
pub unsafe fn i2c_hw_read_byte(
    start: bool,
    send_ack: bool,
    stop: bool,
) -> Result<u8, I2cError> {
    let mut cmd = I2C_READ;
    if start {
        cmd |= I2C_START;
    }
    if !send_ack {
        cmd |= I2C_ACK; // NACK (not sending ACK) = ACK bit set
    }
    if stop {
        cmd |= I2C_STOP;
    }
    i2c_hw_send_cmd(cmd);

    i2c_hw_wait_ready()?;
    Ok(i2c_hw_get_rx_data())
}

// ============================================================================
// High-Level API (i2c layer)
// ============================================================================

/// Set the I2C bus speed by computing the divider from APB clock.
unsafe fn i2c_set_speed(speed_khz: i32) -> Result<(), I2cError> {
    let apb_khz = crate::clock::get_apb_clock();
    let d = (apb_khz / (speed_khz as u32 * 5)).wrapping_sub(1);
    reg_write32(REG_I2C_DIVIDER, d & 0xFFFF);
    Ok(())
}

/// Compute the address buffer for a sub-addressed transfer.
/// Places (addr<<1 | WRITE), subaddr bytes, and optionally (addr<<1 | READ)
/// into the internal buffer.
unsafe fn i2c_calc_addr(mode: I2cState) {
    let dev = &raw mut I2C_DEVICE;
    let mut subaddr = (*dev).subaddr;

    (*dev).buffer[0] = (((*dev).addr << 1) as u32 & 0xFE | 0x00) as u8; // WRITE

    for i in (1..=((*dev).subaddr_len as usize)).rev() {
        (*dev).buffer[i] = subaddr as u8;
        subaddr >>= 8;
    }

    if mode == I2cState::Read {
        let i = (*dev).subaddr_len as usize + 1;
        (*dev).buffer[i] = (((*dev).addr << 1) as u32 & 0xFE | 0x01) as u8; // READ
    }
}

/// Reset the I2C device state.
unsafe fn i2c_reset() {
    let dev = &raw mut I2C_DEVICE;
    (*dev).addr = -1;
    (*dev).last_error = 0;
    (*dev).subaddr = 0;
    (*dev).subaddr_len = 0;
    let _ = i2c_set_speed(100);
}

/// Reset I2C bus if stuck (hardware reset).
unsafe fn i2c_bus_reset() {
    reg_write32(REG_APBIPRST, reg_read32(REG_APBIPRST) | I2CRST);
    reg_write32(REG_APBIPRST, reg_read32(REG_APBIPRST) & !I2CRST);
    reg_write32(REG_I2C_CSR, reg_read32(REG_I2C_CSR) | I2C_EN);
    let _ = i2c_set_speed(I2C_SPEED);
}

// ============================================================================
// Public API
// ============================================================================

/// Initialise I2C GPIO pins and reset the I2C hardware.
pub unsafe fn i2c_init() {
    // Configure GPB13 (SDA) and GPB14 (SCL) to I2C function
    reg_write32(REG_GPBFUN, reg_read32(REG_GPBFUN) | MF_GPB13 | MF_GPB14);

    // Reset I2C
    reg_write32(REG_APBIPRST, reg_read32(REG_APBIPRST) | I2CRST);
    reg_write32(REG_APBIPRST, reg_read32(REG_APBIPRST) & !I2CRST);

    // Clear device state
    let dev = &raw mut I2C_DEVICE;
    core::ptr::write_bytes(dev as *mut u8, 0, 1);
}

/// Open the I2C bus. Must be called before read/write/ioctl.
pub unsafe fn i2c_open() -> Result<(), I2cError> {
    let dev = &raw mut I2C_DEVICE;
    if (*dev).open {
        return Err(I2cError::Busy);
    }

    // Enable I2C clock
    reg_write32(REG_APBCLK, reg_read32(REG_APBCLK) | I2C_CKE);

    // Enable I2C core
    reg_write32(REG_I2C_CSR, reg_read32(REG_I2C_CSR) | I2C_EN);

    // Zero device
    core::ptr::write_bytes(dev as *mut u8, 0, 1);

    i2c_reset();
    (*dev).open = true;

    Ok(())
}

/// Close the I2C bus.
pub unsafe fn i2c_close() -> Result<(), I2cError> {
    reg_write32(REG_I2C_CSR, reg_read32(REG_I2C_CSR) & !I2C_EN);
    reg_write32(REG_APBCLK, reg_read32(REG_APBCLK) & !I2C_CKE);

    let dev = &raw mut I2C_DEVICE;
    (*dev).open = false;
    Ok(())
}

/// Read data from an I2C slave device.
///
/// Uses the device address and sub-address previously set via `i2c_ioctl`.
/// Returns the number of bytes read or an error.
pub unsafe fn i2c_read(buf: &mut [u8]) -> Result<usize, I2cError> {
    let dev = &raw mut I2C_DEVICE;
    if !(*dev).open {
        return Err(I2cError::Io);
    }
    let len = buf.len();
    if len == 0 {
        return Ok(0);
    }
    if !i2c_is_bus_free() {
        return Err(I2cError::Busy);
    }
    let len = len.min(I2C_MAX_BUF_LEN - 10);

    (*dev).state = I2cState::Read;
    (*dev).pos = 1;
    (*dev).len = (*dev).subaddr_len as usize + 1 + len + 1;
    (*dev).last_error = 0;

    i2c_calc_addr(I2cState::Read);

    // Send START + chip address (WRITE)
    i2c_hw_write_byte(true, (*dev).buffer[0], true, false)?;

    // Send sub-address bytes
    for i in 1..((*dev).subaddr_len as usize + 1) {
        i2c_hw_write_byte(false, (*dev).buffer[i], true, false)?;
    }

    // Send repeated START + chip address (READ)
    let addr_idx = (*dev).subaddr_len as usize + 1;
    i2c_hw_write_byte(true, (*dev).buffer[addr_idx], true, false)?;

    // Read data bytes
    for i in 0..len {
        if i == len - 1 {
            // Last byte: NACK + STOP
            match i2c_hw_read_byte(false, false, true) {
                Ok(b) => buf[i] = b,
                Err(_) => {
                    if !i2c_is_bus_free() {
                        i2c_bus_reset();
                    }
                    i2c_hw_send_stop().ok();
                    return Err(I2cError::Nack);
                }
            }
            if !i2c_is_bus_free() {
                i2c_bus_reset();
            }
        } else {
            buf[i] = i2c_hw_read_byte(false, true, false)?;
        }
    }

    Ok(len)
}

/// Write data to an I2C slave device.
///
/// Uses the device address and sub-address previously set via `i2c_ioctl`.
/// Returns the number of bytes written or an error.
pub unsafe fn i2c_write(buf: &[u8]) -> Result<usize, I2cError> {
    let dev = &raw mut I2C_DEVICE;
    if !(*dev).open {
        return Err(I2cError::Io);
    }
    let len = buf.len();
    if len == 0 {
        return Ok(0);
    }
    if !i2c_is_bus_free() {
        return Err(I2cError::Busy);
    }
    let len = len.min(I2C_MAX_BUF_LEN - 10);

    (*dev).state = I2cState::Write;
    (*dev).pos = 1;
    (*dev).len = (*dev).subaddr_len as usize + 1 + len;
    (*dev).last_error = 0;

    i2c_calc_addr(I2cState::Write);

    // Send START + chip address (WRITE)
    i2c_hw_write_byte(true, (*dev).buffer[0], true, false)?;

    // Send sub-address bytes
    for i in 1..((*dev).subaddr_len as usize + 1) {
        i2c_hw_write_byte(false, (*dev).buffer[i], true, false)?;
    }

    // Send data bytes
    for i in 0..len {
        let stop = i == len - 1;
        i2c_hw_write_byte(false, buf[i], true, stop)?;
        if stop && !i2c_is_bus_free() {
            i2c_bus_reset();
        }
    }

    Ok(len)
}

/// I2C control commands (ioctl equivalent).
pub unsafe fn i2c_ioctl(cmd: I2cIoctl, arg0: u32, arg1: u32) -> Result<(), I2cError> {
    let dev = &raw mut I2C_DEVICE;
    if !(*dev).open {
        return Err(I2cError::Io);
    }

    match cmd {
        I2cIoctl::SetDevAddress => {
            (*dev).addr = arg0 as i32;
        }
        I2cIoctl::SetSpeed => {
            I2C_SPEED = arg0 as i32;
            i2c_set_speed(arg0 as i32)?;
        }
        I2cIoctl::SetSubAddress => {
            if arg1 > 4 {
                return Err(I2cError::NotSupported);
            }
            (*dev).subaddr = arg0;
            (*dev).subaddr_len = arg1 as i32;
        }
    }
    Ok(())
}

/// Get the device context pointer (for interrupt-driven use).
pub unsafe fn i2c_get_device_ptr() -> *const I2cDevice {
    &raw const I2C_DEVICE
}

// ============================================================================
// I2C IOCTL Commands
// ============================================================================

/// I2C ioctl command
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum I2cIoctl {
    SetDevAddress = 0,
    SetSubAddress = 1,
    SetSpeed = 2,
}

// ============================================================================
// Interrupt
// ============================================================================

/// I2C ISR — call from your interrupt handler if you registered a callback.
pub unsafe extern "C" fn i2c_isr() {
    if let Some(cb) = I2C_CALLBACK {
        if i2c_hw_is_int_enabled() && i2c_hw_poll_int() {
            cb();
        }
    }
    i2c_hw_clear_int();
}

/// Install an I2C interrupt callback.
pub unsafe fn i2c_install_callback(cb: unsafe extern "C" fn()) {
    I2C_CALLBACK = Some(cb);
    crate::aic::aic_install_isr(
        crate::aic::IntLevel::Irq1,
        IRQ_I2C,
        i2c_isr as unsafe extern "C" fn(),
    );
    crate::aic::aic_enable_interrupt(IRQ_I2C);
}
