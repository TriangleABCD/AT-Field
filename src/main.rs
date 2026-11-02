// SPDX-License-Identifier: Apache-2.0

//! AT Field — Attested Trust Field（可证信任场）。
//!
//! 一个运行于 RISC-V M 模式的 TSM：以 PMP 划出机密域，使宿主无法触及域内内存，
//! 并让该域的静态配置可被外部验证。
//!
//! 目前只有骨架：汇编入口 → ABI 接缝 → 空的初始化。

#![no_std]
#![no_main]

use core::arch::global_asm;
use core::panic::PanicInfo;

// M 模式入口（gp / mstatus / mie / medeleg / mideleg / BSS / 栈），最后 call _primary_main
global_asm!(include_str!("start.S"));

/// 汇编 → Rust 的唯一接缝。
///
/// 只做 ABI 转换：取参、调用内部实现、把 Result 收敛成不返回的出口。
/// 初始化逻辑一律写在 [`primary_main`] 里，不进这里。
///
/// 调用约定：只能由 `start.S` 的 `_start` 调用，此时 `a0` = hartid、`a1` = FDT 地址
/// （RISC-V 启动约定）。函数体只做安全操作，因此不标 `unsafe`——与 salus 的
/// `_primary_main` 一致。
#[unsafe(no_mangle)]
pub extern "C" fn _primary_main(hart_id: u64, fdt_addr: u64) -> ! {
    match primary_main(hart_id, fdt_addr) {
        Ok(()) => halt(),
        // 错误处理留在这里而不是汇编里：先停机，后续换成打印/取证。
        Err(_error) => halt(),
    }
}

/// 真正的初始化入口。
///
/// 目前是空的。下一步依次是：解析 FDT、接管 `mtvec`、建立机密内存布局。
fn primary_main(_hart_id: u64, _fdt_addr: u64) -> Result<(), Error> {
    Ok(())
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
