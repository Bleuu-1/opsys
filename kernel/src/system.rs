use crate::ata;

pub const SYSTEM_START: u32 = 2048;
pub const HOME_START: u32 = 4096;
pub const ACCOUNT_LBA: u32 = 2114;
pub const DESKTOP_LBA: u32 = 2115;

const SYSTEM_SECTORS: u32 = HOME_START - SYSTEM_START;
const MBR_SIGNATURE: [u8; 2] = [0x55, 0xaa];

pub fn initialize_disk() -> bool {
    let Some(disk) = ata::identify() else {
        return false;
    };
    if disk.sectors <= HOME_START + 16 {
        return false;
    }

    let mut mbr = [0u8; 512];
    if !ata::read_sector(0, &mut mbr) {
        return false;
    }
    if mbr[510..512] == MBR_SIGNATURE {
        return valid_layout(&mbr, disk.sectors);
    }

    // Only claim disks whose first sector is entirely blank. Existing disks
    // are left untouched and setup is refused rather than guessing a layout.
    if mbr.iter().any(|byte| *byte != 0) {
        return false;
    }
    let home_sectors = disk.sectors - HOME_START;
    write_partition(&mut mbr, 0, 0x7f, SYSTEM_START, SYSTEM_SECTORS);
    write_partition(&mut mbr, 1, 0x7e, HOME_START, home_sectors);
    mbr[510..512].copy_from_slice(&MBR_SIGNATURE);
    ata::write_sector(0, &mbr) && valid_layout(&mbr, disk.sectors)
}

fn write_partition(mbr: &mut [u8; 512], index: usize, kind: u8, start: u32, length: u32) {
    let offset = 446 + index * 16;
    mbr[offset] = 0;
    mbr[offset + 1..offset + 4].copy_from_slice(&[0xfe, 0xff, 0xff]);
    mbr[offset + 4] = kind;
    mbr[offset + 5..offset + 8].copy_from_slice(&[0xfe, 0xff, 0xff]);
    mbr[offset + 8..offset + 12].copy_from_slice(&start.to_le_bytes());
    mbr[offset + 12..offset + 16].copy_from_slice(&length.to_le_bytes());
}

fn valid_layout(mbr: &[u8; 512], disk_sectors: u32) -> bool {
    if mbr[510..512] != MBR_SIGNATURE {
        return false;
    }
    let system = 446;
    let home = 462;
    let system_start = u32::from_le_bytes(mbr[system + 8..system + 12].try_into().unwrap());
    let system_length = u32::from_le_bytes(mbr[system + 12..system + 16].try_into().unwrap());
    let home_start = u32::from_le_bytes(mbr[home + 8..home + 12].try_into().unwrap());
    let home_length = u32::from_le_bytes(mbr[home + 12..home + 16].try_into().unwrap());
    mbr[system + 4] == 0x7f
        && mbr[home + 4] == 0x7e
        && system_start == SYSTEM_START
        && system_length == SYSTEM_SECTORS
        && home_start == HOME_START
        && home_length > 0
        && home_start.checked_add(home_length) == Some(disk_sectors)
}

pub fn partitions(write: &mut impl FnMut(&[u8]), mut number: impl FnMut(u32)) {
    let Some(disk) = ata::identify() else {
        write(b"No IDE data disk detected.\n");
        return;
    };
    let mut mbr = [0u8; 512];
    if !ata::read_sector(0, &mut mbr) || !valid_layout(&mbr, disk.sectors) {
        write(b"No OpSYS partition layout. Run setup on a blank QEMU data disk.\n");
        return;
    }
    write(b"MBR partitions (custom raw layout; no general filesystem yet):\nSYSTEM  LBA ");
    number(SYSTEM_START);
    write(b".. ");
    number(HOME_START - 1);
    write(b" (Bleuu metadata/packages)\nHOME    LBA ");
    number(HOME_START);
    write(b".. ");
    number(disk.sectors - 1);
    write(b" (persistent user data)\n");
}
