# OpSYS architecture roadmap

This document records implemented milestones and remaining storage, security,
userland, graphics, and process steps for OpSYS. The bounded MBR parser,
shell-level authorization, and small HOME filesystem are implemented in
`kernel/src/partition.rs`, `kernel/src/security.rs`, `kernel/src/diskfs.rs`,
and `kernel/src/vfs.rs`. The remaining designs below are not claims that OpSYS
has a general-purpose filesystem, secure multi-user isolation, a Ring-3
Ring-3 desktop compositor, or system rollback.

## Current boundary

The bootable floppy image contains the kernel. The separate QEMU IDE data disk
stores the account, Bleuu catalog, and one `NOTE` record. On a blank disk OpSYS
writes the default MBR layout: custom type `0x7f` for `SYSTEM` and `0x7e` for
`HOME`. The parser reads all four MBR entries and checks signatures, bounds,
overlap, and duplicate OpSYS partition types. Partition I/O uses relative LBAs
and rejects out-of-range requests. Existing partitions are not reformatted.

The shell authorization layer distinguishes a root session (UID 0) from the
single regular account (UID 1000). It protects package install/remove, service
start/stop/restart, desktop start, halt, and reboot. HOME entries now store
their owner UID; kernel VFS operations restrict regular users to their own
entries while root can access all entries. This is still not process
isolation: the current syscall ABI has no process credentials, and kernel code
remains Ring 0. Treat it as a development guard, not as isolation against
malicious user programs.

`vfs::VFS` exposes built-in read-only `README.TXT` and `HELLO.ELF`, the legacy
persistent `NOTE` sector, and user files on `HOME`. Format version 2 supports
ten file or directory entries, nested paths up to 23 bytes, and files up to 2
KiB. `mkdir`, `touch`, `ls`, `cat`, and `write` operate on those paths. Each
entry stores an owner UID; VFS reads, writes, directory listings, and parent
directory changes enforce ownership, with UID 0 able to administer all
entries. Version 1 metadata is upgraded in place and its shared files are
assigned to UID 1000. Two checksummed directory snapshots alternate across
HOME sectors 1 and 2. Each file has two fixed data banks beginning at HOME
sector 16; replacement writes and verifies the inactive bank before switching
metadata. This is a small copy-on-write record format, not a journal. It has no
deletion, rename, GID/mode bits, or general allocation bitmap. `NOTE` stays at
HOME sector 0 for compatibility.

## 1. User credentials, authorization, and Bleuu packages

### Credentials and capability checks

The shell obtains a `Credentials` value from the authenticated session and
checks required capabilities before dispatching a command. When processes are
introduced, credentials must be stored in each task control block and copied
from a trusted parent or created by the login service. Never accept a UID or
capability mask directly from a Ring-3 syscall argument.

Recommended task fields:

```rust
#[derive(Clone, Copy)]
pub struct ProcessCredentials {
    pub uid: u32,
    pub gid: u32,
    pub capabilities: u64,
}

pub struct TaskControlBlock {
    pub pid: u32,
    pub credentials: ProcessCredentials,
    // saved stack, address space, state, and scheduling fields follow
}

pub fn require(task: &TaskControlBlock, capability: u64) -> Result<(), Errno> {
    if task.credentials.capabilities & capability == capability {
        Ok(())
    } else {
        Err(Errno::PermissionDenied)
    }
}
```

The kernel must check capabilities at the operation boundary too. For example,
`sys_spawn` should check the executable's ownership and mode, and a service
control syscall should check `CAP_SERVICE_ADMIN`; checking only the shell text
would let a Ring-3 program bypass the policy. Keep separate capabilities for
package installation, service control, storage administration, and reboot.
The user-facing `su` flow should create a privileged shell process after
password verification; it should not turn a global boolean into ambient
privilege for every task.

### Package loading

Bleuu currently stores one fixed ELF demo in `SYSTEM`. A general installer
should validate a package manifest, verify its digest/signature, write payload
to a temporary file, flush it, verify the payload, then atomically publish the
catalog entry. Package files must be readable by the VFS and loaded into a
fresh address space, never executed in Ring 0.

Before mapping an ELF segment, validate all arithmetic and ranges:

```rust
let file_end = ph_offset.checked_add(file_size).ok_or(LoadError::Overflow)?;
let mem_end = virtual_address.checked_add(memory_size).ok_or(LoadError::Overflow)?;
if file_end > image.len() || file_size > memory_size || mem_end > USER_TOP {
    return Err(LoadError::OutOfRange);
}
if virtual_address < USER_BASE || memory_size == 0 {
    return Err(LoadError::OutOfRange);
}
```

For each `PT_LOAD`, allocate zeroed user frames, copy only the file-backed
bytes, leave the remainder zero for BSS, and map permissions from ELF flags:
readable pages, writable pages only when `PF_W` is set, and executable pages
only when `PF_X` is set. Reject writable-and-executable mappings unless a
specific runtime requires them. Use NX when supported. Reject overlapping
segments with conflicting permissions, malformed alignment, and entry points
outside executable mappings. A checksum detects accidental corruption but is
not an authenticity check.

### Per-user configuration

The shell stores color, font, and desktop background values through VFS in
separate `cfg0` and `cfg1000` files. New settings use a literal-only Lua table
format and VFS checksums/copy-on-write updates; the kernel parses only those
three named strings and never executes Lua. Existing version-1 binary records
remain readable and are rewritten as Lua on the next setting change. `config`
shows, changes, resets, and prints the active user's configuration; login
reapplies the saved values. Background presets require the desktop package to
be installed. This covers only the current single regular UID and root.
General per-user configuration trees still need a larger filesystem with
directories, allocation, and richer metadata.

## 2. VBE desktop and compositor

The current desktop has a small Ring-0 tiling window manager in
`kernel/src/wm.rs`. It tiles the Terminal, System, and Bleuu panels in a
master-and-stack layout, tracks focus, supports three workspaces, and exposes
mouse and Super-key controls. The Terminal panel is the interactive shell
surface. Six saved background palettes are available through the per-user
Lua-style config. This gives OpSYS a window-management interface, but the
panels are still kernel-rendered and are not independent user processes. The
VBE path still draws directly into the linear framebuffer; it does not yet
have a back buffer or page flipping.

Keep interrupt handlers short: the PIT handler should update time and request
a reschedule, not draw or copy a screen. A later compositor can render in a
normal task into a back buffer and publish a frame at a controlled point.

### Double buffering and dirty rectangles

Allocate a back buffer sized from the checked framebuffer width, height, and
pitch. Render into it, then copy only clipped dirty rectangles to the VBE
front buffer. Coalesce nearby rectangles; if the dirty area becomes large,
copy the full frame. A simple rectangle type is:

```rust
#[derive(Clone, Copy)]
pub struct Rect { pub x: u32, pub y: u32, pub width: u32, pub height: u32 }

pub fn clip(rect: Rect, width: u32, height: u32) -> Option<Rect> {
    let right = rect.x.checked_add(rect.width)?.min(width);
    let bottom = rect.y.checked_add(rect.height)?.min(height);
    (rect.x < right && rect.y < bottom).then_some(Rect {
        x: rect.x, y: rect.y, width: right - rect.x, height: bottom - rect.y,
    })
}
```

When copying rows, use the reported framebuffer pitch and pixel format rather
than assuming tightly packed 32-bit RGB. The compositor should own a bounded
dirty-region list and fall back to a full refresh when it overflows. On
single-buffer VBE hardware, software double buffering still reduces flicker
but cannot guarantee tear-free page flips.

### Ring-3 windows and compositor

Move policy and window layout into a user process. Keep privileged hardware
access in a kernel display service. A first ABI can offer `display_submit`
(validated shared pixel buffer plus damage list), `event_wait`, and
`event_send`; it should not expose unrestricted physical framebuffer memory.
Each window has an owner PID, rectangle, z-order, and input flags. Hit-testing
walks the visible stack from front to back and clips to the window bounds.
Mouse packets from IRQ12 are queued in the existing interrupt-safe byte queue;
the desktop task turns complete packets into pointer and button events.

```rust
pub struct Window {
    pub owner_pid: u32,
    pub bounds: Rect,
    pub z: i16,
    pub accepts_pointer: bool,
}

pub fn hit_test(windows: &[Window], x: u32, y: u32) -> Option<usize> {
    windows.iter().enumerate()
        .filter(|(_, w)| w.accepts_pointer
            && x >= w.bounds.x && y >= w.bounds.y
            && x < w.bounds.x.saturating_add(w.bounds.width)
            && y < w.bounds.y.saturating_add(w.bounds.height))
        .max_by_key(|(_, w)| w.z)
        .map(|(index, _)| index)
}
```

## 3. Block devices, VFS, and recovery

### Block layer and partition devices

The MBR parser exposes bounded `Partition` sector operations. ATA PIO calls
now use a shared atomic bus lock, validate the ATA LBA28 limit, poll with
timeouts, flush writes, and return typed errors. Partition and VFS operations
preserve those failures for shell diagnostics. The lock serializes individual
ATA commands; it does not make a multi-sector operation atomic across account,
filesystem, and Bleuu users. The next storage step is a block-device trait and
broader transaction/journal support.

```rust
pub trait BlockDevice {
    fn sector_count(&self) -> u64;
    fn read(&mut self, lba: u64, sector: &mut [u8; 512]) -> Result<(), BlockError>;
    fn write(&mut self, lba: u64, sector: &[u8; 512]) -> Result<(), BlockError>;
    fn flush(&mut self) -> Result<(), BlockError>;
}

pub struct PartitionDevice<'a, D> { pub disk: &'a mut D, pub start: u64, pub len: u64 }
```

The partition adapter must reject `lba >= len`, use checked addition for
`start + lba`, and call the underlying device only after that check. The current
ATA driver flushes after each sector write, but it has no shared lock across
filesystem, account, and Bleuu metadata operations or multi-sector transaction
support. The tiny filesystem serializes its own calls only.

### Filesystem growth targets

The bounded HOME filesystem is the first implemented step. It is useful for
small persistent shell files, but it is not a POSIX filesystem and should only
be used on a disposable QEMU disk. Add delete/rename, a bitmap or extents,
longer names and files, and explicit mount/format diagnostics before relying
on it for important data.

Keep path resolution independent from storage format:

```rust
pub trait VnodeOps {
    fn lookup(&self, parent: NodeId, name: &[u8]) -> Result<NodeId, FsError>;
    fn read(&self, node: NodeId, offset: u64, out: &mut [u8]) -> Result<usize, FsError>;
    fn write(&mut self, node: NodeId, offset: u64, data: &[u8]) -> Result<usize, FsError>;
    fn create(&mut self, parent: NodeId, name: &[u8], kind: NodeKind) -> Result<NodeId, FsError>;
}
```

The current format uses fixed data banks instead of a bitmap. A later scalable
format can use a versioned superblock, allocation bitmap, directory entries,
and extents. Validate every block number against the containing partition
before I/O. Add `uid`, `gid`, and mode bits to metadata. The current parser
rejects `.` and `..`, repeated separators, invalid characters, and overlong
paths; it does not yet implement path normalization. Start with `SYSTEM`
mounted read-only for normal users and `HOME` writable for their owned subtree.

Do not call a two-copy file record a journal. For reliable metadata changes,
reserve journal blocks and use this order: write transaction records; flush;
write a commit record with sequence and checksum; flush; apply metadata; flush;
then clear/recycle the journal. Recovery replays committed transactions and
ignores incomplete ones. The current per-file dual-bank replacement has a
smaller atomic commit point but does not make multi-file operations atomic.
Keep the data disk disposable.

### A/B system deployments

The current BIOS bootloader reads the kernel from the floppy image, while the
IDE disk is only a data disk. A/B slots on `SYSTEM` cannot replace the active
kernel until the bootloader itself reads a selected slot from that disk.
Reserve `SYSTEM_A` and `SYSTEM_B`, plus two small boot metadata records with a
monotonic generation, active slot, pending slot, attempt count, and checksum.
Install only into the inactive slot, verify its image and manifest, flush it,
then publish a pending-slot record. The bootloader tries the pending image and
marks it healthy only after the kernel explicitly confirms successful startup.
After bounded failed attempts, boot the previous active slot. `bleuu rollback`
should only change this metadata after validating the fallback image.

Keep user data on `HOME` and never format it during system replacement. A fast
reinstall is possible only after the boot chain can select these slots and
filesystem recovery is reliable; it is not implemented by the current build.

## 4. Scheduling, syscalls, and interrupt events

The PIT already drives round-robin scheduling of fixed kernel tasks. The
current ring-3 demo has a single process slot and syscall numbers for write,
yield, and exit. Move toward a preemptive process scheduler in small steps:

1. Define one saved context layout shared by the interrupt stub and Rust.
2. Save all caller/callee-saved registers, user RIP/RSP, RFLAGS, and address
   space on privilege transitions; validate the frame before restoring it.
3. Add task states `Ready`, `Running`, `Blocked`, and `Exited`, a bounded run
   queue, and per-task kernel stacks/TSS `RSP0`.
4. Keep IRQ handlers to acknowledging hardware and enqueueing events. Defer
   wakeups, logging, graphics, and disk requests to tasks.
5. Make scheduler locks interrupt-safe. Do not allocate or block while holding
   a lock needed by an interrupt handler.

System calls should copy user buffers through checked user-memory helpers,
validate lengths and permissions, and return negative errno values. Add
`sys_spawn` only after the ELF loader has safe per-segment mappings. Add
`sys_kill` with PID ownership/capability checks, and never kill the currently
executing task before it has switched to a safe kernel stack.

The keyboard and mouse drivers already enqueue interrupt data into bounded
atomic queues and count dropped input. Preserve the single-producer/single-
consumer rule: IRQ code is the sole producer, a task is the sole consumer, and
overflow is visible to diagnostics. If there are multiple consumers, add a
proper lock or per-consumer queues rather than sharing the SPSC ring.

## Recommended implementation order

1. Add a block-device trait and multi-sector transaction support around the
   current serialized ATA operations.
2. Expand the tiny HOME filesystem on a disposable QEMU disk with delete,
   rename, a scalable allocation scheme, and GID/mode bits.
3. Add per-process credentials and enforce them in syscalls and filesystem
   operations, not just command dispatch.
4. Harden ELF loading and process teardown before allowing arbitrary package
   binaries.
5. Move desktop drawing out of IRQ paths, add a back buffer and damage list,
   then implement window input and display syscalls.
6. Make the bootloader load kernels from the data disk; only then implement
   A/B images, boot confirmation, and rollback.

Build and boot in QEMU after each implementation step, using a disposable data
image. The storage format, syscall ABI, and boot image layout are not stable
interfaces yet.
