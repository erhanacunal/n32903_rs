//! SPU (Sound Processing Unit) driver for N32903 (W55FA93)
//! Port of the vendor `SPU/Src/DrvSPU.c` and `SPU/Src/SPU.c`
//! (N32905 NonOS BSP).
//!
//! 32-channel audio DMA engine with equalizer, volume, PAN and DAC
//! control. Supports PCM16 (mono/stereo), MDPCM, LP8 and tone formats.
//!
//! Playback (`open` → `dac_on` → `start_play`) uses channel 0 in stereo
//! PCM16 mode: the SPU loops over a ring buffer in SDRAM by DMA, and
//! interrupts at its threshold (half) and end addresses, so the consumed
//! half can be refilled by the [`SpuCallback`] passed to `start_play`.
//!
//! Differences from the vendor code: waits on `CH_FN` are bounded, the ring
//! buffer is supplied by the caller (any size, 256-byte aligned) instead of
//! a fixed 32 KB array, `enable_int` accepts several event bits at once, and
//! `sysDelay` ticks (10 ms) are written as milliseconds.
//!
//! # Safety
//!
//! All functions are `unsafe` — they manipulate hardware registers.

use crate::registers::*;

// ============================================================================
// Public Types
// ============================================================================

/// SPU channel index (0–31)
pub type SpuChannel = u32;

/// Audio source format (`SRC_TYPE`)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum SpuSrcType {
    MdPcm = SPU_SRC_MDPCM,
    Lp8 = SPU_SRC_LP8,
    Pcm16 = SPU_SRC_PCM16,
    Tone = SPU_SRC_TONE,
    Pcm16Mono = SPU_SRC_PCM16_MONO,
    /// Stereo PCM16, left sample in bits 15:0 of each 32-bit word.
    Pcm16StereoLeft = SPU_SRC_PCM16_STEREO_L,
    /// Stereo PCM16, right sample in bits 31:16 of each 32-bit word.
    Pcm16StereoRight = SPU_SRC_PCM16_STEREO_R,
}

/// Equalizer band (DC + bands 1–10)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SpuEqBand {
    Dc = 0,
    Band1, Band2, Band3, Band4, Band5,
    Band6, Band7, Band8, Band9, Band10,
}

/// Equalizer gain in dB
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum SpuEqGain {
    M7dB = 0, M6dB, M5dB, M4dB, M3dB, M2dB, M1dB, M0dB,
    P1dB, P2dB, P3dB, P4dB, P5dB, P6dB, P7dB, P8dB,
}

/// Channel event bits (`DRVSPU_*_INT`): the event's flag bit in
/// `CH_EVENT`; the matching enable bit is 8 positions lower.
pub mod int {
    use crate::registers::*;
    pub const USER: u32 = EV_USR_FG;
    pub const SILENT: u32 = EV_SLN_FG;
    pub const LOOP_START: u32 = EV_LP_FG;
    pub const END: u32 = EV_END_FG;
    /// End address reached (the second half has been played).
    pub const END_ADDRESS: u32 = END_FG;
    /// Threshold address reached (the first half has been played).
    pub const TH_ADDRESS: u32 = TH_FG;
    pub const ALL: u32 = USER | SILENT | LOOP_START | END | END_ADDRESS | TH_ADDRESS;
}

/// Refill callback, called from the SPU interrupt with the (uncached)
/// address of the half of the ring buffer that has just been played.
pub type SpuCallback = unsafe fn(half: *mut u8);

/// Errors
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SpuError {
    /// Channel index ≥ 32
    WrongChannel,
    /// Ring buffer not 256-byte aligned, or its size not a multiple of 512
    BadBuffer,
}

// ============================================================================
// Static State
// ============================================================================

/// Refill callbacks per channel (vendor `g_pfn*EventCallBack`): threshold
/// and end address are the only events with a buffer to refill.
static mut TH_CALLBACKS: [Option<SpuCallback>; 32] = [None; 32];
static mut END_CALLBACKS: [Option<SpuCallback>; 32] = [None; 32];

/// Ring buffer used by `open` / the ISR (uncached address, size in bytes).
static mut PLAY_BUF: u32 = 0;
static mut PLAY_SIZE: u32 = 0;

/// The DAC power-up level from `dac_on`, reused by `dac_off`.
static mut DAC_LEVEL: u8 = 0;

/// Upper bound on a `CH_FN` busy wait (the vendor waits forever).
const FN_POLLS: u32 = 1_000_000;

/// Uncached alias of SDRAM addresses (bit 31): the SPU reads by DMA, so the
/// CPU writes the ring buffer through this alias.
pub const UNCACHED: u32 = 0x8000_0000;

// ============================================================================
// Channel register window
// ============================================================================

#[inline]
unsafe fn wait_fn() {
    let mut n = FN_POLLS;
    while reg_read32(REG_SPU_CH_CTRL) & CH_FN != 0 && n > 0 {
        n -= 1;
    }
}

/// Load channel `ch` into the shared register window.
unsafe fn select(ch: SpuChannel) {
    wait_fn();
    reg_write32(REG_SPU_CH_CTRL, (reg_read32(REG_SPU_CH_CTRL) & !CH_NO) | (ch << 24));
    reg_write32(REG_SPU_CH_CTRL, reg_read32(REG_SPU_CH_CTRL) | CH_CTRL_LOAD);
    wait_fn();
}

/// Write the whole window back to the selected channel.
unsafe fn update_all() {
    reg_write32(REG_SPU_CH_CTRL, reg_read32(REG_SPU_CH_CTRL) | CH_CTRL_UPDATE_ALL);
    wait_fn();
}

/// Write back only the `partial` fields (`UP_*`) of the selected channel.
unsafe fn update_partial(partial: u32) {
    reg_write32(REG_SPU_CH_CTRL, reg_read32(REG_SPU_CH_CTRL) & !CH_CTRL_UPDATE_ALL_PARTIALS);
    reg_write32(REG_SPU_CH_CTRL, reg_read32(REG_SPU_CH_CTRL) | partial | CH_CTRL_UPDATE_PARTIAL);
    wait_fn();
}

#[inline]
fn check(ch: SpuChannel) -> Result<(), SpuError> {
    if ch < 32 { Ok(()) } else { Err(SpuError::WrongChannel) }
}

// ============================================================================
// Core Hardware Control (DrvSPU_Open / Close / StartPlay / StopPlay)
// ============================================================================

/// Enable the SPU clocks and reset the engine; all channels off, all
/// channel interrupts cleared and disabled.
pub unsafe fn drv_open() {
    reg_write32(REG_AHBCLK, reg_read32(REG_AHBCLK) | ADO_CKE | SPU_CKE | HCLK4_CKE);
    reg_write32(REG_SPU_CTRL, 0);
    reg_write32(REG_SPU_CTRL, SPU_FIFO_SIZE);
    reg_write32(REG_SPU_CTRL, reg_read32(REG_SPU_CTRL) & !SPU_SWRST);
    reg_write32(REG_SPU_CTRL, reg_read32(REG_SPU_CTRL) | SPU_SWRST);
    reg_write32(REG_SPU_CTRL, reg_read32(REG_SPU_CTRL) & !SPU_SWRST);
    reg_write32(REG_SPU_CH_EN, 0);
    for ch in 0..32 {
        let _ = clear_int(ch, int::ALL);
        let _ = disable_int(ch, int::ALL);
    }
}

/// Reset and disable the SPU and stop its clocks.
pub unsafe fn drv_close() {
    reg_write32(REG_SPU_CTRL, reg_read32(REG_SPU_CTRL) & !SPU_SWRST);
    reg_write32(REG_SPU_CTRL, reg_read32(REG_SPU_CTRL) | SPU_SWRST);
    reg_write32(REG_SPU_CTRL, reg_read32(REG_SPU_CTRL) & !SPU_SWRST);
    reg_write32(REG_SPU_CTRL, reg_read32(REG_SPU_CTRL) & !SPU_EN);
    reg_write32(REG_AHBCLK, reg_read32(REG_AHBCLK) & !ADO_CKE & !SPU_CKE);
}

/// Start the engine (all enabled channels play).
pub unsafe fn drv_start_play() {
    reg_write32(REG_SPU_CTRL, reg_read32(REG_SPU_CTRL) | SPU_EN);
}

/// Stop the engine.
pub unsafe fn drv_stop_play() {
    reg_write32(REG_SPU_CTRL, reg_read32(REG_SPU_CTRL) & !SPU_EN);
}

/// True while the engine is enabled.
pub unsafe fn is_playing() -> bool {
    reg_read32(REG_SPU_CTRL) & SPU_EN != 0
}

// ============================================================================
// Channel Management
// ============================================================================

pub unsafe fn channel_open(ch: SpuChannel) -> Result<(), SpuError> {
    check(ch)?;
    reg_write32(REG_SPU_CH_EN, reg_read32(REG_SPU_CH_EN) | (1 << ch));
    Ok(())
}

pub unsafe fn channel_close(ch: SpuChannel) -> Result<(), SpuError> {
    check(ch)?;
    reg_write32(REG_SPU_CH_EN, reg_read32(REG_SPU_CH_EN) & !(1 << ch));
    Ok(())
}

pub unsafe fn is_channel_enabled(ch: SpuChannel) -> bool {
    ch < 32 && reg_read32(REG_SPU_CH_EN) & (1 << ch) != 0
}

pub unsafe fn channel_pause(ch: SpuChannel) -> Result<(), SpuError> {
    check(ch)?;
    reg_write32(REG_SPU_CH_PAUSE, reg_read32(REG_SPU_CH_PAUSE) | (1 << ch));
    Ok(())
}

pub unsafe fn channel_resume(ch: SpuChannel) -> Result<(), SpuError> {
    check(ch)?;
    reg_write32(REG_SPU_CH_PAUSE, reg_read32(REG_SPU_CH_PAUSE) & !(1 << ch));
    Ok(())
}

// ============================================================================
// Channel Addresses
// ============================================================================

unsafe fn set_addr_reg(ch: SpuChannel, reg: u32, addr: u32) -> Result<(), SpuError> {
    check(ch)?;
    select(ch);
    reg_write32(reg, addr);
    update_all();
    Ok(())
}

unsafe fn get_reg(ch: SpuChannel, reg: u32) -> u32 {
    if ch >= 32 {
        return 0;
    }
    select(ch);
    reg_read32(reg)
}

/// Source start (base) address.
pub unsafe fn set_base_address(ch: SpuChannel, addr: u32) -> Result<(), SpuError> {
    set_addr_reg(ch, REG_SPU_S_ADDR, addr)
}

pub unsafe fn get_base_address(ch: SpuChannel) -> u32 {
    get_reg(ch, REG_SPU_S_ADDR)
}

/// Threshold address (threshold interrupt).
pub unsafe fn set_threshold_address(ch: SpuChannel, addr: u32) -> Result<(), SpuError> {
    set_addr_reg(ch, REG_SPU_M_ADDR, addr)
}

pub unsafe fn get_threshold_address(ch: SpuChannel) -> u32 {
    get_reg(ch, REG_SPU_M_ADDR)
}

/// End address (end-address interrupt; PCM16 wraps back to the base).
pub unsafe fn set_end_address(ch: SpuChannel, addr: u32) -> Result<(), SpuError> {
    set_addr_reg(ch, REG_SPU_E_ADDR, addr)
}

pub unsafe fn get_end_address(ch: SpuChannel) -> u32 {
    get_reg(ch, REG_SPU_E_ADDR)
}

/// Address the DMA is reading.
pub unsafe fn get_current_address(ch: SpuChannel) -> u32 {
    get_reg(ch, REG_SPU_CUR_ADDR)
}

/// Pause address for mono/stereo PCM16.
pub unsafe fn set_pause_address(ch: SpuChannel, addr: u32) -> Result<(), SpuError> {
    check(ch)?;
    select(ch);
    reg_write32(REG_SPU_PA_ADDR, addr);
    update_partial(UP_PAUSE_ADDR);
    Ok(())
}

pub unsafe fn get_pause_address(ch: SpuChannel) -> u32 {
    get_reg(ch, REG_SPU_PA_ADDR)
}

/// Tone amplitude (tone source type).
pub unsafe fn set_tone_amp(ch: SpuChannel, amp: u32) -> Result<(), SpuError> {
    set_addr_reg(ch, REG_SPU_TONE_AMP, amp)
}

/// Tone pulse (tone source type).
pub unsafe fn set_tone_pulse(ch: SpuChannel, pulse: u32) -> Result<(), SpuError> {
    set_addr_reg(ch, REG_SPU_TONE_PULSE, pulse)
}

// ============================================================================
// Channel Parameters
// ============================================================================

/// Source data format.
pub unsafe fn set_src_type(ch: SpuChannel, src: SpuSrcType) -> Result<(), SpuError> {
    check(ch)?;
    select(ch);
    reg_write32(REG_SPU_CH_PAR_1, (reg_read32(REG_SPU_CH_PAR_1) & !SRC_TYPE) | src as u32);
    update_all();
    Ok(())
}

pub unsafe fn get_src_type(ch: SpuChannel) -> u32 {
    get_reg(ch, REG_SPU_CH_PAR_1) & SRC_TYPE
}

/// Channel volume, 0–0x7F (vendor default 0x4F for earphones, 0x3F for
/// a speaker).
pub unsafe fn set_channel_volume(ch: SpuChannel, volume: u8) -> Result<(), SpuError> {
    check(ch)?;
    select(ch);
    reg_write32(
        REG_SPU_CH_PAR_1,
        (reg_read32(REG_SPU_CH_PAR_1) & 0x00FF_FFFF) | ((volume as u32) << 24),
    );
    update_partial(UP_VOL);
    Ok(())
}

pub unsafe fn get_channel_volume(ch: SpuChannel) -> u8 {
    (get_reg(ch, REG_SPU_CH_PAR_1) >> 24) as u8
}

/// PAN: high byte → `PAN_L` (bits 20:16), low byte → `PAN_R` (bits 12:8),
/// 0–0x1F each (0x1F1F = centre).
pub unsafe fn set_pan(ch: SpuChannel, pan: u16) -> Result<(), SpuError> {
    check(ch)?;
    select(ch);
    let pan = ((pan as u32) << 8) & (PAN_L | PAN_R);
    reg_write32(REG_SPU_CH_PAR_1, (reg_read32(REG_SPU_CH_PAR_1) & !(PAN_L | PAN_R)) | pan);
    update_partial(UP_PAN);
    Ok(())
}

pub unsafe fn get_pan(ch: SpuChannel) -> u16 {
    (get_reg(ch, REG_SPU_CH_PAR_1) >> 8) as u16
}

/// DFA (sample rate adjust): 1024 = play at the DAC rate.
pub unsafe fn set_dfa(ch: SpuChannel, dfa: u16) -> Result<(), SpuError> {
    check(ch)?;
    select(ch);
    reg_write32(REG_SPU_CH_PAR_2, dfa as u32);
    update_partial(UP_DFA);
    Ok(())
}

pub unsafe fn get_dfa(ch: SpuChannel) -> u16 {
    (get_reg(ch, REG_SPU_CH_PAR_2) & DFA) as u16
}

/// Program the audio DAC clock (`REG_CLKDIV1`: UPLL ÷ (N0+1) ÷ (N1+1) =
/// 128 × rate) for `sample_rate` from `upll_hz`, then the DFA of channels
/// 0 and 1 to make up the rounding. The headphone volume is ramped to 0
/// around the clock switch (pop suppression) and restored. Returns the
/// DAC sample rate actually set.
pub unsafe fn set_sample_rate(upll_hz: u32, sample_rate: u32) -> u32 {
    reg_write32(REG_AHBCLK, reg_read32(REG_AHBCLK) | ADO_CKE | SPU_CKE);

    let mut div = upll_hz / (128 * sample_rate);
    let mut clk_sel = 0;
    if div > 255 {
        clk_sel = div / 255;
        div /= clk_sel;
    }
    let mut v = reg_read32(REG_CLKDIV1);
    v &= !ADO_S;
    v |= (0x3 << 19) | (clk_sel << 16); // ADO_S = UPLL, ADO_N0
    v &= !ADO_N1;
    v |= ((div & 0xFF).wrapping_sub(1) & 0xFF) << 24;

    let steps = reg_read32(REG_SPU_DAC_VOL) & 0xFF;
    for _ in 0..steps {
        reg_write32(REG_SPU_DAC_VOL, reg_read32(REG_SPU_DAC_VOL).wrapping_sub(0x0101));
        crate::timer::timer_delay_ms(1);
    }
    reg_write32(REG_CLKDIV1, v);
    for _ in 0..steps {
        reg_write32(REG_SPU_DAC_VOL, reg_read32(REG_SPU_DAC_VOL).wrapping_add(0x0101));
        crate::timer::timer_delay_ms(1);
    }

    let real = upll_hz / (128 * div.max(1) * (clk_sel + 1));
    let dfa = (sample_rate * 1024 / real.max(1)) as u16;
    let _ = set_dfa(0, dfa);
    let _ = set_dfa(1, dfa);
    real
}

// ============================================================================
// Global Volume / Equalizer
// ============================================================================

/// Headphone volume, 0 (mute) – 0x3F each side.
pub unsafe fn set_volume(left: u8, right: u8) {
    let vol = ((left as u32 & 0x3F) << 8) | (right as u32 & 0x3F);
    reg_write32(REG_SPU_DAC_VOL, (reg_read32(REG_SPU_DAC_VOL) & !0x3F3F) | vol);
}

/// (left, right) headphone volume.
pub unsafe fn get_volume() -> (u8, u8) {
    let v = reg_read32(REG_SPU_DAC_VOL);
    (((v >> 8) & 0x3F) as u8, (v & 0x3F) as u8)
}

/// Set one equalizer band's gain and enable the equalizer.
pub unsafe fn eq_open(band: SpuEqBand, gain: SpuEqGain) {
    let g = gain as u32;
    let (reg, mask, shift) = match band {
        SpuEqBand::Dc => (REG_SPU_EQGain1, GAINDC, 16),
        SpuEqBand::Band1 => (REG_SPU_EQGain0, GAIN01, 0),
        SpuEqBand::Band2 => (REG_SPU_EQGain0, GAIN02, 4),
        SpuEqBand::Band3 => (REG_SPU_EQGain0, GAIN03, 8),
        SpuEqBand::Band4 => (REG_SPU_EQGain0, GAIN04, 12),
        SpuEqBand::Band5 => (REG_SPU_EQGain0, GAIN05, 16),
        SpuEqBand::Band6 => (REG_SPU_EQGain0, GAIN06, 20),
        SpuEqBand::Band7 => (REG_SPU_EQGain0, GAIN07, 24),
        SpuEqBand::Band8 => (REG_SPU_EQGain0, GAIN08, 28),
        SpuEqBand::Band9 => (REG_SPU_EQGain1, GAIN09, 0),
        SpuEqBand::Band10 => (REG_SPU_EQGain1, GAIN10, 4),
    };
    reg_write32(reg, (reg_read32(reg) & !mask) | (g << shift));
    reg_write32(REG_SPU_DAC_PAR, reg_read32(REG_SPU_DAC_PAR) | EQU_EN | ZERO_EN);
}

/// Disable the equalizer (gains are kept).
pub unsafe fn eq_close() {
    reg_write32(REG_SPU_DAC_PAR, reg_read32(REG_SPU_DAC_PAR) & !EQU_EN & !ZERO_EN);
}

// ============================================================================
// Interrupts
// ============================================================================

/// Enable channel events (`int::*`, several allowed). `cb` is stored for
/// the threshold / end address events.
pub unsafe fn enable_int(ch: SpuChannel, events: u32, cb: Option<SpuCallback>) -> Result<(), SpuError> {
    check(ch)?;
    select(ch);
    // Enable bits sit 8 positions below the flag bits.
    let en = (events & int::ALL) >> 8;
    reg_write32(REG_SPU_CH_EVENT, reg_read32(REG_SPU_CH_EVENT) | en);
    if events & int::TH_ADDRESS != 0 {
        TH_CALLBACKS[ch as usize] = cb;
    }
    if events & int::END_ADDRESS != 0 {
        END_CALLBACKS[ch as usize] = cb;
    }
    update_partial(UP_IRQ);
    Ok(())
}

/// Disable channel events.
pub unsafe fn disable_int(ch: SpuChannel, events: u32) -> Result<(), SpuError> {
    check(ch)?;
    select(ch);
    let en = (events & int::ALL) >> 8;
    reg_write32(REG_SPU_CH_EVENT, reg_read32(REG_SPU_CH_EVENT) & !en);
    update_partial(UP_IRQ);
    Ok(())
}

/// True if any of `events` is enabled on the channel.
pub unsafe fn is_int_enabled(ch: SpuChannel, events: u32) -> bool {
    ch < 32 && get_reg(ch, REG_SPU_CH_EVENT) & ((events & int::ALL) >> 8) != 0
}

/// Clear channel event flags (and auto-clear).
pub unsafe fn clear_int(ch: SpuChannel, events: u32) -> Result<(), SpuError> {
    check(ch)?;
    select(ch);
    for bit in [int::USER, int::SILENT, int::LOOP_START, int::END, int::END_ADDRESS, int::TH_ADDRESS] {
        if events & bit != 0 {
            reg_write32(REG_SPU_CH_EVENT, (reg_read32(REG_SPU_CH_EVENT) & !0x3F00) | bit);
        }
    }
    reg_write32(REG_SPU_CH_EVENT, reg_read32(REG_SPU_CH_EVENT) & !AT_CLR_EN);
    update_partial(UP_IRQ);
    Ok(())
}

/// SPU interrupt handler (`DrvSPU_IntHandler`): acknowledges every
/// channel's events and calls the refill callbacks — threshold → first
/// half, end address → second half of the `open` ring buffer.
///
/// Install with `aic_install_isr(Irq1, IRQ_SPU, isr)`.
pub unsafe extern "C" fn isr() {
    let pending = reg_read32(REG_SPU_CH_IRQ);
    for ch in 0..32u32 {
        if pending & (1 << ch) == 0 {
            continue;
        }
        select(ch);
        let flags = reg_read32(REG_SPU_CH_EVENT);
        reg_write32(REG_SPU_CH_EVENT, reg_read32(REG_SPU_CH_EVENT));
        let enabled = reg_read32(REG_SPU_CH_EVENT);
        let half = PLAY_SIZE / 2;
        if flags & END_FG != 0 && enabled & END_EN != 0 {
            if let Some(cb) = END_CALLBACKS[ch as usize] {
                cb((PLAY_BUF + half) as *mut u8);
            }
        }
        if flags & TH_FG != 0 && enabled & TH_EN != 0 {
            if let Some(cb) = TH_CALLBACKS[ch as usize] {
                cb(PLAY_BUF as *mut u8);
            }
        }
        update_partial(UP_IRQ);
    }
    reg_write32(REG_SPU_CH_IRQ, reg_read32(REG_SPU_CH_IRQ));
}

// ============================================================================
// Playback (SPU.c: spuOpen / spuDacOn / spuStartPlay / spuStopPlay / ...)
// ============================================================================

/// Set up stereo PCM16 playback on channel 0 from the ring buffer `buf`
/// (`size` bytes, 256-byte aligned, a multiple of 512): reset the engine,
/// centre PAN, channel volume `ch_volume` (vendor: 0x4F), DAC clock for
/// `sample_rate` from `upll_hz`, threshold at half the buffer, end at its
/// end, and the SPU interrupt (IRQ level 1). The buffer is zeroed through
/// its uncached alias; the callback passed to `start_play` refills it.
pub unsafe fn open(
    buf: *mut u8,
    size: u32,
    upll_hz: u32,
    sample_rate: u32,
    ch_volume: u8,
) -> Result<u32, SpuError> {
    let addr = buf as u32 | UNCACHED;
    if addr & 0xFF != 0 || size == 0 || size % 512 != 0 {
        return Err(SpuError::BadBuffer);
    }
    PLAY_BUF = addr;
    PLAY_SIZE = size;
    core::ptr::write_bytes(addr as *mut u8, 0, size as usize);

    drv_stop_play();
    drv_open();
    set_pan(0, 0x1F1F)?;
    set_channel_volume(0, ch_volume)?;
    set_channel_volume(1, ch_volume)?;
    channel_open(0)?;
    let real = set_sample_rate(upll_hz, sample_rate);
    set_src_type(0, SpuSrcType::Pcm16StereoLeft)?;
    for ch in 0..2 {
        set_base_address(ch, addr)?;
        set_threshold_address(ch, addr + size / 2)?;
        set_end_address(ch, addr + size)?;
    }

    crate::aic::aic_install_isr(crate::aic::IntLevel::Irq1, IRQ_SPU, isr);
    crate::aic::aic_enable_interrupt(IRQ_SPU);
    Ok(real)
}

/// Power the DAC up with pop suppression. `level`: 1 = ~0.5–1 s ramp,
/// 2 = ~2 s, 3 = ~3 s; 0 = just release the power-down bits. Call before
/// `start_play`; blocks for the ramp (~0.75 s at level 1).
pub unsafe fn dac_on(level: u8) {
    DAC_LEVEL = level;
    reg_write32(REG_SPU_DAC_PAR, reg_read32(REG_SPU_DAC_PAR) | 0x30);
    match level {
        3 => reg_write32(REG_SPU_DAC_PAR, reg_read32(REG_SPU_DAC_PAR) & !0x30),
        1 => reg_write32(REG_SPU_DAC_PAR, reg_read32(REG_SPU_DAC_PAR) & !0x20),
        2 => reg_write32(REG_SPU_DAC_PAR, reg_read32(REG_SPU_DAC_PAR) & !0x10),
        _ => {
            reg_write32(REG_SPU_DAC_VOL, reg_read32(REG_SPU_DAC_VOL) & !0x03FF_0000);
            reg_write32(REG_SPU_DAC_PAR, reg_read32(REG_SPU_DAC_PAR) | 0x30);
            return;
        }
    }
    // Power-down bits off in order: P7, P6, P1–4, P5, then P0.
    for mask in [0x0080_0000, 0x0040_0000, 0x001E_0000, 0x0020_0000] {
        reg_write32(REG_SPU_DAC_VOL, reg_read32(REG_SPU_DAC_VOL) & !mask);
        crate::timer::timer_delay_ms(10);
    }
    reg_write32(REG_SPU_DAC_VOL, reg_read32(REG_SPU_DAC_VOL) & !0x0001_0000);
    let wait = match level { 3 => 2200, 1 => 700, _ => 300 };
    crate::timer::timer_delay_ms(wait);
}

/// Power the DAC down (reverse of `dac_on`). Call after `stop_play`.
pub unsafe fn dac_off() {
    crate::timer::timer_delay_ms(10);
    reg_write32(REG_SPU_DAC_VOL, reg_read32(REG_SPU_DAC_VOL) | 0x0001_0000); // P0
    let wait = match DAC_LEVEL { 3 => 1500, 1 => 700, 2 => 400, _ => 0 };
    crate::timer::timer_delay_ms(wait);
    for bits in [0x0020_0000, 0x001E_0000, 0x0040_0000, 0x0080_0000] {
        reg_write32(REG_SPU_DAC_VOL, reg_read32(REG_SPU_DAC_VOL) | bits);
        crate::timer::timer_delay_ms(10);
    }
    reg_write32(REG_SPU_DAC_PAR, reg_read32(REG_SPU_DAC_PAR) | 0x30);
}

/// Enable the threshold / end address interrupts with `cb` as the refill
/// callback and start the engine. Fill the ring buffer first (it plays
/// from its start).
pub unsafe fn start_play(cb: SpuCallback) {
    let _ = enable_int(0, int::TH_ADDRESS | int::END_ADDRESS, Some(cb));
    drv_start_play();
}

/// Disable every channel's refill interrupts and stop the engine.
pub unsafe fn stop_play() {
    for ch in 0..32 {
        let _ = disable_int(ch, int::END_ADDRESS | int::TH_ADDRESS);
    }
    drv_stop_play();
}

/// Stop the engine, mask its interrupt and turn its clocks off.
pub unsafe fn close() {
    crate::aic::aic_disable_interrupt(IRQ_SPU);
    drv_close();
}
