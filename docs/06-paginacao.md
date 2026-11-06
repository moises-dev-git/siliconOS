# 06: Paginação: endereços virtuais e as tabelas do kernel

> **Você vai entender:** a diferença entre endereço virtual e endereço
> físico; como a CPU traduz um no outro usando 4 níveis de tabelas; por que
> isso é a base de toda a proteção de memória de um sistema operacional; e
> como o SiliconOS troca as tabelas do firmware pelas suas próprias sem
> derrubar a si mesmo no meio do caminho.
>
> **Arquivo:** `src/memoria/paginacao.rs` (chamado em `kernel_main` depois do alocador de quadros)
> **Livro:** capítulo 7
> **Testar:** `./run.sh teste-paginacao`, `./run.sh teste-desmapeado`, `./run.sh teste-nulo`
> (e com mais memória: `RAM=8G ./run.sh teste-paginacao`)
> **Leia antes:** doc 05 (quadros e alocador)
> **Rust novo aqui:** o operador `?` ([§24](rust-basico.md#24-o-operador--propagar-erros)),
> atômicos ([§25](rust-basico.md#25-atômicos-atomicu64-atomicbool)),
> `#[repr(transparent)]` ([§13](rust-basico.md#13-atributos--e-))
>
> **Código deste capítulo:** os docs 00 a 06 estão num único commit: [estado no fim do doc 06](https://github.com/moises-dev-git/siliconOS/tree/doc-06). O que este capítulo acrescenta está em [Mudanças nos arquivos](#mudanças-nos-arquivos).

---

## 1. A ideia: todo ponteiro passa por um "tradutor"

Quando o seu programa Java lê um campo de um objeto, a JVM usa um endereço
de memória, digamos `0x7f3a_1000`. Se você abrir dois programas ao mesmo
tempo, **os dois podem usar exatamente o mesmo endereço** `0x7f3a_1000` sem
que um enxergue os dados do outro. Como?

Porque esse endereço **não é a posição real na RAM**. É um **endereço
virtual**. A CPU, a cada acesso à memória, o traduz para um **endereço
físico** (a posição de verdade nos chips de RAM), usando um conjunto de
tabelas que o **kernel** monta:

```
 programa A:  ponteiro 0x7f3a_1000 ──► tabelas do programa A ──► físico 0x0230_5000
 programa B:  ponteiro 0x7f3a_1000 ──► tabelas do programa B ──► físico 0x0811_2000
```

Esse mecanismo, chamado **paginação**, resolve de uma vez vários problemas:

| Problema | Como a paginação resolve |
|---|---|
| Isolamento: um programa não pode ler a memória de outro | cada programa tem tabelas próprias; o que não está nas tabelas dele simplesmente não existe para ele |
| Proteger o kernel | as páginas do kernel são marcadas "só anel 0"; um programa que tentar acessá-las recebe page fault |
| Memória física fragmentada (doc 05) | o programa vê um bloco contínuo de endereços virtuais, montado com quadros espalhados pela RAM |
| Detectar bugs | uma página sem mapeamento gera page fault: ponteiros nulos e pilhas estouradas são pegos na hora |
| Carregar sob demanda, swap, `mmap`... | o kernel mapeia a página só quando ela é usada pela primeira vez (o page fault avisa) |

A unidade da tradução é a **página**: um bloco de 4 KiB de endereços
virtuais, que aponta para um **quadro** de 4 KiB de RAM física (doc 05). Os
12 bits mais baixos do endereço (o deslocamento dentro da página, de 0 a
4095) passam direto; só o resto é traduzido.

### O que o SiliconOS já usava sem saber

A paginação **é obrigatória** no modo de 64 bits: não existe x86-64 sem ela.
Desde o primeiro boot, o SiliconOS rodava com tabelas que o **UEFI** montou.
Elas estavam em **mapeamento identidade**: virtual X → físico X. Por isso
até aqui pudemos tratar endereço físico e ponteiro como a mesma coisa (doc
05, seção 4.4).

---

## 2. Como a tradução funciona: 4 níveis de tabelas

### 2.1 Por que não uma tabela só

Um endereço virtual em x86-64 tem 48 bits úteis: 256 TiB de espaço. Com
páginas de 4 KiB, são 2³⁶ = 68 bilhões de páginas. Uma tabela simples, com
uma entrada de 8 bytes por página, teria **512 GiB**. Impossível.

A solução é uma **árvore**: tabelas pequenas (uma por quadro de 4 KiB), em
4 níveis, e só se criam as partes da árvore que são usadas. Um programa que
usa pouca memória precisa de poucas tabelas.

### 2.2 O endereço virtual é picado em 5 pedaços

```
 endereço virtual (64 bits)
 ┌────────────┬─────────┬─────────┬─────────┬─────────┬──────────────┐
 │ 63 ... 48  │ 47...39 │ 38...30 │ 29...21 │ 20...12 │  11 ... 0    │
 │ cópia do   │ índice  │ índice  │ índice  │ índice  │ deslocamento │
 │ bit 47     │ PML4    │ PDPT    │ PD      │ PT      │ na página    │
 │ (canônico) │ 9 bits  │ 9 bits  │ 9 bits  │ 9 bits  │ 12 bits      │
 └────────────┴─────────┴─────────┴─────────┴─────────┴──────────────┘
```

Cada índice tem 9 bits, ou seja, vai de 0 a 511. Cada tabela tem **512
entradas de 8 bytes = 4096 bytes**: exatamente um quadro. Não é
coincidência: o formato foi desenhado para isso.

Os quatro níveis têm nomes herdados dos manuais da Intel:

| Nível | Nome | Cada entrada cobre |
|---|---|---|
| 4 | **PML4** (*Page Map Level 4*) | 512 GiB |
| 3 | **PDPT** (*Page Directory Pointer Table*) | 1 GiB |
| 2 | **PD** (*Page Directory*) | 2 MiB |
| 1 | **PT** (*Page Table*) | 4 KiB (uma página) |

No código:

```rust
pub fn indices(virt: u64) -> [usize; 4] {
    [
        ((virt >> 39) & 0x1FF) as usize, // PML4: bits 39–47
        ((virt >> 30) & 0x1FF) as usize, // PDPT: bits 30–38
        ((virt >> 21) & 0x1FF) as usize, // PD:   bits 21–29
        ((virt >> 12) & 0x1FF) as usize, // PT:   bits 12–20
    ]
}
```

`>> 39` desloca o endereço para que o pedaço desejado fique nos bits mais
baixos, e `& 0x1FF` (= 511 = nove bits 1) descarta o resto.

### 2.3 A caminhada (*page walk*)

O registrador **CR3** guarda o endereço **físico** da PML4. A cada acesso à
memória, a CPU faz:

```
 CR3 ──► PML4 ─[índice PML4]─► entrada: "a PDPT está no físico X"
                                  │
          PDPT (em X) ─[índice PDPT]─► entrada: "a PD está no físico Y"
                                  │
          PD (em Y) ─[índice PD]──► entrada: "a PT está no físico Z"
                                  │
          PT (em Z) ─[índice PT]──► entrada: "a página está no quadro Q"
                                  │
          físico final = Q + deslocamento (12 bits de baixo do virtual)
```

Se, em qualquer nível, a entrada estiver marcada como **não presente**, a
CPU para e gera um **page fault** (doc 04), com o endereço virtual em CR2.

O SiliconOS tem essa mesma caminhada em software, na função `traduzir_em`. Ela
não é usada pela CPU (a CPU faz isso em hardware); serve para o kernel
**consultar** o mapeamento e para os relatórios e testes:

```rust
pub fn traduzir_em(raiz: u64, virt: u64) -> Option<Traducao> {
    if !canonico(virt) { return None; }
    let idx = indices(virt);
    let mut tabela = raiz;
    for nivel in 0..4 {
        let e = tabela_em(tabela).entradas[idx[nivel]];
        if !e.presente() { return None; }
        let tamanho = match nivel {
            1 if e.grande() => Some(TamanhoPagina::G1),
            2 if e.grande() => Some(TamanhoPagina::M2),
            3 => Some(TamanhoPagina::K4),
            _ => None,                          // ainda não chegou: desce mais um nível
        };
        if let Some(t) = tamanho {
            let mascara_pagina = t.bytes() - 1;
            let base = e.endereco() & !mascara_pagina;
            return Some(Traducao { fisico: base + (virt & mascara_pagina), tamanho: t, flags: e.flags() });
        }
        tabela = e.endereco();                  // o próximo nível
    }
    None
}
```

(`1 if e.grande() =>` é um `match` com condição extra, a *guarda*: "se for o
nível 1 **e** a entrada for grande".)

### 2.4 Páginas grandes: atalhos na árvore

Mapear 8 GiB de RAM com páginas de 4 KiB exigiria 2 milhões de entradas e
4096 tabelas PT (16 MiB só de tabelas). Para regiões grandes e contínuas,
a CPU permite **parar a caminhada mais cedo**:

- uma entrada de **PD** com o bit **GRANDE** aponta direto para uma página
  de **2 MiB** (pula a PT);
- uma entrada de **PDPT** com o bit **GRANDE** aponta para uma página de
  **1 GiB** (pula a PD e a PT; nem toda CPU suporta).

Nesses casos, os bits do endereço virtual que seriam os índices dos níveis
pulados viram parte do deslocamento: 21 bits de deslocamento para 2 MiB, 30
bits para 1 GiB. É por isso que `traduzir_em` usa `t.bytes() - 1` como
máscara.

O SiliconOS usa páginas de **2 MiB** para mapear a RAM inteira (seção 5) e
páginas de 4 KiB onde precisa de controle fino.

### 2.5 A TLB: o cache da tradução

Fazer 4 leituras de memória a cada acesso seria lentíssimo. A CPU guarda as
traduções recentes num cache chamado **TLB** (*Translation Lookaside
Buffer*). Consequência importante para quem escreve um kernel:

> **Quando você muda uma entrada de tabela, a CPU pode continuar usando a
> tradução antiga que está na TLB.**

Duas formas de limpar:

- **`invlpg endereço`**: tira um endereço da TLB. O SiliconOS faz isso em
  todo `mapear`/`desmapear`;
- **escrever em CR3**: limpa a TLB inteira (exceto páginas marcadas
  GLOBAL). Acontece quando trocamos as tabelas.

Esquecer o `invlpg` é um bug clássico: o código funciona "às vezes",
dependendo do que estava no cache.

---

## 3. A entrada de tabela

Cada entrada é um `u64` com um endereço físico e bits de controle:

```
 bit:  63  62..52  51 ................ 12  11..9  8  7  6  5  4  3  2  1  0
      ┌──┬───────┬────────────────────────┬──────┬──┬──┬──┬──┬──┬──┬──┬──┬──┐
      │NX│  ---  │ endereço físico do     │ livre│G │PS│D │A │CD│WT│US│RW│P │
      │  │       │ próximo nível/página   │      │  │  │  │  │  │  │  │  │  │
      └──┴───────┴────────────────────────┴──────┴──┴──┴──┴──┴──┴──┴──┴──┴──┘
```

O endereço ocupa os bits 12–51. Como tudo é alinhado em 4 KiB, os 12 bits
de baixo de um endereço de quadro são sempre zero, e a Intel aproveitou
esse espaço para os bits de controle. No código, cada bit tem nome:

```rust
pub mod flags {
    pub const PRESENTE: u64 = 1 << 0;       // a entrada é válida
    pub const ESCRITA: u64 = 1 << 1;        // permite escrever (senão, só leitura)
    pub const USUARIO: u64 = 1 << 2;        // acessível pelo anel 3
    pub const ESCRITA_DIRETA: u64 = 1 << 3; // cache write-through
    pub const SEM_CACHE: u64 = 1 << 4;      // desliga o cache para esta página
    pub const ACESSADA: u64 = 1 << 5;       // a CPU liga quando a página é lida
    pub const SUJA: u64 = 1 << 6;           // a CPU liga quando a página é escrita
    pub const GRANDE: u64 = 1 << 7;         // página de 2 MiB (nível PD) ou 1 GiB (nível PDPT)
    pub const GLOBAL: u64 = 1 << 8;         // não sai da TLB ao trocar o CR3
    pub const SEM_EXECUCAO: u64 = 1 << 63;  // proibido executar código daqui (precisa de EFER.NXE)
}
```

Os mais importantes para entender:

- **PRESENTE**: se for 0, a CPU ignora todo o resto e gera page fault.
- **ESCRITA** e **USUARIO**: as permissões. A CPU **combina** as permissões
  de todos os níveis da caminhada: para escrever, **todos** precisam permitir.
  Por isso o SiliconOS cria as tabelas intermediárias com permissões abertas e
  deixa a restrição para a entrada final.
- **ACESSADA** e **SUJA**: a **CPU** os liga sozinha. Sistemas operacionais
  usam isso para decidir quais páginas mandar para o disco (swap) quando falta
  memória: uma página "suja" precisa ser gravada antes; uma não acessada há
  muito tempo é uma boa candidata.
- **SEM_EXECUCAO** (NX): impede executar código em páginas de dados. É uma
  proteção importante contra ataques que injetam código. Só funciona se o
  bit NXE do registrador EFER estiver ligado; no QEMU o UEFI o deixa ligado.

A entrada em Rust é um *newtype* sobre `u64` ([rust-basico §22](rust-basico.md#22-newtype-um-tipo-próprio-para-um-número)):

```rust
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct Entrada(u64);
```

`#[repr(transparent)]` garante que, na memória, uma `Entrada` é exatamente
um `u64`, sem nada a mais, o que é necessário porque a CPU lê essas tabelas
diretamente. Uma `Tabela` é um array de 512 entradas, alinhado em 4 KiB:

```rust
#[repr(C, align(4096))]
pub struct Tabela {
    pub entradas: [Entrada; 512],
}

const _: () = assert!(core::mem::size_of::<Tabela>() == 4096);
```

---

## 4. Por que não dava para só "acrescentar" às tabelas do UEFI

A ideia mais simples seria pegar as tabelas que o UEFI deixou e acrescentar
o que precisamos. Antes de escrever o código, inspecionamos a máquina pelo
monitor do QEMU (`info registers` e `info mem`) e encontramos:

```
CR0=80010033  CR3=0000000007801000  CR4=00000668  EFER=0000000000000d00

0000000006a00000-0000000006c00000  -r-
0000000007800000-0000000007e00000  -r-     ← as tabelas do UEFI (CR3 = 0x7801000) estão aqui
```

- As páginas onde ficam **as próprias tabelas do UEFI estão marcadas como
  somente leitura** (`-r-`). É uma proteção do firmware contra corrupção.
- O bit **WP** (*write protect*, bit 16) de CR0 está ligado (`0x80010033`):
  com ele, nem o kernel (anel 0) consegue escrever em páginas somente leitura.

Qualquer tentativa de alterar as tabelas do UEFI daria page fault. E, mesmo
que desse, elas moram em memória `BootServices`, que um dia queremos
reaproveitar. **O kernel precisa das próprias tabelas.**

Os outros valores confirmaram o ambiente: CR4 sem o bit 12 (sem paginação de
5 níveis) e EFER com o bit 11 (NXE ligado).

---

## 5. O desenho das tabelas do SiliconOS

### 5.1 O problema da troca

Trocar as tabelas é como trocar o pneu com o carro andando. No instante do
`mov cr3`, a CPU passa a traduzir **tudo** com as tabelas novas, inclusive:

- o endereço da **próxima instrução** do kernel;
- a **pilha** (variáveis locais, endereços de retorno);
- a GDT, a IDT, a TSS e a pilha de emergência, que a CPU consulta em
  qualquer exceção;
- o framebuffer e o bitmap do alocador.

Se qualquer um deles não estiver mapeado nas tabelas novas, **no mesmo
endereço virtual de antes**, o kernel cai na hora. E o tratador de page
fault também está em memória que precisa estar mapeada...

### 5.2 A solução: três regiões

```
 espaço virtual (48 bits)
 0xFFFF_FFFF_FFFF_FFFF ┐
                       │
 0xFFFF_8000_0000_0000 ├─ (3) TODA a RAM física de novo: virtual = DESLOCAMENTO + físico
                       │     é por aqui que o kernel acessa memória física daqui em diante
                       │
       ╳ buraco não-canônico ╳
                       │
 0x0000_0000_8000_0000 ├─ (2) framebuffer (identidade)
                       │
 0x0000_0000_0000_1000 ├─ (1) TODA a RAM em identidade: virtual = físico
 0x0000_0000_0000_0000 ┘     ...exceto a página 0 (ponteiro nulo → page fault)
```

**(1) Identidade.** Mantém válidos todos os endereços que o kernel já está
usando (código, pilha, statics), porque o UEFI carregou o kernel e criou a
pilha em endereços físicos. É o que permite a troca "com o carro andando".
É um **andaime temporário**: quando houver programas comuns, a metade baixa
será deles, e o kernel terá que sair dali.

**(2) Framebuffer.** Não é RAM, então não aparece no mapa de memória
(doc 05). Mapeado à parte, em identidade.

**(3) A RAM inteira na metade alta**, a partir de
`DESLOCAMENTO_FISICO = 0xFFFF_8000_0000_0000`. Esta é a parte permanente do
desenho, chamada de *mapeamento com deslocamento* (*offset mapping*). Para o
kernel acessar o quadro físico `F` (uma tabela de página, o bitmap, um quadro
que ele acabou de alocar), basta usar o ponteiro `DESLOCAMENTO_FISICO + F`.
Quase todos os kernels de 64 bits fazem algo parecido. O Linux chama isso de
*direct map*.

### 5.3 A página 0 sem mapeamento

Os primeiros 2 MiB são mapeados em páginas de 4 KiB, **pulando a página 0**:

```rust
for p in 1..512u64 {      // começa em 1, não em 0
    mapear_em(pml4, p * TAMANHO_QUADRO, p * TAMANHO_QUADRO, TamanhoPagina::K4, rw).unwrap();
}
```

Resultado: **ler ou escrever através de um ponteiro nulo agora dá page
fault**. Com as tabelas do UEFI, o endereço 0 era memória válida, e um bug
de ponteiro nulo passaria despercebido, corrompendo dados em silêncio.
(É a mesma razão pela qual seu programa Java ou C no Linux recebe
`NullPointerException` / *segmentation fault* em vez de ler lixo.)

### 5.4 A RAM em páginas de 2 MiB

```rust
for r in mapa.regioes().iter().filter(|r| !matches!(r.tipo, Tipo::Mmio | Tipo::Reservada)) {
    let mut a = r.inicio & !(DOIS_MIB - 1);                  // arredonda para baixo
    let fim = (r.fim() + DOIS_MIB - 1) & !(DOIS_MIB - 1);    // arredonda para cima
    while a < fim {
        if a != 0 && traduzir_em(pml4, a).is_none() {
            mapear_em(pml4, a, a, TamanhoPagina::M2, rw).unwrap();
        }
        if traduzir_em(pml4, DESLOCAMENTO_FISICO + a).is_none() {
            mapear_em(pml4, DESLOCAMENTO_FISICO + a, a, TamanhoPagina::M2, rw).unwrap();
        }
        a += DOIS_MIB;
    }
}
```

Para cada região de RAM do mapa (doc 05), cada bloco de 2 MiB é mapeado duas
vezes: em identidade e na metade alta. Detalhes:

- **`x & !(DOIS_MIB - 1)`** arredonda `x` para baixo, para um múltiplo de 2
  MiB (zera os 21 bits de baixo). Somar `DOIS_MIB - 1` antes faz arredondar
  para cima. É o truque padrão de alinhamento com potências de 2.
- O **`traduzir_em(...).is_none()`** evita mapear duas vezes o mesmo bloco:
  várias regiões pequenas do mapa caem dentro do mesmo bloco de 2 MiB.
- O bloco 0 (`a != 0`) é pulado na identidade porque já foi feito em páginas
  de 4 KiB (seção 5.3), mas **não** na metade alta: lá, o físico 0 não é um
  ponteiro nulo.
- Arredondar para blocos de 2 MiB pode mapear alguns KiB de "buraco" ao
  lado de uma região de RAM. É inofensivo, porque o kernel nunca usa esses
  endereços (o alocador só entrega o que o mapa diz ser livre).

### 5.5 Criar tabelas sob demanda: `mapear_em`

```rust
pub fn mapear_em(raiz: u64, virt: u64, fisico: u64, tamanho: TamanhoPagina, bits: u64)
    -> Result<(), ErroMapeamento>
{
    // ... verifica alinhamento e se o endereço é canônico ...
    let idx = indices(virt);
    let folha = tamanho.nivel_folha();       // 1 para 1 GiB, 2 para 2 MiB, 3 para 4 KiB
    let mut tabela = raiz;

    for nivel in 0..folha {                  // desce até o nível da folha
        let e = &mut tabela_em(tabela).entradas[idx[nivel]];
        if !e.presente() {
            let nova = nova_tabela().ok_or(ErroMapeamento::SemMemoria)?;
            *e = Entrada(nova | PRESENTE | ESCRITA | (bits & USUARIO));
        } else if e.grande() {
            return Err(ErroMapeamento::JaMapeado);
        }
        tabela = e.endereco();
    }

    let e = &mut tabela_em(tabela).entradas[idx[folha]];
    if e.presente() { return Err(ErroMapeamento::JaMapeado); }
    let grande = if tamanho == TamanhoPagina::K4 { 0 } else { GRANDE };
    *e = Entrada(fisico | bits | grande | PRESENTE);
    invalidar(virt);                         // invlpg: tira da TLB
    Ok(())
}
```

É a caminhada da seção 2.3 com uma diferença: **quando falta um nível, ele é
criado**. `nova_tabela()` pede um quadro ao alocador do doc 05 e o zera (uma
tabela zerada = 512 entradas "não presentes"). É aqui que os dois capítulos
se encontram: as tabelas de página são feitas de quadros físicos.

O retorno é um `Result` ([rust-basico §6](rust-basico.md#6-option-e-result-sem-null-e-sem-exceções))
com um `enum` de erros possíveis:

```rust
pub enum ErroMapeamento { Desalinhado, NaoCanonico, JaMapeado, NaoMapeado, SemMemoria }
```

O `?` depois de `ok_or(...)` significa "se deu erro, retorne esse erro
agora" ([rust-basico §24](rust-basico.md#24-o-operador--propagar-erros)).

`desmapear_em` faz a caminhada, zera a entrada final, chama `invlpg` e
devolve o endereço físico que estava mapeado (para quem chamou poder
liberar o quadro). As tabelas intermediárias que ficarem vazias **não** são
devolvidas ao alocador por enquanto; é um pequeno desperdício, anotado no
código.

### 5.6 Acessar uma tabela pelo endereço físico

As entradas guardam endereços **físicos**, mas o código só consegue usar
endereços **virtuais**. A ponte é esta função:

```rust
static DESLOCAMENTO_ATUAL: AtomicU64 = AtomicU64::new(0);

pub fn fisico_para_virtual(fisico: u64) -> u64 {
    fisico + DESLOCAMENTO_ATUAL.load(Ordering::Relaxed)
}

fn tabela_em(fisico: u64) -> &'static mut Tabela {
    unsafe { &mut *(fisico_para_virtual(fisico) as *mut Tabela) }
}
```

O detalhe elegante: o deslocamento **muda no meio da execução**.

- Durante a montagem, as tabelas do UEFI (identidade) ainda estão ativas.
  Deslocamento = **0**: o físico F é acessado pelo ponteiro F.
- Depois da troca, o deslocamento passa a ser `DESLOCAMENTO_FISICO`: o
  físico F é acessado pelo ponteiro `0xFFFF_8000_0000_0000 + F`.

O mesmo código (`mapear_em`, `traduzir_em`, `nova_tabela`) funciona nos dois
momentos sem saber disso. A variável é um `AtomicU64`
([rust-basico §25](rust-basico.md#25-atômicos-atomicu64-atomicbool)) porque é
uma global alterada durante a execução.

### 5.7 A troca

```rust
// 4. A troca
unsafe { asm!("mov cr3, {}", in(reg) pml4, options(nostack, preserves_flags)) };

// 5. Daqui em diante, memória física é acessada pela metade alta.
DESLOCAMENTO_ATUAL.store(DESLOCAMENTO_FISICO, Ordering::Relaxed);
quadros::reposicionar_bitmap(DESLOCAMENTO_FISICO);
```

Uma instrução. Escrever em CR3 também esvazia a TLB, então as traduções
antigas do UEFI não sobrevivem.

`reposicionar_bitmap` resolve a pendência deixada no doc 05: o bitmap do
alocador era acessado pelo endereço físico (identidade). Agora o slice é
recriado apontando para a metade alta. Ainda funcionaria pela identidade,
mas assim o alocador não depende mais do andaime temporário.

---

## 6. Como foi testado

### 6.1 Antes e depois, no boot

O kernel imprime a tradução de alguns endereços com as tabelas do UEFI e
depois com as dele. Os números entre colchetes são os 4 índices (PML4, PDPT,
PD, PT):

```
Paginacao: tabelas do UEFI (CR3=0x7801000)
  codigo   0x00000000060711a0 [  0   0  48 113] -> 0x60711a0 (2 MiB)
  pilha    0x0000000007e96150 [  0   0  63 150] -> 0x7e96150 (4 KiB)
  nulo     0x0000000000000000 [  0   0   0   0] -> 0x0 (2 MiB)
Paginacao: tabelas do KERNEL ativas (CR3 0x7801000 -> 0x101000, 7 quadros usados)
  codigo   0x00000000060711a0 [  0   0  48 113] -> 0x60711a0 (2 MiB)
  pilha    0x0000000007e96150 [  0   0  63 150] -> 0x7e96150 (2 MiB)
  nulo     0x0000000000000000 [  0   0   0   0] -> NAO MAPEADO
  fis 1MiB 0xffff800000100000 [256   0   0 256] -> 0x100000 (2 MiB)
```

Como ler:

- **código e pilha** continuam traduzindo para o mesmo físico: a troca foi
  transparente. O UEFI usava páginas de 4 KiB perto da pilha; nós usamos 2
  MiB.
- **nulo**: antes era memória válida; agora gera page fault.
- **fis 1MiB**: o endereço `0xFFFF_8000_0010_0000` (índice 256 na PML4, a
  primeira entrada da metade alta) chega ao físico `0x100000`, onde mora o
  bitmap.
- O kernel inteiro, com 128 MiB de RAM, precisou de só **7 quadros** (28 KiB)
  de tabelas, graças às páginas de 2 MiB.

### 6.2 Do ponto de vista do QEMU

O monitor do QEMU (`info mem`), que lê as tabelas sem passar pelo nosso
código, mostrou exatamente as três regiões da seção 5.2:

```
CR3=0000000000101000
0000000000001000-0000000008000000 0000000007fff000 -rw     ← (1) identidade, sem a página 0
0000000080000000-0000000080400000 0000000000400000 -rw     ← (2) framebuffer (4 MiB)
ffff800000000000-ffff800008000000 0000000008000000 -rw     ← (3) RAM na metade alta
```

### 6.3 `./run.sh teste-paginacao`

1. mapeia um endereço virtual sem uso (`0x5000_0000_0000`, 80 TiB) num quadro
   novo e confere que `traduzir` devolve esse quadro, com página de 4 KiB;
2. **escreve por esse endereço e lê o mesmo dado pela metade alta**: dois
   caminhos virtuais diferentes, o mesmo quadro físico;
3. mapeia um **segundo** endereço virtual no mesmo quadro (*aliasing*) e
   confere que o que se escreve por um aparece pelo outro;
4. confere os erros: mapear de novo → `JaMapeado`; endereço não alinhado →
   `Desalinhado`; endereço não-canônico → `NaoCanonico`;
5. desmapeia os dois, confere que deixaram de traduzir e que desmapear de
   novo dá `NaoMapeado`, e libera o quadro.

```
Teste de paginacao OK: mapear, traduzir, escrever/ler por 2 caminhos,
  aliasing, erros e desmapear. Quadros em tabelas novas: 3
```

Os 3 quadros são a PDPT, a PD e a PT criadas para o endereço de teste (nada
mais existia naquela região).

### 6.4 `./run.sh teste-desmapeado` e `./run.sh teste-nulo`

```
Teste: escrevi em 0x500000000000; desmapeando e lendo de novo...
Mensagem: EXCECAO #14 PAGE FAULT ao ler 0x500000000000 (pagina nao mapeada, modo kernel)

Teste: lendo o endereco 0 (ponteiro nulo)...
Mensagem: EXCECAO #14 PAGE FAULT ao ler 0x0 (pagina nao mapeada, modo kernel)
```

O primeiro prova que `desmapear` + `invlpg` funcionam: a página foi usada
(e estava na TLB), e mesmo assim a leitura seguinte falhou.

O teste do ponteiro nulo usa **assembly** (`mov rax, [0]`) de propósito. Em
Rust, ler um ponteiro nulo é *comportamento indefinido*: o compilador
assume que isso nunca acontece e pode gerar qualquer código (inclusive
remover a leitura). Em modo debug, o Rust ainda verifica o ponteiro e daria
panic antes de a CPU tentar o acesso. Para testar a **CPU**, é preciso falar
diretamente com ela.

### 6.5 Com mais memória

| RAM | Quadros de tabelas | Observação |
|---|---|---|
| 128 MiB | 7 | — |
| 4 GiB | 13 | código e pilha em PDPT 2 (entre 2 e 3 GiB) |
| 8 GiB | 21 | **código acima de 4 GiB** (`0x140009a00`, PDPT 5) e pilha abaixo: a identidade cobre os dois |

`teste-paginacao` passou nas três configurações.

### 6.6 Todos os testes anteriores

Os 12 testes dos capítulos anteriores (panic, as 6 exceções, quadros,
liberação dupla) passaram com as tabelas novas. Em especial, o
**double fault continua funcionando**: a pilha IST1 é um `static` do kernel,
coberto pela identidade.

---

## Mudanças nos arquivos

O que este capítulo acrescenta ao código do anterior. Os docs 00 a 06 estão
no primeiro commit do repositório: o link de cada arquivo leva ao código
**no fim do doc 06**, que pode já ter o que os capítulos seguintes acrescentam.

| Arquivo | O que muda | Onde |
|---|---|---|
| [`src/memoria/paginacao.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/memoria/paginacao.rs) | **novo**: `flags`, `Entrada`, `Tabela`, `traduzir`, `mapear`/`desmapear` (e as versões `_em`), `init`, `fisico_para_virtual`, `alocar_e_mapear` | — |
| [`src/memoria/mod.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/memoria/mod.rs) | `pub mod paginacao;` | junto dos outros `pub mod` |
| [`src/memoria/quadros.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/memoria/quadros.rs) | `reposicionar_bitmap` (chamada no fim de `paginacao::init`) | fim do arquivo |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/main.rs) | em `kernel_main`: lê o RSP, `explicar_traducao` antes, `memoria::paginacao::init(mapa, fb.base, tamanho_fb)`, `explicar_traducao` depois e `testes_de_paginacao()` | depois de `testes_de_memoria()` |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/main.rs) | `fn explicar_traducao`, `fn testes_de_paginacao` | depois de `testes_de_memoria` |
| [`Cargo.toml`](https://github.com/moises-dev-git/siliconOS/blob/doc-06/Cargo.toml) | `teste-paginacao`, `teste-desmapeado`, `teste-nulo` | `[features]` |

---

## Diferença em relação ao livro

Pelo sumário, o capítulo 7 do livro trata dos mesmos temas: páginas, quadros
e tradução (7.1), mapear e desmapear (7.2), as estratégias *higher-half*,
*offset mapping* e identidade (7.3), e como depurar page faults e a TLB
(7.4). Como o livro parte de um bootloader pronto, o ponto de partida dele
provavelmente é diferente do nosso. Vale comparar ao ler.

As particularidades do SiliconOS:

- parte das tabelas do **UEFI**, que são **somente leitura** (seção 4), e por
  isso **monta as próprias tabelas do zero** e faz a troca do CR3;
- monta tudo à mão, sem a crate `x86_64`;
- usa **identidade + mapeamento com deslocamento** (seção 5.2). O código do
  kernel ainda roda em endereços baixos, porque foi o UEFI que o carregou
  ali. Mover o kernel para a metade alta (*higher-half kernel*) fica para
  quando os programas comuns precisarem da metade baixa.

---

## Resumo

- Todo ponteiro é um **endereço virtual**; a CPU o traduz para **físico**
  percorrendo **4 níveis de tabelas** (PML4 → PDPT → PD → PT), a partir do
  **CR3**. Cada tabela é um quadro de 4 KiB com 512 entradas.
- O endereço virtual é picado em **4 índices de 9 bits + 12 bits de
  deslocamento**. Páginas de **2 MiB** e **1 GiB** param a caminhada mais
  cedo.
- Cada entrada tem um endereço físico e bits de controle (presente, escrita,
  usuário, NX...). A **TLB** guarda traduções; ao mudar uma entrada, é preciso
  `invlpg`.
- As tabelas do UEFI são somente leitura; o SiliconOS monta as suas com
  **identidade** (andaime para a troca), **framebuffer** e **toda a RAM na
  metade alta** (`0xFFFF_8000_0000_0000 + físico`), sem a **página 0**.
- `fisico_para_virtual` usa um deslocamento que vale 0 antes da troca e
  `DESLOCAMENTO_FISICO` depois, então o mesmo código funciona nos dois
  momentos.

<details>
<summary><b>Para fixar (clique para ver as respostas)</b></summary>

1. **Quais são os 4 índices do endereço `0xFFFF_8000_0010_0000`?**
   PML4 = 256, PDPT = 0, PD = 0, PT = 256 (`0x100000 >> 12 = 256`). É o que
   o boot mostra na linha "fis 1MiB".

2. **Por que cada tabela tem exatamente 512 entradas?**
   Porque cada índice tem 9 bits (2⁹ = 512), e 512 entradas × 8 bytes = 4096
   bytes, exatamente um quadro.

3. **O kernel muda uma entrada de PT para apontar para outro quadro, mas
   continua lendo os dados antigos. O que faltou?**
   O `invlpg`: a tradução antiga continuava na TLB.

4. **Por que o SiliconOS não acrescentou seus mapeamentos às tabelas do UEFI?**
   Porque as páginas onde elas ficam estão marcadas como somente leitura, e
   o bit WP de CR0 faz a CPU respeitar isso até no anel 0. Além disso, elas
   moram em memória BootServices.

5. **Para que serve a identidade, se o kernel passa a acessar a memória
   física pela metade alta?**
   Para que o código, a pilha e as tabelas da CPU (GDT, IDT, TSS), que estão
   em endereços físicos baixos, continuem válidos no instante da troca do
   CR3. É um andaime temporário.

6. **Mapear 8 GiB com páginas de 4 KiB custaria quantas tabelas PT?
   E com páginas de 2 MiB?**
   8 GiB / 4 KiB = 2 097 152 páginas, ou seja, 4096 PTs (16 MiB). Com 2 MiB,
   nenhuma PT: só 8 PDs (8 GiB / 1 GiB por PD).

</details>
