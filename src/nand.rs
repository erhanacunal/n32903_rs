//! NAND flash through the FMI/SM controller (DMA + BCH ECC).
//!
//! Ported from the NAND loader's `nand_drv.rs` (NandDrv.c + turbowriter.h).
//! Blocks are addressed physically from 0; the controller inserts and
//! checks the BCH parity in the spare area by itself.
//!
//! # Page buffers
//!
//! Page data moves by DMA, straight between NAND and SDRAM.  Buffers must
//! be 32-byte aligned (one D-cache line) and a whole page long; the driver
//! does the cache maintenance:
//!
//! - write: the D-cache is cleaned, so the DMA reads what the CPU wrote
//! - read: the buffer's lines are invalidated before the DMA, so no stale
//!   line hides the new data (ECC corrections are applied by the CPU after
//!   each 512-byte field arrives, through the cache)
//!
//! # Spare area
//!
//! Spare word 0 of every page programmed by [`write_page`] is
//! [`SPARE_DATA`]: byte 0 (bad-block mark) and byte 1 stay 0xFF, bytes 2-3
//! are 0x00.  [`read_page`] uses bytes 2-3 to tell a programmed page from
//! an erased one, whose BCH parity would not check.
//!
//! ```rust,ignore
//! let info = unsafe { nand::init() }.unwrap();
//! let mut page = Page([0; 2048]);  // #[repr(align(32))]
//! match unsafe { nand::read_page(10, 0, page.0.as_mut_ptr()) } {
//!     Ok(nand::PageState::Data) => {}
//!     Ok(nand::PageState::Erased) => {} // buffer filled with 0xFF
//!     Err(e) => {}
//! }
//! ```

use crate::cache;
use crate::mmu::UNCACHED_ALIAS;
use crate::registers::*;

pub const NAND_PAGE_512B: u32 = 512;
pub const NAND_PAGE_2KB: u32 = 2048;
pub const NAND_PAGE_4KB: u32 = 4096;

/// Spare word 0 for ordinary data pages: bytes 0-1 0xFF, bytes 2-3 0x00
/// ("page used").
pub const SPARE_DATA: u32 = 0x0000_FFFF;

/// Required buffer alignment (D-cache line).
pub const BUF_ALIGN: usize = 32;

/// Driver error.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NandError {
    /// The chip ID is not in the supported list
    UnknownChip([u8; 5]),
    /// Ready/busy never came back
    Timeout,
    /// Uncorrectable ECC error
    Ecc,
    /// The chip reported program or erase failure
    Failed,
}

/// What [`read_page`] found.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PageState {
    /// Programmed page, ECC checked (and corrected)
    Data,
    /// Never programmed since the last erase; buffer filled with 0xFF
    Erased,
}

/// NAND geometry and features (FMI_SM_INFO_T).
#[derive(Clone, Copy, Debug)]
pub struct NandInfo {
    pub id: [u8; 5],
    pub sector_per_flash: u32,
    pub block_per_flash: u32,
    pub page_per_block: u32,
    pub sector_per_block: u32,
    pub page_size: u32,
    pub is_multicycle: bool,
    pub is_mlc_nand: bool,
    pub is_nand_ecc4: bool,
    pub is_nand_ecc8: bool,
    pub is_nand_ecc12: bool,
    pub is_check_ecc: bool,
}

impl NandInfo {
    const fn empty() -> Self {
        NandInfo {
            id: [0; 5],
            sector_per_flash: 0,
            block_per_flash: 0,
            page_per_block: 0,
            sector_per_block: 0,
            page_size: 0,
            is_multicycle: false,
            is_mlc_nand: false,
            is_nand_ecc4: false,
            is_nand_ecc8: false,
            is_nand_ecc12: false,
            is_check_ecc: true,
        }
    }

    /// Block size in bytes.
    pub fn block_size(&self) -> u32 {
        self.page_size * self.page_per_block
    }
}

static mut INITIALIZED: bool = false;
static mut SM0: NandInfo = NandInfo::empty();

/// Geometry of the initialised chip.
pub fn info() -> &'static NandInfo {
    unsafe { &*core::ptr::addr_of!(SM0) }
}

// ============================================================================
// Initialisation
// ============================================================================

/// Turn on the FMI/DMAC, set up the SM controller and identify the chip.
/// Safe to call again (returns the cached info).
///
/// # Safety
/// Takes over the FMI, its DMA and the NAND pins (GPD5-8, GPE8-15).
pub unsafe fn init() -> Result<NandInfo, NandError> {
    if INITIALIZED {
        return Ok(SM0);
    }

    // NAND engine clock on, SD off
    reg_write32(REG_AHBCLK, reg_read32(REG_AHBCLK) | SIC_CKE | NAND_CKE);
    reg_write32(REG_AHBCLK, reg_read32(REG_AHBCLK) & !SD_CKE);

    // DMAC reset + enable, FMI reset
    reg_write32(REG_DMACCSR, 0x0000_0003);
    reg_write32(REG_DMACCSR, 0x0000_0001);
    reg_write32(REG_FMICR, 0x01);

    // NAND control pins: NWR/NRD/RB0/RB1, ALE/CLE/CS0/CS1
    reg_write32(REG_GPDFUN, reg_read32(REG_GPDFUN) | 0x0003_FC00);
    reg_write32(REG_GPEFUN, reg_read32(REG_GPEFUN) | 0x00FF_0000);
    reg_write32(REG_SMCSR, reg_read32(REG_SMCSR) | 0x0600_0000); // CS0/CS1 high
    reg_write32(REG_SMCSR, reg_read32(REG_SMCSR) & !0x0200_0000); // CS0 low
    reg_write32(REG_FMICR, FMI_SM_EN);

    reg_write32(REG_SMCSR, reg_read32(REG_SMCSR) | SMCR_SM_SWRST);
    reg_write32(REG_SMCSR, reg_read32(REG_SMCSR) & !SMCR_SM_SWRST);

    // ECC on, checked, redundant area written automatically
    reg_write32(REG_SMCSR, reg_read32(REG_SMCSR) & !SMCR_PSIZE);
    reg_write32(
        REG_SMCSR,
        reg_read32(REG_SMCSR) | SMCR_ECC_EN | SMCR_ECC_CHK | SMCR_REDUN_AUTO_WEN,
    );
    reg_write32(REG_SMCSR, (reg_read32(REG_SMCSR) & !SMCR_BCH_TSEL) | BCH_T4);
    reg_write32(REG_SMREAREA_CTL, reg_read32(REG_SMREAREA_CTL) & !SMRE_MECC);
    // BCH also protects the first 3 spare bytes
    reg_write32(REG_SMCSR, reg_read32(REG_SMCSR) | SMCR_ECC_3B_PROTECT);

    reg_write32(REG_SMCSR, reg_read32(REG_SMCSR) & !SMCR_CS0);
    reg_write32(REG_SMCSR, reg_read32(REG_SMCSR) | SMCR_CS1);

    reg_write32(REG_SMTCR, 0x3050B);

    let mut sm = NandInfo::empty();
    read_id(&mut sm)?;
    set_page_format(&sm);
    SM0 = sm;
    INITIALIZED = true;
    Ok(sm)
}

/// Program page size, BCH level and redundant-area size.
unsafe fn set_page_format(sm: &NandInfo) {
    let (psize, bch, rea) = match sm.page_size {
        NAND_PAGE_2KB => (PSIZE_2K, BCH_T4, 64),
        NAND_PAGE_512B => (0, BCH_T4, 16),
        _ => (PSIZE_4K, BCH_T8, 128),
    };
    reg_write32(REG_SMCSR, (reg_read32(REG_SMCSR) & !SMCR_PSIZE) | psize);
    reg_write32(REG_SMCSR, (reg_read32(REG_SMCSR) & !SMCR_BCH_TSEL) | bch);
    reg_write32(REG_SMREAREA_CTL, rea);
}

unsafe fn read_id(sm: &mut NandInfo) -> Result<(), NandError> {
    reset_chip();
    reg_write32(REG_SMCMD, 0x90);
    reg_write32(REG_SMADDR, EOA_SM);
    let mut id = [0u8; 5];
    for b in id.iter_mut() {
        *b = reg_read32(REG_SMDATA) as u8;
    }
    sm.id = id;

    // MXIC NAND ROMs have no ECC
    sm.is_check_ecc = !(id[0] == 0xC2 && (id[1] == 0x79 || id[1] == 0x76));

    match id[1] {
        // --- 512 B pages ---
        0x79 | 0x76 | 0x75 | 0x73 => {
            let (sectors, blocks, multi) = match id[1] {
                0x79 => (255_744, 8191, true), // 128 MB
                0x76 => (127_872, 4095, true), // 64 MB
                0x75 => (63_936, 2047, false), // 32 MB
                _ => (31_968, 1023, false),    // 16 MB
            };
            sm.sector_per_flash = sectors;
            sm.block_per_flash = blocks;
            sm.page_per_block = 32;
            sm.sector_per_block = 32;
            sm.is_multicycle = multi;
            sm.page_size = NAND_PAGE_512B;
            sm.is_nand_ecc4 = true;
        }
        // --- 2 KB pages, 128 MB (e.g. Winbond W29N01 EF F1) ---
        0xF1 | 0xD1 => {
            sm.block_per_flash = 1023;
            sm.page_per_block = 64;
            sm.sector_per_block = 256;
            sm.sector_per_flash = 255_744;
            sm.is_multicycle = false;
            sm.page_size = NAND_PAGE_2KB;
            sm.is_nand_ecc8 = true;
            if id[0] == 0xC2 && id[1] == 0xF1 && id[2] == 0x80 && (id[3] == 0x1D || id[3] == 0x95) {
                sm.is_check_ecc = true;
            }
        }
        // --- 256 MB ---
        0xDA => {
            if id[3] & 0x33 == 0x11 {
                sm.block_per_flash = 2047;
                sm.page_per_block = 64;
                sm.sector_per_block = 256;
            } else if id[3] & 0x33 == 0x21 {
                sm.block_per_flash = 1023;
                sm.page_per_block = 128;
                sm.sector_per_block = 512;
                sm.is_mlc_nand = true;
            }
            sm.sector_per_flash = 511_488;
            sm.is_multicycle = true;
            sm.page_size = NAND_PAGE_2KB;
            sm.is_nand_ecc8 = true;
        }
        // --- 512 MB ---
        0xDC => {
            if (id[0] == 0x2C && id[2] == 0x90 && id[3] == 0xA6 && id[4] == 0x54)
                || (id[0] == 0x98 && id[2] == 0x90 && id[3] == 0x26 && id[4] == 0x76)
            {
                // Micron MT29F4G08ABAEA / Toshiba: 4 KB pages
                sm.block_per_flash = 2047;
                sm.page_per_block = 64;
                sm.page_size = NAND_PAGE_4KB;
                sm.sector_per_block = sm.page_size / 512 * sm.page_per_block;
                sm.is_multicycle = true;
                if id[0] == 0x2C {
                    sm.is_nand_ecc12 = true;
                    sm.sector_per_flash = sm.sector_per_block * 2000 / 1000 * 999;
                } else {
                    sm.is_nand_ecc8 = true;
                    sm.sector_per_flash = 1_022_976;
                }
                return Ok(());
            }
            if id[3] & 0x33 == 0x11 {
                sm.block_per_flash = 4095;
                sm.page_per_block = 64;
                sm.sector_per_block = 256;
            } else if id[3] & 0x33 == 0x21 {
                sm.block_per_flash = 2047;
                sm.page_per_block = 128;
                sm.sector_per_block = 512;
                sm.is_mlc_nand = true;
            }
            sm.sector_per_flash = 1_022_976;
            sm.is_multicycle = true;
            sm.page_size = NAND_PAGE_2KB;
            sm.is_nand_ecc8 = true;
        }
        // --- 1 GB ---
        0xD3 => {
            sm.is_multicycle = true;
            sm.is_nand_ecc8 = true;
            sm.sector_per_flash = 2_045_952;
            if id[0] == 0xEC && id[2] == 0x51 && id[3] == 0x95 && id[4] == 0x58 {
                // Samsung K9WAG08U1D
                sm.block_per_flash = 4095;
                sm.page_per_block = 64;
                sm.page_size = NAND_PAGE_2KB;
                sm.sector_per_block = sm.page_size / 512 * sm.page_per_block;
                sm.sector_per_flash = 1_022_976;
                return Ok(());
            }
            match id[3] & 0x33 {
                0x32 => {
                    sm.block_per_flash = 2047;
                    sm.page_per_block = 128;
                    sm.sector_per_block = 1024;
                    sm.page_size = NAND_PAGE_4KB;
                    sm.is_mlc_nand = true;
                }
                0x11 => {
                    sm.block_per_flash = 8191;
                    sm.page_per_block = 64;
                    sm.sector_per_block = 256;
                    sm.page_size = NAND_PAGE_2KB;
                }
                0x21 => {
                    sm.block_per_flash = 4095;
                    sm.page_per_block = 128;
                    sm.sector_per_block = 512;
                    sm.page_size = NAND_PAGE_2KB;
                    sm.is_mlc_nand = true;
                }
                0x22 => {
                    sm.block_per_flash = 4095;
                    sm.page_per_block = 64;
                    sm.sector_per_block = 512;
                    sm.page_size = NAND_PAGE_4KB;
                }
                _ => {}
            }
        }
        // --- 2 GB ---
        0xD5 => {
            match id[3] & 0x33 {
                0x32 => {
                    sm.block_per_flash = 4095;
                    sm.page_per_block = 128;
                    sm.sector_per_block = 1024;
                    sm.page_size = NAND_PAGE_4KB;
                    sm.is_mlc_nand = true;
                }
                0x11 => {
                    sm.block_per_flash = 16383;
                    sm.page_per_block = 64;
                    sm.sector_per_block = 256;
                    sm.page_size = NAND_PAGE_2KB;
                }
                0x21 => {
                    sm.block_per_flash = 8191;
                    sm.page_per_block = 128;
                    sm.sector_per_block = 512;
                    sm.page_size = NAND_PAGE_2KB;
                    sm.is_mlc_nand = true;
                }
                _ => {}
            }
            sm.sector_per_flash = 4_091_904;
            sm.is_multicycle = true;
            sm.is_nand_ecc8 = true;
        }
        _ => {
            // MXIC MX30LF1208AA (64 MB)
            if id[0] == 0xC2 && id[1] == 0xF0 && id[2] == 0x80 && id[3] == 0x1D {
                sm.block_per_flash = 511;
                sm.page_per_block = 64;
                sm.page_size = NAND_PAGE_2KB;
                sm.sector_per_block = sm.page_size / 512 * sm.page_per_block;
                sm.is_nand_ecc8 = true;
                sm.sector_per_flash = sm.sector_per_block * 500 / 1000 * 999;
                sm.is_check_ecc = true;
                return Ok(());
            }
            return Err(NandError::UnknownChip(id));
        }
    }
    Ok(())
}

// ============================================================================
// Low-level controller helpers
// ============================================================================

/// Wait for the R/B rising edge flag (with timeout) and clear it.
unsafe fn wait_rb_edge() -> Result<(), NandError> {
    for _ in 0..0x80000u32 {
        if reg_read32(REG_SMISR) & SMISR_RB0_IF != 0 {
            reg_write32(REG_SMISR, SMISR_RB0_IF);
            return Ok(());
        }
    }
    Err(NandError::Timeout)
}

/// Wait until the chip is ready, then clear the R/B flag before a command.
unsafe fn wait_ready() {
    while reg_read32(REG_SMISR) & SMISR_RB0 == 0 {}
    reg_write32(REG_SMISR, SMISR_RB0_IF);
}

unsafe fn reset_chip() {
    reg_write32(REG_SMCMD, 0xFF);
    spin(100);
    let _ = wait_rb_edge();
}

/// Select chip 0 and the SM function.
unsafe fn select() {
    reg_write32(REG_GPDFUN, reg_read32(REG_GPDFUN) | 0x0003_CC00);
    reg_write32(REG_GPEFUN, reg_read32(REG_GPEFUN) | 0x00F3_0000);
    reg_write32(REG_SMCSR, reg_read32(REG_SMCSR) & !SMCR_CS0);
    reg_write32(REG_SMCSR, reg_read32(REG_SMCSR) | SMCR_CS1);
    reg_write32(REG_FMICR, FMI_SM_EN);
}

/// Row (page) address cycles, the last one with EOA.
unsafe fn send_row(sm: &NandInfo, page: u32) {
    reg_write32(REG_SMADDR, page & 0xFF);
    if !sm.is_multicycle {
        reg_write32(REG_SMADDR, ((page >> 8) & 0xFF) | EOA_SM);
    } else {
        reg_write32(REG_SMADDR, (page >> 8) & 0xFF);
        reg_write32(REG_SMADDR, ((page >> 16) & 0xFF) | EOA_SM);
    }
}

/// Read Status: true if the last program/erase passed.
unsafe fn status_pass() -> bool {
    reg_write32(REG_SMCMD, 0x70);
    reg_read32(REG_SMDATA) & 0x01 == 0
}

/// Start reading the page at column `col` (2K/4K pages: 0x00 .. 0x30).
unsafe fn cmd_read(sm: &NandInfo, page: u32, col: u32) -> Result<(), NandError> {
    wait_ready();
    reg_write32(REG_SMCMD, 0x00);
    reg_write32(REG_SMADDR, col & 0xFF);
    reg_write32(REG_SMADDR, (col >> 8) & 0xFF);
    send_row(sm, page);
    reg_write32(REG_SMCMD, 0x30);
    wait_rb_edge()
}

/// 512 B pages: start reading the page (0x00) or its spare area (0x50).
unsafe fn cmd_read_512(sm: &NandInfo, cmd: u32, page: u32, col: u32) -> Result<(), NandError> {
    wait_ready();
    reg_write32(REG_SMCMD, cmd);
    reg_write32(REG_SMADDR, col);
    send_row(sm, page);
    wait_rb_edge()
}

/// Read the spare area into the SMRA registers; returns its size.
unsafe fn read_spare_to_smra(sm: &NandInfo, page: u32) -> Result<u32, NandError> {
    let len = match sm.page_size {
        NAND_PAGE_512B => {
            cmd_read_512(sm, 0x50, page, 0)?;
            16
        }
        NAND_PAGE_2KB => {
            cmd_read(sm, page, 2048)?;
            64
        }
        _ => {
            cmd_read(sm, page, 4096)?;
            128
        }
    };
    let mut p = REG_SMRA_0 as *mut u8;
    for _ in 0..len {
        core::ptr::write_volatile(p, reg_read32(REG_SMDATA) as u8);
        p = p.add(1);
    }
    Ok(len)
}

// ============================================================================
// BCH correction
// ============================================================================

/// Apply the corrections the BCH engine reports for field `field` (1-based
/// 512-byte field of the page at `daddr`).
unsafe fn correct_field(field: u32, error_cnt: u32, daddr: u32) {
    let mut data = [0u32; 16];
    let mut addr = [0u32; 16];

    let words = (error_cnt / 4 + 1).min(4);
    for i in 0..words as usize {
        let v = reg_read32(REG_BCH_ECC_DATA0 + (i * 4) as u32);
        data[i * 4] = v & 0xFF;
        data[i * 4 + 1] = (v >> 8) & 0xFF;
        data[i * 4 + 2] = (v >> 16) & 0xFF;
        data[i * 4 + 3] = (v >> 24) & 0xFF;
    }
    let words = (error_cnt / 2 + 1).min(8);
    for i in 0..words as usize {
        let v = reg_read32(REG_BCH_ECC_ADDR0 + (i * 4) as u32);
        addr[i * 2] = v & 0x1FFF;
        addr[i * 2 + 1] = (v >> 16) & 0x1FFF;
    }

    let page_size = reg_read32(REG_SMCSR) & SMCR_PSIZE;
    let tsel = reg_read32(REG_SMCSR) & SMCR_BCH_TSEL;
    // Parity bytes per field and the SMRA word where the spare copy of the
    // protected bytes' tail starts, per page size / BCH level
    let (per_field, tail_word) = if page_size == PSIZE_4K {
        if tsel & BCH_T12 != 0 {
            (23, 8)
        } else if tsel & BCH_T8 != 0 {
            (15, 2)
        } else {
            (8, 16)
        }
    } else if page_size == PSIZE_2K {
        if tsel & BCH_T8 != 0 {
            (15, 1)
        } else {
            (8, 8)
        }
    } else {
        (0, 2)
    };
    let tail_len = if per_field == 0 { 8 } else { per_field };

    let field_data = (daddr + (field - 1) * 0x200) as *mut u8;
    for i in 0..(error_cnt as usize).min(16) {
        let a = addr[i];
        let d = data[i] as u8;
        if a < 512 {
            *field_data.add(a as usize) ^= d;
        } else if a < 515 {
            let off = a - 512 + per_field * (field - 1);
            *((REG_SMRA_0 + off) as *mut u8) ^= d;
        } else {
            let off = (tail_len - 1) - (543 - a) + per_field * (field - 1);
            *((REG_SMRA_0 + tail_word * 4 + off) as *mut u8) ^= d;
        }
    }
}

/// Run the DMA read started by `SMCR_DRD_EN`, correcting each field as
/// the ECC engine reports it.  `status_regs` is 1 (up to 4 fields) or 2
/// (4 KB pages, 8 fields).
unsafe fn dma_read_with_ecc(
    sm: &NandInfo,
    daddr: u32,
    status_regs: u32,
) -> Result<(), NandError> {
    let check = sm.is_check_ecc || reg_read32(REG_SMCSR) & SMCR_ECC_CHK != 0;
    let mut uncorrectable = false;
    loop {
        if reg_read32(REG_SMISR) & SMISR_ECC_FIELD_IF != 0 {
            if check {
                // One field completes per flag: handle the first field
                // with a status in each register, like NandDrv.c (a field
                // corrected twice would be flipped back)
                for r in 0..status_regs {
                    let mut st = reg_read32(REG_SM_ECC_ST0 + r * 4);
                    for f in 1..5u32 {
                        match st & 0x03 {
                            0x01 => {
                                correct_field(r * 4 + f, (st >> 2) & 0x3F, daddr);
                                break;
                            }
                            0x02 | 0x03 => {
                                uncorrectable = true;
                                break;
                            }
                            _ => {}
                        }
                        st >>= 8;
                    }
                }
            }
            reg_write32(REG_SMISR, SMISR_ECC_FIELD_IF);
        }
        if reg_read32(REG_SMISR) & SMISR_DMA_IF != 0
            && reg_read32(REG_SMISR) & SMISR_ECC_FIELD_IF == 0
        {
            break;
        }
    }
    reg_write32(REG_SMISR, SMISR_DMA_IF);
    if uncorrectable {
        Err(NandError::Ecc)
    } else {
        Ok(())
    }
}

// ============================================================================
// Cache maintenance for DMA buffers
// ============================================================================

/// Physical (cacheable-view) address of a buffer, accepting either view.
fn phys(ptr: u32) -> u32 {
    ptr & !UNCACHED_ALIAS
}

/// Invalidate the D-cache lines covering `[addr, addr + len)`.
unsafe fn invalidate_range(addr: u32, len: u32) {
    let mut a = addr & !(BUF_ALIGN as u32 - 1);
    while a < addr + len {
        core::arch::asm!("MCR p15, 0, {0}, c7, c6, 1", in(reg) a, options(nostack));
        a += BUF_ALIGN as u32;
    }
}

// ============================================================================
// Public page / block API
// ============================================================================

/// Number of zero bits in spare bytes 2-3 that marks a page as programmed.
/// [`SPARE_DATA`] writes 16; an erased page reads 0 (a few bit flips allowed).
const USED_MARK_MIN_ZEROS: u32 = 8;

/// Read page `page` of block `block` into `buf` (one page, 32-byte aligned).
///
/// # Safety
/// `buf` must be valid for `page_size` bytes and not touched until return.
pub unsafe fn read_page(block: u32, page: u32, buf: *mut u8) -> Result<PageState, NandError> {
    let sm = &*core::ptr::addr_of!(SM0);
    let daddr = phys(buf as u32);
    debug_assert!(daddr as usize % BUF_ALIGN == 0);

    select();
    let row = block * sm.page_per_block + page;

    // Spare area first: it holds the "used" mark, and the controller
    // checks the protected spare bytes against SMRA during the data read.
    read_spare_to_smra(sm, row)?;
    let spare0 = reg_read32(REG_SMRA_0);
    if ((spare0 >> 16) as u16).count_zeros() < USED_MARK_MIN_ZEROS {
        core::ptr::write_bytes(buf, 0xFF, sm.page_size as usize);
        return Ok(PageState::Erased);
    }

    // Clean everything (no dirty line may be written back over the DMA
    // data later), then drop the buffer's lines.
    cache::dcache_clean();
    invalidate_range(daddr, sm.page_size);

    while reg_read32(REG_DMACCSR) & FMI_BUSY != 0 {}
    reg_write32(REG_DMACCSR, reg_read32(REG_DMACCSR) | DMAC_EN);
    reg_write32(REG_DMACSAR, daddr);
    if reg_read32(REG_SMISR) & SMISR_ECC_FIELD_IF != 0 {
        reg_write32(REG_SMISR, SMISR_ECC_FIELD_IF);
    }

    if sm.page_size == NAND_PAGE_512B {
        cmd_read_512(sm, 0x00, row, 0)?;
    } else {
        cmd_read(sm, row, 0)?;
    }

    reg_write32(REG_SMISR, SMISR_DMA_IF);
    reg_write32(REG_SMISR, SMISR_ECC_FIELD_IF);
    reg_write32(REG_SMCSR, reg_read32(REG_SMCSR) | SMCR_DRD_EN);

    let regs = if sm.page_size == NAND_PAGE_4KB { 2 } else { 1 };
    dma_read_with_ecc(sm, daddr, regs)?;
    Ok(PageState::Data)
}

/// Program page `page` of block `block` from `buf` (one page, 32-byte
/// aligned), with [`SPARE_DATA`] in spare word 0.  The block must be erased.
///
/// # Safety
/// `buf` must be valid for `page_size` bytes.
pub unsafe fn write_page(block: u32, page: u32, buf: *const u8) -> Result<(), NandError> {
    let sm = &*core::ptr::addr_of!(SM0);
    let saddr = phys(buf as u32);
    debug_assert!(saddr as usize % BUF_ALIGN == 0);

    select();
    let row = block * sm.page_per_block + page;

    cache::dcache_clean();

    while reg_read32(REG_DMACCSR) & FMI_BUSY != 0 {}
    reg_write32(REG_DMACCSR, reg_read32(REG_DMACCSR) | DMAC_EN);
    reg_write32(REG_DMACSAR, saddr);

    // Spare: our word 0, the rest 0xFF; the controller adds the parity
    let words = (reg_read32(REG_SMREAREA_CTL) & SMRE_REA128_EXT) / 4;
    reg_write32(REG_SMRA_0, SPARE_DATA);
    for i in 1..words {
        reg_write32(REG_SMRA_0 + i * 4, 0xFFFF_FFFF);
    }

    wait_ready();
    if sm.page_size == NAND_PAGE_512B {
        reg_write32(REG_SMCMD, 0x00);
        reg_write32(REG_SMCMD, 0x80);
        reg_write32(REG_SMADDR, 0);
    } else {
        reg_write32(REG_SMCMD, 0x80);
        reg_write32(REG_SMADDR, 0);
        reg_write32(REG_SMADDR, 0);
    }
    send_row(sm, row);

    reg_write32(REG_SMISR, SMISR_DMA_IF);
    reg_write32(REG_SMISR, SMISR_ECC_FIELD_IF);
    reg_write32(REG_SMCSR, reg_read32(REG_SMCSR) | SMCR_DWR_EN);
    while reg_read32(REG_SMISR) & SMISR_DMA_IF == 0 {}
    reg_write32(REG_SMISR, SMISR_DMA_IF);

    reg_write32(REG_SMCMD, 0x10);
    wait_rb_edge()?;
    if status_pass() {
        Ok(())
    } else {
        Err(NandError::Failed)
    }
}

/// Erase block `block`.
///
/// # Safety
/// Destroys the block's contents (blocks 0-3 hold the loader).
pub unsafe fn erase_block(block: u32) -> Result<(), NandError> {
    let sm = &*core::ptr::addr_of!(SM0);
    select();
    wait_ready();
    reg_write32(REG_SMCMD, 0x60);
    send_row(sm, block * sm.page_per_block);
    reg_write32(REG_SMCMD, 0xD0);
    wait_rb_edge()?;
    if status_pass() {
        Ok(())
    } else {
        Err(NandError::Failed)
    }
}

/// True if block `block` carries a bad-block mark (factory or ours).
pub unsafe fn is_bad_block(block: u32) -> bool {
    if block == 0 {
        return false; // block 0 is guaranteed good
    }
    let sm = &*core::ptr::addr_of!(SM0);
    select();
    let first = block * sm.page_per_block;
    let second = if sm.is_mlc_nand && sm.page_size != NAND_PAGE_512B {
        first + sm.page_per_block - 1 // MLC: last page
    } else {
        first + 1
    };
    let bad = !spare_mark_good(sm, first) || !spare_mark_good(sm, second);
    reset_chip();
    bad
}

/// Bad-block bytes of one page's spare area are 0xFF.
unsafe fn spare_mark_good(sm: &NandInfo, page: u32) -> bool {
    if sm.page_size == NAND_PAGE_512B {
        if cmd_read_512(sm, 0x50, page, 0).is_err() {
            return false;
        }
        // bytes 0 and 5 of the spare area
        let b0 = reg_read32(REG_SMDATA) & 0xFF;
        let mut b5 = 0;
        for _ in 0..5 {
            b5 = reg_read32(REG_SMDATA) & 0xFF;
        }
        b0 == 0xFF && b5 == 0xFF
    } else {
        if cmd_read(sm, page, sm.page_size).is_err() {
            return false;
        }
        reg_read32(REG_SMDATA) & 0xFF == 0xFF
    }
}

/// Write a bad-block mark into the spare area of the block's first two
/// pages so [`is_bad_block`] reports it from now on.
pub unsafe fn mark_bad_block(block: u32) {
    let sm = &*core::ptr::addr_of!(SM0);
    select();
    let first = block * sm.page_per_block;
    for page in first..first + 2 {
        wait_ready();
        if sm.page_size == NAND_PAGE_512B {
            reg_write32(REG_SMCMD, 0x50);
            reg_write32(REG_SMCMD, 0x80);
            reg_write32(REG_SMADDR, 0);
        } else {
            reg_write32(REG_SMCMD, 0x80);
            reg_write32(REG_SMADDR, sm.page_size & 0xFF);
            reg_write32(REG_SMADDR, (sm.page_size >> 8) & 0xFF);
        }
        send_row(sm, page);
        for _ in 0..6 {
            reg_write32(REG_SMDATA, 0x00);
        }
        reg_write32(REG_SMCMD, 0x10);
        let _ = wait_rb_edge();
        reset_chip();
    }
}

/// True if the chip reports itself write protected.
pub unsafe fn is_write_protected() -> bool {
    select();
    wait_ready();
    reg_write32(REG_SMCMD, 0x70);
    reg_read32(REG_SMDATA) & 0x80 == 0
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
