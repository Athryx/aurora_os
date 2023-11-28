%include "asm_def.asm"

extern eoi
extern rust_int_handler

%macro save_regs 0
	; rsp, rflags, and rip are set on interrupt stack frame
	sub rsp, registers_size
	mov [rsp + registers.rax], rax
	mov [rsp + registers.rbx], rbx
	mov [rsp + registers.rcx], rcx
	mov [rsp + registers.rdx], rdx
	mov [rsp + registers.rbp], rbp
	mov [rsp + registers.rdi], rdi
	mov [rsp + registers.rsi], rsi
	mov [rsp + registers.r8], r8
	mov [rsp + registers.r9], r9
	mov [rsp + registers.r10], r10
	mov [rsp + registers.r11], r11
	mov [rsp + registers.r12], r12
	mov [rsp + registers.r13], r13
	mov [rsp + registers.r14], r14
	mov [rsp + registers.r15], r15
%endmacro

; first arg is pointer to int_data structure
%macro save_int_data_regs 1
	; move data from stack frame into registers
	mov rax, [%1 + int_data.rip]
	mov [rsp + registers.rip], rax
	mov rax, [%1 + int_data.cs]
	mov [rsp + registers.cs], ax
	mov rax, [%1 + int_data.rflags]
	mov [rsp + registers.rflags], rax
	mov rax, [%1 + int_data.rsp]
	mov [rsp + registers.rsp], rax
	mov rax, [%1 + int_data.ss]
	mov [rsp + registers.ss], ax
%endmacro

; registers data structure should be on stack
%macro load_old_regs 0
	mov rax, [rsp + registers.rax]
	mov rbx, [rsp + registers.rbx]
	mov rcx, [rsp + registers.rcx]
	mov rdx, [rsp + registers.rdx]
	mov rbp, [rsp + registers.rbp]
	mov rdi, [rsp + registers.rdi]
	mov rsi, [rsp + registers.rsi]
	mov r8, [rsp + registers.r8]
	mov r9, [rsp + registers.r9]
	mov r10, [rsp + registers.r10]
	mov r11, [rsp + registers.r11]
	mov r14, [rsp + registers.r14]
	mov r15, [rsp + registers.r15]
%endmacro

; TODO: don't have so much duplicated code with these interrupt handlers, maybe just call into 1 function

%macro make_asm_int_handler 1
global int_handler_ %+ %1
int_handler_ %+ %1 %+ :
	save_regs

	; get location of interrupt stack frame data structure
	mov r15, rsp
	add r15, registers_size

	save_int_data_regs r15

	; call rust function
	mov rdi, %1
	mov rsi, rsp
	mov rdx, 0
	mov rax, rust_int_handler
	; FIXME: stack might not be aligned
	call rax
	
	load_old_regs

	add rsp, registers_size
	iretq
%endmacro

%macro make_asm_int_handler_e 1
global int_handler_ %+ %1
int_handler_ %+ %1 %+ :
	save_regs

	; get location of interrupt stack frame data structure
	mov r15, rsp
	add r15, registers_size

	; get error code
	mov rdx, [r15]
	add r15, 8

	save_int_data_regs r15

	; call rust function
	mov rdi, %1
	mov rsi, rsp
	mov rax, rust_int_handler
	; FIXME: stack might not be aligned
	call rax
	
	load_old_regs

	add rsp, registers_size
	add rsp, 8
	iretq
%endmacro

section .text
bits 64
make_asm_int_handler 0
make_asm_int_handler 1
make_asm_int_handler 2
make_asm_int_handler 3
make_asm_int_handler 4
make_asm_int_handler 5
make_asm_int_handler 6
make_asm_int_handler 7
make_asm_int_handler_e 8
make_asm_int_handler 9
make_asm_int_handler_e 10
make_asm_int_handler_e 11
make_asm_int_handler_e 12
make_asm_int_handler_e 13
make_asm_int_handler_e 14
make_asm_int_handler 15
make_asm_int_handler 16
make_asm_int_handler_e 17
make_asm_int_handler 18
make_asm_int_handler 19
make_asm_int_handler 20
make_asm_int_handler 21
make_asm_int_handler 22
make_asm_int_handler 23
make_asm_int_handler 24
make_asm_int_handler 25
make_asm_int_handler 26
make_asm_int_handler 27
make_asm_int_handler 28
make_asm_int_handler 29
make_asm_int_handler_e 30
make_asm_int_handler 31
make_asm_int_handler 32
make_asm_int_handler 33
make_asm_int_handler 34
make_asm_int_handler 35
make_asm_int_handler 36
make_asm_int_handler 37
make_asm_int_handler 38
make_asm_int_handler 39
make_asm_int_handler 40
make_asm_int_handler 41
make_asm_int_handler 42
make_asm_int_handler 43
make_asm_int_handler 44
make_asm_int_handler 45
make_asm_int_handler 46
make_asm_int_handler 47
make_asm_int_handler 48
make_asm_int_handler 49
make_asm_int_handler 50
make_asm_int_handler 51
make_asm_int_handler 52
make_asm_int_handler 53
make_asm_int_handler 54
make_asm_int_handler 55
make_asm_int_handler 56
make_asm_int_handler 57
make_asm_int_handler 58
make_asm_int_handler 59
make_asm_int_handler 60
make_asm_int_handler 61
make_asm_int_handler 62
make_asm_int_handler 63
make_asm_int_handler 64
make_asm_int_handler 65
make_asm_int_handler 66
make_asm_int_handler 67
make_asm_int_handler 68
make_asm_int_handler 69
make_asm_int_handler 70
make_asm_int_handler 71
make_asm_int_handler 72
make_asm_int_handler 73
make_asm_int_handler 74
make_asm_int_handler 75
make_asm_int_handler 76
make_asm_int_handler 77
make_asm_int_handler 78
make_asm_int_handler 79
make_asm_int_handler 80
make_asm_int_handler 81
make_asm_int_handler 82
make_asm_int_handler 83
make_asm_int_handler 84
make_asm_int_handler 85
make_asm_int_handler 86
make_asm_int_handler 87
make_asm_int_handler 88
make_asm_int_handler 89
make_asm_int_handler 90
make_asm_int_handler 91
make_asm_int_handler 92
make_asm_int_handler 93
make_asm_int_handler 94
make_asm_int_handler 95
make_asm_int_handler 96
make_asm_int_handler 97
make_asm_int_handler 98
make_asm_int_handler 99
make_asm_int_handler 100
make_asm_int_handler 101
make_asm_int_handler 102
make_asm_int_handler 103
make_asm_int_handler 104
make_asm_int_handler 105
make_asm_int_handler 106
make_asm_int_handler 107
make_asm_int_handler 108
make_asm_int_handler 109
make_asm_int_handler 110
make_asm_int_handler 111
make_asm_int_handler 112
make_asm_int_handler 113
make_asm_int_handler 114
make_asm_int_handler 115
make_asm_int_handler 116
make_asm_int_handler 117
make_asm_int_handler 118
make_asm_int_handler 119
make_asm_int_handler 120
make_asm_int_handler 121
make_asm_int_handler 122
make_asm_int_handler 123
make_asm_int_handler 124
make_asm_int_handler 125
make_asm_int_handler 126
make_asm_int_handler 127
make_asm_int_handler 128
make_asm_int_handler 129
make_asm_int_handler 130
make_asm_int_handler 131
make_asm_int_handler 132
make_asm_int_handler 133
make_asm_int_handler 134
make_asm_int_handler 135
make_asm_int_handler 136
make_asm_int_handler 137
make_asm_int_handler 138
make_asm_int_handler 139
make_asm_int_handler 140
make_asm_int_handler 141
make_asm_int_handler 142
make_asm_int_handler 143
make_asm_int_handler 144
make_asm_int_handler 145
make_asm_int_handler 146
make_asm_int_handler 147
make_asm_int_handler 148
make_asm_int_handler 149
make_asm_int_handler 150
make_asm_int_handler 151
make_asm_int_handler 152
make_asm_int_handler 153
make_asm_int_handler 154
make_asm_int_handler 155
make_asm_int_handler 156
make_asm_int_handler 157
make_asm_int_handler 158
make_asm_int_handler 159
make_asm_int_handler 160
make_asm_int_handler 161
make_asm_int_handler 162
make_asm_int_handler 163
make_asm_int_handler 164
make_asm_int_handler 165
make_asm_int_handler 166
make_asm_int_handler 167
make_asm_int_handler 168
make_asm_int_handler 169
make_asm_int_handler 170
make_asm_int_handler 171
make_asm_int_handler 172
make_asm_int_handler 173
make_asm_int_handler 174
make_asm_int_handler 175
make_asm_int_handler 176
make_asm_int_handler 177
make_asm_int_handler 178
make_asm_int_handler 179
make_asm_int_handler 180
make_asm_int_handler 181
make_asm_int_handler 182
make_asm_int_handler 183
make_asm_int_handler 184
make_asm_int_handler 185
make_asm_int_handler 186
make_asm_int_handler 187
make_asm_int_handler 188
make_asm_int_handler 189
make_asm_int_handler 190
make_asm_int_handler 191
make_asm_int_handler 192
make_asm_int_handler 193
make_asm_int_handler 194
make_asm_int_handler 195
make_asm_int_handler 196
make_asm_int_handler 197
make_asm_int_handler 198
make_asm_int_handler 199
make_asm_int_handler 200
make_asm_int_handler 201
make_asm_int_handler 202
make_asm_int_handler 203
make_asm_int_handler 204
make_asm_int_handler 205
make_asm_int_handler 206
make_asm_int_handler 207
make_asm_int_handler 208
make_asm_int_handler 209
make_asm_int_handler 210
make_asm_int_handler 211
make_asm_int_handler 212
make_asm_int_handler 213
make_asm_int_handler 214
make_asm_int_handler 215
make_asm_int_handler 216
make_asm_int_handler 217
make_asm_int_handler 218
make_asm_int_handler 219
make_asm_int_handler 220
make_asm_int_handler 221
make_asm_int_handler 222
make_asm_int_handler 223
make_asm_int_handler 224
make_asm_int_handler 225
make_asm_int_handler 226
make_asm_int_handler 227
make_asm_int_handler 228
make_asm_int_handler 229
make_asm_int_handler 230
make_asm_int_handler 231
make_asm_int_handler 232
make_asm_int_handler 233
make_asm_int_handler 234
make_asm_int_handler 235
make_asm_int_handler 236
make_asm_int_handler 237
make_asm_int_handler 238
make_asm_int_handler 239
make_asm_int_handler 240
make_asm_int_handler 241
make_asm_int_handler 242
make_asm_int_handler 243
make_asm_int_handler 244
make_asm_int_handler 245
make_asm_int_handler 246
make_asm_int_handler 247
make_asm_int_handler 248
make_asm_int_handler 249
make_asm_int_handler 250
make_asm_int_handler 251
make_asm_int_handler 252
make_asm_int_handler 253
make_asm_int_handler 254
make_asm_int_handler 255