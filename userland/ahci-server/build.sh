#!/bin/sh

OUT_BIN=ahci-server.bin

cd $(dirname $0)
[[ $1 = clean ]] && { cargo clean; rm -rf .cargo; exit 0; }
# in kernel, this only needs to be cargo sysroot
[[ $1 = sysroot ]] && { $GEN_SYSROOT; exit 0; }
[[ $1 = fmt ]] && { cargo fmt; exit 0; }
[[ $1 = release ]] && RFLAG=--release

if [[ $1 = test ]]
then
  # complicated command to determine where test is
  IMG=$(cargo test --no-run --message-format=json 2> /dev/null | jq 'select(.executable) | .executable' | cut -d '"' -f 2)
else
  cargo build $RFLAG || exit 1

  # Change these to x86_64-os-kernel for kernel
  IMG=target/x86_64-os-userland/debug/kernel
  [[ $1 = release ]] && IMG=target/x86_64-os-userland/release/kernel
fi

echo $IMG
if [[ $IMG -nt $OUT_BIN ]]
then
  cp $IMG $OUT_BIN
fi

exit 0
