//! GPIO driver for N32903 (W55FA93)
//! Ported from libgpio.c / W55FA93_GPIO.h
//!
//! Supports Port A–E with direction, output, input, pull-up,
//! multifunction pin control, interrupt configuration, and debounce.
//!
//! # Safety
//!
//! All functions in this module are `unsafe` because they read/write
//! hardware registers directly.

use crate::registers::*;

// ============================================================================
// Public Types
// ============================================================================

/// GPIO port (A–E)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GpioPort {
    PortA = 0,
    PortB = 1,
    PortC = 2,
    PortD = 3,
    PortE = 4,
}

impl GpioPort {
    /// Number of pins on this port
    pub fn pin_count(self) -> u8 {
        match self {
            GpioPort::PortA | GpioPort::PortE => 12,
            GpioPort::PortB | GpioPort::PortC | GpioPort::PortD => 16,
        }
    }

    /// Bit mask for all valid pins on this port
    pub fn pin_mask(self) -> u16 {
        match self {
            GpioPort::PortA | GpioPort::PortE => 0x0FFF,
            GpioPort::PortB | GpioPort::PortC | GpioPort::PortD => 0xFFFF,
        }
    }

    /// Register base offset within GPIO_BA
    fn reg_offset(self) -> u32 {
        (self as u32) * 0x10
    }

    /// Multifunction register address for this port
    fn mfp_reg(self) -> u32 {
        match self {
            GpioPort::PortA => REG_GPAFUN,
            GpioPort::PortB => REG_GPBFUN,
            GpioPort::PortC => REG_GPCFUN,
            GpioPort::PortD => REG_GPDFUN,
            GpioPort::PortE => REG_GPEFUN,
        }
    }

    /// OMD (Output Mode) register for this port
    fn omd_reg(self) -> u32 {
        GPIO_BA + self.reg_offset()
    }

    /// PUEN (Pull-Up Enable) register
    fn puen_reg(self) -> u32 {
        GPIO_BA + self.reg_offset() + 0x04
    }

    /// DOUT (Data Output) register
    fn dout_reg(self) -> u32 {
        GPIO_BA + self.reg_offset() + 0x08
    }

    /// PIN (Input Value) register
    fn pin_reg(self) -> u32 {
        GPIO_BA + self.reg_offset() + 0x0C
    }
}

/// Pin direction
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GpioDirection {
    Input = 0,
    Output = 1,
}

/// GPIO interrupt trigger edge
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct GpioIntTrigger {
    pub falling: bool,
    pub rising: bool,
}

// ============================================================================
// Core GPIO Operations
// ============================================================================

/// Clear multifunction bits for a single pin, switching it to GPIO mode.
///
/// Returns `false` if the pin number is out of range for the port.
pub unsafe fn gpio_configure_pin(port: GpioPort, pin: u8) -> bool {
    if pin >= port.pin_count() {
        return false;
    }
    let mfp = port.mfp_reg();
    let mask = 0x3u32 << (pin as u32 * 2);
    reg_write32(mfp, reg_read32(mfp) & !mask);
    true
}

/// Clear all multifunction bits for a port, switching all pins to GPIO mode.
pub unsafe fn gpio_open_port(port: GpioPort) {
    match port {
        GpioPort::PortA => {
            reg_write32(REG_GPAFUN, reg_read32(REG_GPAFUN) & !0x00F0_0000);
        }
        GpioPort::PortD => {
            reg_write32(REG_GPDFUN, reg_read32(REG_GPDFUN) & !0xFF00_03FF);
        }
        GpioPort::PortE => {
            reg_write32(REG_GPEFUN, reg_read32(REG_GPEFUN) & !0xFF0);
        }
        // Port B and C have no peripherals blocking them by default
        _ => {}
    }
}

/// Set pin direction (Output or Input) for masked pins.
/// `mask` selects which pins are affected; `dir` sets the direction
/// (1 = output, 0 = input) for those pins.
pub unsafe fn gpio_set_direction(port: GpioPort, mask: u16, dir: GpioDirection) {
    let mask32 = mask as u32;
    let dir_val = if matches!(dir, GpioDirection::Output) {
        mask32
    } else {
        0
    };
    let reg = port.omd_reg();
    let val = reg_read32(reg);
    // Clear masked bits, then set according to direction
    reg_write32(reg, (val & !(mask32 & (mask32 ^ dir_val))) | (mask32 & dir_val));
}

/// Set output value for masked pins.  `mask` selects pins; `value` sets
/// the output level (1 = high, 0 = low) for those pins.
pub unsafe fn gpio_set_output(port: GpioPort, mask: u16, value: u16) {
    let mask32 = mask as u32;
    let val32 = value as u32 & mask32;
    let reg = port.dout_reg();
    let current = reg_read32(reg);
    reg_write32(reg, (current & !(mask32 & (mask32 ^ val32))) | (mask32 & val32));
}

/// Read the current pin input values for a port (masked to valid pins only).
pub unsafe fn gpio_read_port(port: GpioPort) -> u16 {
    (reg_read32(port.pin_reg()) & port.pin_mask() as u32) as u16
}

/// Enable or disable pull-up resistors for masked pins.
/// `enable` is a bitmask where 1 = pull-up enabled, 0 = disabled.
pub unsafe fn gpio_set_pull(port: GpioPort, mask: u16, enable: u16) {
    let mask32 = mask as u32;
    let en32 = enable as u32 & mask32;
    let reg = port.puen_reg();
    let current = reg_read32(reg);
    reg_write32(reg, (current & !(mask32 & (mask32 ^ en32))) | (mask32 & en32));
}

// ============================================================================
// GPIO Interrupt Operations
// ============================================================================

/// Assign masked pins to an interrupt source group (0–3).
///
/// Each pin maps 2 bits: `00` = group 0, `01` = group 1, `10` = group 2,
/// `11` = group 3.  The groups map to IRQ numbers:
///   - Group 0 → `IRQ_EXTINT0`
///   - Group 1 → `IRQ_EXTINT1`
///   - Group 2 → `IRQ_EXTINT2`
///   - Group 3 → `IRQ_EXTINT3`
pub unsafe fn gpio_set_int_source_group(port: GpioPort, mask: u16, irq_group: u8) -> bool {
    if irq_group > 3 || mask > port.pin_mask() {
        return false;
    }

    // Build per-pin 2-bit field masks
    let mut field_mask: u32 = 0;
    let mut i = 0u8;
    while i < 16 {
        if mask & (1u16 << i) != 0 {
            field_mask |= 0x3u32 << (i as u32 * 2);
        }
        i += 1;
    }

    // IRQ group encoding: 0→00, 1→55.., 2→AA.., 3→FF..
    let irq_pattern: [u32; 4] = [0x0000_0000, 0x5555_5555, 0xAAAA_AAAA, 0xFFFF_FFFF];

    let reg = match port {
        GpioPort::PortA => REG_IRQSRCGPA,
        GpioPort::PortB => REG_IRQSRCGPB,
        GpioPort::PortC => REG_IRQSRCGPC,
        GpioPort::PortD => REG_IRQSRCGPD,
        GpioPort::PortE => REG_IRQSRCGPE,
    };

    let current = reg_read32(reg);
    reg_write32(reg, (current & !field_mask) | (field_mask & irq_pattern[irq_group as usize]));
    true
}

/// Read the interrupt source group assignment for a port.
pub unsafe fn gpio_get_int_source_group(port: GpioPort) -> u32 {
    match port {
        GpioPort::PortA => reg_read32(REG_IRQSRCGPA),
        GpioPort::PortB => reg_read32(REG_IRQSRCGPB),
        GpioPort::PortC => reg_read32(REG_IRQSRCGPC),
        GpioPort::PortD => reg_read32(REG_IRQSRCGPD),
        GpioPort::PortE => reg_read32(REG_IRQSRCGPE),
    }
}

/// Set interrupt trigger mode for masked pins.
///
/// `falling` — mask of pins that trigger on falling edge.
/// `rising` — mask of pins that trigger on rising edge.
/// A pin in both masks triggers on both edges.
pub unsafe fn gpio_set_int_mode(port: GpioPort, mask: u16, falling: u16, rising: u16) -> bool {
    if mask > port.pin_mask() {
        return false;
    }

    let mask32 = mask as u32;
    let fall32 = falling as u32 & mask32;
    let rise32 = rising as u32 & mask32;

    let reg = match port {
        GpioPort::PortA => REG_IRQENGPA,
        GpioPort::PortB => REG_IRQENGPB,
        GpioPort::PortC => REG_IRQENGPC,
        GpioPort::PortD => REG_IRQENGPD,
        GpioPort::PortE => REG_IRQENGPE,
    };

    // Lower 16 bits = falling edge enable, upper 16 = rising edge enable
    let current = reg_read32(reg);
    let clear_mask = !((mask32 << 16) | mask32);
    let set_mask = (rise32 << 16) | fall32;
    reg_write32(reg, (current & clear_mask) | set_mask);
    true
}

/// Get the interrupt edge trigger settings for a port.
/// Returns `(falling_mask, rising_mask)`.
pub unsafe fn gpio_get_int_mode(port: GpioPort) -> (u16, u16) {
    let val = match port {
        GpioPort::PortA => reg_read32(REG_IRQENGPA),
        GpioPort::PortB => reg_read32(REG_IRQENGPB),
        GpioPort::PortC => reg_read32(REG_IRQENGPC),
        GpioPort::PortD => reg_read32(REG_IRQENGPD),
        GpioPort::PortE => reg_read32(REG_IRQENGPE),
    };
    ((val & 0xFFFF) as u16, (val >> 16) as u16)
}

// ============================================================================
// GPIO Interrupt Latch
// ============================================================================

/// Set the latch trigger selection.
/// `src` selects which interrupt source triggers the latch (0–15).
pub unsafe fn gpio_set_latch_trigger(src: u8) -> bool {
    if src > 0xF {
        return false;
    }
    reg_write32(REG_IRQLHSEL, src as u32);
    true
}

/// Read the current latch trigger selection.
pub unsafe fn gpio_get_latch_trigger() -> u8 {
    (reg_read32(REG_IRQLHSEL) & 0xF) as u8
}

/// Read the latched interrupt value for a port.
pub unsafe fn gpio_get_latch_value(port: GpioPort) -> u16 {
    let val = match port {
        GpioPort::PortA => reg_read32(REG_IRQLHGPA),
        GpioPort::PortB => reg_read32(REG_IRQLHGPB),
        GpioPort::PortC => reg_read32(REG_IRQLHGPC),
        GpioPort::PortD => reg_read32(REG_IRQLHGPD),
        GpioPort::PortE => reg_read32(REG_IRQLHGPE),
    };
    (val & port.pin_mask() as u32) as u16
}

// ============================================================================
// GPIO Trigger Source
// ============================================================================

/// Read which pin triggered an interrupt for the given port.
pub unsafe fn gpio_get_trigger_source(port: GpioPort) -> u16 {
    match port {
        GpioPort::PortA => (reg_read32(REG_IRQTGSRC0) & 0xFFFF) as u16,
        GpioPort::PortB => ((reg_read32(REG_IRQTGSRC0) >> 16) & 0xFFFF) as u16,
        GpioPort::PortC => (reg_read32(REG_IRQTGSRC1) & 0xFFFF) as u16,
        GpioPort::PortD => ((reg_read32(REG_IRQTGSRC1) >> 16) & 0xFFFF) as u16,
        GpioPort::PortE => (reg_read32(REG_IRQTGSRC2) & 0xFFFF) as u16,
    }
}

/// Clear the trigger source indicator for a port (acknowledge interrupt).
pub unsafe fn gpio_clear_trigger_source(port: GpioPort) {
    match port {
        GpioPort::PortA => reg_write32(REG_IRQTGSRC0, reg_read32(REG_IRQTGSRC0) & !0xFFFF),
        GpioPort::PortB => reg_write32(REG_IRQTGSRC0, reg_read32(REG_IRQTGSRC0) & 0xFFFF),
        GpioPort::PortC => reg_write32(REG_IRQTGSRC1, reg_read32(REG_IRQTGSRC1) & !0xFFFF),
        GpioPort::PortD => reg_write32(REG_IRQTGSRC1, reg_read32(REG_IRQTGSRC1) & 0xFFFF),
        GpioPort::PortE => reg_write32(REG_IRQTGSRC2, reg_read32(REG_IRQTGSRC2) & !0xFFFF),
    }
}

// ============================================================================
// Debounce Control
// ============================================================================

/// Valid debounce clock divider values.
const DEBOUNCE_CLOCKS: [u32; 16] = [
    1, 2, 4, 8, 16, 32, 64, 128, 256,
    2 * 256, 4 * 256, 8 * 256, 16 * 256, 32 * 256, 64 * 256, 128 * 256,
];

/// Set the debounce clock divider and source clock.
///
/// `clk_div` must be one of: 1, 2, 4, 8, 16, 32, 64, 128, 256,
/// 512, 1024, 2048, 4096, 8192, 16384, 32768.
/// `src` selects the debounce clock source (0–15).
pub unsafe fn gpio_set_debounce(clk_div: u32, src: u8) -> bool {
    if src > 0xF {
        return false;
    }

    // Find the clock divider index
    let idx = DEBOUNCE_CLOCKS.iter().position(|&c| c == clk_div);
    match idx {
        Some(i) => {
            reg_write32(REG_DBNCECON, ((i as u32) << 4) | (src as u32));
            true
        }
        None => false,
    }
}

/// Read the current debounce settings.
/// Returns `(clk_div, src)`.
pub unsafe fn gpio_get_debounce() -> (u32, u8) {
    let val = reg_read32(REG_DBNCECON);
    let idx = ((val >> 4) & 0xF) as usize;
    let clk = DEBOUNCE_CLOCKS.get(idx).copied().unwrap_or(1);
    let src = (val & 0xF) as u8;
    (clk, src)
}
