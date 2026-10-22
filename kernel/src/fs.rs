use crate::ata;

const NOTE_LBA: u32 = 2048;
const NOTE_MAGIC: &[u8; 8] = b"OPSYN001";
static README: &[u8] = b"OpSYS RAM filesystem\nFiles: README.TXT, HELLO.ELF\nUse 'run HELLO.ELF' to launch a ring-3 program.\nUse 'write NOTE text' and 'cat NOTE' for persistent disk storage.\n";

pub fn list(mut write: impl FnMut(&[u8])) {
    write(b"README.TXT  (RAM)\nHELLO.ELF   (RAM)\n");
    if note().is_some() {
        write(b"NOTE        (IDE disk)\n");
    }
}

pub fn read(name: &[u8]) -> Option<&'static [u8]> {
    if name.eq_ignore_ascii_case(b"README.TXT") {
        Some(README)
    } else {
        None
    }
}

pub fn executable(name: &[u8]) -> Option<&'static [u8]> {
    if name.eq_ignore_ascii_case(b"HELLO.ELF") {
        Some(crate::user::HELLO_ELF)
    } else {
        None
    }
}

pub fn read_note(buffer: &mut [u8; 512]) -> Option<&[u8]> {
    if !crate::bleuu::storage_enabled()
        || !ata::read_sector(NOTE_LBA, buffer)
        || &buffer[..8] != NOTE_MAGIC
    {
        return None;
    }
    let length = u16::from_le_bytes([buffer[8], buffer[9]]) as usize;
    if length > 502 {
        return None;
    }
    Some(&buffer[10..10 + length])
}

pub fn note() -> Option<usize> {
    let mut sector = [0u8; 512];
    read_note(&mut sector).map(|bytes| bytes.len())
}

pub fn write_note(contents: &[u8]) -> bool {
    if !crate::bleuu::storage_enabled() || contents.len() > 502 {
        return false;
    }
    let mut sector = [0u8; 512];
    sector[..8].copy_from_slice(NOTE_MAGIC);
    sector[8..10].copy_from_slice(&(contents.len() as u16).to_le_bytes());
    sector[10..10 + contents.len()].copy_from_slice(contents);
    ata::write_sector(NOTE_LBA, &sector)
}
