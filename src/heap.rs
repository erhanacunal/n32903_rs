//! Heap allocator for the `alloc` crate (`Box`, `Vec`, `String`, ...).
//!
//! First-fit free list kept in address order; freed blocks are merged with
//! their neighbours.  Block sizes come from the `Layout` passed to
//! `dealloc`, so allocations carry no header.  Every block is a multiple of
//! 8 bytes and 8-byte aligned.
//!
//! The ARM926EJ-S has no atomic read-modify-write instructions, so the free
//! list is protected by masking IRQ/FIQ for the duration of each call.
//! Allocating from an interrupt handler is therefore safe.
//!
//! # Usage
//!
//! Enable the `heap` feature, then hand the allocator its memory once,
//! before the first allocation:
//!
//! ```rust,ignore
//! extern crate alloc;
//!
//! extern "C" {
//!     static __heap_start__: u8;
//!     static __heap_end__: u8;
//! }
//!
//! unsafe {
//!     let start = core::ptr::addr_of!(__heap_start__) as usize;
//!     let end = core::ptr::addr_of!(__heap_end__) as usize;
//!     n32903_rs::heap::init(start, end - start);
//! }
//! let v = alloc::vec![1, 2, 3];
//! ```

use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;
use core::mem::size_of;
use core::ptr;

/// Block granularity and minimum alignment.  A free block must be able to
/// hold a `Hole`, so this is also the smallest block.
const BLOCK: usize = 8;

/// Free block header, stored in the free memory itself.
#[repr(C)]
struct Hole {
    size: usize,
    next: *mut Hole,
}

const _: () = assert!(size_of::<Hole>() <= BLOCK);

struct Inner {
    /// Dummy list head (size 0); `head.next` is the lowest free block.
    head: Hole,
    start: usize,
    size: usize,
    used: usize,
}

/// The allocator.  The crate's global instance is [`HEAP`].
pub struct Heap {
    inner: UnsafeCell<Inner>,
}

// Access is serialised by `critical_section`.
unsafe impl Sync for Heap {}

/// Heap usage counters.
#[derive(Clone, Copy, Debug)]
pub struct HeapStats {
    /// Total heap size in bytes
    pub size: usize,
    /// Bytes currently allocated (rounded up to the block size)
    pub used: usize,
    /// Bytes free
    pub free: usize,
    /// Largest single free block — the biggest allocation that can succeed
    pub largest_free: usize,
}

impl Heap {
    /// An empty heap; every allocation fails until [`Heap::init`].
    pub const fn empty() -> Self {
        Heap {
            inner: UnsafeCell::new(Inner {
                head: Hole { size: 0, next: ptr::null_mut() },
                start: 0,
                size: 0,
                used: 0,
            }),
        }
    }

    /// Give the heap the memory `[start, start + size)`.
    ///
    /// # Safety
    /// The region must be RAM that nothing else uses (no code, data, or
    /// stack), and this must be called only once, before any allocation.
    pub unsafe fn init(&self, start: usize, size: usize) {
        let aligned = align_up(start, BLOCK);
        let size = size.saturating_sub(aligned - start) & !(BLOCK - 1);

        critical_section(|| {
            let inner = &mut *self.inner.get();
            inner.start = aligned;
            inner.size = size;
            inner.used = 0;
            inner.head.next = ptr::null_mut();
            if size >= BLOCK {
                let hole = aligned as *mut Hole;
                hole.write(Hole { size, next: ptr::null_mut() });
                inner.head.next = hole;
            }
        });
    }

    /// Current usage counters.
    pub fn stats(&self) -> HeapStats {
        unsafe {
            critical_section(|| {
                let inner = &*self.inner.get();
                let mut largest = 0;
                let mut cur = inner.head.next;
                while !cur.is_null() {
                    largest = largest.max((*cur).size);
                    cur = (*cur).next;
                }
                HeapStats {
                    size: inner.size,
                    used: inner.used,
                    free: inner.size - inner.used,
                    largest_free: largest,
                }
            })
        }
    }

    /// First fit: take the first free block that can hold `size` bytes at
    /// `align`.  The unused space in front of and behind the allocation
    /// stays in the free list.
    unsafe fn allocate(inner: &mut Inner, size: usize, align: usize) -> *mut u8 {
        let mut prev: *mut Hole = &mut inner.head;
        let mut cur = inner.head.next;

        while !cur.is_null() {
            let hole_start = cur as usize;
            let hole_end = hole_start + (*cur).size;
            let next = (*cur).next;

            // All holes are BLOCK aligned, so the front gap is either 0 or
            // a whole number of blocks.
            let addr = align_up(hole_start, align);

            if let Some(end) = addr.checked_add(size) {
                if end <= hole_end {
                    // Remainder behind the allocation becomes a hole
                    let mut link = next;
                    let back = hole_end - end;
                    if back >= BLOCK {
                        let tail = end as *mut Hole;
                        tail.write(Hole { size: back, next });
                        link = tail;
                    }

                    let front = addr - hole_start;
                    if front >= BLOCK {
                        // Keep the front gap where `cur` already is
                        (*cur).size = front;
                        (*cur).next = link;
                    } else {
                        (*prev).next = link;
                    }

                    inner.used += size;
                    return addr as *mut u8;
                }
            }

            prev = cur;
            cur = next;
        }

        ptr::null_mut()
    }

    /// Return a block to the free list, merging it with adjacent holes.
    unsafe fn free(inner: &mut Inner, addr: usize, size: usize) {
        let head: *mut Hole = &mut inner.head;

        // Find the last hole below `addr`
        let mut prev = head;
        while !(*prev).next.is_null() && ((*prev).next as usize) < addr {
            prev = (*prev).next;
        }
        let next = (*prev).next;

        // Merge with the hole in front, or link in a new one
        let node = if prev != head && prev as usize + (*prev).size == addr {
            (*prev).size += size;
            prev
        } else {
            let node = addr as *mut Hole;
            node.write(Hole { size, next });
            (*prev).next = node;
            node
        };

        // Merge with the hole behind
        if !next.is_null() && node as usize + (*node).size == next as usize {
            (*node).size += (*next).size;
            (*node).next = (*next).next;
        }

        inner.used -= size;
    }
}

unsafe impl GlobalAlloc for Heap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let (size, align) = block_layout(layout);
        critical_section(|| Self::allocate(&mut *self.inner.get(), size, align))
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let (size, _) = block_layout(layout);
        critical_section(|| Self::free(&mut *self.inner.get(), ptr as usize, size));
    }
}

/// The global allocator used by `alloc`.
#[global_allocator]
pub static HEAP: Heap = Heap::empty();

/// Initialise the global heap with `[start, start + size)`.
///
/// # Safety
/// See [`Heap::init`].
pub unsafe fn init(start: usize, size: usize) {
    HEAP.init(start, size);
}

/// Usage counters of the global heap.
pub fn stats() -> HeapStats {
    HEAP.stats()
}

/// Block size and alignment actually used for `layout`.
fn block_layout(layout: Layout) -> (usize, usize) {
    let size = align_up(layout.size().max(BLOCK), BLOCK);
    let align = layout.align().max(BLOCK);
    (size, align)
}

fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}

/// Run `f` with IRQ and FIQ masked, restoring the previous mask afterwards.
/// Must run in a privileged mode (the startup code leaves us in SVC).
#[inline(always)]
unsafe fn critical_section<R>(f: impl FnOnce() -> R) -> R {
    let cpsr: u32;
    core::arch::asm!(
        "mrs {old}, cpsr",
        "orr {tmp}, {old}, #0xC0",
        "msr cpsr_c, {tmp}",
        old = out(reg) cpsr,
        tmp = out(reg) _,
        options(nostack, preserves_flags),
    );
    let r = f();
    core::arch::asm!(
        "msr cpsr_c, {0}",
        in(reg) cpsr,
        options(nostack, preserves_flags),
    );
    r
}
