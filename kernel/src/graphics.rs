use crate::memory::{self, BootInfo, PAGE_SIZE};
use crate::visual;
use crate::wm::{self, Placement, Rect};

const FRAMEBUFFER_VIRTUAL_BASE: u64 = 0xffff_ffff_d000_0000;
const PRESENT: u64 = 1;
const WRITABLE: u64 = 2;
const PWT: u64 = 1 << 3;
const PCD: u64 = 1 << 4;
const MAX_FRAMEBUFFER_BYTES: usize = 16 * 1024 * 1024;
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
static mut DESKTOP_MODE: bool = false;
static mut MOUSE_X: i32 = 0;
static mut MOUSE_Y: i32 = 0;
static mut CURSOR_DRAWN: bool = false;
static mut BOLD_FONT: bool = false;
static mut CELL_WIDTH: usize = TEXT_CELL_WIDTH;
static mut BACKGROUND: u8 = 0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum UiAction {
    None,
    Redraw,
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
    CELL_WIDTH = if width < 960 { 6 } else { TEXT_CELL_WIDTH };
    BACKGROUND = 0;
    MOUSE_X = (width / 2) as i32;
    MOUSE_Y = (height / 2) as i32;
    ACTIVE = true;
    DESKTOP_MODE = false;
    wm::initialize();
    fill_rect(0, 0, WIDTH, HEIGHT, rgb(3, 7, 14));
    true
}

pub fn activate(cells: *const [[u16; 80]; 25]) -> bool {
    unsafe {
        if PIXELS.is_null() || WIDTH == 0 || HEIGHT == 0 {
            return false;
        }
        wm::initialize();
        ACTIVE = true;
        DESKTOP_MODE = true;
        draw_desktop();
        CURSOR_DRAWN = false;
        render_console_buffer(cells);
        draw_mouse_cursor();
        true
    }
}

pub fn deactivate(cells: *const [[u16; 80]; 25]) {
    unsafe {
        if !ACTIVE {
            return;
        }
        DESKTOP_MODE = false;
        CURSOR_DRAWN = false;
        fill_rect(0, 0, WIDTH, HEIGHT, rgb(3, 7, 14));
        render_console_buffer(cells);
    }
}

pub fn active() -> bool {
    unsafe { ACTIVE && DESKTOP_MODE }
}

pub fn bold_font() -> bool {
    unsafe { BOLD_FONT }
}

pub fn set_bold_font(bold: bool) -> bool {
    unsafe {
        if PIXELS.is_null() {
            return false;
        }
        BOLD_FONT = bold;
        true
    }
}

pub fn set_background(preset: u8) -> bool {
    if preset >= 6 {
        return false;
    }
    unsafe {
        BACKGROUND = preset;
    }
    true
}

pub fn dimensions() -> (u32, u32) {
    unsafe { (WIDTH as u32, HEIGHT as u32) }
}

pub fn refresh(cells: *const [[u16; 80]; 25]) {
    unsafe {
        if ACTIVE {
            if DESKTOP_MODE {
                draw_desktop();
            } else {
                fill_rect(0, 0, WIDTH, HEIGHT, rgb(3, 7, 14));
            }
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
        if DESKTOP_MODE {
            draw_console_cell(row, column, character, attribute);
        } else {
            draw_terminal_character(
                32 + column * TEXT_CELL_WIDTH,
                48 + row * TEXT_CELL_HEIGHT,
                character,
                vga_color(attribute & 0x0f),
                vga_color((attribute >> 4) & 0x0f),
            );
        }
        if redraw_cursor {
            draw_mouse_cursor();
        }
    }
}

unsafe fn draw_console_cell(row: usize, column: usize, character: u8, attribute: u8) {
    let Some(placement) = wm::layout(WIDTH, HEIGHT)[wm::TERMINAL] else {
        return;
    };
    let foreground = vga_color(attribute & 0x0f);
    let background = vga_color((attribute >> 4) & 0x0f);
    let x = placement.rect.x + 9 + column * CELL_WIDTH;
    let y = placement.rect.y + 39 + row * TEXT_CELL_HEIGHT;
    if x + CELL_WIDTH > placement.rect.x + placement.rect.width
        || y + TEXT_CELL_HEIGHT > placement.rect.y + placement.rect.height
    {
        return;
    }
    draw_terminal_character(x, y, character, foreground, background);
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
        if !DESKTOP_MODE {
            fill_rect(0, 0, WIDTH, HEIGHT, rgb(3, 7, 14));
        }
        for row in 0..25 {
            for column in 0..80 {
                let cell = cells.cast::<u16>().add(row * 80 + column).read_volatile();
                let character = cell as u8;
                let attribute = (cell >> 8) as u8;
                if DESKTOP_MODE {
                    draw_console_cell(row, column, character, attribute);
                } else {
                    draw_terminal_character(
                        32 + column * TEXT_CELL_WIDTH,
                        48 + row * TEXT_CELL_HEIGHT,
                        character,
                        vga_color(attribute & 0x0f),
                        vga_color((attribute >> 4) & 0x0f),
                    );
                }
            }
        }
        if DESKTOP_MODE {
            draw_mouse_cursor();
        }
    }
}

pub fn mouse_moved(x: i32, y: i32, buttons: u8, cells: *const [[u16; 80]; 25]) -> UiAction {
    unsafe {
        if !ACTIVE || !DESKTOP_MODE {
            return UiAction::None;
        }
        if CURSOR_DRAWN {
            toggle_mouse_cursor();
        }
        MOUSE_X = x.clamp(0, WIDTH.saturating_sub(1) as i32);
        MOUSE_Y = y.clamp(0, HEIGHT.saturating_sub(1) as i32);
        let action =
            match wm::pointer_event(MOUSE_X as usize, MOUSE_Y as usize, buttons, WIDTH, HEIGHT) {
                wm::Action::None => UiAction::None,
                wm::Action::Redraw => UiAction::Redraw,
                wm::Action::BleuuStatus => UiAction::BleuuStatus,
            };
        if action == UiAction::Redraw || action == UiAction::BleuuStatus {
            draw_desktop();
            render_console_buffer(cells);
        } else {
            draw_mouse_cursor();
        }
        action
    }
}

/// Accept a bounded ARGB back buffer from the active Ring-3 panel process.
/// Transparent pixels leave the desktop underneath unchanged. This is a small
/// copy syscall rather than a shared framebuffer mapping, so user code never
/// receives direct access to the hardware framebuffer.
pub fn submit_user_bar(pixels: &[u32], width: usize, height: usize, x: u64, y: u64) -> bool {
    unsafe {
        if !active()
            || width == 0
            || width > 1920
            || height == 0
            || height > 64
            || x > WIDTH as u64
            || y > 128
            || x.saturating_add(width as u64) > WIDTH as u64
            || y.saturating_add(height as u64) > HEIGHT as u64
            || pixels.len() != width.saturating_mul(height)
        {
            return false;
        }
        if CURSOR_DRAWN {
            toggle_mouse_cursor();
        }
        for row in 0..height {
            for column in 0..width {
                let color = pixels[row * width + column];
                if color >> 24 != 0 {
                    set_pixel(x as usize + column, y as usize + row, color & 0x00ff_ffff);
                }
            }
        }
        draw_mouse_cursor();
        true
    }
}

unsafe fn draw_desktop() {
    let theme = visual::palette(BACKGROUND);
    let last_row = HEIGHT.saturating_sub(1).max(1);
    for row in 0..HEIGHT {
        let amount = (row.saturating_mul(255) / last_row).min(255) as u8;
        let color = visual::alpha_blend(theme.canvas_bottom, theme.canvas_top, amount);
        fill_rect(0, row, WIDTH, 1, color);
    }
    draw_glass_rect(
        18,
        11,
        WIDTH.saturating_sub(36),
        38,
        13,
        theme.title_glass,
        214,
    );
    draw_text(34, 26, b"BLEUU / OPSYS", theme.text);
    draw_text(166, 26, b"DESKTOP", theme.muted_text);
    let workspace = wm::current_workspace();
    draw_rounded_rect(WIDTH.saturating_sub(82), 16, 56, 27, 12, theme.glass);
    draw_text(WIDTH.saturating_sub(70), 26, b"WS", theme.muted_text);
    draw_text(
        WIDTH.saturating_sub(43),
        26,
        &[b'1' + workspace],
        theme.accent,
    );

    let placements = wm::layout(WIDTH, HEIGHT);
    for id in 0..wm::WINDOW_COUNT {
        if let Some(placement) = placements[id] {
            draw_window(placement, wm::is_focused(id));
        }
    }
    if placements.iter().all(Option::is_none) {
        draw_empty_workspace();
    }

    let taskbar_y = HEIGHT.saturating_sub(56);
    draw_glass_rect(
        12,
        taskbar_y,
        WIDTH.saturating_sub(24),
        42,
        14,
        theme.taskbar_glass,
        224,
    );
    draw_button(22, taskbar_y + 6, 44, 29, b"OP", theme.accent);
    draw_button(
        72,
        taskbar_y + 6,
        66,
        29,
        b"TERM",
        if wm::is_open(wm::TERMINAL) {
            theme.title_glass
        } else {
            theme.glass
        },
    );
    draw_button(
        142,
        taskbar_y + 6,
        55,
        29,
        b"SYS",
        if wm::is_open(wm::SYSTEM) {
            theme.title_glass
        } else {
            theme.glass
        },
    );
    draw_button(
        202,
        taskbar_y + 6,
        65,
        29,
        b"BLEUU",
        if wm::is_open(wm::BLEUU) {
            theme.title_glass
        } else {
            theme.glass
        },
    );
    draw_button(275, taskbar_y + 6, 66, 29, b"STATUS", theme.title_glass);
    for index in 0..wm::WORKSPACE_COUNT {
        let x = 354 + index as usize * 32;
        draw_button(
            x,
            taskbar_y + 6,
            28,
            29,
            &[b'1' + index],
            if index == workspace {
                theme.accent
            } else {
                theme.glass
            },
        );
    }
    draw_text(462, taskbar_y + 17, b"SUPER+J/K FOCUS", theme.muted_text);
}

unsafe fn draw_window(placement: Placement, focused: bool) {
    let rect = placement.rect;
    if rect.width < 110 || rect.height < 90 {
        return;
    }
    let theme = visual::palette(BACKGROUND);
    let border = if focused {
        theme.focused_border
    } else {
        theme.border
    };
    draw_rounded_rect(rect.x, rect.y, rect.width, rect.height, 17, border);
    draw_glass_rect(
        rect.x + 2,
        rect.y + 2,
        rect.width.saturating_sub(4),
        rect.height.saturating_sub(4),
        15,
        theme.glass,
        218,
    );
    draw_glass_rect(
        rect.x + 4,
        rect.y + 4,
        rect.width.saturating_sub(8),
        34,
        12,
        theme.title_glass,
        226,
    );
    let title = match placement.id {
        wm::TERMINAL => b"TERMINAL  /  OPSH" as &[u8],
        wm::SYSTEM => b"SYSTEM MONITOR",
        _ => b"BLEUU CONTROL",
    };
    draw_text(rect.x + 18, rect.y + 17, title, theme.text);
    let controls_x = rect.x + rect.width.saturating_sub(52);
    draw_rounded_rect(controls_x, rect.y + 16, 10, 10, 5, theme.accent_secondary);
    draw_rounded_rect(controls_x + 21, rect.y + 16, 10, 10, 5, 0xf38ba8);

    match placement.id {
        wm::TERMINAL => {
            draw_glass_rect(
                rect.x + 3,
                rect.y + 32,
                rect.width.saturating_sub(6),
                rect.height.saturating_sub(35),
                13,
                theme.surface,
                192,
            );
        }
        wm::SYSTEM => draw_system_panel(rect, theme),
        wm::BLEUU => draw_bleuu_panel(rect, theme),
        _ => {}
    }
}

unsafe fn draw_system_panel(rect: Rect, theme: visual::Palette) {
    let x = rect.x + 20;
    let mut y = rect.y + 58;
    draw_text(x, y, b"SYSTEM", theme.accent);
    y += 30;
    draw_text(x, y, b"x86_64  /  RUST NO_STD", theme.text);
    y += 27;
    draw_text(x, y, b"PIT TIMER   100 HZ", theme.muted_text);
    y += 27;
    draw_text(x, y, b"KEYBOARD    IRQ1", theme.muted_text);
    y += 27;
    draw_text(x, y, b"MOUSE       IRQ12", theme.muted_text);
    y += 27;
    draw_text(x, y, b"VBE SOFTWARE RENDERER", theme.muted_text);
    y += 27;
    draw_text(x, y, b"TILE WINDOWS / 3 WORKSPACES", theme.muted_text);
}

unsafe fn draw_bleuu_panel(rect: Rect, theme: visual::Palette) {
    let x = rect.x + 20;
    let y = rect.y + 58;
    draw_text(x, y, b"BLEUU DESKTOP", theme.accent);
    draw_text(x, y + 30, b"PACKAGE PROFILE: INSTALLED", theme.text);
    draw_text(x, y + 57, b"SHELL / SYSTEM / SERVICES", theme.muted_text);
    let button_y = y + 103;
    let button_width = rect.width.saturating_sub(28).min(188);
    draw_rounded_rect(x, button_y, button_width, 34, 11, theme.accent);
    draw_text(
        x + 13,
        button_y + 13,
        b"VIEW SERVICE STATUS",
        theme.canvas_top,
    );
    draw_text(
        x,
        button_y + 55,
        b"SUPER+1..3  SWITCH WORKSPACE",
        theme.muted_text,
    );
    draw_text(
        x,
        button_y + 81,
        b"SUPER+SHIFT+1..3  MOVE WINDOW",
        theme.muted_text,
    );
}

unsafe fn draw_empty_workspace() {
    let x = 34;
    let y = 90;
    let theme = visual::palette(BACKGROUND);
    draw_glass_rect(x, y, WIDTH.saturating_sub(68), 166, 18, theme.glass, 224);
    draw_text(x + 24, y + 30, b"EMPTY WORKSPACE", theme.accent);
    draw_text(
        x + 24,
        y + 70,
        b"Use the taskbar to open Terminal, System, or Bleuu.",
        theme.text,
    );
    draw_text(
        x + 24,
        y + 98,
        b"SUPER+1..3 switches workspaces.",
        theme.muted_text,
    );
}

unsafe fn draw_button(x: usize, y: usize, width: usize, height: usize, label: &[u8], color: u32) {
    let theme = visual::palette(BACKGROUND);
    draw_glass_rect(x, y, width, height, 10, color, 220);
    let text_color = if color == theme.accent {
        theme.canvas_top
    } else {
        theme.text
    };
    draw_text(x + 10, y + 11, label, text_color);
}

unsafe fn draw_terminal_character(
    x: usize,
    y: usize,
    character: u8,
    foreground: u32,
    background: u32,
) {
    let cell_width = if DESKTOP_MODE {
        CELL_WIDTH
    } else {
        TEXT_CELL_WIDTH
    };
    fill_rect(x, y, cell_width, TEXT_CELL_HEIGHT, background);
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
            if glyph_pixel(*pattern, column, BOLD_FONT) {
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
                if glyph_pixel(*pattern, column, BOLD_FONT) {
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

/// Paint a rounded rectangle without touching pixels outside its corner mask.
unsafe fn draw_rounded_rect(
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    radius: usize,
    color: u32,
) {
    let x_end = x.saturating_add(width).min(WIDTH);
    let y_end = y.saturating_add(height).min(HEIGHT);
    for row in y.min(HEIGHT)..y_end {
        for column in x.min(WIDTH)..x_end {
            if visual::inside_rounded_rect(column - x, row - y, width, height, radius) {
                set_pixel(column, row, color);
            }
        }
    }
}

/// Alpha-composite a tinted glass surface over the current framebuffer.
/// A fine highlight on the top edge and sparse low-contrast grain suggest
/// frosted material while keeping the implementation single-buffered.
unsafe fn draw_glass_rect(
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    radius: usize,
    tint: u32,
    opacity: u8,
) {
    let x_end = x.saturating_add(width).min(WIDTH);
    let y_end = y.saturating_add(height).min(HEIGHT);
    for row in y.min(HEIGHT)..y_end {
        for column in x.min(WIDTH)..x_end {
            if !visual::inside_rounded_rect(column - x, row - y, width, height, radius) {
                continue;
            }
            let underlay = read_pixel_rgb(column, row);
            let mut color = visual::alpha_blend(tint, underlay, opacity);
            if row - y <= 1 {
                color = visual::alpha_blend(0xc0caf5, color, 24);
            } else if (column.wrapping_mul(13) ^ row.wrapping_mul(7)) & 63 == 0 {
                color = visual::alpha_blend(0xdde4ff, color, 7);
            }
            set_pixel(column, row, color);
        }
    }
}

fn glyph_pixel(pattern: u8, column: usize, bold: bool) -> bool {
    let shift = 4 - column;
    let current = pattern & (1 << shift) != 0;
    let previous = bold && pattern & (1 << (shift + 1)) != 0;
    current || previous
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

unsafe fn read_pixel_rgb(x: usize, y: usize) -> u32 {
    if x >= WIDTH || y >= HEIGHT {
        return 0;
    }
    let address = PIXELS.add(y * PITCH_BYTES + x * BYTES_PER_PIXEL);
    let mut packed = 0u32;
    for byte in 0..BYTES_PER_PIXEL {
        packed |= (address.add(byte).read_volatile() as u32) << (byte * 8);
    }
    let red = (packed >> RED_SHIFT) & 0xff;
    let green = (packed >> GREEN_SHIFT) & 0xff;
    let blue = (packed >> BLUE_SHIFT) & 0xff;
    (red << 16) | (green << 8) | blue
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
    match character {
        // Lowercase uses its own shapes rather than being folded to capitals.
        // This makes pairs such as t/T and i/I visibly different in the
        // framebuffer console while keeping the existing 8x16 text cell.
        b'a' => [0, 0x0e, 0x01, 0x0f, 0x11, 0x13, 0x0d],
        b'b' => [0x10, 0x10, 0x1e, 0x11, 0x11, 0x11, 0x1e],
        b'c' => [0, 0, 0x0f, 0x10, 0x10, 0x10, 0x0f],
        b'd' => [0x01, 0x01, 0x0f, 0x11, 0x11, 0x13, 0x0d],
        b'e' => [0, 0, 0x0e, 0x11, 0x1f, 0x10, 0x0e],
        b'f' => [0x06, 0x08, 0x1e, 0x08, 0x1c, 0x08, 0x08],
        b'g' => [0, 0, 0x0f, 0x11, 0x0f, 0x01, 0x0e],
        b'h' => [0x10, 0x10, 0x1e, 0x11, 0x11, 0x11, 0x11],
        b'i' => [0x04, 0, 0x0c, 0x04, 0x04, 0x04, 0x0e],
        b'j' => [0x02, 0, 0x06, 0x02, 0x02, 0x12, 0x0c],
        b'k' => [0x10, 0x10, 0x12, 0x14, 0x18, 0x14, 0x12],
        b'l' => [0x0c, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0e],
        b'm' => [0, 0, 0x1a, 0x15, 0x15, 0x15, 0x15],
        b'n' => [0, 0, 0x1e, 0x11, 0x11, 0x11, 0x11],
        b'o' => [0, 0, 0x0e, 0x11, 0x11, 0x11, 0x0e],
        b'p' => [0, 0, 0x1e, 0x11, 0x1e, 0x10, 0x10],
        b'q' => [0, 0, 0x0f, 0x11, 0x0f, 0x01, 0x01],
        b'r' => [0, 0, 0x16, 0x19, 0x10, 0x10, 0x10],
        b's' => [0, 0, 0x0f, 0x10, 0x0e, 0x01, 0x1e],
        b't' => [0x08, 0x08, 0x1e, 0x08, 0x08, 0x09, 0x06],
        b'u' => [0, 0, 0x11, 0x11, 0x11, 0x13, 0x0d],
        b'v' => [0, 0, 0x11, 0x11, 0x11, 0x0a, 0x04],
        b'w' => [0, 0, 0x11, 0x11, 0x15, 0x15, 0x0a],
        b'x' => [0, 0, 0x11, 0x0a, 0x04, 0x0a, 0x11],
        b'y' => [0, 0, 0x11, 0x11, 0x0f, 0x01, 0x0e],
        b'z' => [0, 0, 0x1f, 0x02, 0x04, 0x08, 0x1f],
        _ => glyph_rows_upper(character),
    }
}

fn glyph_rows_upper(character: u8) -> [u8; 7] {
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
