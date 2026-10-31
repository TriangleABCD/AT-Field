// SPDX-License-Identifier: Apache-2.0

//! AT Field — Attested Trust Field（可证信任场）。
//!
//! 此文件目前只是占位入口：保证仓库能独立构建出 RISC-V 镜像。
//! 真正的入口会在与宿主固件集成后确定——由固件在 M 模式下跳转进来。

#![no_std]
#![no_main]

use core::panic::PanicInfo;

/// 占位入口。
#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    halt()
}

fn halt() -> ! {
    loop {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    halt()
}
