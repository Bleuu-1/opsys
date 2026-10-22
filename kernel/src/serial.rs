use core::sync::atomic::{AtomicBool, Ordering};

const COM1: u16 = 0x3f8;
static READY: AtomicBool = AtomicBool::new(false);

pub unsafe fn initialize() {
    write_reg(1, 0x00); // Disable UART interrupts.
    write_reg(3, 0x80); // Divisor latch access.
    write_reg(0, 0x01); // 115200 baud.
    write_reg(1, 0x00);
    write_reg(3, 0x03); // 8 data bits, no parity, one stop bit.
    write_reg(2, 0xc7); // FIFO on, clear queues, 14-byte threshold.
    write_reg(4, 0x0b); // IRQs enabled, RTS/DSR set.
    READY.store(true, Ordering::Release);
}

pub fn enabled() -> bool {
    READY.load(Ordering::Acquire)
}

pub fn disable() {
    READY.store(false, Ordering::Release);
}

pub fn write(bytes: &[u8]) {
    if !READY.load(Ordering::Acquire) {
        return;
    }
    for byte in bytes {
        if *byte == b'\n' {
            write_byte(b'\r');
        }
        write_byte(*byte);
    }
}

fn write_byte(byte: u8) {
    for _ in 0..100_000 {
        let status: u8;
        unsafe {
            core::arch::asm!("in al, dx", in("dx") COM1 + 5, out("al") status,
                options(nomem, nostack, preserves_flags));
        }
        if status & 0x20 != 0 {
            unsafe {
                core::arch::asm!("out dx, al", in("dx") COM1, in("al") byte,
                    options(nomem, nostack, preserves_flags));
            }
            return;
        }
        core::hint::spin_loop();
    }
}

unsafe fn write_reg(offset: u16, value: u8) {
    core::arch::asm!("out dx, al", in("dx") COM1 + offset, in("al") value,
        options(nomem, nostack, preserves_flags));
}
