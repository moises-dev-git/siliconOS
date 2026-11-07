<!-- GERADO por site/indice_funcoes.py; não editar à mão. / GENERATED; do not edit. -->

# Índice de funções

Todas as funções do kernel, agrupadas pelo arquivo onde moram e em ordem alfabética, cada uma com uma frase sobre o que faz e o capítulo que a criou. Para ver como as peças se encaixam, comece pelo [Mapa do SiliconOS](mapa.md).

Esta página é **gerada automaticamente** (`site/indice_funcoes.py`) a partir das tabelas "Funções deste capítulo" de cada capítulo, toda vez que o site é publicado. Quando uma função mudou num capítulo posterior, a frase diz "(desde o doc NN ...)"; o código atual está sempre em [`src/`](https://github.com/moises-dev-git/siliconOS/tree/main/src).

324 funções e 74 testes (ou ajudantes de teste), de 18 capítulos.

---

## Kernel (`src/`)

### [src/main.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/main.rs)

| Função | O que faz | Doc |
|---|---|---|
| `_print` | O que `kprint!`/`kprintln!` chamam: formata o texto e o envia à serial e à tela. (Desde o doc 08, com as interrupções desligadas, via `escrever`.) | [01](01-saida-serial-e-video.md#funções-deste-capítulo) |
| `demonstrar_heap` | No boot normal, usa `Box`, `Vec` e `String` e imprime onde eles moram: a prova de que o heap funciona. | [07](07-heap.md#funções-deste-capítulo) |
| `ecoar_teclado` | O laço principal do doc 08: dorme, e cada tecla que chega é traduzida e mostrada (o doc 09 o troca pelo shell). | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |
| `escrever` | A parte de `_print` que escreve de fato (serial e tela), rodando dentro de `sem_interrupcoes`. | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |
| `explicar_traducao` | Imprime os 4 índices de um endereço virtual e o resultado da tradução (usado no boot e no comando `traduzir`). | [06](06-paginacao.md#funções-deste-capítulo) |
| `imprimir_alocador` | Imprime o tamanho e o endereço do bitmap e quantos quadros estão livres. | [05](05-memoria-fisica.md#funções-deste-capítulo) |
| `imprimir_mapa` | Imprime o resumo do mapa de memória (por tipo) e onde estão o kernel, a pilha e o framebuffer. | [05](05-memoria-fisica.md#funções-deste-capítulo) |
| `kernel_main` | O kernel propriamente dito: inicializa cada subsistema na ordem e, no fim, fica num laço. Cada capítulo acrescenta um passo (GDT, IDT, memória, heap, interrupções; desde o doc 09, entrega o controle ao shell). Nunca retorna. | [00](00-visao-geral.md#funções-deste-capítulo) |
| `main` | Ponto de entrada UEFI (`#[entry]`). Guarda o framebuffer e a RSDP, sai dos serviços de boot e chama `kernel_main`. | [00](00-visao-geral.md#funções-deste-capítulo) |
| `testes_de_excecao` | Provoca de propósito a exceção escolhida pela feature (`teste-breakpoint`, `teste-divisao`, `teste-gpf`...). | [04](04-idt-e-excecoes.md#funções-deste-capítulo) |
| `testes_de_heap` | Testes do heap escolhidos pela feature (`teste-heap`, `teste-heap-liberacao-dupla`, `teste-heap-esgotar`...). | [07](07-heap.md#funções-deste-capítulo) |
| `testes_de_interrupcoes` | Testes escolhidos pela feature (`teste-timer`, `teste-sem-eoi`, `teste-print-interrupcao`, `teste-deadlock`). | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |
| `testes_de_memoria` | Testes do alocador de quadros escolhidos pela feature (`teste-quadros`, `teste-liberacao-dupla`). | [05](05-memoria-fisica.md#funções-deste-capítulo) |
| `testes_de_paginacao` | Testes de paginação escolhidos pela feature (`teste-paginacao`, `teste-desmapeado`, `teste-nulo`). | [06](06-paginacao.md#funções-deste-capítulo) |

### [src/acpi.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/acpi.rs)

| Função | O que faz | Doc |
|---|---|---|
| `checksum_ok` | Confere a soma de verificação ACPI: todos os bytes somados (com estouro) dão zero. | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |
| `desligar` | Desliga a máquina (estado S5): acha o SLP_TYP no DSDT e o escreve com SLP_EN no registrador PM1a da FADT. | [09](09-shell-e-testes.md#funções-deste-capítulo) |
| `guardar_rsdp` | Pega na tabela de configuração do UEFI o endereço da RSDP (ACPI 2.0) e guarda; tem que rodar antes de `exit_boot_services`. | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |
| `ler` | Lê um valor de qualquer tipo num endereço físico, sem exigir alinhamento (`read_unaligned`). | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |
| `ler_madt` | Interpreta as entradas da MADT: APICs locais, I/O APICs e desvios de IRQ. | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |
| `madt` | Lê a MADT na primeira chamada e devolve sempre a mesma cópia. | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |
| `Madt::rotear_irq` | Para que GSI vai uma IRQ ISA e com que polaridade e disparo, aplicando os desvios (overrides) da MADT. | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |
| `procurar_tabela` | Valida a RSDP e a XSDT (assinatura e checksum) e percorre a XSDT atrás da tabela com a assinatura dada ("APIC", "FACP"...). | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |
| `tipo_s5` | Procura o objeto `_S5_` nos bytes AML do DSDT e devolve o primeiro elemento do pacote (SLP_TYPa). | [09](09-shell-e-testes.md#funções-deste-capítulo) |

### [src/console.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/console.rs)

| Função | O que faz | Doc |
|---|---|---|
| `Console::cols` | Quantas colunas de texto cabem na tela (largura em pixels ÷ largura de um caractere). | [01](01-saida-serial-e-video.md#funções-deste-capítulo) |
| `Console::desenhar_char` | Desenha um caractere na célula (coluna, linha): para cada bit do glifo da fonte 8×16, um pixel na cor do texto ou do fundo. | [01](01-saida-serial-e-video.md#funções-deste-capítulo) |
| `Console::limpar` | Pinta a tela inteira com a cor de fundo e volta o cursor para o início. | [01](01-saida-serial-e-video.md#funções-deste-capítulo) |
| `Console::new` | Cria o console a partir das informações do framebuffer (endereço, largura, altura, pixels por linha), com o cursor no canto superior esquerdo. | [01](01-saida-serial-e-video.md#funções-deste-capítulo) |
| `Console::nova_linha` | Leva o cursor ao começo da linha seguinte; se já estava na última, chama `rolar`. | [01](01-saida-serial-e-video.md#funções-deste-capítulo) |
| `Console::pixel` | Pinta um pixel: calcula o deslocamento `y * stride + x` e escreve a cor (u32) direto na memória do framebuffer. | [01](01-saida-serial-e-video.md#funções-deste-capítulo) |
| `Console::rolar` | Quando a tela enche, copia todas as linhas de pixels uma linha de texto para cima e apaga a última. | [01](01-saida-serial-e-video.md#funções-deste-capítulo) |
| `Console::rows` | Quantas linhas de texto cabem na tela (altura em pixels ÷ altura de um caractere). | [01](01-saida-serial-e-video.md#funções-deste-capítulo) |
| `Console::write_byte` | Escreve um byte na tela: `\n` chama `nova_linha`, `\r` volta à coluna 0; os outros viram `desenhar_char`, quebrando a linha no fim. (Desde o doc 08, backspace apaga o caractere anterior.) | [01](01-saida-serial-e-video.md#funções-deste-capítulo) |
| `Console::write_str` | Implementação de `fmt::Write`: passa cada byte da string para `write_byte`. É o que permite usar `write!` com o console. | [01](01-saida-serial-e-video.md#funções-deste-capítulo) |

### [src/cpu.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/cpu.rs)

| Função | O que faz | Doc |
|---|---|---|
| `desligar_interrupcoes` | `cli`: as interrupções de hardware ficam esperando. | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |
| `escrever_msr` | `wrmsr`: escreve um MSR (número em ECX, valor em EDX:EAX). | [14](14-syscall-sysret.md#funções-deste-capítulo) |
| `interrupcoes_ligadas` | Lê o bit IF do RFLAGS. | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |
| `ler_msr` | `rdmsr`: lê um registrador específico do modelo (MSR) pelo número. | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |
| `ligar_interrupcoes` | `sti`: a CPU passa a aceitar interrupções de hardware. | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |
| `reiniciar` | Reinicia: pulso de RESET pelo 8042 (comando 0xFE); se não der, carrega uma IDT vazia e força um triple fault. | [09](09-shell-e-testes.md#funções-deste-capítulo) |
| `sem_interrupcoes` | Roda uma closure com as interrupções desligadas e devolve o IF ao estado anterior; protege tudo que um tratador também usa. | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |

### [src/escalonador.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/escalonador.rs)

| Função | O que faz | Doc |
|---|---|---|
| `acordar` | Põe como `Pronto` todo processo bloqueado pelo motivo dado e devolve quantos; pode ser chamada de um tratador. | [15](15-dormir-e-bloqueio.md#funções-deste-capítulo) |
| `acordar_em` | O mesmo que `acordar`, para quem já está com o lock da tabela (`encerrar`). | [15](15-dormir-e-bloqueio.md#funções-deste-capítulo) |
| `bloquear_e_trocar` | Marca o atual como `Bloqueado(motivo)` e troca de processo; volta quando o acordarem e chegar a vez dele. | [15](15-dormir-e-bloqueio.md#funções-deste-capítulo) |
| `ceder` | Com as interrupções desligadas, escolhe o próximo e troca para ele; volta quando chegar de novo a vez deste. | [12](12-escalonador.md#funções-deste-capítulo) |
| `com_arquivos` | Roda uma closure com a tabela de arquivos do processo atual, com o lock pego e as interrupções desligadas. | [17](17-vfs-e-arquivos.md#funções-deste-capítulo) |
| `criar` | Cria um processo com pilha do kernel fabricada (6 registradores, retorno para `primeira_entrada`, quadro de `iretq`) e o põe na tabela como `Pronto`. | [12](12-escalonador.md#funções-deste-capítulo) |
| `dormir` | Bloqueia o processo atual por pelo menos tantos tiques, sem gastar CPU. | [15](15-dormir-e-bloqueio.md#funções-deste-capítulo) |
| `encerrar` | Marca o processo atual como `Terminado`, imprime o resultado se for de segundo plano e cede para nunca mais voltar. | [12](12-escalonador.md#funções-deste-capítulo) |
| `escolher_proximo` | Rodízio: acha o próximo `Pronto`, atualiza os estados, troca TSS.RSP0 e CR3 e devolve os dois RSP para `trocar`. | [12](12-escalonador.md#funções-deste-capítulo) |
| `esperar` | Cede a CPU até o processo `pid` terminar e devolve o resultado. (Desde o doc 15, bloqueado: fica fora do rodízio até o processo terminar.) | [12](12-escalonador.md#funções-deste-capítulo) |
| `esperar_ate` | Bloqueia o processo atual até a condição ser verdadeira, conferindo-a com as interrupções desligadas (sem acordar perdido). | [15](15-dormir-e-bloqueio.md#funções-deste-capítulo) |
| `ha_outros_prontos` | Há algum processo, além do atual, pronto para rodar? (Removida no doc 15: o shell passou a bloquear.) | [12](12-escalonador.md#funções-deste-capítulo) |
| `ha_terminados_em_segundo_plano` | Há processo de segundo plano terminado esperando ser recolhido? (parte da condição do shell) | [15](15-dormir-e-bloqueio.md#funções-deste-capítulo) |
| `init` | Registra o código que está rodando (o kernel) como processo 0. | [12](12-escalonador.md#funções-deste-capítulo) |
| `janela_do_acordar_perdido` | (só com `teste-acordar-perdido`) Liga as interrupções por 3 s entre conferir e bloquear, para mostrar o bug. | [15](15-dormir-e-bloqueio.md#funções-deste-capítulo) |
| `laco_ocioso` | (naked) O processo ocioso: `sti; hlt` para sempre; roda quando ninguém mais pode rodar. | [15](15-dormir-e-bloqueio.md#funções-deste-capítulo) |
| `listar` | Pid, nome, estado e tiques de CPU de cada processo (comando `ps`). | [12](12-escalonador.md#funções-deste-capítulo) |
| `pid_atual` | O pid de quem está rodando (a syscall `pid`). | [12](12-escalonador.md#funções-deste-capítulo) |
| `primeira_entrada` | (naked) Onde o primeiro `trocar` de um processo novo "retorna": zera os registradores e faz `iretq` para o anel 3. | [12](12-escalonador.md#funções-deste-capítulo) |
| `Processo::topo_pilha` | Endereço do topo da pilha do kernel do processo (o valor de TSS.RSP0). | [12](12-escalonador.md#funções-deste-capítulo) |
| `recolher` | Se o processo `pid` terminou, retira-o, libera pilha e espaço e devolve o resultado. | [12](12-escalonador.md#funções-deste-capítulo) |
| `recolher_segundo_plano` | Libera todos os processos de segundo plano que já terminaram. | [12](12-escalonador.md#funções-deste-capítulo) |
| `retirar` | Tira um processo terminado da tabela (corrigindo o índice do atual) e devolve o processo e o resultado. | [12](12-escalonador.md#funções-deste-capítulo) |
| `terminou` | O processo `pid` terminou (ou nem existe)? A condição de `esperar`. | [15](15-dormir-e-bloqueio.md#funções-deste-capítulo) |
| `tique` | Chamada pelo timer: conta o tempo de CPU do programa interrompido no anel 3, encerra quem passou do limite e cede a vez. | [12](12-escalonador.md#funções-deste-capítulo) |
| `trocar` | (naked) A troca de contexto: empilha 6 registradores, salva o RSP, carrega o RSP do outro processo, desempilha e `ret`. | [12](12-escalonador.md#funções-deste-capítulo) |

### [src/gdt.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/gdt.rs)

| Função | O que faz | Doc |
|---|---|---|
| `definir_rsp0` | Escreve TSS.rsp[0] (pela `UnsafeCell`, com escrita não alinhada): a pilha usada na próxima interrupção vinda do anel 3. | [12](12-escalonador.md#funções-deste-capítulo) |
| `descritor_tss` | Monta o descritor de TSS, que ocupa duas entradas da GDT (128 bits) porque a base tem 64 bits. | [03](03-gdt-e-tss.md#funções-deste-capítulo) |
| `dpl` | Monta os bits de nível de privilégio (DPL, bits 45–46) de um descritor de segmento. | [03](03-gdt-e-tss.md#funções-deste-capítulo) |
| `estado` | Lê de volta a GDT ativa (`sgdt`) e os seletores CS, SS, DS e TR, para o boot mostrar que a troca funcionou. | [03](03-gdt-e-tss.md#funções-deste-capítulo) |
| `init` | Preenche a TSS (pilha IST1 do double fault e, desde o doc 10, RSP0), carrega a GDT com `lgdt`, troca o CS com `retfq`, recarrega SS/DS/ES e carrega a TSS com `ltr`. | [03](03-gdt-e-tss.md#funções-deste-capítulo) |
| `topo_pilha_anel0` | Topo da pilha que a CPU usa ao vir do anel 3 (TSS.rsp[0]). | [10](10-modo-usuario.md#funções-deste-capítulo) |
| `topo_pilha_double_fault` | Endereço do topo da pilha IST1 (a do double fault), para imprimir no boot. | [03](03-gdt-e-tss.md#funções-deste-capítulo) |

### [src/idt.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/idt.rs)

| Função | O que faz | Doc |
|---|---|---|
| `breakpoint` | Tratador do #BP (`int3`): imprime o quadro e deixa a execução continuar na instrução seguinte. | [04](04-idt-e-excecoes.md#funções-deste-capítulo) |
| `divisao` | Tratador do #DE (divisão por zero): imprime o quadro e dá panic. (Desde o doc 10, se veio do anel 3, encerra só o programa.) | [04](04-idt-e-excecoes.md#funções-deste-capítulo) |
| `double_fault` | Tratador do #DF, rodando na pilha IST1 (funciona mesmo com a pilha normal destruída). Não há como continuar: imprime e dá panic. | [04](04-idt-e-excecoes.md#funções-deste-capítulo) |
| `Entrada::new` | Cria uma entrada da IDT (interrupt gate, DPL 0, presente) apontando para o tratador, com a pilha IST escolhida (0 = nenhuma). | [04](04-idt-e-excecoes.md#funções-deste-capítulo) |
| `Entrada::para_usuario` | Muda o DPL da entrada para 3 (tipo 0xEE), para que `int 0x80` funcione a partir do anel 3. | [10](10-modo-usuario.md#funções-deste-capítulo) |
| `estado` | Base e limite da IDT ativa, lidos com `sidt` (diagnóstico do boot). | [04](04-idt-e-excecoes.md#funções-deste-capítulo) |
| `init` | Preenche a tabela com os tratadores de exceção e a carrega com `lidt`. (Os docs 08 e 10 acrescentam timer, teclado, espúrias e `int 0x80`.) | [04](04-idt-e-excecoes.md#funções-deste-capítulo) |
| `opcode_invalido` | Tratador do #UD (instrução inválida): imprime o quadro e dá panic. (Desde o doc 10, encerra só o programa se veio do anel 3.) | [04](04-idt-e-excecoes.md#funções-deste-capítulo) |
| `page_fault` | Tratador do #PF: lê o endereço culpado em CR2, decodifica o código de erro (presente/escrita/usuário/execução) e dá panic. (Desde o doc 10, encerra só o programa se veio do anel 3.) | [04](04-idt-e-excecoes.md#funções-deste-capítulo) |
| `protecao_geral` | Tratador do #GP: mostra o código de erro (o seletor envolvido, se houver) e dá panic. (Desde o doc 10, encerra só o programa se veio do anel 3.) | [04](04-idt-e-excecoes.md#funções-deste-capítulo) |
| `QuadroInterrupcao::fmt` | Implementação de `Display` para `QuadroInterrupcao`: imprime RIP, CS, RFLAGS, RSP e SS que a CPU empilhou. | [04](04-idt-e-excecoes.md#funções-deste-capítulo) |

### [src/interrupcoes/apic.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/interrupcoes/apic.rs)

| Função | O que faz | Doc |
|---|---|---|
| `eoi` | Escreve 0 no registrador EOI: "terminei". Sem isso, nenhuma interrupção de prioridade igual ou menor chega. | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |
| `escrever` | Escreve num registrador do APIC local. | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |
| `id` | Número deste APIC (desta CPU), usado como destino no I/O APIC. | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |
| `init` | Lê o MSR 0x1B, mapeia os registradores, zera a prioridade, mascara o LINT0 (por onde viria o 8259) e liga o APIC com o vetor espúrio. | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |
| `ler` | Lê um registrador de 32 bits do APIC local (MMIO no deslocamento dado). | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |

### [src/interrupcoes/ioapic.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/interrupcoes/ioapic.rs)

| Função | O que faz | Doc |
|---|---|---|
| `entrada` | A entrada de redirecionamento atual de um pino (diagnóstico). | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |
| `escrever` | Escreve num registrador do I/O APIC pelo mesmo par IOREGSEL/IOWIN. | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |
| `init` | Mapeia o I/O APIC, lê quantos pinos ele tem e mascara todos. | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |
| `ler` | Lê um registrador do I/O APIC: escreve o número em IOREGSEL e lê IOWIN. | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |
| `rotear` | Programa a entrada de redirecionamento de um pino: vetor, CPU de destino, polaridade e disparo, desmascarado. | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |

### [src/interrupcoes/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/interrupcoes/mod.rs)

| Função | O que faz | Doc |
|---|---|---|
| `espuria` | Tratador das interrupções espúrias (8259 e APIC): só conta, sem EOI. | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |
| `espurias` | Quantas interrupções espúrias chegaram. | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |
| `init` | A sequência toda: desliga o 8259, liga o APIC local e o I/O APIC, calibra e liga o timer, roteia o teclado e dá `sti`. | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |

### [src/interrupcoes/pic.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/interrupcoes/pic.rs)

| Função | O que faz | Doc |
|---|---|---|
| `desligar` | Remapeia os dois 8259 para 0x20–0x2F (para não colidir com exceções) e mascara todas as IRQs. | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |
| `esperar` | Escreve na porta 0x80 só para gastar ~1 µs entre comandos do 8259. | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |

### [src/interrupcoes/teclado.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/interrupcoes/teclado.rs)

| Função | O que faz | Doc |
|---|---|---|
| `Decodificador::new` | Decodificador novo: Shift e Caps Lock desligados. | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |
| `Decodificador::processar` | Traduz um scancode do conjunto 1 em caractere (layout US), acompanhando Shift, Caps Lock e o prefixo 0xE0. | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |
| `digitar` | Ajudante dos testes: passa uma sequência de scancodes pelo decodificador e junta os caracteres. | [09](09-shell-e-testes.md#funções-deste-capítulo) |
| `enviar_comando` | Espera o buffer de entrada do 8042 esvaziar e escreve um comando na porta 0x64. | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |
| `enviar_dado` | Espera o buffer de entrada esvaziar e escreve um byte de dado na porta 0x60. | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |
| `esperar` | Dorme com `hlt` se a fila estiver vazia, usando `cli; teste; sti; hlt` para não perder uma tecla que chegue no meio. (Substituída no doc 15: o shell bloqueia com `escalonador::esperar_ate` e o tratador o acorda.) | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |
| `fila_vazia` | A fila está vazia? (compara os índices de leitura e escrita). (Substituída no doc 15 por `tem_scancode`.) | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |
| `init` | Descarta bytes velhos, lê a configuração do 8042, liga o bit de IRQ 1 e grava de volta. | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |
| `ler_dado` | Espera o buffer de saída encher e lê um byte da porta 0x60. | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |
| `perdidos` | Quantos scancodes foram descartados porque a fila estava cheia. | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |
| `proximo_scancode` | Tira o próximo scancode da fila (o lado consumidor da fila SPSC). | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |
| `recebidos` | Quantos scancodes o tratador já recebeu. | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |
| `tem_scancode` | Há scancode na fila? Substitui `esperar` e `fila_vazia`. | [15](15-dormir-e-bloqueio.md#funções-deste-capítulo) |
| `tratador` | Tratador do vetor 0x31: lê o scancode da porta 0x60, põe na fila (ou conta como perdido) e manda EOI. | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |

### [src/interrupcoes/timer.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/interrupcoes/timer.rs)

| Função | O que faz | Doc |
|---|---|---|
| `calibrar` | Mede quanto o timer do APIC conta em 10 ms usando o canal 2 do PIT como cronômetro. | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |
| `hora_rtc` | Hora, minuto e segundo do relógio CMOS, convertidos de BCD e de 12 h quando preciso. | [09](09-shell-e-testes.md#funções-deste-capítulo) |
| `iniciar` | Liga o timer do APIC em modo periódico no vetor dado, com a contagem calibrada. | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |
| `milissegundos` | Tiques × 10. | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |
| `registrador` | (Função interna de `hora_rtc`.) Lê um registrador do CMOS: número na porta 0x70, valor na 0x71. | [09](09-shell-e-testes.md#funções-deste-capítulo) |
| `segundos_rtc` | Lê os segundos do relógio CMOS (RTC) por polling, para conferir o timer. | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |
| `tiques` | Quantos tiques (de 10 ms) desde que o timer foi ligado. | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |
| `tratador` | Tratador do vetor 0x30: soma um tique, manda EOI (e, desde o doc 10, encerra o programa do anel 3 que passou do limite). | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |

### [src/memoria/bump.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/memoria/bump.rs)

| Função | O que faz | Doc |
|---|---|---|
| `AlocadorBump::alloc` | Alocador bump: alinha o ponteiro, avança-o pelo tamanho pedido e devolve o início. Nunca reaproveita nada. | [07](07-heap.md#funções-deste-capítulo) |
| `AlocadorBump::dealloc` | Só conta a liberação: um bump não consegue devolver memória. | [07](07-heap.md#funções-deste-capítulo) |
| `estado` | Bytes consumidos, alocações e liberações do bump (para comparar com o heap de verdade). | [07](07-heap.md#funções-deste-capítulo) |
| `init` | Mapeia a faixa do heap para o alocador bump (usado só com a feature `teste-bump`). | [07](07-heap.md#funções-deste-capítulo) |

### [src/memoria/espaco.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/memoria/espaco.rs)

| Função | O que faz | Doc |
|---|---|---|
| `EspacoEnderecos::ativar` | Passa a usar este espaço (escreve a raiz no CR3). | [11](11-espacos-de-enderecos.md#funções-deste-capítulo) |
| `EspacoEnderecos::do_programa` | O endereço está na metade baixa, numa entrada da PML4 que o kernel não usa? | [11](11-espacos-de-enderecos.md#funções-deste-capítulo) |
| `EspacoEnderecos::drop` | `Drop`: confere que o espaço não está ativo e libera as árvores das entradas do programa e a PML4. | [11](11-espacos-de-enderecos.md#funções-deste-capítulo) |
| `EspacoEnderecos::entrada_livre` | A entrada `i` da PML4 do kernel está vazia? | [11](11-espacos-de-enderecos.md#funções-deste-capítulo) |
| `EspacoEnderecos::mapear` | Mapeia uma página de 4 KiB e fica dono do quadro; recusa endereços da área do kernel (`AreaDoKernel`) e, em caso de erro, libera o quadro. | [11](11-espacos-de-enderecos.md#funções-deste-capítulo) |
| `EspacoEnderecos::novo` | Cria uma PML4 zerada e copia para ela as entradas presentes da PML4 do kernel (compartilhando as tabelas abaixo delas). | [11](11-espacos-de-enderecos.md#funções-deste-capítulo) |
| `EspacoEnderecos::raiz` | Endereço físico da PML4 do espaço. | [11](11-espacos-de-enderecos.md#funções-deste-capítulo) |
| `EspacoEnderecos::traduzir` | `traduzir_em` com a PML4 deste espaço. | [11](11-espacos-de-enderecos.md#funções-deste-capítulo) |
| `liberar_arvore` | Libera recursivamente uma tabela, as tabelas abaixo dela e as páginas apontadas pela PT. | [11](11-espacos-de-enderecos.md#funções-deste-capítulo) |

### [src/memoria/heap.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/memoria/heap.rs)

| Função | O que faz | Doc |
|---|---|---|
| `alinhar_acima` | Arredonda um número para cima até o próximo múltiplo de uma potência de 2. | [07](07-heap.md#funções-deste-capítulo) |
| `AlocadorLista::alloc` | `GlobalAlloc::alloc`: o que `Box`/`Vec` chamam. Trava o heap com as interrupções desligadas e chama `alocar`. | [07](07-heap.md#funções-deste-capítulo) |
| `AlocadorLista::dealloc` | `GlobalAlloc::dealloc`: trava o heap com as interrupções desligadas e chama `liberar`. | [07](07-heap.md#funções-deste-capítulo) |
| `contem` | O endereço está na faixa já mapeada do heap? | [07](07-heap.md#funções-deste-capítulo) |
| `estatisticas` | Tamanho mapeado, bytes em uso, blocos livres, alocações e liberações (há o método em `Heap` e este atalho global). | [07](07-heap.md#funções-deste-capítulo) |
| `Heap::alocar` | `procurar`; se não achar, `crescer` e tenta de novo; se ainda assim não der, devolve nulo. | [07](07-heap.md#funções-deste-capítulo) |
| `Heap::crescer` | Mapeia mais páginas no fim do heap (até o limite) e as insere como bloco livre. | [07](07-heap.md#funções-deste-capítulo) |
| `Heap::inserir` | Devolve um bloco à lista em ordem de endereço e o junta com os vizinhos encostados; detecta liberação dupla. | [07](07-heap.md#funções-deste-capítulo) |
| `Heap::liberar` | Calcula o tamanho real e chama `inserir`. | [07](07-heap.md#funções-deste-capítulo) |
| `Heap::livre_no_fim` | Tamanho do último bloco livre se ele encostar no fim do heap (para crescer só o que falta). | [07](07-heap.md#funções-deste-capítulo) |
| `Heap::procurar` | First fit: anda pela lista e recorta o primeiro bloco livre onde o pedido cabe alinhado; as sobras voltam para a lista. | [07](07-heap.md#funções-deste-capítulo) |
| `init` | Mapeia o heap inicial e coloca esse espaço como um único bloco livre. | [07](07-heap.md#funções-deste-capítulo) |
| `mapear_faixa` | Mapeia uma faixa de páginas, cada uma num quadro novo, com ESCRITA e SEM_EXECUCAO; devolve quantos bytes conseguiu. | [07](07-heap.md#funções-deste-capítulo) |
| `tamanho_real` | Tamanho que o heap realmente reserva: múltiplo de 16 e nunca menor que um `NoLivre`. | [07](07-heap.md#funções-deste-capítulo) |

### [src/memoria/mapa.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/memoria/mapa.rs)

| Função | O que faz | Doc |
|---|---|---|
| `get` | Devolve o mapa já inicializado (usado por quem vem depois, como o alocador). | [05](05-memoria-fisica.md#funções-deste-capítulo) |
| `init` | Copia o mapa do UEFI para o formato do kernel (juntando regiões vizinhas do mesmo tipo). Só trabalha na primeira chamada. | [05](05-memoria-fisica.md#funções-deste-capítulo) |
| `Mapa::encontrar` | Em que região está um endereço físico (se estiver em alguma). | [05](05-memoria-fisica.md#funções-deste-capítulo) |
| `Mapa::fim_da_ram` | Maior endereço de RAM (ignora MMIO e Reservada): até onde o bitmap do alocador precisa ir. | [05](05-memoria-fisica.md#funções-deste-capítulo) |
| `Mapa::regioes` | As regiões válidas, como uma fatia (`&[Regiao]`) do array fixo. | [05](05-memoria-fisica.md#funções-deste-capítulo) |
| `Mapa::total` | Soma os quadros e conta as regiões de um tipo (para o resumo do boot). | [05](05-memoria-fisica.md#funções-deste-capítulo) |
| `Regiao::contem` | O endereço físico está dentro desta região? | [05](05-memoria-fisica.md#funções-deste-capítulo) |
| `Regiao::fim` | Primeiro endereço físico depois da região (início + quadros × 4096). | [05](05-memoria-fisica.md#funções-deste-capítulo) |
| `Tipo::de_uefi` | Converte o tipo de memória do UEFI (uns 15 tipos) para os poucos tipos que o kernel distingue (Livre, Kernel, ACPI, MMIO, Reservada...). | [05](05-memoria-fisica.md#funções-deste-capítulo) |
| `Tipo::nome` | Nome legível do tipo, para imprimir. | [05](05-memoria-fisica.md#funções-deste-capítulo) |

### [src/memoria/paginacao.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/memoria/paginacao.rs)

| Função | O que faz | Doc |
|---|---|---|
| `alocar_e_mapear` | Atalho para testes: aloca um quadro e o mapeia no endereço virtual dado. | [06](06-paginacao.md#funções-deste-capítulo) |
| `ativar` | Escreve uma PML4 no CR3 (se já não for a ativa); a CPU esvazia a TLB. | [11](11-espacos-de-enderecos.md#funções-deste-capítulo) |
| `canonico` | O endereço é canônico (bits 48–63 iguais ao bit 47)? | [06](06-paginacao.md#funções-deste-capítulo) |
| `desmapear` | `desmapear_em` na PML4 ativa, seguido de `invalidar`. | [06](06-paginacao.md#funções-deste-capítulo) |
| `desmapear_em` | Remove o mapeamento e devolve o físico que ele usava (as tabelas intermediárias vazias ficam). | [06](06-paginacao.md#funções-deste-capítulo) |
| `Entrada::endereco` | O endereço físico guardado na entrada (bits 12–51). | [06](06-paginacao.md#funções-deste-capítulo) |
| `Entrada::flags` | Só os bits de controle da entrada (tudo menos o endereço). | [06](06-paginacao.md#funções-deste-capítulo) |
| `Entrada::grande` | A entrada tem o bit PS, ou seja, mapeia uma página de 2 MiB ou 1 GiB em vez de apontar para outra tabela? | [06](06-paginacao.md#funções-deste-capítulo) |
| `Entrada::presente` | A entrada tem o bit P (presente)? | [06](06-paginacao.md#funções-deste-capítulo) |
| `fisico_para_virtual` | Converte um endereço físico no virtual por onde o kernel o enxerga agora (igual antes da troca de tabelas; físico + 0xFFFF_8000_0000_0000 depois). | [06](06-paginacao.md#funções-deste-capítulo) |
| `indices` | Os 4 índices de 9 bits que um endereço virtual escolhe (PML4, PDPT, PD, PT). | [06](06-paginacao.md#funções-deste-capítulo) |
| `init` | Monta as tabelas do kernel (primeiros 2 MiB em páginas de 4 KiB sem a página 0; toda a RAM em 2 MiB em identidade e na metade alta; o framebuffer), troca o CR3 e passa a usar a metade alta. | [06](06-paginacao.md#funções-deste-capítulo) |
| `invalidar` | `invlpg`: tira um endereço da TLB, para a CPU não usar a tradução antiga. | [06](06-paginacao.md#funções-deste-capítulo) |
| `mapear` | `mapear_em` na PML4 ativa, seguido de `invalidar`. | [06](06-paginacao.md#funções-deste-capítulo) |
| `mapear_em` | Mapeia virtual → físico numa hierarquia, criando as tabelas intermediárias que faltarem; recusa endereço desalinhado, não canônico ou já mapeado. | [06](06-paginacao.md#funções-deste-capítulo) |
| `mapear_mmio` | Mapeia registradores de dispositivo na faixa MMIO_INICIO, com ESCRITA, SEM_CACHE e SEM_EXECUCAO, e devolve o endereço virtual. | [08](08-interrupcoes-de-hardware.md#funções-deste-capítulo) |
| `nova_tabela` | Pega um quadro livre e o zera: uma tabela nova sem nenhuma entrada. | [06](06-paginacao.md#funções-deste-capítulo) |
| `raiz_atual` | Endereço físico da PML4 ativa (CR3 sem os bits de controle). | [06](06-paginacao.md#funções-deste-capítulo) |
| `raiz_kernel` | Endereço físico da PML4 do kernel (as tabelas ativas, antes de `init`). | [11](11-espacos-de-enderecos.md#funções-deste-capítulo) |
| `tabela_em` | Dá acesso, como `&mut Tabela`, à tabela de página que está no endereço físico informado (via `fisico_para_virtual`). | [06](06-paginacao.md#funções-deste-capítulo) |
| `TamanhoPagina::bytes` | Tamanho da página em bytes (4 KiB, 2 MiB ou 1 GiB). | [06](06-paginacao.md#funções-deste-capítulo) |
| `TamanhoPagina::nivel_folha` | Em que nível fica a entrada final para este tamanho de página (PT para 4 KiB, PD para 2 MiB, PDPT para 1 GiB). | [06](06-paginacao.md#funções-deste-capítulo) |
| `TamanhoPagina::nome` | Nome legível do tamanho, para imprimir. | [06](06-paginacao.md#funções-deste-capítulo) |
| `traduzir` | `traduzir_em` com a PML4 ativa. | [06](06-paginacao.md#funções-deste-capítulo) |
| `traduzir_em` | Faz à mão o que a CPU faz: percorre as tabelas a partir de uma PML4 e devolve o físico, o tamanho e as flags (ou `None`). | [06](06-paginacao.md#funções-deste-capítulo) |

### [src/memoria/quadros.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/memoria/quadros.rs)

| Função | O que faz | Doc |
|---|---|---|
| `AlocadorQuadros::alocar` | Procura no bitmap, a partir da última palavra usada, a primeira palavra de 64 bits que não está cheia; `trailing_ones` dá o bit livre, que é marcado e devolvido. | [05](05-memoria-fisica.md#funções-deste-capítulo) |
| `AlocadorQuadros::liberar` | Desmarca o quadro e recua a posição de busca se preciso; endereço desalinhado, fora da RAM ou já livre (liberação dupla) dá panic. | [05](05-memoria-fisica.md#funções-deste-capítulo) |
| `AlocadorQuadros::livres` | Contador de quadros livres. | [05](05-memoria-fisica.md#funções-deste-capítulo) |
| `AlocadorQuadros::marcar` | Liga ou desliga o bit de um quadro no bitmap. | [05](05-memoria-fisica.md#funções-deste-capítulo) |
| `AlocadorQuadros::marcar_intervalo` | Marca `n` quadros seguidos como livres ou ocupados. | [05](05-memoria-fisica.md#funções-deste-capítulo) |
| `AlocadorQuadros::ocupado` | Lê o bit do quadro no bitmap (1 = ocupado). | [05](05-memoria-fisica.md#funções-deste-capítulo) |
| `AlocadorQuadros::total` | Quantos quadros o bitmap cobre. | [05](05-memoria-fisica.md#funções-deste-capítulo) |
| `alocar` | Atalho global: trava o `Mutex` do alocador e chama `AlocadorQuadros::alocar`. | [05](05-memoria-fisica.md#funções-deste-capítulo) |
| `init` | Cria o alocador: põe o bitmap na primeira região livre acima de 1 MiB onde ele cabe, marca tudo como ocupado, libera as regiões livres acima de 1 MiB e protege o próprio bitmap. | [05](05-memoria-fisica.md#funções-deste-capítulo) |
| `liberar` | Atalho global para `AlocadorQuadros::liberar`. | [05](05-memoria-fisica.md#funções-deste-capítulo) |
| `livres` | Atalho global para `AlocadorQuadros::livres`. | [05](05-memoria-fisica.md#funções-deste-capítulo) |
| `Quadro::endereco` | Endereço físico do quadro. | [05](05-memoria-fisica.md#funções-deste-capítulo) |
| `Quadro::indice` | Número do quadro (endereço ÷ 4096): a posição dele no bitmap. | [05](05-memoria-fisica.md#funções-deste-capítulo) |
| `reposicionar_bitmap` | Passa a acessar o bitmap por outro endereço virtual; chamada quando a paginação troca para as tabelas do kernel. | [05](05-memoria-fisica.md#funções-deste-capítulo) |

### [src/panic.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/panic.rs)

| Função | O que faz | Doc |
|---|---|---|
| `escrever_detalhes` | Escreve mensagem, arquivo, linha e coluna do panic em qualquer destino que implemente `Write` (é chamada duas vezes: serial e tela). | [02](02-panic-handler.md#funções-deste-capítulo) |
| `panic` | O `#[panic_handler]`: desliga as interrupções, detecta panic dentro de panic, escreve os detalhes na serial e na tela (fundo vermelho, destravando o console à força) e para. (Desde o doc 09, em `cargo test`, sai do QEMU com código de falha.) | [02](02-panic-handler.md#funções-deste-capítulo) |
| `parar` | Laço eterno de `cli; hlt`: a CPU dorme e não acorda mais. | [02](02-panic-handler.md#funções-deste-capítulo) |

### [src/qemu.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/qemu.rs)

| Função | O que faz | Doc |
|---|---|---|
| `sair` | Escreve na porta 0xF4 do dispositivo isa-debug-exit: o QEMU termina com o código (valor × 2 + 1), 33 = ok, 35 = falha. | [09](09-shell-e-testes.md#funções-deste-capítulo) |

### [src/ramdisk.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/ramdisk.rs)

| Função | O que faz | Doc |
|---|---|---|
| `abrir` | O arquivo com esse nome, se existir. | [13](13-disco-em-ram-e-elf.md#funções-deste-capítulo) |
| `arquivos` | Um iterador sobre todos os arquivos do disco em RAM. | [13](13-disco-em-ram-e-elf.md#funções-deste-capítulo) |
| `Arquivos::next` | O iterador: lê um cabeçalho, devolve nome e dados (fatias do tar) e pula para o próximo múltiplo de 512. | [13](13-disco-em-ram-e-elf.md#funções-deste-capítulo) |
| `cabecalho_valido` | Confere a assinatura "ustar" e a soma de verificação de um cabeçalho. | [13](13-disco-em-ram-e-elf.md#funções-deste-capítulo) |
| `ler_octal` | Lê um número octal em texto, recusando dígitos inválidos e estouros. | [13](13-disco-em-ram-e-elf.md#funções-deste-capítulo) |
| `tamanho_total` | Tamanho do tar inteiro, em bytes. | [13](13-disco-em-ram-e-elf.md#funções-deste-capítulo) |

### [src/serial.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/serial.rs)

| Função | O que faz | Doc |
|---|---|---|
| `inb` | Instrução `in`: lê um byte de uma porta de E/S. | [01](01-saida-serial-e-video.md#funções-deste-capítulo) |
| `inw` | Instrução `in` de 16 bits. | [09](09-shell-e-testes.md#funções-deste-capítulo) |
| `outb` | Instrução `out`: escreve um byte numa porta de E/S. `unsafe` porque uma porta errada pode fazer qualquer coisa com o hardware. | [01](01-saida-serial-e-video.md#funções-deste-capítulo) |
| `outw` | Instrução `out` de 16 bits (usada no registrador PM1a do ACPI). | [09](09-shell-e-testes.md#funções-deste-capítulo) |
| `Serial::init` | Configura a UART COM1 (porta 0x3F8): 38400 baud, 8 bits, sem paridade, 1 stop bit, FIFOs ligadas, interrupções da UART desligadas. | [01](01-saida-serial-e-video.md#funções-deste-capítulo) |
| `Serial::write_byte` | Espera o registrador de transmissão esvaziar (bit 5 da porta de status) e envia um byte. | [01](01-saida-serial-e-video.md#funções-deste-capítulo) |
| `Serial::write_str` | Implementação de `fmt::Write` para a serial: envia byte a byte, convertendo `\n` em `\r\n`. (Desde o doc 08, backspace vira "volta, espaço, volta".) | [01](01-saida-serial-e-video.md#funções-deste-capítulo) |

### [src/shell.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/shell.rs)

| Função | O que faz | Doc |
|---|---|---|
| `ajuda` | Comando `ajuda`: lista os comandos e suas descrições, tirados da própria tabela. | [09](09-shell-e-testes.md#funções-deste-capítulo) |
| `cpu_info` | Comando `cpu`: fabricante e modelo lidos com `cpuid`. | [09](09-shell-e-testes.md#funções-deste-capítulo) |
| `desligar` | Comando `desligar`: chama `acpi::desligar` e espera 20 tiques antes de dizer que não conseguiu. | [09](09-shell-e-testes.md#funções-deste-capítulo) |
| `eco` | Comando `eco`: imprime os argumentos. | [09](09-shell-e-testes.md#funções-deste-capítulo) |
| `elf` | Comando `elf <arquivo>`: cabeçalho e segmentos, como o `readelf -l`. | [13](13-disco-em-ram-e-elf.md#funções-deste-capítulo) |
| `espacos` | Comando `espacos`: dois espaços vivos com `ola` e `relogio` no mesmo endereço, mostrando PML4, físico e os primeiros bytes de cada. | [11](11-espacos-de-enderecos.md#funções-deste-capítulo) |
| `executar` | Separa a linha em palavras, procura a primeira na tabela de comandos e chama a função com o resto. | [09](09-shell-e-testes.md#funções-deste-capítulo) |
| `int` | Comando `int`: contadores de tiques, teclas recebidas/perdidas e espúrias. | [09](09-shell-e-testes.md#funções-deste-capítulo) |
| `ler` | Comando `ler <endereço>`: lê 8 bytes do endereço sem conferir nada (de propósito: `ler 0` mostra um page fault). | [09](09-shell-e-testes.md#funções-deste-capítulo) |
| `ler_numero` | Lê um número decimal ou hexadecimal (`0x`), aceitando `_` como separador. | [09](09-shell-e-testes.md#funções-deste-capítulo) |
| `limpar` | Comando `limpar`: apaga a tela. | [09](09-shell-e-testes.md#funções-deste-capítulo) |
| `ls` | Comando `ls`: os arquivos do disco em RAM. | [13](13-disco-em-ram-e-elf.md#funções-deste-capítulo) |
| `mem` | Comando `mem`: quadros livres e estatísticas do heap (em uso, pico, mapeado, blocos livres). | [09](09-shell-e-testes.md#funções-deste-capítulo) |
| `programas` | Comando `programas`: lista os programas embutidos e seus tamanhos. | [10](10-modo-usuario.md#funções-deste-capítulo) |
| `ps` | Comando `ps`: a tabela de processos. | [12](12-escalonador.md#funções-deste-capítulo) |
| `reiniciar` | Comando `reiniciar`: chama `cpu::reiniciar`. | [09](09-shell-e-testes.md#funções-deste-capítulo) |
| `rodar` | O laço principal do kernel: tira scancodes da fila, decodifica, passa ao shell e dorme quando não há nada. | [09](09-shell-e-testes.md#funções-deste-capítulo) |
| `rodar_programa` | Comando `rodar <programa>`: roda no anel 3 e mostra o resultado (código de saída, falha ou tempo esgotado). | [10](10-modo-usuario.md#funções-deste-capítulo) |
| `Shell::new` | Shell novo, com a linha vazia. | [09](09-shell-e-testes.md#funções-deste-capítulo) |
| `Shell::tecla` | Trata uma tecla: caractere vai para a linha (até o limite), backspace apaga, Enter executa e mostra o prompt de novo. | [09](09-shell-e-testes.md#funções-deste-capítulo) |
| `tempo` | Comando `tempo`: tempo desde o boot (pelos tiques) e hora do RTC. | [09](09-shell-e-testes.md#funções-deste-capítulo) |
| `traduzir` | Comando `traduzir <endereço>`: mostra a tradução do endereço virtual (via `explicar_traducao`). | [09](09-shell-e-testes.md#funções-deste-capítulo) |

### [src/testes.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/testes.rs)

| Função | O que faz | Doc |
|---|---|---|
| `Testavel::rodar` | `Testavel::rodar` para toda função `Fn()`: imprime o nome do teste, roda e imprime `[ok]` (se der panic, o handler cuida). | [09](09-shell-e-testes.md#funções-deste-capítulo) |

### [src/usuario/argumentos.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/usuario/argumentos.rs)

| Função | O que faz | Doc |
|---|---|---|
| `montar` | Monta num vetor o topo da pilha inicial de um programa (argc, argv, zeros, textos) e calcula o RSP, alinhado em 16; recusa argumentos vazios, demais, grandes ou com o byte 0. | [16](16-argumentos.md#funções-deste-capítulo) |

### [src/usuario/elf.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/usuario/elf.rs)

| Função | O que faz | Doc |
|---|---|---|
| `carregar` | Carrega todos os segmentos LOAD num espaço e confere que a entrada é executável. | [13](13-disco-em-ram-e-elf.md#funções-deste-capítulo) |
| `carregar_segmento` | Valida o segmento e, página por página, aloca um quadro zerado, copia a parte do arquivo e mapeia com as permissões do segmento. | [13](13-disco-em-ram-e-elf.md#funções-deste-capítulo) |
| `ler` | `N` bytes do arquivo a partir de uma posição, ou `Truncado`. | [13](13-disco-em-ram-e-elf.md#funções-deste-capítulo) |
| `ler_cabecalho` | Confere assinatura, classe, ordem dos bytes, tipo, máquina e tamanho dos program headers; devolve entrada e tabela. | [13](13-disco-em-ram-e-elf.md#funções-deste-capítulo) |
| `permissoes` | As flags de um segmento como "R-X", "RW-"... | [13](13-disco-em-ram-e-elf.md#funções-deste-capítulo) |
| `segmento` | Lê o program header número `i`. | [13](13-disco-em-ram-e-elf.md#funções-deste-capítulo) |
| `u16_em` / `u32_em` / `u64_em` | Um número little-endian numa posição do arquivo. | [13](13-disco-em-ram-e-elf.md#funções-deste-capítulo) |

### [src/usuario/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/usuario/mod.rs)

| Função | O que faz | Doc |
|---|---|---|
| `abrir` (syscall) | Syscall 5: valida e copia o caminho, abre pelo VFS e devolve o descritor. | [17](17-vfs-e-arquivos.md#funções-deste-capítulo) |
| `descrever` | Texto curto do resultado de um programa ("terminou: codigo de saida 42"...). | [12](12-escalonador.md#funções-deste-capítulo) |
| `dormir` (syscall) | Syscall 4: dorme `ms` milissegundos (arredondados para cima em tiques); devolve 0. | [15](15-dormir-e-bloqueio.md#funções-deste-capítulo) |
| `encerrar` | Guarda o resultado e pula para `voltar_ao_kernel`; chamada de dentro de um tratador, nunca retorna. (Substituída no doc 12 por `escalonador::encerrar`.) | [10](10-modo-usuario.md#funções-deste-capítulo) |
| `entrada_syscall` | (naked) Tratador do vetor 0x80: empilha os registradores numa `Registros`, chama `tratar_syscall` e volta com `iretq`. | [10](10-modo-usuario.md#funções-deste-capítulo) |
| `entrar_no_anel3` | (naked) Salva os registradores do kernel e o RSP, monta à mão um quadro de interrupção (SS, RSP, RFLAGS, CS, RIP do programa) e executa `iretq`. (Substituída no doc 12 pela pilha fabricada de `escalonador::criar` + `primeira_entrada`.) | [10](10-modo-usuario.md#funções-deste-capítulo) |
| `escrever` | A syscall `escrever`: valida o ponteiro, imprime os bytes e devolve quantos foram escritos (ou -14). | [10](10-modo-usuario.md#funções-deste-capítulo) |
| `faixa_com_flags` | Toda página da faixa é do programa e tem os bits exigidos? | [17](17-vfs-e-arquivos.md#funções-deste-capítulo) |
| `faixa_do_programa` | Confere se um ponteiro + tamanho vindo do programa está inteiro dentro das páginas dele (sem estouro de soma). | [10](10-modo-usuario.md#funções-deste-capítulo) |
| `faixa_gravavel_do_programa` | Como `faixa_do_programa`, mas exige páginas graváveis (destino de uma cópia do kernel). | [17](17-vfs-e-arquivos.md#funções-deste-capítulo) |
| `falha_do_programa` | Chamada pelos tratadores de exceção: se CS indica anel 3, imprime a falha e encerra o programa; senão retorna e o tratador dá panic. | [10](10-modo-usuario.md#funções-deste-capítulo) |
| `fechar` (syscall) | Syscall 7: fecha o descritor. | [17](17-vfs-e-arquivos.md#funções-deste-capítulo) |
| `iniciar` | Prepara o espaço de endereços do programa (código e pilha) e o entrega ao escalonador; devolve o pid. | [12](12-escalonador.md#funções-deste-capítulo) |
| `iniciar_elf` | Cria o espaço, carrega o ELF, mapeia 4 páginas de pilha no fim da metade baixa e entrega ao escalonador com RSP = topo − 8. | [13](13-disco-em-ram-e-elf.md#funções-deste-capítulo) |
| `ler` (syscall) | Syscall 6: copia os próximos bytes do arquivo para um destino gravável do programa. | [17](17-vfs-e-arquivos.md#funções-deste-capítulo) |
| `ms_para_tiques` | Milissegundos em tiques, arredondando para cima e sem estourar. | [15](15-dormir-e-bloqueio.md#funções-deste-capítulo) |
| `nova_pagina_zerada` | Aloca um quadro e o zera (para o programa não ver lixo de quem usou o quadro antes). | [10](10-modo-usuario.md#funções-deste-capítulo) |
| `rodar` | Mapeia código e pilha do programa com o bit USUARIO, entra no anel 3, e quando ele termina desmapeia tudo, libera os quadros e devolve o `Resultado`. (Desde o doc 11, num espaço de endereços próprio; desde o doc 12, é `iniciar` + `escalonador::esperar`.) | [10](10-modo-usuario.md#funções-deste-capítulo) |
| `rodar_por_nome` | Ajudante dos testes: procura o programa pelo nome e o roda. | [10](10-modo-usuario.md#funções-deste-capítulo) |
| `tempo_esgotado` | Chamada pelo timer: o tique interrompeu o anel 3 e o programa já passou de `LIMITE_TIQUES`? (Substituída no doc 12 por `escalonador::tique`, que conta tempo de CPU.) | [10](10-modo-usuario.md#funções-deste-capítulo) |
| `tratar_syscall` | Despacha pelo número em rax: `sair`, `escrever` ou `tiques`; número desconhecido devolve -38. | [10](10-modo-usuario.md#funções-deste-capítulo) |
| `voltar_ao_kernel` | (naked) Restaura o RSP salvo por `entrar_no_anel3`, desempilha os registradores e faz `ret`: parece que `entrar_no_anel3` retornou. (Substituída no doc 12 por `escalonador::trocar`.) | [10](10-modo-usuario.md#funções-deste-capítulo) |

### [src/usuario/programas.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/usuario/programas.rs)

| Função | O que faz | Doc |
|---|---|---|
| `procurar` | Procura um programa embutido pelo nome. | [10](10-modo-usuario.md#funções-deste-capítulo) |
| `Programa::codigo` | Os bytes de máquina do programa (uma fatia de `.rdata` entre dois rótulos). | [10](10-modo-usuario.md#funções-deste-capítulo) |

### [src/usuario/syscall.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/usuario/syscall.rs)

| Função | O que faz | Doc |
|---|---|---|
| `entrada_syscall_rapida` | (naked) Troca para a pilha do kernel do processo, monta o mesmo quadro e os mesmos registradores do `int 0x80`, chama `tratar_syscall` e volta com `sysretq`. | [14](14-syscall-sysret.md#funções-deste-capítulo) |
| `init` | Escreve STAR (seletores), LSTAR (entrada), FMASK (IF, TF, DF) e liga o EFER.SCE. | [14](14-syscall-sysret.md#funções-deste-capítulo) |

### [src/vfs/disco.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/vfs/disco.rs)

| Função | O que faz | Doc |
|---|---|---|
| `Disco::abrir`, `Disco::listar` | O disco em RAM (tar) visto pelo VFS. | [17](17-vfs-e-arquivos.md#funções-deste-capítulo) |

### [src/vfs/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/vfs/mod.rs)

| Função | O que faz | Doc |
|---|---|---|
| `abrir` (VFS) | Abre um arquivo pelo caminho completo, repassando ao sistema de arquivos da montagem. | [17](17-vfs-e-arquivos.md#funções-deste-capítulo) |
| `ArquivoAberto::ler` | Copia os próximos bytes e avança a posição; 0 no fim. | [17](17-vfs-e-arquivos.md#funções-deste-capítulo) |
| `ArquivoAberto::restante` | Quantos bytes faltam ler. | [17](17-vfs-e-arquivos.md#funções-deste-capítulo) |
| `Conteudo::bytes` | Os bytes do conteúdo, fixo (do tar) ou gerado. | [17](17-vfs-e-arquivos.md#funções-deste-capítulo) |
| `Descritores::arquivo` | O arquivo de um descritor, se aberto; aceita qualquer número sem cair. | [17](17-vfs-e-arquivos.md#funções-deste-capítulo) |
| `Descritores::fechar` | Fecha um descritor e libera o arquivo. | [17](17-vfs-e-arquivos.md#funções-deste-capítulo) |
| `Descritores::guardar` | Guarda o arquivo no menor descritor livre (a partir de 3), ou o devolve se a tabela está cheia. | [17](17-vfs-e-arquivos.md#funções-deste-capítulo) |
| `Descritores::novo` | Uma tabela de arquivos abertos vazia. | [17](17-vfs-e-arquivos.md#funções-deste-capítulo) |
| `Descritores::quantos` | Quantos arquivos estão abertos (usada nos testes). | [17](17-vfs-e-arquivos.md#funções-deste-capítulo) |
| `listar` (VFS) | O conteúdo de um diretório, com as montagens de dentro dele como subdiretórios. | [17](17-vfs-e-arquivos.md#funções-deste-capítulo) |
| `pai` | O diretório que contém um caminho. | [17](17-vfs-e-arquivos.md#funções-deste-capítulo) |
| `procurar` | Acha a montagem que cuida de um caminho e o resto do caminho depois dela. | [17](17-vfs-e-arquivos.md#funções-deste-capítulo) |
| `SistemaDeArquivos::abrir` | (trait) Abre um arquivo pelo nome dentro da montagem e devolve o conteúdo. | [17](17-vfs-e-arquivos.md#funções-deste-capítulo) |
| `SistemaDeArquivos::listar` | (trait) Os arquivos da montagem, com o tamanho. | [17](17-vfs-e-arquivos.md#funções-deste-capítulo) |

### [src/vfs/sis.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/vfs/sis.rs)

| Função | O que faz | Doc |
|---|---|---|
| `Sis::abrir`, `Sis::listar` | Os arquivos de /sis, gerados na hora pela função de cada um. | [17](17-vfs-e-arquivos.md#funções-deste-capítulo) |
| `tempo`, `mem`, `processos` | Geram o texto de /sis/tempo, /sis/mem e /sis/processos. | [17](17-vfs-e-arquivos.md#funções-deste-capítulo) |

---

## Programas de usuário (`programas/`) e compilação

### [build.rs](https://github.com/moises-dev-git/siliconOS/blob/main/build.rs)

| Função | O que faz | Doc |
|---|---|---|
| `acrescentar_ao_tar` | Escreve um cabeçalho USTAR (com soma de verificação) e os dados, completados até múltiplo de 512. | [13](13-disco-em-ram-e-elf.md#funções-deste-capítulo) |
| `main` (build.rs) | Compila os programas com outro cargo e empacota os ELF em `OUT_DIR/ramdisk.tar`. | [13](13-disco-em-ram-e-elf.md#funções-deste-capítulo) |
| `octal` | Escreve um número em octal, com zeros à esquerda e um 0 no fim, num campo do cabeçalho. | [13](13-disco-em-ram-e-elf.md#funções-deste-capítulo) |

### [programas/build.rs](https://github.com/moises-dev-git/siliconOS/blob/main/programas/build.rs)

| Função | O que faz | Doc |
|---|---|---|
| `main` (programas/build.rs) | Avisa o cargo para religar os programas quando o `linker.ld` mudar. | [13](13-disco-em-ram-e-elf.md#funções-deste-capítulo) |

### [programas/src/bin/args.rs](https://github.com/moises-dev-git/siliconOS/blob/main/programas/src/bin/args.rs)

| Função | O que faz | Doc |
|---|---|---|
| `main` (args) | Mostra argc e cada argumento com o endereço; sai com argc. | [16](16-argumentos.md#funções-deste-capítulo) |

### [programas/src/bin/cat.rs](https://github.com/moises-dev-git/siliconOS/blob/main/programas/src/bin/cat.rs)

| Função | O que faz | Doc |
|---|---|---|
| `main`, `mostrar` (cat) | Mostra cada arquivo pedido, lendo aos pedaços de 64 bytes. | [17](17-vfs-e-arquivos.md#funções-deste-capítulo) |

### [programas/src/bin/conferir.rs](https://github.com/moises-dev-git/siliconOS/blob/main/programas/src/bin/conferir.rs)

| Função | O que faz | Doc |
|---|---|---|
| `main`, `prova` (conferir) | Manda pedidos de arquivo errados e confere cada resposta do kernel. | [17](17-vfs-e-arquivos.md#funções-deste-capítulo) |

### [programas/src/bin/dados.rs](https://github.com/moises-dev-git/siliconOS/blob/main/programas/src/bin/dados.rs)

| Função | O que faz | Doc |
|---|---|---|
| `main` (dados) | Confere que a `.bss` veio zerada, escreve nela, soma 1 ao `CONTADOR` da `.data` e devolve o valor (42). | [13](13-disco-em-ram-e-elf.md#funções-deste-capítulo) |

### [programas/src/bin/insone.rs](https://github.com/moises-dev-git/siliconOS/blob/main/programas/src/bin/insone.rs)

| Função | O que faz | Doc |
|---|---|---|
| `main` (insone) | O mesmo que `soneca`, mas com espera ocupada. | [15](15-dormir-e-bloqueio.md#funções-deste-capítulo) |

### [programas/src/bin/medir.rs](https://github.com/moises-dev-git/siliconOS/blob/main/programas/src/bin/medir.rs)

| Função | O que faz | Doc |
|---|---|---|
| `ciclos` | `rdtsc`: o contador de ciclos da CPU. | [14](14-syscall-sysret.md#funções-deste-capítulo) |
| `main` (medir) | Mede as duas portas e imprime os ciclos por chamada de cada uma. | [14](14-syscall-sysret.md#funções-deste-capítulo) |
| `medir` | Ciclos médios por chamada de uma função, em 100 000 repetições. | [14](14-syscall-sysret.md#funções-deste-capítulo) |
| `tiques_int80` | A chamada `tiques` pela porta antiga, `int 0x80`. | [14](14-syscall-sysret.md#funções-deste-capítulo) |

### [programas/src/bin/oi.rs](https://github.com/moises-dev-git/siliconOS/blob/main/programas/src/bin/oi.rs)

| Função | O que faz | Doc |
|---|---|---|
| `main` (oi) | Diz oi com o próprio pid e devolve 0. | [13](13-disco-em-ram-e-elf.md#funções-deste-capítulo) |

### [programas/src/bin/primos.rs](https://github.com/moises-dev-git/siliconOS/blob/main/programas/src/bin/primos.rs)

| Função | O que faz | Doc |
|---|---|---|
| `main` (primos) | Conta os primos abaixo de 10 000, mostra os 10 primeiros e devolve a quantidade (1229). | [13](13-disco-em-ram-e-elf.md#funções-deste-capítulo) |
| `primo` | `n` é primo? (divisões até a raiz quadrada) | [13](13-disco-em-ram-e-elf.md#funções-deste-capítulo) |

### [programas/src/bin/soneca.rs](https://github.com/moises-dev-git/siliconOS/blob/main/programas/src/bin/soneca.rs)

| Função | O que faz | Doc |
|---|---|---|
| `main` (soneca) | Acorda 5 vezes, a cada 500 ms, dormindo no intervalo. | [15](15-dormir-e-bloqueio.md#funções-deste-capítulo) |

### [programas/src/lib.rs](https://github.com/moises-dev-git/siliconOS/blob/main/programas/src/lib.rs)

| Função | O que faz | Doc |
|---|---|---|
| `__iniciar` | Guarda argc e argv lidos da pilha inicial, chama `main` e sai com o valor devolvido. | [16](16-argumentos.md#funções-deste-capítulo) |
| `_start` (de `principal!`) | (naked) Ponto de entrada do programa: passa o RSP inicial e `main` para `__iniciar`, com a pilha alinhada. | [16](16-argumentos.md#funções-deste-capítulo) |
| `_start` (macro `principal!`) | Ponto de entrada gerado pela macro: chama o `main` do programa e passa o resultado a `sair`. | [13](13-disco-em-ram-e-elf.md#funções-deste-capítulo) |
| `argumento` | O argumento `i` como `&'static str` (anda até o byte 0), ou `None` se não existe. | [16](16-argumentos.md#funções-deste-capítulo) |
| `argumentos` | Iterador sobre todos os argumentos, como o `std::env::args()`. | [16](16-argumentos.md#funções-deste-capítulo) |
| `Arquivo::abrir`, `Arquivo::ler`, `Arquivo::descritor` | Um arquivo aberto, com erros em `Result`. | [17](17-vfs-e-arquivos.md#funções-deste-capítulo) |
| `chamada` | Faz uma chamada de sistema por `syscall`, avisando o compilador que RCX e R11 mudam. | [14](14-syscall-sysret.md#funções-deste-capítulo) |
| `chamada3` | Uma syscall com três argumentos (RDI, RSI, RDX). | [17](17-vfs-e-arquivos.md#funções-deste-capítulo) |
| `Console::write_str` | `fmt::Write`: cada pedaço de texto formatado vira um `escrever` (base de `print!`/`println!`). | [13](13-disco-em-ram-e-elf.md#funções-deste-capítulo) |
| `cru::abrir`, `cru::ler`, `cru::fechar` | As syscalls de arquivo com números e ponteiros crus. | [17](17-vfs-e-arquivos.md#funções-deste-capítulo) |
| `dormir` (sos) | Chama a syscall `dormir`. | [15](15-dormir-e-bloqueio.md#funções-deste-capítulo) |
| `drop` (Arquivo) | Fecha o arquivo quando ele sai de escopo. | [17](17-vfs-e-arquivos.md#funções-deste-capítulo) |
| `escrever` | Chamada de sistema 1: escreve bytes no console e devolve quantos (ou um erro negativo). | [13](13-disco-em-ram-e-elf.md#funções-deste-capítulo) |
| `nome_do_erro` | O texto de um código de erro do kernel. | [17](17-vfs-e-arquivos.md#funções-deste-capítulo) |
| `panic` | Tratador de panic dos programas: imprime a mensagem e sai com 101. | [13](13-disco-em-ram-e-elf.md#funções-deste-capítulo) |
| `pid` | Chamada de sistema 3: o número do processo. | [13](13-disco-em-ram-e-elf.md#funções-deste-capítulo) |
| `quantos_argumentos` | argc: quantos argumentos, contando o nome do programa. | [16](16-argumentos.md#funções-deste-capítulo) |
| `sair` | Chamada de sistema 0: termina o programa com o código dado. | [13](13-disco-em-ram-e-elf.md#funções-deste-capítulo) |
| `tiques` | Chamada de sistema 2: tiques do timer desde o boot. | [13](13-disco-em-ram-e-elf.md#funções-deste-capítulo) |

---

## Testes automáticos

Os `#[test_case]` que rodam com `cargo test` (doc [09](09-shell-e-testes.md)), agrupados da mesma forma.

### src/acpi.rs

| Função | O que faz | Doc |
|---|---|---|
| `dsdt_tem_s5` | Teste: a FADT aponta para um DSDT válido, que tem o objeto `_S5_`. | [09](09-shell-e-testes.md#funções-deste-capítulo) |
| `irq0_desviada_para_gsi2_no_qemu` | Teste: no QEMU, a IRQ 0 (PIT) é desviada para a GSI 2. | [09](09-shell-e-testes.md#funções-deste-capítulo) |
| `irq_sem_desvio_vai_para_a_mesma_gsi` | Teste: IRQ 1 (sem desvio) vai para a GSI 1, ativo alto, por borda. | [09](09-shell-e-testes.md#funções-deste-capítulo) |
| `madt_tem_cpu_e_ioapic` | Teste: a MADT lista pelo menos uma CPU, o APIC local em 0xFEE0_0000 e um I/O APIC. | [09](09-shell-e-testes.md#funções-deste-capítulo) |

### src/cpu.rs

| Função | O que faz | Doc |
|---|---|---|
| `msr_do_apic` | Teste: o MSR 0x1B tem a base 0xFEE0_0000 e o bit 11 (APIC ligado). | [09](09-shell-e-testes.md#funções-deste-capítulo) |
| `sem_interrupcoes_restaura_o_estado` | Teste: `sem_interrupcoes` desliga o IF dentro e o devolve como estava, inclusive aninhada. | [09](09-shell-e-testes.md#funções-deste-capítulo) |

### src/escalonador.rs

| Função | O que faz | Doc |
|---|---|---|
| `dois_contadores_ao_mesmo_tempo` | Teste: dois `contador` juntos levam menos de 300 tiques (um depois do outro seriam 500). | [12](12-escalonador.md#funções-deste-capítulo) |
| `dois_lacos_dividem_a_cpu` | Teste: dois `laco` juntos levam pelo menos 390 tiques de relógio, porque cada um precisa de 200 de CPU. | [12](12-escalonador.md#funções-deste-capítulo) |
| `iniciar` (dos testes) | Ajudante: inicia um programa embutido pelo nome, sem segundo plano. | [12](12-escalonador.md#funções-deste-capítulo) |
| `insone_gasta_a_cpu_e_e_encerrado` | Teste: o `insone` não deixa a CPU parar e é encerrado por tempo esgotado. | [15](15-dormir-e-bloqueio.md#funções-deste-capítulo) |
| `o_kernel_dorme_e_a_cpu_fica_parada` | Teste: `dormir(20)` leva 20 ou 21 tiques, quase todos com o ocioso rodando. | [15](15-dormir-e-bloqueio.md#funções-deste-capítulo) |
| `o_kernel_e_o_processo_zero` | Teste: quem roda os testes é o processo 0, "kernel", `Rodando`. | [12](12-escalonador.md#funções-deste-capítulo) |
| `o_ocioso_e_o_processo_um` | Teste: o processo 1 é o "ocioso" e está `Pronto`. | [15](15-dormir-e-bloqueio.md#funções-deste-capítulo) |
| `processo_terminado_devolve_pilha_e_quadros` | Teste: depois de recolhido, o processo some da tabela e quadros e heap voltam ao que eram. | [12](12-escalonador.md#funções-deste-capítulo) |
| `rodar_do_disco` | Ajudante: roda um programa do disco em RAM e espera o resultado. | [15](15-dormir-e-bloqueio.md#funções-deste-capítulo) |
| `soneca_dorme_sem_gastar_cpu` | Teste: enquanto o `soneca` existe, a CPU fica parada pelo menos 90% do tempo. | [15](15-dormir-e-bloqueio.md#funções-deste-capítulo) |
| `tiques_ociosos` | Ajudante: os tiques do ocioso até agora (tempo de CPU parada). | [15](15-dormir-e-bloqueio.md#funções-deste-capítulo) |

### src/interrupcoes/teclado.rs

| Função | O que faz | Doc |
|---|---|---|
| `enter_e_backspace` | Teste: Enter vira `\n` e Backspace vira `\x08`. | [09](09-shell-e-testes.md#funções-deste-capítulo) |
| `letras_e_numeros` | Teste: apertar e soltar A, 1 e espaço dá "a1 " (as soltas não produzem nada). | [09](09-shell-e-testes.md#funções-deste-capítulo) |
| `shift_e_caps_lock` | Teste: Shift e Caps Lock mudam maiúsculas e símbolos. | [09](09-shell-e-testes.md#funções-deste-capítulo) |
| `teclas_estendidas_ignoradas` | Teste: teclas com prefixo 0xE0 não produzem caractere. | [09](09-shell-e-testes.md#funções-deste-capítulo) |

### src/interrupcoes/timer.rs

| Função | O que faz | Doc |
|---|---|---|
| `hora_rtc_valida` | Teste: a hora do RTC está em faixas válidas (0–23, 0–59, 0–59). | [09](09-shell-e-testes.md#funções-deste-capítulo) |
| `tiques_avancam` | Teste: esperando com `hlt`, chegam 3 tiques e `milissegundos()` passa de 30. | [09](09-shell-e-testes.md#funções-deste-capítulo) |

### src/memoria/espaco.rs

| Função | O que faz | Doc |
|---|---|---|
| `area_do_kernel_recusada` | Teste: mapear no heap ou na entrada 0 dá `AreaDoKernel`, sem vazar quadros. | [11](11-espacos-de-enderecos.md#funções-deste-capítulo) |
| `destruir_devolve_todos_os_quadros` | Teste: duas páginas a 2 MiB uma da outra custam 7 quadros, e o `drop` devolve todos. | [11](11-espacos-de-enderecos.md#funções-deste-capítulo) |
| `kernel_aparece_igual_em_todos` | Teste: um endereço do heap traduz igual num espaço novo e nas tabelas do kernel. | [11](11-espacos-de-enderecos.md#funções-deste-capítulo) |
| `mesmo_endereco_paginas_diferentes` | Teste: dois espaços com quadros diferentes no mesmo endereço; traduções e leituras reais diferentes, nada nas tabelas do kernel. | [11](11-espacos-de-enderecos.md#funções-deste-capítulo) |
| `pagina_com` | Ajudante dos testes: aloca um quadro e escreve um valor no começo dele. | [11](11-espacos-de-enderecos.md#funções-deste-capítulo) |

### src/memoria/heap.rs

| Função | O que faz | Doc |
|---|---|---|
| `alinhar_acima_arredonda` | Teste: `alinhar_acima` com vários valores. | [09](09-shell-e-testes.md#funções-deste-capítulo) |
| `box_mora_no_heap` | Teste: o endereço de um `Box` está dentro do heap. | [09](09-shell-e-testes.md#funções-deste-capítulo) |
| `sem_vazamento_e_blocos_juntados` | Teste: aloca 1000 `Box`, libera metade (deixando buracos), depois o resto; bytes em uso e número de blocos livres voltam ao início. | [09](09-shell-e-testes.md#funções-deste-capítulo) |

### src/memoria/paginacao.rs

| Função | O que faz | Doc |
|---|---|---|
| `erros_de_mapeamento` | Teste: endereço não canônico e desalinhado são recusados com o erro certo. | [09](09-shell-e-testes.md#funções-deste-capítulo) |
| `mapear_e_desmapear` | Teste: mapeia uma página, escreve por ela, lê o mesmo valor pelo quadro físico, desmapeia e a tradução some. | [09](09-shell-e-testes.md#funções-deste-capítulo) |
| `metade_alta_mostra_a_ram` | Teste: o endereço da metade alta + 0x20_0000 traduz para o físico 0x20_0000. | [09](09-shell-e-testes.md#funções-deste-capítulo) |
| `pagina_zero_nao_mapeada` | Teste: a página 0 não está mapeada (ponteiro nulo dá page fault). | [09](09-shell-e-testes.md#funções-deste-capítulo) |

### src/memoria/quadros.rs

| Função | O que faz | Doc |
|---|---|---|
| `alocar_e_liberar_devolve_a_contagem` | Teste: alocar e liberar devolve a contagem de livres ao valor inicial. | [09](09-shell-e-testes.md#funções-deste-capítulo) |

### src/ramdisk.rs

| Função | O que faz | Doc |
|---|---|---|
| `cabecalho_estragado_para_a_leitura` | Teste: trocar um byte do primeiro cabeçalho quebra a soma, e a leitura não devolve nenhum arquivo. | [13](13-disco-em-ram-e-elf.md#funções-deste-capítulo) |
| `octal_do_tar` | Teste: números octais com 0 e com espaço no fim, e um dígito 9 recusado. | [13](13-disco-em-ram-e-elf.md#funções-deste-capítulo) |
| `os_tres_programas_estao_no_disco` | Teste: o tar tem dados, oi e primos, todos ELF, e um nome inexistente dá `None`. | [13](13-disco-em-ram-e-elf.md#funções-deste-capítulo) |

### src/shell.rs

| Função | O que faz | Doc |
|---|---|---|
| `editar_linha` | Teste: digitar, apagar com backspace (até mais vezes que letras) e Tab ignorado deixam a linha certa. | [09](09-shell-e-testes.md#funções-deste-capítulo) |
| `ler_numero_decimal_e_hexadecimal` | Teste: `ler_numero` com decimais, hexadecimais, separadores e entradas inválidas. | [09](09-shell-e-testes.md#funções-deste-capítulo) |
| `linha_tem_limite` | Teste: a linha não passa do tamanho máximo. | [09](09-shell-e-testes.md#funções-deste-capítulo) |
| `nomes_de_comandos_unicos` | Teste: não há dois comandos com o mesmo nome na tabela. | [09](09-shell-e-testes.md#funções-deste-capítulo) |

### src/testes.rs

| Função | O que faz | Doc |
|---|---|---|
| `este_teste_falha` | Teste que falha de propósito (feature `teste-que-falha`), para mostrar a saída de uma falha. | [09](09-shell-e-testes.md#funções-deste-capítulo) |
| `executar` | O `test_runner`: recebe a lista gerada pelo compilador, roda cada teste e sai do QEMU com sucesso. | [09](09-shell-e-testes.md#funções-deste-capítulo) |

### src/usuario/argumentos.rs

| Função | O que faz | Doc |
|---|---|---|
| `argumentos_ruins_recusados` | Teste: cada erro de `montar`, e o limite exato de uma página (4053 cabe, 4054 não). | [16](16-argumentos.md#funções-deste-capítulo) |
| `ler_u64` | Ajudante: lê o número de 8 bytes que vai morar num endereço da pilha montada. | [16](16-argumentos.md#funções-deste-capítulo) |
| `pilha_inicial_no_formato_system_v` | Teste: argc, ponteiros, zeros e textos nos lugares certos, RSP alinhado em 16. | [16](16-argumentos.md#funções-deste-capítulo) |

### src/usuario/elf.rs

| Função | O que faz | Doc |
|---|---|---|
| `argumentos_chegam_ao_programa` | Teste: `args` sai com argc; `primos 100` sai com 25; `primos abc` sai com -1. | [16](16-argumentos.md#funções-deste-capítulo) |
| `argumentos_demais_recusados_sem_vazar` | Teste: 33 argumentos são recusados por `iniciar_elf` sem alocar nenhum quadro. | [16](16-argumentos.md#funções-deste-capítulo) |
| `arquivos_ruins_recusados_sem_vazar` | Teste: oito estragos diferentes, cada um recusado com o erro certo, sem perder quadros. | [13](13-disco-em-ram-e-elf.md#funções-deste-capítulo) |
| `cabecalho_do_oi` | Teste: o primeiro segmento de `oi` é LOAD, R-X, em `0x2000_0000_0000`, e contém a entrada. | [13](13-disco-em-ram-e-elf.md#funções-deste-capítulo) |
| `carregar_alterado` | Ajudante: carrega uma cópia de `oi` modificada por uma closure. | [13](13-disco-em-ram-e-elf.md#funções-deste-capítulo) |
| `programas_em_rust_rodam` | Teste: oi, primos e dados saem com 0, 1229 e 42. | [13](13-disco-em-ram-e-elf.md#funções-deste-capítulo) |
| `rodar` (dos testes) | Ajudante: carrega um ELF do disco, roda e espera o resultado. | [13](13-disco-em-ram-e-elf.md#funções-deste-capítulo) |
| `rodar_com` | Ajudante: roda um programa do disco com um argv e espera o resultado. | [16](16-argumentos.md#funções-deste-capítulo) |

### src/usuario/mod.rs

| Função | O que faz | Doc |
|---|---|---|
| `invasor_encerrado_por_page_fault` | Teste: `invasor` (lê memória do kernel) termina por #PF. | [10](10-modo-usuario.md#funções-deste-capítulo) |
| `laco_encerrado_pelo_timer` | Teste: `laco` (laço infinito) é encerrado pelo timer. | [10](10-modo-usuario.md#funções-deste-capítulo) |
| `milissegundos_viram_tiques_para_cima` | Teste: 1 ms vira 1 tique, 11 ms viram 2, `u64::MAX` não estoura. | [15](15-dormir-e-bloqueio.md#funções-deste-capítulo) |
| `nada_fica_mapeado_nem_vaza` | Teste: depois de um aquecimento, rodar `ola` deixa os endereços do programa desmapeados e os quadros livres no mesmo número. | [10](10-modo-usuario.md#funções-deste-capítulo) |
| `ola_sai_com_42` | Teste: `ola` escreve e sai com 42. | [10](10-modo-usuario.md#funções-deste-capítulo) |
| `ponteiro_do_kernel_recusado` | Teste: `ponteiro` passa um endereço do kernel, recebe -14 e sai com 1. | [10](10-modo-usuario.md#funções-deste-capítulo) |
| `privilegiado_encerrado_por_gp` | Teste: `privilegiado` (`cli` no anel 3) termina por #GP. | [10](10-modo-usuario.md#funções-deste-capítulo) |
| `relogio_usa_syscall_de_tiques` | Teste: `relogio` espera 100 tiques pela syscall e sai com 100. | [10](10-modo-usuario.md#funções-deste-capítulo) |
| `validacao_de_ponteiros` | Teste: `faixa_do_programa` aceita e recusa os casos de borda certos. | [10](10-modo-usuario.md#funções-deste-capítulo) |

### src/usuario/syscall.rs

| Função | O que faz | Doc |
|---|---|---|
| `as_duas_portas_funcionam_no_mesmo_programa` | Teste: `medir` (que usa `int 0x80` e `syscall`) sai com 0. | [14](14-syscall-sysret.md#funções-deste-capítulo) |
| `msrs_configurados` | Teste: EFER.SCE ligado e STAR, LSTAR e FMASK com os valores esperados. | [14](14-syscall-sysret.md#funções-deste-capítulo) |

### src/vfs/mod.rs

| Função | O que faz | Doc |
|---|---|---|
| `abrir_e_ler_aos_pedacos` | Teste: ler de 7 em 7 bytes dá o arquivo inteiro e depois 0; diretório e inexistente não abrem. | [17](17-vfs-e-arquivos.md#funções-deste-capítulo) |
| `cat_le_do_disco_e_de_sis` | Teste: o `cat` sai com 0, 1 (arquivo que falta) ou 2 (sem argumentos). | [17](17-vfs-e-arquivos.md#funções-deste-capítulo) |
| `descritores_reaproveitam_o_menor_livre` | Teste: 3 e 4, o 3 volta depois de fechado, números inválidos recusados, tabela cheia em 16. | [17](17-vfs-e-arquivos.md#funções-deste-capítulo) |
| `kernel_se_defende_e_libera_o_que_ficou_aberto` | Teste: o `conferir` passa em todas as provas, e os 16 arquivos que ele deixa abertos voltam para o heap. | [17](17-vfs-e-arquivos.md#funções-deste-capítulo) |
| `montagens_escolhidas_pelo_caminho` | Teste: cada caminho cai na montagem certa, inclusive `/sistema` (que não é de `/sis`). | [17](17-vfs-e-arquivos.md#funções-deste-capítulo) |
| `raiz_mostra_a_montagem_sis` | Teste: `listar("/")` inclui `sis/` e `leiame.txt`; arquivo não é diretório. | [17](17-vfs-e-arquivos.md#funções-deste-capítulo) |
| `rodar_com` | Ajudante: roda um programa do disco com um argv e espera o resultado. | [17](17-vfs-e-arquivos.md#funções-deste-capítulo) |
