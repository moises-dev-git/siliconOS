#!/bin/bash

# Defina os caminhos de origem e destino aqui
ORIGEM="target/x86_64-unknown-uefi/debug/siliconos.efi"
DESTINO="esp/EFI/BOOT/BOOTX64.efi"

# Executa a cópia forçada
cp -f "$ORIGEM" "$DESTINO"

# Verifica se o comando anterior foi executado com sucesso
if [ $? -eq 0 ]; then
    echo "Arquivo copiado e substituído com sucesso!"
else
    echo "Erro ao copiar o arquivo."
fi

