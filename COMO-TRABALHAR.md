# Como trabalhar no SiliconOS

Guia rápido: o que cada script faz, como publicar uma mudança no site e como
fazer commit. Todos os comandos rodam na pasta do projeto
(`~/Área de trabalho/Dev/Rust/siliconos`).

---

## 1. Os scripts

| Script | O que faz | Quando usar |
|---|---|---|
| `./run.sh` | Compila o kernel, copia o `.efi` para `esp/EFI/BOOT/BOOTX64.efi` e abre o QEMU (janela + saída serial no terminal). | Para ver o sistema rodando. |
| `./run.sh teste-panic` | O mesmo, mas compilando com uma *feature* de teste (veja a lista em `Cargo.toml` ou em `docs/README.md`). | Para ver um cenário de teste (panic, page fault, timer...). |
| `SEM_TELA=1 ./run.sh` | Sem janela: grava a serial em `serial.log`, espera 15 s, fecha o QEMU e mostra o log. | Para conferir a saída do boot rapidamente. |
| `RAM=4G ./run.sh` | Muda a memória da máquina virtual (padrão do QEMU: 128M). | Combina com os outros. |
| `./depurar.sh` | Compila com os nomes das funções, abre o QEMU com o servidor do GDB e abre o GDB já conectado. | Para parar o kernel num ponto e olhar registradores, pilha etc. (doc 09). |
| `./testar.sh` | **Não é para rodar à mão.** É o "runner" que o `cargo test` chama: roda o `.efi` de teste no QEMU sem janela e traduz o código de saída (33 = passou, 35 = falhou). | Indiretamente, via `cargo test` (abaixo). |
| `./site/publicar.sh` | Gera o índice de funções e os links do código para o GitHub, monta o site em português e em inglês (`mdbook build`) e publica no Cloudflare Pages (`wrangler pages deploy`). | Sempre que mudar algo em `docs/` ou `docs-en/`. |
| `./copiar_para_esp.sh` | Só copia o `.efi` compilado para `esp/`. | Sobra do começo do projeto: o `run.sh` já faz isso. Pode ser apagado. |

`comando.txt` também é anotação antiga (os comandos do QEMU que hoje estão
dentro do `run.sh`).

### Testes automáticos

```bash
cargo test --target x86_64-unknown-uefi                              # todos os testes
cargo test --target x86_64-unknown-uefi --features teste-que-falha   # mostra como é uma falha
```

---

## 2. Mudar o texto do site e publicar

O site (https://siliconos.pages.dev) é gerado a partir dos arquivos `.md`.

1. **Edite o `.md`**, nunca o HTML:
   - português: `docs/NN-nome.md`
   - inglês: `docs-en/NN-nome.md`
2. **Publique:**
   ```bash
   ./site/publicar.sh
   ```
3. Confira no navegador (às vezes é preciso Ctrl+Shift+R para escapar do cache).

**O que não editar à mão:**

- `site/book/` é a saída gerada: o `publicar.sh` apaga e recria tudo lá a cada
  publicação.
- `docs/funcoes.md` e `docs-en/funcoes.md` são gerados por
  `site/indice_funcoes.py` a partir das tabelas "Funções deste capítulo" de
  cada capítulo. Para mudar uma frase do índice, mude a tabela no capítulo.

**Para ver o site sem publicar** (servidor local, atualiza sozinho ao salvar):

```bash
cd site && python3 simbolos.py && mdbook serve --open   # português em http://localhost:3000
```

**Links do código para o GitHub:** nos trechos de código do site, os nomes
de funções, tipos, statics e macros do projeto (por exemplo `Serial::init`,
`CONSOLE`, `kernel_main`) viram links para a linha onde estão definidos no
GitHub. Dois arquivos cuidam disso:

- `site/simbolos.py` lê o código do último commit e gera `site/simbolos.js`
  (nome → arquivo e linha). Ele é gerado, por isso fica fora do git.
- `site/codigo.js` roda no navegador e transforma os nomes em links. Um nome
  só vira link quando não há dúvida de qual é a definição: os `init` soltos,
  por exemplo, não viram (`Serial::init` vira).

Os links apontam para o **último commit**. Por isso, quando mudar código,
faça commit **e push antes** de publicar o site: o `simbolos.py` avisa se o
commit ainda não está no GitHub (os links dariam erro 404) ou se há código
sem commit (as linhas poderiam não bater).

**Se o `wrangler` pedir login** (primeira vez, ou depois de muito tempo):

```bash
wrangler login
```

**Cuidados com títulos (`##`):** o endereço de cada seção (`#5-por-que-...`)
vem do texto do título. Mudar uma palavra do título muda o endereço e quebra
os links que apontam para ele. Pontuação (`?`, `:`) e crases não contam.

---

## 3. Commit e envio para o GitHub

O repositório é https://github.com/moises-dev-git/siliconOS (branch `main`).

```bash
git status                     # o que mudou
git diff                       # as mudanças, linha a linha
git add docs/00-visao-geral.md # escolha os arquivos (ou: git add -A para todos)
git commit -m "Docs: título da seção 5 do doc 00 vira pergunta"
git push                       # envia para o GitHub
```

O commit e a publicação no site são independentes: dá para publicar sem fazer
commit (o site fica à frente do GitHub) e vice-versa. O ideal é fazer os dois,
nesta ordem: commit, push e depois `./site/publicar.sh` (os links do código
apontam para o commit atual).

**Tags de capítulo:** cada capítulo tem uma tag `doc-NN` apontando para o
commit do código dele (os links "Código deste capítulo" usam essa tag). Ao
fechar um capítulo novo:

```bash
git tag doc-18
git push origin doc-18
```

Correções de texto depois disso **não** precisam mover a tag.

**O que fica fora do git** (`.gitignore`): `target/`, `site/book/`,
`serial.log`, `site/simbolos.js` (gerado), `CLAUDE.md` (notas locais) e backups.

---

## 4. Resumo de um minuto

| Quero... | Comando |
|---|---|
| ver o sistema rodar | `./run.sh` |
| rodar os testes | `cargo test --target x86_64-unknown-uefi` |
| depurar | `./depurar.sh` |
| guardar no GitHub | `git add ...` → `git commit -m "..."` → `git push` |
| publicar uma correção no site | editar `docs/*.md` → (commit e push) → `./site/publicar.sh` |
