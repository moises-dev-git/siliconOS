#!/usr/bin/env bash
# Gera o site (português na raiz, inglês em /en/) e publica no Cloudflare Pages.
# Builds the site (Portuguese at the root, English under /en/) and publishes it.
# Uso / usage: ./site/publicar.sh        (de qualquer pasta / from any folder)
set -euo pipefail
cd "$(dirname "$0")"
python3 indice_funcoes.py   # docs*/funcoes.md a partir das tabelas / from the tables
mdbook build          # pt-BR -> book/
mdbook build en       # en    -> book/en/
wrangler pages deploy book --project-name siliconos --branch main --commit-dirty=true
