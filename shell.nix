{ pkgs ? import <nixpkgs> {} }:
  pkgs.mkShell {
    nativeBuildInputs = with pkgs.buildPackages; [
      lld_18
      parted
      grub2
      qemu_full
      nasm
    ];
  }
