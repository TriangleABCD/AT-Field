// SPDX-License-Identifier: Apache-2.0

//! NS16550A 串口驱动：`Sink` 的一个具体实现。
//!
//! **不认识 `core::fmt`**，只负责"把一个字节塞进 THR"。格式化在 `console.rs`。
//! 基地址不写死在驱动里：启动时 `bind()` 进来（现阶段用板级默认值，
//! 之后由 FDT 的 `stdout-path` / `chosen` 节点提供）。

use core::sync::atomic::{AtomicUsize, Ordering};

use crate::console::Sink;

/// QEMU `virt` 的 UART0 基地址。
///
/// 这是**板级默认值**，不是驱动的常量：上真板时应由 FDT 提供，
/// 届时删掉这个常量即可。
pub const QEMU_VIRT_UART0: usize = 0x1000_0000;

/// 发送保持寄存器（写它即发送）
const REG_THR: usize = 0x00;
/// 中断使能寄存器（这里只用来关中断）
const REG_IER: usize = 0x01;
/// 线路控制寄存器：8N1 + DLAB=0
const REG_LCR: usize = 0x03;
/// 线路状态寄存器
const REG_LSR: usize = 0x05;

/// LSR bit5：发送保持寄存器空
const LSR_THRE: u8 = 1 << 5;
/// 8 bits，无校验，1 停止位，DLAB=0
const LCR_8N1: u8 = 0x03;

/// 一个 NS16550A 实例。
///
/// 用 `AtomicUsize` 存基地址，好处是 `static` 可以直接构造（`const fn`），
/// 且满足 `Sink: Sync`，不需要内部 `unsafe`。
pub struct Ns16550 {
    base: AtomicUsize,
}

impl Ns16550 {
    /// 构造一个未绑定地址的实例。
    pub const fn new() -> Self {
        Self {
            base: AtomicUsize::new(0),
        }
    }

    /// 绑定 MMIO 基地址。启动时调用一次。
    pub fn bind(&self, base: usize) {
        self.base.store(base, Ordering::Relaxed);
    }

    fn base(&self) -> usize {
        self.base.load(Ordering::Relaxed)
    }

    fn read_reg(&self, offset: usize) -> u8 {
        // SAFETY: 基地址由 `bind` 提供，必须指向一块可读的 MMIO 区；
        // 一字节 volatile 访问。
        unsafe { ((self.base() + offset) as *const u8).read_volatile() }
    }

    fn write_reg(&self, offset: usize, value: u8) {
        // SAFETY: 同上，必须指向可写的 MMIO 区；一字节 volatile 访问。
        unsafe { ((self.base() + offset) as *mut u8).write_volatile(value) }
    }

    /// 初始化线路格式。
    ///
    /// QEMU 的 NS16550A 不需要设置波特率分频（输出直接接 chardev），
    /// 这里只关中断、设 8N1。真机上还需补 divisor latch 的配置。
    pub fn init(&self) {
        self.write_reg(REG_IER, 0x00);
        self.write_reg(REG_LCR, LCR_8N1);
    }

    /// 阻塞写一个字节，直到发送保持寄存器为空。
    fn put_blocking(&self, byte: u8) {
        while self.read_reg(REG_LSR) & LSR_THRE == 0 {
            core::hint::spin_loop();
        }
        self.write_reg(REG_THR, byte);
    }
}

impl Sink for Ns16550 {
    fn put(&self, byte: u8) {
        self.put_blocking(byte);
    }
}

/// 板上的第一个串口。多个 UART 时按需再加实例。
pub static UART0: Ns16550 = Ns16550::new();
