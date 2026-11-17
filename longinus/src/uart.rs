// SPDX-License-Identifier: Apache-2.0

//! 直接写 NS16550 的 THR——**不经过 SBI console**。
//!
//! 两个理由：
//! 1. 少一层依赖：TSM 接管 trap 之后，SBI 这条路可能不再可靠；这条通道自己写
//!    MMIO，只要 PMP 还给宿主 MMIO 权限（1 号条目，`L=0 RWX`）就一直可用。
//! 2. 不需要额外特权：S 模式对 `0x1000_0000` 有 RWX。
//!
//! 代价：和 M 模式共用同一个 THR，且**双方没有跨组件的锁**（现在靠"启动期不并发
//! 打印"的顺序契约规避，见 `docs/architecture.md` 第九节）。

use core::fmt::{self, Write};

/// QEMU `virt` 的 UART0 基地址。
const UART0_BASE: usize = 0x1000_0000;
/// 发送保持寄存器：写它即发送。
const REG_THR: usize = 0x00;
/// 线路状态寄存器。
const REG_LSR: usize = 0x05;
/// LSR bit5：发送保持寄存器空。
const LSR_THRE: u8 = 1 << 5;

struct Uart;

static UART: Uart = Uart;

impl Uart {
    fn read_reg(&self, offset: usize) -> u8 {
        // SAFETY: UART0 是板级已知的 MMIO；PMP 给宿主 MMIO RWX。一字节 volatile。
        unsafe { ((UART0_BASE + offset) as *const u8).read_volatile() }
    }

    fn write_reg(&self, offset: usize, value: u8) {
        // SAFETY: 同上，一字节 volatile 写。
        unsafe { ((UART0_BASE + offset) as *mut u8).write_volatile(value) }
    }

    fn put(&self, byte: u8) {
        // THR 只有一个寄存器，"等空再写"对两个写者不是原子的——单 hart 测试程序
        // 先不管这个；多 hart 时要先想清楚和 M 模式怎么共享。
        while self.read_reg(REG_LSR) & LSR_THRE == 0 {
            core::hint::spin_loop();
        }
        self.write_reg(REG_THR, byte);
    }
}

impl Write for Uart {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            // 终端需要 CRLF；放在这一层，调用方写普通 `\n` 就行。
            if byte == b'\n' {
                UART.put(b'\r');
            }
            UART.put(byte);
        }
        Ok(())
    }
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments<'_>) {
    let _ = Uart.write_fmt(args);
}

/// 格式化打印，不换行。
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::uart::_print(format_args!($($arg)*)));
}

/// 格式化打印并换行。
#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}
