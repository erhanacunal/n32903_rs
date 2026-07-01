//! MMU (Memory Management Unit) for N32903 (W55FA93) ARM926EJ-S
//! Ported from wb_mmu.c and wb_sysctl.s
//!
//! Builds a 4 GB section-descriptor translation table (16 KB aligned).
//! First 64 MB are cacheable/write-back; addresses above 64 MB are
//! non-cacheable.  A non-cacheable shadow of the first 64 MB is mapped
//! at `0x8000_0000` (used for DMA / frame buffers).
//!
//! # Safety
//!
//! All functions are `unsafe` — they write to CP15 and build page tables
//! in SDRAM.

// ============================================================================
// Section Table (16 KB aligned, 4096 entries × 4 bytes each)
// ============================================================================

/// MMU section descriptor table — 4096 entries covering 4 GB.
/// Each entry maps a 1 MB section.
#[repr(align(16384))]
struct SectionTable([u32; 4096]);

static mut MMU_SECTION_TABLE: SectionTable = SectionTable([0u32; 4096]);

/// Initialisation guard
static mut MMU_INITIALIZED: bool = false;

// ============================================================================
// Section Descriptor Encoding
// ============================================================================

/// Section descriptor layout (ARM926EJ-S short-descriptor format):
///
/// | Bits   | Field               |
/// |--------|---------------------|
/// | 31:20  | Base address        |
/// | 19:12  | Should be zero      |
/// | 11:10  | AP (access perm)    |
/// | 9      | Should be zero      |
/// | 8:5    | Domain              |
/// | 4      | Must be 1           |
/// | 3:2    | C, B (cache/buffer) |
/// | 1:0    | Descriptor type (10 = section) |

/// Access permission: full read/write
const AP_RW: u32 = 0xC00;

/// Domain 0, client
const DOMAIN0: u32 = 0x000;

/// Section type
const SECTION_TYPE: u32 = 0x02;

/// Cacheable, write-back (C=1, B=1)
const CACHE_WB: u32 = 0x0C;

/// Non-cacheable, non-bufferable (C=0, B=0)
const CACHE_NONE: u32 = 0x00;

/// Full descriptor for a cacheable section
const fn section_desc(base_mb: u32, cacheable: bool) -> u32 {
    let cb = if cacheable { CACHE_WB } else { CACHE_NONE };
    (base_mb << 20) | AP_RW | DOMAIN0 | 0x10 | cb | SECTION_TYPE
}

// ============================================================================
// CP15 Setup
// ============================================================================

/// Write the translation table base address to CP15, set domain access
/// control, and enable I-cache, D-cache, and MMU.
unsafe fn cp15_setup(ttb: u32) {
    core::arch::asm!(
        // Write translation table base (c2)
        "MCR p15, 0, {0}, c2, c0, 0",
        // Domain access control: client for domain 0
        "MOV {0}, #0x40000000",
        "MCR p15, 0, {0}, c3, c0, 0",
        // Read control register, enable I-cache, D-cache, MMU
        "MRC p15, 0, {0}, c1, c0, 0",
        "ORR {0}, {0}, #0x1000",  // I-cache
        "ORR {0}, {0}, #0x5",     // D-cache + MMU
        "MCR p15, 0, {0}, c1, c0, 0",
        in(reg) ttb,
    );
}

// ============================================================================
// Public API
// ============================================================================

/// Build the MMU section table and enable the MMU + caches.
///
/// Layout:
///   - 0x0000_0000 – 0x03FF_FFFF (first 64 MB): cacheable, write-back
///   - 0x0400_0000 – 0x7FFF_FFFF: non-cacheable
///   - 0x8000_0000 – 0x83FF_FFFF: non-cacheable mirror of first 64 MB
///   - 0x8400_0000 – 0xFFFF_FFFF: non-cacheable
///
/// `_cache_mode` is accepted for API compatibility (write-back is always used
/// in section-table mode).
pub unsafe fn mmu_init_table(_cache_mode: i32) -> i32 {
    if MMU_INITIALIZED {
        return 0;
    }

    let tbl = &raw mut MMU_SECTION_TABLE;

    // First 64 MB: cacheable, write-back
    for i in 0..64usize {
        (*tbl).0[i] = section_desc(i as u32, true);
    }

    // 64 MB – 2 GB: non-cacheable
    for i in 64..2048usize {
        (*tbl).0[i] = section_desc(i as u32, false);
    }

    // 2 GB – 2 GB + 64 MB: non-cacheable mirror of first 64 MB
    for i in 2048..2112usize {
        (*tbl).0[i] = section_desc((i - 2048) as u32, false);
    }

    // 2 GB + 64 MB – 4 GB: non-cacheable
    for i in 2112..4096usize {
        (*tbl).0[i] = section_desc(i as u32, false);
    }

    MMU_INITIALIZED = true;

    // Write TTB to CP15 and enable MMU + caches
    cp15_setup((*tbl).0.as_ptr() as u32);

    0
}

// ============================================================================
// Heap Support (_sbrk)
// ============================================================================

extern "C" {
    static __heap_start__: u8;
    static __heap_end__: u8;
}

/// Simple `_sbrk` implementation for the GCC toolchain.
/// Provides heap memory between `__heap_start__` and `__heap_end__`
/// linker symbols.
#[no_mangle]
pub unsafe extern "C" fn _sbrk(incr: i32) -> *mut u8 {
    static mut CURRENT_HEAP_END: *const u8 = core::ptr::null();

    if CURRENT_HEAP_END.is_null() {
        CURRENT_HEAP_END = &__heap_start__ as *const u8;
    }

    let current = CURRENT_HEAP_END;
    let incr_aligned = ((incr as usize) + 3) & !3; // align to 4 bytes
    let new_end = unsafe { current.add(incr_aligned) };

    if new_end as *const u8 > &__heap_end__ as *const u8 {
        // Out of memory
        return (-1isize as usize) as *mut u8;
    }

    CURRENT_HEAP_END = new_end;
    current as *mut u8
}
