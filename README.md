# OpSYS

**A small, experimental 64-bit hobby operating system built from scratch with a NASM BIOS bootloader and a Rust `no_std` kernel.** OpSYS boots in QEMU to a graphical desktop with a keyboard-driven terminal. It is a learning project for exploring how an operating system works; it is not intended to replace Linux or to run on everyday hardware.

The name on the console is drawn with text characters:

```text
██████╗ ██████╗ ███████╗██╗   ██╗███████╗
██╔═══██╗██╔══██╗██╔════╝╚██╗ ██╔╝██╔════╝
██║   ██║██████╔╝███████╗ ╚████╔╝ ███████╗
██║   ██║██╔═══╝ ╚════██║  ╚██╔╝  ╚════██║
╚██████╔╝██║     ███████║   ██║   ███████║
 ╚═════╝ ╚═╝     ╚══════╝   ╚═╝   ╚══════╝
```

## What it has so far

- A custom BIOS boot path: 16-bit real mode to 32-bit protected mode to 64-bit long mode.
- VBE setup for a 1024×768 color linear framebuffer in QEMU (24-bit on the current QEMU VGA BIOS), with a VGA text-mode fallback.
- BIOS E820 memory-map handoff, four-level paging, a physical frame allocator, and an 8 MiB kernel heap.
- Rust kernel with CPU exception diagnostics, an IDT, PIC, PIT timer, queued keyboard input, and a PS/2 mouse driver on IRQ12.
- A small software framebuffer graphics driver and a desktop-style shell UI with a mouse cursor and a clickable Bleuu taskbar button. It uses VBE linear framebuffer memory; it is not a hardware-accelerated driver for a specific GPU.
- Round-robin kernel tasks and a small ring-3 ELF demo with `write`, `yield`, and `exit` system calls.
- A minimal ATA PIO driver and a tiny RAM filesystem, with one persistent `NOTE` record on the IDE disk.
- **Bleuu**, an early service and package manager. It stores the built-in `hello` demo on the IDE disk and can start or stop it, the storage service, and the serial console.
- An interactive shell with system information, memory, paging, task, disk, file, and package commands.

## Build and run

On Arch Linux, install the build tools and QEMU:

```sh
sudo pacman -S nasm qemu-system-x86 make binutils
```

Install Rust with `rustup`, then add the bare-metal target:

```sh
rustup target add x86_64-unknown-none
```

Build the bootable floppy image and start it in a QEMU window:

```sh
make
make run
```

The build creates `build/opsys.img` for booting and `build/opsys-data.img` as an 8 MiB IDE data disk. The QEMU window shows the framebuffer desktop; kernel output is also mirrored to COM1 in the terminal. The IDE image keeps the demo package and NOTE data between runs. **`make clean` deletes generated build files and the IDE data image, including saved data.**

## Shell commands

At the `opsys>` prompt, try:

```text
help
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

`run` launches the embedded demo directly. `bleuu start hello` launches the installed copy from the IDE disk. Click **BLEUU** on the graphical taskbar to show service status. The keyboard shell remains available inside the desktop terminal. Bleuu currently has one built-in package and a fixed on-disk catalog; it does not yet support repositories, dependency resolution, package versions, or arbitrary package files.

Other shell commands include `uptime`, `ticks`, `keyboard`, `mouse`, `ui`, `alloc`, `cat NOTE`, `write NOTE <text>`, `color <name>`, `clear`, `echo <text>`, `reboot`, and `halt`.

## How it boots

The first boot sector loads the second stage. The second stage gathers the BIOS memory map, asks VBE for a linear framebuffer, prepares the GDT and initial page tables, enters 64-bit mode, loads the kernel, and jumps to its entry point. The kernel maps the framebuffer into its virtual address space and draws the desktop and terminal using software pixel routines. The Rust kernel also initializes memory management, the heap, interrupt handling, the scheduler, and the shell. Make assembles and links the separate x86_64 `HELLO.ELF` demo and embeds it in the kernel for the `run` command and Bleuu installer.

The floppy layout is stage 1 in sector 0, stage 2 in sectors 1–4, and the kernel from LBA 5. The bootloader currently loads the kernel at physical `0x10000`, identity-maps the first GiB, and also maps it at `0xffffffff80000000`. BootInfo version 2 carries the VBE framebuffer address, pitch, resolution, and color-channel positions into the kernel.

## Project layout

```text
boot/          NASM BIOS stages
kernel/        Rust no_std kernel, mouse/graphics drivers, and target configuration
user/          Source and linker script for the ring-3 demo ELF
linker.ld      Kernel linker script
Makefile       Build, image creation, and QEMU run targets
build/         Generated images and binaries (ignored by Git)
```

## Current limitations and next steps

This is still an early hobby OS. It boots through legacy BIOS, supports a single demo process, has no general filesystem or userspace libc, and has no networking, USB, SMP, or hardware driver framework. The graphics driver draws to a VBE linear framebuffer and does not use GPU acceleration. User pages are not yet protected with per-segment ELF permissions, and a user page fault currently halts the kernel. Bleuu is a small experiment rather than a Linux distribution package manager or a full `systemd`/`66`-style service system.

Possible next milestones are terminating only the process on a user page fault, adding a real filesystem, loading arbitrary ELF files, improving the service model, and expanding device support.
