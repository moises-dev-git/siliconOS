/* Script de ligação dos programas de usuário (docs/13-disco-em-ram-e-elf.md).
 * Cada seção começa numa página nova, para o kernel poder dar permissões
 * diferentes a código (R-X), constantes (R--) e dados (RW-). */
ENTRY(_start)

SECTIONS
{
    . = 0x200000000000;       /* entrada 64 da PML4: área dos programas (doc 11) */

    /* Com code-model=large, o compilador põe o nosso código em .ltext,
     * .lrodata, .ldata e .lbss ("l" de large); o core da biblioteca padrão
     * vem com os nomes de sempre. A .got (tabela de endereços que o código
     * do core consulta) é preenchida na ligação, mas o ligador a marca como
     * gravável: se fosse junto com as constantes, elas virariam graváveis
     * também. Vai com os dados. */
    .text   : ALIGN(4K) { *(.text .text.* .ltext .ltext.*) }
    .rodata : ALIGN(4K) { *(.rodata .rodata.* .lrodata .lrodata.*) }
    .data   : ALIGN(4K) { *(.data .data.* .ldata .ldata.*) *(.got .got.*) }
    .bss    : ALIGN(4K) { *(.bss .bss.* .lbss .lbss.*) *(COMMON) }

    /DISCARD/ : { *(.eh_frame*) *(.comment) }
}
