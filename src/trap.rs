// SPDX-License-Identifier: Apache-2.0

//! 最小 M 模式 trap 观测层：把 `mtvec` 临时换成"打印后停机"的向量。
//!
//! **这不是** v0 第 7 步的 trap 接管（白名单分派 + 把不认识的 trap 反向调用给
//! 固件，见决策 `2026-11-05-trap-strategy.md`）。这里只解决一个前置问题：
//! 配 PMP 的时候如果程序配错，现象是"整机没反应"——装一层能打印 `mcause` 的
//! 向量，才能分清"PMP 拦住了"和"代码写坏了"。
//!
//! **为什么用完要还原**：`mtvec` 一旦长期归 TSM，固件（RustSBI）自己的 trap
//! 处理就被顶掉了，而我们还没有把陌生 trap 转回去的能力——那样 `rt` 返回后整机
//! 立刻失效。所以现在的用法是"装 → 试 → 还原"，只改变验证期的可观测性，不改变
//! 固件的运行语义。
//!
//! 失败策略：进到这里一律 **停机**（fail-stop）。TSM 的 trap 里没有"恢复继续跑"
//! 这条路径——继续跑意味着在状态未知的情况下接着处理机密数据。

use core::arch::asm;

unsafe extern "C" {
    /// 向量入口，定义在 `src/trap.S`。
    fn atfield_trap_vector();
}

/// 装上报错向量，返回固件原来的 `mtvec`（后面交给 [`restore`]）。
pub fn install() -> usize {
    let saved = read_mtvec();
    // SAFETY: 写 `mtvec` 是 M 模式特权操作；`atfield_trap_vector` 是 16 字节对齐的
    // `.text` 入口，满足 direct 模式对 BASE 的 4 字节对齐要求。
    unsafe { asm!("csrw mtvec, {}", in(reg) vector_address(), options(nostack)) };
    saved
}

/// 还原固件的 `mtvec`。
///
/// **必须在正常返回固件之前调用。** 只在"故意制造的 fault 没有发生"这条路上
/// 会走到这里；真进了向量就停在那儿了。
pub fn restore(saved: usize) {
    // SAFETY: 把固件自己写过的值原样写回。
    unsafe { asm!("csrw mtvec, {}", in(reg) saved, options(nostack)) };
}

fn vector_address() -> usize {
    // 取符号地址不读写内存。
    atfield_trap_vector as *const () as usize
}

fn read_mtvec() -> usize {
    let value: usize;
    // SAFETY: `mtvec` 是 M 模式的只读可写 CSR。
    unsafe { asm!("csrr {}, mtvec", out(reg) value, options(nomem, nostack)) };
    value
}

/// 在已装好的临时向量下执行一条 `ecall`。
///
/// 预期：`mcause = 11`（environment call from M-mode）→ 打印 → 停机。
/// **若返回**，说明 trap 根本没进来，这时把固件的 `mtvec` 还原并报告。
///
/// **为什么不用“读一个非法地址”来验**（2026-11-18 实测）：QEMU 会把未分配的物理
/// 地址挂到 `unassigned_mem_ops` 上，**读返回 0、不报错**。实测 `0xdead_0000` 读到
/// 0 且无 fault，所以那条路在 QEMU 上验不了向量。`ecall` 与内存布局无关，
/// 才是这一步该用的触发器；真正的 load access fault 留给下一步用 **PMP 违例**
/// 制造（那才是 QEMU 一定会计的错误）。
#[allow(dead_code)] // 诊断工具：需要“只用 ecall 验一次向量”时再调用
pub fn probe_self_trap(saved_mtvec: usize) {
    crate::println!(
        "TRAP: 已临时装入向量（固件原 mtvec = {:#x}），执行一条 ecall 期望 mcause = 11",
        saved_mtvec
    );
    // SAFETY: `ecall` 不访问内存，只产生一个同步异常；进来后不会返回。
    unsafe { asm!("ecall", options(nomem, nostack)) };
    restore(saved_mtvec);
    crate::println!("!! 意外：ecall 没有产生 trap（也不该走到这里），mtvec 已还原");
}

/// 向只读的代码区写一字节，验证 `L=1` 条目对 **M 模式** 也生效。
///
/// 预期：`mcause = 7`（store/AMO access fault）→ 打印 → 停机。若返回，说明
/// PMP 没有约束住 M 模式（`L` 位没生效），那就是隔离整个不成立，必须停机告警。
///
/// 这是经典语义下最有价值的一个自检：它同时证明了“条目真写进去了”与
/// “**TSM 自己也改不回代码**”（锁死语义），不依赖任何 S 模式负载。
pub fn probe_write_to_code(address: usize) {
    crate::println!(
        "TRAP: 向代码区 {:#x} 写一字节（该段 PMP 为 L=1 RX），期望 store/AMO access fault",
        address
    );
    // SAFETY: 故意违例。PMP 应当拦下；万一没拦，被改的是自己的一条指令，随后就停机。
    unsafe { (address as *mut u8).write_volatile(0xff) };
    crate::println!("!! 意外：写代码区没有触发 fault（PMP 没对 M 模式生效）");
    crate::halt();
}

/// 打印 trap 现场后停机。由 `trap.S` 的向量以 `a0`/`a1`/`a2` 调用。
///
/// `no_mangle` 是必须的：汇编里按符号名直接 `j` 过来。
#[unsafe(no_mangle)]
extern "C" fn atfield_trap_report(cause: usize, epc: usize, tval: usize) -> ! {
    let interrupt = cause >> (usize::BITS - 1) != 0;
    let code = cause & !(1 << (usize::BITS - 1));
    if interrupt {
        crate::println!(
            "\x1b[31m!! TRAP: interrupt {}, mepc {:#x}\x1b[0m",
            code,
            epc
        );
    } else {
        crate::println!(
            "\x1b[31m!! TRAP: mcause {} = {}, mepc {:#x}, mtval {:#x}\x1b[0m",
            code,
            cause_name(code),
            epc,
            tval
        );
    }
    crate::halt()
}

/// 同步异常的名字。只覆盖会用到的那些；中断走另一条分支。
fn cause_name(code: usize) -> &'static str {
    match code {
        0 => "instruction address misaligned",
        1 => "instruction access fault",
        2 => "illegal instruction",
        3 => "breakpoint",
        4 => "load address misaligned",
        5 => "load access fault",
        6 => "store/AMO address misaligned",
        7 => "store/AMO access fault",
        8 => "environment call from U-mode",
        9 => "environment call from S-mode",
        11 => "environment call from M-mode",
        12 => "instruction page fault",
        13 => "load page fault",
        15 => "store/AMO page fault",
        _ => "unknown synchronous cause",
    }
}
