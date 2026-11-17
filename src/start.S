# SPDX-License-Identifier: Apache-2.0
#
# AT Field — 被宿主固件调用的入口（**不是**冷启动入口）。
#
# 调用约定（宿主固件侧遵守）：
#   a0 = hartid
#   a1 = 角色：0 = boot hart（primary），1 = 其它 hart（secondary）
#   a2 = 下一阶段 FDT 的物理地址（0 = 固件没给）；本入口把它作为 Rust 的第二个参数
#   ra = 返回地址
#   sp / gp = 调用者的，AT Field 必须原样还回去
#
# 本入口**绝不触碰任何 CSR**：trap 向量、委派配置、mstatus/mie 全部归宿主固件。
# （较早的冷启动版本会清这些 CSR 并接管 mtvec；那个语义等做独立镜像自启动时
#  再单独开一个入口，不能混在这里，否则会破坏固件的 trap 处理。）
#
# 注：本文件由 global_asm!(include_str!(...)) 引入，不经 C 预处理器，
# 因此注释统一用汇编的 `#` 风格（LLVM 的 RISC-V 汇编器不认 `/* */`）。

.attribute arch, "rv64gc"   # 声明 ISA，汇编器才放行 mul 等 M 扩展指令

.equ STACK_PER_HART, 0x4000    # 每 hart 栈 16 KiB；与 linker.ld 的栈区大小由 ASSERT 绑定
.option norvc

.section .text.entry
.global _start
_start:
    # 1. 保存调用者的 ra/sp/gp，以及 s0（本入口用它跨 Rust 调用持住槽位地址）。
    #    槽位表在 .hart_save（**不是** .bss：primary 会清 .bss，而这里存着当前
    #    hart 正在用的返回路径）。写在前、读在后，不依赖它预先清零。
    la   t0, HF_SAVE
    andi t1, a0, 7              # 实验阶段：最多 8 个 hart；>8 会复用槽位（已知限制）
    slli t1, t1, 5              # 32 字节 / 槽
    add  t0, t0, t1
    sd   ra, 0(t0)
    sd   sp, 8(t0)
    sd   gp, 16(t0)
    sd   s0, 24(t0)
    mv   s0, t0                 # s0 是 callee-saved，Rust 代码会替我们保住

    # 2. 切到 AT Field 自己的运行环境
.option push
.option norelax
    la   gp, __global_pointer$
.option pop
    la   sp, _stack_top
    li   t2, STACK_PER_HART
    andi t3, a0, 7
    mul  t3, t3, t2
    sub  sp, sp, t3             # 每 hart 一段栈，避免并发调用互相踩踏

    # 3. 按角色分派到两个 Rust 入口。
    #    Rust 侧签名是 (_hartid, fdt)：角色只在汇编里看，FDT 地址搬到 a1。
    bnez a1, .Lsecondary_entry
.Lprimary_entry:
    mv   a1, a2
    call _primary_main
    j    .Lepilogue
.Lsecondary_entry:
    mv   a1, a2
    call _secondary_main

    # 4. 原样还回调用者环境并返回
.Lepilogue:
    ld   ra, 0(s0)
    ld   sp, 8(s0)
    ld   gp, 16(s0)
    ld   s0, 24(s0)
    ret

# 每 hart 的调用者上下文保存槽：8 hart x 4 word x 8 字节。
#
# 用**独立段**而不是 .bss：primary 阶段会清零 .bss，而这里存着当前 hart 正在用的
# 返回路径（ra/sp/gp/s0）。混进 .bss 的话，清完返回地址就变成 0。
# linker.ld 里的 `.hart_save (NOLOAD)` 吸收本段。
.section .hart_save, "aw", @nobits
.align 3
.global HF_SAVE
HF_SAVE:
    .zero 256
