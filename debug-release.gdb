set disassembly-flavor intel
add-symbol-file kernel/target/x86_64-os/release/kernel
break _start
target remote localhost:1234
layout asm
layout next
