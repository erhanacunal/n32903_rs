//! Timer driver for N32903 (W55FA93)
//! Ported from wb_timer.c
//!
//! Provides two 32-bit general-purpose timers (Timer 0 and Timer 1)
//! with one-shot, periodic, toggle, and continuous modes.
//! Includes a software event scheduler and watchdog timer support.
//!
//! # Safety
//!
//! All functions are `unsafe` — they manipulate hardware registers directly.

use crate::registers::*;

// ============================================================================
// Public Types
// ============================================================================

/// Timer channel selection
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TimerChannel {
    Timer0 = 0,
    Timer1 = 1,
}

/// Timer operating mode (matches hardware bits 31:29 of TCSR)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TimerOpMode {
    /// Fire once, then stop
    OneShot = 0,
    /// Fire repeatedly at the configured interval
    Periodic = 1,
    /// Toggle output on each tick
    Toggle = 2,
    /// Continuous (uninterrupt) mode
    Continuous = 3,
}

/// Timer prescaler value (0 = /1, … 15 = /16384)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TimerPrescaler {
    Div1 = 0,
    Div2 = 1,
    Div4 = 2,
    Div8 = 3,
    Div16 = 4,
    Div32 = 5,
    Div64 = 6,
    Div128 = 7,
    Div256 = 8,
    Div512 = 9,
    Div1024 = 10,
    Div2048 = 11,
    Div4096 = 12,
    Div8192 = 13,
    Div16384 = 14,
    Div32768 = 15,
}

/// Watchdog timeout interval
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WdtInterval {
    /// 0.5 seconds
    Ms500 = 0,
    /// 5 seconds
    Sec5 = 1,
    /// 10 seconds
    Sec10 = 2,
    /// 20 seconds
    Sec20 = 3,
}

/// Callback type for timer events (function pointer, no arguments).
pub type TimerCallback = unsafe extern "C" fn();

// ============================================================================
// Constants
// ============================================================================

/// Maximum number of software timer events per channel (matching the C BSP).
pub const TIMER_EVENT_COUNT: usize = 10;

/// Default timer reference clock rate (external crystal).
pub const TIMER_DEFAULT_CLOCK: u32 = EXTERNAL_CRYSTAL_CLOCK;

// ============================================================================
// Static State
// ============================================================================

/// Per-channel tick counter
static mut TICK_COUNT: [u32; 2] = [0, 0];

/// Per-channel ticks-per-second setting
static mut TICK_PER_SEC: [u32; 2] = [0, 0];

/// Per-channel reference clock rate in Hz
static mut REF_CLOCK: [u32; 2] = [TIMER_DEFAULT_CLOCK, TIMER_DEFAULT_CLOCK];

/// Per-channel initialisation flag
static mut IS_INITIALIZED: [bool; 2] = [false, false];

/// Software event table
#[derive(Clone, Copy)]
struct TimerEvent {
    active: bool,
    init_tick: u32,
    cur_tick: u32,
    callback: Option<TimerCallback>,
}

static mut TIMER0_EVENTS: [TimerEvent; TIMER_EVENT_COUNT] = [TimerEvent {
    active: false,
    init_tick: 0,
    cur_tick: 0,
    callback: None,
}; TIMER_EVENT_COUNT];

static mut TIMER1_EVENTS: [TimerEvent; TIMER_EVENT_COUNT] = [TimerEvent {
    active: false,
    init_tick: 0,
    cur_tick: 0,
    callback: None,
}; TIMER_EVENT_COUNT];

// ============================================================================
// Timer Core API
// ============================================================================

/// Set the reference clock frequency for a timer channel (in Hz).
pub unsafe fn timer_set_ref_clock(ch: TimerChannel, clock_hz: u32) {
    REF_CLOCK[ch as usize] = clock_hz;
}

/// Get the reference clock frequency for a timer channel.
pub fn timer_get_ref_clock(ch: TimerChannel) -> u32 {
    unsafe { REF_CLOCK[ch as usize] }
}

/// Initialise and start a timer channel.
///
/// `ticks_per_sec` controls how many times per second the timer fires.
/// `prescaler` divides the reference clock before the tick counter.
/// `mode` sets one-shot, periodic, toggle, or continuous operation.
///
/// The ISR increments a software tick counter.  Call `timer_isr()`
/// from your interrupt handler to drive the event system.
pub unsafe fn timer_start(
    ch: TimerChannel,
    ticks_per_sec: u32,
    prescaler: TimerPrescaler,
    mode: TimerOpMode,
) {
    let ch_idx = ch as usize;
    let tcsr_reg = if ch_idx == 0 { REG_TCSR0 } else { REG_TCSR1 };
    let ticr_reg = if ch_idx == 0 { REG_TICR0 } else { REG_TICR1 };
    let cke_bit = if ch_idx == 0 { TMR0_CKE } else { TMR1_CKE };
    let rst_bit = if ch_idx == 0 { TMR0RST } else { TMR1RST };

    IS_INITIALIZED[ch_idx] = true;
    TICK_PER_SEC[ch_idx] = ticks_per_sec;

    // Enable clock
    reg_write32(REG_APBCLK, reg_read32(REG_APBCLK) | cke_bit);

    // Reset timer
    reg_write32(REG_APBIPRST, reg_read32(REG_APBIPRST) | rst_bit);
    reg_write32(REG_APBIPRST, reg_read32(REG_APBIPRST) & !rst_bit);

    // Clear event table
    let events = if ch_idx == 0 {
        &mut TIMER0_EVENTS
    } else {
        &mut TIMER1_EVENTS
    };
    for e in events.iter_mut() {
        e.active = false;
    }

    // Reset tick counter
    TICK_COUNT[ch_idx] = 0;

    // Set initial count: clock / ticks_per_sec
    let ref_clk = REF_CLOCK[ch_idx];
    let prescale_div = 1u32 << (prescaler as u32);
    let effective_clock = ref_clk / prescale_div;
    let initial_count = effective_clock / ticks_per_sec;
    reg_write32(ticr_reg, initial_count);

    // Configure mode + prescaler + enable:
    // TCSR = (prescaler << 24) | ((mode | 0xC) << 27)
    // The | 0xC sets bit 12 (CEN) plus one extra bit for the mode encoding.
    let tcsr_val = (reg_read32(tcsr_reg) & 0x87FF_FF00)
        | ((prescaler as u32) << TCSR_PRESCALE_POS)
        | (((mode as u32) | 0xC) << TCSR_MODE_POS);
    reg_write32(tcsr_reg, tcsr_val);
}

/// Stop a timer channel, disable its interrupt, and gate the clock.
pub unsafe fn timer_stop(ch: TimerChannel) {
    let ch_idx = ch as usize;
    IS_INITIALIZED[ch_idx] = false;

    let tcsr_reg = if ch_idx == 0 { REG_TCSR0 } else { REG_TCSR1 };
    let cke_bit = if ch_idx == 0 { TMR0_CKE } else { TMR1_CKE };
    let int_bit = if ch_idx == 0 { TIF0 } else { TIF1 };

    // Disable timer
    reg_write32(tcsr_reg, 0);

    // Clear interrupt flag
    reg_write32(REG_TISR, int_bit);

    // Clear event table
    let events = if ch_idx == 0 {
        &mut TIMER0_EVENTS
    } else {
        &mut TIMER1_EVENTS
    };
    for e in events.iter_mut() {
        e.active = false;
    }

    // Gate clock
    reg_write32(REG_APBCLK, reg_read32(REG_APBCLK) & !cke_bit);
}

/// Read the current tick count.
pub unsafe fn timer_get_ticks(ch: TimerChannel) -> u32 {
    TICK_COUNT[ch as usize]
}

/// Reset the tick counter to zero.
pub unsafe fn timer_reset_ticks(ch: TimerChannel) {
    TICK_COUNT[ch as usize] = 0;
}

/// Check if a timer channel has been initialised.
pub fn timer_is_initialized(ch: TimerChannel) -> bool {
    unsafe { IS_INITIALIZED[ch as usize] }
}

// ============================================================================
// Software Timer Events
// ============================================================================

/// Register a timer event callback.  Returns the event slot number
/// (1-based, so 0 means failure / no free slots).
pub unsafe fn timer_set_event(
    ch: TimerChannel,
    tick_interval: u32,
    callback: TimerCallback,
) -> u32 {
    let events = if ch == TimerChannel::Timer0 {
        &mut TIMER0_EVENTS
    } else {
        &mut TIMER1_EVENTS
    };

    for (i, e) in events.iter_mut().enumerate() {
        if !e.active {
            e.active = true;
            e.init_tick = tick_interval;
            e.cur_tick = tick_interval;
            e.callback = Some(callback);
            return (i + 1) as u32;
        }
    }
    0 // No free slot
}

/// Remove a timer event by its slot number (1-based).
pub unsafe fn timer_clear_event(ch: TimerChannel, event_id: u32) {
    if event_id == 0 || event_id as usize > TIMER_EVENT_COUNT {
        return;
    }

    let events = if ch == TimerChannel::Timer0 {
        &mut TIMER0_EVENTS
    } else {
        &mut TIMER1_EVENTS
    };

    events[(event_id - 1) as usize].active = false;
}

// ============================================================================
// Timer ISR (call from your interrupt handler)
// ============================================================================

/// Must be called from the timer interrupt handler.
/// Checks both channels, increments counters, and fires registered events.
pub unsafe fn timer_isr() {
    // --- Channel 0 ---
    if reg_read32(REG_TISR) & TIF0 != 0 {
        TICK_COUNT[0] = TICK_COUNT[0].wrapping_add(1);

        // Clear interrupt
        reg_write32(REG_TISR, TIF0);

        // Fire events
        for e in TIMER0_EVENTS.iter_mut() {
            if e.active {
                e.cur_tick -= 1;
                if e.cur_tick == 0 {
                    if let Some(cb) = e.callback {
                        cb();
                    }
                    e.cur_tick = e.init_tick;
                }
            }
        }
    }

    // --- Channel 1 ---
    if reg_read32(REG_TISR) & TIF1 != 0 {
        TICK_COUNT[1] = TICK_COUNT[1].wrapping_add(1);

        // Clear interrupt
        reg_write32(REG_TISR, TIF1);

        // Fire events
        for e in TIMER1_EVENTS.iter_mut() {
            if e.active {
                e.cur_tick -= 1;
                if e.cur_tick == 0 {
                    if let Some(cb) = e.callback {
                        cb();
                    }
                    e.cur_tick = e.init_tick;
                }
            }
        }
    }
}

// ============================================================================
// Blocking Delay
// ============================================================================

/// Busy-wait delay for the given number of milliseconds.
///
/// If Timer 0 is not already running it will be started automatically
/// at 100 ticks/sec (10 ms resolution).
pub unsafe fn timer_delay_ms(ms: u32) {
    if !IS_INITIALIZED[0] {
        timer_start(
            TimerChannel::Timer0,
            100,
            TimerPrescaler::Div1,
            TimerOpMode::Periodic,
        );
    }

    let ticks_per_ms = TICK_PER_SEC[0] as u64 * ms as u64 / 1000;
    let target_ticks = ticks_per_ms as u32;

    let start = timer_get_ticks(TimerChannel::Timer0);
    loop {
        let current = timer_get_ticks(TimerChannel::Timer0);
        if current.wrapping_sub(start) > target_ticks {
            break;
        }
    }
}

/// Busy-wait for N timer ticks on Timer 0.
pub unsafe fn timer_delay_ticks(ticks: u32) {
    if !IS_INITIALIZED[0] {
        timer_start(
            TimerChannel::Timer0,
            100,
            TimerPrescaler::Div1,
            TimerOpMode::Periodic,
        );
    }

    let start = timer_get_ticks(TimerChannel::Timer0);
    loop {
        let current = timer_get_ticks(TimerChannel::Timer0);
        if current.wrapping_sub(start) > ticks {
            break;
        }
    }
}

// ============================================================================
// Watchdog Timer API
// ============================================================================

/// Enable the watchdog timer with the given timeout interval.
/// If `reset_on_timeout` is true the chip will be reset on timeout.
pub unsafe fn wdt_enable(interval: WdtInterval, reset_on_timeout: bool) {
    let mut wtcr = reg_read32(REG_WTCR);
    wtcr &= !WDT_INTERVAL_MASK;
    wtcr |= (interval as u32) << 4;
    wtcr |= WDT_WTEN; // system enable
    wtcr |= WDT_WTE; // timer enable
    if reset_on_timeout {
        wtcr |= WDT_WTRE;
    }
    reg_write32(REG_WTCR, wtcr);
}

/// Disable the watchdog timer.
pub unsafe fn wdt_disable() {
    let mut wtcr = reg_read32(REG_WTCR);
    wtcr &= !WDT_WTEN;
    wtcr &= !WDT_WTE;
    reg_write32(REG_WTCR, wtcr);
}

/// Clear the watchdog counter (prevent timeout).  Must be called
/// periodically while the watchdog is enabled.
pub unsafe fn wdt_clear() {
    reg_write32(REG_WTCR, reg_read32(REG_WTCR) | WDT_WTR);
}

/// Clear the watchdog interrupt flag.
pub unsafe fn wdt_clear_interrupt() {
    reg_write32(REG_WTCR, reg_read32(REG_WTCR) | WDT_WTIF);
}

/// Enable the watchdog interrupt.  The ISR should call `wdt_clear_interrupt()`.
pub unsafe fn wdt_enable_interrupt() {
    reg_write32(REG_WTCR, reg_read32(REG_WTCR) | WDT_WTIE);
}

/// Set the watchdog timeout interval.
pub unsafe fn wdt_set_interval(interval: WdtInterval) {
    let mut wtcr = reg_read32(REG_WTCR);
    wtcr &= !WDT_INTERVAL_MASK;
    wtcr |= (interval as u32) << 4;
    reg_write32(REG_WTCR, wtcr);
}

/// Disable the watchdog reset-on-timeout behavior.
pub unsafe fn wdt_disable_reset() {
    reg_write32(REG_WTCR, reg_read32(REG_WTCR) & !WDT_WTRE);
}

/// Enable the watchdog reset-on-timeout behavior.
pub unsafe fn wdt_enable_reset() {
    reg_write32(REG_WTCR, reg_read32(REG_WTCR) | WDT_WTRE);
}
