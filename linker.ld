/* SPDX-License-Identifier: Apache-2.0 */
/* AT Field — linker script.
 *
 * 装载地址固定在**机密半区的起点**：QEMU `virt` 用 -m 4G 时 RAM 为
 * 0x8000_0000..0x1_8000_0000，取后半段 0x1_0000_0000 起。
 * 这个地址是编译期常量，宿主固件按它跳转（见 src/start.S 的调用约定）。
 *
 * Symbols consumed by src/start.S:
 *   __global_pointer$  gp, 0x800 past the start of .sdata/.data
 *   _bss_start/_bss_end  区域必须 8 字节对齐（main.rs 的 clear_bss_once() 按
 *                        8 字节写零）
 *   _stack_top        初始 sp（栈向下生长）
 *
 * Symbols consumed by src/pmp.rs（PMP 条目的唯一真值源，改容量只改这里）:
 *   _conf_start        机密区起点（= 镜像起点 = 4G RAM 的上半区起点）
 *   _code_region_end   代码区终点（RX）；必须 ≥ _text_end 且 ≥ .rodata 末
 *   _tsm_data_end      TSM 自有数据区终点（RW），其后是 TVM 机密页池
 *   _conf_end          机密区终点（= RAM 顶；本配置要求 -m 4G）
 */

ENTRY(_start)

SECTIONS
{
    /* 机密区（AT Field 的总地盘）。
     *
     * 布局前提：`qemu -m 8G`，RAM = [0x8000_0000, 0x2_8000_0000)。
     *   宿主 RAM  [0x8000_0000, 0x1_8000_0000)  4 GiB
     *   DTB       [0x1_7ff8_0000, 0x1_8000_0000)  512 KiB（紧贴机密区下方）
     *   机密区    [0x1_8000_0000, 0x2_8000_0000)  4 GiB（= RAM 顶）
     *
     * 四个边界是**编译期常量**（要写进 PMP 条目，而 `L=1` 条目一写就锁死），
     * 且**只能在本文档里定义一次**：pmp.rs 从 `.conf_limits` 读，不自己写一份。
     * 改容量只改这里，然后两侧重新构建（QEMU 内存大小必须跟着改）。 */
    PROVIDE(_conf_start = 0x180000000);                   /* = 镜像起点 */
    PROVIDE(_code_region_end = _conf_start + 0x4000);     /* 16 KiB：代码区（RX） */
    PROVIDE(_tsm_data_end = _conf_start + 0x4000000);     /* 64 MiB：数据/栈/堆（RW） */
    PROVIDE(_conf_end = 0x280000000);                     /* RAM 顶（-m 8G） */

    . = _conf_start;

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

    /* 把四个边界当**数据**放在镜像里：pmp.rs 直接用符号**地址**取不到它们
     * （边界离镜像 4 GiB，超出 PC 相对重定位的 ±512 KiB）。这个段必须落在
     * 镜像内部、且在 RX 区（只读），所以放在 .rodata 后面。 */
    .conf_limits : {
        . = ALIGN(8);
        PROVIDE(_conf_limits = ABSOLUTE(.));
        QUAD(_conf_start)          /* 必须是 QUAD：LONG 只有 4 字节，且会截断 >4 GiB 的地址 */
        QUAD(_code_region_end)
        QUAD(_tsm_data_end)
        QUAD(_conf_end)
        . = ALIGN(8);
    }

    /* 把可写段推到代码区之外：PMP 的代码区是 RX（不可写），而 .rodata 还在里面；
     * 从 .data 开始必须越过 `_code_region_end`，否则写 .data 会被自己的 PMP 拦住。 */
    . = ALIGN(0x4000);

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

    /* 每 hart 的调用者上下文保存槽（内容由 start.S 的 `HF_SAVE` 定义）。
     *
     * **单独成段且 NOLOAD，不能放进 .bss**：primary 阶段要清零 .bss，而这里
     * 存着**当前 hart 正在用的** ra/sp/gp/s0——混进 .bss 的话，一清就把自己的
     * 返回路径抹掉，`ret` 会跳到 0。 */
    .hart_save (NOLOAD) : {
        . = ALIGN(8);
        *(.hart_save)
        . = ALIGN(8);
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

    /* 编译期护栏：入口必须是机密区的第一个字节，宿主固件才敢按常量跳。 */
    ASSERT(_start == _conf_start, "AT Field entry must sit at the confidential region base")

    /* 代码区必须装得下 .text + .rodata + .conf_limits（RX 条目盖这一段）。 */
    ASSERT(_conf_limits + 32 <= _code_region_end,
           "AT Field .conf_limits escaped the RX region (_code_region_end)")
    ASSERT(. <= _conf_end, "AT Field image escaped the confidential region")

    /* 镜像（含栈）不能超出 TSM 自有数据区。 */
    ASSERT(_stack_top <= _tsm_data_end,
           "AT Field stack escaped the TSM data region (_tsm_data_end)")

    /* 分区顺序必须严格递增，TOR 链才成立。 */
    ASSERT(_conf_start < _code_region_end, "code region must be non-empty")
    ASSERT(_code_region_end < _tsm_data_end, "TSM data region must be non-empty")
    ASSERT(_tsm_data_end < _conf_end, "TVM pool must be non-empty")

    /* 栈区必须正好是 start.S 里 STACK_PER_HART(0x4000) 的 8 倍。 */
    ASSERT(_stack_top - _stack_bottom == 8 * 0x4000,
           "stack region must be 8 * STACK_PER_HART (0x20000)")
}
