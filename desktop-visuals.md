# OpSYS desktop visual layer

OpSYS 0.15.0 adds a small `no_std`, allocation-free style layer to the existing VBE software renderer. It keeps the current Ring-0 desktop and window manager; this work is a visual refresh, not a new Ring-3 compositor or a Wayland implementation.

## Source layout

```text
kernel/src/main.rs       Declares the visual module.
kernel/src/visual.rs     Tokyo Night / Catppuccin-inspired palettes,
                         8-bit alpha blending, and rounded-rectangle mask.
kernel/src/graphics.rs   VBE pixel read/write, gradient background, glass
                         surfaces, borders, glyphs, panels, and taskbar.
kernel/src/wm.rs         Floating outer margins, tiled placements, gaps,
                         focus, hit testing, and workspace behavior.
Makefile                 Includes visual.rs in kernel rebuild dependencies.
README.md                Feature summary and build instructions.
VERSION                  0.15.0.
```

`visual.rs` contains only color values and integer geometry math. The graphics module owns the framebuffer and applies the primitives; the window manager returns rectangles and handles pointer/key events. This keeps VBE memory access separate from layout policy.

## Rendering math

Colors are stored as `0xRRGGBB`. For each color channel, alpha compositing is:

```text
out = (foreground * alpha + background * (255 - alpha) + 127) / 255
```

`alpha` ranges from 0 (show the surface below) to 255 (fully opaque). The glass routine reads the current VBE pixel, blends a dark palette tint over it, adds a faint top-edge sheen, and adds very sparse low-contrast grain. It uses no heap or full-screen back buffer. This is a tinted frosted-glass approximation; it does not run a Gaussian blur kernel.

Rounded clipping uses an integer circle test. For pixels in a corner, the renderer selects that corner's circle center and keeps the pixel when:

```text
dx * dx + dy * dy <= radius * radius
```

Pixels outside the mask remain untouched, so the desktop gradient appears around each window and floating panel.

## Layout and colors

The tiled layout floats inside 24 px side margins, 64 px below the top status strip, and 148 px above the bottom edge. Adjacent tiles have a 14 px gap. The maximized state uses the same inset work area, keeping the rounded border visible. The panel positions and click targets are defined together so visual placement and mouse hit testing stay aligned.

The `midnight` preset uses Tokyo Night colors (`#1a1b26`, `#7aa2f7`, `#bb9af7`). The other saved presets keep their existing names and map to blue/teal, forest, plum, slate, and ember palettes. `config set background <name>` continues to select and persist them.

## Git commands

From a normal clone of `Bleuu-1/opsys`, review and commit these paths:

```sh
git status --short
git add VERSION Makefile README.md kernel/src/main.rs kernel/src/graphics.rs kernel/src/visual.rs kernel/src/wm.rs docs/desktop-visuals.md
git diff --cached
git commit -m "Refresh desktop with frosted glass visuals"
git push origin main
```

If your clone uses a branch other than `main`, replace the final branch name with the checked-out branch shown by `git branch --show-current`.
