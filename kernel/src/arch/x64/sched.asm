%include "asm_def.asm"

global asm_switch_thread

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

    ; load new address space
    mov cr3, rsi

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