use core::ptr;
use core::sync::atomic::{AtomicBool, Ordering};

pub const PAGE_SIZE: u64 = 4096;
pub const IDENTITY_LIMIT: u64 = 1 << 30;
pub const HIGHER_HALF_OFFSET: u64 = 0xffff_ffff_8000_0000;
const FIRST_USABLE_ADDRESS: u64 = 0x10_0000;
const FRAME_COUNT: usize = (IDENTITY_LIMIT / PAGE_SIZE) as usize;
const BITMAP_WORDS: usize = FRAME_COUNT / 64;
const MAX_E820_ENTRIES: usize = 32;
const PRESENT: u64 = 1;
const WRITABLE: u64 = 2;
const PWT: u64 = 1 << 3;
const PCD: u64 = 1 << 4;
const USER: u64 = 1 << 2;
const HUGE_PAGE: u64 = 1 << 7;
const ADDRESS_MASK: u64 = 0x000f_ffff_ffff_f000;
pub const BOOT_INFO_MAGIC: u64 = 0x0034_3653_5953_504f;

#[repr(C)]
pub struct BootInfo {
    pub magic: u64,
    pub e820_entries: u64,
    pub e820_count: u32,
    pub version: u32,
    pub pml4_phys: u64,
    pub identity_map_limit: u64,
    pub gdt_phys: u64,
    pub framebuffer_phys: u64,
    pub framebuffer_pitch: u32,
    pub framebuffer_width: u32,
    pub framebuffer_height: u32,
    pub framebuffer_bpp: u32,
    pub framebuffer_rgb_positions: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct E820Entry {
    base: u64,
    length: u64,
    kind: u32,
    attributes: u32,
}

pub struct MemoryStats {
    pub e820_entries: usize,
}

static mut FRAME_BITMAP: [u64; BITMAP_WORDS] = [0; BITMAP_WORDS];
// A set bit means the frame is backed by usable RAM and isn't reserved for firmware,
// the loader, page tables, or the kernel. FRAME_BITMAP separately tracks ownership.
static mut ALLOCATABLE_BITMAP: [u64; BITMAP_WORDS] = [0; BITMAP_WORDS];
static mut FREE_FRAMES: usize = 0;
static mut NEXT_FRAME: usize = 0;
static mut E820_ENTRY_COUNT: usize = 0;
static FRAME_LOCK: AtomicBool = AtomicBool::new(false);
static PAGE_TABLE_LOCK: AtomicBool = AtomicBool::new(false);

struct InterruptLockGuard<'a> {
    lock: &'a AtomicBool,
    restore_interrupts: bool,
}

impl<'a> InterruptLockGuard<'a> {
    unsafe fn acquire(lock: &'a AtomicBool) -> Self {
        let flags: u64;
        core::arch::asm!("pushfq", "pop {}", out(reg) flags, options());
        core::arch::asm!("cli", options(nomem, nostack));
        while lock.swap(true, Ordering::Acquire) {
            core::hint::spin_loop();
        }
        Self {
            lock,
            restore_interrupts: flags & (1 << 9) != 0,
        }
    }
}

impl Drop for InterruptLockGuard<'_> {
    fn drop(&mut self) {
        self.lock.store(false, Ordering::Release);
        if self.restore_interrupts {
            unsafe { core::arch::asm!("sti", options(nomem, nostack)) };
        }
    }
}

extern "C" {
    static __kernel_end: u8;
}

pub unsafe fn initialize(boot_info: *const BootInfo) -> Option<MemoryStats> {
    if boot_info.is_null() {
        return None;
    }
    let info = &*boot_info;
    if info.magic != BOOT_INFO_MAGIC
        || info.version != 2
        || info.e820_count == 0
        || info.e820_count as usize > MAX_E820_ENTRIES
        || info.identity_map_limit < IDENTITY_LIMIT
        || info.pml4_phys != 0x70000
        || info.e820_entries != 0x5000
        || info.gdt_phys < 0x8000
        || info.gdt_phys >= 0x8800
    {
        return None;
    }

    let bitmap = ptr::addr_of_mut!(FRAME_BITMAP);
    let allocatable = ptr::addr_of_mut!(ALLOCATABLE_BITMAP);
    for word in 0..BITMAP_WORDS {
        (*bitmap).as_mut_ptr().add(word).write_volatile(u64::MAX);
        (*allocatable).as_mut_ptr().add(word).write_volatile(0);
    }
    FREE_FRAMES = 0;
    NEXT_FRAME = (FIRST_USABLE_ADDRESS / PAGE_SIZE) as usize;

    let entries = core::slice::from_raw_parts(
        info.e820_entries as *const E820Entry,
        info.e820_count as usize,
    );
    E820_ENTRY_COUNT = entries.len();

    // Free only complete pages explicitly reported as usable RAM.
    for entry in entries
        .iter()
        .filter(|entry| entry.kind == 1 && entry.attributes & 1 != 0)
    {
        let start = entry
            .base
            .max(FIRST_USABLE_ADDRESS)
            .saturating_add(PAGE_SIZE - 1)
            & !(PAGE_SIZE - 1);
        let end = entry.base.saturating_add(entry.length).min(IDENTITY_LIMIT) & !(PAGE_SIZE - 1);
        if start < end {
            mark_allocatable(start / PAGE_SIZE, end / PAGE_SIZE, true);
            mark_range(start / PAGE_SIZE, end / PAGE_SIZE, false);
        }
    }

    // Re-reserve every non-usable entry so overlapping firmware ranges win.
    for entry in entries
        .iter()
        .filter(|entry| entry.kind != 1 || entry.attributes & 1 == 0)
    {
        let start = entry.base.min(IDENTITY_LIMIT) / PAGE_SIZE;
        let end_address = entry.base.saturating_add(entry.length).min(IDENTITY_LIMIT);
        let end = end_address.saturating_add(PAGE_SIZE - 1) / PAGE_SIZE;
        mark_allocatable(start, end, false);
        mark_range(start, end, true);
    }

    // Keep low BIOS memory, loader data, page tables, stack, and the loaded kernel reserved.
    mark_allocatable(0, FIRST_USABLE_ADDRESS / PAGE_SIZE, false);
    mark_range(0, FIRST_USABLE_ADDRESS / PAGE_SIZE, true);
    let kernel_end = ptr::addr_of!(__kernel_end) as u64 - HIGHER_HALF_OFFSET;
    mark_allocatable(
        0x10000 / PAGE_SIZE,
        kernel_end.saturating_add(PAGE_SIZE - 1) / PAGE_SIZE,
        false,
    );
    mark_range(
        0x10000 / PAGE_SIZE,
        kernel_end.saturating_add(PAGE_SIZE - 1) / PAGE_SIZE,
        true,
    );

    Some(MemoryStats {
        e820_entries: info.e820_count as usize,
    })
}

unsafe fn mark_range(start: u64, end: u64, used: bool) {
    let first = (start as usize).min(FRAME_COUNT);
    let last = (end as usize).min(FRAME_COUNT);
    let bitmap = ptr::addr_of_mut!(FRAME_BITMAP);
    for frame in first..last {
        let word = frame / 64;
        let mask = 1u64 << (frame % 64);
        let slot = (*bitmap).as_mut_ptr().add(word);
        let old = slot.read_volatile();
        if used && old & mask == 0 {
            slot.write_volatile(old | mask);
            FREE_FRAMES = FREE_FRAMES.saturating_sub(1);
        } else if !used && old & mask != 0 {
            slot.write_volatile(old & !mask);
            FREE_FRAMES += 1;
        }
    }
}

unsafe fn mark_allocatable(start: u64, end: u64, allocatable: bool) {
    let first = (start as usize).min(FRAME_COUNT);
    let last = (end as usize).min(FRAME_COUNT);
    let bitmap = ptr::addr_of_mut!(ALLOCATABLE_BITMAP);
    for frame in first..last {
        let slot = (*bitmap).as_mut_ptr().add(frame / 64);
        let mask = 1u64 << (frame % 64);
        let old = slot.read_volatile();
        slot.write_volatile(if allocatable { old | mask } else { old & !mask });
    }
}

pub unsafe fn allocate_frame() -> Option<u64> {
    let _guard = InterruptLockGuard::acquire(&FRAME_LOCK);
    let bitmap = ptr::addr_of_mut!(FRAME_BITMAP);
    let allocatable = ptr::addr_of_mut!(ALLOCATABLE_BITMAP);
    let mut result = None;
    for offset in 0..FRAME_COUNT {
        let frame = (NEXT_FRAME + offset) % FRAME_COUNT;
        if frame < (FIRST_USABLE_ADDRESS / PAGE_SIZE) as usize {
            continue;
        }
        let mask = 1u64 << (frame % 64);
        let eligible = (*allocatable).as_ptr().add(frame / 64).read_volatile();
        let slot = (*bitmap).as_mut_ptr().add(frame / 64);
        if eligible & mask != 0 && slot.read_volatile() & mask == 0 {
            slot.write_volatile(slot.read_volatile() | mask);
            FREE_FRAMES = FREE_FRAMES.saturating_sub(1);
            NEXT_FRAME = (frame + 1) % FRAME_COUNT;
            result = Some(frame as u64 * PAGE_SIZE);
            break;
        }
    }
    result
}

pub unsafe fn free_frame(address: u64) {
    let _ = release_frame(address, None);
}

unsafe fn release_frame(address: u64, page_table_parent: Option<*mut u64>) -> bool {
    if address % PAGE_SIZE != 0 || address >= IDENTITY_LIMIT || address < FIRST_USABLE_ADDRESS {
        return false;
    }
    let _guard = InterruptLockGuard::acquire(&FRAME_LOCK);
    let frame = (address / PAGE_SIZE) as usize;
    let mask = 1u64 << (frame % 64);
    let allocatable = ptr::addr_of!(ALLOCATABLE_BITMAP)
        .cast::<u64>()
        .add(frame / 64)
        .read_volatile();
    let bitmap = ptr::addr_of_mut!(FRAME_BITMAP)
        .cast::<u64>()
        .add(frame / 64);
    let used = bitmap.read_volatile();
    // Ignore double frees and attempts to release firmware, loader, page-table, or
    // kernel frames. Only frames both eligible and currently allocated can return.
    if allocatable & mask != 0 && used & mask != 0 {
        if let Some(parent) = page_table_parent {
            parent.write_volatile(0);
        }
        bitmap.write_volatile(used & !mask);
        FREE_FRAMES += 1;
        NEXT_FRAME = NEXT_FRAME.min(frame);
        return true;
    }
    false
}

pub unsafe fn free_frame_count() -> usize {
    let _guard = InterruptLockGuard::acquire(&FRAME_LOCK);
    FREE_FRAMES
}

pub unsafe fn e820_entry_count() -> usize {
    E820_ENTRY_COUNT
}

pub unsafe fn map_page(virtual_address: u64, physical_address: u64, flags: u64) -> bool {
    let cr3 = read_cr3();
    map_page_in(cr3, virtual_address, physical_address, flags)
}

pub unsafe fn map_page_in(
    root_phys: u64,
    virtual_address: u64,
    physical_address: u64,
    flags: u64,
) -> bool {
    if virtual_address % PAGE_SIZE != 0
        || physical_address % PAGE_SIZE != 0
        || physical_address > ADDRESS_MASK
        || root_phys % PAGE_SIZE != 0
        || root_phys >= IDENTITY_LIMIT
        || !is_canonical(virtual_address)
        || flags & !(PRESENT | WRITABLE | PWT | PCD | USER) != 0
    {
        return false;
    }

    let _guard = InterruptLockGuard::acquire(&PAGE_TABLE_LOCK);

    let pml4 = (root_phys & ADDRESS_MASK) as *mut u64;
    let indices = [
        ((virtual_address >> 39) & 0x1ff) as usize,
        ((virtual_address >> 30) & 0x1ff) as usize,
        ((virtual_address >> 21) & 0x1ff) as usize,
        ((virtual_address >> 12) & 0x1ff) as usize,
    ];

    let mut table = pml4;
    let mut created_slots = [ptr::null_mut::<u64>(); 3];
    let mut created_frames = [0u64; 3];
    let mut created = 0usize;
    for index in indices[..3].iter().copied() {
        let slot = table.add(index);
        let mut entry = slot.read_volatile();
        if entry & PRESENT == 0 {
            let Some(frame) = allocate_frame() else {
                rollback_page_tables(&created_slots, &created_frames, created);
                return false;
            };
            zero_frame(frame);
            entry = frame | PRESENT | WRITABLE | (flags & USER);
            slot.write_volatile(entry);
            created_slots[created] = slot;
            created_frames[created] = frame;
            created += 1;
        } else if entry & HUGE_PAGE != 0 {
            rollback_page_tables(&created_slots, &created_frames, created);
            return false;
        }
        if flags & USER != 0 && entry & USER == 0 {
            entry |= USER;
            slot.write_volatile(entry);
        }
        table = (entry & ADDRESS_MASK) as *mut u64;
    }

    let slot = table.add(indices[3]);
    if slot.read_volatile() & PRESENT != 0 {
        rollback_page_tables(&created_slots, &created_frames, created);
        return false;
    }
    slot.write_volatile((physical_address & ADDRESS_MASK) | flags | PRESENT);
    if read_cr3() == root_phys {
        core::arch::asm!("invlpg [{}]", in(reg) virtual_address, options(nostack, preserves_flags));
    }
    true
}

/// Make a private PML4 and PDPT while retaining the bootstrap supervisor mappings.
/// User mappings occupy 1–2 GiB and therefore never modify the shared identity PDPT.
pub unsafe fn create_address_space() -> Option<u64> {
    let kernel_root = read_cr3() & ADDRESS_MASK;
    let root = allocate_frame()?;
    zero_frame(root);
    ptr::copy_nonoverlapping(kernel_root as *const u64, root as *mut u64, 512);

    let private_pdpt = match allocate_frame() {
        Some(frame) => frame,
        None => {
            free_frame(root);
            return None;
        }
    };
    zero_frame(private_pdpt);
    let kernel_pdpt = ((*((root as *mut u64).add(0))) & ADDRESS_MASK) as *const u64;
    ptr::copy_nonoverlapping(kernel_pdpt, private_pdpt as *mut u64, 512);
    *((root as *mut u64).add(0)) = private_pdpt | PRESENT | WRITABLE;
    Some(root)
}

pub unsafe fn switch_address_space(root_phys: u64) {
    core::arch::asm!("mov cr3, {}", in(reg) root_phys, options(nomem, nostack, preserves_flags));
}

pub unsafe fn kernel_address_space() -> u64 {
    read_cr3() & ADDRESS_MASK
}

pub unsafe fn user_range_mapped(root_phys: u64, start: u64, length: usize) -> bool {
    if length == 0 {
        return true;
    }
    let Some(end) = start.checked_add(length as u64 - 1) else {
        return false;
    };
    if start < 0x4000_0000 || end >= 0x8000_0000 || root_phys >= IDENTITY_LIMIT {
        return false;
    }
    let mut address = start & !(PAGE_SIZE - 1);
    let last = end & !(PAGE_SIZE - 1);
    loop {
        if !is_user_page(root_phys, address) {
            return false;
        }
        if address == last {
            return true;
        }
        address = address.saturating_add(PAGE_SIZE);
    }
}

unsafe fn is_user_page(root_phys: u64, address: u64) -> bool {
    let indices = [
        ((address >> 39) & 0x1ff) as usize,
        ((address >> 30) & 0x1ff) as usize,
        ((address >> 21) & 0x1ff) as usize,
        ((address >> 12) & 0x1ff) as usize,
    ];
    let mut table = root_phys as *const u64;
    for index in indices {
        let entry = table.add(index).read_volatile();
        if entry & (PRESENT | USER) != (PRESENT | USER) {
            return false;
        }
        table = (entry & ADDRESS_MASK) as *const u64;
    }
    true
}

/// Release the user range and private paging structures created by `create_address_space`.
pub unsafe fn destroy_address_space(root_phys: u64) {
    if root_phys >= IDENTITY_LIMIT || root_phys % PAGE_SIZE != 0 {
        return;
    }
    let root = root_phys as *mut u64;
    let pdpt_entry = root.add(0).read_volatile();
    if pdpt_entry & PRESENT == 0 {
        free_frame(root_phys);
        return;
    }
    let pdpt_phys = pdpt_entry & ADDRESS_MASK;
    if pdpt_phys >= IDENTITY_LIMIT {
        return;
    }
    let pdpt = pdpt_phys as *mut u64;
    let user_pd_entry = pdpt.add(1).read_volatile();
    if user_pd_entry & PRESENT != 0 && user_pd_entry & HUGE_PAGE == 0 {
        let pd_phys = user_pd_entry & ADDRESS_MASK;
        let pd = pd_phys as *mut u64;
        if pd_phys < IDENTITY_LIMIT {
            for pde_index in 0..512 {
                let pde = pd.add(pde_index).read_volatile();
                if pde & PRESENT == 0 || pde & HUGE_PAGE != 0 {
                    continue;
                }
                let pt_phys = pde & ADDRESS_MASK;
                if pt_phys >= IDENTITY_LIMIT {
                    continue;
                }
                let pt = pt_phys as *mut u64;
                for pte_index in 0..512 {
                    let pte = pt.add(pte_index).read_volatile();
                    if pte & (PRESENT | USER) == PRESENT | USER {
                        free_frame(pte & ADDRESS_MASK);
                    }
                }
                free_frame(pt_phys);
            }
            free_frame(pd_phys);
        }
    }
    free_frame(pdpt_phys);
    free_frame(root_phys);
}

unsafe fn read_cr3() -> u64 {
    let value: u64;
    core::arch::asm!("mov {}, cr3", out(reg) value, options(nomem, nostack, preserves_flags));
    value & ADDRESS_MASK
}

/// Remove a 4 KiB mapping and return its physical frame without freeing it.
/// The caller owns the mapped frame and may release it with `free_frame` when safe.
pub unsafe fn unmap_page(virtual_address: u64) -> Option<u64> {
    if virtual_address % PAGE_SIZE != 0 || !is_canonical(virtual_address) {
        return None;
    }

    let _guard = InterruptLockGuard::acquire(&PAGE_TABLE_LOCK);

    let mut cr3: u64;
    core::arch::asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack, preserves_flags));
    cr3 &= ADDRESS_MASK;
    if cr3 >= IDENTITY_LIMIT {
        return None;
    }
    let indices = [
        ((virtual_address >> 39) & 0x1ff) as usize,
        ((virtual_address >> 30) & 0x1ff) as usize,
        ((virtual_address >> 21) & 0x1ff) as usize,
        ((virtual_address >> 12) & 0x1ff) as usize,
    ];

    let mut table = cr3 as *mut u64;
    let mut parent_slots = [ptr::null_mut::<u64>(); 3];
    let mut table_frames = [0u64; 3];
    for (level, index) in indices[..3].iter().copied().enumerate() {
        let slot = table.add(index);
        let entry = slot.read_volatile();
        if entry & PRESENT == 0 || entry & HUGE_PAGE != 0 {
            return None;
        }
        let next = entry & ADDRESS_MASK;
        if next >= IDENTITY_LIMIT {
            return None;
        }
        parent_slots[level] = slot;
        table_frames[level] = next;
        table = next as *mut u64;
    }

    let leaf = table.add(indices[3]);
    let entry = leaf.read_volatile();
    if entry & PRESENT == 0 {
        return None;
    }
    leaf.write_volatile(0);
    core::arch::asm!("invlpg [{}]", in(reg) virtual_address, options(nostack, preserves_flags));

    // Reclaim now-empty page tables from the bottom up. Bootstrap tables are
    // reserved and fail release_frame's allocatable check, so they stay in place.
    for level in (0..3).rev() {
        let candidate = table_frames[level] as *const u64;
        let mut empty = true;
        for index in 0..512 {
            if candidate.add(index).read_volatile() != 0 {
                empty = false;
                break;
            }
        }
        if !empty || !release_frame(table_frames[level], Some(parent_slots[level])) {
            break;
        }
    }
    Some(entry & ADDRESS_MASK)
}

unsafe fn rollback_page_tables(slots: &[*mut u64; 3], frames: &[u64; 3], count: usize) {
    for index in (0..count).rev() {
        slots[index].write_volatile(0);
        free_frame(frames[index]);
    }
}

/// Exercise allocation, a temporary 4 KiB mapping, access through that mapping,
/// unmapping, and frame release. Intended for the interactive `paging` command.
pub fn paging_self_check() -> bool {
    const TEST_ADDRESS: u64 = 0xffff_ffff_e000_0000;
    const PATTERN: u64 = 0x4f50_5359_5041_4745;

    unsafe {
        let Some(frame) = allocate_frame() else {
            return false;
        };
        if !map_page(TEST_ADDRESS, frame, PRESENT | WRITABLE) {
            free_frame(frame);
            return false;
        }

        let mapped = TEST_ADDRESS as *mut u64;
        mapped.write_volatile(PATTERN);
        let passed = mapped.read_volatile() == PATTERN;
        match unmap_page(TEST_ADDRESS) {
            Some(unmapped) if unmapped == frame => {
                free_frame(unmapped);
                passed
            }
            _ => false,
        }
    }
}

fn is_canonical(address: u64) -> bool {
    let upper = address >> 48;
    if address & (1 << 47) == 0 {
        upper == 0
    } else {
        upper == 0xffff
    }
}

unsafe fn zero_frame(address: u64) {
    let page = address as *mut u64;
    for index in 0..(PAGE_SIZE as usize / core::mem::size_of::<u64>()) {
        page.add(index).write_volatile(0);
    }
}
