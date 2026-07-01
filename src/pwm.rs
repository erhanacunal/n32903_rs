//! PWM (Pulse Width Modulation) driver for N32903 (W55FA93)
//! Ported from PWM.c and PWM.h
//!
//! Provides 4 PWM output channels and 4 capture input channels.
//! Each channel supports configurable frequency, duty cycle, dead-zone,
//! inverter, one-shot/toggle mode, and interrupt callbacks.
//!
//! PWM output pins: GPD[0]–GPD[3]
//! Capture input pins: GPD[0]–GPD[3]
//!
//! # Safety
//!
//! All functions are `unsafe` — they manipulate hardware registers directly.

use crate::registers::*;

// ============================================================================
// Public Types
// ============================================================================

/// PWM channel index
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PwmChannel {
    Ch0 = 0,
    Ch1 = 1,
    Ch2 = 2,
    Ch3 = 3,
}

/// Capture channel index
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CapChannel {
    Cap0 = 0x10,
    Cap1 = 0x11,
    Cap2 = 0x12,
    Cap3 = 0x13,
}

/// PWM output mode
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PwmMode {
    OneShot = 0,
    Toggle = 1,
}

/// Clock divider for PWM timer
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum PwmClkDiv {
    Div1 = 1,
    Div2 = 2,
    Div4 = 4,
    Div8 = 8,
    Div16 = 16,
}

/// Capture interrupt type
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CapIntType {
    Rising = 1,
    Falling = 2,
    Both = 0,
}

/// Capture interrupt flag (for status queries)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum CapIntFlag {
    Rising = 6,
    Falling = 7,
}

/// PWM configuration parameters
#[derive(Clone, Copy, Debug)]
pub struct PwmConfig {
    /// Toggle or one-shot mode
    pub mode: PwmMode,
    /// Target frequency in Hz (0 = use manual clock/prescaler/duty)
    pub frequency: f32,
    /// High pulse ratio (0–100 %)
    pub high_pulse_ratio: u8,
    /// Invert output polarity
    pub inverter: bool,
    /// Clock divider (used when frequency == 0)
    pub clock_div: PwmClkDiv,
    /// Prescaler 2–256 (used when frequency == 0)
    pub prescale: u8,
    /// Manual duty cycle value (used when frequency == 0 or for capture)
    pub duty: u32,
}

impl Default for PwmConfig {
    fn default() -> Self {
        Self {
            mode: PwmMode::Toggle,
            frequency: 1000.0,
            high_pulse_ratio: 50,
            inverter: false,
            clock_div: PwmClkDiv::Div1,
            prescale: 2,
            duty: 0,
        }
    }
}

/// Callback type for PWM events
pub type PwmCallback = unsafe extern "C" fn();

// ============================================================================
// Constants
// ============================================================================

/// Per-channel register stride: CNR(n) = PWM_BA + 0x0C + n*12
const CH_STRIDE: u32 = 12;

/// Capture latch stride: CRLR/CFLR(n) = PWM_BA + 0x058 + n*8
const CAP_STRIDE: u32 = 8;

/// Bit position of CHnEN in PCR: ch0=0, ch1=8, ch2=16, ch3=24
const CH_EN_SHIFT: [u32; 4] = [0, 8, 16, 24];
/// Bit position of CHnMOD in PCR: ch0=3, ch1=11, ch2=19, ch3=27
const CH_MOD_SHIFT: [u32; 4] = [3, 11, 19, 27];
/// Bit position of CHnINV in PCR: ch0=2, ch1=10, ch2=18, ch3=26
const CH_INV_SHIFT: [u32; 4] = [2, 10, 18, 26];

// ============================================================================
// Static State
// ============================================================================

/// Callback table for all 4 PWM timer and 4 capture interrupts
struct CallbackTable {
    pwm: [Option<PwmCallback>; 4],
    cap: [Option<PwmCallback>; 4],
}

static mut CALLBACKS: CallbackTable = CallbackTable {
    pwm: [None; 4],
    cap: [None; 4],
};

// ============================================================================
// Register Helpers
// ============================================================================

/// CNR register address for a channel (0–3)
fn cnr_reg(ch: u8) -> u32 {
    REG_CNR0 + (ch as u32) * CH_STRIDE
}

/// CMR register address for a channel (0–3)
fn cmr_reg(ch: u8) -> u32 {
    REG_CMR0 + (ch as u32) * CH_STRIDE
}

/// PDR register address for a channel (0–3)
fn pdr_reg(ch: u8) -> u32 {
    REG_PDR0 + (ch as u32) * CH_STRIDE
}

/// CRLR register address for a capture channel (0–3)
fn crlr_reg(ch: u8) -> u32 {
    REG_CRLR0 + (ch as u32) * CAP_STRIDE
}

/// CFLR register address for a capture channel (0–3)
fn cflr_reg(ch: u8) -> u32 {
    REG_CFLR0 + (ch as u32) * CAP_STRIDE
}

// ============================================================================
// Open / Close
// ============================================================================

/// Enable the PWM engine clock and reset the PWM module.
pub unsafe fn pwm_open() {
    reg_write32(REG_APBCLK, reg_read32(REG_APBCLK) | PWM_CKE);
    reg_write32(REG_APBIPRST, reg_read32(REG_APBIPRST) | PWMRST);
    reg_write32(REG_APBIPRST, reg_read32(REG_APBIPRST) & !PWMRST);
}

/// Disable all PWM outputs/capture inputs, disable PWM interrupt,
/// and gate the PWM clock.
pub unsafe fn pwm_close() {
    reg_write32(REG_POE, 0);
    reg_write32(REG_CAPENR, 0);
    crate::aic::aic_disable_interrupt(IRQ_PWM);
    reg_write32(REG_APBCLK, reg_read32(REG_APBCLK) & !PWM_CKE);
}

// ============================================================================
// Channel Enable / Disable
// ============================================================================

/// Check whether a PWM timer channel is enabled.
pub unsafe fn pwm_is_enabled(ch: PwmChannel) -> bool {
    let idx = ch as u8 as usize;
    reg_read32(REG_PCR) & (1 << CH_EN_SHIFT[idx]) != 0
}

/// Enable or disable a PWM timer / capture channel.
pub unsafe fn pwm_enable(ch: u8, enable: bool) {
    let idx = (ch & 0x07) as usize;
    let mut pcr = reg_read32(REG_PCR);
    if enable {
        pcr |= 1 << CH_EN_SHIFT[idx];
    } else {
        pcr &= !(1 << CH_EN_SHIFT[idx]);
    }
    reg_write32(REG_PCR, pcr);

    // For capture channels, also set CAPCHnEN
    if ch & 0x10 != 0 {
        let cap_bit = 1u32 << (((ch as u32 & 0x01) << 4) + 3);
        let ccr_reg = if ch & 0x02 != 0 { REG_CCR1 } else { REG_CCR0 };
        let mut ccr = reg_read32(ccr_reg);
        if enable {
            ccr |= cap_bit;
        } else {
            ccr &= !cap_bit;
        }
        reg_write32(ccr_reg, ccr);
    }
}

/// Enable or disable PWM / capture I/O pins and multifunction.
pub unsafe fn pwm_set_io(ch: u8, enable: bool) {
    let idx = (ch & 0x07) as u32;

    if enable {
        if ch & 0x10 != 0 {
            // Capture input
            reg_write32(REG_CAPENR, reg_read32(REG_CAPENR) | (1 << idx));
        } else {
            // PWM output
            reg_write32(REG_POE, reg_read32(REG_POE) | (1 << idx));
        }
        // Set GPD pin to PWM function (MF = 0x2)
        let mf_mask = 0x3u32 << (idx * 2);
        reg_write32(
            REG_GPDFUN,
            (reg_read32(REG_GPDFUN) & !mf_mask) | (0x2 << (idx * 2)),
        );
    } else {
        if ch & 0x10 != 0 {
            reg_write32(REG_CAPENR, reg_read32(REG_CAPENR) & !(1 << idx));
        } else {
            reg_write32(REG_POE, reg_read32(REG_POE) & !(1 << idx));
        }
        // Clear GPD pin multifunction (back to GPIO)
        let mf_mask = 0x3u32 << (idx * 2);
        reg_write32(REG_GPDFUN, reg_read32(REG_GPDFUN) & !mf_mask);
    }
}

// ============================================================================
// Counter
// ============================================================================

/// Set the PWM timer counter value (CNR register).
pub unsafe fn pwm_set_counter(ch: u8, counter: u16) {
    reg_write32(cnr_reg(ch & 0x07), counter as u32);
}

/// Read the current PWM timer counter value (PDR register).
pub unsafe fn pwm_get_counter(ch: u8) -> u16 {
    (reg_read32(pdr_reg(ch & 0x07)) & 0xFFFF) as u16
}

// ============================================================================
// Dead Zone
// ============================================================================

/// Configure dead-zone for a paired channel.
///
/// Ch0+Ch1 share dead-zone generator 0; Ch2+Ch3 share generator 1.
/// `length` is 0–255.
pub unsafe fn pwm_dead_zone(ch: u8, length: u8, enable: bool) {
    if ch & 0x02 != 0 {
        // Channels 2/3 — dead-zone 1
        let mut ppr = reg_read32(REG_PPR);
        ppr &= !DZI1;
        ppr |= (length as u32) << 24;
        reg_write32(REG_PPR, ppr);

        let mut pcr = reg_read32(REG_PCR);
        if enable {
            pcr |= DZEN1;
        } else {
            pcr &= !DZEN1;
        }
        reg_write32(REG_PCR, pcr);
    } else {
        // Channels 0/1 — dead-zone 0
        let mut ppr = reg_read32(REG_PPR);
        ppr &= !DZI0;
        ppr |= (length as u32) << 16;
        reg_write32(REG_PPR, ppr);

        let mut pcr = reg_read32(REG_PCR);
        if enable {
            pcr |= DZEN0;
        } else {
            pcr &= !DZEN0;
        }
        reg_write32(REG_PCR, pcr);
    }
}

// ============================================================================
// Frequency / Duty Configuration
// ============================================================================

/// Configure the PWM timer clock (frequency, duty, mode, inverter).
///
/// When `cfg.frequency != 0`, the prescaler and clock divider are computed
/// automatically to achieve the requested frequency.  When `cfg.frequency == 0`,
/// the manual `prescale`, `clock_div`, and `duty` fields are used directly.
///
/// Returns the actual achieved frequency.
pub unsafe fn pwm_set_timer_clk(ch: u8, cfg: &PwmConfig) -> f32 {
    let idx = (ch & 0x07) as usize;
    let apb_hz = crate::clock::get_apb_clock() * 1000;

    // --- Inverter ---
    if ch & 0x10 != 0 {
        // Capture channel: inverter in CCR
        let ccr_reg = if ch & 0x02 != 0 { REG_CCR1 } else { REG_CCR0 };
        let inv_bit = 1u32 << ((ch as u32 & 0x01) << 4);
        let mut ccr = reg_read32(ccr_reg);
        if cfg.inverter {
            ccr |= inv_bit;
        } else {
            ccr &= !inv_bit;
        }
        reg_write32(ccr_reg, ccr);
    } else {
        // PWM timer: inverter in PCR
        let mut pcr = reg_read32(REG_PCR);
        if cfg.inverter {
            pcr |= 1 << CH_INV_SHIFT[idx];
        } else {
            pcr &= !(1 << CH_INV_SHIFT[idx]);
        }
        reg_write32(REG_PCR, pcr);
    }

    // --- Mode ---
    {
        let mut pcr = reg_read32(REG_PCR);
        if matches!(cfg.mode, PwmMode::Toggle) {
            pcr |= 1 << CH_MOD_SHIFT[idx];
        } else {
            pcr &= !(1 << CH_MOD_SHIFT[idx]);
        }
        reg_write32(REG_PCR, pcr);
    }

    // --- Frequency / divider setup ---
    if cfg.frequency == 0.0 {
        // Manual mode
        let div_code = match cfg.clock_div {
            PwmClkDiv::Div1 => PWM_CSR_DIV1,
            PwmClkDiv::Div2 => PWM_CSR_DIV2,
            PwmClkDiv::Div4 => PWM_CSR_DIV4,
            PwmClkDiv::Div8 => PWM_CSR_DIV8,
            PwmClkDiv::Div16 => PWM_CSR_DIV16,
        };

        // Prescaler
        if ch & 0x02 != 0 {
            let mut ppr = reg_read32(REG_PPR);
            ppr &= !CP1;
            ppr |= (cfg.prescale.wrapping_sub(1) as u32) << 8;
            reg_write32(REG_PPR, ppr);
        } else {
            let mut ppr = reg_read32(REG_PPR);
            ppr &= !CP0;
            ppr |= cfg.prescale.wrapping_sub(1) as u32;
            reg_write32(REG_PPR, ppr);
        }

        // Clock divider in CSR
        let shift = (idx as u32) * 4;
        let mut csr = reg_read32(REG_PWM_CSR);
        csr &= !(0x7 << shift);
        csr |= (div_code & 0x7) << shift;
        reg_write32(REG_PWM_CSR, csr);

        // CNR = duty - 1
        reg_write32(cnr_reg(ch & 0x07), cfg.duty.wrapping_sub(1));

        // CMR = duty * ratio% / 100 - 1
        let cmp = if cfg.duty > 0 {
            (cfg.duty * cfg.high_pulse_ratio as u32 / 100).wrapping_sub(1)
        } else {
            0
        };
        reg_write32(cmr_reg(ch & 0x07), cmp);

        apb_hz as f32 / cfg.prescale as f32 / cfg.clock_div as u32 as f32 / cfg.duty as f32
    } else {
        // Auto-compute mode
        let f_total = apb_hz as f32 / cfg.frequency;

        if f_total > 0x1000_0000u32 as f32 {
            return 0.0;
        }

        let (pre, div_code, div_val, cnr_val) =
            compute_dividers(apb_hz, cfg.frequency, f_total);

        // Prescaler
        if ch & 0x02 != 0 {
            let mut ppr = reg_read32(REG_PPR);
            ppr &= !CP1;
            ppr |= ((pre - 1) as u32) << 8;
            reg_write32(REG_PPR, ppr);
        } else {
            let mut ppr = reg_read32(REG_PPR);
            ppr &= !CP0;
            ppr |= (pre - 1) as u32;
            reg_write32(REG_PPR, ppr);
        }

        // Clock select
        let shift = (idx as u32) * 4;
        let mut csr = reg_read32(REG_PWM_CSR);
        csr &= !(0x7 << shift);
        csr |= (div_code & 0x7) << shift;
        reg_write32(REG_PWM_CSR, csr);

        if ch & 0x10 != 0 && cfg.duty != 0 {
            reg_write32(cnr_reg(ch & 0x07), cfg.duty.wrapping_sub(1));
            reg_write32(
                cmr_reg(ch & 0x07),
                (cfg.duty * cfg.high_pulse_ratio as u32 / 100).wrapping_sub(1),
            );
        } else {
            let cnr = cnr_val.wrapping_sub(1) as u32;
            reg_write32(cnr_reg(ch & 0x07), cnr);
            let cmr = (cnr_val as u32 * cfg.high_pulse_ratio as u32 / 100).wrapping_sub(1);
            reg_write32(cmr_reg(ch & 0x07), cmr);
        }

        (apb_hz as f32 / pre as f32 / div_val as f32) / cnr_val as f32
    }
}

/// Compute prescaler and clock divider to achieve the target frequency.
fn compute_dividers(_apb_hz: u32, _target_hz: f32, mut f_total: f32) -> (u16, u32, u16, u16) {
    let mut pre: u16;
    let mut div_val: u16 = 1;
    if f_total < 0x20000u32 as f32 {
        pre = 2;
    } else {
        pre = (f_total / 65536.0) as u16;
        if (f_total / pre as f32) > 65536.0 {
            pre += 1;
        }

        if pre > 256 {
            pre = 256;
            f_total /= pre as f32;
            div_val = (f_total / 65536.0) as u16;
            if (f_total / div_val as f32) > 65536.0 {
                div_val += 1;
            }

            let mut i: u8 = 0;
            loop {
                if (1u16 << i) > div_val {
                    break;
                }
                i += 1;
            }
            div_val = 1 << (i.wrapping_sub(1));
            if div_val > 16 {
                div_val = 16;
            }
            f_total *= pre as f32;
        }
    }

    let cnr = (f_total / pre as f32 / div_val as f32) as u16;

    let div_code = match div_val {
        1 => PWM_CSR_DIV1,
        2 => PWM_CSR_DIV2,
        4 => PWM_CSR_DIV4,
        8 => PWM_CSR_DIV8,
        16 => PWM_CSR_DIV16,
        _ => PWM_CSR_DIV1,
    };

    (pre, div_code, div_val, cnr)
}

// ============================================================================
// Interrupt Control
// ============================================================================

/// Enable PWM timer or capture interrupt.
///
/// For PWM timer channels, `int_type` is ignored.
/// For capture channels, `int_type` selects rising, falling, or both.
pub unsafe fn pwm_int_enable(ch: u8, int_type: CapIntType) {
    if ch & 0x10 != 0 {
        // Capture interrupt
        let ccr_reg = if ch & 0x02 != 0 { REG_CCR1 } else { REG_CCR0 };
        let base_shift = ((ch as u32 & 0x01) << 4) + 1;
        let mask = 0x06u32 << base_shift;
        let val = (int_type as u32 & 0x03) << base_shift;
        let mut ccr = reg_read32(ccr_reg);
        ccr = (ccr & !mask) | val;
        reg_write32(ccr_reg, ccr);
    } else {
        // Timer interrupt: set PIER bit
        reg_write32(REG_PIER, reg_read32(REG_PIER) | (1 << (ch & 0x03)));
    }
}

/// Disable PWM timer or capture interrupt.
pub unsafe fn pwm_int_disable(ch: u8, int_type: CapIntType) {
    if ch & 0x10 != 0 {
        let ccr_reg = if ch & 0x02 != 0 { REG_CCR1 } else { REG_CCR0 };
        let base_shift = ((ch as u32 & 0x01) << 4) + 1;
        let mask = (int_type as u32 & 0x03) << base_shift;
        reg_write32(ccr_reg, reg_read32(ccr_reg) & !mask);
    } else {
        let bit = 1 << (ch & 0x03);
        reg_write32(REG_PIER, reg_read32(REG_PIER) & !bit);
        reg_write32(REG_PIIR, reg_read32(REG_PIIR) & !bit);
    }

    // Clear callbacks if all interrupts disabled
    if (reg_read32(REG_PIER) & 0xF) == 0
        && (reg_read32(REG_CCR0) & 0x0006_0006) == 0
        && (reg_read32(REG_CCR1) & 0x0006_0006) == 0
    {
        crate::aic::aic_disable_interrupt(IRQ_PWM);
    }

    // Clear callback
    let idx = (ch & 0x03) as usize;
    if ch & 0x10 != 0 {
        CALLBACKS.cap[idx] = None;
    } else {
        CALLBACKS.pwm[idx] = None;
    }
}

/// Clear PWM interrupt flag.
pub unsafe fn pwm_int_clear(ch: u8) {
    if ch & 0x10 != 0 {
        let ccr_reg = if ch & 0x02 != 0 { REG_CCR1 } else { REG_CCR0 };
        let bit = 1u32 << (((ch as u32 & 0x01) << 4) + 4);
        reg_write32(ccr_reg, reg_read32(ccr_reg) & !bit);
    } else {
        reg_write32(REG_PIIR, reg_read32(REG_PIIR) & !(1 << (ch & 0x03)));
    }
}

/// Get PWM timer / capture interrupt flag status.
pub unsafe fn pwm_get_int_flag(ch: u8) -> bool {
    if ch & 0x10 != 0 {
        let ccr_reg = if ch & 0x02 != 0 { REG_CCR1 } else { REG_CCR0 };
        let bit = 1u32 << (((ch as u32 & 0x01) << 4) + 4);
        reg_read32(ccr_reg) & bit != 0
    } else {
        reg_read32(REG_PIIR) & (1 << (ch & 0x03)) != 0
    }
}

// ============================================================================
// Capture
// ============================================================================

/// Get capture interrupt status (rising or falling flag).
pub unsafe fn pwm_get_capture_int_status(cap: u8, flag: CapIntFlag) -> bool {
    let ccr_reg = if cap & 0x02 != 0 { REG_CCR1 } else { REG_CCR0 };
    let bit = 1u32 << (((cap as u32 & 0x01) << 4) + flag as u32);
    reg_read32(ccr_reg) & bit != 0
}

/// Clear capture interrupt status.
pub unsafe fn pwm_clear_capture_int_status(cap: u8, flag: CapIntFlag) {
    let ccr_reg = if cap & 0x02 != 0 { REG_CCR1 } else { REG_CCR0 };
    let bit = 1u32 << (((cap as u32 & 0x01) << 4) + flag as u32);
    reg_write32(ccr_reg, reg_read32(ccr_reg) | bit);
}

/// Read capture rising-edge latch value.
pub unsafe fn pwm_get_rising_counter(cap: u8) -> u16 {
    (reg_read32(crlr_reg(cap & 0x07)) & 0xFFFF) as u16
}

/// Read capture falling-edge latch value.
pub unsafe fn pwm_get_falling_counter(cap: u8) -> u16 {
    (reg_read32(cflr_reg(cap & 0x07)) & 0xFFFF) as u16
}

// ============================================================================
// Callback Installation
// ============================================================================

/// Install a callback for a PWM timer or capture interrupt.
///
/// Returns the previous callback (if any).
pub unsafe fn pwm_install_callback(
    ch: u8,
    callback: PwmCallback,
) -> Option<PwmCallback> {
    let idx = (ch & 0x03) as usize;
    let old = if ch & 0x10 != 0 {
        CALLBACKS.cap[idx].replace(callback)
    } else {
        CALLBACKS.pwm[idx].replace(callback)
    };

    // Install PWM ISR in AIC
    crate::aic::aic_install_isr(
        crate::aic::IntLevel::Irq1,
        IRQ_PWM,
        pwm_isr as PwmCallback,
    );
    crate::aic::aic_set_local_interrupt(crate::aic::LocalIntState::EnableIrq);
    crate::aic::aic_enable_interrupt(IRQ_PWM);

    old
}

// ============================================================================
// PWM ISR
// ============================================================================

/// Shared PWM interrupt service routine.
unsafe extern "C" fn pwm_isr() {
    let piir = reg_read32(REG_PIIR);
    let ccr0 = reg_read32(REG_CCR0);
    let ccr1 = reg_read32(REG_CCR1);

    // Timer interrupts
    if piir & PIIR0 != 0 {
        reg_write32(REG_PIIR, PIIR0);
        if let Some(cb) = CALLBACKS.pwm[0] {
            cb();
        }
    }
    if piir & PIIR1 != 0 {
        reg_write32(REG_PIIR, PIIR1);
        if let Some(cb) = CALLBACKS.pwm[1] {
            cb();
        }
    }
    if piir & PIIR2 != 0 {
        reg_write32(REG_PIIR, PIIR2);
        if let Some(cb) = CALLBACKS.pwm[2] {
            cb();
        }
    }
    if piir & PIIR3 != 0 {
        reg_write32(REG_PIIR, PIIR3);
        if let Some(cb) = CALLBACKS.pwm[3] {
            cb();
        }
    }

    // Capture 0 interrupt
    if ccr0 & CIIR0 != 0 {
        reg_write32(REG_CCR0, reg_read32(REG_CCR0) & !CIIR0);
        if let Some(cb) = CALLBACKS.cap[0] {
            cb();
        }
    }
    // Capture 1 interrupt
    if ccr0 & CIIR1 != 0 {
        reg_write32(REG_CCR0, reg_read32(REG_CCR0) & !CIIR1);
        if let Some(cb) = CALLBACKS.cap[1] {
            cb();
        }
    }
    // Capture 2 interrupt
    if ccr1 & CIIR2 != 0 {
        reg_write32(REG_CCR1, reg_read32(REG_CCR1) & !CIIR2);
        if let Some(cb) = CALLBACKS.cap[2] {
            cb();
        }
    }
    // Capture 3 interrupt
    if ccr1 & CIIR3 != 0 {
        reg_write32(REG_CCR1, reg_read32(REG_CCR1) & !CIIR3);
        if let Some(cb) = CALLBACKS.cap[3] {
            cb();
        }
    }
}
