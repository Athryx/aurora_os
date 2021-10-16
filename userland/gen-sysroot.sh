#!/bin/sh

if [[ -e $SYSROOT ]]
then
	mkdir -p .cargo
cat > .cargo/config.toml <<EOF
[build]
target = "$TARGET"
rustflags = ["--sysroot", "$SYSROOT"]
EOF
else
	cargo sysroot --target $TARGET --sysroot-dir $SYSROOT
fi
