bits 64
default rel
global _start

section .text
_start:
    mov rax, 1                  ; SYS_WRITE
    mov rdi, message
    mov rsi, message_end - message
    int 0x31

.hang:
    xor rax, rax                ; SYS_YIELD
    int 0x31
    jmp .hang

section .rodata
message: db "Hello from an isolated ring-3 ELF process!", 10
message_end:
