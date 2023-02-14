%include "asm_def.asm"

global asm_switch_thread

asm_switch_thread:
    ; args:
    ; rdi: old_rsp: &mut usize
    ; rsi: new_rsp: usize
    ; save all registers that need to be saved by sysv abi into the old registers argument

    ; save all registers that need to be saved by sysv abi onto the stack
    push rbx
    push rbp
    push r12
    push r13
    push r14
    push r15
    pushfq      ; save rflags

    ; push rip we will return to next time this thread runs
    mov rax, asm_switch_thread.return
    push rax

    ; load rsp of new thread
    mov rsp, rsi

    ret

.return:
    popfq       ; restore flags

    ; restore all registers sysv abi says are saved
    pop r15
    pop r14
    pop r13
    pop r12
    pop rbp
    pop rbx

    ret