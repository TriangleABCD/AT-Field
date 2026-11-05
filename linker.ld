/* SPDX-License-Identifier: Apache-2.0 */
/* AT Field — linker script.
 *
 * 装载地址固定在**机密半区的起点**：QEMU `virt` 用 -m 4G 时 RAM 为
 * 0x8000_0000..0x1_8000_0000，取后半段 0x1_0000_0000 起。
 * 这个地址是编译期常量，宿主固件按它跳转（见 src/start.S 的调用约定）。
 *
 * Symbols consumed by src/start.S:
 *   __global_pointer$  gp, 0x800 past the start of .sdata/.data
 *   _bss_start/_bss_end  区域必须 8 字节对齐（用 sd 清零）
 *   _stack_top        初始 sp（栈向下生长）
 */

ENTRY(_start)

SECTIONS
{
    . = 0x100000000;

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

    /* 每 hart 的栈区：128 KiB，按 16 KiB 切给 8 个 hart（见 start.S 的
     * `STACK_PER_HART`）。NOLOAD：不占文件内容。
     * 后续换成从机密内存分配的每 hart 栈（见 PMP 条目分层方案）。 */
    .stack (NOLOAD) : {
        . = ALIGN(16);
        PROVIDE(_stack_bottom = .);
        . += 0x20000;
        PROVIDE(_stack_top = .);
    }

    /* 编译期护栏：入口必须是机密半区的第一个字节，宿主固件才敢按常量跳。 */
    ASSERT(_start == 0x100000000, "AT Field entry must sit at 0x1_0000_0000")
}
