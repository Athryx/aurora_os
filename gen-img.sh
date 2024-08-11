#!/bin/sh

# kernel.bin in arg 1, initrd in arg 2, output image name in arg 3

cd $(dirname $0)

IMG="$3"
BOOT_DIR="boot"
PART_NUM="p1"
DEV0="/dev/loop0"
LOOP=""
MNT=""
MNT_DIR="$(pwd)/mnt"

echo "generating disk image..."

sudo modprobe loop || exit 1

rm -f $IMG
rm -f $BOOT_DIR/kernel.bin
cp $1 $BOOT_DIR/kernel.bin
cp $2 $BOOT_DIR/initrd

dd if=/dev/zero of=$IMG bs=516096 count=160 || exit 1

sudo losetup $DEV0 $IMG || exit 1
LOOP="1"

sudo mkdir -p $MNT_DIR

cleanup () {
	if [ -n $MNT ]
	then
		sudo umount $MNT_DIR || ( sleep 1 && sync && sudo umount $MNT_DIR)
	fi

	if [ -n $LOOP ]
	then
		sudo losetup -d $DEV0
	fi

	sudo rmdir $MNT_DIR
}
trap cleanup EXIT

sudo parted -s $DEV0 mklabel msdos mkpart primary ext2 1M 100% -a minimal set 1 boot on || exit 1

sudo mke2fs $DEV0$PART_NUM || exit 1

sudo mount $DEV0$PART_NUM $MNT_DIR || exit 1
MNT="1"

sudo rm -rf $MNT_DIR/boot/
sudo cp -r $BOOT_DIR $MNT_DIR/boot

# NOTE: --root-directory has to be an absolute path
# it seems to not even load any files from the root direcotory (kernel and initrd) and silently fail if it is not an absolute path
sudo grub-install --root-directory=$MNT_DIR --no-floppy --target="i386-pc" --modules="normal part_msdos ext2 multiboot" $DEV0 || exit 1

echo "done"
exit 0
