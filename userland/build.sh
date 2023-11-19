#!/bin/sh

cd $(dirname $0)

[[ $1 = clean ]] && { cargo clean; exit 0; }
[[ $1 = fmt ]] && { cargo fmt; exit 0; }
[[ $1 = release ]] && RFLAG=--release

if [[ $1 = test ]]
then
	# TODO
	exit 0
else
  cargo build $RFLAG || exit 1

  # Change these to x86_64-os-kernel for kernel
  TARGET_DIR=target/x86_64-os-userland/debug
  [[ $1 = release ]] && TARGET_DIR=target/x86_64-os-userland/release
fi

gen-initrd -n --init $TARGET_DIR/early-init --fs $TARGET_DIR/fs-server --hwaccess $TARGET_DIR/hwaccess-server --part-list part-list -o initrd

exit 0
