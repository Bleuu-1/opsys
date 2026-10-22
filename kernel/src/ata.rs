const DATA: u16 = 0x1f0;
const ERROR: u16 = 0x1f1;
const SECTOR_COUNT: u16 = 0x1f2;
const LBA_LOW: u16 = 0x1f3;
const LBA_MID: u16 = 0x1f4;
const LBA_HIGH: u16 = 0x1f5;
const DRIVE: u16 = 0x1f6;
const STATUS: u16 = 0x1f7;
const COMMAND: u16 = 0x1f7;
const ALT_STATUS: u16 = 0x3f6;

#[derive(Clone, Copy)]
pub struct DiskInfo {
    pub model: [u8; 40],
    pub sectors: u32,
}

pub fn identify() -> Option<DiskInfo> {
    unsafe {
        out8(DRIVE, 0xa0);
        delay_400ns();
        out8(SECTOR_COUNT, 0);
        out8(LBA_LOW, 0);
        out8(LBA_MID, 0);
        out8(LBA_HIGH, 0);
        out8(COMMAND, 0xec);
        if in8(STATUS) == 0 {
            return None;
        }
        if !wait_not_busy() {
            return None;
        }
        let mid = in8(LBA_MID);
        let high = in8(LBA_HIGH);
        if mid != 0 || high != 0 || !wait_data() {
            return None;
        }
        let mut words = [0u16; 256];
        for word in &mut words {
            *word = in16(DATA);
        }
        if words[0] == 0 {
            return None;
        }
        let mut model = [b' '; 40];
        for index in 0..20 {
            model[index * 2] = (words[27 + index] >> 8) as u8;
            model[index * 2 + 1] = words[27 + index] as u8;
        }
        let sectors = words[60] as u32 | ((words[61] as u32) << 16);
        Some(DiskInfo { model, sectors })
    }
}

pub fn read_sector(lba: u32, buffer: &mut [u8; 512]) -> bool {
    transfer_sector(lba, buffer, false)
}

pub fn write_sector(lba: u32, buffer: &[u8; 512]) -> bool {
    let mut copy = *buffer;
    transfer_sector(lba, &mut copy, true)
}

fn transfer_sector(lba: u32, buffer: &mut [u8; 512], write: bool) -> bool {
    if lba >= 0x1000_0000 {
        return false;
    }
    unsafe {
        out8(DRIVE, 0xe0 | ((lba >> 24) as u8 & 0x0f));
        delay_400ns();
        out8(SECTOR_COUNT, 1);
        out8(LBA_LOW, lba as u8);
        out8(LBA_MID, (lba >> 8) as u8);
        out8(LBA_HIGH, (lba >> 16) as u8);
        out8(COMMAND, if write { 0x30 } else { 0x20 });
        if !wait_not_busy() || (in8(STATUS) & 0x01 != 0) {
            let _device_error = in8(ERROR);
            return false;
        }
        if !wait_data() {
            return false;
        }
        if write {
            for pair in buffer.chunks_exact(2) {
                out16(DATA, u16::from_le_bytes([pair[0], pair[1]]));
            }
            out8(COMMAND, 0xe7); // Flush cache before reporting success.
            return wait_not_busy() && in8(STATUS) & 0x01 == 0;
        }
        for pair in buffer.chunks_exact_mut(2) {
            let word = in16(DATA).to_le_bytes();
            pair[0] = word[0];
            pair[1] = word[1];
        }
        true
    }
}

unsafe fn wait_not_busy() -> bool {
    for _ in 0..1_000_000 {
        let status = in8(STATUS);
        if status & 0x80 == 0 {
            return status != 0xff;
        }
        core::hint::spin_loop();
    }
    false
}

unsafe fn wait_data() -> bool {
    for _ in 0..1_000_000 {
        let status = in8(STATUS);
        if status & 0x01 != 0 || status == 0xff {
            return false;
        }
        if status & 0x80 == 0 && status & 0x08 != 0 {
            return true;
        }
        core::hint::spin_loop();
    }
    false
}

unsafe fn delay_400ns() {
    for _ in 0..4 {
        let _ = in8(ALT_STATUS);
    }
}

unsafe fn in8(port: u16) -> u8 {
    let value: u8;
    core::arch::asm!("in al, dx", in("dx") port, out("al") value,
        options(nomem, nostack, preserves_flags));
    value
}

unsafe fn out8(port: u16, value: u8) {
    core::arch::asm!("out dx, al", in("dx") port, in("al") value,
        options(nomem, nostack, preserves_flags));
}

unsafe fn in16(port: u16) -> u16 {
    let value: u16;
    core::arch::asm!("in ax, dx", in("dx") port, out("ax") value,
        options(nomem, nostack, preserves_flags));
    value
}

unsafe fn out16(port: u16, value: u16) {
    core::arch::asm!("out dx, ax", in("dx") port, in("ax") value,
        options(nomem, nostack, preserves_flags));
}
