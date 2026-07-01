//! SPU (Sound Processing Unit) driver for N32903 (W55FA93)
//! Ported from DrvSPU.c, SPU.c, DrvSPU.h, and spu.h
//!
//! 32-channel audio DMA engine with equalizer, volume, PAN, and DAC
//! control.  Supports PCM16 (mono/stereo), MDPCM, LP8, and tone
//! generation formats.
//!
//! The SPU reads samples from a ring buffer in SDRAM via DMA.
//! Interrupts fire at half-buffer and end-of-buffer thresholds so the
//! application can refill the consumed half.
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

/// Audio source format
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum SpuSrcType {
    MdPcm = SPU_SRC_MDPCM,
    Lp8 = SPU_SRC_LP8,
    Pcm16 = SPU_SRC_PCM16,
    Tone = SPU_SRC_TONE,
    Pcm16Mono = SPU_SRC_PCM16_MONO,
    Pcm16StereoLeft = SPU_SRC_PCM16_STEREO_L,
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

/// Sample rate presets
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SpuSampleRate {
    Hz48000 = 48000,
    Hz44100 = 44100,
    Hz32000 = 32000,
    Hz24000 = 24000,
    Hz22050 = 22050,
    Hz16000 = 16000,
    Hz12000 = 12000,
    Hz11025 = 11025,
    Hz8000 = 8000,
}

/// SPU interrupt event types (bitmask)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SpuInt(u32);

impl SpuInt {
    pub const THRESHOLD: u32 = TH_FG;
    pub const END_ADDR: u32 = END_FG;
    pub const END_EVENT: u32 = EV_END_FG;
    pub const LOOP_START: u32 = EV_LP_FG;
    pub const SILENT: u32 = EV_SLN_FG;
    pub const USER: u32 = EV_USR_FG;
    pub const ALL: u32 = TH_FG | END_FG | EV_END_FG | EV_LP_FG | EV_SLN_FG | EV_USR_FG;
}

/// SPU interrupt callback
pub type SpuCallback = unsafe extern "C" fn(buf: *const u8);

/// Default fragment size (32 KB) — half-buffer for double-buffering
pub const SPU_FRAG_SIZE: usize = 32 * 1024;
/// Half of fragment size
pub const SPU_HALF_FRAG_SIZE: usize = SPU_FRAG_SIZE / 2;

// ============================================================================
// Static State
// ============================================================================

/// Playback buffer (256-byte aligned)
#[repr(align(256))]
#[allow(dead_code)]
struct PlayBuffer([u8; SPU_FRAG_SIZE]);

static mut PLAY_BUFFER: PlayBuffer = PlayBuffer([0u8; SPU_FRAG_SIZE]);

/// Callbacks per channel per event type (6 event types × 32 channels)
static mut THRESHOLD_CALLBACKS: [Option<SpuCallback>; 32] = [None; 32];
static mut END_ADDR_CALLBACKS: [Option<SpuCallback>; 32] = [None; 32];

// ============================================================================
// Core Hardware Control
// ============================================================================

/// Open the SPU hardware: enable clocks, reset, initialise FIFO.
pub unsafe fn spu_open() {
    // Enable clocks
    reg_write32(REG_AHBCLK, reg_read32(REG_AHBCLK) | ADO_CKE | SPU_CKE | HCLK4_CKE);

    // Disable SPU, set FIFO size = 4
    reg_write32(REG_SPU_CTRL, 0x0400_0000);

    // Reset
    reg_write32(REG_SPU_CTRL, reg_read32(REG_SPU_CTRL) & !SPU_SWRST);
    reg_write32(REG_SPU_CTRL, reg_read32(REG_SPU_CTRL) | SPU_SWRST);
    reg_write32(REG_SPU_CTRL, reg_read32(REG_SPU_CTRL) & !SPU_SWRST);

    // Disable all channels
    reg_write32(REG_SPU_CH_EN, 0);

    // Clear interrupts for all channels
    for ch in 0..32u32 {
        spu_clear_int(ch, SpuInt::ALL);
        spu_disable_int(ch, SpuInt::ALL);
    }
}

/// Close the SPU hardware.
pub unsafe fn spu_close() {
    reg_write32(REG_SPU_CTRL, reg_read32(REG_SPU_CTRL) & !SPU_SWRST);
    reg_write32(REG_SPU_CTRL, reg_read32(REG_SPU_CTRL) | SPU_SWRST);
    reg_write32(REG_SPU_CTRL, reg_read32(REG_SPU_CTRL) & !SPU_SWRST);

    reg_write32(REG_SPU_CTRL, reg_read32(REG_SPU_CTRL) & !SPU_EN);

    reg_write32(REG_AHBCLK, reg_read32(REG_AHBCLK) & !ADO_CKE & !SPU_CKE);
}

/// Enable the SPU (start DMA).
pub unsafe fn spu_enable() {
    reg_write32(REG_SPU_CTRL, reg_read32(REG_SPU_CTRL) | SPU_EN);
}

/// Disable the SPU (stop DMA).
pub unsafe fn spu_disable() {
    reg_write32(REG_SPU_CTRL, reg_read32(REG_SPU_CTRL) & !SPU_EN);
}

/// Start playback on all enabled channels.
pub unsafe fn spu_start_play() {
    reg_write32(REG_SPU_CTRL, reg_read32(REG_SPU_CTRL) | SPU_EN);
}

/// Stop playback.
pub unsafe fn spu_stop_play() {
    reg_write32(REG_SPU_CTRL, reg_read32(REG_SPU_CTRL) & !SPU_EN);
}

// ============================================================================
// Channel Management
// ============================================================================

/// Open (enable) a channel.
pub unsafe fn spu_channel_open(ch: SpuChannel) -> Result<(), ()> {
    if ch >= 32 { return Err(()); }
    reg_write32(REG_SPU_CH_EN, reg_read32(REG_SPU_CH_EN) | (1 << ch));
    Ok(())
}

/// Close (disable) a channel.
pub unsafe fn spu_channel_close(ch: SpuChannel) -> Result<(), ()> {
    if ch >= 32 { return Err(()); }
    reg_write32(REG_SPU_CH_EN, reg_read32(REG_SPU_CH_EN) & !(1 << ch));
    Ok(())
}

/// Check if a channel is enabled.
pub unsafe fn spu_is_channel_enabled(ch: SpuChannel) -> bool {
    reg_read32(REG_SPU_CH_EN) & (1 << ch) != 0
}

/// Select a channel for subsequent register operations.
/// The SPU uses a shared register window — CH_NO selects the active channel.
unsafe fn spu_select_channel(ch: SpuChannel) {
    reg_write32(REG_SPU_CH_CTRL, (reg_read32(REG_SPU_CH_CTRL) & !CH_NO) | (ch << 24));
    reg_write32(REG_SPU_CH_CTRL, reg_read32(REG_SPU_CH_CTRL) | CH_CTRL_LOAD);
    while reg_read32(REG_SPU_CH_CTRL) & CH_FN != 0 {}
}

/// Upload channel settings to hardware.
unsafe fn spu_upload_settings(partial: u32) {
    reg_write32(REG_SPU_CH_CTRL, reg_read32(REG_SPU_CH_CTRL) & !CH_CTRL_UPDATE_ALL_PARTIALS);
    let cmd = if partial != 0 {
        CH_CTRL_UPDATE_PARTIAL | (partial & CH_CTRL_UPDATE_ALL_PARTIALS)
    } else {
        CH_CTRL_UPDATE_ALL
    };
    reg_write32(REG_SPU_CH_CTRL, reg_read32(REG_SPU_CH_CTRL) | cmd);
    while reg_read32(REG_SPU_CH_CTRL) & CH_FN != 0 {}
}

// ============================================================================
// Channel Configuration
// ============================================================================

/// Set the source buffer base (start) address for a channel.
pub unsafe fn spu_set_base_address(ch: SpuChannel, addr: u32) {
    spu_select_channel(ch);
    reg_write32(REG_SPU_S_ADDR, addr);
    spu_upload_settings(UP_DFA);
}

/// Set the threshold (half-buffer) address for a channel.
pub unsafe fn spu_set_threshold_address(ch: SpuChannel, addr: u32) {
    spu_select_channel(ch);
    reg_write32(REG_SPU_M_ADDR, addr);
    spu_upload_settings(0);
}

/// Set the end (wrap) address for a channel.
pub unsafe fn spu_set_end_address(ch: SpuChannel, addr: u32) {
    spu_select_channel(ch);
    reg_write32(REG_SPU_E_ADDR, addr);
    spu_upload_settings(0);
}

/// Get the base address.
pub unsafe fn spu_get_base_address(ch: SpuChannel) -> u32 {
    spu_select_channel(ch);
    reg_read32(REG_SPU_S_ADDR)
}

/// Get the current DMA read address.
pub unsafe fn spu_get_current_address(_ch: SpuChannel) -> u32 {
    reg_read32(REG_SPU_CUR_ADDR)
}

/// Set paused address for PCM16 (auto-pause point).
pub unsafe fn spu_set_pause_address(ch: SpuChannel, addr: u32) {
    spu_select_channel(ch);
    reg_write32(REG_SPU_PA_ADDR, addr);
    spu_upload_settings(UP_PAUSE_ADDR);
}

// ============================================================================
// Audio Parameters
// ============================================================================

/// Set the source data type (PCM16, MDPCM, etc.) for a channel.
pub unsafe fn spu_set_src_type(ch: SpuChannel, src_type: SpuSrcType) {
    spu_select_channel(ch);
    let mut val = reg_read32(REG_SPU_CH_PAR_1);
    val &= !SRC_TYPE;
    val |= src_type as u32 & SRC_TYPE;
    reg_write32(REG_SPU_CH_PAR_1, val);
    spu_upload_settings(0);
}

/// Set channel volume (0–127).
pub unsafe fn spu_set_channel_volume(ch: SpuChannel, volume: u8) {
    spu_select_channel(ch);
    let mut val = reg_read32(REG_SPU_CH_PAR_1);
    val &= !CH_VOL;
    val |= (volume as u32 & 0x7F) << 24;
    reg_write32(REG_SPU_CH_PAR_1, val);
    spu_upload_settings(UP_VOL);
}

/// Get channel volume.
pub unsafe fn spu_get_channel_volume(ch: SpuChannel) -> u8 {
    spu_select_channel(ch);
    ((reg_read32(REG_SPU_CH_PAR_1) & CH_VOL) >> 24) as u8
}

/// Set PAN (left/right balance). Each value is 0–31 (5-bit).
pub unsafe fn spu_set_pan(ch: SpuChannel, left: u8, right: u8) {
    spu_select_channel(ch);
    let mut val = reg_read32(REG_SPU_CH_PAR_1);
    val &= !(PAN_L | PAN_R);
    val |= ((right as u32 & 0x1F) << 16) | ((left as u32 & 0x1F) << 8);
    reg_write32(REG_SPU_CH_PAR_1, val);
    spu_upload_settings(UP_PAN);
}

/// Set DFA (DAC frequency adjustment) value for sample rate conversion.
pub unsafe fn spu_set_dfa(ch: SpuChannel, dfa: u16) {
    spu_select_channel(ch);
    let mut val = reg_read32(REG_SPU_CH_PAR_2);
    val &= !DFA;
    val |= dfa as u32 & DFA;
    reg_write32(REG_SPU_CH_PAR_2, val);
    spu_upload_settings(UP_DFA);
}

/// Compute and set DFA from system clock and sample rate.
pub unsafe fn spu_set_sample_rate(sys_clock_hz: u32, sample_rate: u32) {
    let dfa = ((sys_clock_hz / sample_rate) >> 4) & 0x1FFF;
    reg_write32(REG_SPU_CH_PAR_2, (reg_read32(REG_SPU_CH_PAR_2) & !DFA) | dfa);
}

// ============================================================================
// Global Volume
// ============================================================================

/// Set master headphone volume. Left/right 0–31 (5-bit each, 0x00=mute, 0x1F=max).
pub unsafe fn spu_set_volume(left: u8, right: u8) {
    let mut val = reg_read32(REG_SPU_DAC_VOL);
    val &= !(LHPVL | RHPVL);
    val |= ((left as u32 & 0x1F) << 8) | (right as u32 & 0x1F);
    reg_write32(REG_SPU_DAC_VOL, val);
}

/// Get master volume. Returns (left, right) values 0–31.
pub unsafe fn spu_get_volume() -> (u8, u8) {
    let val = reg_read32(REG_SPU_DAC_VOL);
    (((val & LHPVL) >> 8) as u8, (val & RHPVL) as u8)
}

// ============================================================================
// DAC Power Sequencing (Pop Suppression)
// ============================================================================

/// Turn on the DAC with pop-noise suppression.
///
/// `level` controls the ramp time:
///   1 = ~0.5–1 s, 2 = ~2 s, 3 = ~3 s, 0 = mute
pub unsafe fn spu_dac_on(level: u8) {
    // Disable pop-control
    reg_write32(REG_SPU_DAC_PAR, reg_read32(REG_SPU_DAC_PAR) | 0x30);

    if level == 3 {
        reg_write32(REG_SPU_DAC_PAR, reg_read32(REG_SPU_DAC_PAR) & !0x30);
    } else if level == 1 {
        reg_write32(REG_SPU_DAC_PAR, reg_read32(REG_SPU_DAC_PAR) & !0x20);
    } else if level == 2 {
        reg_write32(REG_SPU_DAC_PAR, reg_read32(REG_SPU_DAC_PAR) & !0x10);
    } else {
        reg_write32(REG_SPU_DAC_VOL, reg_read32(REG_SPU_DAC_VOL) & !0x03FF_0000); // P7
        reg_write32(REG_SPU_DAC_PAR, reg_read32(REG_SPU_DAC_PAR) | 0x30);
        return;
    }

    // Power sequence: P7 → P6 → P1-4 → P5 → P0
    reg_write32(REG_SPU_DAC_VOL, reg_read32(REG_SPU_DAC_VOL) & !0x0800_0000); // P7
    crate::timer::timer_delay_ms(1);
    reg_write32(REG_SPU_DAC_VOL, reg_read32(REG_SPU_DAC_VOL) & !0x0400_0000); // P6
    crate::timer::timer_delay_ms(1);
    reg_write32(REG_SPU_DAC_VOL, reg_read32(REG_SPU_DAC_VOL) & !0x01E0_0000); // P1-4
    crate::timer::timer_delay_ms(1);
    reg_write32(REG_SPU_DAC_VOL, reg_read32(REG_SPU_DAC_VOL) & !0x0200_0000); // P5
    crate::timer::timer_delay_ms(1);
    reg_write32(REG_SPU_DAC_VOL, reg_read32(REG_SPU_DAC_VOL) & !0x0001_0000); // P0

    // Final delay for ramp
    let delay_ms = match level { 3 => 220u32, 1 => 70, 2 => 30, _ => 0 };
    crate::timer::timer_delay_ms(delay_ms);
}

/// Turn off the DAC with pop-noise suppression (reverse sequence).
pub unsafe fn spu_dac_off(level: u8) {
    crate::timer::timer_delay_ms(1);
    reg_write32(REG_SPU_DAC_VOL, reg_read32(REG_SPU_DAC_VOL) | 0x0001_0000); // P0

    let delay_ms = match level { 3 => 150u32, 1 => 70, 2 => 40, _ => 0 };
    crate::timer::timer_delay_ms(delay_ms);

    reg_write32(REG_SPU_DAC_VOL, reg_read32(REG_SPU_DAC_VOL) | 0x0200_0000); // P5
    crate::timer::timer_delay_ms(1);
    reg_write32(REG_SPU_DAC_VOL, reg_read32(REG_SPU_DAC_VOL) | 0x01E0_0000); // P1-4
    crate::timer::timer_delay_ms(1);
    reg_write32(REG_SPU_DAC_VOL, reg_read32(REG_SPU_DAC_VOL) | 0x0400_0000); // P6
    crate::timer::timer_delay_ms(1);
    reg_write32(REG_SPU_DAC_VOL, reg_read32(REG_SPU_DAC_VOL) | 0x0800_0000); // P7
    crate::timer::timer_delay_ms(1);

    reg_write32(REG_SPU_DAC_PAR, reg_read32(REG_SPU_DAC_PAR) | 0x30);
}

// ============================================================================
// Equalizer
// ============================================================================

/// Open the equalizer with a gain setting for a specific band.
pub unsafe fn spu_eq_open(band: SpuEqBand, gain: SpuEqGain) {
    let g = gain as u32 & 0xF;
    match band {
        SpuEqBand::Dc => {
            reg_write32(REG_SPU_EQGain1, (reg_read32(REG_SPU_EQGain1) & !GAINDC) | (g << 16));
        }
        SpuEqBand::Band1 => {
            reg_write32(REG_SPU_EQGain0, (reg_read32(REG_SPU_EQGain0) & !GAIN01) | g);
        }
        SpuEqBand::Band2 => {
            reg_write32(REG_SPU_EQGain0, (reg_read32(REG_SPU_EQGain0) & !GAIN02) | (g << 4));
        }
        SpuEqBand::Band3 => {
            reg_write32(REG_SPU_EQGain0, (reg_read32(REG_SPU_EQGain0) & !GAIN03) | (g << 8));
        }
        SpuEqBand::Band4 => {
            reg_write32(REG_SPU_EQGain0, (reg_read32(REG_SPU_EQGain0) & !GAIN04) | (g << 12));
        }
        SpuEqBand::Band5 => {
            reg_write32(REG_SPU_EQGain0, (reg_read32(REG_SPU_EQGain0) & !GAIN05) | (g << 16));
        }
        SpuEqBand::Band6 => {
            reg_write32(REG_SPU_EQGain0, (reg_read32(REG_SPU_EQGain0) & !GAIN06) | (g << 20));
        }
        SpuEqBand::Band7 => {
            reg_write32(REG_SPU_EQGain0, (reg_read32(REG_SPU_EQGain0) & !GAIN07) | (g << 24));
        }
        SpuEqBand::Band8 => {
            reg_write32(REG_SPU_EQGain0, (reg_read32(REG_SPU_EQGain0) & !GAIN08) | (g << 28));
        }
        SpuEqBand::Band9 => {
            reg_write32(REG_SPU_EQGain1, (reg_read32(REG_SPU_EQGain1) & !GAIN09) | g);
        }
        SpuEqBand::Band10 => {
            reg_write32(REG_SPU_EQGain1, (reg_read32(REG_SPU_EQGain1) & !GAIN10) | (g << 4));
        }
    }
}

/// Disable the equalizer.
pub unsafe fn spu_eq_close() {
    reg_write32(REG_SPU_EQGain0, 0);
    reg_write32(REG_SPU_EQGain1, 0);
}

// ============================================================================
// Interrupt Control
// ============================================================================

/// Enable an interrupt event for a channel.
pub unsafe fn spu_enable_int(ch: SpuChannel, int_mask: u32, _callback: Option<SpuCallback>) {
    spu_select_channel(ch);
    let mut ev = reg_read32(REG_SPU_CH_EVENT);
    // Map flag bits to enable bits (flag << (enable_pos - flag_pos))
    if int_mask & TH_FG != 0 { ev |= TH_EN; }
    if int_mask & END_FG != 0 { ev |= END_EN; }
    if int_mask & EV_END_FG != 0 { ev |= EV_END_EN; }
    if int_mask & EV_LP_FG != 0 { ev |= EV_LP_EN; }
    if int_mask & EV_SLN_FG != 0 { ev |= EV_SLN_EN; }
    if int_mask & EV_USR_FG != 0 { ev |= EV_USR_EN; }
    reg_write32(REG_SPU_CH_EVENT, ev);
    spu_upload_settings(UP_IRQ);

    // Save callback
    if int_mask & TH_FG != 0 { THRESHOLD_CALLBACKS[ch as usize] = _callback; }
    if int_mask & END_FG != 0 { END_ADDR_CALLBACKS[ch as usize] = _callback; }
}

/// Disable an interrupt event for a channel.
pub unsafe fn spu_disable_int(ch: SpuChannel, int_mask: u32) {
    spu_select_channel(ch);
    let mut ev = reg_read32(REG_SPU_CH_EVENT);
    if int_mask & TH_FG != 0 { ev &= !TH_EN; THRESHOLD_CALLBACKS[ch as usize] = None; }
    if int_mask & END_FG != 0 { ev &= !END_EN; END_ADDR_CALLBACKS[ch as usize] = None; }
    if int_mask & EV_END_FG != 0 { ev &= !EV_END_EN; }
    if int_mask & EV_LP_FG != 0 { ev &= !EV_LP_EN; }
    if int_mask & EV_SLN_FG != 0 { ev &= !EV_SLN_EN; }
    if int_mask & EV_USR_FG != 0 { ev &= !EV_USR_EN; }
    reg_write32(REG_SPU_CH_EVENT, ev);
    spu_upload_settings(UP_IRQ);
}

/// Clear interrupt flags for a channel.
pub unsafe fn spu_clear_int(ch: SpuChannel, int_mask: u32) {
    spu_select_channel(ch);
    let ev = reg_read32(REG_SPU_CH_EVENT);
    reg_write32(REG_SPU_CH_EVENT, ev & !(int_mask & SpuInt::ALL));
}

/// Poll interrupt flags for a channel.
pub unsafe fn spu_poll_int(ch: SpuChannel, int_mask: u32) -> bool {
    spu_select_channel(ch);
    reg_read32(REG_SPU_CH_EVENT) & int_mask != 0
}

// ============================================================================
// SPU ISR (call from interrupt handler)
// ============================================================================

/// SPU interrupt handler — dispatches per-channel callbacks.
///
/// Install with `aic_install_isr(Irq1, IRQ_SPU, spu_isr)`.
pub unsafe extern "C" fn spu_isr() {
    let mut ch_mask: u32 = 1;

    for ch in 0u32..32 {
        if reg_read32(REG_SPU_CH_IRQ) & ch_mask != 0 {
            // Wait for channel function done
            while reg_read32(REG_SPU_CH_CTRL) & CH_FN != 0 {}

            // Select channel and load settings
            reg_write32(REG_SPU_CH_CTRL, (reg_read32(REG_SPU_CH_CTRL) & !CH_NO) | (ch << 24));
            reg_write32(REG_SPU_CH_CTRL, reg_read32(REG_SPU_CH_CTRL) | CH_CTRL_LOAD);
            while reg_read32(REG_SPU_CH_CTRL) & CH_FN != 0 {}

            let flags = reg_read32(REG_SPU_CH_EVENT);
            // Ack by re-writing
            reg_write32(REG_SPU_CH_EVENT, reg_read32(REG_SPU_CH_EVENT));

            // Threshold address hit → refill first half
            if flags & TH_FG != 0 && reg_read32(REG_SPU_CH_EVENT) & TH_EN != 0 {
                if let Some(cb) = THRESHOLD_CALLBACKS[ch as usize] {
                    cb(core::ptr::null());
                }
            }
            // End address hit → refill second half
            if flags & END_FG != 0 && reg_read32(REG_SPU_CH_EVENT) & END_EN != 0 {
                if let Some(cb) = END_ADDR_CALLBACKS[ch as usize] {
                    cb(core::ptr::null());
                }
            }

            // Update IRQ partial to clear in hardware
            reg_write32(REG_SPU_CH_CTRL, reg_read32(REG_SPU_CH_CTRL) & !CH_CTRL_UPDATE_ALL_PARTIALS);
            reg_write32(
                REG_SPU_CH_CTRL,
                reg_read32(REG_SPU_CH_CTRL) | UP_IRQ | CH_CTRL_UPDATE_PARTIAL,
            );
            while reg_read32(REG_SPU_CH_CTRL) & CH_FN != 0 {}
        }
        ch_mask <<= 1;
    }

    // Clear all channel IRQ flags
    reg_write32(REG_SPU_CH_IRQ, reg_read32(REG_SPU_CH_IRQ));
}

// ============================================================================
// High-Level Convenience: Double-Buffered Playback
// ============================================================================

/// Get a pointer to the 256-byte-aligned 32 KB playback buffer.
pub unsafe fn spu_get_play_buffer() -> *mut u8 {
    (&raw mut PLAY_BUFFER) as *mut u8
}

/// Configure channel 0 for stereo PCM16 double-buffered playback.
///
/// Sets up base/threshold/end addresses within the play buffer,
/// sample rate, volume, PAN, and installs the SPU ISR.
pub unsafe fn spu_configure_playback(
    sample_rate: SpuSampleRate,
    volume: u8,
    callback: SpuCallback,
) {
    let buf_addr = spu_get_play_buffer() as u32 | 0x8000_0000;

    // Zero the buffer
    core::ptr::write_bytes(spu_get_play_buffer(), 0, SPU_FRAG_SIZE);

    spu_disable();
    spu_open();

    spu_set_pan(0, 0x1F, 0x1F); // center
    spu_set_channel_volume(0, volume);
    spu_set_channel_volume(1, volume);

    spu_channel_open(0).ok();

    // Compute DFA from system clock
    let sys_clk = crate::clock::get_clock(crate::clock::ClockNode::System) * 1000;
    spu_set_sample_rate(sys_clk, sample_rate as u32);

    // Stereo PCM16: ch0 = left, ch1 = right
    spu_set_src_type(0, SpuSrcType::Pcm16StereoLeft);

    spu_set_base_address(0, buf_addr);
    spu_set_base_address(1, buf_addr);

    spu_set_threshold_address(0, buf_addr + SPU_HALF_FRAG_SIZE as u32);
    spu_set_threshold_address(1, buf_addr + SPU_HALF_FRAG_SIZE as u32);

    spu_set_end_address(0, buf_addr + SPU_FRAG_SIZE as u32);
    spu_set_end_address(1, buf_addr + SPU_FRAG_SIZE as u32);

    // Install ISR
    crate::aic::aic_install_isr(
        crate::aic::IntLevel::Irq1,
        IRQ_SPU,
        spu_isr as unsafe extern "C" fn(),
    );
    crate::aic::aic_set_local_interrupt(crate::aic::LocalIntState::EnableIrq);
    crate::aic::aic_enable_interrupt(IRQ_SPU);

    // Enable threshold + end-address interrupts on ch0
    spu_enable_int(0, TH_FG | END_FG, Some(callback));
}
