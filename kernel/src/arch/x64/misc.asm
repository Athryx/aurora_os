%include "asm_def.asm"

global asm_gs_addr
global asm_prid

asm_gs_addr:
	swapgs
	mov rax, [gs:gs_data_ptr.ptr]
	swapgs
	ret

asm_prid:
	swapgs
	mov rax, [gs:gs_data_ptr.prid]
	swapgs
	ret
