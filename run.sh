#!/bin/bash
# Compila o SiliconOS, copia para a partição EFI e roda no QEMU.
#   ./run.sh                 -> roda normalmente (janela + serial no terminal)
#   ./run.sh teste-panic     -> compila com a feature que provoca um panic
#   SEM_TELA=1 ./run.sh      -> sem janela, serial em serial.log, encerra após 15 s
#   RAM=4G ./run.sh          -> muda a quantidade de memória da máquina (padrão do QEMU: 128M)
set -e
cd "$(dirname "$0")"

FEATURES=""
[ -n "$1" ] && FEATURES="--features $1"

cargo build --target x86_64-unknown-uefi $FEATURES
mkdir -p esp/EFI/BOOT
cp -f target/x86_64-unknown-uefi/debug/siliconos.efi esp/EFI/BOOT/BOOTX64.efi

QEMU_ARGS=(-bios /usr/share/ovmf/OVMF.fd
           -drive format=raw,file=fat:rw:esp
           -net none
           -no-reboot)   # se a CPU resetar, o QEMU fecha em vez de reiniciar em loop
[ -n "$RAM" ] && QEMU_ARGS+=(-m "$RAM")

if [ -n "$SEM_TELA" ]; then
    rm -f serial.log
    timeout 15 qemu-system-x86_64 "${QEMU_ARGS[@]}" -display none -serial file:serial.log || true
    cat serial.log
else
    qemu-system-x86_64 "${QEMU_ARGS[@]}" -serial stdio
fi
