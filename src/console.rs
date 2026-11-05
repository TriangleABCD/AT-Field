// SPDX-License-Identifier: Apache-2.0

//! 调试打印的**前端**：格式化 + `print!` / `println!` 宏。
//!
//! 这一层只认识 `Sink`（能输出一个字节的东西），不认识任何寄存器。
//! 换底层驱动（NS16550 / SBI DBCN / semihosting / 别的 UART）时，
//! 本文件不需要改动——只要新驱动 `impl Sink` 并在启动时 `install`。

use core::fmt::{self, Write};

/// 字节输出的最小抽象。所有具体驱动都实现它。
///
/// 只要求"能写一个字节"，缓冲、flush、锁都留给实现自己决定。
pub trait Sink: Sync {
    fn put(&self, byte: u8);
}

/// 未安装驱动时的空实现：静默丢弃，保证 `print!` 永远不会炸。
struct NullSink;

impl Sink for NullSink {
    fn put(&self, _byte: u8) {}
}

static NULL_SINK: NullSink = NullSink;

/// 已安装的后端。用 `Once` 而不是 `static mut`：无 unsafe，且天然只写一次。
static SLOT: spin::Once<&'static dyn Sink> = spin::Once::new();

/// 安装后端驱动。**只有第一次调用生效**，重复调用被忽略。
pub fn install(sink: &'static dyn Sink) {
    SLOT.call_once(|| sink);
}

fn current() -> &'static dyn Sink {
    match SLOT.get() {
        Some(sink) => *sink,
        None => &NULL_SINK,
    }
}

struct Writer {
    sink: &'static dyn Sink,
}

impl Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            // 串口终端需要 CRLF；放在这一层，驱动保持字节忠实。
            if byte == b'\n' {
                self.sink.put(b'\r');
            }
            self.sink.put(byte);
        }
        Ok(())
    }
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments<'_>) {
    let _ = Writer { sink: current() }.write_fmt(args);
}

/// 格式化打印，不换行。用法同 `std::print!`。
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::console::_print(format_args!($($arg)*)));
}

/// 格式化打印并换行。用法同 `std::println!`。
#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}
