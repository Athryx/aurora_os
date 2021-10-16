#!/bin/sh

SUBDIRS=""

cd $(dirname $0)

for SUBDIR in $SUBDIRS
do
	if ! $SUBDIR/build.sh $1
	then
		echo "$SUBDIR build failed"
		exit 1
	fi
done

[[ $1 = clean ]] && exit 0
[[ $1 = sysroot ]] && exit 0
[[ $1 = test ]] && exit 0
[[ $1 = fmt ]] && exit 0

#gen-initrd --ahci ahci-server/ahci-server.bin --init early-init/early-init.bin --fs fs-server/fs-server.bin --ext2 ext2-server/ext2-server.bin --part-list part-list -o initrd
