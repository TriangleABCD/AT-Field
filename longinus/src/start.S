# SPDX-License-Identifier: Apache-2.0
#
# Longinus — S 态入口。
#
# 由固件 `mret` 进来：a0 = hartid，a1 = opaque（我们这一版传的是下一阶段 FDT 地址，
# 见固件的 `boot::enter_stage`）。**不假设 M 态替我们准备好任何东西**——gp、sp、
# .bss、sstatus/sie 都在这里自己弄，这一点照抄 Salus 的 TELLUS。
#
# 注：本文件由 global_asm!(include_str!(...)) 引入，不经 C 预处理器，注释用 `#`。

.attribute arch, "rv64gc"
.option norvc

.section .text.start
.global _start
_start:
    # 1. gp：Rust 靠它取 .sdata/.data，必须在任何 Rust 代码之前设好。
.option push
.option norelax
    la   gp, __global_pointer$
.option pop

    # 2. 清 S 态自己的 CSR：M 态可能把上一次运行的值留着。
    csrw sstatus, zero
    csrw sie, zero

    # 3. 清 .bss（链接脚本保证 _bss_start/_bss_end 8 字节对齐）。
    la   t0, _bss_start
    la   t1, _bss_end
1:
    sd   zero, 0(t0)
    addi t0, t0, 8
    bltu t0, t1, 1b

    # 4. 栈：从链接脚本给的顶端向下生长。
    la   sp, _stack_end

    # 5. 进 Rust。a0/a1 原样传下去（hartid / FDT 地址）。
    call rust_main

    # rust_main 是 `-> !`，正常不会回到这里；万一回来了就停下，别跑野。
2:
    wfi
    j    2b
