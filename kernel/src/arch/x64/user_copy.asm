global asm_user_copy
global asm_user_copy_end
global asm_user_copy_fail:

section .text
bits 64
asm_user_copy:
    ; asm_user_copy(dst, src, count) -> bool
    cld
    mov ecx, edx
    rep movsb

    ; indicates success
    mov rax, 1

    ret
asm_user_copy_end:

; page fault handler will go to this function on user copy failure
asm_user_copy_fail:
    ; indicates a failure (page fault) while copying to or from userspace
    xor rax, rax
    ret