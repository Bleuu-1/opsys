# OpSYS

**OpSYS 0.15.0** is an experimental 64-bit x86 hobby operating system built with a custom NASM BIOS bootloader and a Rust `no_std` kernel. It boots to a text-first console, creates a local account on first boot, and can enable its built-in software desktop through Bleuu. It is a learning project for operating-system development, not a replacement for Linux or an OS for everyday hardware.

```text
██████╗ ██████╗ ███████╗██╗   ██╗███████╗
██╔═══██╗██╔══██╗██╔════╝╚██╗ ██╔╝██╔════╝
██║   ██║██████╔╝███████╗ ╚████╔╝ ███████╗
██║   ██║██╔═══╝ ╚════██║  ╚██╔╝  ╚════██║
╚██████╔╝██║     ███████║   ██║   ███████║
 ╚═════╝ ╚═╝     ╚══════╝   ╚═╝   ╚══════╝
```

## Features

- BIOS boot from 16-bit real mode through protected mode into 64-bit long mode.
- E820 memory-map handoff, four-level paging, physical frame allocator, and kernel heap.
- Rust kernel with CPU exception diagnostics, IDT, PIC, PIT timer, keyboard queue, and PS/2 mouse IRQ12.
- Text-first shell drawn to VGA text memory and, when available, the VBE linear framebuffer.
- Optional mouse-driven desktop with rounded floating-gap windows, translucent tinted-glass panels, Tokyo Night and Catppuccin-inspired palettes, Terminal, System Monitor, and Bleuu panels; three workspaces; focus, maximize, close, and reopen controls; Super-key shortcuts; and six saved background palettes. A per-user literal-only Lua table config stores shell and desktop preferences. This is a Ring-0 software renderer, not a hardware-accelerated GPU driver or a Ring-3 Wayland compositor.
- Round-robin kernel tasks and a ring-3 ELF demo with `write`, `yield`, and `exit` system calls.
- ATA PIO storage, a bounds-checked four-entry MBR parser, and a VFS with persistent HOME files and directories.
- Tiny HOME filesystem with two checksummed metadata copies, two data banks per file, UID ownership, and root override.
- Per-UID shell color and font preferences stored as versioned, checksummed HOME records and restored at login.
- First-boot username, user password, and root password setup, followed by login. Passwords are stored as salted PBKDF2-HMAC-SHA256 digests.
- **Bleuu**, an early package and service manager for the built-in `hello` demo and optional desktop profile. The shell keeps eight recent non-password commands, supports Up/Down recall, lists them with `history`, and completes built-ins, settings, and HOME paths with Tab.
- **Bleuubar**, a native `no_std` Ring-3 status bar with an allocation-free CSS subset, floating margins, rounded corners, workspace buttons, free-memory and uptime indicators, and a bounded `display_submit` syscall.

## Build and run

On Arch Linux install the build tools and QEMU:

```sh
sudo pacman -S nasm qemu-system-x86 make binutils
rustup target add x86_64-unknown-none
make
make run
```

Create and boot a BIOS-bootable ISO:

```sh
make release-assets
make run-iso
```

This produces `build/opsys-0.15.0.iso` and a fresh blank `build/opsys-0.15.0-data.img`. The ISO uses El Torito 1.44 MiB floppy emulation for the custom BIOS bootloader and includes a copy of the blank IDE disk image. Attach the separate data image as an IDE disk in QEMU; first boot creates its partitions and account. Back up the data image after setup. The root `VERSION` file and shell's `version` command identify the build.

Pushing a tag that matches `v` plus the `VERSION` value (for example, `v0.15.0`) runs the GitHub release workflow. It builds a fresh ISO and blank IDE data disk, then attaches both files to that version's GitHub Release. Each version needs a new version value and matching tag.

`make` creates `build/opsys.img` for booting and `build/opsys-data.img` as an 8 MiB IDE data disk. On first boot, OpSYS writes its custom partition table only if the disk's first sector is completely blank. It creates a `SYSTEM` area and a persistent `HOME` area, then asks you to create a username, user password, and root password. After setup, log in with the user account. Kernel output is also mirrored to COM1 in the terminal.

`make clean` removes generated boot and kernel build files while preserving `build/opsys-data.img`, including account information, installed package state, and HOME data. `make clean-data` explicitly deletes that image and all its contents.

The HOME filesystem supports up to 10 entries, 2 KiB per file, paths up to 23 characters, and nested directories. Use `ls [dir]`, `mkdir <dir>`, `touch <file>`, `write <file> <text>`, and `cat <file>`. Built-in `README.TXT`, `HELLO.ELF`, and `NOTE` names are reserved at the root. Entries record the creating UID; users can access their own entries, while root can access all entries. The old shared metadata format is upgraded in place and its entries are assigned to the regular account. `NOTE` is a legacy shared record; regular users can read it, but only root can change it. There are no GID/mode bits, delete/rename, or journal; keep backups of the QEMU data image.

## First login and shell commands

At first boot, enter a username (letters, numbers, `_`, and `-`; `root` is reserved), then a user password and root password (8 to 63 characters). Password input is masked. Usernames are case-insensitive at login. Enter `root` at the login prompt to authenticate with the root password, or use `su` from a user shell.

During a login session, Up/Down cycles through the last eight shell commands and `history` lists them. Tab completes built-in commands, settings, and visible HOME paths. Password prompts are excluded, and history is cleared on logout.

## Bleuubar status bar

Stop the demo if it is using the single user-process slot, install the optional desktop as root, then start Bleuubar from either account:

```text
bleuu stop hello
su
bleuu install desktop
bleuu start desktop
bleuu start bleuubar
```

Bleuubar draws three workspace buttons, free memory, and an uptime clock. Time is based on PIT ticks; wall-clock/RTC support is not available yet. The panel uses the single Ring-3 process slot (PID 3), so it cannot run alongside the `hello` demo.

The stylesheet is read through the VFS from `/home/<uid>/.config/opsys/bleuubar.css`. The tiny HOME filesystem limits stored names to 23 bytes, so the VFS maps this logical path to the owner-protected root entry `bar0.css` or `bar1000.css`. Create or replace it with the normal HOME file commands; for example, `write /home/1000/.config/opsys/bleuubar.css window{margin-top:16px;}` sets a short override. If no file exists, the embedded stylesheet is used.

The parser uses fixed storage and no allocator. It supports `window`, `#clock`, `#workspaces button`, and `#workspaces button.active`, with `background-color`, `color`, `border-radius`, `margin-top`, `margin-left`, `margin-right`, and `height`. Colors accept `#RGB` and `#RRGGBB`; dimensions accept values such as `12px`. Unknown selectors/declarations are skipped and invalid values leave defaults unchanged. The app renders into its own back buffer and asks the kernel to copy a clipped bar surface; it does not map the hardware framebuffer into Ring 3 or provide Wayland/GPU acceleration.

See [docs/bleuubar.md](docs/bleuubar.md) for the syscall ABI and component details.

The desktop's software glass, color math, rounded clipping, floating layout, and repository file list are described in [docs/desktop-visuals.md](docs/desktop-visuals.md).

The first account setup ends in a root session. After you log out and sign in as your user, use `su` before any Bleuu install/remove command. Try:

```text
help
history
whoami
partitions
opfetch
mem
paging
tasks
ls
cat README.TXT
disk
bleuu status
bleuu packages
su
bleuu install hello
bleuu start hello
ps
bleuu stop hello
```

Desktop enablement is root-only:

```text
su
bleuu install desktop
bleuu start desktop
window terminal
config set background ocean
bleuu stop desktop
```

The desktop code is bundled with this kernel. Bleuu's `desktop` package currently persists an enablement profile; it does not download a separate compositor. `terminal` or `window terminal` focuses the Terminal window; `bleuu stop desktop` returns to the text console. Click **STATUS** on the taskbar or the service status button in the Bleuu panel to show service status. The taskbar can open or reopen Terminal, System, and Bleuu panels. Super+1..3 switches workspaces, Super+Shift+1..3 moves the focused panel, Super+J/K or Super+Tab changes focus, Super+M maximizes, Super+Q closes the focused panel, and Super+Enter opens Terminal. Click a panel title to focus it; the title buttons maximize or close it. `config set background midnight|ocean|forest|sunset|slate|ember` saves the current user's desktop palette. `config lua` prints the generated Lua table stored in the user's HOME settings file. OpSYS parses only literal color, font, and background values; it does not execute general Lua code. `run` launches the embedded demo; `bleuu start hello` launches the installed copy from the IDE disk. Bleuu currently has a fixed catalog, without repositories, dependency resolution, package versions, or arbitrary package files.

Other commands include `id`, `wm`, `window terminal`, `config`, `config lua`, `config set color <name>`, `config set font <bold|regular>`, `config set background <name>`, `config reset`, `uptime`, `ticks`, `keyboard`, `mouse`, `ui`, `alloc`, `font`, `font bold`, `font regular`, `cat NOTE`, `write NOTE <text>`, `ls [dir]`, `mkdir <dir>`, `touch <file>`, `write <file> <text>`, `cat <file>`, `color <name>`, `clear`, `echo <text>`, `reboot`, and `halt`. Shell color, font, and background preferences are stored separately for root and the regular UID and restored after login. The VBE framebuffer console has OpSYS Pixel regular and bold bitmap weights, with distinct lowercase glyphs. VGA text-only mode uses its hardware font. `NOTE` remains stored in `HOME` sector 0; an older note at the original disk sector is copied forward on first read without erasing the old sector.

## Storage and reinstall behavior

On a blank QEMU data disk, first boot writes two OpSYS-specific MBR entries: `SYSTEM` at LBA 2048–4095 for account and Bleuu metadata, and `HOME` from LBA 4096 to the end of the disk. The parser reads all four MBR entries and validates bounds and overlap; account, package, desktop-profile, and NOTE I/O use partition-relative offsets. Existing nonblank disks are not repartitioned. HOME keeps the legacy NOTE record at relative sector 0. The tiny filesystem stores alternating metadata snapshots in sectors 1 and 2 and fixed file banks beginning at sector 16.

The bootable OS image and the QEMU data image are separate. Replacing or rebuilding `opsys.img` leaves the data image intact; `make clean` also preserves it. Atomic A/B deployments, rollback, and a 30-second reinstall tool are not implemented yet.

## Boot details

Stage 1 loads stage 2. Stage 2 gathers the BIOS memory map, asks VBE for a linear framebuffer, prepares the GDT and initial page tables, enters 64-bit mode, loads the kernel, and jumps to its entry point. The kernel initializes memory, heap, interrupts, tasks, text console, and shell. The desktop is activated only when requested. Make assembles and links the separate x86_64 `HELLO.ELF` demo and embeds it in the kernel.

The floppy layout is stage 1 in sector 0, stage 2 in sectors 1–4, and the kernel from LBA 5. The bootloader loads the kernel at physical `0x10000`, identity-maps the first GiB, and also maps it at `0xffffffff80000000`. BootInfo version 2 passes the framebuffer metadata into the kernel.

## Project layout

```text
boot/          NASM BIOS stages
kernel/        Rust no_std kernel and target configuration
user/          Ring-3 demo plus the native Rust Bleuubar app and CSS parser
linker.ld      Kernel linker script
Makefile       Build, image creation, and QEMU run targets
build/         Generated images and binaries (ignored by Git)
```

## Limitations

This is an early hobby OS. The HOME filesystem is intentionally tiny: ten total file/directory entries, 2 KiB per file, and 23-byte paths. It records UID ownership and enforces it in kernel VFS operations, with root override, but has no deletion, rename, GID/mode bits, or journal. The account system currently supports one regular UID. Keep OpSYS on a disposable QEMU disk and back it up. Credentials are not attached to processes, so ownership checks do not provide isolation against arbitrary Ring-3 programs. Login is not a hardened security boundary: salts use the CPU timestamp counter, and there is no secure boot, cryptographic random generator, or password reset. There is no OCI runtime, Ring-3 Wayland compositor, JavaScript/CSS engine, general Lua runtime, userspace libc, networking, USB, SMP, or hardware driver framework. BleuuWM accepts a small literal-only Lua config format but does not embed a Lua interpreter. The current tiling desktop and VBE renderer run in Ring 0 and use software pixel routines rather than GPU acceleration. User pages lack per-segment ELF permissions and a user page fault currently halts the kernel. Bleuu is an early experiment rather than a Linux distribution package manager or a full `systemd`/`66`-style service system. See [docs/ARCHITECTURE_ROADMAP.md](docs/ARCHITECTURE_ROADMAP.md) for the remaining work and subsystem boundaries.
