#!/bin/sh

SUBDIRS="early-init ahci-server fs-server"

cd $(dirname $0)

# used by subdir build scripts
export TARGET=$(realpath x86_64-os-userland.json)
export SYSROOT=$(realpath sysroot)
export GEN_SYSROOT=$(realpath gen-sysroot.sh)

for SUBDIR in $SUBDIRS
do
	if ! $SUBDIR/build.sh $1
	then
		echo "$SUBDIR build failed"
		exit 1
	fi
done

[[ $1 = clean ]] && { rm -rf sysroot; exit 0; }
[[ $1 = sysroot ]] && exit 0
[[ $1 = fmt ]] && exit 0

gen-initrd --ahci ahci-server/ahci-server.bin --init early-init/early-init.bin --fs fs-server/fs-server.bin --part-list part-list -o initrd

exit 0
