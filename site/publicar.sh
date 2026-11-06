#!/usr/bin/env bash
# Gera o site a partir de docs/ e publica no Cloudflare Pages (https://siliconos.pages.dev).
# Uso: ./site/publicar.sh        (de qualquer pasta)
set -euo pipefail
cd "$(dirname "$0")"
mdbook build
wrangler pages deploy book --project-name siliconos --branch main --commit-dirty=true
