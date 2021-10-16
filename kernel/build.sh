#!/bin/sh

cd $(dirname $0)
[[ $1 = clean ]] && { cargo clean; exit 0; }
[[ $1 = sysroot ]] && { cargo sysroot; exit 0; }
[[ $1 = test ]] && { cargo test; exit 0; }
[[ $1 = fmt ]] && { cargo fmt; exit 0; }
[[ $1 = release ]] && RFLAG=--release

cargo build $RFLAG || exit 1

IMG=target/x86_64-os-kernel/debug/kernel
[[ $1 = release ]] && IMG=target/x86_64-os-kernel/release/kernel

cp $IMG kernel.bin

exit 0
