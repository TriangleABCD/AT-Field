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
mod uart;

use core::arch::global_asm;
use core::panic::PanicInfo;

// 被调用的入口：保存调用者上下文 → 换环境 → 按角色分派 → 恢复 → ret。
// 每 hart 的栈大小由 start.S 的 `STACK_PER_HART` 决定，linker.ld 里的 ASSERT
// 保证栈区正好切成 8 份。
global_asm!(include_str!("start.S"));

/// 汇编 → Rust 的接缝（boot hart）。
///
/// 调用约定（见 `src/start.S`）：`a0` = hartid，`a1` = 0。
/// 出错时直接停机，不把错误还给固件——固件没有对应 ABI 来处理我们的失败。
#[unsafe(no_mangle)]
pub extern "C" fn _primary_main(hart_id: usize) {
    console_init_once();
    if primary_main(hart_id).is_err() {
        halt();
    }
}

/// 汇编 → Rust 的接缝（非 boot hart）。
///
/// 调用约定：`a0` = hartid，`a1` = 1。AT Field 自己无法判断谁是 boot hart
/// （RustSBI 在 dynamic 模式下是抢锁选举的，不一定是 hartid 0），所以角色由
/// 调用方通过 `a1` 传入。
#[unsafe(no_mangle)]
pub extern "C" fn _secondary_main(hart_id: usize) {
    console_init_once();
    if secondary_main(hart_id).is_err() {
        halt();
    }
}

/// 全局（一次性）初始化：内存布局、PMP 基线、全局表……
///
/// 目前只有打印。之后这里会做"只允许做一次"的事。
fn primary_main(hart_id: usize) -> Result<(), Error> {
    crate::println!("\x1b[31mHART {}: AT Field init (primary)\x1b[0m", hart_id);
    Ok(())
}

/// 每 hart 初始化：per-hart 状态、per-hart PMP 条目……
///
/// 目前只有打印。
fn secondary_main(hart_id: usize) -> Result<(), Error> {
    crate::println!("\x1b[33mHART {}: AT Field init (secondary)\x1b[0m", hart_id);
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
