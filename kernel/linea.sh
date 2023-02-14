#!/bin/sh

cd $(dirname $0)

addr2line -e ../boot/kernel.bin $1
