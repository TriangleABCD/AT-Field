// SPDX-License-Identifier: Apache-2.0

//! Longinus — **S 态测试程序**（名字取自 EVA 的朗基努斯之枪：用来戳穿 AT Field 的枪）。
//!
//! 它跑在宿主侧（S 模式），固件把自己跳转到 `0x8020_0000` 就进这里。目的不是当内核，
//! 而是**从 S 模式去验 TSM 的承诺**：机密区读不到、SBI 还能用、失败路径干净停机。
//!
//! 现有内容只有骨架：打印、读两个 S 态 CSR。第一批真检查（宿主内存可读写、机密区
//! 不可读）等在 `README.md` 里排了顺序，等这一版跑通再往里加。
//!
//! 设计约束：
//! - **不装 `stvec`**（当前阶段的选择）：违规访问会直接把整机打停，靠 QEMU 停在哪
//!   证明发生了什么。等测试框架成形再补 S 态 trap 处理。
//! - 打印走 UART MMIO，不走 SBI console（理由见 `uart.rs`）。

#![no_std]
#![no_main]

mod uart;

use core::arch::{asm, global_asm};
use core::panic::PanicInfo;

// S 态入口：设 gp / 清 sstatus·sie / 清 .bss / 设 sp，然后 call rust_main。
global_asm!(include_str!("start.S"));

/// S 态入口（由 `start.S` 调用）。
///
/// 调用约定（固件侧 `boot::enter_stage`）：`a0` = hartid，`a1` = opaque，
/// 我们这一版让它等于下一阶段 FDT 的物理地址。
#[unsafe(no_mangle)]
extern "C" fn rust_main(hart_id: usize, fdt: usize) -> ! {
    crate::println!("[longinus] S 态已接管：hart {}，FDT @ {:#x}", hart_id, fdt);

    // 能读到这两个 CSR 本身就说明：S 模式在我们手上（它们都是 S 级 CSR）。
    crate::println!(
        "[longinus] sstatus = {:#x}，satp = {:#x}",
        read_sstatus(),
        read_satp()
    );

    crate::println!("[longinus] 骨架就绪（下一步：宿主内存可读写 / 机密区不可读）");
    halt()
}

fn read_sstatus() -> usize {
    let value: usize;
    // SAFETY: `sstatus` 是 S 模式可读的 CSR，只读不写。
    unsafe { asm!("csrr {}, sstatus", out(reg) value, options(nomem, nostack)) };
    value
}

fn read_satp() -> usize {
    let value: usize;
    // SAFETY: `satp` 是 S 模式可读的 CSR，只读不写。
    unsafe { asm!("csrr {}, satp", out(reg) value, options(nomem, nostack)) };
    value
}

fn halt() -> ! {
    loop {
        // SAFETY: 纯空闲等待，无内存副作用。
        unsafe { asm!("wfi", options(nomem, nostack)) }
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    crate::println!("[longinus] panic: {:?}", info);
    halt()
}
