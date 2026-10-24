NASM ?= nasm
CARGO ?= cargo
OBJCOPY ?= objcopy
NM ?= nm
LD ?= ld
QEMU ?= qemu-system-x86_64
VERSION := $(shell tr -d '\r\n' < VERSION)
IMAGE := build/opsys.img
DATA_IMAGE := build/opsys-data.img
ISO := build/opsys-$(VERSION).iso
RELEASE_DATA := build/opsys-$(VERSION)-data.img
KERNEL := kernel/target/x86_64-unknown-none/release/opsys-kernel

.PHONY: all run iso run-iso release-assets clean clean-data force-release-data
all: $(IMAGE) $(DATA_IMAGE)

build:
	mkdir -p build

build/stage1.bin: boot/stage1.asm | build
	$(NASM) -f bin $< -o $@

build/stage2.bin: boot/stage2.asm | build
	$(NASM) -f bin $< -o $@

build/hello.o: user/hello.asm | build
	$(NASM) -f elf64 $< -o $@

build/hello.elf: build/hello.o user/linker.ld
	$(LD) -nostdlib -z max-page-size=0x1000 -T user/linker.ld -o $@ build/hello.o

build/bleuubar.elf: user/bleuubar/Cargo.toml user/bleuubar/.cargo/config.toml user/bleuubar/bleuubar.ld user/bleuubar/bleuubar.css user/bleuubar/src/main.rs user/bleuubar/src/css.rs user/bleuubar/src/render.rs user/bleuubar/src/sys.rs | build
	cd user/bleuubar && $(CARGO) build --release
	cp user/bleuubar/target/x86_64-unknown-none/release/bleuubar $@

$(KERNEL): VERSION kernel/src/main.rs kernel/src/auth.rs kernel/src/security.rs kernel/src/partition.rs kernel/src/system.rs kernel/src/memory.rs kernel/src/heap.rs kernel/src/task.rs kernel/src/keyboard.rs kernel/src/mouse.rs kernel/src/graphics.rs kernel/src/visual.rs kernel/src/wm.rs kernel/src/serial.rs kernel/src/ata.rs kernel/src/fs.rs kernel/src/diskfs.rs kernel/src/vfs.rs kernel/src/settings.rs kernel/src/user.rs kernel/src/bleuu.rs kernel/Cargo.toml kernel/.cargo/config.toml linker.ld build/hello.elf build/bleuubar.elf
	cd kernel && $(CARGO) build --release

build/kernel.bin: $(KERNEL) | build
	$(OBJCOPY) -O binary $(KERNEL) $@
	@test "$$(wc -c < $@)" -le 458752 || (echo 'kernel exceeds 896-sector boot limit' >&2; exit 1)
	@python3 -c 'import subprocess; lines=subprocess.check_output(["$(NM)", "-n", "$(KERNEL)"], text=True).splitlines(); end=int(next(line.split()[0] for line in lines if line.endswith(" __kernel_end")), 16)-0xffffffff80000000; assert end <= 0x80000, f"kernel BSS ends at physical {end:#x}, overlapping the boot stack"'

$(IMAGE): build/stage1.bin build/stage2.bin build/kernel.bin
	@test "$$(wc -c < build/stage1.bin)" -eq 512 || (echo 'stage1 must be exactly 512 bytes' >&2; exit 1)
	@test "$$(wc -c < build/stage2.bin)" -le 2048 || (echo 'stage2 must fit in four sectors' >&2; exit 1)
	cp build/stage1.bin $@
	dd if=build/stage2.bin bs=512 conv=sync,notrunc of=$@ seek=1 status=none
	dd if=build/kernel.bin bs=512 conv=sync,notrunc of=$@ seek=5 status=none
	truncate -s 1474560 $@

$(DATA_IMAGE): | build
	truncate -s 8M $@

iso: $(ISO)

$(RELEASE_DATA): force-release-data | build
	rm -f $@
	truncate -s 8M $@

force-release-data:

$(ISO): $(IMAGE) $(RELEASE_DATA) README.md VERSION tools/mkiso.py
	python3 tools/mkiso.py --boot $(IMAGE) --data $(RELEASE_DATA) --readme README.md --version $(VERSION) --output $@

release-assets: $(ISO) $(RELEASE_DATA)

run: $(IMAGE) build/opsys-data.img
	$(QEMU) -machine pc -display gtk -serial stdio -boot order=a -drive format=raw,file=$(IMAGE),if=floppy -drive file=build/opsys-data.img,if=ide,index=0,format=raw -no-reboot -no-shutdown

run-iso: $(ISO) $(DATA_IMAGE)
	$(QEMU) -machine pc -display gtk -serial stdio -boot order=d -cdrom $(ISO) -drive file=$(DATA_IMAGE),if=ide,index=0,format=raw -no-reboot -no-shutdown

clean:
	rm -f build/stage1.bin build/stage2.bin build/hello.o build/hello.elf build/bleuubar.elf build/kernel.bin build/opsys.img build/opsys-*.iso build/opsys-*-data.img
	rm -rf kernel/target
	rm -rf user/bleuubar/target

# Explicitly destructive: removes saved accounts, HOME files, and installed
# Bleuu package state from the QEMU data disk.
clean-data:
	rm -f build/opsys-data.img
