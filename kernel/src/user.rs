use crate::memory::{self, PAGE_SIZE};

const USER_START: u64 = 0x4000_0000;
const USER_END: u64 = 0x8000_0000;
const USER_STACK_PAGE: u64 = 0x7fff_e000;
const USER_STACK_TOP: u64 = 0x7fff_f000;
const PRESENT: u64 = 1;
const WRITABLE: u64 = 2;
const USER: u64 = 4;
const PT_LOAD: u32 = 1;

pub const HELLO_ELF: &[u8] = include_bytes!("../../build/hello.elf");

pub struct LoadedProgram {
    pub entry: u64,
    pub stack: u64,
    pub root: u64,
}

#[derive(Clone, Copy)]
struct LoadSegment {
    file_offset: usize,
    virtual_address: u64,
    file_size: usize,
    memory_size: usize,
}

const EMPTY_SEGMENT: LoadSegment = LoadSegment {
    file_offset: 0,
    virtual_address: 0,
    file_size: 0,
    memory_size: 0,
};

struct InterruptGuard {
    restore_interrupts: bool,
}

impl InterruptGuard {
    unsafe fn acquire() -> Self {
        let flags: u64;
        core::arch::asm!("pushfq", "pop {}", out(reg) flags, options());
        core::arch::asm!("cli", options(nomem, nostack));
        Self {
            restore_interrupts: flags & (1 << 9) != 0,
        }
    }
}

impl Drop for InterruptGuard {
    fn drop(&mut self) {
        if self.restore_interrupts {
            unsafe { core::arch::asm!("sti", options(nomem, nostack)) };
        }
    }
}

pub unsafe fn load_elf(image: &[u8]) -> Option<LoadedProgram> {
    if image.len() < 64
        || &image[0..4] != b"\x7fELF"
        || image[4] != 2
        || image[5] != 1
        || read_u16(image, 16)? != 2
        || read_u16(image, 18)? != 62
        || read_u32(image, 20)? != 1
        || read_u16(image, 52)? != 64
        || read_u16(image, 54)? != 56
    {
        return None;
    }
    let entry = read_u64(image, 24)?;
    let phoff = usize::try_from(read_u64(image, 32)?).ok()?;
    let phnum = read_u16(image, 56)? as usize;
    if phnum == 0 || phnum > 16 || phoff.checked_add(phnum.checked_mul(56)?)? > image.len() {
        return None;
    }

    // Validate every load segment before changing CR3, then use a fixed table so
    // malformed ELF metadata cannot early-return with a user page table active.
    let mut segments = [EMPTY_SEGMENT; 16];
    let mut segment_count = 0usize;
    let mut entry_is_loaded = false;
    for index in 0..phnum {
        let ph = phoff + index * 56;
        if read_u32(image, ph)? != PT_LOAD {
            continue;
        }
        let file_offset = usize::try_from(read_u64(image, ph + 8)?).ok()?;
        let virtual_address = read_u64(image, ph + 16)?;
        let file_size = usize::try_from(read_u64(image, ph + 32)?).ok()?;
        let memory_size = usize::try_from(read_u64(image, ph + 40)?).ok()?;
        let file_end = file_offset.checked_add(file_size)?;
        let segment_end = virtual_address.checked_add(memory_size as u64)?;
        if memory_size == 0
            || file_size > memory_size
            || file_end > image.len()
            || virtual_address < USER_START
            || segment_end > USER_END
            || (virtual_address < USER_STACK_TOP && segment_end > USER_STACK_PAGE)
        {
            return None;
        }
        entry_is_loaded |= entry >= virtual_address && entry < segment_end;
        segments[segment_count] = LoadSegment {
            file_offset,
            virtual_address,
            file_size,
            memory_size,
        };
        segment_count += 1;
    }
    if segment_count == 0 || !entry_is_loaded {
        return None;
    }

    let _interrupt_guard = InterruptGuard::acquire();
    let root = memory::create_address_space()?;
    let kernel_root = memory::kernel_address_space();
    memory::switch_address_space(root);

    for segment in &segments[..segment_count] {
        let segment_end = segment.virtual_address + segment.memory_size as u64;
        let page_start = segment.virtual_address & !(PAGE_SIZE - 1);
        let page_end = (segment_end + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
        let mut page = page_start;
        while page < page_end {
            let Some(frame) = memory::allocate_frame() else {
                memory::switch_address_space(kernel_root);
                memory::destroy_address_space(root);
                return None;
            };
            if !memory::map_page_in(root, page, frame, PRESENT | WRITABLE | USER) {
                memory::free_frame(frame);
                memory::switch_address_space(kernel_root);
                memory::destroy_address_space(root);
                return None;
            }
            page += PAGE_SIZE;
        }
        if segment.file_size != 0 {
            core::ptr::copy_nonoverlapping(
                image.as_ptr().add(segment.file_offset),
                segment.virtual_address as *mut u8,
                segment.file_size,
            );
        }
    }

    let Some(stack_frame) = memory::allocate_frame() else {
        memory::switch_address_space(kernel_root);
        memory::destroy_address_space(root);
        return None;
    };
    if !memory::map_page_in(
        root,
        USER_STACK_PAGE,
        stack_frame,
        PRESENT | WRITABLE | USER,
    ) {
        memory::free_frame(stack_frame);
        memory::switch_address_space(kernel_root);
        memory::destroy_address_space(root);
        return None;
    }
    memory::switch_address_space(kernel_root);
    Some(LoadedProgram {
        entry,
        stack: USER_STACK_TOP,
        root,
    })
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(offset..offset.checked_add(2)?)?.try_into().ok()?,
    ))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?,
    ))
}

fn read_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        bytes.get(offset..offset.checked_add(8)?)?.try_into().ok()?,
    ))
}
