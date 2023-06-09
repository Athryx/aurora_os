#!/bin/sh

IMG="disk.img"
SUBDIRS="kernel userland"
KERNEL="kernel/kernel.bin"
INITRD="userland/initrd"

cd $(dirname $0)

for SUBDIR in $SUBDIRS
do
	if ! $SUBDIR/build.sh $1
	then
		echo "$SUBDIR build failed"
		exit 1
	fi
done

if [[ $1 != sysroot ]] && [[ $1 != clean ]] && [[ $1 != fmt ]]
then
	if [[ $KERNEL -nt $IMG ]] || [[ $INITRD -nt $IMG ]]
	then
		./gen-img.sh $KERNEL $INITRD $IMG
	fi
fi

if [[ $1 = debug ]]
then
	# FIXME: use $TERM environment variable instead of konsole
	qemu-system-x86_64 -m 5120 -smp cpus=4,cores=4 -debugcon stdio -s -S -drive file=$IMG,format=raw & konsole -e "$HOME/.cargo/bin/rust-gdb" "--nh" "-x" "debug.gdb"
elif [[ $1 = release ]] && [[ $2 = debug ]]
then
	qemu-system-x86_64 -m 5120 -debugcon stdio -s -S -drive file=$IMG,format=raw & $TERM -e "$HOME/.cargo/bin/rust-gdb" "--nh" "-x" "debug-release.gdb"
elif [[ $1 = bochs ]]
then
	konsole -e bochs -f bochsrc
elif [[ -z $1 ]] || [[ $1 = release ]] || [[ $1 = test ]]
then
	qemu-system-x86_64 -m 5120 -smp cpus=4,cores=4 -debugcon stdio -drive file=$IMG,format=raw
fi
