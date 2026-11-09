#!/usr/bin/env python3
"""Gera site/simbolos.js: onde cada função, tipo, static, const e macro do
código está definida (arquivo e linha), para o site/codigo.js transformar os
nomes dos trechos de código em links para o GitHub.

Lê os arquivos do commit HEAD (não da pasta), para que as linhas batam com o
commit para onde os links apontam. Roda sozinho no ./site/publicar.sh.

Builds site/simbolos.js (where each function/type/static/const/macro is
defined) from the HEAD commit, so site/codigo.js can link names in code
blocks to GitHub. Run by ./site/publicar.sh.
"""
import json
import re
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

SITE = Path(__file__).resolve().parent
RAIZ = SITE.parent
REPO = "https://github.com/moises-dev-git/siliconOS"

# Nomes comuns demais (ou da biblioteca padrão) para virar link sem qualificador.
COMUNS = {
    "main", "new", "init", "drop", "fmt", "next", "default", "from", "into",
    "len", "get", "set", "push", "pop", "iter", "lock", "read", "write",
    "clone", "eq", "deref", "deref_mut", "index", "write_str", "alloc",
    "dealloc", "is_empty", "as_ptr", "as_str", "panic", "test", "run",
}


def git(*args):
    return subprocess.run(["git", *args], cwd=RAIZ, check=True,
                          capture_output=True, text=True).stdout


def limpar(texto):
    """Troca comentários, strings e caracteres por espaços (mantém as linhas)."""
    padrao = re.compile(
        r'//[^\n]*'                       # comentário de linha
        r'|/\*.*?\*/'                     # comentário de bloco
        r'|b?r(#*)".*?"\1'                # string crua r"..." / r#"..."#
        r'|b?"(?:\\.|[^"\\])*"'           # string normal
        r"|b?'(?:\\.|[^'\\])'",           # caractere 'x' (não pega lifetimes)
        re.S)
    return padrao.sub(lambda m: re.sub(r"[^\n]", " ", m.group(0)), texto)


DEF = re.compile(
    r"^\s*(?:#\[[^\]]*\]\s*)*(?:pub(?:\([^)]*\))?\s+)?"
    r"(?:(?:const|unsafe|async|extern\s*(?:\"[^\"]*\")?)\s+)*"
    r"(fn|struct|enum|trait|union|type|static(?:\s+mut)?|const|mod)\s+([A-Za-z_]\w*)")
MACRO = re.compile(r"^\s*(?:#\[[^\]]*\]\s*)*macro_rules!\s*([A-Za-z_]\w*)")
IMPL = re.compile(r"^\s*(?:unsafe\s+)?impl\b(?:\s*<[^{]*?>)?\s+(?:(.+?)\s+for\s+)?([A-Za-z_][\w:]*)")


def modulos_do_arquivo(caminho):
    """src/vfs/mod.rs -> ["vfs"]; src/vfs/sis.rs -> ["sis"]; lib.rs -> ["sos"]."""
    p = Path(caminho)
    if caminho == "programas/src/lib.rs":
        return ["sos"]
    if p.name in ("mod.rs", "main.rs", "lib.rs"):
        return [p.parent.name] if p.parent.name not in ("src",) else []
    return [p.stem]


def analisar(caminho, texto, defs):
    linhas = limpar(texto).split("\n")
    pilha = []          # (profundidade, contexto) de impl/mod abertos
    prof = 0
    for n, linha in enumerate(linhas, 1):
        local = f"{caminho}#L{n}"
        contexto = pilha[-1][1] if pilha else None
        m = MACRO.match(linha)
        if m:
            defs.append((m.group(1), "macro", None, local, caminho))
        m = DEF.match(linha)
        abre = None
        if m:
            tipo, nome = m.group(1).split()[0], m.group(2)
            if tipo == "mod":
                if "{" in linha:
                    abre = ("mod", nome)
            elif tipo == "fn" and contexto and contexto[0] == "impl":
                _, alvo, de_trait = contexto
                defs.append((nome, "metodo_trait" if de_trait else "metodo", alvo, local, caminho))
            elif tipo == "fn":
                mod = contexto[1] if contexto and contexto[0] == "mod" else None
                defs.append((nome, "fn", mod, local, caminho))
            else:
                if tipo == "trait" and "{" in linha:
                    abre = ("impl", nome, True)   # métodos do trait: como os de impl Trait
                defs.append((nome, "tipo" if tipo in ("struct", "enum", "trait", "union", "type") else "valor",
                             None, local, caminho))
        else:
            m = IMPL.match(linha)
            if m:
                alvo = m.group(2).split("::")[-1]
                abre = ("impl", alvo, m.group(1) is not None)
        for c in linha:
            if c == "{":
                prof += 1
                if abre:
                    pilha.append((prof, abre))
                    abre = None
            elif c == "}":
                if pilha and pilha[-1][0] == prof:
                    pilha.pop()
                prof -= 1


def main():
    commit = git("rev-parse", "HEAD").strip()
    arquivos = [a for a in git("ls-files", "src", "programas/src", "build.rs").split()
                if a.endswith(".rs")]
    sujos = git("status", "--porcelain", "--", "src", "programas/src", "build.rs").strip()
    if sujos:
        print("simbolos.py: AVISO: há mudanças sem commit no código; os links usam o "
              "último commit:\n" + sujos, file=sys.stderr)
    if not git("branch", "-r", "--contains", commit).strip():
        print(f"simbolos.py: AVISO: o commit {commit[:7]} ainda não está no GitHub; "
              "os links do site darão 404 até o git push.", file=sys.stderr)

    defs = []
    for caminho in arquivos:
        analisar(caminho, git("show", f"{commit}:{caminho}"), defs)

    qualificado = defaultdict(set)   # "Tipo::nome" / "modulo::nome"
    solto = defaultdict(set)         # "nome" (fn, tipo, valor, macro)
    metodo = defaultdict(set)        # ".nome(" (métodos próprios)
    for nome, tipo, dono, local, caminho in defs:
        if tipo in ("metodo", "metodo_trait"):
            qualificado[f"{dono}::{nome}"].add(local)
            if tipo == "metodo":
                metodo[nome].add(local)
            continue
        donos = [dono] if dono else modulos_do_arquivo(caminho)
        for d in donos:
            qualificado[f"{d}::{nome}"].add(local)
        solto[nome].add(local)
        if tipo == "fn":
            metodo.setdefault(nome, set())  # uma fn solta com o mesmo nome deixa o método ambíguo
            metodo[nome].add(local)

    def unicos(tabela, filtrar_comuns):
        return {k: next(iter(v)) for k, v in sorted(tabela.items())
                if len(v) == 1 and not (filtrar_comuns and k in COMUNS)}

    dados = {
        "base": f"{REPO}/blob/{commit}/",
        "q": unicos(qualificado, False),
        "s": unicos(solto, True),
        "m": unicos(metodo, True),
    }
    saida = SITE / "simbolos.js"
    saida.write_text("// GERADO por site/simbolos.py; não editar. / GENERATED; do not edit.\n"
                     "window.SIMBOLOS = " + json.dumps(dados, ensure_ascii=False,
                                                       separators=(",", ":")) + ";\n")
    print(f"simbolos.py: {len(defs)} definições, {len(dados['q'])} nomes qualificados, "
          f"{len(dados['s'])} soltos, commit {commit[:7]}")


if __name__ == "__main__":
    main()
