//! 4-wire resistive touch panel on the on-chip ADC.
//!
//! The N32903 ADC has a touch screen controller (XP/XM/YP/YM analog pins),
//! so resistive Nextion boards have no touch IC.  The sequence follows the
//! Nextion firmware (`ReadCalibratedTouchPosition` and friends) and the BSP
//! `libadc.c`:
//!
//! - ADC clock from the 12 MHz crystal / 2 (CLKDIV3)
//! - idle in "waiting for trigger" mode with the pull-up on; the pen state
//!   is `ADC_TSC` bit 0
//! - on touch: auto mode conversions (X and Y per conversion), several
//!   samples, median, spread check; then back to waiting mode
//! - the ADC is reset before every mode change (BSP: it hangs otherwise)
//!
//! The firmware swaps the axes: screen X comes from `ADC_YDATA` and screen
//! Y from `ADC_XDATA`.  [`DEFAULT_CALIBRATION`] does the same; the raw
//! ranges are guesses until measured — print [`TouchPoint`]'s `raw_x/raw_y`
//! while touching the corners and put the values into a [`Calibration`].
//!
//! ```rust,ignore
//! let mut touch = unsafe { Touch::init(800, 480, touch::DEFAULT_CALIBRATION) };
//! if let Some(p) = unsafe { touch.read() } {
//!     // p.x, p.y
//! }
//! ```

use crate::registers::*;

// ADC registers (W55FA93_reg.h)
const REG_ADC_CON: u32 = ADC_BA + 0x00;
const REG_ADC_TSC: u32 = ADC_BA + 0x04;
const REG_ADC_DLY: u32 = ADC_BA + 0x08;
const REG_ADC_XDATA: u32 = ADC_BA + 0x0C;
const REG_ADC_YDATA: u32 = ADC_BA + 0x10;

// ADC_CON bits
const WT_INT_EN: u32 = 1 << 23;
const WT_INT: u32 = 1 << 20;
const LVD_INT: u32 = 1 << 19;
const ADC_INT: u32 = 1 << 18;
const ADC_EN: u32 = 1 << 17;
const ADC_CONV: u32 = 1 << 13;
const TSC_MODE_SHIFT: u32 = 14;

// ADC_TSC bits
const TSC_PU_EN: u32 = 1 << 3;
const TSC_UD: u32 = 1 << 0; // pen down (waiting-for-trigger mode)

// Clock / reset
const ADC_CKE: u32 = 1 << 0; // APBCLK
const ADCRST: u32 = 1 << 14; // APBIPRST

/// Touch screen conversion modes (ADC_CON bits 15:14).
#[derive(Clone, Copy)]
enum Mode {
    Auto = 2,
    WaitTrigger = 3,
}

/// CPU clock (MHz) the `spin` counts below were tuned at.
const SPIN_REF_MHZ: u32 = 96;

/// Running CPU clock in MHz, read in [`Touch::init`]: the settle delays are
/// busy loops, so they scale with the clock the loader chose.
static mut CPU_MHZ: u32 = SPIN_REF_MHZ;

/// Busy-wait `n` loop iterations as tuned at [`SPIN_REF_MHZ`].
fn settle(n: u32) {
    spin(n * unsafe { CPU_MHZ } / SPIN_REF_MHZ);
}

/// Conversion delay (ADC clocks) used by the firmware.
const DELAY: u32 = 0x80;

/// Samples per reading; the middle half is averaged.
const SAMPLES: usize = 8;

/// Largest spread (raw units) between the kept samples of one axis.
const MAX_SPREAD: u16 = 40;

/// Raw ADC range per screen axis, and whether X/Y are swapped.
///
/// `x_min`/`x_max` are the raw values at the left/right edge, `y_min`/
/// `y_max` at the top/bottom edge.  A "min" larger than its "max" flips
/// the axis.
#[derive(Clone, Copy, Debug)]
pub struct Calibration {
    pub x_min: u16,
    pub x_max: u16,
    pub y_min: u16,
    pub y_max: u16,
    /// Screen X from ADC_YDATA and screen Y from ADC_XDATA (firmware default)
    pub swap_xy: bool,
}

/// Uncalibrated: full 10-bit range, axes swapped like the firmware.
pub const DEFAULT_CALIBRATION: Calibration = Calibration {
    x_min: 0,
    x_max: 1023,
    y_min: 0,
    y_max: 1023,
    swap_xy: true,
};

/// One touch reading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TouchPoint {
    /// Screen coordinates (after calibration, clamped to the screen)
    pub x: u16,
    pub y: u16,
    /// Raw ADC values for the screen X and Y axes (after the swap)
    pub raw_x: u16,
    pub raw_y: u16,
}

/// The touch panel.
pub struct Touch {
    width: u16,
    height: u16,
    cal: Calibration,
}

impl Touch {
    /// Enable the ADC and put it in waiting-for-trigger mode.
    ///
    /// # Safety
    /// Takes over the ADC (no other ADC channel can be used meanwhile).
    pub unsafe fn init(width: u16, height: u16, cal: Calibration) -> Self {
        CPU_MHZ = (crate::clock::get_cpu_clock() / 1000).max(1);
        reg_write32(REG_APBCLK, reg_read32(REG_APBCLK) | ADC_CKE);
        // ADC clock: XIN (12 MHz), N0 = 0, N1 = 1 -> 6 MHz
        reg_write32(
            REG_CLKDIV3,
            (reg_read32(REG_CLKDIV3) & !0xFF1F_0000) | (1 << 24),
        );
        set_mode(Mode::WaitTrigger, true);
        Touch { width, height, cal }
    }

    pub fn calibration(&self) -> Calibration {
        self.cal
    }

    pub fn set_calibration(&mut self, cal: Calibration) {
        self.cal = cal;
    }

    /// Whether the panel is pressed right now.
    pub fn is_pressed(&self) -> bool {
        unsafe { reg_read32(REG_ADC_TSC) & TSC_UD != 0 }
    }

    /// Raise `IRQ_ADC` on the next pen-down (waiting-for-trigger interrupt).
    ///
    /// There is no pen-up interrupt: after a pen-down the caller samples
    /// with [`read`](Self::read) until the pen is up, then arms again.
    /// `read` resets the ADC, which also disarms the interrupt.
    ///
    /// # Safety
    /// Uses the ADC; must not run concurrently with other ADC use.
    pub unsafe fn arm_pen_irq(&self) {
        // Status bits are write-1-to-clear: ack a stale WT_INT, keep the rest.
        let con = reg_read32(REG_ADC_CON) & !(WT_INT | ADC_INT | LVD_INT);
        reg_write32(REG_ADC_CON, con | WT_INT);
        reg_write32(REG_ADC_CON, con | WT_INT_EN);
    }

    /// Disable and acknowledge the pen-down interrupt (call from the ISR).
    ///
    /// # Safety
    /// Uses the ADC; must not run concurrently with other ADC use.
    pub unsafe fn disarm_pen_irq(&self) {
        let con = reg_read32(REG_ADC_CON) & !(WT_INT_EN | WT_INT | ADC_INT | LVD_INT);
        reg_write32(REG_ADC_CON, con | WT_INT);
    }

    /// Read the touch position.  `None` when not pressed, or when the
    /// reading is unstable (pen moving, or released during sampling).
    ///
    /// # Safety
    /// Uses the ADC; must not run concurrently with other ADC use.
    pub unsafe fn read(&mut self) -> Option<TouchPoint> {
        if !self.is_pressed() {
            return None;
        }

        let mut xs = [0u16; SAMPLES];
        let mut ys = [0u16; SAMPLES];
        set_mode(Mode::Auto, false);
        let mut ok = true;
        for i in 0..SAMPLES {
            match convert() {
                Some((x, y)) => {
                    xs[i] = x;
                    ys[i] = y;
                }
                None => {
                    ok = false;
                    break;
                }
            }
        }

        // Back to waiting mode; the pen must still be down, otherwise the
        // last samples were taken while it was lifting off.
        set_mode(Mode::WaitTrigger, true);
        settle(2_000);
        if !ok || !self.is_pressed() {
            return None;
        }

        let adc_x = median_mean(&mut xs)?;
        let adc_y = median_mean(&mut ys)?;
        let (raw_x, raw_y) = if self.cal.swap_xy { (adc_y, adc_x) } else { (adc_x, adc_y) };

        Some(TouchPoint {
            x: scale(raw_x, self.cal.x_min, self.cal.x_max, self.width),
            y: scale(raw_y, self.cal.y_min, self.cal.y_max, self.height),
            raw_x,
            raw_y,
        })
    }
}

/// Reset the ADC and switch the touch screen controller to `mode`.
unsafe fn set_mode(mode: Mode, pull_up: bool) {
    reg_write32(REG_APBIPRST, reg_read32(REG_APBIPRST) | ADCRST);
    reg_write32(REG_APBIPRST, reg_read32(REG_APBIPRST) & !ADCRST);

    reg_write32(REG_ADC_CON, (mode as u32) << TSC_MODE_SHIFT);
    reg_write32(REG_ADC_DLY, DELAY);
    // 4-wire type (0), optional pull-up, no MAV filter
    reg_write32(REG_ADC_TSC, if pull_up { TSC_PU_EN } else { 0 });
    reg_write32(REG_ADC_CON, reg_read32(REG_ADC_CON) | ADC_EN);
    settle(500);
}

/// One auto-mode conversion: (ADC_XDATA, ADC_YDATA), 10 bits each.
unsafe fn convert() -> Option<(u16, u16)> {
    reg_write32(REG_ADC_CON, reg_read32(REG_ADC_CON) | ADC_CONV);
    let mut timeout = 100_000;
    while reg_read32(REG_ADC_CON) & ADC_INT == 0 {
        timeout -= 1;
        if timeout == 0 {
            return None;
        }
    }
    let x = (reg_read32(REG_ADC_XDATA) & 0x3FF) as u16;
    let y = (reg_read32(REG_ADC_YDATA) & 0x3FF) as u16;
    // Flags are cleared by writing 1 (BSP DrvADC_PollingADC)
    reg_write32(
        REG_ADC_CON,
        (reg_read32(REG_ADC_CON) & !(WT_INT | LVD_INT)) | WT_INT | ADC_INT,
    );
    Some((x, y))
}

/// Sort, drop the lowest and highest quarter, average the rest.  `None`
/// when the kept samples spread more than `MAX_SPREAD`.
fn median_mean(s: &mut [u16; SAMPLES]) -> Option<u16> {
    s.sort_unstable();
    let kept = &s[SAMPLES / 4..SAMPLES - SAMPLES / 4];
    if kept[kept.len() - 1] - kept[0] > MAX_SPREAD {
        return None;
    }
    let sum: u32 = kept.iter().map(|&v| v as u32).sum();
    Some((sum / kept.len() as u32) as u16)
}

/// Map `raw` from [lo, hi] (either order) to [0, size - 1], clamped.
fn scale(raw: u16, lo: u16, hi: u16, size: u16) -> u16 {
    let max = size.saturating_sub(1) as i32;
    if lo == hi {
        return 0;
    }
    let v = (raw as i32 - lo as i32) * max / (hi as i32 - lo as i32);
    v.clamp(0, max) as u16
}

/// Busy-wait `n` loop iterations.
fn spin(n: u32) {
    if n == 0 {
        return;
    }
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
