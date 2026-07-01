//! VPOST (Video Post-Processor / LCD Controller) driver for N32903 (W55FA93)
//! Ported from W55FA93_VPOST.h, W55FA93_VPOST_Driver.c, and W55FA93_reg.h
//!
//! Provides base hardware control for the LCD controller and TV encoder.
//! Panel-specific initialisation is **not** included — instantiate
//! [`SyncLcmConfig`] or [`TvConfig`] with your panel's timing parameters.
//!
//! # Safety
//!
//! All functions are `unsafe` — they manipulate hardware registers directly.

use crate::registers::*;

// ============================================================================
// Public Types
// ============================================================================

/// LCD device type
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum LcdType {
    /// High-resolution sync type (16/18/24-bit parallel)
    HighResSync = 0,
    /// 8-bit serial sync type TFT
    SyncTft = 1,
    /// Sync-type color STN
    SyncStn = 2,
    /// MPU-type LCD
    Mpu = 3,
}

/// LCD data interface (for 8-bit serial sync LCM)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum LcdDataInterface {
    /// CCIR601 YUV422
    Yuv422 = 0,
    /// RGB dummy serial
    RgbDummy = 1,
    /// CCIR656
    Ccir656 = 2,
    /// RGB through mode
    RgbThrough = 4,
}

/// Parallel RGB data bus width
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum ParallelRgbBus {
    Bits16 = 0,
    Bits18 = 1,
    Bits24 = 2,
}

/// Frame buffer pixel format
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum FrameBufferFormat {
    Rgb555 = 0,
    Rgb565 = 1,
    Rgbx888 = 2,
    Rgb888x = 3,
    CbYCrY = 4,
    YCbYCr = 5,
    CrYCbY = 6,
    YCrYCb = 7,
}

/// Image source for LCD output
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum ImageSource {
    LineBuffer = 0,
    FrameBuffer = 1,
    RegisterColor = 2,
    ColorBar = 3,
}

/// TV system selection
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TvSystem {
    Ntsc = 0,
    Pal = 1,
}

/// Frame buffer size for TV non-interlace mode
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum TvFrameBufferSize {
    Qvga = 0, // 320x240
    Hvga = 1, // 640x240
    Vga = 2,  // 640x480
    D1 = 3,   // 720x480
}

/// Horizontal timing parameters for sync-type LCM
#[derive(Clone, Copy, Debug)]
pub struct HTiming {
    /// HSYNC pulse width (in pixel clocks, 1–256)
    pub pulse_width: u8,
    /// Horizontal back porch (in pixel clocks, 1–256)
    pub back_porch: u8,
    /// Horizontal front porch (in pixel clocks, 1–256)
    pub front_porch: u8,
}

/// Vertical timing parameters for sync-type LCM
#[derive(Clone, Copy, Debug)]
pub struct VTiming {
    /// VSYNC pulse width (in lines, 1–256)
    pub pulse_width: u8,
    /// Vertical back porch (in lines, 1–256)
    pub back_porch: u8,
    /// Vertical front porch (in lines, 1–256)
    pub front_porch: u8,
}

/// Display window / resolution
#[derive(Clone, Copy, Debug)]
pub struct DisplayWindow {
    /// Total clocks per line (PPL + porches + sync width)
    pub clock_per_line: u16,
    /// Active lines per panel
    pub line_per_panel: u16,
    /// Active pixels per line
    pub pixel_per_line: u16,
}

/// Signal polarity configuration
#[derive(Clone, Copy, Debug)]
pub struct SignalPolarity {
    /// true = active low, false = active high
    pub vsync_active_low: bool,
    /// true = active low, false = active high
    pub hsync_active_low: bool,
    /// true = active low, false = active high
    pub vden_active_low: bool,
    /// true = rising edge, false = falling edge
    pub dclk_rising_edge: bool,
}

impl Default for SignalPolarity {
    fn default() -> Self {
        Self {
            vsync_active_low: false,
            hsync_active_low: false,
            vden_active_low: false,
            dclk_rising_edge: true,
        }
    }
}

/// Complete configuration for a sync-type (TFT) LCM.
///
/// This is the main configuration struct for driving a parallel or serial
/// RGB LCD panel.  Fill in the fields with your panel's datasheet values
/// and call [`vpost_configure_sync_lcm`].
#[derive(Clone, Debug)]
pub struct SyncLcmConfig {
    /// LCD type (HighResSync or SyncTft)
    pub lcd_type: LcdType,
    /// Data bus interface
    pub data_interface: LcdDataInterface,
    /// Parallel bus width (for HighResSync type)
    pub parallel_bus: ParallelRgbBus,
    /// Frame buffer pixel format
    pub fb_format: FrameBufferFormat,
    /// Image source
    pub image_source: ImageSource,
    /// Horizontal timing
    pub h_timing: HTiming,
    /// Vertical timing
    pub v_timing: VTiming,
    /// Display resolution
    pub window: DisplayWindow,
    /// Signal polarities
    pub polarity: SignalPolarity,
    /// YUV big-endian flag
    pub yuv_big_endian: bool,
    /// Frame buffer base address (physical)
    pub fb_address: u32,
}

/// TV encoder configuration
#[derive(Clone, Debug)]
pub struct TvConfig {
    /// TV system (NTSC or PAL)
    pub system: TvSystem,
    /// Interlace mode
    pub interlace: bool,
    /// Frame buffer size
    pub fb_size: TvFrameBufferSize,
    /// LCD image source
    pub lcd_source: ImageSource,
    /// TV image source
    pub tv_source: ImageSource,
    /// Enable notch filter
    pub notch_filter: bool,
    /// Enable TV DAC
    pub dac_enable: bool,
    /// Color modulation clock: true = 27 MHz, false = 13.5 MHz
    pub cmm_27mhz: bool,
}

impl Default for TvConfig {
    fn default() -> Self {
        Self {
            system: TvSystem::Ntsc,
            interlace: true,
            fb_size: TvFrameBufferSize::Qvga,
            lcd_source: ImageSource::FrameBuffer,
            tv_source: ImageSource::FrameBuffer,
            notch_filter: true,
            dac_enable: true,
            cmm_27mhz: false,
        }
    }
}

/// VPOST interrupt source
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VpostInt {
    Hsync = 0,
    Vsync = 1,
    TvField = 2,
    MpuComplete = 3,
}

/// LCD data bus pin width selection
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DataBusWidth {
    Bits8 = 0,
    Bits9 = 1,
    Bits16 = 2,
    Bits18 = 3,
    Bits24 = 4,
}

// ============================================================================
// Clock & Reset
// ============================================================================

/// Enable the VPOST engine clock.
pub unsafe fn vpost_clock_enable() {
    reg_write32(REG_AHBCLK, reg_read32(REG_AHBCLK) | VPOST_CKE);
}

/// Disable the VPOST engine clock.
pub unsafe fn vpost_clock_disable() {
    reg_write32(REG_AHBCLK, reg_read32(REG_AHBCLK) & !VPOST_CKE);
}

/// Reset the VPOST engine.
pub unsafe fn vpost_reset() {
    reg_write32(REG_APBIPRST, reg_read32(REG_APBIPRST) | VPOSTRST);
    reg_write32(REG_APBIPRST, reg_read32(REG_APBIPRST) & !VPOSTRST);
}

// ============================================================================
// LCD Controller Control
// ============================================================================

/// Start the LCD controller (begin scanning out the frame buffer).
pub unsafe fn vpost_lcd_start() {
    reg_write32(REG_LCM_LCDCCtl, reg_read32(REG_LCM_LCDCCtl) | LCDCCtl_LCDRUN);
}

/// Stop the LCD controller.
pub unsafe fn vpost_lcd_stop() {
    reg_write32(REG_LCM_LCDCCtl, reg_read32(REG_LCM_LCDCCtl) & !LCDCCtl_LCDRUN);
}

/// Set LCD enable flags: YUV endian, frame buffer format, and run state.
pub unsafe fn vpost_set_lcd_enable(yuv_big_endian: bool, fb_format: FrameBufferFormat, run: bool) {
    let mut val = reg_read32(REG_LCM_LCDCCtl);
    val &= !(LCDCCtl_YUVBL | LCDCCtl_FBDS | LCDCCtl_LCDRUN);
    if yuv_big_endian {
        val |= LCDCCtl_YUVBL;
    }
    val |= ((fb_format as u32) << 1) & LCDCCtl_FBDS;
    if run {
        val |= LCDCCtl_LCDRUN;
    }
    reg_write32(REG_LCM_LCDCCtl, val);
}

/// Set the LCD type and data interface.
pub unsafe fn vpost_set_lcd_config(lcd_type: LcdType, data_if: LcdDataInterface) {
    let mut val = reg_read32(REG_LCM_LCDCPrm);
    val &= !(LCDCPrm_LCDTYPE | LCDCPrm_LCDDataSel);
    val |= (lcd_type as u32) & LCDCPrm_LCDTYPE;
    val |= ((data_if as u32) << 2) & LCDCPrm_LCDDataSel;
    reg_write32(REG_LCM_LCDCPrm, val);
}

/// Set parallel RGB bus width (for HighResSync LCD type).
pub unsafe fn vpost_set_parallel_bus(bus: ParallelRgbBus) {
    let mut val = reg_read32(REG_LCM_LCDCCtl);
    val &= !LCDCCtl_PRDB_SEL;
    val |= ((bus as u32) << 20) & LCDCCtl_PRDB_SEL;
    reg_write32(REG_LCM_LCDCCtl, val);
}

/// Set the frame buffer base address.
pub unsafe fn vpost_set_fb_address(addr: u32) {
    reg_write32(REG_LCM_FSADDR, addr);
}

/// Set the image source for the LCD path.
pub unsafe fn vpost_set_image_source(source: ImageSource) {
    let mut val = reg_read32(REG_LCM_TVCtl);
    val &= !TVCtl_LCDSrc;
    val |= ((source as u32) << 10) & TVCtl_LCDSrc;
    reg_write32(REG_LCM_TVCtl, val);
}

// ============================================================================
// Timing Configuration (Sync LCM)
// ============================================================================

/// Configure horizontal timing (HSYNC pulse, back porch, front porch).
/// Values are written as (n-1) to match the hardware convention.
pub unsafe fn vpost_set_h_timing(timing: &HTiming) {
    let pw = timing.pulse_width.wrapping_sub(1);
    let bp = timing.back_porch.wrapping_sub(1);
    let fp = timing.front_porch.wrapping_sub(1);

    let mut val = reg_read32(REG_LCM_TCON1);
    val &= !(TCON1_HSPW | TCON1_HBPD | TCON1_HFPD);
    val |= ((pw as u32) << 16) & TCON1_HSPW;
    val |= ((bp as u32) << 8) & TCON1_HBPD;
    val |= (fp as u32) & TCON1_HFPD;
    reg_write32(REG_LCM_TCON1, val);
}

/// Configure vertical timing (VSYNC pulse, back porch, front porch).
/// Values are written as (n-1).
pub unsafe fn vpost_set_v_timing(timing: &VTiming) {
    let pw = timing.pulse_width.wrapping_sub(1);
    let bp = timing.back_porch.wrapping_sub(1);
    let fp = timing.front_porch.wrapping_sub(1);

    let mut val = reg_read32(REG_LCM_TCON2);
    val &= !(TCON2_VSPW | TCON2_VBPD | TCON2_VFPD);
    val |= ((pw as u32) << 16) & TCON2_VSPW;
    val |= ((bp as u32) << 8) & TCON2_VBPD;
    val |= (fp as u32) & TCON2_VFPD;
    reg_write32(REG_LCM_TCON2, val);
}

/// Configure the display window (resolution).
/// clock_per_line is written as (n-1).
pub unsafe fn vpost_set_display_window(win: &DisplayWindow) {
    let cpl = if win.clock_per_line > 0 {
        win.clock_per_line - 1
    } else {
        0
    };

    let mut val = reg_read32(REG_LCM_TCON3);
    val &= !(TCON3_PPL | TCON3_LPP);
    val |= ((cpl as u32) << 16) & TCON3_PPL;
    val |= (win.line_per_panel as u32) & TCON3_LPP;
    reg_write32(REG_LCM_TCON3, val);

    // Total active pixels
    let mut tcon4 = reg_read32(REG_LCM_TCON4);
    tcon4 &= !TCON4_TAPN;
    tcon4 |= ((win.pixel_per_line as u32 & 0x7FF) << 16) & TCON4_TAPN;
    reg_write32(REG_LCM_TCON4, tcon4);
}

/// Configure signal polarities (VSYNC, HSYNC, VDEN, pixel clock).
pub unsafe fn vpost_set_signal_polarity(pol: &SignalPolarity) {
    let mut val = reg_read32(REG_LCM_TCON4);
    val &= !(TCON4_VSP | TCON4_HSP | TCON4_DEP | TCON4_PCLKP);
    if pol.vsync_active_low {
        val |= TCON4_VSP;
    }
    if pol.hsync_active_low {
        val |= TCON4_HSP;
    }
    if pol.vden_active_low {
        val |= TCON4_DEP;
    }
    if pol.dclk_rising_edge {
        val |= TCON4_PCLKP;
    }
    reg_write32(REG_LCM_TCON4, val);
}

// ============================================================================
// TV Encoder
// ============================================================================

/// Configure the TV encoder.
pub unsafe fn vpost_configure_tv(cfg: &TvConfig) {
    let mut val = reg_read32(REG_LCM_TVCtl);
    val &= !(TVCtl_FBSIZE | TVCtl_LCDSrc | TVCtl_TvSrc | TVCtl_NotchE
           | TVCtl_Tvdac | TVCtl_TvInter | TVCtl_TvSys | TVCtl_TvSleep | TVCtl_TvCMM);

    val |= ((cfg.fb_size as u32) << 14) & TVCtl_FBSIZE;
    val |= ((cfg.lcd_source as u32) << 10) & TVCtl_LCDSrc;
    val |= ((cfg.tv_source as u32) << 8) & TVCtl_TvSrc;
    if cfg.notch_filter {
        val |= TVCtl_NotchE;
    }
    if cfg.dac_enable {
        val |= TVCtl_Tvdac;
    }
    if cfg.interlace {
        val |= TVCtl_TvInter;
    }
    if cfg.system == TvSystem::Pal {
        val |= TVCtl_TvSys;
    }
    if cfg.cmm_27mhz {
        val |= TVCtl_TvCMM;
    }
    // Enable TV encoder (TvSleep=0 means enabled)
    // val already has TvSleep cleared

    reg_write32(REG_LCM_TVCtl, val);
}

/// Enable the TV encoder.
pub unsafe fn vpost_tv_enable() {
    reg_write32(REG_LCM_TVCtl, reg_read32(REG_LCM_TVCtl) & !TVCtl_TvSleep);
}

/// Disable (sleep) the TV encoder.
pub unsafe fn vpost_tv_disable() {
    reg_write32(REG_LCM_TVCtl, reg_read32(REG_LCM_TVCtl) | TVCtl_TvSleep);
}

/// Read the current TV field (odd/even) status.
pub unsafe fn vpost_tv_field() -> bool {
    reg_read32(REG_LCM_TVCtl) & TVCtl_TvField != 0
}

// ============================================================================
// Data Bus Pin Configuration
// ============================================================================

/// Configure GPIO multifunction pins for the LCD data bus.
/// Selects the appropriate GPB/GPC/GPD/GPE function bits for the
/// requested bus width.  Includes LPCLK, HSYNC, VSYNC, and VDEN pins.
pub unsafe fn vpost_set_data_bus_pins(width: DataBusWidth) {
    // LPCLK on GPB31
    reg_write32(REG_GPBFUN, reg_read32(REG_GPBFUN) | 0xC000_0000);

    match width {
        DataBusWidth::Bits8 => {
            // LVDATA[7:0] on GPC[7:0]
            reg_write32(REG_GPCFUN, reg_read32(REG_GPCFUN) | 0x0000_FFFF);
            // HSYNC, VSYNC, VDEN on GPD[23:18]
            reg_write32(REG_GPDFUN, reg_read32(REG_GPDFUN) | 0x00FC_0000);
        }
        DataBusWidth::Bits9 => {
            reg_write32(REG_GPCFUN, reg_read32(REG_GPCFUN) | 0x0003_FFFF);
            reg_write32(REG_GPDFUN, reg_read32(REG_GPDFUN) | 0x00FC_0000);
        }
        DataBusWidth::Bits16 => {
            reg_write32(REG_GPCFUN, reg_read32(REG_GPCFUN) | 0xFFFF_FFFF);
            reg_write32(REG_GPDFUN, reg_read32(REG_GPDFUN) | 0x00FC_0000);
        }
        DataBusWidth::Bits18 => {
            reg_write32(REG_GPCFUN, reg_read32(REG_GPCFUN) | 0xFFFF_FFFF);
            reg_write32(REG_GPDFUN, reg_read32(REG_GPDFUN) | 0x00FC_0000);
            // LVDATA[17:16] on GPE[3:0]
            reg_write32(REG_GPEFUN, reg_read32(REG_GPEFUN) | 0x0000_000F);
        }
        DataBusWidth::Bits24 => {
            reg_write32(REG_GPCFUN, reg_read32(REG_GPCFUN) | 0xFFFF_FFFF);
            reg_write32(REG_GPDFUN, reg_read32(REG_GPDFUN) | 0x00FC_0000);
            reg_write32(REG_GPEFUN, reg_read32(REG_GPEFUN) | 0x0000_000F);
            // LVDATA[23:18] on GPB[23:18]
            reg_write32(
                REG_GPBFUN,
                (reg_read32(REG_GPBFUN) & !0x03FF_C000) | 0x02AA_8000,
            );
        }
    }
}

// ============================================================================
// Interrupt Control
// ============================================================================

/// Convert VpostInt variant to the corresponding register bit masks.
fn int_masks(int: VpostInt) -> (u32, u32) {
    match int {
        VpostInt::Hsync => (LCDCInt_HINTEN, LCDCInt_HINT),
        VpostInt::Vsync => (LCDCInt_VINTEN, LCDCInt_VINT),
        VpostInt::TvField => (LCDCInt_TVFIELDINTEN, LCDCInt_TVFIELDINT),
        VpostInt::MpuComplete => (LCDCInt_MPUCPLINTEN, LCDCInt_MPUCPL),
    }
}

/// Enable a VPOST interrupt.
pub unsafe fn vpost_int_enable(int: VpostInt) {
    let (ien, _) = int_masks(int);
    reg_write32(REG_LCM_LCDCInt, reg_read32(REG_LCM_LCDCInt) | ien);
}

/// Disable a VPOST interrupt.
pub unsafe fn vpost_int_disable(int: VpostInt) {
    let (ien, _) = int_masks(int);
    reg_write32(REG_LCM_LCDCInt, reg_read32(REG_LCM_LCDCInt) & !ien);
}

/// Clear a VPOST interrupt flag.
pub unsafe fn vpost_int_clear(int: VpostInt) {
    let (_, flag) = int_masks(int);
    reg_write32(REG_LCM_LCDCInt, reg_read32(REG_LCM_LCDCInt) & !flag);
}

/// Check if a VPOST interrupt is enabled.
pub unsafe fn vpost_int_is_enabled(int: VpostInt) -> bool {
    let (ien, _) = int_masks(int);
    reg_read32(REG_LCM_LCDCInt) & ien != 0
}

/// Read all pending interrupt flags.
pub unsafe fn vpost_int_flags() -> u32 {
    reg_read32(REG_LCM_LCDCInt) & 0x0000_001F
}

// ============================================================================
// Background / Register Color
// ============================================================================

/// Set the background color drawn when the image source is RegisterColor.
pub unsafe fn vpost_set_background_color(r: u8, g: u8, b: u8) {
    let val = ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
    reg_write32(REG_LCM_COLORSET, val);
}

// ============================================================================
// Convenience: Full Sync LCM Configuration
// ============================================================================

/// Apply a complete sync LCM configuration in one call.
///
/// This is the primary entry point for setting up a TFT LCD panel.
/// It does **not** handle panel-specific power sequencing or
/// initialisation commands — those belong in panel-specific code.
pub unsafe fn vpost_configure_sync_lcm(cfg: &SyncLcmConfig) {
    // Clock and reset
    vpost_clock_enable();
    vpost_reset();

    // LCD type and data interface
    vpost_set_lcd_config(cfg.lcd_type, cfg.data_interface);

    // Parallel bus width
    if matches!(cfg.lcd_type, LcdType::HighResSync) {
        vpost_set_parallel_bus(cfg.parallel_bus);
    }

    // Timing
    vpost_set_h_timing(&cfg.h_timing);
    vpost_set_v_timing(&cfg.v_timing);
    vpost_set_display_window(&cfg.window);
    vpost_set_signal_polarity(&cfg.polarity);

    // Data format and source
    vpost_set_lcd_enable(cfg.yuv_big_endian, cfg.fb_format, false);
    vpost_set_image_source(cfg.image_source);

    // Frame buffer
    vpost_set_fb_address(cfg.fb_address);
}

// ============================================================================
// Convenience: TV Output Configuration
// ============================================================================

/// Apply a complete TV encoder configuration and enable output.
pub unsafe fn vpost_configure_tv_full(cfg: &TvConfig, fb_addr: u32) {
    vpost_clock_enable();
    vpost_reset();

    vpost_configure_tv(cfg);
    vpost_set_fb_address(fb_addr);

    // Set frame buffer format to YCbYCr for TV
    vpost_set_lcd_enable(false, FrameBufferFormat::YCbYCr, false);

    vpost_tv_enable();
}
