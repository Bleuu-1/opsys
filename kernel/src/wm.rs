//! Small keyboard-driven tiling window manager for the VBE desktop.
//!
//! This manages kernel-rendered app panels. It is not a Ring-3 compositor.

pub const TERMINAL: usize = 0;
pub const SYSTEM: usize = 1;
pub const BLEUU: usize = 2;
pub const WINDOW_COUNT: usize = 3;
pub const WORKSPACE_COUNT: u8 = 3;
const FLOATING_MARGIN_X: usize = 24;
const FLOATING_MARGIN_TOP: usize = 64;
const FLOATING_MARGIN_BOTTOM: usize = 148;
const WINDOW_GAP: usize = 14;

#[derive(Clone, Copy)]
pub struct Rect {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

#[derive(Clone, Copy)]
pub struct Placement {
    pub id: usize,
    pub rect: Rect,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Action {
    None,
    Redraw,
    BleuuStatus,
}

#[derive(Clone, Copy)]
struct Window {
    open: bool,
    workspace: u8,
}

const CLOSED: Window = Window {
    open: false,
    workspace: 0,
};

static mut WINDOWS: [Window; WINDOW_COUNT] = [CLOSED; WINDOW_COUNT];
static mut CURRENT_WORKSPACE: u8 = 0;
static mut FOCUSED: usize = TERMINAL;
static mut MAXIMIZED: bool = false;
static mut LAST_BUTTONS: u8 = 0;

pub fn initialize() {
    unsafe {
        WINDOWS = [
            Window {
                open: true,
                workspace: 0,
            },
            Window {
                open: true,
                workspace: 0,
            },
            Window {
                open: true,
                workspace: 0,
            },
        ];
        CURRENT_WORKSPACE = 0;
        FOCUSED = TERMINAL;
        MAXIMIZED = false;
        LAST_BUTTONS = 0;
    }
}

pub fn focused() -> usize {
    unsafe { FOCUSED }
}

pub fn focused_name() -> &'static [u8] {
    match focused() {
        TERMINAL => b"Terminal",
        SYSTEM => b"System",
        BLEUU => b"Bleuu",
        _ => b"unknown",
    }
}

pub fn current_workspace() -> u8 {
    unsafe { CURRENT_WORKSPACE }
}

pub fn is_open(id: usize) -> bool {
    if id >= WINDOW_COUNT {
        return false;
    }
    unsafe { WINDOWS[id].open && WINDOWS[id].workspace == CURRENT_WORKSPACE }
}

pub fn is_focused(id: usize) -> bool {
    unsafe { FOCUSED == id }
}

pub fn focused_window_visible() -> bool {
    unsafe { WINDOWS[FOCUSED].open && WINDOWS[FOCUSED].workspace == CURRENT_WORKSPACE }
}

pub fn open_terminal() {
    open_window(TERMINAL);
}

/// Handle OpSYS Super-key bindings. Set `shift` to move the focused window
/// into a workspace instead of switching to it.
pub fn handle_key(scancode: u8, super_down: bool, shift: bool) -> bool {
    if !super_down {
        return false;
    }
    unsafe {
        match scancode {
            // Super+1..3 switches workspaces; Super+Shift+1..3 moves a window.
            0x02..=0x04 => {
                let workspace = scancode - 0x02;
                if shift {
                    if WINDOWS[FOCUSED].open && WINDOWS[FOCUSED].workspace == CURRENT_WORKSPACE {
                        WINDOWS[FOCUSED].workspace = workspace;
                        MAXIMIZED = false;
                        focus_any_visible();
                    }
                } else {
                    CURRENT_WORKSPACE = workspace;
                    MAXIMIZED = false;
                    focus_any_visible();
                }
            }
            // Super+J/K or Super+Tab cycles focus through tiled clients.
            0x24 | 0x0f => focus_next(true),
            0x25 => focus_next(false),
            // Super+M toggles a focused window between tiled and maximized.
            0x32 => MAXIMIZED = !MAXIMIZED,
            // Super+Q closes the focused panel. Taskbar buttons reopen it.
            0x10 => {
                if WINDOWS[FOCUSED].open && WINDOWS[FOCUSED].workspace == CURRENT_WORKSPACE {
                    WINDOWS[FOCUSED].open = false;
                    MAXIMIZED = false;
                    focus_any_visible();
                }
            }
            // Super+Enter always returns to the terminal panel.
            0x1c => open_window(TERMINAL),
            _ => return false,
        }
    }
    true
}

/// Process one left-button mouse sample and update focus or panel state.
pub fn pointer_event(x: usize, y: usize, buttons: u8, width: usize, height: usize) -> Action {
    let clicked;
    unsafe {
        clicked = buttons & 1 != 0 && LAST_BUTTONS & 1 == 0;
        LAST_BUTTONS = buttons;
    }
    if !clicked {
        return Action::None;
    }

    let taskbar_y = height.saturating_sub(56);
    if y >= taskbar_y + 5 && y < taskbar_y + 37 {
        if (72..138).contains(&x) {
            open_window(TERMINAL);
            return Action::Redraw;
        }
        if (142..197).contains(&x) {
            open_window(SYSTEM);
            return Action::Redraw;
        }
        if (202..267).contains(&x) {
            open_window(BLEUU);
            return Action::Redraw;
        }
        if (275..341).contains(&x) {
            open_window(BLEUU);
            open_window(TERMINAL);
            return Action::BleuuStatus;
        }
        if (354..382).contains(&x) {
            switch_workspace(0);
            return Action::Redraw;
        }
        if (386..414).contains(&x) {
            switch_workspace(1);
            return Action::Redraw;
        }
        if (418..446).contains(&x) {
            switch_workspace(2);
            return Action::Redraw;
        }
    }

    let placements = layout(width, height);
    for placement in placements.into_iter().flatten() {
        let rect = placement.rect;
        if x < rect.x || y < rect.y || x >= rect.x + rect.width || y >= rect.y + rect.height {
            continue;
        }
        unsafe {
            FOCUSED = placement.id;
        }
        if y < rect.y + 40 {
            if x >= rect.x + rect.width.saturating_sub(28) {
                unsafe {
                    WINDOWS[placement.id].open = false;
                    MAXIMIZED = false;
                    focus_any_visible();
                }
            } else if x >= rect.x + rect.width.saturating_sub(58) {
                unsafe {
                    MAXIMIZED = !MAXIMIZED;
                }
            }
            return Action::Redraw;
        }
        if placement.id == BLEUU {
            let status_top = rect.y + 161;
            if y >= status_top && y < status_top + 36 && x >= rect.x + 20 && x < rect.x + 210 {
                open_window(TERMINAL);
                return Action::BleuuStatus;
            }
        }
        return Action::Redraw;
    }
    Action::None
}

pub fn layout(width: usize, height: usize) -> [Option<Placement>; WINDOW_COUNT] {
    let mut result = [None; WINDOW_COUNT];
    let mut visible = [0usize; WINDOW_COUNT];
    let mut count = 0usize;
    unsafe {
        for id in 0..WINDOW_COUNT {
            if WINDOWS[id].open && WINDOWS[id].workspace == CURRENT_WORKSPACE {
                visible[count] = id;
                count += 1;
            }
        }
    }
    if count == 0 {
        return result;
    }

    let area = Rect {
        x: FLOATING_MARGIN_X,
        y: FLOATING_MARGIN_TOP,
        width: width.saturating_sub(FLOATING_MARGIN_X * 2),
        height: height.saturating_sub(FLOATING_MARGIN_TOP + FLOATING_MARGIN_BOTTOM),
    };
    unsafe {
        let focused = FOCUSED;
        if MAXIMIZED && visible[..count].contains(&focused) {
            result[focused] = Some(Placement {
                id: focused,
                rect: area,
            });
            return result;
        }
    }

    if count == 1 {
        let id = visible[0];
        result[id] = Some(Placement { id, rect: area });
    } else if count == 2 {
        let master = if visible[..count].contains(&TERMINAL) {
            TERMINAL
        } else {
            visible[0]
        };
        let stack = if visible[0] == master {
            visible[1]
        } else {
            visible[0]
        };
        let usable_width = area.width.saturating_sub(WINDOW_GAP);
        let master_width = usable_width.saturating_mul(66) / 100;
        result[master] = Some(Placement {
            id: master,
            rect: Rect {
                width: master_width,
                ..area
            },
        });
        result[stack] = Some(Placement {
            id: stack,
            rect: Rect {
                x: area.x + master_width + WINDOW_GAP,
                width: area.width.saturating_sub(master_width + WINDOW_GAP),
                ..area
            },
        });
    } else {
        let master = if visible[..count].contains(&TERMINAL) {
            TERMINAL
        } else {
            visible[0]
        };
        let mut stack = [0usize; 2];
        let mut stack_index = 0;
        for id in visible[..count].iter().copied() {
            if id != master {
                stack[stack_index] = id;
                stack_index += 1;
            }
        }
        let usable_width = area.width.saturating_sub(WINDOW_GAP);
        let master_width = usable_width.saturating_mul(66) / 100;
        let stack_x = area.x + master_width + WINDOW_GAP;
        let stack_width = area.width.saturating_sub(master_width + WINDOW_GAP);
        let stack_height = area.height.saturating_sub(WINDOW_GAP) / 2;
        result[master] = Some(Placement {
            id: master,
            rect: Rect {
                width: master_width,
                ..area
            },
        });
        result[stack[0]] = Some(Placement {
            id: stack[0],
            rect: Rect {
                x: stack_x,
                width: stack_width,
                height: stack_height,
                ..area
            },
        });
        result[stack[1]] = Some(Placement {
            id: stack[1],
            rect: Rect {
                x: stack_x,
                y: area.y + stack_height + WINDOW_GAP,
                width: stack_width,
                height: area.height.saturating_sub(stack_height + WINDOW_GAP),
            },
        });
    }
    result
}

fn open_window(id: usize) {
    if id >= WINDOW_COUNT {
        return;
    }
    unsafe {
        WINDOWS[id].open = true;
        WINDOWS[id].workspace = CURRENT_WORKSPACE;
        FOCUSED = id;
        MAXIMIZED = false;
    }
}

fn switch_workspace(workspace: u8) {
    if workspace >= WORKSPACE_COUNT {
        return;
    }
    unsafe {
        CURRENT_WORKSPACE = workspace;
        MAXIMIZED = false;
        focus_any_visible();
    }
}

unsafe fn focus_any_visible() {
    if WINDOWS[FOCUSED].open && WINDOWS[FOCUSED].workspace == CURRENT_WORKSPACE {
        return;
    }
    for id in 0..WINDOW_COUNT {
        if WINDOWS[id].open && WINDOWS[id].workspace == CURRENT_WORKSPACE {
            FOCUSED = id;
            return;
        }
    }
    FOCUSED = TERMINAL;
}

unsafe fn focus_next(forward: bool) {
    for step in 1..=WINDOW_COUNT {
        let id = if forward {
            (FOCUSED + step) % WINDOW_COUNT
        } else {
            (FOCUSED + WINDOW_COUNT - (step % WINDOW_COUNT)) % WINDOW_COUNT
        };
        if WINDOWS[id].open && WINDOWS[id].workspace == CURRENT_WORKSPACE {
            FOCUSED = id;
            MAXIMIZED = false;
            return;
        }
    }
}
