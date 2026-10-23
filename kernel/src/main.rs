#![no_std]
#![no_main]

extern crate alloc;

mod ata;
mod auth;
mod bleuu;
mod fs;
mod graphics;
mod heap;
mod keyboard;
mod memory;
mod mouse;
mod serial;
mod system;
mod task;
mod user;

use core::fmt::Write as FmtWrite;
use core::panic::PanicInfo;

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct IdtEntry {
    offset_low: u16,
    selector: u16,
    ist: u8,
    attributes: u8,
    offset_mid: u16,
    offset_high: u32,
    reserved: u32,
}
impl IdtEntry {
    const MISSING: Self = Self {
        offset_low: 0,
        selector: 0,
        ist: 0,
        attributes: 0,
        offset_mid: 0,
        offset_high: 0,
        reserved: 0,
    };
    fn new(handler: usize) -> Self {
        Self::with_attributes(handler, 0x8e)
    }
    fn new_user(handler: usize) -> Self {
        Self::with_attributes(handler, 0xee)
    }
    fn with_attributes(handler: usize, attributes: u8) -> Self {
        Self {
            offset_low: handler as u16,
            selector: 0x18,
            ist: 0,
            attributes,
            offset_mid: (handler >> 16) as u16,
            offset_high: (handler >> 32) as u32,
            reserved: 0,
        }
    }
}
#[repr(C, packed)]
struct IdtPointer {
    limit: u16,
    base: u64,
}
static mut IDT: [IdtEntry; 256] = [IdtEntry::MISSING; 256];

struct Console {
    row: usize,
    column: usize,
    attribute: u8,
    cells: [[u16; 80]; 25],
    input: [u8; 80],
    input_len: usize,
    shift: bool,
    caps_lock: bool,
}

static mut CONSOLE: Console = Console {
    row: 0,
    column: 0,
    attribute: 0x0f,
    cells: [[0x0f20; 80]; 25],
    input: [0; 80],
    input_len: 0,
    shift: false,
    caps_lock: false,
};

static mut TIMER_TICKS: u64 = 0;

#[repr(C, align(16))]
struct TaskStateSegment {
    bytes: [u8; 104],
}

static mut TSS: TaskStateSegment = TaskStateSegment { bytes: [0; 104] };

unsafe fn init_tss(gdt_phys: u64) {
    let tss_base = core::ptr::addr_of!(TSS) as u64;
    let tss_limit = 103u64;
    let low = (tss_limit & 0xffff)
        | ((tss_base & 0x00ff_ffff) << 16)
        | (0x89u64 << 40)
        | (((tss_limit >> 16) & 0x0f) << 48)
        | (((tss_base >> 24) & 0xff) << 56);
    let high = tss_base >> 32;
    (gdt_phys as *mut u64).add(6).write_volatile(low);
    (gdt_phys as *mut u64).add(7).write_volatile(high);
    let descriptor = IdtPointer {
        limit: 63,
        base: gdt_phys,
    };
    core::arch::asm!("lgdt [{}]", in(reg) &descriptor, options(readonly, nostack, preserves_flags));
    let tss = core::ptr::addr_of_mut!(TSS.bytes).cast::<u8>();
    tss.add(4)
        .cast::<u64>()
        .write_unaligned(0xffff_ffff_8009_0000);
    tss.add(102).cast::<u16>().write_unaligned(104);
    core::arch::asm!("ltr ax", in("ax") 0x30u16, options(nostack, preserves_flags));
}

unsafe fn set_tss_stack(stack_top: u64) {
    if stack_top != 0 {
        core::ptr::addr_of_mut!(TSS.bytes)
            .cast::<u8>()
            .add(4)
            .cast::<u64>()
            .write_unaligned(stack_top);
    }
}

extern "C" {
    static mut __bss_start: u8;
    static mut __bss_end: u8;
}

extern "C" {
    fn isr0();
    fn isr1();
    fn isr2();
    fn isr3();
    fn isr4();
    fn isr5();
    fn isr6();
    fn isr7();
    fn isr8();
    fn isr9();
    fn isr10();
    fn isr11();
    fn isr12();
    fn isr13();
    fn isr14();
    fn isr15();
    fn isr16();
    fn isr17();
    fn isr18();
    fn isr19();
    fn isr20();
    fn isr21();
    fn isr22();
    fn isr23();
    fn isr24();
    fn isr25();
    fn isr26();
    fn isr27();
    fn isr28();
    fn isr29();
    fn isr30();
    fn isr31();
    fn isr32();
    fn isr33();
    fn isr34();
    fn isr35();
    fn isr36();
    fn isr37();
    fn isr38();
    fn isr39();
    fn isr40();
    fn isr41();
    fn isr42();
    fn isr43();
    fn isr44();
    fn isr45();
    fn isr46();
    fn isr47();
    fn isr48();
    fn isr49();
}

core::arch::global_asm!(
    r#"
    .macro NOERR n
    .global isr\n
isr\n:
    push 0
    push \n
    jmp exception_common
    .endm
    .macro HASERR n
    .global isr\n
isr\n:
    push \n
    jmp exception_common
    .endm
    NOERR 0
    NOERR 1
    NOERR 2
    NOERR 3
    NOERR 4
    NOERR 5
    NOERR 6
    NOERR 7
    HASERR 8
    NOERR 9
    HASERR 10
    HASERR 11
    HASERR 12
    HASERR 13
    HASERR 14
    NOERR 15
    NOERR 16
    HASERR 17
    NOERR 18
    NOERR 19
    NOERR 20
    HASERR 21
    NOERR 22
    NOERR 23
    NOERR 24
    NOERR 25
    NOERR 26
    NOERR 27
    NOERR 28
    HASERR 29
    HASERR 30
    NOERR 31

    .macro IRQ n
    .global isr\n
isr\n:
    push \n
    jmp irq_common
    .endm
    IRQ 32
    IRQ 33
    IRQ 34
    IRQ 35
    IRQ 36
    IRQ 37
    IRQ 38
    IRQ 39
    IRQ 40
    IRQ 41
    IRQ 42
    IRQ 43
    IRQ 44
    IRQ 45
    IRQ 46
    IRQ 47
    .global isr48
isr48:
    push 48
    jmp irq_common
    .global isr49
isr49:
    push 49
    jmp irq_common

exception_common:
    cli
    cld
    push rax
    push rbx
    push rcx
    push rdx
    push rsi
    push rdi
    push rbp
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15
    mov rdi, [rsp + 120]
    mov rsi, [rsp + 128]
    xor edx, edx
    mov rcx, [rsp + 136]
    cmp rdi, 14
    jne 1f
    mov rdx, cr2
1:
    and rsp, -16
    call exception_handler
2:  hlt
    jmp 2b

irq_common:
    cld
    push rax
    push rbx
    push rcx
    push rdx
    push rsi
    push rdi
    push rbp
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15
    mov rdi, [rsp + 120]
    mov rsi, rsp
    mov r12, rsp
    and rsp, -16
    call irq_handler
    test rax, rax
    jz 3f
    mov rsp, rax
    jmp 4f
3:  mov rsp, r12
4:
    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rbp
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rbx
    pop rax
    add rsp, 8
    iretq
"#
);

#[no_mangle]
pub extern "C" fn _start(boot_info: *const memory::BootInfo) -> ! {
    unsafe {
        let mut current = core::ptr::addr_of_mut!(__bss_start);
        let end = core::ptr::addr_of_mut!(__bss_end);
        while current < end {
            current.write_volatile(0);
            current = current.add(1);
        }
    }
    unsafe {
        console_clear();
        serial::initialize();
        init_idt();
        remap_pic_for_keyboard();
        let Some(memory_stats) = memory::initialize(boot_info) else {
            console_write(b"Boot memory map invalid. Halting.\n");
            halt_forever();
        };
        init_tss((*boot_info).gdt_phys);
        if !heap::initialize() {
            console_write(b"Could not initialize kernel heap. Halting.\n");
            halt_forever();
        }
        let graphics_ready = graphics::initialize(&*boot_info);
        let (screen_width, screen_height) = graphics::dimensions();
        let mouse_ready = mouse::initialize(screen_width, screen_height);
        enable_pic_irqs(mouse_ready);
        task::initialize();
        init_pit_timer();
        bleuu::initialize();
        let auth_stage = auth::initialize();
        print_logo();
        console_write(b"OpSYS 0.6 | x86_64 | Rust no_std | terminal-first\n");
        if graphics_ready {
            console_write(b"VBE framebuffer available: ");
            console_write_u64(screen_width as u64);
            console_put_byte(b'x');
            console_write_u64(screen_height as u64);
            console_write(b" | optional software desktop\n");
        } else {
            console_write(b"VBE framebuffer unavailable; using VGA text mode.\n");
        }
        if mouse_ready {
            console_write(b"PS/2 mouse IRQ12 ready | desktop mouse support available\n");
        } else {
            console_write(b"PS/2 mouse unavailable; keyboard remains active.\n");
        }
        console_write(b"E820 entries: ");
        console_write_u64(memory_stats.e820_entries as u64);
        console_write(b" | free 4 KiB frames after heap: ");
        console_write_u64(memory::free_frame_count() as u64);
        console_write(b"\nPIT 100 Hz | keyboard IRQ1 | kernel tasks + user process slot\n");
        console_write(b"COM1 serial | SYSTEM and persistent HOME disk partitions\n");
        match auth_stage {
            auth::Stage::SetupUsername => {
                console_write(b"First boot: create a user and root password.\n")
            }
            auth::Stage::LoginUsername => console_write(b"Account found. Log in to continue.\n"),
            _ => console_write(b"Attach a blank IDE data disk to complete account setup.\n"),
        }
        console_write(
            b"Desktop is optional: root can install it with 'bleuu install desktop'.\n\n",
        );
        print_prompt();
    }
    loop {
        unsafe {
            core::arch::asm!("cli", options(nomem, nostack));
        }
        let scancode = keyboard::dequeue();
        let mouse_event = mouse::dequeue_event();
        if scancode.is_some() || mouse_event.is_some() {
            unsafe { core::arch::asm!("sti", options(nomem, nostack)) };
            if let Some(scancode) = scancode {
                handle_scancode(scancode);
            }
            if let Some(event) = mouse_event {
                handle_mouse_event(event);
            }
        } else {
            // STI's interrupt shadow ensures HLT executes before a pending IRQ,
            // so a key arriving between the queue check and sleep is not lost.
            unsafe { core::arch::asm!("sti; hlt", options(nomem, nostack)) };
        }
    }
}

unsafe fn init_idt() {
    let handlers: [usize; 32] = [
        isr0 as *const () as usize,
        isr1 as *const () as usize,
        isr2 as *const () as usize,
        isr3 as *const () as usize,
        isr4 as *const () as usize,
        isr5 as *const () as usize,
        isr6 as *const () as usize,
        isr7 as *const () as usize,
        isr8 as *const () as usize,
        isr9 as *const () as usize,
        isr10 as *const () as usize,
        isr11 as *const () as usize,
        isr12 as *const () as usize,
        isr13 as *const () as usize,
        isr14 as *const () as usize,
        isr15 as *const () as usize,
        isr16 as *const () as usize,
        isr17 as *const () as usize,
        isr18 as *const () as usize,
        isr19 as *const () as usize,
        isr20 as *const () as usize,
        isr21 as *const () as usize,
        isr22 as *const () as usize,
        isr23 as *const () as usize,
        isr24 as *const () as usize,
        isr25 as *const () as usize,
        isr26 as *const () as usize,
        isr27 as *const () as usize,
        isr28 as *const () as usize,
        isr29 as *const () as usize,
        isr30 as *const () as usize,
        isr31 as *const () as usize,
    ];
    let idt_ptr = core::ptr::addr_of_mut!(IDT);
    for (vector, handler) in handlers.iter().enumerate() {
        (*idt_ptr)[vector] = IdtEntry::new(*handler);
    }
    let irq_handlers: [usize; 16] = [
        isr32 as *const () as usize,
        isr33 as *const () as usize,
        isr34 as *const () as usize,
        isr35 as *const () as usize,
        isr36 as *const () as usize,
        isr37 as *const () as usize,
        isr38 as *const () as usize,
        isr39 as *const () as usize,
        isr40 as *const () as usize,
        isr41 as *const () as usize,
        isr42 as *const () as usize,
        isr43 as *const () as usize,
        isr44 as *const () as usize,
        isr45 as *const () as usize,
        isr46 as *const () as usize,
        isr47 as *const () as usize,
    ];
    for (irq, handler) in irq_handlers.iter().enumerate() {
        (*idt_ptr)[32 + irq] = IdtEntry::new(*handler);
    }
    (*idt_ptr)[48] = IdtEntry::new(isr48 as *const () as usize);
    (*idt_ptr)[49] = IdtEntry::new_user(isr49 as *const () as usize);
    let pointer = IdtPointer {
        limit: (core::mem::size_of::<[IdtEntry; 256]>() - 1) as u16,
        base: idt_ptr as u64,
    };
    core::arch::asm!("lidt [{}]", in(reg) &pointer, options(readonly, nostack, preserves_flags));
}

unsafe fn remap_pic_for_keyboard() {
    fn wait() {
        unsafe {
            core::arch::asm!("out 0x80, al", in("al") 0u8, options(nomem, nostack, preserves_flags));
        }
    }
    core::arch::asm!("out 0x20, al", in("al") 0x11u8, options(nomem, nostack));
    wait();
    core::arch::asm!("out 0xa0, al", in("al") 0x11u8, options(nomem, nostack));
    wait();
    core::arch::asm!("out 0x21, al", in("al") 0x20u8, options(nomem, nostack));
    wait();
    core::arch::asm!("out 0xa1, al", in("al") 0x28u8, options(nomem, nostack));
    wait();
    core::arch::asm!("out 0x21, al", in("al") 0x04u8, options(nomem, nostack));
    wait();
    core::arch::asm!("out 0xa1, al", in("al") 0x02u8, options(nomem, nostack));
    wait();
    core::arch::asm!("out 0x21, al", in("al") 0x01u8, options(nomem, nostack));
    wait();
    core::arch::asm!("out 0xa1, al", in("al") 0x01u8, options(nomem, nostack));
    wait();
    // Keep all device interrupts masked until their handlers and queues are ready.
    core::arch::asm!("out 0x21, al", in("al") 0xffu8, options(nomem, nostack));
    core::arch::asm!("out 0xa1, al", in("al") 0xffu8, options(nomem, nostack));
}

unsafe fn enable_pic_irqs(mouse_enabled: bool) {
    // IRQ0 timer, IRQ1 keyboard and IRQ2 cascade are enabled. IRQ12 is slave IRQ4.
    core::arch::asm!("out 0x21, al", in("al") 0xf8u8, options(nomem, nostack));
    let slave_mask: u8 = if mouse_enabled { 0xef } else { 0xff };
    core::arch::asm!("out 0xa1, al", in("al") slave_mask, options(nomem, nostack));
}

unsafe fn init_pit_timer() {
    // Channel 0, square-wave mode, approximately 100 ticks per second.
    let divisor: u16 = 11_932;
    core::arch::asm!("out 0x43, al", in("al") 0x36u8, options(nomem, nostack));
    core::arch::asm!("out 0x40, al", in("al") divisor as u8, options(nomem, nostack));
    core::arch::asm!("out 0x40, al", in("al") (divisor >> 8) as u8, options(nomem, nostack));
}

#[no_mangle]
extern "C" fn irq_handler(vector: u64, interrupted_stack: *mut u64) -> *mut u64 {
    let mut next_stack = interrupted_stack;
    if vector == 32 {
        unsafe {
            TIMER_TICKS = TIMER_TICKS.wrapping_add(1);
        }
        next_stack = unsafe { task::on_timer(interrupted_stack) };
    } else if vector == 48 {
        next_stack = unsafe { task::on_timer(interrupted_stack) };
    } else if vector == 49 {
        next_stack = unsafe { handle_syscall(interrupted_stack) };
    } else if vector == 33 {
        let scancode: u8;
        unsafe {
            core::arch::asm!("in al, 0x60", out("al") scancode, options(nomem, nostack, preserves_flags));
        }
        keyboard::enqueue(scancode);
    } else if vector == 44 {
        let byte: u8;
        unsafe {
            core::arch::asm!("in al, 0x60", out("al") byte, options(nomem, nostack, preserves_flags));
        }
        mouse::enqueue(byte);
    }
    unsafe {
        if (40..48).contains(&vector) {
            core::arch::asm!("out 0xa0, al", in("al") 0x20u8, options(nomem, nostack));
        }
        if (32..48).contains(&vector) {
            core::arch::asm!("out 0x20, al", in("al") 0x20u8, options(nomem, nostack));
        }
        if vector == 32 || vector == 48 || vector == 49 || next_stack != interrupted_stack {
            memory::switch_address_space(task::current_address_space());
            set_tss_stack(task::current_kernel_stack_top());
        }
    }
    next_stack
}

unsafe fn handle_syscall(frame: *mut u64) -> *mut u64 {
    match frame.add(14).read() {
        0 => task::on_timer(frame),
        1 => {
            let address = frame.add(9).read();
            let length = frame.add(10).read();
            let root = task::current_address_space();
            if length <= 512 && memory::user_range_mapped(root, address, length as usize) {
                let bytes = core::slice::from_raw_parts(address as *const u8, length as usize);
                console_write(bytes);
                frame.add(14).write(length);
            } else {
                frame.add(14).write(u64::MAX);
            }
            frame
        }
        2 => task::exit_current(frame),
        _ => {
            frame.add(14).write(u64::MAX);
            frame
        }
    }
}

fn handle_scancode(scancode: u8) {
    let console = core::ptr::addr_of_mut!(CONSOLE);

    // Shift has separate make and break codes; Caps Lock toggles on make.
    match scancode {
        0x2a | 0x36 => {
            unsafe {
                (*console).shift = true;
            }
            return;
        }
        0xaa | 0xb6 => {
            unsafe {
                (*console).shift = false;
            }
            return;
        }
        0x3a => {
            unsafe {
                (*console).caps_lock = !(*console).caps_lock;
            }
            return;
        }
        _ => {}
    }

    // Ignore all other key-release codes.
    if scancode & 0x80 != 0 {
        return;
    }

    match scancode {
        0x1c => unsafe {
            console_put_byte(b'\n');
            run_command();
            print_prompt();
        },
        0x0e => unsafe {
            let len = (*console).input_len;
            if len > 0 {
                (*console).input_len = len - 1;
                console_put_byte(8);
            }
        },
        _ => {
            let (normal, shifted) = match scancode_to_ascii(scancode) {
                Some(keys) => keys,
                None => return,
            };
            let shift = unsafe { (*console).shift };
            let caps = unsafe { (*console).caps_lock };
            let byte = if normal.is_ascii_lowercase() {
                if shift ^ caps {
                    normal.to_ascii_uppercase()
                } else {
                    normal
                }
            } else if shift {
                shifted
            } else {
                normal
            };
            unsafe {
                let len = (*console).input_len;
                if len < (*console).input.len() {
                    (*console).input[len] = byte;
                    (*console).input_len = len + 1;
                    console_put_byte(if auth::hides_input() { b'*' } else { byte });
                }
            }
        }
    }
}

fn handle_mouse_event(event: mouse::MouseState) {
    if graphics::mouse_moved(event.x, event.y, event.buttons) == graphics::UiAction::BleuuStatus {
        unsafe {
            (*core::ptr::addr_of_mut!(CONSOLE)).input_len = 0;
            console_write(b"\n[Desktop] Bleuu service status\n");
            let _ = bleuu::execute(b"bleuu status", |bytes| console_write(bytes));
            print_prompt();
        }
    }
}

fn scancode_to_ascii(scancode: u8) -> Option<(u8, u8)> {
    let pair = match scancode {
        0x02..=0x0b => {
            let i = (scancode - 0x02) as usize;
            (b"1234567890"[i], b"!@#$%^&*()"[i])
        }
        0x0c => (b'-', b'_'),
        0x0d => (b'=', b'+'),
        0x10 => (b'q', b'Q'),
        0x11 => (b'w', b'W'),
        0x12 => (b'e', b'E'),
        0x13 => (b'r', b'R'),
        0x14 => (b't', b'T'),
        0x15 => (b'y', b'Y'),
        0x16 => (b'u', b'U'),
        0x17 => (b'i', b'I'),
        0x18 => (b'o', b'O'),
        0x19 => (b'p', b'P'),
        0x1a => (b'[', b'{'),
        0x1b => (b']', b'}'),
        0x1e => (b'a', b'A'),
        0x1f => (b's', b'S'),
        0x20 => (b'd', b'D'),
        0x21 => (b'f', b'F'),
        0x22 => (b'g', b'G'),
        0x23 => (b'h', b'H'),
        0x24 => (b'j', b'J'),
        0x25 => (b'k', b'K'),
        0x26 => (b'l', b'L'),
        0x27 => (b';', b':'),
        0x28 => (b'\'', b'"'),
        0x29 => (b'`', b'~'),
        0x2b => (b'\\', b'|'),
        0x2c => (b'z', b'Z'),
        0x2d => (b'x', b'X'),
        0x2e => (b'c', b'C'),
        0x2f => (b'v', b'V'),
        0x30 => (b'b', b'B'),
        0x31 => (b'n', b'N'),
        0x32 => (b'm', b'M'),
        0x33 => (b',', b'<'),
        0x34 => (b'.', b'>'),
        0x35 => (b'/', b'?'),
        0x39 => (b' ', b' '),
        _ => return None,
    };
    Some(pair)
}

unsafe fn print_logo() {
    let console = core::ptr::addr_of_mut!(CONSOLE);
    (*console).attribute = 0x0b;
    console_write(b"\xdb\xdb\xdb\xdb\xdb\xdb\xbb \xdb\xdb\xdb\xdb\xdb\xdb\xbb \xdb\xdb\xdb\xdb\xdb\xdb\xdb\xbb\xdb\xdb\xbb   \xdb\xdb\xbb\xdb\xdb\xdb\xdb\xdb\xdb\xdb\xbb\n");
    console_write(b"\xdb\xdb\xc9\xcd\xcd\xcd\xdb\xdb\xbb\xdb\xdb\xc9\xcd\xcd\xdb\xdb\xbb\xdb\xdb\xc9\xcd\xcd\xcd\xcd\xbc\xc8\xdb\xdb\xbb \xdb\xdb\xc9\xbc\xdb\xdb\xc9\xcd\xcd\xcd\xcd\xbc\n");
    console_write(b"\xdb\xdb\xba   \xdb\xdb\xba\xdb\xdb\xdb\xdb\xdb\xdb\xc9\xbc\xdb\xdb\xdb\xdb\xdb\xdb\xdb\xbb \xc8\xdb\xdb\xdb\xdb\xc9\xbc \xdb\xdb\xdb\xdb\xdb\xdb\xdb\xbb\n");
    console_write(b"\xdb\xdb\xba   \xdb\xdb\xba\xdb\xdb\xc9\xcd\xcd\xcd\xbc \xc8\xcd\xcd\xcd\xcd\xdb\xdb\xba  \xc8\xdb\xdb\xc9\xbc  \xc8\xcd\xcd\xcd\xcd\xdb\xdb\xba\n");
    console_write(b"\xc8\xdb\xdb\xdb\xdb\xdb\xdb\xc9\xbc\xdb\xdb\xba     \xdb\xdb\xdb\xdb\xdb\xdb\xdb\xba   \xdb\xdb\xba   \xdb\xdb\xdb\xdb\xdb\xdb\xdb\xba\n");
    console_write(b" \xc8\xcd\xcd\xcd\xcd\xcd\xbc \xc8\xcd\xbc     \xc8\xcd\xcd\xcd\xcd\xcd\xcd\xbc   \xc8\xcd\xbc   \xc8\xcd\xcd\xcd\xcd\xcd\xcd\xbc\n");
    (*console).attribute = 0x0f;
}

unsafe fn print_opfetch() {
    let console = core::ptr::addr_of_mut!(CONSOLE);
    let old_attribute = (*console).attribute;
    (*console).attribute = 0x0b;
    console_write(
        br#"       ;   :   ;
   .   \_,!,_/   ,
    `.,'     `.,'
     /         \
~ -- :         : -- ~
     \         /
    ,'`._   _.'`.
   '   / `!` \   `
      ;   :   ;
"#,
    );
    (*console).attribute = old_attribute;

    console_write(b"OpSYS 0.5.0 | x86_64\n");
    console_write(b"Kernel: Rust no_std\n");
    console_write(b"Boot: BIOS | 4-level paging | ring 3\n");
    console_write(b"Heap: ");
    console_write_u64((heap::size() / (1024 * 1024)) as u64);
    console_write(b" MiB\nFree 4 KiB frames: ");
    console_write_u64(memory::free_frame_count() as u64);
    console_write(b"\nKernel tasks: 3 | process slot: 1\nUptime: ");
    console_write_u64(TIMER_TICKS / 100);
    console_write(b" seconds\n");
}

unsafe fn console_clear() {
    let vga = 0xb8000 as *mut u16;
    let console = core::ptr::addr_of_mut!(CONSOLE);
    let attribute = (*console).attribute as u16;
    for row in 0..25 {
        for column in 0..80 {
            let cell = (attribute << 8) | b' ' as u16;
            (*console).cells[row][column] = cell;
            vga.add(row * 80 + column).write_volatile(cell);
        }
    }
    (*console).row = 0;
    (*console).column = 0;
    console_update_cursor();
    graphics::refresh(core::ptr::addr_of!((*console).cells));
}

unsafe fn console_put_byte(byte: u8) {
    if byte < 0x80 {
        serial::write(core::slice::from_ref(&byte));
    }
    let console = core::ptr::addr_of_mut!(CONSOLE);
    match byte {
        b'\n' => {
            (*console).column = 0;
            (*console).row += 1;
        }
        b'\r' => (*console).column = 0,
        8 => {
            if (*console).column > 0 {
                (*console).column -= 1;
            } else if (*console).row > 0 {
                (*console).row -= 1;
                (*console).column = 79;
            }
            let cell = (*console).row * 80 + (*console).column;
            (*console).cells[(*console).row][(*console).column] =
                ((*console).attribute as u16) << 8 | b' ' as u16;
            (0xb8000 as *mut u16)
                .add(cell)
                .write_volatile(((*console).attribute as u16) << 8 | b' ' as u16);
            graphics::render_console_cell(
                (*console).row,
                (*console).column,
                b' ',
                (*console).attribute,
            );
        }
        b'\t' => {
            for _ in 0..4 {
                console_put_byte(b' ');
            }
            return;
        }
        0x20..=0xff => {
            let cell = (*console).row * 80 + (*console).column;
            (0xb8000 as *mut u16)
                .add(cell)
                .write_volatile((((*console).attribute as u16) << 8) | byte as u16);
            (*console).cells[(*console).row][(*console).column] =
                ((*console).attribute as u16) << 8 | byte as u16;
            graphics::render_console_cell(
                (*console).row,
                (*console).column,
                byte,
                (*console).attribute,
            );
            (*console).column += 1;
            if (*console).column >= 80 {
                (*console).column = 0;
                (*console).row += 1;
            }
        }
        _ => return,
    }

    if (*console).row >= 25 {
        let vga = 0xb8000 as *mut u16;
        for row in 1..25 {
            for column in 0..80 {
                let source = row * 80 + column;
                let target = (row - 1) * 80 + column;
                vga.add(target)
                    .write_volatile(vga.add(source).read_volatile());
                (*console).cells[row - 1][column] = (*console).cells[row][column];
            }
        }
        for column in 0..80 {
            let blank = (((*console).attribute as u16) << 8) | b' ' as u16;
            vga.add(24 * 80 + column).write_volatile(blank);
            (*console).cells[24][column] = blank;
        }
        (*console).row = 24;
        graphics::render_console_buffer(core::ptr::addr_of!((*console).cells));
    }
    console_update_cursor();
}

unsafe fn console_update_cursor() {
    let console = core::ptr::addr_of!(CONSOLE);
    let position = ((*console).row * 80 + (*console).column) as u16;
    core::arch::asm!("out dx, al", in("dx") 0x3d4u16, in("al") 0x0fu8, options(nomem, nostack));
    core::arch::asm!("out dx, al", in("dx") 0x3d5u16, in("al") position as u8, options(nomem, nostack));
    core::arch::asm!("out dx, al", in("dx") 0x3d4u16, in("al") 0x0eu8, options(nomem, nostack));
    core::arch::asm!("out dx, al", in("dx") 0x3d5u16, in("al") (position >> 8) as u8, options(nomem, nostack));
}

unsafe fn console_write(bytes: &[u8]) {
    let flags: u64;
    core::arch::asm!("pushfq", "pop {}", out(reg) flags, options());
    core::arch::asm!("cli", options(nomem, nostack));
    for byte in bytes {
        console_put_byte(*byte);
    }
    if flags & (1 << 9) != 0 {
        core::arch::asm!("sti", options(nomem, nostack));
    }
}

unsafe fn print_prompt() {
    console_write(auth::prompt());
}

unsafe fn run_command() {
    let console = core::ptr::addr_of_mut!(CONSOLE);
    let len = (*console).input_len;
    let mut command = [0u8; 80];
    for index in 0..len {
        command[index] = (*console).input[index];
        (*console).input[index] = 0;
    }
    (*console).input_len = 0;

    if len == 0 {
        return;
    }
    if !auth::is_ready() {
        let result = auth::submit(&command[..len]);
        match result {
            auth::Result::Continue => console_write(b"\n"),
            auth::Result::Accepted => {
                console_write(if auth::is_root() { b"\nRoot session active.\n" } else { b"\nLogin successful.\n" });
                console_write(b"Type help for commands.\n");
            }
            auth::Result::SetupComplete => {
                console_write(b"\nAccount created. You are logged in as root.\n");
                console_write(b"Use 'logout' to test normal login; 'bleuu install desktop' enables the optional UI.\n");
            }
            auth::Result::Invalid => console_write(b"\nInvalid value. Use a non-root username with letters, digits, _ or -; passwords need 8-63 characters.\n"),
            auth::Result::Rejected => match auth::stage() {
                auth::Stage::LoginUsername => console_write(b"\nAccount or password not accepted. Usernames ignore letter case; enter your account name or root.\n"),
                auth::Stage::DiskUnavailable => console_write(b"\nLogin failed: the IDE data disk or saved account could not be read.\n"),
                auth::Stage::Ready => console_write(b"\nRoot password not accepted; returning to the shell.\n"),
                _ => console_write(b"\nLogin or disk setup failed. Check the IDE data disk and try again.\n"),
            },
        }
        // Remove the transient command copy containing a password from the stack.
        command.fill(0);
        return;
    }
    let command = &command[..len];
    if command == b"help" {
        console_write(b"help, whoami, su, logout, opfetch, about, version, uptime, ticks, mem, paging, tasks, ps\n");
        console_write(b"keyboard, mouse, ui, desktop, terminal, alloc\n");
        console_write(b"ls, cat <file>, write NOTE <text>, disk, partitions\n");
        console_write(b"bleuu status|packages|install|remove|start|stop|restart ...\n");
        console_write(b"color <name>, clear, echo <text>, reboot, halt\n");
    } else if command == b"whoami" {
        if auth::is_root() {
            console_write(b"root\n");
        } else {
            console_write(auth::username());
            console_put_byte(b'\n');
        }
    } else if command == b"su" {
        if auth::begin_root_login() {
            console_write(b"\n");
        } else {
            console_write(b"\nAlready root or authentication is unavailable.\n");
        }
    } else if command == b"logout" {
        auth::begin_login();
        console_write(b"\nLogged out.\n");
    } else if command == b"opfetch" {
        print_opfetch();
    } else if command == b"about" {
        console_write(b"OpSYS is a tiny BIOS-booted x86_64 kernel written in Rust.\n");
        console_write(b"It uses paging, ring 3, syscalls, IDT exceptions and preemptive tasks.\n");
    } else if command == b"version" {
        console_write(b"OpSYS 0.6.0 | x86_64 | Rust no_std\n");
    } else if command == b"uptime" {
        let ticks = TIMER_TICKS;
        let seconds = ticks / 100;
        let hundredths = (ticks % 100) as u8;
        console_write(b"Uptime: ");
        console_write_u64(seconds);
        console_put_byte(b'.');
        console_put_byte(b'0' + hundredths / 10);
        console_put_byte(b'0' + hundredths % 10);
        console_write(b" seconds\n");
    } else if command == b"ticks" {
        console_write(b"PIT ticks: ");
        console_write_u64(TIMER_TICKS);
        console_put_byte(b'\n');
    } else if command == b"keyboard" {
        console_write(b"Dropped keyboard scancodes: ");
        console_write_u64(keyboard::dropped_count());
        console_put_byte(b'\n');
    } else if command == b"mouse" {
        if mouse::enabled() {
            let state = mouse::state();
            console_write(b"PS/2 mouse at ");
            console_write_u64(state.x as u64);
            console_put_byte(b',');
            console_write_u64(state.y as u64);
            console_write(b" | buttons: ");
            console_write_u64(state.buttons as u64);
            console_write(b" | dropped bytes: ");
            console_write_u64(mouse::dropped_count());
            console_write(b" | IRQ bytes: ");
            console_write_u64(mouse::irq_byte_count());
            console_write(b" | packets: ");
            console_write_u64(mouse::packet_count());
            console_put_byte(b'\n');
        } else {
            console_write(b"No PS/2 mouse detected.\n");
        }
    } else if command == b"ui" {
        if graphics::active() {
            graphics::refresh(core::ptr::addr_of!((*console).cells));
            console_write(
                b"Redrew the VBE desktop. Click BLEUU on the taskbar for service status.\n",
            );
        } else {
            console_write(b"Desktop is inactive. Run 'bleuu start desktop' after installing it.\n");
        }
    } else if command == b"desktop" || command == b"bleuu start desktop" {
        if !auth::is_root() {
            console_write(b"Root access required. Run 'su' first.\n");
        } else if !bleuu::desktop_installed() {
            console_write(b"Desktop is not installed. Run 'bleuu install desktop'.\n");
        } else if graphics::activate(core::ptr::addr_of!((*console).cells)) {
            console_write(b"Started the built-in VBE desktop. Type 'terminal' to return.\n");
        } else {
            console_write(b"No usable VBE framebuffer is available.\n");
        }
    } else if command == b"terminal" || command == b"bleuu stop desktop" {
        graphics::deactivate(core::ptr::addr_of!((*console).cells));
        console_write(b"Returned to terminal mode.\n");
    } else if command == b"mem" {
        console_write(b"BIOS E820 entries: ");
        console_write_u64(memory::e820_entry_count() as u64);
        console_write(b"\nFree 4 KiB frames: ");
        console_write_u64(memory::free_frame_count() as u64);
        console_write(b"\nHigher-half direct map: 1 GiB | heap: ");
        console_write_u64((heap::size() / (1024 * 1024)) as u64);
        console_write(b" MiB\n");
    } else if command == b"paging" {
        if memory::paging_self_check() {
            console_write(b"4 KiB map/read/unmap/frame-release: passed\n");
        } else {
            console_write(b"Paging self-check failed. See exception diagnostics.\n");
        }
    } else if command == b"clear" {
        console_clear();
    } else if command == b"color" {
        console_write(b"Colors: white, gray, red, green, yellow, blue, magenta, cyan, default\n");
    } else if command.starts_with(b"color ") {
        let name = &command[6..];
        let attribute = match name {
            b"default" | b"white" => Some(0x0f),
            b"gray" => Some(0x07),
            b"red" => Some(0x0c),
            b"green" => Some(0x0a),
            b"yellow" => Some(0x0e),
            b"blue" => Some(0x09),
            b"magenta" => Some(0x0d),
            b"cyan" => Some(0x0b),
            _ => None,
        };
        if let Some(attribute) = attribute {
            (*console).attribute = attribute;
            console_write(b"Text color changed.\n");
        } else {
            console_write(b"Unknown color. Try: color\n");
        }
    } else if command == b"halt" {
        console_write(b"CPU halted.\n");
        halt_forever();
    } else if command == b"reboot" {
        console_write(b"Rebooting...\n");
        reboot();
    } else if command.starts_with(b"echo ") {
        console_write(&command[5..]);
        console_put_byte(b'\n');
    } else if command == b"tasks" {
        console_write(b"Current task: ");
        console_write_u64(task::current_id() as u64);
        console_write(b" | switches: ");
        console_write_u64(task::context_switches());
        console_write(b"\nWorker A loops: ");
        console_write_u64(task::worker_count(0));
        console_write(b" | worker B loops: ");
        console_write_u64(task::worker_count(1));
        console_put_byte(b'\n');
        for id in 0..task::count() {
            if let Some((task_id, state, switches)) = task::snapshot(id) {
                console_write(b"Task ");
                console_write_u64(task_id as u64);
                console_write(match state {
                    task::RUNNING => b" running, switches: ",
                    task::READY => b" ready, switches: ",
                    task::EXITED => b" exited, switches: ",
                    _ => b" empty, switches: ",
                });
                console_write_u64(switches);
                console_put_byte(b'\n');
            }
        }
    } else if command == b"ps" {
        for id in 0..task::count() {
            if let Some((task_id, state, _)) = task::snapshot(id) {
                if state != task::EMPTY {
                    console_write(b"PID ");
                    console_write_u64(task_id as u64);
                    console_write(match state {
                        task::RUNNING => b" running\n",
                        task::READY => b" ready\n",
                        _ => b" exited\n",
                    });
                }
            }
        }
    } else if command == b"ls" {
        fs::list(|line| console_write(line));
    } else if command == b"cat README.TXT" || command == b"cat readme.txt" {
        if let Some(bytes) = fs::read(b"README.TXT") {
            console_write(bytes);
        }
    } else if command == b"cat NOTE" || command == b"cat note" {
        let mut sector = [0u8; 512];
        if let Some(bytes) = fs::read_note(&mut sector) {
            console_write(bytes);
            console_put_byte(b'\n');
        } else {
            console_write(b"NOTE is empty (or IDE disk unavailable).\n");
        }
    } else if command.starts_with(b"write NOTE ") || command.starts_with(b"write note ") {
        if fs::write_note(&command[11..]) {
            console_write(b"Saved NOTE to IDE disk.\n");
        } else {
            console_write(b"Could not save NOTE. Check the disk and note length.\n");
        }
    } else if command == b"disk" {
        if let Some(info) = ata::identify() {
            console_write(b"IDE disk: ");
            console_write(&info.model);
            console_write(b" | LBA28 sectors: ");
            console_write_u64(info.sectors as u64);
            console_put_byte(b'\n');
            console_write(b"Persistent HOME partition begins at LBA 4096.\n");
        } else {
            console_write(b"No ATA disk detected.\n");
        }
    } else if command == b"partitions" {
        system::partitions(&mut |bytes| console_write(bytes), |value| {
            console_write_u64(value as u64)
        });
    } else if command == b"bleuu install desktop" {
        if !auth::is_root() {
            console_write(b"Root access required. Run 'su' first.\n");
        } else if bleuu::install_desktop() {
            console_write(
                b"Bleuu: desktop profile installed. Run 'bleuu start desktop' to launch it.\n",
            );
        } else {
            console_write(b"Bleuu: could not save the desktop profile to SYSTEM.\n");
        }
    } else if command == b"bleuu remove desktop" {
        if !auth::is_root() {
            console_write(b"Root access required. Run 'su' first.\n");
        } else if bleuu::remove_desktop() {
            graphics::deactivate(core::ptr::addr_of!((*console).cells));
            console_write(b"Bleuu: desktop profile removed. HOME files were left untouched.\n");
        } else {
            console_write(b"Bleuu: desktop profile is not installed.\n");
        }
    } else if (command.starts_with(b"bleuu install ") || command.starts_with(b"bleuu remove "))
        && !auth::is_root()
    {
        console_write(b"Root access required. Run 'su' first.\n");
    } else if bleuu::execute(command, |bytes| console_write(bytes)) {
    } else if command == b"alloc" {
        let mut values = alloc::vec::Vec::new();
        values.push(10u64);
        values.push(20u64);
        let message = alloc::string::String::from("heap allocations work");
        let boxed_value = alloc::boxed::Box::new(42u64);
        console_write(b"Vec, String, and Box allocated: ");
        console_write_u64(values[0] + values[1] + *boxed_value);
        console_write(b" (");
        console_write(message.as_bytes());
        console_write(b")\n");
    } else {
        console_write(b"Unknown command. Type help.\n");
    }
}

fn halt_forever() -> ! {
    loop {
        unsafe {
            core::arch::asm!("cli; hlt", options(nomem, nostack));
        }
    }
}

unsafe fn console_write_u64(mut value: u64) {
    let mut digits = [0u8; 20];
    let mut length = 0;
    loop {
        digits[length] = b'0' + (value % 10) as u8;
        length += 1;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    while length > 0 {
        length -= 1;
        console_put_byte(digits[length]);
    }
}

unsafe fn reboot() -> ! {
    // Ask the legacy keyboard controller to pulse the CPU reset line.
    for _ in 0..100_000 {
        let status: u8;
        core::arch::asm!("in al, dx", in("dx") 0x64u16, out("al") status, options(nomem, nostack, preserves_flags));
        if status & 0x02 == 0 {
            break;
        }
    }
    core::arch::asm!("out dx, al", in("dx") 0x64u16, in("al") 0xfeu8, options(nomem, nostack));
    loop {
        core::arch::asm!("cli; hlt", options(nomem, nostack));
    }
}

#[no_mangle]
extern "C" fn exception_handler(
    vector: u64,
    error_code: u64,
    fault_address: u64,
    instruction_pointer: u64,
) -> ! {
    unsafe {
        console_clear();
        if vector == 14 {
            console_write(b"PAGE FAULT\nRIP: ");
            console_write_hex(instruction_pointer);
            console_write(b"\nCR2: ");
            console_write_hex(fault_address);
            console_write(b"\nError code: ");
            console_write_hex(error_code);
            console_write(b"\n");
            console_write(b"Fault: ");
            if error_code & 1 != 0 {
                console_write(b"protection violation");
            } else {
                console_write(b"non-present page");
            }
            if error_code & 2 != 0 {
                console_write(b", write");
            } else {
                console_write(b", read");
            }
            if error_code & 4 != 0 {
                console_write(b", user mode");
            } else {
                console_write(b", supervisor mode");
            }
            if error_code & 8 != 0 {
                console_write(b", reserved-bit violation");
            }
            if error_code & 16 != 0 {
                console_write(b", instruction fetch");
            }
            if error_code & 32 != 0 {
                console_write(b", protection-key violation");
            }
            if error_code & 64 != 0 {
                console_write(b", shadow-stack access");
            }
            if error_code & (1 << 15) != 0 {
                console_write(b", SGX violation");
            }
            console_put_byte(b'\n');
            console_write(b"System halted.\n");
            halt_forever();
        }
        console_write(b"CPU EXCEPTION: ");
        console_write(exception_name(vector));
        console_write(b" (#");
        console_write_u64(vector);
        console_write(b")\nRIP: ");
        console_write_hex(instruction_pointer);
        console_write(b"\nError code: ");
        console_write_hex(error_code);
        console_write(b"\nSystem halted.\n");
    }
    loop {
        unsafe {
            core::arch::asm!("cli; hlt", options(nomem, nostack));
        }
    }
}

fn exception_name(vector: u64) -> &'static [u8] {
    match vector {
        0 => b"divide error",
        1 => b"debug",
        2 => b"non-maskable interrupt",
        3 => b"breakpoint",
        4 => b"overflow",
        5 => b"bound range exceeded",
        6 => b"invalid opcode",
        7 => b"device not available",
        8 => b"double fault",
        9 => b"coprocessor segment overrun",
        10 => b"invalid TSS",
        11 => b"segment not present",
        12 => b"stack-segment fault",
        13 => b"general protection fault",
        14 => b"page fault",
        16 => b"x87 floating-point exception",
        17 => b"alignment check",
        18 => b"machine check",
        19 => b"SIMD floating-point exception",
        20 => b"virtualization exception",
        21 => b"control protection exception",
        29 => b"VMM communication exception",
        30 => b"security exception",
        _ => b"reserved exception",
    }
}

unsafe fn console_write_hex(mut value: u64) {
    console_write(b"0x");
    let mut digits = [b'0'; 16];
    for index in (0..16).rev() {
        let digit = (value & 0xf) as u8;
        digits[index] = if digit < 10 {
            b'0' + digit
        } else {
            b'a' + digit - 10
        };
        value >>= 4;
    }
    console_write(&digits);
}

struct ConsoleFormatter;

impl FmtWrite for ConsoleFormatter {
    fn write_str(&mut self, text: &str) -> core::fmt::Result {
        unsafe { console_write(text.as_bytes()) };
        Ok(())
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    unsafe { core::arch::asm!("cli", options(nomem, nostack)) };
    unsafe {
        console_clear();
        console_write(b"KERNEL PANIC\n");
    }
    let _ = write!(ConsoleFormatter, "{}\n", info);
    unsafe { console_write(b"System halted.\n") };
    loop {
        unsafe {
            core::arch::asm!("cli; hlt", options(nomem, nostack));
        }
    }
}
