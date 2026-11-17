/* SPDX-License-Identifier: Apache-2.0 */
/* Longinus — S 态测试程序的链接脚本。
 *
 * 链在 0x8020_0000：这是 QEMU/OpenSBI 一系的 kernel 落点，落在我们的宿主 RAM
 * （0x8000_0000..0x1_8000_0000）里，且不与固件（0x8000_0000 起）重叠。
 *
 * 与 TSM 的镜像不同：这里**没有**"必须在机密区"的约束——测试程序是宿主的一部分，
 * 故意放在 S 模式能访问的地方。
 *
 * Symbols consumed by src/start.S:
 *   __global_pointer$   gp
 *   _bss_start/_bss_end 必须 8 字节对齐（用 sd 清零）
 *   _stack_end          初始 sp（栈向下生长）
 */

OUTPUT_ARCH("riscv")
ENTRY(_start)

SECTIONS
{
    . = 0x80200000;

    .text : {
        *(.text.start)
        *(.text .text.*)
        . = ALIGN(8);
        PROVIDE(_text_end = .);
    }

    .rodata : {
        *(.rodata .rodata.*)
        . = ALIGN(8);
    }

    .data : {
        . = ALIGN(4096);
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

    /* 栈：只是地址，不进镜像文件（loader 只装 .text/.rodata/.data 的内容）。 */
    .stack (NOLOAD) : {
        . = ALIGN(16);
        PROVIDE(_stack_start = .);
        . += 0x10000;
        PROVIDE(_stack_end = .);
    }

    /* 入口必须是 0x8020_0000 的第一个字节——固件按常量跳过来。 */
    ASSERT(_start == 0x80200000, "longinus entry must sit at 0x8020_0000")
}
