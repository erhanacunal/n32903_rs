//! Cache and CP15 control for N32903 (W55FA93) ARM926EJ-S
//! Ported from wb_cache.c, wb_dcache.s, and wb_sysctl.s
//!
//! Provides I-cache, D-cache, write-buffer, and MMU enable/disable
//! via CP15 system control coprocessor operations.
//!
//! # Safety
//!
//! All functions are `unsafe` — they use inline assembly to access CP15.

use crate::registers::*;

// ============================================================================
// Constants
// ============================================================================

/// Cache operation mode
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(i32)]
pub enum CacheMode {
    WriteBack = 0,
    WriteThrough = 1,
    Disabled = -1,
}

/// Which cache(s) to flush
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum CacheType {
    ICache = 6,
    DCache = 7,
    Both = 8,
}

/// Cache size for lock operations
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum CacheSize {
    Cache4M = 2,
    Cache8M = 3,
    Cache16M = 4,
    Cache32M = 5,
}

// ============================================================================
// Static State
// ============================================================================

static mut IS_CACHE_ON: bool = false;
static mut CACHE_MODE: CacheMode = CacheMode::Disabled;

// ============================================================================
// SDRAM Size Detection
// ============================================================================

/// Read the total SDRAM size in megabytes from the SDSIZE registers.
/// Returns 0 on failure.
pub unsafe fn sdram_size_mb() -> i32 {
    let mut total: i32 = 0;

    let reg0 = reg_read32(REG_SDSIZE0) & 0x07;
    total += match reg0 {
        1 => 2,
        2 => 4,
        3 => 8,
        4 => 16,
        5 => 32,
        6 => 64,
        _ => 0,
    };

    let reg1 = reg_read32(REG_SDSIZE1) & 0x07;
    total += match reg1 {
        1 => 2,
        2 => 4,
        3 => 8,
        4 => 16,
        5 => 32,
        6 => 64,
        _ => 0,
    };

    if total != 0 { total } else { -1 }
}

// ============================================================================
// D-Cache Clean & Invalidate
// ============================================================================

/// Clean and invalidate the entire D-cache.
/// Loops on CP15 test-clean-and-invalidate until complete.
#[inline(always)]
pub unsafe fn dcache_clean_invalidate() {
    core::arch::asm!(
        "1:",
        "MRC p15, 0, {tmp}, c7, c14, 3",
        "CMP {tmp}, #0",
        "BNE 1b",
        tmp = out(reg) _,
        options(nomem, nostack),
    );
}

// ============================================================================
// Cache Enable / Disable
// ============================================================================

/// Enable caches by building the MMU translation table and enabling
/// the MMU, I-cache, and D-cache in CP15.
///
/// `mode` selects write-back or write-through caching.
pub unsafe fn cache_enable(mode: CacheMode) {
    crate::mmu::mmu_init_table(mode as i32);
    IS_CACHE_ON = true;
    CACHE_MODE = mode;
}

/// Disable caches and MMU.
/// Flushes D-cache, invalidates I-cache, drains write buffer,
/// then clears the MMU enable bit in CP15 control register.
pub unsafe fn cache_disable() {
    dcache_clean_invalidate();

    core::arch::asm!(
        "MOV r0, #0",
        "MCR p15, 0, r0, c7, c5, 0",   // invalidate I-cache
        "MCR p15, 0, r0, c7, c6, 0",   // invalidate D-cache
        "MCR p15, 0, r0, c7, c10, 4",  // drain write buffer
        "MRC p15, 0, r0, c1, c0, 0",   // read control register
        "BIC r0, r0, #0x01",           // clear MMU enable bit
        "MCR p15, 0, r0, c1, c0, 0",   // write control register
        out("r0") _,
        options(nomem, nostack),
    );

    IS_CACHE_ON = false;
    CACHE_MODE = CacheMode::Disabled;
}

// ============================================================================
// Cache Flush
// ============================================================================

/// Flush (clean and/or invalidate) caches.
///
/// - `ICache` — invalidate I-cache
/// - `DCache` — clean+invalidate D-cache, drain write buffer
/// - `Both` — both of the above
pub unsafe fn cache_flush(cache_type: CacheType) {
    match cache_type {
        CacheType::ICache => {
            core::arch::asm!(
                "MOV r0, #0",
                "MCR p15, 0, r0, c7, c5, 0",  // invalidate I-cache
                out("r0") _,
                options(nomem, nostack),
            );
        }
        CacheType::DCache => {
            dcache_clean_invalidate();
            core::arch::asm!(
                "MOV r0, #0",
                "MCR p15, 0, r0, c7, c10, 4", // drain write buffer
                out("r0") _,
                options(nomem, nostack),
            );
        }
        CacheType::Both => {
            dcache_clean_invalidate();
            core::arch::asm!(
                "MOV r0, #0",
                "MCR p15, 0, r0, c7, c5, 0",   // invalidate I-cache
                "MCR p15, 0, r0, c7, c10, 4",  // drain write buffer
                out("r0") _,
                options(nomem, nostack),
            );
        }
    }
}

/// Invalidate both I-cache and D-cache.
pub unsafe fn cache_invalidate() {
    core::arch::asm!(
        "MOV r0, #0",
        "MCR p15, 0, r0, c7, c7, 0",  // invalidate both caches
        out("r0") _,
        options(nomem, nostack),
    );
}

// ============================================================================
// Cache Locking
// ============================================================================

/// Lock a code region into I-cache way 3.
/// `addr` must be aligned; `size` is rounded up to 16-byte blocks.
pub unsafe fn cache_lock_code(addr: u32, size: u32) {
    // Select way 3 for locking
    core::arch::asm!(
        "MRC p15, 0, r0, c9, c0, 1",
        "ORR r0, r0, #0x07",
        "MCR p15, 0, r0, c9, c0, 1",
        out("r0") _,
    );

    let cnt = if size % 16 == 0 { size / 16 } else { size / 16 + 1 };
    let mut a = addr;

    for _ in 0..cnt {
        core::arch::asm!(
            "MCR p15, 0, {0}, c7, c13, 1",
            in(reg) a,
        );
        a += 16;
    }

    // Lock way 3
    core::arch::asm!(
        "MRC p15, 0, r0, c9, c0, 1",
        "BIC r0, r0, #0x07",
        "ORR r0, r0, #0x08",
        "MCR p15, 0, r0, c9, c0, 1",
        out("r0") _,
    );
}

/// Unlock I-cache way 3.
pub unsafe fn cache_unlock_code() {
    core::arch::asm!(
        "MRC p15, 0, r0, c9, c0, 1",
        "BIC r0, r0, #0x08",
        "MCR p15, 0, r0, c9, c0, 1",
        out("r0") _,
    );
}

// ============================================================================
// State Queries
// ============================================================================

/// Check whether the cache is currently enabled.
pub fn cache_is_enabled() -> bool {
    unsafe { IS_CACHE_ON }
}

/// Get the current cache mode.
pub fn cache_get_mode() -> CacheMode {
    unsafe { CACHE_MODE }
}
