; Loaded at 0x8000. Kernel is stored at LBA 5 and loaded to 0x10000.
bits 16
org 0x8000
start:
    cli
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov sp, 0x7c00
    mov [boot_drive], dl
    call collect_e820
    call setup_vbe
    call populate_boot_info
    ; Read the kernel via standard floppy CHS calls. Some BIOSes do not
    ; implement EDD reads for floppy drives. Image layout: kernel starts LBA 5
    ; (cylinder 0, head 0, sector 6) and reserves 127 sectors.
    mov byte [chs_cylinder], 0
    mov byte [chs_head], 0
    mov byte [chs_sector], 6
    mov word [sectors_left], 127
    mov ax, 0x1000
    mov es, ax
    xor bx, bx
.read_kernel_sector:
    mov ah, 0x02
    mov al, 1
    mov ch, [chs_cylinder]
    mov cl, [chs_sector]
    mov dh, [chs_head]
    mov dl, [boot_drive]
    int 0x13
    jc disk_error
    add bx, 512
    dec word [sectors_left]
    jz .kernel_loaded
    inc byte [chs_sector]
    cmp byte [chs_sector], 19
    jb .read_kernel_sector
    mov byte [chs_sector], 1
    inc byte [chs_head]
    cmp byte [chs_head], 2
    jb .read_kernel_sector
    mov byte [chs_head], 0
    inc byte [chs_cylinder]
    jmp .read_kernel_sector
.kernel_loaded:
    in al, 0x92
    or al, 2
    out 0x92, al
    lgdt [gdt_descriptor]
    mov eax, cr0
    or eax, 1
    mov cr0, eax
    jmp 0x08:protected_mode
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

bits 32
protected_mode:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov fs, ax
    mov gs, ax
    mov dword [0x70000], 0x71003
    mov dword [0x70004], 0
    mov dword [0x70000 + 511 * 8], 0x73003
    mov dword [0x70000 + 511 * 8 + 4], 0
    mov dword [0x71000], 0x72003
    mov dword [0x71004], 0
    mov dword [0x73000 + 510 * 8], 0x74003
    mov dword [0x73000 + 510 * 8 + 4], 0
    mov edi, 0x72000
    mov esi, 0x74000
    mov eax, 0x83
    mov ecx, 512
.map:
    mov [edi], eax
    mov dword [edi + 4], 0
    mov [esi], eax
    mov dword [esi + 4], 0
    add eax, 0x200000
    add edi, 8
    add esi, 8
    loop .map
    mov eax, cr4
    or eax, 1 << 5
    mov cr4, eax
    mov eax, 0x70000
    mov cr3, eax
    mov ecx, 0xc0000080
    rdmsr
    or eax, 1 << 8
    wrmsr
    mov eax, cr0
    or eax, 1 << 31
    mov cr0, eax
    jmp 0x18:long_mode

bits 64
long_mode:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov rsp, 0xffffffff8008fff8 ; SysV function-entry alignment, physical stack at 0x8fff8
    xor rbp, rbp
    mov edi, 0x6000              ; BootInfo pointer, identity mapped
    mov rax, 0xffffffff80010000  ; Higher-half alias of physical 0x10000
    jmp rax

bits 16
collect_e820:
    xor ebx, ebx
    mov word [e820_count], 0
    mov byte [e820_remaining], 32
.next:
    xor ax, ax
    mov es, ax
    mov ax, [e820_count]
    imul ax, 24
    add ax, 0x5000
    mov di, ax
    mov dword [es:di + 20], 1     ; Valid attributes even for 20-byte replies
    mov eax, 0xe820
    mov edx, 0x534d4150           ; 'SMAP'
    mov ecx, 24
    push ds
    int 0x15
    pop ds
    jc .done
    cmp eax, 0x534d4150
    jne .done
    cmp ecx, 20
    jb .done
    inc word [e820_count]
    dec byte [e820_remaining]
    jz .done
    test ebx, ebx
    jnz .next
.done:
    ret

populate_boot_info:
    ; repr(C) BootInfo v2 at physical 0x6000; E820 entries begin at 0x5000.
    mov dword [0x6000], 0x5953504f ; "OPSY" (little-endian)
    mov dword [0x6004], 0x00343653 ; "S64\0"
    mov dword [0x6008], 0x00005000
    mov dword [0x600c], 0
    movzx eax, word [e820_count]
    mov dword [0x6010], eax
    mov dword [0x6014], 2
    mov dword [0x6018], 0x00070000
    mov dword [0x601c], 0
    mov dword [0x6020], 0x40000000 ; Bootstrap direct map is 1 GiB
    mov dword [0x6024], 0
    mov dword [0x6028], gdt
    mov dword [0x602c], 0
    mov dword [0x6030], 0
    mov dword [0x6034], 0
    mov dword [0x6038], 0
    mov dword [0x603c], 0
    mov dword [0x6040], 0
    mov dword [0x6044], 0
    mov dword [0x6048], 0
    cmp byte [vbe_enabled], 1
    jne .done
    mov eax, [0x6228]           ; VBE linear framebuffer physical base
    mov [0x6030], eax
    movzx eax, word [0x6210]    ; bytes per scanline
    mov [0x6038], eax
    movzx eax, word [0x6212]    ; horizontal resolution
    mov [0x603c], eax
    movzx eax, word [0x6214]    ; vertical resolution
    mov [0x6040], eax
    movzx eax, byte [0x6219]    ; bits per pixel
    mov [0x6044], eax
    mov al, [0x6220]            ; red field position
    mov [0x6048], al
    mov al, [0x6222]            ; green field position
    mov [0x6049], al
    mov al, [0x6224]            ; blue field position
    mov [0x604a], al
.done:
    ret

setup_vbe:
    ; Request VBE mode 0x118 (1024x768 color) with its linear framebuffer.
    ; If the BIOS cannot provide it, keep the firmware's text mode.
    push ds
    push es
    xor ax, ax
    mov es, ax
    mov di, 0x6200
    mov ax, 0x4f01
    mov cx, 0x0118
    int 0x10
    cmp ax, 0x004f
    jne .unavailable
    test word [0x6200], 0x0080  ; linear framebuffer supported
    jz .unavailable
    cmp byte [0x6219], 24       ; accept common 24-bit VBE color modes
    je .color_mode_ok
    cmp byte [0x6219], 32       ; and 32-bit modes if the BIOS offers one
    jne .unavailable
.color_mode_ok:
    cmp byte [0x621b], 6        ; direct-color memory model
    jne .unavailable
    cmp dword [0x6228], 0       ; framebuffer physical address
    je .unavailable
    mov ax, 0x4f02
    mov bx, 0x4118              ; mode 0x118 plus linear-framebuffer flag
    int 0x10
    cmp ax, 0x004f
    jne .unavailable
    mov byte [vbe_enabled], 1
    pop es
    pop ds
    ret
.unavailable:
    mov byte [vbe_enabled], 0
    pop es
    pop ds
    ret

bits 16
align 8
gdt:
    dq 0
    dq 0x00cf9a000000ffff
    dq 0x00cf92000000ffff
    dq 0x00af9a000000ffff
    dq 0x00cff2000000ffff ; Ring 3 data, selector 0x23
    dq 0x00affa000000ffff ; Ring 3 64-bit code, selector 0x2b
    dq 0, 0                ; 64-bit TSS descriptor, filled by Rust
gdt_end:
gdt_descriptor:
    dw gdt_end - gdt - 1
    dd gdt
chs_cylinder db 0
chs_head db 0
chs_sector db 0
sectors_left dw 0
boot_drive db 0
e820_count dw 0
e820_remaining db 0
vbe_enabled db 0
error_msg db 'kernel CHS read failed', 0
