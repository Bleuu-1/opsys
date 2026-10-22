use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;
use core::ptr;
use core::sync::atomic::{AtomicBool, Ordering};

use crate::memory;

const HEAP_SIZE: usize = 8 * 1024 * 1024;
const HEAP_START: u64 = memory::HIGHER_HALF_OFFSET + memory::IDENTITY_LIMIT;
const PRESENT: u64 = 1;
const WRITABLE: u64 = 2;

#[repr(C)]
struct FreeBlock {
    size: usize,
    next: *mut FreeBlock,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct AllocationHeader {
    block_start: usize,
    block_size: usize,
}

struct FreeList {
    head: *mut FreeBlock,
}

impl FreeList {
    const fn empty() -> Self {
        Self {
            head: ptr::null_mut(),
        }
    }

    unsafe fn initialize(&mut self, start: usize, size: usize) {
        let original_start = start;
        let start = align_up(start, core::mem::align_of::<FreeBlock>());
        let size = (size - (start - original_start)) & !(core::mem::align_of::<FreeBlock>() - 1);
        self.head = start as *mut FreeBlock;
        (*self.head).size = size;
        (*self.head).next = ptr::null_mut();
    }

    unsafe fn allocate(&mut self, layout: Layout) -> *mut u8 {
        let requested = layout.size().max(1);
        let alignment = layout
            .align()
            .max(core::mem::align_of::<AllocationHeader>());
        let mut previous: *mut FreeBlock = ptr::null_mut();
        let mut current = self.head;

        while !current.is_null() {
            let block_start = current as usize;
            let block_size = (*current).size;
            let block_end = match block_start.checked_add(block_size) {
                Some(end) => end,
                None => return ptr::null_mut(),
            };
            let header_size = core::mem::size_of::<AllocationHeader>();
            let mut user = match block_start
                .checked_add(header_size)
                .and_then(|v| align_up_checked(v, alignment))
            {
                Some(value) => value,
                None => return ptr::null_mut(),
            };
            let mut allocation_start = user - header_size;
            if allocation_start - block_start < core::mem::size_of::<FreeBlock>() {
                user = match block_start
                    .checked_add(header_size)
                    .and_then(|v| align_up_checked(v, alignment))
                {
                    Some(value) => value,
                    None => return ptr::null_mut(),
                };
                allocation_start = block_start;
            }
            let payload_end = match user.checked_add(requested) {
                Some(end) => end,
                None => return ptr::null_mut(),
            };
            let mut allocation_end =
                match align_up_checked(payload_end, core::mem::align_of::<FreeBlock>()) {
                    Some(end) => end,
                    None => return ptr::null_mut(),
                };
            if allocation_end > block_end {
                previous = current;
                current = (*current).next;
                continue;
            }

            let trailing = block_end - allocation_end;
            if trailing < core::mem::size_of::<FreeBlock>() {
                allocation_end = block_end;
            }
            let leading = allocation_start - block_start;
            let next = (*current).next;
            if previous.is_null() {
                self.head = next;
            } else {
                (*previous).next = next;
            }

            if leading >= core::mem::size_of::<FreeBlock>() {
                insert_free_block(&mut self.head, block_start, leading);
            }
            let remaining = block_end - allocation_end;
            if remaining >= core::mem::size_of::<FreeBlock>() {
                insert_free_block(&mut self.head, allocation_end, remaining);
            }

            let header = (user - header_size) as *mut AllocationHeader;
            header.write(AllocationHeader {
                block_start: allocation_start,
                block_size: allocation_end - allocation_start,
            });
            return user as *mut u8;
        }
        ptr::null_mut()
    }

    unsafe fn deallocate(&mut self, pointer: *mut u8) {
        if pointer.is_null() {
            return;
        }
        let header = (pointer as usize - core::mem::size_of::<AllocationHeader>())
            as *const AllocationHeader;
        let allocation = header.read();
        insert_free_block(
            &mut self.head,
            allocation.block_start,
            allocation.block_size,
        );
        coalesce(&mut self.head);
    }
}

unsafe fn insert_free_block(head: &mut *mut FreeBlock, start: usize, size: usize) {
    let node = start as *mut FreeBlock;
    (*node).size = size;
    let mut previous: *mut FreeBlock = ptr::null_mut();
    let mut current = *head;
    while !current.is_null() && (current as usize) < start {
        previous = current;
        current = (*current).next;
    }
    (*node).next = current;
    if previous.is_null() {
        *head = node;
    } else {
        (*previous).next = node;
    }
}

unsafe fn coalesce(head: &mut *mut FreeBlock) {
    let mut current = *head;
    while !current.is_null() {
        let next = (*current).next;
        if !next.is_null() && (current as usize + (*current).size == next as usize) {
            (*current).size += (*next).size;
            (*current).next = (*next).next;
        } else {
            current = next;
        }
    }
}

fn align_up(value: usize, alignment: usize) -> usize {
    (value + alignment - 1) & !(alignment - 1)
}

fn align_up_checked(value: usize, alignment: usize) -> Option<usize> {
    value
        .checked_add(alignment - 1)
        .map(|v| v & !(alignment - 1))
}

struct KernelAllocator {
    locked: AtomicBool,
    free_list: UnsafeCell<FreeList>,
}

unsafe impl Sync for KernelAllocator {}

impl KernelAllocator {
    const fn new() -> Self {
        Self {
            locked: AtomicBool::new(false),
            free_list: UnsafeCell::new(FreeList::empty()),
        }
    }

    unsafe fn lock(&self) -> bool {
        let flags: u64;
        core::arch::asm!("pushfq", "pop {}", out(reg) flags, options());
        core::arch::asm!("cli", options(nomem, nostack));
        while self.locked.swap(true, Ordering::Acquire) {
            core::hint::spin_loop();
        }
        flags & (1 << 9) != 0
    }

    unsafe fn unlock(&self, restore_interrupts: bool) {
        self.locked.store(false, Ordering::Release);
        if restore_interrupts {
            core::arch::asm!("sti", options(nomem, nostack));
        }
    }
}

#[global_allocator]
static ALLOCATOR: KernelAllocator = KernelAllocator::new();

unsafe impl GlobalAlloc for KernelAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let interrupts_were_enabled = self.lock();
        let result = (&mut *self.free_list.get()).allocate(layout);
        self.unlock(interrupts_were_enabled);
        result
    }

    unsafe fn dealloc(&self, pointer: *mut u8, _layout: Layout) {
        let interrupts_were_enabled = self.lock();
        (&mut *self.free_list.get()).deallocate(pointer);
        self.unlock(interrupts_were_enabled);
    }
}

pub unsafe fn initialize() -> bool {
    let page_count = HEAP_SIZE / memory::PAGE_SIZE as usize;
    for page in 0..page_count {
        let Some(frame) = memory::allocate_frame() else {
            return false;
        };
        if !memory::map_page(
            HEAP_START + page as u64 * memory::PAGE_SIZE,
            frame,
            PRESENT | WRITABLE,
        ) {
            memory::free_frame(frame);
            return false;
        }
        ptr::write_bytes(frame as *mut u8, 0, memory::PAGE_SIZE as usize);
    }

    let heap = &mut *ALLOCATOR.free_list.get();
    heap.initialize(HEAP_START as usize, HEAP_SIZE);
    true
}

pub const fn size() -> usize {
    HEAP_SIZE
}
