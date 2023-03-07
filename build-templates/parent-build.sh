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
[[ $1 = fmt ]] && exit 0

# Run any other scripts here

exit 0
