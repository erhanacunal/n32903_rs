//! LCD backlight for Nextion N32903 boards.
//!
//! Follows the Nextion firmware's `dim` handler (`SetPWMDutyCycle`).  The
//! board variant is detected from GPB0 at power-on (see [`detect_variant`]):
//!
//! | Variant | GPB0 at boot | Backlight pin | Polarity    | Enable       |
//! |---------|--------------|---------------|-------------|--------------|
//! | `A`     | high (float) | GPD1 / PWM1   | active high | –            |
//! | `B`     | pulled low   | GPD0 / PWM0   | active low  | GPB0 high=on |
//!
//! 0 % and 100 % drive the pin as a plain GPIO level; anything in between
//! hands the pin to the PWM (pin function 2).
//!
//! **GPD0–GPD4 are also the JTAG pins.**  Driving the backlight pin drops an
//! attached debugger.
//!
//! ```rust,ignore
//! let variant = unsafe { backlight::detect_variant() };
//! let mut bl = unsafe { Backlight::init(variant, backlight::DEFAULT_FREQ_HZ) };
//! unsafe { bl.set_brightness(50) };
//! ```

use crate::clock;
use crate::registers::*;

/// Default backlight PWM frequency.
pub const DEFAULT_FREQ_HZ: u32 = 1000;

/// Board variant, selected by GPB0 at power-on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BoardVariant {
    /// GPB0 floats high: backlight on GPD1 / PWM1, active high
    A,
    /// GPB0 pulled low: backlight on GPD0 / PWM0, active low, GPB0 = enable
    B,
}

/// Clock select value for "input clock / 1" in REG_PWM_CSR.
const CSR_DIV1: u32 = 4;

/// PCR control bits of a channel, relative to its field (ch × 8).
const PCR_EN: u32 = 1 << 0;
const PCR_INV: u32 = 1 << 2;
const PCR_AUTO_RELOAD: u32 = 1 << 3;

/// GPB0: variant strap, and backlight enable on variant B.
const GPB0: u32 = 1 << 0;

/// Detect the board variant like the firmware does: GPB0 as input with
/// pull-up, sampled twice; low both times means variant B.
///
/// # Safety
/// Reconfigures GPB0.  Call before [`Backlight::init`], which drives GPB0
/// on variant B.
pub unsafe fn detect_variant() -> BoardVariant {
    reg_write32(REG_AHBCLK, reg_read32(REG_AHBCLK) | 0x2);
    reg_write32(REG_GPBFUN, reg_read32(REG_GPBFUN) & !0x3);
    reg_write32(REG_GPIOB_PUEN, reg_read32(REG_GPIOB_PUEN) | GPB0);
    reg_write32(REG_GPIOB_OMD, reg_read32(REG_GPIOB_OMD) & !GPB0);
    reg_write32(REG_GPIOB_DOUT, reg_read32(REG_GPIOB_DOUT) | GPB0);

    let mut low = 0;
    for _ in 0..2 {
        spin(100_000);
        if reg_read32(REG_GPIOB_PIN) & GPB0 == 0 {
            low += 1;
        }
    }
    if low == 2 { BoardVariant::B } else { BoardVariant::A }
}

/// The LCD backlight.
pub struct Backlight {
    variant: BoardVariant,
    ch: u32,
    cnr: u32,
    percent: u8,
}

impl Backlight {
    /// Set up the backlight PWM for `variant` at `freq_hz`.  Starts off (0 %).
    ///
    /// # Safety
    /// Takes over PWM channel 0 or 1 (and the ch0/1 prescaler), GPD0 or
    /// GPD1 and, on variant B, GPB0.  Disconnects JTAG.
    pub unsafe fn init(variant: BoardVariant, freq_hz: u32) -> Self {
        let ch = match variant {
            BoardVariant::A => 1,
            BoardVariant::B => 0,
        };
        let mut bl = Backlight { variant, ch, cnr: 0, percent: 0 };
        let pin = bl.pin();

        // Pin as GPIO output, off level
        reg_write32(REG_GPDFUN, reg_read32(REG_GPDFUN) & !(0x3 << (ch * 2)));
        bl.set_pin_level(false);
        reg_write32(REG_GPIOD_OMD, reg_read32(REG_GPIOD_OMD) | pin);
        if variant == BoardVariant::B {
            reg_write32(REG_GPBFUN, reg_read32(REG_GPBFUN) & !0x3);
            reg_write32(REG_GPIOB_DOUT, reg_read32(REG_GPIOB_DOUT) & !GPB0);
            reg_write32(REG_GPIOB_OMD, reg_read32(REG_GPIOB_OMD) | GPB0);
        }

        reg_write32(REG_APBCLK, reg_read32(REG_APBCLK) | PWM_CKE);

        // Not inverted, auto-reload, running.  The mode must be set before
        // CNR is written: a CNR write in the reset (one-shot) state is lost
        // and the counter stays at 0.
        let pcr_shift = ch * 8;
        reg_write32(
            REG_PCR,
            (reg_read32(REG_PCR) & !((PCR_EN | PCR_INV | PCR_AUTO_RELOAD) << pcr_shift))
                | ((PCR_EN | PCR_AUTO_RELOAD) << pcr_shift),
        );

        // Clock = PWM input / prescaler / 1
        let csr_shift = ch * 4;
        reg_write32(
            REG_PWM_CSR,
            (reg_read32(REG_PWM_CSR) & !(0x7 << csr_shift)) | (CSR_DIV1 << csr_shift),
        );
        bl.set_frequency(freq_hz);

        // Output enabled; the pin only follows the PWM once its function is
        // switched (1–99 %).
        reg_write32(REG_POE, reg_read32(REG_POE) | pin);

        bl.set_brightness(0);
        bl
    }

    /// Set the PWM frequency.  Picks the smallest prescaler that keeps the
    /// period counter within 16 bits; the brightness is kept.
    ///
    /// # Safety
    /// Changes the prescaler shared by PWM channels 0 and 1.
    pub unsafe fn set_frequency(&mut self, freq_hz: u32) {
        let freq_hz = freq_hz.max(1);
        let pwm_clk_hz = clock::get_apb_clock() * 1000;

        // Prescaler register value 0 stops the clock, so divide by >= 2
        let ticks = pwm_clk_hz / freq_hz;
        let prescale = ((ticks + 0xFFFF) / 0x1_0000).clamp(2, 256);
        let cnr = (ticks / prescale).clamp(2, 0x1_0000) - 1;

        // CP0 (bits 7:0) clocks channels 0 and 1
        reg_write32(REG_PPR, (reg_read32(REG_PPR) & !0xFF) | (prescale - 1));
        reg_write32(REG_CNR0 + self.ch * 12, cnr);
        self.cnr = cnr;

        self.set_brightness(self.percent);
    }

    /// Set the brightness in percent (values above 100 are clamped).
    ///
    /// # Safety
    /// Writes the PWM compare register and the pin configuration.
    pub unsafe fn set_brightness(&mut self, percent: u8) {
        let percent = percent.min(100);
        let fun_shift = self.ch * 2;
        let gpio_mode = reg_read32(REG_GPDFUN) & !(0x3 << fun_shift);

        match percent {
            0 | 100 => {
                let on = percent == 100;
                reg_write32(REG_GPDFUN, gpio_mode);
                self.set_pin_level(on);
                self.set_enable(on);
            }
            _ => {
                // Output is high for CMR / CNR of the period
                let high = match self.variant {
                    BoardVariant::A => percent as u32,
                    BoardVariant::B => 100 - percent as u32,
                };
                reg_write32(REG_CMR0 + self.ch * 12, self.cnr * high / 100);
                reg_write32(REG_GPDFUN, gpio_mode | (0x2 << fun_shift));
                self.set_enable(true);
            }
        }
        self.percent = percent;
    }

    /// Current brightness in percent.
    pub fn brightness(&self) -> u8 {
        self.percent
    }

    /// Detected board variant.
    pub fn variant(&self) -> BoardVariant {
        self.variant
    }

    /// PWM channel in use (0 or 1); the pin is GPD with the same number.
    pub fn channel(&self) -> u32 {
        self.ch
    }

    /// Period counter value (CNR); the brightness resolution is CNR + 1 steps.
    pub fn period_counts(&self) -> u32 {
        self.cnr
    }

    fn pin(&self) -> u32 {
        1 << self.ch
    }

    /// Drive the backlight pin (GPIO mode) to its on or off level.
    unsafe fn set_pin_level(&self, on: bool) {
        let high = match self.variant {
            BoardVariant::A => on,
            BoardVariant::B => !on,
        };
        let dout = reg_read32(REG_GPIOD_DOUT);
        reg_write32(
            REG_GPIOD_DOUT,
            if high { dout | self.pin() } else { dout & !self.pin() },
        );
    }

    /// Variant B: GPB0 high enables the backlight.
    unsafe fn set_enable(&self, on: bool) {
        if self.variant == BoardVariant::B {
            let dout = reg_read32(REG_GPIOB_DOUT);
            reg_write32(REG_GPIOB_DOUT, if on { dout | GPB0 } else { dout & !GPB0 });
        }
    }
}

/// Busy-wait `n` loop iterations (not optimised away).
fn spin(n: u32) {
    unsafe {
        core::arch::asm!(
            "1:",
            "subs {0}, {0}, #1",
            "bne 1b",
            inout(reg) n => _,
            options(nomem, nostack),
        );
    }
}
