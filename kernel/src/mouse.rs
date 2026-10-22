use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

const DATA_PORT: u16 = 0x60;
const STATUS_PORT: u16 = 0x64;
const CAPACITY: usize = 128;

static mut QUEUE: [u8; CAPACITY] = [0; CAPACITY];
static HEAD: AtomicUsize = AtomicUsize::new(0);
static TAIL: AtomicUsize = AtomicUsize::new(0);
static DROPPED: AtomicU64 = AtomicU64::new(0);
static IRQ_BYTES: AtomicU64 = AtomicU64::new(0);
static PACKETS: AtomicU64 = AtomicU64::new(0);
static mut PACKET: [u8; 3] = [0; 3];
static mut PACKET_INDEX: usize = 0;
static mut STATE: MouseState = MouseState {
    x: 0,
    y: 0,
    buttons: 0,
};
static mut SCREEN_WIDTH: i32 = 640;
static mut SCREEN_HEIGHT: i32 = 400;
static mut ENABLED: bool = false;

#[derive(Clone, Copy)]
pub struct MouseState {
    pub x: i32,
    pub y: i32,
    pub buttons: u8,
}

/// Enable the PS/2 auxiliary port and ask the device to report three-byte packets.
/// Call this with interrupts disabled and the PIC line still masked.
pub unsafe fn initialize(width: u32, height: u32) -> bool {
    SCREEN_WIDTH = width.max(1) as i32;
    SCREEN_HEIGHT = height.max(1) as i32;
    STATE.x = SCREEN_WIDTH / 2;
    STATE.y = SCREEN_HEIGHT / 2;
    STATE.buttons = 0;

    // Enable the auxiliary clock, then enable IRQ12 in the 8042 command byte.
    if !controller_command(0xa8) || !controller_command(0x20) {
        return false;
    }
    if !wait_output_full() {
        return false;
    }
    let mut command_byte = in8(DATA_PORT);
    command_byte |= 0x02;
    command_byte &= !0x20;
    if !controller_command(0x60) || !controller_data(command_byte) {
        return false;
    }

    if !send_mouse_command(0xf6) || !send_mouse_command(0xf4) {
        return false;
    }
    ENABLED = true;
    true
}

pub fn enabled() -> bool {
    unsafe { ENABLED }
}

/// Called by IRQ12. The handler only queues the byte; packet parsing is deferred.
pub fn enqueue(byte: u8) {
    IRQ_BYTES.fetch_add(1, Ordering::Relaxed);
    let head = HEAD.load(Ordering::Relaxed);
    let next = (head + 1) % CAPACITY;
    if next == TAIL.load(Ordering::Acquire) {
        DROPPED.fetch_add(1, Ordering::Relaxed);
        return;
    }
    unsafe {
        core::ptr::addr_of_mut!(QUEUE)
            .cast::<u8>()
            .add(head)
            .write_volatile(byte);
    }
    HEAD.store(next, Ordering::Release);
}

/// Dequeue bytes until one full, synchronized mouse packet is available.
pub fn dequeue_event() -> Option<MouseState> {
    loop {
        let tail = TAIL.load(Ordering::Relaxed);
        if tail == HEAD.load(Ordering::Acquire) {
            return None;
        }
        let byte = unsafe {
            core::ptr::addr_of!(QUEUE)
                .cast::<u8>()
                .add(tail)
                .read_volatile()
        };
        TAIL.store((tail + 1) % CAPACITY, Ordering::Release);
        if let Some(state) = unsafe { process_byte(byte) } {
            return Some(state);
        }
    }
}

pub fn state() -> MouseState {
    unsafe { STATE }
}

pub fn dropped_count() -> u64 {
    DROPPED.load(Ordering::Relaxed)
}

pub fn irq_byte_count() -> u64 {
    IRQ_BYTES.load(Ordering::Relaxed)
}

pub fn packet_count() -> u64 {
    PACKETS.load(Ordering::Relaxed)
}

unsafe fn process_byte(byte: u8) -> Option<MouseState> {
    if PACKET_INDEX == 0 && byte & 0x08 == 0 {
        return None;
    }
    PACKET[PACKET_INDEX] = byte;
    PACKET_INDEX += 1;
    if PACKET_INDEX < 3 {
        return None;
    }
    PACKET_INDEX = 0;

    let flags = PACKET[0];
    if flags & 0xc0 != 0 {
        return None;
    }
    PACKETS.fetch_add(1, Ordering::Relaxed);
    let dx = PACKET[1] as i8 as i32;
    let dy = -(PACKET[2] as i8 as i32);
    STATE.x = (STATE.x + dx * 2).clamp(0, SCREEN_WIDTH - 1);
    STATE.y = (STATE.y + dy * 2).clamp(0, SCREEN_HEIGHT - 1);
    STATE.buttons = flags & 0x07;
    Some(STATE)
}

unsafe fn send_mouse_command(value: u8) -> bool {
    if !controller_command(0xd4) || !controller_data(value) {
        return false;
    }
    for _ in 0..100_000 {
        let status = in8(STATUS_PORT);
        if status & 1 != 0 {
            let response = in8(DATA_PORT);
            if status & 0x20 != 0 {
                return response == 0xfa;
            }
        }
        core::hint::spin_loop();
    }
    false
}

unsafe fn controller_command(value: u8) -> bool {
    if !wait_input_empty() {
        return false;
    }
    out8(STATUS_PORT, value);
    true
}

unsafe fn controller_data(value: u8) -> bool {
    if !wait_input_empty() {
        return false;
    }
    out8(DATA_PORT, value);
    true
}

unsafe fn wait_input_empty() -> bool {
    for _ in 0..100_000 {
        if in8(STATUS_PORT) & 0x02 == 0 {
            return true;
        }
        core::hint::spin_loop();
    }
    false
}

unsafe fn wait_output_full() -> bool {
    for _ in 0..100_000 {
        if in8(STATUS_PORT) & 0x01 != 0 {
            return true;
        }
        core::hint::spin_loop();
    }
    false
}

unsafe fn in8(port: u16) -> u8 {
    let value: u8;
    core::arch::asm!("in al, dx", in("dx") port, out("al") value, options(nomem, nostack, preserves_flags));
    value
}

unsafe fn out8(port: u16, value: u8) {
    core::arch::asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags));
}
