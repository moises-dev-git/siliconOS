#!/usr/bin/env python3
"""Gera docs/funcoes.md e docs-en/funcoes.md (o índice de funções).

Lê a seção "Funções deste capítulo" ("Functions in this chapter") de cada
capítulo e junta todas as linhas num índice só, agrupado por arquivo e em
ordem alfabética. Roda sozinho no ./site/publicar.sh; não editar o .md gerado.

Builds the function index from the "Functions in this chapter" table of every
chapter, grouped by file, alphabetically. Run by ./site/publicar.sh; do not
edit the generated .md by hand.
"""
import re
import sys
from pathlib import Path

RAIZ = Path(__file__).resolve().parent.parent

IDIOMAS = {
    "pt": {
        "pasta": "docs",
        "secao": "## Funções deste capítulo",
        "ancora": "funções-deste-capítulo",
        "titulo": "Índice de funções",
        "intro": (
            "Todas as funções do kernel, agrupadas pelo arquivo onde moram e em ordem "
            "alfabética, cada uma com uma frase sobre o que faz e o capítulo que a criou. "
            "Para ver como as peças se encaixam, comece pelo [Mapa do SiliconOS](mapa.md).\n\n"
            "Esta página é **gerada automaticamente** (`site/indice_funcoes.py`) a partir das "
            "tabelas \"Funções deste capítulo\" de cada capítulo, toda vez que o site é publicado. "
            "Quando uma função mudou num capítulo posterior, a frase diz \"(desde o doc NN ...)\"; "
            "o código atual está sempre em [`src/`](https://github.com/moises-dev-git/siliconOS/tree/main/src)."
        ),
        "cab": "| Função | O que faz | Doc |",
        "testes": "Testes automáticos",
        "testes_intro": (
            "Os `#[test_case]` que rodam com `cargo test` (doc [09](09-shell-e-testes.md)), "
            "agrupados da mesma forma."
        ),
        "fora": "Programas de usuário (`programas/`) e compilação",
        "kernel": "Kernel (`src/`)",
        "total": "funções e {t} testes (ou ajudantes de teste), de {c} capítulos.",
    },
    "en": {
        "pasta": "docs-en",
        "secao": "## Functions in this chapter",
        "ancora": "functions-in-this-chapter",
        "titulo": "Function index",
        "intro": (
            "Every kernel function, grouped by the file it lives in and sorted "
            "alphabetically, each with one sentence about what it does and the chapter "
            "that created it. To see how the pieces fit together, start with the "
            "[SiliconOS map](mapa.md).\n\n"
            "This page is **generated automatically** (`site/indice_funcoes.py`) from the "
            "\"Functions in this chapter\" table of every chapter, each time the site is published. "
            "When a function changed in a later chapter, the sentence says \"(since doc NN ...)\"; "
            "the current code is always in [`src/`](https://github.com/moises-dev-git/siliconOS/tree/main/src)."
        ),
        "cab": "| Function | What it does | Doc |",
        "testes": "Automated tests",
        "testes_intro": (
            "The `#[test_case]`s that run with `cargo test` (doc [09](09-shell-e-testes.md)), "
            "grouped the same way."
        ),
        "fora": "User programs (`programas/`) and build",
        "kernel": "Kernel (`src/`)",
        "total": "functions and {t} tests (or test helpers), from {c} chapters.",
    },
}

LINHA = re.compile(r"^\|\s*(.+?)\s*\|\s*\[([^\]]+)\]\(([^)]+)\)\s*\|\s*(.+?)\s*\|\s*$")


def caminho_do_arquivo(url: str, texto: str) -> str:
    """Caminho do arquivo no repositório, a partir do link para o GitHub."""
    m = re.search(r"/blob/[^/]+/(.+)$", url)
    return m.group(1) if m else texto


def ler_capitulo(arq: Path, secao: str):
    funcoes, testes = [], []
    dentro, tabela = False, 0
    for linha in arq.read_text(encoding="utf-8").splitlines():
        if linha.startswith("## "):
            dentro = linha.strip() == secao
            tabela = 0
            continue
        if not dentro:
            continue
        if linha.startswith("|---"):
            tabela += 1
            continue
        m = LINHA.match(linha)
        if m and tabela:
            nome, texto, url, desc = m.groups()
            item = (nome, caminho_do_arquivo(url, texto), desc)
            (funcoes if tabela == 1 else testes).append(item)
    return funcoes, testes


def chave_nome(nome: str) -> str:
    return re.sub(r"[`\\]", "", nome).lower()


def tabela(itens, cab):
    out = [cab, "|---|---|---|"]
    for nome, desc, doc, pagina, ancora in sorted(itens, key=lambda i: (chave_nome(i[0]), i[2])):
        out.append(f"| {nome} | {desc} | [{doc}]({pagina}#{ancora}) |")
    return out


def gerar(idioma: str) -> tuple[int, int]:
    cfg = IDIOMAS[idioma]
    pasta = RAIZ / cfg["pasta"]
    capitulos = sorted(p for p in pasta.glob("[0-9][0-9]-*.md"))
    por_arquivo: dict[str, list] = {}
    testes_por_arquivo: dict[str, list] = {}
    usados = 0
    for cap in capitulos:
        funcoes, testes = ler_capitulo(cap, cfg["secao"])
        if funcoes or testes:
            usados += 1
        doc = cap.name[:2]
        for destino, lista in ((por_arquivo, funcoes), (testes_por_arquivo, testes)):
            for nome, arquivo, desc in lista:
                destino.setdefault(arquivo, []).append((nome, desc, doc, cap.name, cfg["ancora"]))

    def ordem(arquivo: str):
        # src/ primeiro (main.rs no topo), depois o resto.
        return (not arquivo.startswith("src/"), arquivo != "src/main.rs", arquivo)

    n_funcoes = sum(len(v) for v in por_arquivo.values())
    n_testes = sum(len(v) for v in testes_por_arquivo.values())
    out = [
        "<!-- GERADO por site/indice_funcoes.py; não editar à mão. / GENERATED; do not edit. -->",
        "",
        f"# {cfg['titulo']}",
        "",
        cfg["intro"],
        "",
        f"{n_funcoes} " + cfg["total"].format(t=n_testes, c=usados),
        "",
    ]
    secao_atual = None
    for arquivo in sorted(por_arquivo, key=ordem):
        secao = cfg["kernel"] if arquivo.startswith("src/") else cfg["fora"]
        if secao != secao_atual:
            out += ["---", "", f"## {secao}", ""]
            secao_atual = secao
        url = f"https://github.com/moises-dev-git/siliconOS/blob/main/{arquivo}"
        out += [f"### [{arquivo}]({url})", ""]
        out += tabela(por_arquivo[arquivo], cfg["cab"])
        out.append("")
    out += ["---", "", f"## {cfg['testes']}", "", cfg["testes_intro"], ""]
    for arquivo in sorted(testes_por_arquivo, key=ordem):
        out += [f"### {arquivo}", ""]
        out += tabela(testes_por_arquivo[arquivo], cfg["cab"])
        out.append("")
    (pasta / "funcoes.md").write_text("\n".join(out), encoding="utf-8")
    return n_funcoes, n_testes


if __name__ == "__main__":
    for idioma in IDIOMAS:
        f, t = gerar(idioma)
        print(f"{idioma}: {f} funções, {t} testes", file=sys.stderr)
