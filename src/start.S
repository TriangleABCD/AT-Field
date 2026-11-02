/* SPDX-License-Identifier: Apache-2.0 */

/* AT Field — M-mode entry.
 *
 * 相对 salus 的 HS 模式版，这里没有 satp / sfence.vma：
 * M 模式永远使用物理地址，地址翻译只出现在给 TVM 用的 hgatp 上。
 * 详见 30-projects/riscv-cove-tee/upstream/salus/src/start.S 的对照。 */

.option norvc

.section .text.entry
.global _start
_start:
.option push
.option norelax
    /* gp 必须在任何可能被松弛成 gp 相对寻址的指令之前就位 */
    la   gp, __global_pointer$
.option pop

    /* 清 M 模式状态与中断。
     * DM3 取 "uninterruptible TSM"：trap 一律留在 M 模式，因此 medeleg/mideleg 归零；
     * 真正的委派与转发策略等有 S 模式负载之后再定。 */
    csrw mstatus, zero
    csrw mie, zero
    csrw medeleg, zero
    csrw mideleg, zero

    /* 临时 trap 向量：在接管 mtvec 之前若有异常，停在这里而不是跳到垃圾地址 */
    la   t0, halt
    csrw mtvec, t0

    /* 清零 .bss（.data 随镜像加载，无需搬运） */
    la   a3, _bss_start
    la   a4, _bss_end
1:
    bgeu a3, a4, 2f
    sd   zero, (a3)
    addi a3, a3, 8
    j    1b
2:

    /* 栈：linker.ld 里预留的 .stack 段顶部；后续换成从机密内存分配的栈 */
    la   sp, _stack_top

    /* a0 = hartid, a1 = FDT 地址（沿用 RISC-V 启动约定），直接透传给 Rust 侧 */
    call _primary_main

    /* _primary_main 返回类型是 !，不应走到这里 */
/* mtvec 的 base 需 4 字节对齐（.option norvc 下所有指令 4 字节，这里显式对齐） */
.align 2
halt:
    wfi
    j    halt
