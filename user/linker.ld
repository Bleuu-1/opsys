OUTPUT_FORMAT(elf64-x86-64)
ENTRY(_start)
PHDRS
{
    text PT_LOAD FLAGS(5);
    rodata PT_LOAD FLAGS(4);
}
SECTIONS
{
    . = 0x40000000;
    .text : ALIGN(0x1000) { *(.text .text.*) } :text
    .rodata : ALIGN(0x1000) { *(.rodata .rodata.*) } :rodata
    /DISCARD/ : { *(.comment*) *(.eh_frame*) *(.note*) }
}
