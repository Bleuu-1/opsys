use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

const CAPACITY: usize = 256;

static mut SCANCODES: [u8; CAPACITY] = [0; CAPACITY];
static HEAD: AtomicUsize = AtomicUsize::new(0);
static TAIL: AtomicUsize = AtomicUsize::new(0);
static DROPPED: AtomicU64 = AtomicU64::new(0);

/// Called by the keyboard IRQ handler. It only queues the byte; command parsing
/// and screen output happen later in the kernel's main loop.
pub fn enqueue(scancode: u8) {
    let head = HEAD.load(Ordering::Relaxed);
    let next = (head + 1) % CAPACITY;
    if next == TAIL.load(Ordering::Acquire) {
        DROPPED.fetch_add(1, Ordering::Relaxed);
        return;
    }
    unsafe {
        core::ptr::addr_of_mut!(SCANCODES)
            .cast::<u8>()
            .add(head)
            .write_volatile(scancode);
    }
    HEAD.store(next, Ordering::Release);
}

pub fn dequeue() -> Option<u8> {
    let tail = TAIL.load(Ordering::Relaxed);
    if tail == HEAD.load(Ordering::Acquire) {
        return None;
    }
    let scancode = unsafe {
        core::ptr::addr_of!(SCANCODES)
            .cast::<u8>()
            .add(tail)
            .read_volatile()
    };
    TAIL.store((tail + 1) % CAPACITY, Ordering::Release);
    Some(scancode)
}

pub fn dropped_count() -> u64 {
    DROPPED.load(Ordering::Relaxed)
}
