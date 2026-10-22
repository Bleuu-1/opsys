use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};

const CATALOG_LBA: u32 = 2049;
const PACKAGE_DATA_LBA: u32 = 2050;
const MAX_PACKAGE_BYTES: usize = 32 * 1024;
const MAX_PACKAGE_SECTORS: usize = MAX_PACKAGE_BYTES / 512;
const CATALOG_MAGIC: &[u8; 8] = b"BLEUU001";
const PACKAGE_NAME: &[u8; 5] = b"hello";
const DESKTOP_MAGIC: &[u8; 8] = b"BLUDESK1";

static STORAGE_ENABLED: AtomicBool = AtomicBool::new(false);

pub fn initialize() {
    STORAGE_ENABLED.store(crate::ata::identify().is_some(), Ordering::Release);
}

pub fn storage_enabled() -> bool {
    STORAGE_ENABLED.load(Ordering::Acquire)
}

pub fn desktop_installed() -> bool {
    if !storage_enabled() {
        return false;
    }
    let mut sector = [0u8; 512];
    crate::ata::read_sector(crate::system::DESKTOP_LBA, &mut sector)
        && &sector[..8] == DESKTOP_MAGIC
}

pub fn install_desktop() -> bool {
    if !storage_enabled() {
        return false;
    }
    let mut sector = [0u8; 512];
    sector[..8].copy_from_slice(DESKTOP_MAGIC);
    sector[8..23].copy_from_slice(b"VBE-SOFTWARE-UI");
    crate::ata::write_sector(crate::system::DESKTOP_LBA, &sector)
}

pub fn remove_desktop() -> bool {
    if !desktop_installed() {
        return false;
    }
    crate::ata::write_sector(crate::system::DESKTOP_LBA, &[0; 512])
}

pub fn execute(command: &[u8], mut write: impl FnMut(&[u8])) -> bool {
    if command == b"bleuu" || command == b"bleuu status" {
        print_status(&mut write);
        return true;
    }
    if command == b"bleuu packages" || command == b"bleuu list" {
        print_packages(&mut write);
        return true;
    }
    if command == b"bleuu install hello" {
        if install_hello() {
            write(b"Bleuu: installed hello on the IDE disk.\n");
        } else if package_installed() {
            write(b"Bleuu: hello is already installed.\n");
        } else {
            write(b"Bleuu: install failed; check the IDE disk.\n");
        }
        return true;
    }
    if command == b"bleuu remove hello" {
        stop_hello(&mut write);
        if remove_hello() {
            write(b"Bleuu: removed hello from the package catalog.\n");
        } else {
            write(b"Bleuu: hello is not installed or the disk is unavailable.\n");
        }
        return true;
    }
    if command == b"bleuu start hello" {
        start_hello(&mut write);
        return true;
    }
    if command == b"run" || command == b"run HELLO.ELF" || command == b"run hello.elf" {
        start_embedded_hello(&mut write);
        return true;
    }
    if command == b"bleuu stop hello" || command == b"kill 3" {
        stop_hello(&mut write);
        return true;
    }
    if command == b"bleuu start storage" {
        if crate::ata::identify().is_some() {
            STORAGE_ENABLED.store(true, Ordering::Release);
            write(b"Bleuu: storage service started.\n");
        } else {
            write(b"Bleuu: no IDE disk found.\n");
        }
        return true;
    }
    if command == b"bleuu stop storage" {
        STORAGE_ENABLED.store(false, Ordering::Release);
        write(b"Bleuu: storage service stopped.\n");
        return true;
    }
    if command == b"bleuu start serial" {
        unsafe { crate::serial::initialize() };
        write(b"Bleuu: serial service started.\n");
        return true;
    }
    if command == b"bleuu stop serial" {
        crate::serial::disable();
        write(b"Bleuu: serial service stopped.\n");
        return true;
    }
    if command == b"bleuu restart hello" {
        stop_hello(&mut write);
        start_hello(&mut write);
        return true;
    }
    if command == b"bleuu help" {
        print_help(&mut write);
        return true;
    }
    false
}

pub fn start_hello(write: &mut impl FnMut(&[u8])) {
    let Some(image) = load_package(PACKAGE_NAME) else {
        write(b"Bleuu: hello is not installed. Run 'bleuu install hello'.\n");
        return;
    };
    let Some(program) = (unsafe { crate::user::load_elf(&image) }) else {
        write(b"Bleuu: hello package is damaged or not a valid ELF.\n");
        return;
    };
    if unsafe { crate::task::spawn_user(program.entry, program.stack, program.root) } {
        write(b"Bleuu: started hello (PID 3, ring 3).\n");
    } else {
        unsafe { crate::memory::destroy_address_space(program.root) };
        write(b"Bleuu: hello is already running.\n");
    }
}

fn start_embedded_hello(write: &mut impl FnMut(&[u8])) {
    let Some(image) = crate::fs::executable(b"HELLO.ELF") else {
        write(b"Bleuu: embedded hello image is missing.\n");
        return;
    };
    let Some(program) = (unsafe { crate::user::load_elf(image) }) else {
        write(b"Bleuu: embedded hello image is not a valid ELF.\n");
        return;
    };
    if unsafe { crate::task::spawn_user(program.entry, program.stack, program.root) } {
        write(b"Started embedded HELLO.ELF (PID 3, ring 3).\n");
    } else {
        unsafe { crate::memory::destroy_address_space(program.root) };
        write(b"Bleuu: hello is already running.\n");
    }
}

pub fn stop_hello(write: &mut impl FnMut(&[u8])) {
    match crate::task::snapshot(3).map(|(_, state, _)| state) {
        Some(crate::task::EMPTY | crate::task::EXITED) | None => {
            write(b"Bleuu: hello is already stopped.\n");
        }
        _ if unsafe { crate::task::kill_user() } => {
            write(b"Bleuu: stopped hello.\n");
        }
        _ => write(b"Bleuu: could not stop hello.\n"),
    }
}

fn print_status(write: &mut impl FnMut(&[u8])) {
    write(b"Bleuu lightweight service manager\n");
    write(if crate::serial::enabled() {
        b"serial: running\n"
    } else {
        b"serial: stopped\n"
    });
    write(if storage_enabled() {
        b"storage: running\n"
    } else {
        b"storage: stopped/unavailable\n"
    });
    let state = crate::task::snapshot(3).map(|(_, state, _)| state);
    write(match state {
        Some(crate::task::RUNNING) => b"hello: running\n",
        Some(crate::task::READY) => b"hello: ready\n",
        _ => b"hello: stopped\n",
    });
    print_help(write);
}

fn print_help(write: &mut impl FnMut(&[u8])) {
    write(b"Usage: bleuu status|packages|install hello|remove hello\n");
    write(b"       bleuu start|stop|restart hello\n");
    write(b"       bleuu start|stop storage|serial\n");
}

fn print_packages(write: &mut impl FnMut(&[u8])) {
    write(b"Available packages:\n  hello (embedded source ELF) - ");
    write(if package_installed() {
        b"installed on IDE disk\n"
    } else {
        b"not installed\n"
    });
    write(b"  desktop (built-in VBE desktop profile) - ");
    write(if desktop_installed() {
        b"installed\n"
    } else {
        b"not installed\n"
    });
}

fn package_installed() -> bool {
    catalog_entry().is_some()
}

fn install_hello() -> bool {
    if !storage_enabled() {
        return false;
    }
    let image = crate::user::HELLO_ELF;
    if image.is_empty() || image.len() > MAX_PACKAGE_BYTES || image.len() > u32::MAX as usize {
        return false;
    }
    if package_installed() {
        return false;
    }
    let Some(disk) = crate::ata::identify() else {
        return false;
    };
    let sector_count = (image.len() + 511) / 512;
    let Some(end_lba) = PACKAGE_DATA_LBA.checked_add(sector_count as u32) else {
        return false;
    };
    if end_lba > disk.sectors {
        return false;
    }
    for (index, chunk) in image.chunks(512).enumerate() {
        let mut sector = [0u8; 512];
        sector[..chunk.len()].copy_from_slice(chunk);
        if !crate::ata::write_sector(PACKAGE_DATA_LBA + index as u32, &sector) {
            return false;
        }
    }
    if checksum_disk(image.len()) != Some(checksum(image)) {
        return false;
    }
    let mut catalog = [0u8; 512];
    catalog[..8].copy_from_slice(CATALOG_MAGIC);
    catalog[8..13].copy_from_slice(PACKAGE_NAME);
    catalog[16..20].copy_from_slice(&(image.len() as u32).to_le_bytes());
    catalog[20..24].copy_from_slice(&checksum(image).to_le_bytes());
    catalog[24..28].copy_from_slice(&(sector_count as u32).to_le_bytes());
    crate::ata::write_sector(CATALOG_LBA, &catalog)
}

fn remove_hello() -> bool {
    if !storage_enabled() || catalog_entry().is_none() {
        return false;
    }
    crate::ata::write_sector(CATALOG_LBA, &[0u8; 512])
}

fn load_package(name: &[u8]) -> Option<Vec<u8>> {
    if !storage_enabled() || !name.eq_ignore_ascii_case(PACKAGE_NAME) {
        return None;
    }
    let (length, expected_checksum) = catalog_entry()?;
    if length == 0 || length > MAX_PACKAGE_BYTES {
        return None;
    }
    let disk = crate::ata::identify()?;
    let mut image = Vec::with_capacity(length);
    let sector_count = (length + 511) / 512;
    let end_lba = PACKAGE_DATA_LBA.checked_add(sector_count as u32)?;
    if end_lba > disk.sectors {
        return None;
    }
    for index in 0..sector_count {
        let mut sector = [0u8; 512];
        if !crate::ata::read_sector(PACKAGE_DATA_LBA + index as u32, &mut sector) {
            return None;
        }
        let remaining = length - image.len();
        image.extend_from_slice(&sector[..remaining.min(512)]);
    }
    if checksum(&image) == expected_checksum {
        Some(image)
    } else {
        None
    }
}

fn catalog_entry() -> Option<(usize, u32)> {
    if !storage_enabled() {
        return None;
    }
    let mut sector = [0u8; 512];
    if !crate::ata::read_sector(CATALOG_LBA, &mut sector)
        || &sector[..8] != CATALOG_MAGIC
        || &sector[8..13] != PACKAGE_NAME
    {
        return None;
    }
    let length = u32::from_le_bytes(sector[16..20].try_into().ok()?) as usize;
    let expected_sectors = (length + 511) / 512;
    let stored_sectors = u32::from_le_bytes(sector[24..28].try_into().ok()?) as usize;
    if length == 0
        || length > MAX_PACKAGE_BYTES
        || expected_sectors != stored_sectors
        || stored_sectors > MAX_PACKAGE_SECTORS
    {
        return None;
    }
    let disk = crate::ata::identify()?;
    let end_lba = PACKAGE_DATA_LBA.checked_add(stored_sectors as u32)?;
    if end_lba > disk.sectors {
        return None;
    }
    let checksum = u32::from_le_bytes(sector[20..24].try_into().ok()?);
    Some((length, checksum))
}

fn checksum(bytes: &[u8]) -> u32 {
    let mut hash = 0x811c_9dc5u32;
    for byte in bytes {
        hash = (hash ^ *byte as u32).wrapping_mul(0x0100_0193);
    }
    hash
}

fn checksum_disk(length: usize) -> Option<u32> {
    if length == 0 || length > MAX_PACKAGE_BYTES {
        return None;
    }
    let mut hash = 0x811c_9dc5u32;
    let mut remaining = length;
    let sectors = (length + 511) / 512;
    for index in 0..sectors {
        let mut sector = [0u8; 512];
        if !crate::ata::read_sector(PACKAGE_DATA_LBA + index as u32, &mut sector) {
            return None;
        }
        let count = remaining.min(512);
        for byte in &sector[..count] {
            hash = (hash ^ *byte as u32).wrapping_mul(0x0100_0193);
        }
        remaining -= count;
    }
    Some(hash)
}
