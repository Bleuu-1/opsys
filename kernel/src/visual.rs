//! Small software-rendering primitives and desktop color palettes.
//!
//! Colors use 0xRRGGBB. The glass effect is a tinted, partially transparent
//! overlay with a fine highlight; it deliberately needs no allocator or
//! off-screen framebuffer.

#[derive(Clone, Copy)]
pub struct Palette {
    pub canvas_top: u32,
    pub canvas_bottom: u32,
    pub surface: u32,
    pub glass: u32,
    pub title_glass: u32,
    pub border: u32,
    pub focused_border: u32,
    pub accent: u32,
    pub accent_secondary: u32,
    pub text: u32,
    pub muted_text: u32,
    pub taskbar_glass: u32,
}

/// Blend `foreground` over `background` using an 8-bit opacity.
pub fn alpha_blend(foreground: u32, background: u32, alpha: u8) -> u32 {
    let a = alpha as u32;
    let inverse = 255 - a;
    let red = ((((foreground >> 16) & 0xff) * a + ((background >> 16) & 0xff) * inverse + 127)
        / 255)
        & 0xff;
    let green = ((((foreground >> 8) & 0xff) * a + ((background >> 8) & 0xff) * inverse + 127)
        / 255)
        & 0xff;
    let blue = ((foreground & 0xff) * a + (background & 0xff) * inverse + 127) / 255;
    (red << 16) | (green << 8) | blue
}

/// Integer circle test used by rounded rectangle clipping and hit masks.
pub fn inside_rounded_rect(x: usize, y: usize, width: usize, height: usize, radius: usize) -> bool {
    if x >= width || y >= height {
        return false;
    }
    let radius = radius.min(width / 2).min(height / 2);
    if radius == 0 {
        return true;
    }
    let center_x = if x < radius {
        radius - 1
    } else if x >= width.saturating_sub(radius) {
        width.saturating_sub(radius)
    } else {
        x
    };
    let center_y = if y < radius {
        radius - 1
    } else if y >= height.saturating_sub(radius) {
        height.saturating_sub(radius)
    } else {
        y
    };
    let dx = x as i64 - center_x as i64;
    let dy = y as i64 - center_y as i64;
    let radius = radius as i64;
    dx * dx + dy * dy <= radius * radius
}

/// Dark palettes chosen to retain readable contrast on low-resolution VBE.
/// The existing six background names remain stable for saved user settings.
pub const fn palette(preset: u8) -> Palette {
    match preset {
        // ocean: Catppuccin-inspired blue teal
        1 => Palette {
            canvas_top: 0x111b2b,
            canvas_bottom: 0x101f2a,
            surface: 0x172335,
            glass: 0x1b2638,
            title_glass: 0x283b55,
            border: 0x35445b,
            focused_border: 0x89b4fa,
            accent: 0x7aa2f7,
            accent_secondary: 0x94e2d5,
            text: 0xdce7f5,
            muted_text: 0x9aaac0,
            taskbar_glass: 0x172234,
        },
        // forest
        2 => Palette {
            canvas_top: 0x111c1b,
            canvas_bottom: 0x142320,
            surface: 0x192924,
            glass: 0x1d302b,
            title_glass: 0x29453b,
            border: 0x3b5548,
            focused_border: 0xa6e3a1,
            accent: 0x89dceb,
            accent_secondary: 0xa6e3a1,
            text: 0xe0eee5,
            muted_text: 0x9cb5a5,
            taskbar_glass: 0x182722,
        },
        // sunset
        3 => Palette {
            canvas_top: 0x211927,
            canvas_bottom: 0x2a1b28,
            surface: 0x302331,
            glass: 0x392938,
            title_glass: 0x50344c,
            border: 0x624357,
            focused_border: 0xf5c2e7,
            accent: 0xf5c2e7,
            accent_secondary: 0xfab387,
            text: 0xf6e9ef,
            muted_text: 0xc5a8bb,
            taskbar_glass: 0x291d29,
        },
        // slate
        4 => Palette {
            canvas_top: 0x171b24,
            canvas_bottom: 0x1b202b,
            surface: 0x222833,
            glass: 0x252c38,
            title_glass: 0x343d4c,
            border: 0x414b5c,
            focused_border: 0xc0caf5,
            accent: 0x7aa2f7,
            accent_secondary: 0xbb9af7,
            text: 0xe1e5f2,
            muted_text: 0xa1a9bb,
            taskbar_glass: 0x1e242e,
        },
        // ember
        5 => Palette {
            canvas_top: 0x211a18,
            canvas_bottom: 0x271d19,
            surface: 0x30241f,
            glass: 0x392a23,
            title_glass: 0x50372b,
            border: 0x664735,
            focused_border: 0xfab387,
            accent: 0xfab387,
            accent_secondary: 0xf9e2af,
            text: 0xf5e9dc,
            muted_text: 0xc5ad96,
            taskbar_glass: 0x291f1a,
        },
        // midnight: Tokyo Night / Catppuccin Mocha base
        _ => Palette {
            canvas_top: 0x1a1b26,
            canvas_bottom: 0x161821,
            surface: 0x1e2030,
            glass: 0x222536,
            title_glass: 0x2b3045,
            border: 0x39405b,
            focused_border: 0x7aa2f7,
            accent: 0x7aa2f7,
            accent_secondary: 0xbb9af7,
            text: 0xc0caf5,
            muted_text: 0xa9b1d6,
            taskbar_glass: 0x1c1e2b,
        },
    }
}
