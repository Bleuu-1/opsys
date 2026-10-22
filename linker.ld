ENTRY(_start)
KERNEL_OFFSET = 0xffffffff80000000;
KERNEL_PHYS_START = 0x00010000;
SECTIONS {
    . = KERNEL_OFFSET + KERNEL_PHYS_START;
    .text ALIGN(16) : AT(ADDR(.text) - KERNEL_OFFSET) {
        KEEP(*(.text._start)) *(.text .text.*)
    }
    .rodata ALIGN(16) : AT(ADDR(.rodata) - KERNEL_OFFSET) { *(.rodata .rodata.*) }
    .data ALIGN(16) : AT(ADDR(.data) - KERNEL_OFFSET) {
        *(.got .got.*) *(.data .data.*)
    }
    .bss ALIGN(16) (NOLOAD) : AT(ADDR(.bss) - KERNEL_OFFSET) {
        __bss_start = .;
        *(COMMON) *(.bss .bss.*)
        __bss_end = .;
    }
    __kernel_end = .;
    /DISCARD/ : { *(.eh_frame*) *(.comment*) }
}
