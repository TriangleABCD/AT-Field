# SPDX-License-Identifier: Apache-2.0
#
# AT Field — M 模式 trap 向量（mtvec，direct 模式）。
#
# 现在只是"观测层"：把三个 CSR 交给 Rust 侧打印然后停机，不保存寄存器、不返回。
# 真正的 trap 接管（白名单分派 + 把不认识的 trap 反向调用给固件）见决策
# `2026-11-05-trap-strategy.md`，那时这里会变成完整的存取 + 分派入口。
#
# 为什么用 .S 而不是 `#[unsafe(naked)]` 函数：向量必须是 mtvec direct 模式要求
# 的对齐入口，且进来时栈是"出事的那个上下文"的栈；写在汇编里，寄存器约定和
# 对齐都一目了然，不需要靠属性去约束编译器。
#
# 注：本文件由 global_asm!(include_str!(...)) 引入，不经 C 预处理器，注释用 `#`。

.section .text
.align 4                       # mtvec.MODE=0（direct）要求 BASE 4 字节对齐
.global atfield_trap_vector
atfield_trap_vector:
    csrr a0, mcause            # 异常/中断号
    csrr a1, mepc              # 出事的那条指令
    csrr a2, mtval             # 附属信息（出错地址 / 非法指令字）
    j    atfield_trap_report   # Rust 侧：打印后停机，不返回
