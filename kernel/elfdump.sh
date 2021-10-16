#!/bin/sh

cd $(dirname $0)

readelf -e kernel.bin > temp.txt
