; BIOS boot sector: load the next four sectors (stage 2) to 0000:8000.
bits 16
org 0x7c00

start:
    cli
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov sp, 0x7c00
    sti
    mov [boot_drive], dl

    mov ah, 0x02             ; BIOS CHS read
    mov al, 4                ; stage 2 occupies sectors 2 through 5
    mov ch, 0
    mov cl, 2
    mov dh, 0
    mov dl, [boot_drive]
    mov bx, 0x8000
    int 0x13
    jc disk_error
    jmp 0x0000:0x8000

disk_error:
    mov si, error_msg
.print:
    lodsb
    test al, al
    jz .halt
    mov ah, 0x0e
    int 0x10
    jmp .print
.halt:
    cli
    hlt
    jmp .halt

boot_drive db 0
error_msg db 'stage2 read failed', 0

times 510-($-$$) db 0
dw 0xaa55
