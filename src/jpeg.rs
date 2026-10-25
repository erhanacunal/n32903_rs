//! JPEG codec for N32903 (W55FA93) — hardware baseline decoder
//! Ported from the N3290x Non-OS BSP JPEG driver (`JPEG/Src/jpegcodec.c`:
//! `jpegOpen`, `jpegInit`, `jpegSetDecodeMode`, `jpegDecodeTrigger`, the
//! decode branches of `jpegISR`, `jpegCalScalingFactor`,
//! `jpegSetScalingFactor`, `jpegClose`) and the sample's stream check
//! (`ParsingJPEG` in `JPEG/Example/common.c`).
//!
//! Differences from the C driver:
//! - Polled. `IRQ_JPG` stays disabled; [`decode`] watches the interrupt
//!   status bits and does the work of `jpegISR` itself, including the
//!   header-decode-end step that sets the output dimension.
//! - Every wait is bounded, so a corrupt stream makes [`decode`] fail
//!   instead of hanging.
//! - No global state: the options `jpegIoctl` stored in `g_*` variables are
//!   passed per call in [`DecodeParams`].
//! - Decode to packet formats only. Encode, planar YUV output, window
//!   decode, thumbnail decode and input-wait (streamed bit-stream) are not
//!   ported.
//! - [`decode`] checks that the output buffer can hold what the engine will
//!   write, and does the D-cache maintenance for both buffers.
//!
//! # Safety
//!
//! All functions in this module are `unsafe` because they program the
//! hardware directly. Call [`open`] once before the first [`decode`].

use crate::cache;
use crate::clock::{self, ClockNode};
use crate::mmu::UNCACHED_ALIAS;
use crate::registers::*;

// ============================================================================
// Public Types
// ============================================================================

/// The output buffer must start on a cache line and be a whole number of
/// them (it is invalidated after the decode).
pub const OUT_ALIGN: usize = cache::CACHE_LINE as usize;

/// Bit-stream buffer alignment (the vendor sample rounds it up to 4).
pub const SRC_ALIGN: usize = 4;

/// Largest dimension the width/height registers hold.
pub const MAX_DIM: u16 = 0x1FFF;

/// Packet output pixel format (`JPEG_DEC_PRIMARY_PACKET_*`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OutputFormat {
    /// RGB565, one `u16` per pixel — the VPOST frame-buffer format.
    Rgb565,
    /// RGB555, one `u16` per pixel.
    Rgb555,
    /// Packed YUV 4:2:2, 2 bytes per pixel.
    Yuv422,
    /// ARGB8888, one `u32` per pixel.
    Argb8888,
}

impl OutputFormat {
    /// JITCR value (`PDHTAB | DHEND` plus the format bits).
    fn jitcr(self) -> u32 {
        match self {
            OutputFormat::Rgb565 => 0x0_6021,
            OutputFormat::Rgb555 => 0x0_4021,
            OutputFormat::Yuv422 => 0x0_0021,
            OutputFormat::Argb8888 => 0x1_4021,
        }
    }

    /// Bytes per output pixel.
    pub fn bytes_per_pixel(self) -> usize {
        match self {
            OutputFormat::Argb8888 => 4,
            _ => 2,
        }
    }
}

/// Chroma subsampling of a stream (`JPEG_DEC_YUV*`, JITCR `DYUV_MODE`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sampling {
    Yuv420,
    Yuv422,
    Yuv444,
    Yuv411,
    Gray,
    /// 4:2:2 transposed (vertical subsampling)
    Yuv422T,
}

impl Sampling {
    fn from_dyuv(v: u32) -> Self {
        match (v & JITCR_DYUV_MODE) >> 8 {
            0 => Sampling::Yuv420,
            1 => Sampling::Yuv422,
            2 => Sampling::Yuv444,
            3 => Sampling::Yuv411,
            4 => Sampling::Gray,
            _ => Sampling::Yuv422T,
        }
    }

    /// Output width alignment without scaling (the MCU width).
    pub fn width_align(self) -> u16 {
        match self {
            Sampling::Yuv411 => 32,
            Sampling::Yuv444 | Sampling::Yuv422T => 8,
            _ => 16,
        }
    }
}

/// What [`probe`] found in a stream's headers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct JpegHeader {
    pub width: u16,
    pub height: u16,
    pub sampling: Sampling,
}

/// Decode request.
#[derive(Clone, Copy, Debug)]
pub struct DecodeParams {
    pub format: OutputFormat,
    /// Output row pitch in pixels; 0 = the image width rounded up to the
    /// MCU width. A pitch smaller than that is ignored, as in the C driver.
    pub stride: u16,
    /// Down-scale to this `(width, height)`; neither may exceed the image.
    pub downscale: Option<(u16, u16)>,
}

/// Result of a decode (`JPEG_INFO_T`).
#[derive(Clone, Copy, Debug)]
pub struct DecodeInfo {
    pub sampling: Sampling,
    /// Size in the stream header.
    pub jpeg_width: u16,
    pub jpeg_height: u16,
    /// Pixels written per row (MCU-aligned unless down-scaled).
    pub width: u16,
    pub height: u16,
    /// Row pitch in pixels.
    pub stride: u16,
}

/// JPEG error
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum JpegError {
    /// Not a JPEG stream, or a truncated one
    Format,
    /// Valid JPEG the engine can't decode (progressive, arithmetic, CMYK…)
    Unsupported,
    /// The engine reported a decode error
    Decode,
    /// The engine did not finish
    Timeout,
    /// Bad alignment, or the output would not fit
    BadBuffer,
    /// Down-scale size larger than the image
    InvalidParam,
}

// ============================================================================
// Configuration
// ============================================================================

/// Poll bound for one decode, in status reads (seconds at 200 MHz; an
/// 800×480 decode takes a few milliseconds).
const DECODE_POLLS: u32 = 0x0400_0000;

/// Highest JPEG engine clock (vendor: 100 MHz).
const ENGINE_MAX_KHZ: u32 = 100_000;

// ============================================================================
// Engine Control
// ============================================================================

/// Physical (cacheable-view) address of a buffer, accepting either view.
fn phys(ptr: u32) -> u32 {
    ptr & !UNCACHED_ALIAS
}

/// `jpegOpen` without the ISR: clock on, reset, engine clock ≤ 100 MHz.
pub unsafe fn open() {
    reg_write32(REG_AHBCLK, reg_read32(REG_AHBCLK) | JPG_CKE);
    reg_write32(REG_AHBIPRST, reg_read32(REG_AHBIPRST) | JPGRST);
    reg_write32(REG_AHBIPRST, reg_read32(REG_AHBIPRST) & !JPGRST);

    let src = clock::get_clock(ClockNode::Hclk234);
    let div = if src > ENGINE_MAX_KHZ {
        if src % ENGINE_MAX_KHZ != 0 {
            src / ENGINE_MAX_KHZ
        } else {
            src / ENGINE_MAX_KHZ - 1
        }
    } else {
        0
    };
    reg_write32(REG_CLKDIV4, (reg_read32(REG_CLKDIV4) & !JPG_N) | ((div & 0x7) << 24));
}

/// `jpegClose`: reset the engine and stop its clock.
pub unsafe fn close() {
    reg_write32(REG_JMCR, JMCR_ENG_RST);
    reg_write32(REG_JMCR, 0);
    reg_write32(REG_AHBCLK, reg_read32(REG_AHBCLK) & !JPG_CKE);
}

/// `jpegInit`: register defaults and an engine reset.
unsafe fn init() {
    reg_write32(REG_JPRIQC, 0xF4);
    reg_write32(REG_JTHBQC, 0xF4);
    reg_write32(REG_JPRST, 0x04);
    reg_write32(REG_JTRST, 0x04);
    reg_write32(REG_JITCR, 0);
    // No up-/down-scaling.
    reg_write32(REG_JPSCALU, 0);
    reg_write32(REG_JPSCALD, 0);
    reg_write32(REG_JUPRAT, 0);
    reg_write32(REG_JSRCH, 0xFFF);
    reg_write32(REG_JMCR, JMCR_ENG_RST);
    reg_write32(REG_JMCR, 0);
    // Can't use a single buffer (vendor note).
    reg_write32(REG_JMACR, 0x0040_0000);
}

/// `JPEG_CLEAR_INT`: write-1-to-clear the given status bits only.
unsafe fn clear_int(bits: u32) {
    reg_write32(REG_JINTCR, (reg_read32(REG_JINTCR) & !0xFF) | bits);
}

/// `jpegCalScalingFactor` + `jpegSetScalingFactor` for packet down-scaling:
/// ratio = target / (source − 1) in 1/8192 units, at most 1.0.
unsafe fn set_downscale(w: u16, h: u16, tw: u16, th: u16) -> Result<(), JpegError> {
    if tw == 0 || th == 0 || tw > w || th > h || w < 2 || h < 2 {
        return Err(JpegError::InvalidParam);
    }
    let ratio = |t: u16, s: u16| ((t as u32 * 8192) / (s as u32 - 1)).min(8192);
    let (rw, rh) = (ratio(tw, w), ratio(th, h));
    reg_write32(REG_JPSCALD, reg_read32(REG_JPSCALD) | JPSCALD_PSX_ON);
    reg_write32(REG_JPSCALU, 0);
    reg_write32(REG_JPSCALD, reg_read32(REG_JPSCALD) & !(JPSCALD_PSCALX_F | JPSCALD_PSCALY_F));
    reg_write32(REG_JUPRAT, ((rh & 0x3FFF) << 16) | (rw & 0x3FFF));
    Ok(())
}

/// Stop a decode in progress (error / bad buffer / timeout).
unsafe fn abort() {
    reg_write32(REG_JINTCR, 0xFF);
    init();
}

/// Decode the baseline JPEG in `src` into `dst`.
///
/// `src` must be [`SRC_ALIGN`]-aligned; `dst` must be [`OUT_ALIGN`]-aligned
/// and a whole number of cache lines long. The engine writes `stride` pixels
/// per row for `height` rows rounded up to the MCU height; when that does not
/// fit in `dst` the decode is stopped before any pixel is written
/// ([`JpegError::BadBuffer`]).
pub unsafe fn decode(src: &[u8], dst: &mut [u8], p: &DecodeParams) -> Result<DecodeInfo, JpegError> {
    let (sa, da) = (src.as_ptr() as usize, dst.as_mut_ptr() as usize);
    if src.is_empty() || sa % SRC_ALIGN != 0 || da % OUT_ALIGN != 0 || dst.len() % OUT_ALIGN != 0 {
        return Err(JpegError::BadBuffer);
    }

    // The engine reads the stream from memory: write the cache back. Dirty
    // lines of `dst` go too, so none is evicted over the output later.
    cache::dcache_clean();

    init();
    reg_write32(REG_JIOADDR0, phys(sa as u32));
    reg_write32(REG_JYADDR0, phys(da as u32));
    // jpegSetDecodeMode
    reg_write32(REG_JITCR, p.format.jitcr());
    reg_write32(REG_JMCR, reg_read32(REG_JMCR) & !JMCR_ENC_DEC);

    // jpegDecodeTrigger
    reg_write32(REG_JINTCR, JINT_DEC_INTE | JINT_DER_INTE | JINT_DHE_INTE);
    clear_int(JINT_DEC_INTS | JINT_DER_INTS | JINT_DHE_INTS | JINT_IPW_INTS);
    reg_write32(REG_JMCR, reg_read32(REG_JMCR) | JMCR_JPG_EN);
    reg_write32(REG_JMCR, reg_read32(REG_JMCR) & !JMCR_JPG_EN);

    let mut info = DecodeInfo {
        sampling: Sampling::Yuv420,
        jpeg_width: 0,
        jpeg_height: 0,
        width: 0,
        height: 0,
        stride: 0,
    };
    let mut pad = 0u16;

    for _ in 0..DECODE_POLLS {
        let status = reg_read32(REG_JINTCR) & 0xFF;

        if status & JINT_DHE_INTS != 0 {
            // Header decoded: the engine waits for the output dimension.
            info.sampling = Sampling::from_dyuv(reg_read32(REG_JITCR));
            let wh = reg_read32(REG_JDECWH);
            let (jw, jh) = ((wh & 0xFFFF) as u16, (wh >> 16) as u16);
            info.jpeg_width = jw;
            info.jpeg_height = jh;
            if jw == 0 || jh == 0 || jw > MAX_DIM || jh > MAX_DIM {
                abort();
                return Err(JpegError::Unsupported);
            }

            let (mut w, h) = match p.downscale {
                Some((tw, th)) => {
                    if let Err(e) = set_downscale(jw, jh, tw, th) {
                        abort();
                        return Err(e);
                    }
                    (tw, th)
                }
                None => (jw.next_multiple_of(info.sampling.width_align()), jh),
            };

            // Row pitch: `JYSTRIDE` is the padding after each row.
            if p.stride >= w {
                pad = p.stride - w;
                w = p.stride;
            }
            reg_write32(REG_JYSTRIDE, pad as u32);

            // Whole MCU rows may be written (16 lines at most).
            let need = w as usize * h.next_multiple_of(16) as usize * p.format.bytes_per_pixel();
            if w > MAX_DIM || h > MAX_DIM || need > dst.len() {
                abort();
                return Err(JpegError::BadBuffer);
            }
            info.stride = w;

            // jpegSetDimension
            reg_write32(REG_JPRIWH, ((h as u32 & 0x1FFF) << 16) | (w as u32 & 0x1FFF));
            clear_int(JINT_DHE_INTS);
        } else if status & JINT_DEC_INTS != 0 {
            // jpegGetDimension
            let wh = reg_read32(REG_JPRIWH);
            info.width = (wh & 0x1FFF) as u16 - pad;
            info.height = (wh >> 16) as u16;
            clear_int(JINT_DEC_INTS);
            // Drop stale lines so the CPU reads what the engine wrote.
            cache::dcache_invalidate_range(da as u32, dst.len() as u32);
            return Ok(info);
        } else if status & JINT_DER_INTS != 0 {
            abort();
            return Err(JpegError::Decode);
        }
    }
    abort();
    Err(JpegError::Timeout)
}

// ============================================================================
// Stream Check (ParsingJPEG)
// ============================================================================

/// Read the frame header of a stream and check that the engine can decode
/// it: baseline (SOF0), three components, chroma at 1×1 and luma at 2×2
/// (4:2:0), 2×1 (4:2:2) or 1×1 (4:4:4).
pub fn probe(data: &[u8]) -> Result<JpegHeader, JpegError> {
    if data.len() < 4 || data[0] != 0xFF || data[1] != 0xD8 {
        return Err(JpegError::Format);
    }
    let mut i = 2usize;
    let mut header = None;
    loop {
        // Markers may be padded with any number of 0xFF bytes.
        if *data.get(i).ok_or(JpegError::Format)? != 0xFF {
            return Err(JpegError::Format);
        }
        while data.get(i) == Some(&0xFF) {
            i += 1;
        }
        let marker = *data.get(i).ok_or(JpegError::Format)?;
        i += 1;
        if marker == 0x01 || (0xD0..=0xD7).contains(&marker) {
            continue; // no length field
        }
        let len = u16::from_be_bytes([
            *data.get(i).ok_or(JpegError::Format)?,
            *data.get(i + 1).ok_or(JpegError::Format)?,
        ]) as usize;
        let seg = data.get(i + 2..i + len).ok_or(JpegError::Format)?;
        match marker {
            0xC0 => header = Some(parse_sof0(seg)?),
            // Every other SOF: progressive, lossless, arithmetic coding.
            0xC1..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF => {
                return Err(JpegError::Unsupported);
            }
            0xDA => return header.ok_or(JpegError::Format), // start of scan
            0xD9 => return Err(JpegError::Format),
            _ => {}
        }
        i += len;
    }
}

/// Frame header payload (after the length).
fn parse_sof0(seg: &[u8]) -> Result<JpegHeader, JpegError> {
    if seg.len() < 6 {
        return Err(JpegError::Format);
    }
    if seg[0] != 8 {
        return Err(JpegError::Unsupported); // 12-bit precision
    }
    let height = u16::from_be_bytes([seg[1], seg[2]]);
    let width = u16::from_be_bytes([seg[3], seg[4]]);
    if seg[5] != 3 {
        return Err(JpegError::Unsupported); // grayscale / CMYK
    }
    let comps = seg.get(6..15).ok_or(JpegError::Format)?;
    let factors = |c: usize| (comps[c * 3 + 1] >> 4, comps[c * 3 + 1] & 0x0F);
    if factors(1) != (1, 1) || factors(2) != (1, 1) {
        return Err(JpegError::Unsupported);
    }
    let sampling = match factors(0) {
        (2, 2) => Sampling::Yuv420,
        (2, 1) => Sampling::Yuv422,
        (1, 1) => Sampling::Yuv444,
        _ => return Err(JpegError::Unsupported),
    };
    if width == 0 || height == 0 || width > MAX_DIM || height > MAX_DIM {
        return Err(JpegError::Unsupported);
    }
    Ok(JpegHeader { width, height, sampling })
}
