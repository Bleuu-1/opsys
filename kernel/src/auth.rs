use crate::system;

const MAGIC: &[u8; 8] = b"OPSAUTH1";
const USERNAME_MAX: usize = 31;
const PASSWORD_MAX: usize = 63;
const KDF_ROUNDS: u32 = 10_000;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    SetupUsername,
    SetupUserPassword,
    SetupRootPassword,
    LoginUsername,
    LoginPassword,
    RootPassword,
    Ready,
    DiskUnavailable,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Result {
    Continue,
    Accepted,
    Rejected,
    SetupComplete,
    Invalid,
}

static mut STAGE: Stage = Stage::DiskUnavailable;
static mut USERNAME: [u8; USERNAME_MAX] = [0; USERNAME_MAX];
static mut USERNAME_LEN: usize = 0;
static mut ROOT_SESSION: bool = false;
// A failed root-password attempt should return to the state from which root
// authentication was requested: the login prompt for direct root login, or
// the normal shell after `su`.
static mut ROOT_RETURN_STAGE: Stage = Stage::LoginUsername;
static mut PENDING_NAME: [u8; USERNAME_MAX] = [0; USERNAME_MAX];
static mut PENDING_NAME_LEN: usize = 0;
static mut PENDING_USER_SALT: [u8; 16] = [0; 16];
static mut PENDING_ROOT_SALT: [u8; 16] = [0; 16];
static mut PENDING_USER_HASH: [u8; 32] = [0; 32];

pub fn initialize() -> Stage {
    unsafe {
        ROOT_SESSION = false;
        if !system::initialize_disk() {
            STAGE = Stage::DiskUnavailable;
            return STAGE;
        }
        let mut sector = [0u8; 512];
        if crate::ata::read_sector(system::ACCOUNT_LBA, &mut sector)
            && &sector[..8] == MAGIC
            && sector[8] > 0
            && sector[8] as usize <= USERNAME_MAX
        {
            STAGE = Stage::LoginUsername;
        } else {
            STAGE = Stage::SetupUsername;
        }
        STAGE
    }
}

pub fn stage() -> Stage {
    unsafe { STAGE }
}

pub fn prompt() -> &'static [u8] {
    match stage() {
        Stage::SetupUsername => b"create username: ",
        Stage::SetupUserPassword => b"set user password (8+ chars): ",
        Stage::SetupRootPassword => b"set root password (8+ chars): ",
        Stage::LoginUsername => b"login: ",
        Stage::LoginPassword => b"password: ",
        Stage::RootPassword => b"root password: ",
        Stage::Ready if is_root() => b"opsys# ",
        Stage::Ready => b"opsys> ",
        Stage::DiskUnavailable => b"setup unavailable> ",
    }
}

pub fn hides_input() -> bool {
    matches!(
        stage(),
        Stage::SetupUserPassword
            | Stage::SetupRootPassword
            | Stage::LoginPassword
            | Stage::RootPassword
    )
}

pub fn is_ready() -> bool {
    stage() == Stage::Ready
}

pub fn is_root() -> bool {
    unsafe { ROOT_SESSION }
}

pub fn username() -> &'static [u8] {
    unsafe { &USERNAME[..USERNAME_LEN] }
}

pub fn submit(line: &[u8]) -> Result {
    unsafe {
        match STAGE {
            Stage::SetupUsername => {
                if !valid_username(line) {
                    return Result::Invalid;
                }
                PENDING_NAME = [0; USERNAME_MAX];
                PENDING_NAME[..line.len()].copy_from_slice(line);
                PENDING_NAME_LEN = line.len();
                STAGE = Stage::SetupUserPassword;
                Result::Continue
            }
            Stage::SetupUserPassword => {
                if line.len() < 8 || line.len() > PASSWORD_MAX {
                    return Result::Invalid;
                }
                let salt = make_salt(line, 0x51);
                PENDING_USER_SALT = salt;
                PENDING_USER_HASH = pbkdf2(line, &salt);
                STAGE = Stage::SetupRootPassword;
                Result::Continue
            }
            Stage::SetupRootPassword => {
                if line.len() < 8 || line.len() > PASSWORD_MAX {
                    return Result::Invalid;
                }
                let root_salt = make_salt(line, 0xa7);
                PENDING_ROOT_SALT = root_salt;
                let root_hash = pbkdf2(line, &root_salt);
                let username = PENDING_NAME;
                let username_len = PENDING_NAME_LEN;
                let user_salt = PENDING_USER_SALT;
                let user_hash = PENDING_USER_HASH;
                let mut sector = [0u8; 512];
                sector[..8].copy_from_slice(MAGIC);
                sector[8] = username_len as u8;
                sector[9..9 + username_len].copy_from_slice(&username[..username_len]);
                sector[40..56].copy_from_slice(&user_salt);
                sector[56..88].copy_from_slice(&user_hash);
                sector[88..104].copy_from_slice(&root_salt);
                sector[104..136].copy_from_slice(&root_hash);
                if !crate::ata::write_sector(system::ACCOUNT_LBA, &sector) {
                    return Result::Rejected;
                }
                USERNAME = username;
                USERNAME_LEN = username_len;
                ROOT_SESSION = true;
                STAGE = Stage::Ready;
                Result::SetupComplete
            }
            Stage::LoginUsername => {
                if line.is_empty() || line.len() > USERNAME_MAX {
                    return Result::Invalid;
                }
                if is_root_name(line) {
                    ROOT_RETURN_STAGE = Stage::LoginUsername;
                    STAGE = Stage::RootPassword;
                    return Result::Continue;
                }
                let mut sector = [0u8; 512];
                if !crate::ata::read_sector(system::ACCOUNT_LBA, &mut sector)
                    || &sector[..8] != MAGIC
                {
                    return Result::Rejected;
                }
                let stored_len = sector[8] as usize;
                if stored_len == 0
                    || stored_len > USERNAME_MAX
                    || stored_len != line.len()
                    || !ascii_case_eq(&sector[9..9 + stored_len], line)
                {
                    return Result::Rejected;
                }
                USERNAME = [0; USERNAME_MAX];
                USERNAME[..stored_len].copy_from_slice(&sector[9..9 + stored_len]);
                USERNAME_LEN = stored_len;
                STAGE = Stage::LoginPassword;
                Result::Continue
            }
            Stage::LoginPassword => {
                let mut sector = [0u8; 512];
                if !crate::ata::read_sector(system::ACCOUNT_LBA, &mut sector)
                    || &sector[..8] != MAGIC
                {
                    STAGE = Stage::DiskUnavailable;
                    return Result::Rejected;
                }
                let mut salt = [0u8; 16];
                salt.copy_from_slice(&sector[40..56]);
                let actual = pbkdf2(line, &salt);
                if constant_time_eq(&actual, &sector[56..88]) {
                    ROOT_SESSION = false;
                    STAGE = Stage::Ready;
                    Result::Accepted
                } else {
                    STAGE = Stage::LoginUsername;
                    Result::Rejected
                }
            }
            Stage::RootPassword => {
                let mut sector = [0u8; 512];
                if !crate::ata::read_sector(system::ACCOUNT_LBA, &mut sector)
                    || &sector[..8] != MAGIC
                {
                    STAGE = Stage::DiskUnavailable;
                    return Result::Rejected;
                }
                let mut salt = [0u8; 16];
                salt.copy_from_slice(&sector[88..104]);
                let actual = pbkdf2(line, &salt);
                if constant_time_eq(&actual, &sector[104..136]) {
                    ROOT_SESSION = true;
                    STAGE = Stage::Ready;
                    Result::Accepted
                } else {
                    STAGE = ROOT_RETURN_STAGE;
                    Result::Rejected
                }
            }
            Stage::Ready | Stage::DiskUnavailable => Result::Rejected,
        }
    }
}

pub fn begin_login() {
    unsafe {
        ROOT_SESSION = false;
        STAGE = Stage::LoginUsername;
    }
}

pub fn begin_root_login() -> bool {
    unsafe {
        if STAGE != Stage::Ready || ROOT_SESSION {
            return false;
        }
        ROOT_RETURN_STAGE = Stage::Ready;
        STAGE = Stage::RootPassword;
        true
    }
}

fn valid_username(value: &[u8]) -> bool {
    !value.is_empty()
        && value.len() <= USERNAME_MAX
        && !is_root_name(value)
        && value
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'_' || *byte == b'-')
}

fn is_root_name(value: &[u8]) -> bool {
    ascii_case_eq(value, b"root")
}

fn ascii_case_eq(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
}

fn make_salt(password: &[u8], domain: u8) -> [u8; 16] {
    let time: u64;
    // rdtsc's two outputs are collected separately to avoid requiring a CPU RNG.
    let lo: u32;
    let hi: u32;
    unsafe {
        core::arch::asm!("rdtsc", out("eax") lo, out("edx") hi, options(nomem, nostack));
    }
    time = ((hi as u64) << 32) | lo as u64;
    let mut state = time ^ ((password.len() as u64) << 32) ^ domain as u64;
    let mut salt = [0u8; 16];
    for byte in &mut salt {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        *byte = state as u8;
    }
    salt
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in left.iter().zip(right) {
        diff |= *a ^ *b;
    }
    diff == 0
}

fn pbkdf2(password: &[u8], salt: &[u8; 16]) -> [u8; 32] {
    let mut block = [0u8; 20];
    block[..16].copy_from_slice(salt);
    block[19] = 1;
    let mut u = hmac_sha256(password, &block);
    let mut output = u;
    for _ in 1..KDF_ROUNDS {
        u = hmac_sha256(password, &u);
        for index in 0..32 {
            output[index] ^= u[index];
        }
    }
    output
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut key_block = [0u8; 64];
    if key.len() > 64 {
        key_block[..32].copy_from_slice(&sha256(key));
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }
    let mut inner = [0u8; 128];
    for index in 0..64 {
        inner[index] = key_block[index] ^ 0x36;
    }
    inner[64..64 + message.len()].copy_from_slice(message);
    let inner_hash = sha256(&inner[..64 + message.len()]);
    let mut outer = [0u8; 96];
    for index in 0..64 {
        outer[index] = key_block[index] ^ 0x5c;
    }
    outer[64..].copy_from_slice(&inner_hash);
    sha256(&outer)
}

fn sha256(input: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut state = [
        0x6a09e667u32,
        0xbb67ae85,
        0x3c6ef372,
        0xa54ff53a,
        0x510e527f,
        0x9b05688c,
        0x1f83d9ab,
        0x5be0cd19,
    ];
    let bit_len = (input.len() as u64).wrapping_mul(8);
    let padded_len = ((input.len() + 9 + 63) / 64) * 64;
    for chunk_index in 0..(padded_len / 64) {
        let mut chunk = [0u8; 64];
        for (i, byte) in chunk.iter_mut().enumerate() {
            let pos = chunk_index * 64 + i;
            *byte = if pos < input.len() {
                input[pos]
            } else if pos == input.len() {
                0x80
            } else if pos >= padded_len - 8 {
                (bit_len >> ((padded_len - 1 - pos) * 8)) as u8
            } else {
                0
            };
        }
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(chunk[i * 4..i * 4 + 4].try_into().unwrap());
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = state;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
        state[4] = state[4].wrapping_add(e);
        state[5] = state[5].wrapping_add(f);
        state[6] = state[6].wrapping_add(g);
        state[7] = state[7].wrapping_add(h);
    }
    let mut result = [0u8; 32];
    for (i, word) in state.iter().enumerate() {
        result[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    result
}
