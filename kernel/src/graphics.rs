use crate::memory::{self, BootInfo, PAGE_SIZE};

const FRAMEBUFFER_VIRTUAL_BASE: u64 = 0xffff_ffff_d000_0000;
const PRESENT: u64 = 1;
const WRITABLE: u64 = 2;
const PWT: u64 = 1 << 3;
const PCD: u64 = 1 << 4;
const MAX_FRAMEBUFFER_BYTES: usize = 16 * 1024 * 1024;
const TEXT_X: usize = 48;
const TEXT_Y: usize = 98;
const TEXT_CELL_WIDTH: usize = 8;
const TEXT_CELL_HEIGHT: usize = 16;

static mut PIXELS: *mut u8 = core::ptr::null_mut();
static mut WIDTH: usize = 0;
static mut HEIGHT: usize = 0;
static mut PITCH_BYTES: usize = 0;
static mut BYTES_PER_PIXEL: usize = 0;
static mut RED_SHIFT: u32 = 16;
static mut GREEN_SHIFT: u32 = 8;
static mut BLUE_SHIFT: u32 = 0;
static mut ACTIVE: bool = false;
static mut MOUSE_X: i32 = 0;
static mut MOUSE_Y: i32 = 0;
static mut CURSOR_DRAWN: bool = false;
static mut LAST_BUTTONS: u8 = 0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum UiAction {
    None,
    BleuuStatus,
}

pub unsafe fn initialize(info: &BootInfo) -> bool {
    if info.framebuffer_phys == 0
        || (info.framebuffer_bpp != 24 && info.framebuffer_bpp != 32)
        || info.framebuffer_width < 800
        || info.framebuffer_height < 600
    {
        return false;
    }
    let width = info.framebuffer_width as usize;
    let height = info.framebuffer_height as usize;
    let pitch = info.framebuffer_pitch as usize;
    let bytes_per_pixel = (info.framebuffer_bpp / 8) as usize;
    let Some(row_bytes) = width.checked_mul(bytes_per_pixel) else {
        return false;
    };
    let Some(bytes) = pitch.checked_mul(height) else {
        return false;
    };
    if pitch < row_bytes
        || pitch % bytes_per_pixel != 0
        || bytes == 0
        || bytes > MAX_FRAMEBUFFER_BYTES
    {
        return false;
    }
    let physical_page = info.framebuffer_phys & !(PAGE_SIZE - 1);
    let offset = (info.framebuffer_phys - physical_page) as usize;
    let Some(mapped_bytes) = bytes.checked_add(offset) else {
        return false;
    };
    let pages = (mapped_bytes + PAGE_SIZE as usize - 1) / PAGE_SIZE as usize;
    if pages == 0 || pages > MAX_FRAMEBUFFER_BYTES / PAGE_SIZE as usize + 1 {
        return false;
    }

    for index in 0..pages {
        let virtual_page = FRAMEBUFFER_VIRTUAL_BASE + index as u64 * PAGE_SIZE;
        let physical = physical_page + index as u64 * PAGE_SIZE;
        if !memory::map_page(virtual_page, physical, PRESENT | WRITABLE | PWT | PCD) {
            for mapped in 0..index {
                let _ = memory::unmap_page(FRAMEBUFFER_VIRTUAL_BASE + mapped as u64 * PAGE_SIZE);
            }
            return false;
        }
    }

    PIXELS = (FRAMEBUFFER_VIRTUAL_BASE as usize + offset) as *mut u8;
    WIDTH = width;
    HEIGHT = height;
    PITCH_BYTES = pitch;
    BYTES_PER_PIXEL = bytes_per_pixel;
    RED_SHIFT = (info.framebuffer_rgb_positions & 0xff).min(24);
    GREEN_SHIFT = ((info.framebuffer_rgb_positions >> 8) & 0xff).min(24);
    BLUE_SHIFT = ((info.framebuffer_rgb_positions >> 16) & 0xff).min(24);
    MOUSE_X = (width / 2) as i32;
    MOUSE_Y = (height / 2) as i32;
    ACTIVE = true;
    draw_desktop();
    draw_mouse_cursor();
    true
}

pub fn active() -> bool {
    unsafe { ACTIVE }
}

pub fn dimensions() -> (u32, u32) {
    unsafe { (WIDTH as u32, HEIGHT as u32) }
}

pub fn refresh(cells: *const [[u16; 80]; 25]) {
    unsafe {
        if ACTIVE {
            draw_desktop();
            CURSOR_DRAWN = false;
            render_console_buffer(cells);
        }
    }
}

pub fn render_console_cell(row: usize, column: usize, character: u8, attribute: u8) {
    unsafe {
        if !ACTIVE || row >= 25 || column >= 80 {
            return;
        }
        let redraw_cursor = CURSOR_DRAWN;
        if redraw_cursor {
            toggle_mouse_cursor();
        }
        draw_console_cell(row, column, character, attribute);
        if redraw_cursor {
            draw_mouse_cursor();
        }
    }
}

unsafe fn draw_console_cell(row: usize, column: usize, character: u8, attribute: u8) {
    let foreground = vga_color(attribute & 0x0f);
    let background = vga_color((attribute >> 4) & 0x0f);
    draw_terminal_character(
        TEXT_X + column * TEXT_CELL_WIDTH,
        TEXT_Y + row * TEXT_CELL_HEIGHT,
        character,
        foreground,
        background,
    );
}

/// Redraw a text console cell from the existing VGA text buffer.
pub unsafe fn render_console_buffer(cells: *const [[u16; 80]; 25]) {
    unsafe {
        if !ACTIVE {
            return;
        }
        if CURSOR_DRAWN {
            toggle_mouse_cursor();
        }
        for row in 0..25 {
            for column in 0..80 {
                let cell = cells.cast::<u16>().add(row * 80 + column).read_volatile();
                draw_console_cell(row, column, cell as u8, (cell >> 8) as u8);
            }
        }
        draw_mouse_cursor();
    }
}

pub fn mouse_moved(x: i32, y: i32, buttons: u8) -> UiAction {
    unsafe {
        if !ACTIVE {
            return UiAction::None;
        }
        if CURSOR_DRAWN {
            toggle_mouse_cursor();
        }
        MOUSE_X = x.clamp(0, WIDTH.saturating_sub(1) as i32);
        MOUSE_Y = y.clamp(0, HEIGHT.saturating_sub(1) as i32);
        let clicked = buttons & 1 != 0 && LAST_BUTTONS & 1 == 0;
        LAST_BUTTONS = buttons;
        draw_mouse_cursor();

        if clicked {
            let taskbar_y = HEIGHT.saturating_sub(42);
            if MOUSE_Y as usize >= taskbar_y + 5
                && (MOUSE_Y as usize) < taskbar_y + 37
                && (148..238).contains(&(MOUSE_X as usize))
            {
                return UiAction::BleuuStatus;
            }
        }
        UiAction::None
    }
}

unsafe fn draw_desktop() {
    fill_rect(0, 0, WIDTH, HEIGHT, rgb(13, 22, 39));
    // A restrained desktop background with a blue accent panel.
    fill_rect(0, 40, WIDTH, HEIGHT.saturating_sub(82), rgb(17, 31, 53));
    fill_rect(0, 40, 8, HEIGHT.saturating_sub(82), rgb(36, 117, 176));

    fill_rect(0, 0, WIDTH, 38, rgb(8, 14, 26));
    draw_text(14, 13, b"OPSYS DESKTOP", rgb(235, 245, 255));
    draw_text(
        WIDTH.saturating_sub(165),
        13,
        b"QEMU  X86_64",
        rgb(145, 167, 190),
    );

    let window_width = WIDTH.saturating_sub(80);
    let window_height = HEIGHT.saturating_sub(122);
    fill_rect(34, 51, window_width, window_height, rgb(4, 9, 18));
    fill_rect(38, 55, window_width - 8, window_height - 8, rgb(3, 7, 14));
    fill_rect(38, 55, window_width - 8, 30, rgb(29, 43, 64));
    draw_text(
        52,
        66,
        b"OPSYS TERMINAL  -  KEYBOARD SHELL",
        rgb(220, 232, 245),
    );
    fill_rect(window_width + 5, 64, 10, 10, rgb(231, 91, 91));
    fill_rect(window_width + 20, 64, 10, 10, rgb(231, 181, 79));
    fill_rect(window_width + 35, 64, 10, 10, rgb(79, 190, 135));

    let taskbar_y = HEIGHT.saturating_sub(42);
    fill_rect(0, taskbar_y, WIDTH, HEIGHT - taskbar_y, rgb(8, 14, 26));
    draw_button(10, taskbar_y + 6, 54, 29, b"OP", rgb(38, 96, 151));
    draw_button(72, taskbar_y + 6, 68, 29, b"SHELL", rgb(33, 48, 69));
    draw_button(148, taskbar_y + 6, 88, 29, b"BLEUU", rgb(25, 91, 113));
    draw_text(
        252,
        taskbar_y + 16,
        b"CLICK BLEUU FOR SERVICE STATUS",
        rgb(143, 165, 188),
    );
}

unsafe fn draw_button(x: usize, y: usize, width: usize, height: usize, label: &[u8], color: u32) {
    fill_rect(x, y, width, height, color);
    draw_text(x + 9, y + 11, label, rgb(235, 244, 252));
}

unsafe fn draw_terminal_character(
    x: usize,
    y: usize,
    character: u8,
    foreground: u32,
    background: u32,
) {
    fill_rect(x, y, TEXT_CELL_WIDTH, TEXT_CELL_HEIGHT, background);
    if character == 0xdb {
        fill_rect(x, y, TEXT_CELL_WIDTH, TEXT_CELL_HEIGHT, foreground);
        return;
    }
    if character == 0xc4 {
        fill_rect(x, y + 7, TEXT_CELL_WIDTH, 2, foreground);
        return;
    }
    if character == 0xb3 {
        fill_rect(x + 3, y, 2, TEXT_CELL_HEIGHT, foreground);
        return;
    }
    let glyph = glyph_rows(character);
    for (row, pattern) in glyph.iter().enumerate() {
        for column in 0..5 {
            if pattern & (1 << (4 - column)) != 0 {
                fill_rect(x + column + 1, y + row * 2 + 1, 1, 2, foreground);
            }
        }
    }
}

unsafe fn draw_text(x: usize, y: usize, text: &[u8], color: u32) {
    for (index, byte) in text.iter().copied().enumerate() {
        let glyph = glyph_rows(byte);
        for (row, pattern) in glyph.iter().enumerate() {
            for column in 0..5 {
                if pattern & (1 << (4 - column)) != 0 {
                    set_pixel(x + index * 7 + column, y + row, color);
                }
            }
        }
    }
}

unsafe fn fill_rect(x: usize, y: usize, width: usize, height: usize, color: u32) {
    let x_end = x.saturating_add(width).min(WIDTH);
    let y_end = y.saturating_add(height).min(HEIGHT);
    for row in y.min(HEIGHT)..y_end {
        for column in x.min(WIDTH)..x_end {
            set_pixel(column, row, color);
        }
    }
}

unsafe fn set_pixel(x: usize, y: usize, color: u32) {
    if x < WIDTH && y < HEIGHT {
        let value = pack_rgb(color);
        let address = PIXELS.add(y * PITCH_BYTES + x * BYTES_PER_PIXEL);
        for byte in 0..BYTES_PER_PIXEL {
            address
                .add(byte)
                .write_volatile((value >> (byte * 8)) as u8);
        }
    }
}

unsafe fn toggle_mouse_cursor() {
    const CURSOR: [u16; 16] = [
        0x8000, 0xc000, 0xe000, 0xf000, 0xf800, 0xfc00, 0xfe00, 0xff00, 0xff80, 0xfc00, 0xdc00,
        0x8e00, 0x0600, 0x0700, 0x0300, 0x0000,
    ];
    for (row, bits) in CURSOR.iter().enumerate() {
        for column in 0..16 {
            if bits & (0x8000 >> column) != 0 {
                let x = MOUSE_X + column as i32;
                let y = MOUSE_Y + row as i32;
                if x >= 0 && y >= 0 && (x as usize) < WIDTH && (y as usize) < HEIGHT {
                    let pixel = PIXELS.add(y as usize * PITCH_BYTES + x as usize * BYTES_PER_PIXEL);
                    for byte in 0..BYTES_PER_PIXEL {
                        let channel = pixel.add(byte);
                        channel.write_volatile(channel.read_volatile() ^ 0xff);
                    }
                }
            }
        }
    }
    CURSOR_DRAWN = false;
}

unsafe fn draw_mouse_cursor() {
    toggle_mouse_cursor();
    CURSOR_DRAWN = true;
}

unsafe fn pack_rgb(color: u32) -> u32 {
    let red = (color >> 16) & 0xff;
    let green = (color >> 8) & 0xff;
    let blue = color & 0xff;
    (red << RED_SHIFT) | (green << GREEN_SHIFT) | (blue << BLUE_SHIFT)
}

fn rgb(red: u8, green: u8, blue: u8) -> u32 {
    ((red as u32) << 16) | ((green as u32) << 8) | blue as u32
}

fn vga_color(index: u8) -> u32 {
    const COLORS: [u32; 16] = [
        0x000000, 0x0000aa, 0x00aa00, 0x00aaaa, 0xaa0000, 0xaa00aa, 0xaa5500, 0xaaaaaa, 0x555555,
        0x5555ff, 0x55ff55, 0x55ffff, 0xff5555, 0xff55ff, 0xffff55, 0xffffff,
    ];
    COLORS[(index & 0x0f) as usize]
}

fn glyph_rows(character: u8) -> [u8; 7] {
    match character.to_ascii_uppercase() {
        b'A' => [0x0e, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11],
        b'B' => [0x1e, 0x11, 0x11, 0x1e, 0x11, 0x11, 0x1e],
        b'C' => [0x0f, 0x10, 0x10, 0x10, 0x10, 0x10, 0x0f],
        b'D' => [0x1e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1e],
        b'E' => [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x1f],
        b'F' => [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x10],
        b'G' => [0x0f, 0x10, 0x10, 0x13, 0x11, 0x11, 0x0f],
        b'H' => [0x11, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11],
        b'I' => [0x0e, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0e],
        b'J' => [0x07, 0x02, 0x02, 0x02, 0x02, 0x12, 0x0c],
        b'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        b'L' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1f],
        b'M' => [0x11, 0x1b, 0x15, 0x15, 0x11, 0x11, 0x11],
        b'N' => [0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11],
        b'O' => [0x0e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e],
        b'P' => [0x1e, 0x11, 0x11, 0x1e, 0x10, 0x10, 0x10],
        b'Q' => [0x0e, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0d],
        b'R' => [0x1e, 0x11, 0x11, 0x1e, 0x14, 0x12, 0x11],
        b'S' => [0x0f, 0x10, 0x10, 0x0e, 0x01, 0x01, 0x1e],
        b'T' => [0x1f, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        b'U' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e],
        b'V' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x0a, 0x04],
        b'W' => [0x11, 0x11, 0x11, 0x15, 0x15, 0x15, 0x0a],
        b'X' => [0x11, 0x11, 0x0a, 0x04, 0x0a, 0x11, 0x11],
        b'Y' => [0x11, 0x11, 0x0a, 0x04, 0x04, 0x04, 0x04],
        b'Z' => [0x1f, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1f],
        b'0' => [0x0e, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0e],
        b'1' => [0x04, 0x0c, 0x04, 0x04, 0x04, 0x04, 0x0e],
        b'2' => [0x0e, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1f],
        b'3' => [0x1e, 0x01, 0x01, 0x0e, 0x01, 0x01, 0x1e],
        b'4' => [0x02, 0x06, 0x0a, 0x12, 0x1f, 0x02, 0x02],
        b'5' => [0x1f, 0x10, 0x10, 0x1e, 0x01, 0x01, 0x1e],
        b'6' => [0x0e, 0x10, 0x10, 0x1e, 0x11, 0x11, 0x0e],
        b'7' => [0x1f, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        b'8' => [0x0e, 0x11, 0x11, 0x0e, 0x11, 0x11, 0x0e],
        b'9' => [0x0e, 0x11, 0x11, 0x0f, 0x01, 0x01, 0x0e],
        b':' => [0, 0x04, 0x04, 0, 0x04, 0x04, 0],
        b'.' => [0, 0, 0, 0, 0, 0x0c, 0x0c],
        b',' => [0, 0, 0, 0, 0x0c, 0x0c, 0x08],
        b'-' => [0, 0, 0, 0x1f, 0, 0, 0],
        b'_' => [0, 0, 0, 0, 0, 0, 0x1f],
        b'/' => [0x01, 0x02, 0x02, 0x04, 0x08, 0x08, 0x10],
        b'>' => [0x10, 0x08, 0x04, 0x02, 0x04, 0x08, 0x10],
        b'<' => [0x01, 0x02, 0x04, 0x08, 0x04, 0x02, 0x01],
        b'(' => [0x02, 0x04, 0x08, 0x08, 0x08, 0x04, 0x02],
        b')' => [0x08, 0x04, 0x02, 0x02, 0x02, 0x04, 0x08],
        b'!' => [0x04, 0x04, 0x04, 0x04, 0x04, 0, 0x04],
        b'\'' => [0x04, 0x04, 0x02, 0, 0, 0, 0],
        b'=' => [0, 0x1f, 0, 0x1f, 0, 0, 0],
        b'+' => [0, 0x04, 0x04, 0x1f, 0x04, 0x04, 0],
        _ => [0; 7],
    }
}
