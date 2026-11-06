#!/bin/bash
# Roda o SiliconOS no QEMU com o servidor do GDB ligado e abre o GDB já
# conectado e com os nomes das funções (docs/09-shell-e-testes.md).
#   ./depurar.sh                 janela do QEMU + GDB neste terminal
#   ./depurar.sh teste-timer     idem, com uma feature de teste
#   SEM_TELA=1 ./depurar.sh      sem janela (para scripts)
# No GDB, por exemplo:
#   break siliconos::shell::executar
#   continue          (digite um comando na janela do QEMU)
#   bt 2 / info registers / x/8i $pc / finish / continue
set -e
cd "$(dirname "$0")"

FEATURES=""
[ -n "${1:-}" ] && FEATURES="--features $1"

# /debug:symtab: o linker grava a tabela de símbolos (nomes das funções)
# dentro do .efi. Pasta separada para não misturar com a compilação normal.
RUSTFLAGS="-C link-arg=/debug:symtab" \
    cargo build --target x86_64-unknown-uefi --target-dir target/simbolos $FEATURES
EFI=target/simbolos/x86_64-unknown-uefi/debug/siliconos.efi
mkdir -p target/esp-depurar/EFI/BOOT
cp -f "$EFI" target/esp-depurar/EFI/BOOT/BOOTX64.efi

SERIAL=target/depurar-serial.log
rm -f "$SERIAL"
TELA=(); [ -n "${SEM_TELA:-}" ] && TELA=(-display none)
# -s: servidor do GDB na porta TCP 1234.
qemu-system-x86_64 -bios /usr/share/ovmf/OVMF.fd \
    -drive format=raw,file=fat:rw:target/esp-depurar -net none -no-reboot \
    "${TELA[@]}" -serial file:"$SERIAL" -s &
QEMU=$!
trap 'kill $QEMU 2>/dev/null' EXIT

# O UEFI carrega o .efi num endereço que só se sabe em tempo de execução;
# o kernel o imprime logo no começo ("Imagem do kernel: base 0x...").
for _ in $(seq 60); do
    BASE=$(grep -ao "Imagem do kernel: base 0x[0-9a-f]*" "$SERIAL" 2>/dev/null | grep -o "0x[0-9a-f]*" || true)
    [ -n "$BASE" ] && break
    sleep 0.5
done
[ -z "$BASE" ] && { echo "depurar.sh: o kernel nao informou a base"; exit 1; }
echo "Kernel carregado em $BASE (no arquivo: 0x140000000). Saida serial em $SERIAL"

# Os endereços do arquivo partem de 0x140000000; os da memória, de BASE.
cat > target/simbolos/gdb.txt <<FIM
target remote :1234
add-symbol-file $EFI -o $BASE-0x140000000
FIM
gdb -q -x target/simbolos/gdb.txt
