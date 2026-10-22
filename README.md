# OpSYS

**OpSYS is an experimental 64-bit x86 hobby operating system built with a custom NASM BIOS bootloader and a Rust `no_std` kernel.** It boots to a text-first console, creates a local account on first boot, and can enable its built-in software desktop through Bleuu. It is a learning project for operating-system development, not a replacement for Linux or an OS for everyday hardware.

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
- Optional software-drawn desktop, mouse cursor, terminal window, and Bleuu taskbar button. This is not a hardware-accelerated GPU driver.
- Round-robin kernel tasks and a ring-3 ELF demo with `write`, `yield`, and `exit` system calls.
- Minimal ATA PIO storage, custom MBR layout, and a tiny RAM filesystem.
- First-boot username, user password, and root password setup, followed by login. Passwords are stored as salted PBKDF2-HMAC-SHA256 digests.
- **Bleuu**, an early package and service manager for the built-in `hello` demo and optional desktop profile.

## Build and run

On Arch Linux install the build tools and QEMU:

```sh
sudo pacman -S nasm qemu-system-x86 make binutils
rustup target add x86_64-unknown-none
make
make run
```

`make` creates `build/opsys.img` for booting and `build/opsys-data.img` as an 8 MiB IDE data disk. On first boot, OpSYS writes its custom partition table only if the disk's first sector is completely blank. It creates a `SYSTEM` area and a persistent `HOME` area, then asks you to create a username, user password, and root password. After setup, log in with the user account. Kernel output is also mirrored to COM1 in the terminal.

`make clean` removes generated boot and kernel build files while preserving `build/opsys-data.img`, including account information, installed package state, and HOME data. `make clean-data` explicitly deletes that image and all its contents.

## First login and shell commands

At first boot, enter a username (letters, numbers, `_`, and `-`), then a user password and root password (8 to 63 characters). Password input is masked. Use `su` to enter the root session. Try:

```text
help
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
terminal
```

The desktop code is bundled with this kernel. Bleuu's `desktop` package currently persists an enablement profile; it does not download a separate compositor. Click **BLEUU** on the taskbar to show service status. `run` launches the embedded demo; `bleuu start hello` launches the installed copy from the IDE disk. Bleuu currently has a fixed catalog, without repositories, dependency resolution, package versions, or arbitrary package files.

Other commands include `uptime`, `ticks`, `keyboard`, `mouse`, `ui`, `alloc`, `cat NOTE`, `write NOTE <text>`, `color <name>`, `clear`, `echo <text>`, `reboot`, and `halt`. `NOTE` is stored in `HOME`; an older note at the original disk sector is copied forward on first read without erasing the old sector.

## Storage and reinstall behavior

The MBR contains two OpSYS-specific partition entries: `SYSTEM` at LBA 2048–4095 for account and Bleuu metadata, and `HOME` from LBA 4096 to the end of the QEMU data disk. This is a raw custom layout, not a general-purpose filesystem. The current persistent file example is the single `NOTE` record in `HOME`.

The bootable OS image and the QEMU data image are separate. Replacing or rebuilding `opsys.img` leaves the data image intact; `make clean` also preserves it. Atomic A/B deployments, rollback, and a 30-second reinstall tool are not implemented yet.

## Boot details

Stage 1 loads stage 2. Stage 2 gathers the BIOS memory map, asks VBE for a linear framebuffer, prepares the GDT and initial page tables, enters 64-bit mode, loads the kernel, and jumps to its entry point. The kernel initializes memory, heap, interrupts, tasks, text console, and shell. The desktop is activated only when requested. Make assembles and links the separate x86_64 `HELLO.ELF` demo and embeds it in the kernel.

The floppy layout is stage 1 in sector 0, stage 2 in sectors 1–4, and the kernel from LBA 5. The bootloader loads the kernel at physical `0x10000`, identity-maps the first GiB, and also maps it at `0xffffffff80000000`. BootInfo version 2 passes the framebuffer metadata into the kernel.

## Project layout

```text
boot/          NASM BIOS stages
kernel/        Rust no_std kernel and target configuration
user/          Source and linker script for the ring-3 demo ELF
linker.ld      Kernel linker script
Makefile       Build, image creation, and QEMU run targets
build/         Generated images and binaries (ignored by Git)
```

## Limitations

This is an early hobby OS. The MBR entries are custom raw areas; there is no general filesystem or installer that repartitions arbitrary disks. Login is a learning feature, not a hardened security boundary: salts use the CPU timestamp counter, and there is no secure boot, cryptographic random generator, password reset, or per-file permission enforcement. Keep OpSYS on a disposable QEMU disk and do not reuse real passwords. There is no OCI runtime, Wayland compositor, JavaScript/CSS engine, Lua runtime, userspace libc, networking, USB, SMP, or hardware driver framework. Graphics use VBE software pixel routines rather than GPU acceleration. User pages lack per-segment ELF permissions and a user page fault currently halts the kernel. Bleuu is an early experiment rather than a Linux distribution package manager or a full `systemd`/`66`-style service system.
