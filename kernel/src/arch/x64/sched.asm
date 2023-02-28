%include "asm_def.asm"

global asm_switch_thread

extern post_switch_handler

asm_switch_thread:
    ; args:
    ; rdi: old_int_status: usize
    ; rsi: new_rsp: usize
    ; rdx: new_addr_space: usize
    ; save all registers that need to be saved by sysv abi into the old registers argument

    ; save all registers that need to be saved by sysv abi onto the stack
    push rbx
    push rbp
    push r12
    push r13
    push r14
    push r15

    ; save rflags, but set interrupt enable bit according to old_int_status
    pushfq
    pop rax
    cmp rdi, 0
    je .keep_int_disabled

    ; mark interrupts as enabled when we return
    or rax, 1 << 9

.keep_int_disabled
    push rax      ; save rflags


    ; save old rsp in rdi to pass to post switch handler
    mov rdi, rsp

    ; load new address space
    mov cr3, rdx

    ; load rsp of new thread
    mov rsp, rsi

    ; at this point, we have switched to the new thread

    ; call post_switch_handler
    ; arg1 rdi is the old rsp, which was saved from before
    mov rax, post_switch_handler
    call rax

    popfq       ; restore flags

    ; restore all registers sysv abi says are saved
    pop r15
    pop r14
    pop r13
    pop r12
    pop rbp
    pop rbx

    ret