// SPDX-License-Identifier: Apache-2.0

//! AT Field — Attested Trust Field（可证信任场）。
//!
//! 一个运行于 RISC-V M 模式的 TSM：以 PMP 划出机密域，使宿主无法触及域内内存，
//! 并让该域的静态配置可被外部验证。
//!
//! 目前的启动模型：**由宿主固件（RustSBI）在自己的初始化流程中调用**，
//! 而不是自己冷启动。入口见 `src/start.S`：它保存调用者的 `ra`/`sp`/`gp`，
//! 切到 AT Field 自己的环境，调用 [`atf_hart_init`]，然后原样返回。

#![no_std]
#![no_main]

mod console;
mod uart;

use core::arch::global_asm;
use core::panic::PanicInfo;

/// 每个 hart 分到的栈大小。`.stack` 区共 128 KiB，按 8 个 hart 切分。
/// 这个常量同时传给 `start.S`（汇编里用它计算 `sp`）。
const STACK_PER_HART: usize = 0x4000; // 16 KiB

// 被调用的入口：保存调用者上下文 → 换环境 → call atf_hart_init → 恢复 → ret
global_asm!(include_str!("start.S"), STACK_PER_HART = const STACK_PER_HART);

/// 由宿主固件调用的每 hart 初始化入口。
///
/// 调用约定（见 `src/start.S`）：`a0` = hartid，返回类型为 `()`，
/// 返回后控制权交还宿主固件。
///
/// 注意：这里**不触碰任何 CSR**。trap 向量、委派配置、`mstatus`/`mie`
/// 全部归宿主固件所有；接管 `mtvec` 是后续 v0 的工作。
#[unsafe(no_mangle)]
pub extern "C" fn atf_hart_init(hart_id: usize) {
    // 控制台：先绑板级默认地址，等 FDT 解析就绪后改为从设备树读取。
    uart::UART0.bind(uart::QEMU_VIRT_UART0);
    uart::UART0.init();
    console::install(&uart::UART0);

    // TODO(bootstrap): `.bss` 目前依赖"上电即为 0"（QEMU 的行为）。
    // 上真板或 RustSBI 开始使用机密半区后，需要改成"由 boot hart 清一次"，
    // 并用一个放在 `.data`（不随 BSS 清零）的原子标志防止重复清理。

    crate::println!("\x1b[31mHART {}: AT Field init\x1b[0m", hart_id);
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
