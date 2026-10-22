NASM ?= nasm
CARGO ?= cargo
OBJCOPY ?= objcopy
LD ?= ld
QEMU ?= qemu-system-x86_64
IMAGE := build/opsys.img
KERNEL := kernel/target/x86_64-unknown-none/release/opsys-kernel

.PHONY: all run clean
all: $(IMAGE) build/opsys-data.img

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

$(KERNEL): kernel/src/main.rs kernel/src/memory.rs kernel/src/heap.rs kernel/src/task.rs kernel/src/keyboard.rs kernel/src/mouse.rs kernel/src/graphics.rs kernel/src/serial.rs kernel/src/ata.rs kernel/src/fs.rs kernel/src/user.rs kernel/src/bleuu.rs kernel/Cargo.toml kernel/.cargo/config.toml linker.ld build/hello.elf
	cd kernel && $(CARGO) build --release

build/kernel.bin: $(KERNEL) | build
	$(OBJCOPY) -O binary $(KERNEL) $@
	@test "$$(wc -c < $@)" -le 65024 || (echo 'kernel exceeds 127-sector boot limit' >&2; exit 1)

$(IMAGE): build/stage1.bin build/stage2.bin build/kernel.bin
	@test "$$(wc -c < build/stage1.bin)" -eq 512 || (echo 'stage1 must be exactly 512 bytes' >&2; exit 1)
	@test "$$(wc -c < build/stage2.bin)" -le 2048 || (echo 'stage2 must fit in four sectors' >&2; exit 1)
	cp build/stage1.bin $@
	dd if=build/stage2.bin bs=512 conv=sync,notrunc of=$@ seek=1 status=none
	dd if=build/kernel.bin bs=512 conv=sync,notrunc of=$@ seek=5 status=none
	truncate -s 1474560 $@

build/opsys-data.img: | build
	truncate -s 8M $@

run: $(IMAGE) build/opsys-data.img
	$(QEMU) -machine pc -display gtk -serial stdio -boot order=a -drive format=raw,file=$(IMAGE),if=floppy -drive file=build/opsys-data.img,if=ide,index=0,format=raw -no-reboot -no-shutdown

clean:
	rm -rf build kernel/target
