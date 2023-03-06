%include "asm_def.asm"

global asm_switch_thread
global asm_thread_init

extern post_switch_handler

asm_switch_thread:
    ; args:
    ; rdi: new_rsp: usize
    ; rsi: new_addr_space: usize
    ; save all registers that need to be saved by sysv abi into the old registers argument

    ; save all registers that need to be saved by sysv abi onto the stack
    push rbx
    push rbp
    push r12
    push r13
    push r14
    push r15

    ; save rflags
    pushfq

    ; save old rsp in rdi to pass to post switch handler
    mov rdi, rsp

    ; check if new address space is different than the currently loaded address space
    mov rax, cr3
    cmp rax, rsi
    je .skip_addr_space_load

    ; load new address space if it was different
    mov cr3, rsi

.skip_addr_space_load

    ; load rsp of new thread
    mov rsp, rdi

    ; at this point, we have switched to the new thread

    ; call post_switch_handler
    ; arg1 rdi is the old rsp, which was saved from before
    mov rax, post_switch_handler
    call rax

    ; restore flags
    popfq

    ; restore all registers sysv abi says are saved
    pop r15
    pop r14
    pop r13
    pop r12
    pop rbp
    pop rbx

    ret

asm_thread_init:
    ; load registers specified on stack
    pop rbx
    pop rdx
    pop rsi
    pop rdi

    ; load instruction pointer into rcx
    pop rcx

    ; load default rflags (only bit set is interrupt enable + reserved bit that is always 1)
    mov r11, 0x202

    ; load userspace stack
    pop rax
    mov rsp, rax

    o64 sysret