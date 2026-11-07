#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)] // permite extern "x86-interrupt" (idt.rs)
// Framework de testes próprio para `cargo test` (src/testes.rs, docs/09-shell-e-testes.md).
#![feature(custom_test_frameworks)]
#![test_runner(crate::testes::executar)]
#![reexport_test_harness_main = "rodar_testes"]

// A crate `alloc` (Box, Vec, String) passa a existir porque o kernel agora
// tem um alocador global (src/memoria/heap.rs, docs/07-heap.md).
extern crate alloc;

mod acpi;
mod console;
mod cpu;
mod gdt;
mod idt;
#[allow(dead_code)] // partes usadas só pelas features de teste
mod interrupcoes;
mod memoria;
mod panic;
#[cfg_attr(not(test), allow(dead_code))] // só o `cargo test` encerra o QEMU
mod qemu;
mod serial;
mod shell;
#[cfg(test)]
mod testes;
mod usuario;
mod escalonador;
mod ramdisk;

use core::fmt::Write;
use spin::Mutex;
use uefi::boot::MemoryType;
use uefi::mem::memory_map::MemoryMap;
use uefi::prelude::*;
use uefi::proto::console::gop::GraphicsOutput;

use console::{Console, FrameBuffer};
use serial::Serial;

/// Console global, protegido por um spinlock (ainda não há threads,
/// mas assim o `kprintln!` pode ser chamado de qualquer lugar com segurança).
pub static CONSOLE: Mutex<Option<Console>> = Mutex::new(None);

/// Onde o UEFI carregou o nosso .efi (endereço e tamanho). Serve para
/// depurar com o GDB: os endereços do arquivo começam em 0x1_4000_0000.
static IMAGEM_BASE: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
static IMAGEM_TAMANHO: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// Imprime na serial E na tela.
#[macro_export]
macro_rules! kprintln {
    () => { $crate::kprint!("\n") };
    ($($arg:tt)*) => { $crate::kprint!("{}\n", format_args!($($arg)*)) };
}

#[macro_export]
macro_rules! kprint {
    ($($arg:tt)*) => { $crate::_print(format_args!($($arg)*)) };
}

pub fn _print(args: core::fmt::Arguments) {
    // Com as interrupções desligadas enquanto o lock do CONSOLE está pego,
    // nenhum tratador consegue interromper no meio e pedir o mesmo lock
    // (docs/08-interrupcoes-de-hardware.md). `teste-deadlock` tira essa
    // proteção para mostrar o que acontece sem ela.
    #[cfg(not(feature = "teste-deadlock"))]
    cpu::sem_interrupcoes(|| escrever(args));
    #[cfg(feature = "teste-deadlock")]
    escrever(args);
}

fn escrever(args: core::fmt::Arguments) {
    let _ = Serial.write_fmt(args);
    if let Some(con) = CONSOLE.lock().as_mut() {
        let _ = con.write_fmt(args);
    }
}

#[entry]
fn main() -> Status {
    // ===== FASE UEFI: o firmware ainda nos ajuda =====
    uefi::helpers::init().unwrap();
    uefi::println!("Carregador UEFI iniciado.");

    // Pega as informações do framebuffer ANTES de desligar o UEFI.
    // O bloco garante que o protocolo GOP seja fechado (drop) aqui dentro.
    let fb = {
        let handle = uefi::boot::get_handle_for_protocol::<GraphicsOutput>().unwrap();
        let mut gop = uefi::boot::open_protocol_exclusive::<GraphicsOutput>(handle).unwrap();
        let info = gop.current_mode_info();
        let (width, height) = info.resolution();
        FrameBuffer {
            base: gop.frame_buffer().as_mut_ptr() as *mut u32,
            width,
            height,
            stride: info.stride(),
        }
    };

    // Onde estão as tabelas ACPI (só o UEFI sabe; capítulo 9).
    acpi::guardar_rsdp();

    // Onde o firmware carregou o kernel (para o GDB; doc 09).
    {
        use core::sync::atomic::Ordering::Relaxed;
        use uefi::proto::loaded_image::LoadedImage;
        let imagem = uefi::boot::open_protocol_exclusive::<LoadedImage>(uefi::boot::image_handle()).unwrap();
        let (base, tamanho) = imagem.info();
        IMAGEM_BASE.store(base as u64, Relaxed);
        IMAGEM_TAMANHO.store(tamanho, Relaxed);
    }

    uefi::println!("Desligando os servicos de boot do UEFI...");
    let memory_map = unsafe { uefi::boot::exit_boot_services(MemoryType::LOADER_DATA) };

    // ===== BARE-METAL: a partir daqui a máquina é nossa =====
    kernel_main(fb, &memory_map);
}

fn kernel_main(fb: FrameBuffer, memory_map: &impl MemoryMap) -> ! {
    // Interrupções desligadas: a IDT ainda é a do firmware e aponta para o
    // seletor de código DELE, que deixa de existir quando trocarmos a GDT.
    // Elas só são religadas em interrupcoes::init() (doc 08).
    unsafe { core::arch::asm!("cli", options(nomem, nostack)) };

    Serial::init();
    *CONSOLE.lock() = Some(Console::new(fb));

    kprintln!("=== SiliconOS ===");
    kprintln!("Kernel bare-metal iniciado (UEFI desligado).");
    kprintln!("Video: {}x{} (stride {})", fb.width, fb.height, fb.stride);
    {
        use core::sync::atomic::Ordering::Relaxed;
        kprintln!("Imagem do kernel: base {:#x}, {} KiB (no arquivo, a base e 0x140000000)",
                  IMAGEM_BASE.load(Relaxed), IMAGEM_TAMANHO.load(Relaxed) / 1024);
    }

    // ---- GDT + TSS (docs/03-gdt-e-tss.md) ----
    let antes = gdt::estado();
    kprintln!("GDT do firmware:  base={:#x} CS={:#x} SS={:#x} TR={:#x}",
              antes.base, antes.cs, antes.ss, antes.tr);
    gdt::init();
    let depois = gdt::estado();
    kprintln!("GDT do SiliconOS: base={:#x} limite={} CS={:#x} SS={:#x} DS={:#x} TR={:#x}",
              depois.base, depois.limite, depois.cs, depois.ss, depois.ds, depois.tr);
    kprintln!("Pilha IST1 (double fault): topo em {:#x}", gdt::topo_pilha_double_fault());
    kprintln!("Pilha do anel 0 para o anel 3 (TSS.RSP0): topo em {:#x}", gdt::topo_pilha_anel0());

    // ---- IDT + exceções (docs/04-idt-e-excecoes.md) ----
    idt::init();
    let (idt_base, idt_limite) = idt::estado();
    kprintln!("IDT do SiliconOS: base={:#x} limite={} (256 entradas)", idt_base, idt_limite);
    testes_de_excecao();

    // ---- Memória física (docs/05-memoria-fisica.md) ----
    let mapa = memoria::mapa::init(memory_map);
    imprimir_mapa(mapa, fb);
    memoria::quadros::init(mapa);
    imprimir_alocador();
    testes_de_memoria();

    // ---- Paginação (docs/06-paginacao.md) ----
    let rsp: u64;
    unsafe { core::arch::asm!("mov {}, rsp", out(reg) rsp, options(nomem, nostack)) };
    let codigo = kernel_main as fn(FrameBuffer, &uefi::mem::memory_map::MemoryMapOwned) -> ! as *const () as u64;
    kprintln!("Paginacao: tabelas do UEFI (CR3={:#x})", memoria::paginacao::raiz_atual());
    explicar_traducao("codigo", codigo);
    explicar_traducao("pilha", rsp);
    explicar_traducao("nulo", 0);

    let tamanho_fb = (fb.stride * fb.height * 4) as u64;
    let r = memoria::paginacao::init(mapa, fb.base as u64, tamanho_fb);
    kprintln!("Paginacao: tabelas do KERNEL ativas (CR3 {:#x} -> {:#x}, {} quadros usados)",
              r.raiz_uefi, r.raiz_kernel, r.quadros_usados);
    explicar_traducao("codigo", codigo);
    explicar_traducao("pilha", rsp);
    explicar_traducao("nulo", 0);
    explicar_traducao("fis 1MiB",
                      memoria::paginacao::DESLOCAMENTO_FISICO + memoria::INICIO_UTILIZAVEL);
    testes_de_paginacao();

    // ---- Heap (docs/07-heap.md) ----
    memoria::heap::init();
    demonstrar_heap();
    testes_de_heap();

    // Compile com `--features teste-panic` para ver o panic handler agindo.
    #[cfg(feature = "teste-panic")]
    {
        let v: [u32; 3] = [1, 2, 3];
        let i = memoria::quadros::livres(); // índice inválido de propósito
        kprintln!("Testando panic: acessando v[{}]...", i);
        kprintln!("{}", v[i]);
    }

    // ---- Escalonador (docs/12-escalonador.md) ----
    // Antes das interrupções: o primeiro tique já chama o escalonador. O
    // código que está rodando agora vira o processo 0.
    escalonador::init();
    // `syscall`/`sysret` (docs/14-syscall-sysret.md): MSRs STAR, LSTAR, FMASK e EFER.SCE.
    usuario::syscall::init();

    // ---- Interrupções de hardware (docs/08-interrupcoes-de-hardware.md) ----
    interrupcoes::init();
    testes_de_interrupcoes();

    // `cargo test`: roda os #[test_case] e encerra o QEMU (não volta).
    #[cfg(test)]
    rodar_testes();

    // ---- Shell (docs/09-shell-e-testes.md) ----
    kprintln!();
    shell::rodar();
}

/// Testes das interrupções de hardware:
///   ./run.sh teste-timer              confere o timer contra o relógio CMOS (RTC)
///   ./run.sh teste-sem-eoi            tratador do timer sem EOI -> o timer para
///   ./run.sh teste-print-interrupcao  kprint! dentro do tratador do timer: funciona
///   ./run.sh teste-deadlock           o mesmo sem a proteção no _print -> trava
fn testes_de_interrupcoes() {
    use interrupcoes::timer;

    #[cfg(feature = "teste-timer")]
    {
        // Espera o segundo do RTC virar, para começar no início de um segundo.
        let mut s = timer::segundos_rtc();
        while timer::segundos_rtc() == s {}
        s = timer::segundos_rtc();
        kprintln!("Teste timer: contando tiques a cada segundo do RTC (esperado: {})", timer::FREQUENCIA_HZ);
        let mut inicio = timer::tiques();
        let mut total = 0;
        for _ in 0..5 {
            while timer::segundos_rtc() == s {
                unsafe { core::arch::asm!("hlt") }; // acorda a cada tique
            }
            s = timer::segundos_rtc();
            let agora = timer::tiques();
            kprintln!("  segundo {:>2} do RTC: {} tiques", s, agora - inicio);
            total += agora - inicio;
            inicio = agora;
        }
        let media = total / 5;
        assert!(media.abs_diff(timer::FREQUENCIA_HZ) <= timer::FREQUENCIA_HZ / 10,
                "timer fora de 10%: {} tiques por segundo", media);
        kprintln!("Teste timer OK: media de {} tiques/s; {} ms desde que o timer ligou; espurias: {}",
                  media, timer::milissegundos(), interrupcoes::espurias());
    }

    #[cfg(feature = "teste-sem-eoi")]
    {
        // Sem `hlt`: se o timer parar, nada mais acordaria a CPU.
        kprintln!("Teste: tratador do timer SEM EOI. Esperando 2 s pelo RTC...");
        for _ in 0..2 {
            let s = timer::segundos_rtc();
            while timer::segundos_rtc() == s {}
        }
        kprintln!("  tiques depois de 2 s: {} (com EOI seriam ~{})", timer::tiques(), 2 * timer::FREQUENCIA_HZ);
        kprintln!("  teclas recebidas: {} (o teclado tambem ficou bloqueado?)",
                  interrupcoes::teclado::recebidos());
    }

    #[cfg(feature = "teste-print-interrupcao")]
    {
        // O laço principal imprime sem parar; o tratador do timer imprime
        // "[tique N]" a cada 10 tiques. Os dois disputam o lock do CONSOLE.
        kprintln!("Teste: imprimindo no laco principal E no tratador do timer...");
        let mut linha = 0u64;
        while timer::tiques() < 300 {
            linha += 1;
            kprintln!("laco principal: linha {} (tiques: {})", linha, timer::tiques());
            // Um respiro com as interrupções LIGADAS. Sem ele, a janela entre
            // um kprintln! e o próximo tem poucas instruções e o QEMU quase
            // nunca entrega o timer nela (ver doc 08).
            for _ in 0..1000 { core::hint::spin_loop(); }
        }
        kprintln!("Teste OK: 300 tiques e {} linhas sem travar.", linha);
    }
    let _ = timer::tiques;
}

/// Provoca exceções de propósito, conforme a feature usada na compilação:
///   ./run.sh teste-breakpoint   int3 -> tratador imprime e o kernel CONTINUA
///   ./run.sh teste-divisao      div por zero em assembly -> #DE
///   ./run.sh teste-opcode       ud2 -> #UD
///   ./run.sh teste-pagefault    lê um endereço não mapeado -> #PF
///   ./run.sh teste-gpf          lê um endereço não-canônico -> #GP
///   ./run.sh teste-doublefault  pilha apontando para o nada -> #PF impossível de
///                               entregar -> #DF, tratado na pilha IST1
fn testes_de_excecao() {
    #[cfg(feature = "teste-breakpoint")]
    {
        kprintln!("Teste: executando int3...");
        unsafe { core::arch::asm!("int3") };
        kprintln!("Teste: voltamos do breakpoint normalmente.");
    }
    #[cfg(feature = "teste-divisao")]
    unsafe {
        // Em Rust, `x / 0` vira panic antes de chegar à CPU; em assembly não.
        kprintln!("Teste: dividindo por zero com a instrucao div...");
        core::arch::asm!("xor edx, edx", "mov eax, 1", "xor ecx, ecx", "div ecx",
                         out("eax") _, out("edx") _, out("ecx") _);
    }
    #[cfg(feature = "teste-opcode")]
    unsafe {
        kprintln!("Teste: executando ud2 (instrucao invalida de proposito)...");
        core::arch::asm!("ud2");
    }
    #[cfg(feature = "teste-pagefault")]
    unsafe {
        let endereco = 0x0000_4000_0000_0000u64 as *const u64; // 64 TiB: nada mapeado lá
        kprintln!("Teste: lendo {:p}...", endereco);
        let _ = core::ptr::read_volatile(endereco);
    }
    #[cfg(feature = "teste-gpf")]
    unsafe {
        // Endereço "não-canônico": os bits 48-63 não repetem o bit 47.
        // Não é sequer um endereço válido em x86-64 -> #GP (não #PF).
        let endereco = 0x8000_0000_0000_0000u64 as *const u64;
        kprintln!("Teste: lendo endereco nao-canonico {:p}...", endereco);
        let _ = core::ptr::read_volatile(endereco);
    }
    #[cfg(feature = "teste-doublefault")]
    unsafe {
        kprintln!("Teste: apontando RSP para memoria nao mapeada e fazendo push...");
        core::arch::asm!("mov rsp, 0x400000000000", "push rax", options(noreturn));
    }
}

const MIB: u64 = 1024 * 1024;

/// Resumo do mapa de memória e de onde estão as coisas importantes.
fn imprimir_mapa(mapa: &memoria::mapa::Mapa, fb: FrameBuffer) {
    use memoria::mapa::Tipo;
    use memoria::TAMANHO_QUADRO;

    kprintln!("Mapa de memoria: {} entradas do UEFI -> {} regioes (vizinhas unidas)",
              mapa.entradas_uefi, mapa.regioes().len());
    kprintln!("  {:<13}{:>8}{:>10}", "tipo", "regioes", "MiB");
    for tipo in Tipo::TODOS {
        let (n, quadros) = mapa.total(tipo);
        if n > 0 {
            kprintln!("  {:<13}{:>8}{:>10}", tipo.nome(), n, quadros * TAMANHO_QUADRO / MIB);
        }
    }
    kprintln!("  fim da RAM: {:#x} ({} MiB)", mapa.fim_da_ram(), mapa.fim_da_ram() / MIB);

    // `./run.sh listar-regioes` mostra o mapa completo, região por região.
    #[cfg(feature = "listar-regioes")]
    for r in mapa.regioes() {
        kprintln!("    {:#014x} - {:#014x}  {:>9} KiB  {}",
                  r.inicio, r.fim(), r.quadros * TAMANHO_QUADRO / 1024, r.tipo.nome());
    }

    // Onde estão as coisas que o kernel está usando agora mesmo?
    let rsp: u64;
    unsafe { core::arch::asm!("mov {}, rsp", out(reg) rsp, options(nomem, nostack)) };
    let onde = |endereco: u64| mapa.encontrar(endereco).map_or("fora do mapa", |r| r.tipo.nome());
    let codigo = imprimir_mapa as *const () as u64; // endereço de uma função nossa
    let gdt_base = gdt::estado().base;
    kprintln!("  codigo do kernel {:#x} -> {}", codigo, onde(codigo));
    kprintln!("  GDT              {:#x} -> {}", gdt_base, onde(gdt_base));
    kprintln!("  pilha atual(RSP) {:#x} -> {}", rsp, onde(rsp));
    kprintln!("  framebuffer      {:#x} -> {}", fb.base as u64, onde(fb.base as u64));
}

fn imprimir_alocador() {
    let guarda = memoria::quadros::QUADROS.lock();
    let a = guarda.as_ref().unwrap();
    kprintln!("Alocador de quadros: bitmap de {} KiB em {:#x}",
              a.bitmap_quadros * 4, a.bitmap_endereco);
    kprintln!("  {} quadros livres ({} MiB) de {} cobertos",
              a.livres(), a.livres() as u64 * memoria::TAMANHO_QUADRO / MIB, a.total());
}

/// Testes do alocador de quadros:
///   ./run.sh teste-quadros            aloca, escreve, confere, libera, realoca
///   ./run.sh teste-liberacao-dupla    libera o mesmo quadro duas vezes -> panic
fn testes_de_memoria() {
    #[cfg(feature = "teste-quadros")]
    {
        use memoria::mapa::Tipo;
        use memoria::quadros::{self, Quadro};
        use memoria::TAMANHO_QUADRO;

        const N: usize = 1000;
        let livres_antes = quadros::livres();
        let mut lista = [Quadro(0); N];

        // 1. Alocar N quadros.
        for item in lista.iter_mut() {
            *item = quadros::alocar().expect("memoria acabou durante o teste");
        }
        assert_eq!(quadros::livres(), livres_antes - N);

        // 2. Cada quadro: alinhado, em região Livre, acima de 1 MiB, sem repetição.
        let mapa = memoria::mapa::get();
        for (i, q) in lista.iter().enumerate() {
            assert!(q.endereco() % TAMANHO_QUADRO == 0, "quadro desalinhado");
            assert!(q.endereco() >= memoria::INICIO_UTILIZAVEL, "quadro abaixo de 1 MiB");
            let r = mapa.encontrar(q.endereco()).expect("quadro fora do mapa");
            assert!(r.tipo == Tipo::Livre, "quadro {:#x} numa regiao {}", q.endereco(), r.tipo.nome());
            assert!(!lista[..i].contains(q), "quadro {:#x} entregue duas vezes", q.endereco());
        }

        // 3. Escrever um padrão diferente em cada quadro e conferir depois.
        //    Prova que é RAM de verdade e que os quadros não se sobrepõem.
        for (i, q) in lista.iter().enumerate() {
            let p = q.endereco() as *mut u64;
            for j in 0..512 {
                unsafe { p.add(j).write_volatile((i as u64) << 32 | j as u64) };
            }
        }
        for (i, q) in lista.iter().enumerate() {
            let p = q.endereco() as *const u64;
            for j in 0..512 {
                let lido = unsafe { p.add(j).read_volatile() };
                assert_eq!(lido, (i as u64) << 32 | j as u64, "padrao corrompido no quadro {:#x}", q.endereco());
            }
        }

        // 4. Liberar tudo: a contagem volta ao valor inicial.
        for q in lista.iter() {
            quadros::liberar(*q);
        }
        assert_eq!(quadros::livres(), livres_antes);

        // 5. Realocar: o primeiro quadro entregue deve ser o mesmo de antes.
        let de_novo = quadros::alocar().unwrap();
        assert_eq!(de_novo, lista[0]);
        quadros::liberar(de_novo);

        kprintln!("Teste de quadros OK: {} alocados ({:#x}..{:#x}), escritos,",
                  N, lista[0].endereco(), lista[N - 1].endereco());
        kprintln!("  conferidos, liberados e realocados. Livres: {}", quadros::livres());
    }

    #[cfg(feature = "teste-liberacao-dupla")]
    {
        let q = memoria::quadros::alocar().unwrap();
        kprintln!("Teste: liberando o quadro {:#x} duas vezes...", q.endereco());
        memoria::quadros::liberar(q);
        memoria::quadros::liberar(q); // <- deve dar panic
    }
}

/// Mostra como a CPU traduziria um endereço virtual com as tabelas ativas:
/// os 4 índices (um por nível) e o resultado.
fn explicar_traducao(rotulo: &str, virt: u64) {
    use memoria::paginacao::{indices, traduzir};
    let i = indices(virt);
    match traduzir(virt) {
        Some(t) => kprintln!("  {:<8} {:#018x} [{:>3} {:>3} {:>3} {:>3}] -> {:#x} ({})",
                             rotulo, virt, i[0], i[1], i[2], i[3], t.fisico, t.tamanho.nome()),
        None => kprintln!("  {:<8} {:#018x} [{:>3} {:>3} {:>3} {:>3}] -> NAO MAPEADO",
                          rotulo, virt, i[0], i[1], i[2], i[3]),
    }
}

/// Testes de paginação:
///   ./run.sh teste-paginacao     mapeia, escreve, lê por outro caminho, desmapeia
///   ./run.sh teste-desmapeado    acessa uma página depois de desmapeá-la -> #PF
///   ./run.sh teste-nulo          lê o endereço 0 -> #PF (a página 0 não é mapeada)
fn testes_de_paginacao() {
    #[cfg(feature = "teste-paginacao")]
    {
        use memoria::paginacao::{self as pg, flags, ErroMapeamento, TamanhoPagina};
        use memoria::quadros;

        // Um endereço virtual que nada usa: 80 TiB, metade baixa.
        const V1: u64 = 0x0000_5000_0000_0000;
        const V2: u64 = 0x0000_5000_0000_1000; // a página seguinte

        let livres_antes = quadros::livres();
        assert!(pg::traduzir(V1).is_none(), "V1 ja estava mapeado");

        // 1. Mapear V1 num quadro novo e conferir a tradução.
        let q = pg::alocar_e_mapear(V1, flags::ESCRITA).expect("mapear V1");
        let t = pg::traduzir(V1).expect("V1 deveria estar mapeado");
        assert_eq!(t.fisico, q.endereco());
        assert_eq!(t.tamanho, TamanhoPagina::K4);

        // 2. Escrever por V1 e ler o MESMO quadro pela metade alta.
        unsafe { (V1 as *mut u64).write_volatile(0xC0FFEE) };
        let pela_metade_alta = pg::fisico_para_virtual(q.endereco()) as *const u64;
        assert_eq!(unsafe { pela_metade_alta.read_volatile() }, 0xC0FFEE);

        // 3. Dois endereços virtuais para o mesmo quadro (aliasing).
        pg::mapear(V2, q.endereco(), TamanhoPagina::K4, flags::ESCRITA).expect("mapear V2");
        unsafe { (V2 as *mut u64).add(1).write_volatile(0xBEEF) };
        assert_eq!(unsafe { (V1 as *const u64).add(1).read_volatile() }, 0xBEEF);

        // 4. Erros esperados.
        assert_eq!(pg::mapear(V1, q.endereco(), TamanhoPagina::K4, 0), Err(ErroMapeamento::JaMapeado));
        assert_eq!(pg::mapear(V1 + 8, q.endereco(), TamanhoPagina::K4, 0), Err(ErroMapeamento::Desalinhado));
        assert_eq!(pg::mapear(0x8000_0000_0000_0000, q.endereco(), TamanhoPagina::K4, 0),
                   Err(ErroMapeamento::NaoCanonico));

        // 5. Desmapear os dois e devolver o quadro.
        assert_eq!(pg::desmapear(V2), Ok(q.endereco()));
        assert_eq!(pg::desmapear(V1), Ok(q.endereco()));
        assert!(pg::traduzir(V1).is_none() && pg::traduzir(V2).is_none());
        assert_eq!(pg::desmapear(V1), Err(ErroMapeamento::NaoMapeado));
        quadros::liberar(q);

        // As 3 tabelas intermediárias criadas para V1 (PDPT, PD, PT) continuam alocadas.
        kprintln!("Teste de paginacao OK: mapear, traduzir, escrever/ler por 2 caminhos,");
        kprintln!("  aliasing, erros e desmapear. Quadros em tabelas novas: {}",
                  livres_antes - quadros::livres());
    }

    #[cfg(feature = "teste-desmapeado")]
    {
        use memoria::paginacao::{self as pg, flags};
        const V: u64 = 0x0000_5000_0000_0000;
        let q = pg::alocar_e_mapear(V, flags::ESCRITA).expect("mapear");
        unsafe { (V as *mut u64).write_volatile(42) };
        kprintln!("Teste: escrevi em {:#x}; desmapeando e lendo de novo...", V);
        pg::desmapear(V).unwrap();
        memoria::quadros::liberar(q);
        unsafe { core::arch::asm!("mov rax, [{}]", in(reg) V, out("rax") _) };
    }

    #[cfg(feature = "teste-nulo")]
    {
        kprintln!("Teste: lendo o endereco 0 (ponteiro nulo)...");
        // Em assembly de propósito: em Rust, ler um ponteiro nulo é
        // comportamento indefinido e o compilador pode fazer qualquer coisa.
        unsafe { core::arch::asm!("mov rax, [0]", out("rax") _) };
    }
}

/// Usa o heap no boot normal: a prova de que Box, Vec e String funcionam.
fn demonstrar_heap() {
    use alloc::boxed::Box;
    use alloc::string::String;
    use alloc::vec::Vec;
    use memoria::heap::HEAP_INICIO;

    let caixa = Box::new(42u64);
    let quadrados: Vec<u64> = (1..=8).map(|x| x * x).collect();
    let mut texto = String::from("SiliconOS");
    texto.push_str(" tem heap");

    kprintln!("Heap: faixa virtual a partir de {:#x}", HEAP_INICIO);
    kprintln!("  Box::new(42)      -> {} em {:p}", caixa, &*caixa);
    kprintln!("  Vec (quadrados)   -> {:?}", quadrados);
    kprintln!("  String            -> \"{}\" ({} bytes)", texto, texto.len());

    #[cfg(not(feature = "teste-bump"))]
    {
        let e = memoria::heap::estatisticas();
        kprintln!("  em uso: {} bytes em {} alocacoes; mapeado: {} KiB; blocos livres: {}",
                  e.em_uso, e.alocacoes - e.liberacoes, e.mapeado / 1024, e.blocos_livres);
    }
}

/// Testes do heap:
///   ./run.sh teste-heap                  Box, Vec crescendo, String, milhares de
///                                        alocações, alinhamento e vazamento zero
///   ./run.sh teste-heap-liberacao-dupla  libera o mesmo bloco duas vezes -> panic
///   ./run.sh teste-heap-esgotar          aloca 1 MiB por vez até o limite -> panic
///   ./run.sh teste-heap-executar         executa código gravado no heap -> #PF (NX)
///   ./run.sh teste-bump                  usa o alocador bump: a memória nunca volta
fn testes_de_heap() {
    #[cfg(feature = "teste-heap")]
    {
        use alloc::boxed::Box;
        use alloc::format;
        use alloc::string::String;
        use alloc::vec::Vec;
        use core::alloc::Layout;
        use memoria::heap::{self, HEAP_INICIO, TAMANHO_INICIAL};

        let antes = heap::estatisticas();
        let quadros_antes = memoria::quadros::livres();

        // 1. Box: o valor mora no heap.
        let b = Box::new(0xDEAD_BEEFu64);
        let endereco = &*b as *const u64 as u64;
        assert_eq!(*b, 0xDEAD_BEEF);
        assert!(heap::contem(endereco), "Box fora do heap: {:#x}", endereco);
        drop(b);

        // 2. Vec crescendo além do heap inicial (200 000 u64 = 1,6 MB > 1 MiB).
        let mut v: Vec<u64> = Vec::new();
        for i in 0..200_000u64 {
            v.push(i);
        }
        let soma: u64 = v.iter().sum();
        assert_eq!(soma, 199_999 * 200_000 / 2);
        let durante = heap::estatisticas();
        assert!(durante.mapeado > TAMANHO_INICIAL, "o heap deveria ter crescido");
        kprintln!("Teste heap: Vec de {} u64 ok; heap cresceu de {} KiB para {} KiB",
                  v.len(), TAMANHO_INICIAL / 1024, durante.mapeado / 1024);
        drop(v);

        // 3. String e format!.
        let mut s = String::new();
        for i in 0..100 {
            s.push_str(&format!("{},", i));
        }
        assert!(s.starts_with("0,1,2,") && s.ends_with("98,99,"));
        assert_eq!(s.len(), 290);
        drop(s);

        // 4. Muitas alocações pequenas, liberando metade e realocando.
        let mut caixas: Vec<Box<[u64; 4]>> = (0..10_000u64).map(|i| Box::new([i; 4])).collect();
        let mut i = 0u64;
        caixas.retain(|_| { i += 1; i % 2 == 0 });           // libera as ímpares
        let com_buracos = heap::estatisticas();
        for j in 0..5_000u64 {
            caixas.push(Box::new([1_000_000 + j; 4]));        // reaproveita os buracos
        }
        assert_eq!(caixas.len(), 10_000);
        assert!(caixas[..5_000].iter().enumerate().all(|(k, c)| c[0] == 2 * k as u64 + 1 && c[3] == c[0]));
        assert!(caixas[5_000..].iter().enumerate().all(|(k, c)| c[0] == 1_000_000 + k as u64));
        let com_caixas = heap::estatisticas();
        drop(caixas);

        // 5. Alinhamentos grandes, pedidos direto ao alocador.
        for &alinhamento in &[64usize, 4096] {
            let layout = Layout::from_size_align(100, alinhamento).unwrap();
            let p = unsafe { alloc::alloc::alloc(layout) };
            assert!(!p.is_null() && p as usize % alinhamento == 0,
                    "alinhamento {} falhou: {:p}", alinhamento, p);
            unsafe { alloc::alloc::dealloc(p, layout) };
        }

        // 6. Tudo devolvido: nada vazou e os buracos se juntaram de novo.
        let depois = heap::estatisticas();
        assert_eq!(depois.em_uso, antes.em_uso, "vazamento no heap");
        assert_eq!(depois.blocos_livres, 1, "os blocos livres deveriam ter se juntado");
        assert_eq!(depois.maior_livre as u64, depois.mapeado - depois.em_uso as u64);
        kprintln!("Teste heap OK: Box, Vec, String, {} alocacoes e {} liberacoes,",
                  depois.alocacoes - antes.alocacoes, depois.liberacoes - antes.liberacoes);
        kprintln!("  pico {} KiB; blocos livres: {} com metade das caixas liberada, {} depois de",
                  depois.pico / 1024, com_buracos.blocos_livres, com_caixas.blocos_livres);
        kprintln!("  realocar 5 000 caixas nos buracos; alinhamento 64 e 4096 ok;");
        kprintln!("  em uso no fim = no inicio ({} bytes), 1 bloco livre de {} KiB;",
                  depois.em_uso, depois.maior_livre / 1024);
        kprintln!("  quadros usados pelo crescimento: {} (o heap nao devolve quadros)",
                  quadros_antes - memoria::quadros::livres());
        let _ = HEAP_INICIO;
    }

    #[cfg(feature = "teste-heap-liberacao-dupla")]
    {
        use core::alloc::Layout;
        let layout = Layout::new::<u64>();
        let p = unsafe { alloc::alloc::alloc(layout) };
        kprintln!("Teste: liberando o bloco {:p} do heap duas vezes...", p);
        unsafe {
            alloc::alloc::dealloc(p, layout);
            alloc::alloc::dealloc(p, layout); // <- deve dar panic
        }
    }

    #[cfg(feature = "teste-heap-esgotar")]
    {
        use alloc::vec;
        use alloc::vec::Vec;
        use memoria::heap::TAMANHO_MAXIMO;
        kprintln!("Teste: alocando 1 MiB por vez ate o limite do heap ({} MiB)...",
                  TAMANHO_MAXIMO / (1024 * 1024));
        let mut blocos: Vec<Vec<u8>> = Vec::new();
        loop {
            blocos.push(vec![0xAB; 1024 * 1024]);
            if blocos.len() % 16 == 0 || blocos.len() >= 60 {
                let e = memoria::heap::estatisticas();
                kprintln!("  {} MiB alocados (heap mapeado: {} KiB)", blocos.len(), e.mapeado / 1024);
            }
        }
    }

    #[cfg(feature = "teste-heap-executar")]
    {
        use alloc::boxed::Box;
        // 0xC3 é a instrução `ret`. Se a página permitisse execução, a chamada
        // abaixo simplesmente voltaria. Com NX, a CPU recusa buscar a instrução.
        let codigo = Box::new([0xC3u8; 16]);
        let endereco = codigo.as_ptr() as u64;
        kprintln!("Teste: executando um `ret` gravado no heap em {:#x}...", endereco);
        let funcao: extern "C" fn() = unsafe { core::mem::transmute(endereco) };
        funcao();
        kprintln!("ERRO: o heap deveria ser proibido de executar (NX)");
    }

    #[cfg(feature = "teste-bump")]
    {
        use alloc::boxed::Box;
        use memoria::bump;
        kprintln!("Teste bump: cria e destroi uma caixa de 1 KiB por vez.");
        kprintln!("  So uma caixa existe a cada momento, mas a memoria nunca volta:");
        for i in 1..=2_000u32 {
            let caixa = Box::new([i as u8; 1024]);
            core::hint::black_box(&caixa);
            drop(caixa);
            if i % 250 == 0 {
                let (usado, aloc, lib) = bump::estado();
                kprintln!("  {:>4} caixas: {:>4} KiB consumidos ({} alocacoes, {} liberacoes ignoradas)",
                          i, usado / 1024, aloc, lib);
            }
        }
    }
}
