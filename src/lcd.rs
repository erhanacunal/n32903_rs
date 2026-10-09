//! Parallel RGB LCD (VPOST) for Nextion N32903 boards.
//!
//! Register sequence taken from the Nextion firmware's `VideoMode_Setup`
//! for its 800×480 panel modes:
//!
//! - VPOST + HCLK4 clocks on, VPOST reset (AHBIPRST bit 10)
//! - pixel clock from UPLL (CLKDIV1)
//! - pins: LPCLK (GPB15), LVDATA (GPC0–15, GPE0–1), HSYNC/VSYNC/VDEN (GPD9–11)
//! - sync "high resolution" LCD, 16-bit bus, RGB565 frame buffer
//! - timing `TCON1 = 0x002F27A7`, `TCON2 = 0x00011613`
//!
//! The firmware feeds the frame buffer address through its frame switch
//! controller; here the address goes straight into `LCM_FSADDR`.
//!
//! # Double buffering
//!
//! With two buffers ([`Lcd::init_double`]) all drawing goes to the hidden
//! back buffer; [`Lcd::present`] writes the D-cache back, switches the
//! controller to that buffer and waits for VSYNC, after which the old
//! front buffer becomes the new back buffer.  Nothing is drawn while it is
//! on screen.
//!
//! Drawing works through the D-cache (buffers in cacheable SDRAM, see
//! [`crate::mmu`]); `present` cleans the cache so the LCD controller, which
//! reads SDRAM directly, sees every pixel.  Without the MMU/caches on it
//! works the same, only slower.
//!
//! ```rust,ignore
//! let mut lcd = unsafe { Lcd::init_double(fb0, fb1, &lcd::NEXTION_800X480) };
//! lcd.fill(lcd::rgb565(255, 0, 0));
//! unsafe { lcd.present() };
//! ```

use crate::cache;
use crate::clock::{self, ClockNode};
use crate::registers::*;

/// Panel timing.  Porch and sync values are raw register fields, exactly as
/// the firmware writes them.
#[derive(Clone, Copy, Debug)]
pub struct PanelTiming {
    pub width: u16,
    pub height: u16,
    pub h_front_porch: u8,
    pub h_back_porch: u8,
    pub h_sync_width: u8,
    pub v_front_porch: u8,
    pub v_back_porch: u8,
    pub v_sync_width: u8,
    /// TCON4 polarity bits: VSP (bit 3) / HSP (bit 2) / DEP (bit 1) set =
    /// active high, PCLKP (bit 0) set = data on the falling edge
    pub polarity: u32,
    /// Wanted pixel clock; the closest UPLL division at or below it is used
    pub pixel_clock_khz: u32,
}

/// 800×480 panel as set up by the Nextion firmware (modes 0x441/0x501).
/// Modes 0x701/0x801/0x901 use the same timing with polarity `0x3`.
pub const NEXTION_800X480: PanelTiming = PanelTiming {
    width: 800,
    height: 480,
    h_front_porch: 0xA7,
    h_back_porch: 0x27,
    h_sync_width: 0x2F,
    v_front_porch: 0x13,
    v_back_porch: 0x16,
    v_sync_width: 0x01,
    polarity: 0xA, // VSYNC and DE active high
    pixel_clock_khz: 33_000,
};

/// RGB565 colour from 8-bit components.
pub const fn rgb565(r: u8, g: u8, b: u8) -> u16 {
    ((r as u16 & 0xF8) << 8) | ((g as u16 & 0xFC) << 3) | (b as u16 >> 3)
}

/// The LCD and its RGB565 frame buffer(s).
pub struct Lcd {
    /// Frame buffers; both entries are the same buffer when single-buffered
    fb: [*mut u16; 2],
    /// Index of the buffer being drawn (not shown when double-buffered)
    draw: usize,
    width: usize,
    height: usize,
    pixel_clock_khz: u32,
}

impl Lcd {
    /// Frame buffer size in bytes for `timing`.
    pub const fn frame_buffer_size(timing: &PanelTiming) -> usize {
        timing.width as usize * timing.height as usize * 2
    }

    /// Single-buffered: the LCD shows `fb_addr` and drawing goes straight
    /// to it (visible while it happens).  Call [`Lcd::present`] after
    /// drawing when the D-cache is on.
    ///
    /// # Safety
    /// See [`Lcd::init_double`].
    pub unsafe fn init(fb_addr: u32, timing: &PanelTiming) -> Self {
        Self::init_double(fb_addr, fb_addr, timing)
    }

    /// Double-buffered: the LCD starts on `front`, drawing goes to `back`
    /// until [`Lcd::present`] swaps them.
    ///
    /// # Safety
    /// Both addresses must point to RAM reserved for a frame buffer
    /// (`frame_buffer_size` bytes, word aligned).  Takes over the VPOST
    /// controller and the LCD pins.
    pub unsafe fn init_double(front: u32, back: u32, timing: &PanelTiming) -> Self {
        // Clocks on, VPOST reset
        reg_write32(REG_AHBCLK, reg_read32(REG_AHBCLK) | VPOST_CKE | HCLK4_CKE);
        reg_write32(REG_AHBIPRST, reg_read32(REG_AHBIPRST) | VPOSTRST);
        reg_write32(REG_AHBIPRST, reg_read32(REG_AHBIPRST) & !VPOSTRST);

        // Pixel clock = UPLL / (N1 + 1); source UPLL (bits 4:3 = 3), N0 = 0
        let upll_khz = clock::get_clock(ClockNode::UPll);
        let div = ((upll_khz + timing.pixel_clock_khz - 1) / timing.pixel_clock_khz).clamp(1, 256);
        reg_write32(
            REG_CLKDIV1,
            (reg_read32(REG_CLKDIV1) & !0xFF1F) | ((div - 1) << 8) | (3 << 3),
        );

        reg_write32(REG_LCM_LCDCCtl, reg_read32(REG_LCM_LCDCCtl) & !LCDCCtl_LCDRUN);

        // LPCLK, LVDATA[17:0], HSYNC/VSYNC/VDEN
        reg_write32(REG_GPBFUN, reg_read32(REG_GPBFUN) | 0xC000_0000);
        reg_write32(REG_GPCFUN, 0xFFFF_FFFF);
        reg_write32(REG_GPDFUN, reg_read32(REG_GPDFUN) | 0x00FC_0000);
        reg_write32(REG_GPEFUN, reg_read32(REG_GPEFUN) | 0x0000_000F);

        // RGB565 frame buffer, YUV little endian, 16-bit bus, stopped
        reg_write32(REG_LCM_LCDCCtl, LCDCCtl_YUVBL | (1 << 1));
        // High-resolution sync LCD, not synchronised to the TV timing
        reg_write32(REG_LCM_LCDCPrm, reg_read32(REG_LCM_LCDCPrm) & !0x103);
        // LCD image source: frame buffer
        reg_write32(REG_LCM_TVCtl, (reg_read32(REG_LCM_TVCtl) & !TVCtl_LCDSrc) | (1 << 10));

        let w = timing.width as u32;
        let h = timing.height as u32;
        reg_write32(
            REG_LCM_TCON1,
            (timing.h_sync_width as u32) << 16
                | (timing.h_back_porch as u32) << 8
                | timing.h_front_porch as u32,
        );
        reg_write32(
            REG_LCM_TCON2,
            (timing.v_sync_width as u32) << 16
                | (timing.v_back_porch as u32) << 8
                | timing.v_front_porch as u32,
        );
        reg_write32(REG_LCM_TCON3, (w - 1) << 16 | h);
        reg_write32(REG_LCM_TCON4, w << 16 | (timing.polarity & 0xF));

        // VSYNC flag (polled by wait_vsync; the AIC interrupt stays off)
        reg_write32(REG_LCM_LCDCInt, reg_read32(REG_LCM_LCDCInt) | LCDCInt_VINTEN);

        reg_write32(REG_LCM_FSADDR, front);
        reg_write32(REG_LCM_LCDCCtl, reg_read32(REG_LCM_LCDCCtl) | LCDCCtl_LCDRUN);

        Lcd {
            fb: [front as *mut u16, back as *mut u16],
            draw: 1,
            width: timing.width as usize,
            height: timing.height as usize,
            pixel_clock_khz: upll_khz / div,
        }
    }

    /// Wait for the start of the next vertical sync.
    pub fn wait_vsync(&self) {
        unsafe {
            reg_write32(REG_LCM_LCDCInt, reg_read32(REG_LCM_LCDCInt) & !LCDCInt_VINT);
            while reg_read32(REG_LCM_LCDCInt) & LCDCInt_VINT == 0 {}
        }
    }

    /// Show what has been drawn: write the D-cache back to SDRAM and, when
    /// double-buffered, switch the LCD to the back buffer at VSYNC.  The
    /// buffer that was on screen becomes the new drawing buffer.
    ///
    /// # Safety
    /// Cleans the whole D-cache (harmless, but takes a moment).
    pub unsafe fn present(&mut self) {
        cache::dcache_clean();
        if self.fb[0] != self.fb[1] {
            reg_write32(REG_LCM_FSADDR, self.fb[self.draw] as u32);
            // The new address is taken at the next frame; until then the
            // old buffer is still being scanned out.
            self.wait_vsync();
            self.draw ^= 1;
        }
    }

    /// Whether drawing is double-buffered.
    pub fn is_double_buffered(&self) -> bool {
        self.fb[0] != self.fb[1]
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    /// Pixel clock actually used.
    pub fn pixel_clock_khz(&self) -> u32 {
        self.pixel_clock_khz
    }

    /// Address of the buffer being drawn into.
    pub fn frame_buffer(&self) -> *mut u16 {
        self.fb[self.draw]
    }

    /// Set one pixel; out-of-range coordinates are ignored.
    pub fn set_pixel(&self, x: usize, y: usize, color: u16) {
        if x < self.width && y < self.height {
            unsafe { self.frame_buffer().add(y * self.width + x).write(color) };
        }
    }

    /// Fill a rectangle, clipped to the screen.  Writes two pixels per
    /// 32-bit store along each row.
    pub fn fill_rect(&self, x: usize, y: usize, w: usize, h: usize, color: u16) {
        let x0 = x.min(self.width);
        let x1 = x.saturating_add(w).min(self.width);
        let y0 = y.min(self.height);
        let y1 = y.saturating_add(h).min(self.height);
        let pair = (color as u32) << 16 | color as u32;

        for row in y0..y1 {
            unsafe {
                let mut p = self.frame_buffer().add(row * self.width + x0);
                let mut n = x1 - x0;
                if n > 0 && (p as usize) & 2 != 0 {
                    p.write(color);
                    p = p.add(1);
                    n -= 1;
                }
                let mut q = p as *mut u32;
                for _ in 0..n / 2 {
                    q.write(pair);
                    q = q.add(1);
                }
                if n & 1 != 0 {
                    (q as *mut u16).write(color);
                }
            }
        }
    }

    /// Copy `pixels` into row `y` starting at column `x`, clipped.
    pub fn write_row(&self, x: usize, y: usize, pixels: &[u16]) {
        if y >= self.height || x >= self.width {
            return;
        }
        let n = pixels.len().min(self.width - x);
        unsafe {
            core::ptr::copy_nonoverlapping(
                pixels.as_ptr(),
                self.frame_buffer().add(y * self.width + x),
                n,
            );
        }
    }

    /// Fill the whole screen.
    pub fn fill(&self, color: u16) {
        self.fill_rect(0, 0, self.width, self.height, color);
    }
}
