#!/bin/sh

# This script is called in userland build scripts to generate the sysroot
# or include it in project if it already exists

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

exit 0
