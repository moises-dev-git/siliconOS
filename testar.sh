#!/bin/bash
# Runner do `cargo test`: recebe o .efi de teste, roda no QEMU sem janela e
# traduz o código de saída do isa-debug-exit (33 = sucesso, 35 = falha).
#   cargo test --target x86_64-unknown-uefi
set -u
cd "$(dirname "$0")"
EFI="$1"
mkdir -p target/esp-teste/EFI/BOOT
cp -f "$EFI" target/esp-teste/EFI/BOOT/BOOTX64.efi

timeout 60 qemu-system-x86_64 -bios /usr/share/ovmf/OVMF.fd \
    -drive format=raw,file=fat:rw:target/esp-teste -net none -no-reboot \
    -display none -serial stdio \
    -device isa-debug-exit,iobase=0xf4,iosize=0x04
codigo=$?
case $codigo in
    33)  exit 0 ;;                                  # Saida::Sucesso
    35)  echo "testar.sh: o kernel relatou FALHA"; exit 1 ;;
    124) echo "testar.sh: tempo esgotado (60 s)"; exit 2 ;;
    *)   echo "testar.sh: QEMU saiu com codigo $codigo"; exit 3 ;;
esac
