// SPDX-License-Identifier: Apache-2.0

//! AT Field — Attested Trust Field（可证信任场）。
//!
//! 一个运行于 RISC-V M 模式的 TSM：以 PMP 划出机密域，使宿主无法触及域内内存，
//! 并让该域的静态配置可被外部验证。
//!
//! 启动模型：**由宿主固件（RustSBI）在自己的初始化流程中逐 hart 调用**，
//! 不是冷启动。入口见 `src/start.S`：它保存调用者的 `ra`/`sp`/`gp`/`s0`，
//! 切到 AT Field 自己的环境，按 `a1` 的角色分派到 [`_primary_main`] 或
//! [`_secondary_main`]，然后原样返回。

#![no_std]
#![no_main]

mod console;
mod fdt;
mod pmp;
mod trap;
mod uart;

use core::arch::global_asm;
use core::panic::PanicInfo;
use core::sync::atomic::{AtomicU8, Ordering};

// 被调用的入口：保存调用者上下文 → 换环境 → 按角色分派 → 恢复 → ret。
// 每 hart 的栈大小由 start.S 的 `STACK_PER_HART` 决定，linker.ld 里的 ASSERT
// 保证栈区正好切成 8 份。
global_asm!(include_str!("start.S"));
// M 模式 trap 向量（`mtvec` 的 direct 模式入口）。现在是观测层：打印后停机。
global_asm!(include_str!("trap.S"));

/// 汇编 → Rust 的接缝（boot hart）。
///
/// 调用约定（见 `src/start.S`）：`a0` = hartid，`a1` = 0（角色），`a2` = 下一阶段
/// FDT 物理地址；汇编把角色消耗掉，`a2` 搬到 `a1` 再进来，所以这里是
/// `(hart_id, fdt)`。
/// 出错时直接停机，不把错误还给固件——固件没有对应 ABI 来处理我们的失败。
#[unsafe(no_mangle)]
pub extern "C" fn _primary_main(hart_id: usize, fdt: usize) {
    // **必须最先**：下面的 console 后端、锁、UART 基地址全在 `.bss` 里。
    clear_bss_once();
    console_init_once();
    if primary_main(hart_id, fdt).is_err() {
        halt();
    }
}

/// 汇编 → Rust 的接缝（非 boot hart）。
///
/// 调用约定：`a0` = hartid，`a1` = 1，`a2` = FDT 地址。AT Field 自己无法判断谁是
/// boot hart（RustSBI 在 dynamic 模式下是抢锁选举的，不一定是 hartid 0），所以
/// 角色由调用方通过 `a1` 传入。
///
/// 这里**不**清 `.bss`：固件的顺序契约保证 secondary 进来时 primary 已完成，
/// 清空只会破坏正在使用的锁与它们。
#[unsafe(no_mangle)]
pub extern "C" fn _secondary_main(hart_id: usize, fdt: usize) {
    console_init_once();
    if secondary_main(hart_id, fdt).is_err() {
        halt();
    }
}

/// 全局（一次性）初始化：内存布局、PMP、全局表……
///
/// 目前：打印 + FDT 头握手 → 读旧 PMP 配置 → 装新链 → 对账 → 写保护自检。
fn primary_main(hart_id: usize, fdt: usize) -> Result<(), Error> {
    crate::println!("\x1b[31mHART {}: AT Field init (primary)\x1b[0m", hart_id);
    fdt::report(fdt);

    // 先装临时向量，再动任何 CSR/条目：万一 `mseccfg` 不存在或配置写坏，
    // 能打印出原因而不是整机卡死。
    let saved_mtvec = trap::install();

    crate::println!("--- PMP: 装入前的配置（固件铺的） ---");
    pmp::report();

    pmp::install();

    crate::println!("--- PMP: 装入后的配置 ---");
    pmp::report();

    if PMP_SELF_TEST {
        // 向代码区（`L=1 RX`）写一字节：期望 store/AMO access fault。
        trap::probe_write_to_code(pmp::confidential_start());
    }
    trap::restore(saved_mtvec);

    Ok(())
}

/// 装完 PMP 后是否跑一次“故意违例”自检。
///
/// 开：验证 `L=1` 条目对 **M 模式** 也生效（这才是“TSM 自己也改不回来”的证据），
/// 代价是**不再返回固件**——所以这一版看不到 `AT Field returned control`。
/// 先关掉：开着会在自检 trap 处停机，固件就永远走不到 S 态（longinus）。
/// 需要重新验“写代码区必 fault”时再打开。
const PMP_SELF_TEST: bool = false;

/// 每 hart 初始化：per-hart 状态、per-hart PMP 条目……
///
/// 目前只有打印。FDT 地址每 hart 都传一遍，是为了验证固件确实对**所有** hart
/// 给了同一个值（不对的话后面的 per-hart 解析会各读各的）。
fn secondary_main(hart_id: usize, fdt: usize) -> Result<(), Error> {
    crate::println!(
        "\x1b[33mHART {}: AT Field init (secondary), fdt {:#x}\x1b[0m",
        hart_id,
        fdt
    );
    Ok(())
}

/// 幂等地把控制台接上。
///
/// primary 与 secondary 都会调：secondary hart 可能**先于** boot hart 进入
/// AT Field（它们只等固件的 READY 标志），若只有 primary 安装后端，
/// 这些 hart 的输出会落进 `NullSink` 被静默丢弃。`spin::Once` 保证只装一次。
fn console_init_once() {
    uart::UART0.bind(uart::QEMU_VIRT_UART0);
    uart::UART0.init();
    console::install(&uart::UART0);
}

/// `.bss` 的清理状态。
///
/// **初值故意非零**：Rust 把全零初值的 `static` 放进 `.bss`，而 `.bss` 正是要清的
/// 对象——标志自己若在里面，清零会把它一并抹掉，防重入就失效了。非零初值 →
/// 落在 `.data`（由 loader 从镜像装入），清 `.bss` 时不受影响。
static BSS_STATE: AtomicU8 = AtomicU8::new(BSS_PENDING);

const BSS_PENDING: u8 = 1;
const BSS_CLEARING: u8 = 2;
const BSS_CLEARED: u8 = 3;

unsafe extern "C" {
    /// 由 `linker.ld` 提供：`.bss` 的首字节。
    static _bss_start: u8;
    /// 由 `linker.ld` 提供：`.bss` 的末尾（不含）。
    static _bss_end: u8;
}

/// 清零 `.bss`，**只做一次**。
///
/// 为什么必须最先做：`console` 的 `spin::Once`、`PRINT_LOCK`、`UART0` 的基地址
/// 全在 `.bss` 里。QEMU 上电会把 RAM 清零，所以到现在"看起来没问题"纯属运气；
/// 真机复位、或以后由别的组件复用这段 RAM 时不保证。
///
/// 放在 Rust 而不是 `start.S`，是为了用原子 CAS 做防重入：固件若把某个 hart 误标
/// 成 primary 再进来一次，第二次会静默跳过，不会把别的 hart 正在用的锁一并抹掉。
fn clear_bss_once() {
    if BSS_STATE
        .compare_exchange(
            BSS_PENDING,
            BSS_CLEARING,
            Ordering::AcqRel,
            Ordering::Acquire,
        )
        .is_err()
    {
        return;
    }
    let start = core::ptr::addr_of!(_bss_start) as usize;
    let end = core::ptr::addr_of!(_bss_end) as usize;
    // linker.ld 把两个边界都 ALIGN(8)。不成对齐说明链接脚本被改坏了，别再往下走
    // （按 8 字节写零会写越界）。
    if !start.is_multiple_of(8) || !end.is_multiple_of(8) || end < start {
        halt();
    }
    let mut cursor = start;
    while cursor < end {
        // SAFETY: [start, end) 是链接脚本声明的 .bss，归本镜像独占；此刻除本 hart
        // 外无人使用（固件的顺序契约保证 primary 早于任何 secondary），写入不违反
        // 别名规则，且按 8 字节对齐。
        unsafe { (cursor as *mut usize).write_volatile(0) };
        cursor += 8;
    }
    BSS_STATE.store(BSS_CLEARED, Ordering::Release);
}

fn halt() -> ! {
    loop {
        // SAFETY: 纯空闲等待，无内存副作用。
        unsafe { core::arch::asm!("wfi", options(nomem, nostack)) }
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    halt()
}

/// 占位的错误类型。第一个真实错误出现时删掉 `allow`。
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub enum Error {
    /// 尚未实现的路径。
    Unimplemented,
}
