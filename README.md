# Aurora OS

## Build

install required tools for building

	sudo pacman -S nasm lld
	cargo install cargo-sysroot

the [gen-initrd](https://github.com/Athryx/gen-initrd) tool is also required to generate the init ramdisk
clone the [gen-initrd](https://github.com/Athryx/gen-initrd) repo, build it and put the gen-initrd executable in your path

set toolchain and build sysroot

	rustup override set nightly
	./run.sh sysroot

if building sysroot fails, you may have to install rust-src

	rustup component add rust-src

compile and run

	./run.sh

or compile and run in release mode

	./run.sh release
