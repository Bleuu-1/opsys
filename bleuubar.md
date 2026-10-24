# Bleuubar architecture

Bleuubar is a freestanding `no_std` Rust ELF embedded into the kernel build and loaded into the existing PID 3 Ring-3 slot by `bleuu start bleuubar`. It has no allocator or libc. Its source is split into the CSS parser (`user/bleuubar/src/css.rs`), bar rasterizer (`render.rs`), syscall wrapper (`sys.rs`), and app loop (`main.rs`).

## CSS subset

The parser accepts a fixed set of selectors and properties. It stores only four `Style` values and the window geometry in a `Theme`, then applies declarations directly without allocating strings or tokens.

```css
window {
    background-color: #101827;
    color: #f3f6fc;
    border-radius: 12px;
    margin-top: 8px;
    margin-left: 12px;
    margin-right: 12px;
    height: 44px;
}
#clock { background-color: #1d2a40; color: #f3f6fc; border-radius: 8px; }
#workspaces button { background-color: #1b293d; color: #b8c7db; border-radius: 7px; }
#workspaces button.active { background-color: #287db5; color: #fff; border-radius: 7px; }
```

`#RGB` and `#RRGGBB` are parsed to `0x00RRGGBB`. Pixel values accept decimal numbers with an optional `px` suffix and are bounded before rendering. Unknown selectors are skipped. Invalid or unsupported declarations increment the parse report and leave the current default unchanged. This is intentionally not a browser CSS implementation: no cascade, variables, fonts, selectors beyond the four above, or general layout engine.

## Render loop

The app computes a bounded top-bar rectangle from the VBE size and CSS margins. It draws workspace pills, a BLEUU label, free-memory in MiB, and an `UP HH:MM` elapsed-time clock into a static 1920x64 ARGB back buffer. Rounded corners are represented by alpha-zero pixels. The loop refreshes at 10 Hz to restore the bar after the Ring-0 desktop redraws and to keep workspace and memory values current.

The kernel validates user pages and the surface dimensions before accepting the copy. It only accepts a bar up to 1920x64 pixels in the top 128 screen rows while the optional desktop is active. Transparent pixels preserve the desktop below. The user program never gets the framebuffer address.

## `int 0x31` calls

The syscall number is in `RAX`; results return in `RAX`. Arguments use the saved `RDI`, `RSI`, `RDX`, `RCX`, and `R8` registers.

| Number | Call | Arguments |
| ---: | --- | --- |
| 0 | yield | none |
| 1 | write | `RDI=bytes`, `RSI=length` (up to 512) |
| 2 | exit | none |
| 3 | display submit | `RDI=ARGB pixels`, `RSI=width`, `RDX=height`, `RCX=x`, `R8=y` |
| 4 | system info | `RDI=pointer to a fixed SystemInfo record` |
| 5 | VFS read | `RDI=path`, `RSI=path length`, `RDX=output`, `RCX=capacity` (up to 2 KiB) |

The kernel checks every userspace range against the current task's user page tables. File reads pass through the VFS, so HOME ownership checks still apply. Because syscall 5 can block on disk I/O, the kernel briefly enables interrupts during the VFS operation so the preemptive scheduler can resume a task that owns the disk lock.

## Per-user stylesheet

The logical path is `/home/<uid>/.config/opsys/bleuubar.css`. The current HOME filesystem only stores 23-byte paths, so the VFS aliases the two supported accounts to `bar0.css` and `bar1000.css`, both ordinary owner-tagged HOME entries. Reading and writing the logical path goes through the same UID checks as any other HOME file. If the entry is absent or invalid, Bleuubar keeps its embedded defaults.

Start the bar after enabling the desktop with `bleuu start bleuubar`; stop it with `bleuu stop bleuubar`. PID 3 is shared with the `hello` demo, so only one Ring-3 app can run at once. The displayed clock is elapsed uptime from the PIT, not civil time. This syscall compositor bridge is experimental and is neither Wayland nor hardware acceleration.
