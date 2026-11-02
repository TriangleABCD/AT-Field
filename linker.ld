/* SPDX-License-Identifier: Apache-2.0 */
/* AT Field — linker script (placeholder).
 *
 * 0x8000_0000 is the QEMU `virt` RAM base; the real load address will be
 * fixed once the integration with the host firmware is decided.
 *
 * Symbols consumed by src/start.S:
 *   __global_pointer$  gp, 0x800 past the start of .sdata/.data
 *   _bss_start/_bss_end  区域必须 8 字节对齐（用 sd 清零）
 *   _stack_top        初始 sp（栈向下生长）
 */

ENTRY(_start)

SECTIONS
{
    . = 0x80000000;

    .text : {
        *(.text.entry)
        *(.text .text.*)
        . = ALIGN(8);
        PROVIDE(_text_end = .);
    }

    .rodata : {
        *(.rodata .rodata.*)
        . = ALIGN(8);
    }

    .data : {
        *(.sdata .sdata.*)
        *(.data .data.*)
        . = ALIGN(8);
        PROVIDE(__global_pointer$ = . + 0x800);
    }

    .bss : {
        . = ALIGN(8);
        PROVIDE(_bss_start = .);
        *(.sbss .sbss.*)
        *(.bss .bss.*)
        *(COMMON)
        . = ALIGN(8);
        PROVIDE(_bss_end = .);
    }

    /* 占位栈：64 KiB。NOLOAD 表示不占文件内容。
     * 后续换成从机密内存分配的每 hart 栈（见 PMP 条目分层方案）。 */
    .stack (NOLOAD) : {
        . = ALIGN(16);
        PROVIDE(_stack_bottom = .);
        . += 0x10000;
        PROVIDE(_stack_top = .);
    }
}
