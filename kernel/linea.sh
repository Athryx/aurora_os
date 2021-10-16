#!/bin/sh

cd $(dirname $0)

addr2line -e iso/boot/kernel.bin $1
