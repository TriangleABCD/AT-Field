// SPDX-License-Identifier: Apache-2.0

//! Flattened Device Tree（FDT）头的**最小**读取。
//!
//! 这一步只回答"地址对不对、这份树是谁的"：魔数、总长、版本、各块偏移。
//! 真正的解析（`/memory`、`/chosen` 里的机密区、`stdout-path`、`/reserved-memory`）
//! 归 `init.rs`；这里刻意不引入任何树遍历。
//!
//! 拿到的是**下一阶段**那份 FDT（宿主固件打过补丁、准备交给 S 模式的那份），
//! 不是 SPL 交来的原件——只有前者带 `/reserved-memory` 与
//! `/chosen/atfield,confidential-region`（见决策 2026-11-07）。

use core::sync::atomic::{AtomicUsize, Ordering};

/// FDT 魔数，大端 `0xd00dfeed`（devicetree spec v0.4 §5.2）。
pub const MAGIC: u32 = 0xd00d_feed;

/// 头里各字段的字节偏移。
mod offset {
    pub(super) const MAGIC: usize = 0;
    pub(super) const TOTAL_SIZE: usize = 4;
    pub(super) const OFF_STRUCT: usize = 8;
    pub(super) const OFF_STRINGS: usize = 12;
    pub(super) const OFF_RSVMAP: usize = 16;
    pub(super) const VERSION: usize = 20;
    pub(super) const LAST_COMP_VERSION: usize = 24;
}

/// 固件经 `a2` 传进来的 FDT 地址（0 = 没给）。后续模块从这里取。
static ADDRESS: AtomicUsize = AtomicUsize::new(0);

/// 记下入口拿到的地址，返回它自己（便于就地使用）。
pub fn bind(address: usize) -> usize {
    ADDRESS.store(address, Ordering::Relaxed);
    address
}

/// 已绑定的 FDT 地址；0 表示宿主固件没传。
pub fn address() -> usize {
    ADDRESS.load(Ordering::Relaxed)
}

/// FDT 头的只读视图。**不做任何校验**就构造，读之前先看 `magic()`。
#[derive(Clone, Copy)]
pub struct Header {
    base: usize,
}

impl Header {
    /// 在给定物理地址上构造视图。地址是否可读由调用方保证。
    pub const fn at(base: usize) -> Self {
        Self { base }
    }

    /// 一个大端 `u32`。
    ///
    /// 逐字节拼而不是 `*const u32`：FDT 头按 8 字节对齐，但调用方给的地址不一定，
    /// 未对齐的 `u32` 读在 Rust 里是 UB。
    fn be32(&self, offset: usize) -> u32 {
        let ptr = (self.base + offset) as *const u8;
        let mut value = 0u32;
        for index in 0..4 {
            // SAFETY: 调用方保证 [base, base + 28) 可读（FDT 头）；逐字节 volatile。
            value = (value << 8) | unsafe { ptr.add(index).read_volatile() } as u32;
        }
        value
    }

    pub fn magic(&self) -> u32 {
        self.be32(offset::MAGIC)
    }

    pub fn total_size(&self) -> u32 {
        self.be32(offset::TOTAL_SIZE)
    }

    pub fn off_struct(&self) -> u32 {
        self.be32(offset::OFF_STRUCT)
    }

    pub fn off_strings(&self) -> u32 {
        self.be32(offset::OFF_STRINGS)
    }

    pub fn off_rsvmap(&self) -> u32 {
        self.be32(offset::OFF_RSVMAP)
    }

    pub fn version(&self) -> u32 {
        self.be32(offset::VERSION)
    }

    pub fn last_comp_version(&self) -> u32 {
        self.be32(offset::LAST_COMP_VERSION)
    }

    /// 魔数是否正确——唯一值得先查的一件事。
    pub fn is_valid(&self) -> bool {
        self.magic() == MAGIC
    }
}

/// 打印一次握手结果：证明 `a2` 那条 ABI 通了，且拿到的是该拿的那份树。
///
/// v0 第一步只验证"传得过来、内容像 FDT"。解析留给 `init.rs`，那时这里会
/// 变成 `from_fdt(address) -> Result<Config>` 之类的形状。
pub fn report(fdt: usize) {
    bind(fdt);
    if fdt == 0 {
        crate::println!("FDT: 固件没有传地址（a2 = 0）");
        return;
    }
    let header = Header::at(fdt);
    if !header.is_valid() {
        crate::println!(
            "FDT @ {:#x}: 魔数不符 {:#010x}（期望 {:#010x}）",
            fdt,
            header.magic(),
            MAGIC
        );
        return;
    }
    crate::println!(
        "FDT @ {:#x}: totalsize {} B, version {} (last_comp {})",
        fdt,
        header.total_size(),
        header.version(),
        header.last_comp_version()
    );
    crate::println!(
        "FDT: off_struct {} off_strings {} off_rsvmap {}",
        header.off_struct(),
        header.off_strings(),
        header.off_rsvmap()
    );
}
