%include "asm_def.asm"

global syscall_entry

extern syscalls

section .text
bits 64
syscall_entry:
	; kernel stack pointer should be 16 byte aligned
	; iretq clears gs_base msr, so we have to keep it in gs_base_k, and use swapgs to access it
	swapgs
	mov [gs:gs_data_ptr.temp], rcx			; save return rip in temp register
	mov r10, [gs:gs_data_ptr.ptr]			; load pointer to gs data

	mov rcx, rsp
	mov [r10 + gs_data.call_save_rsp], rsp	; save caller rsp
	mov rsp, [r10 + gs_data.call_rsp]		; load kernel rsp

	push r11		; save old flags
	push rcx		; save old rsp

	mov rcx, [gs:gs_data_ptr.temp]			; restore return rip

	swapgs
	sti

	push rcx		; save return rip

	push r15		; push args on stack
	push r14
	push r13
	push r12
	push r9
	push r8
	push rdi
	push rsi
	push rdx
	push rbx

	mov rcx, rax	; push options
	shr rcx, 32
	push rcx

	shl rax, 32		; cant use and because it messes things up
	shr rax, 32

	cmp rax, 41		; make sure it is a valid syscall
	jg .invalid_syscall

	mov rdi, rsp
	sub rsp, 8		; align stack

	mov r10, syscalls
	mov rax, [r10 + rax * 8]
	call rax		; stack is already 16 byte aligned

	add rsp, 8		; put stack pointer to right place
	pop rax

	jmp .valid_syscall

.invalid_syscall:
	add rsp, 8
	mov rax, -1 

.valid_syscall:
	pop rbx
	pop rdx
	pop rsi
	pop rdi
	pop r8
	pop r9
	pop r12
	pop r13
	pop r14
	pop r15

	pop rcx			; restore return rip

	cli
	swapgs

	mov r10, [gs:gs_data_ptr.ptr]		; get gs ptr
	mov [gs:gs_data_ptr.temp], rcx		; return rip in temporary location
	mov rcx, [r10 + gs_data.call_save_rsp]	; get save rsp and put in rcx


	pop r10			; read old rsp
	pop r11			; restore flags
	mov rsp, rcx	; restore save rsp

	mov rcx, [gs:gs_data_ptr.temp]	; load return rip from temporary location
	swapgs
	o64 sysret
