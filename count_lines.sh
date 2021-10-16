#!/bin/sh

cd $(dirname $0)
grep -v '^\s*$' $(tree -fi | grep -E '.*\.rs$|.*\.asm$' | grep -v '/old/' | grep -v 'build\.rs' | grep -v '/target/') | wc -l
