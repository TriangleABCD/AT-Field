// SPDX-License-Identifier: Apache-2.0

//! PMP（Physical Memory Protection）：条目编码、**安装**与对账。
//!
//! ## 语义选择
//!
//! 走**经典语义**（`mseccfg` 一个字段都不写）：`L=1` 对 M 模式也强制、对 S/U
//! 拒绝、并锁死；`L=0` 只约束 S/U，M 模式无视。这样 M 与 S/U 可以共享区间
//! （串口、共享页都靠它），而"宿主对机密区零访问"由 `L=1` 独占地实现。
//! 否决 Smepmp 的理由见 `docs/decisions/2026-11-18-classic-pmp-over-smepmp.md`。
//!
//! ## 分区（常量在 `linker.ld`，这里只读符号）
//!
//! ```text
//! ...宿主 RAM...[ 固件 ][ 宿主 RAM ][ DTB ][ 机密区                      ]
//!                                        ├ 代码区 RX（_code_region_end）
//!                                        ├ TSM 数据 RW（_tsm_data_end）
//!                                        └ TVM 页池（到 _conf_end，v0 不开条目）
//! ```
//!
//! ## 为什么只重建 6 号以后的条目
//!
//! 固件（RustSBI）已经铺好了 0..5：MMIO、固件 `.text`/`.rodata`/`.data`，以及
//! 6 号那条"宿主 RAM 全给 RWX"。我们不需要知道固件各区间的具体地址，只要
//! **把 6 号的上界从 RAM 顶收窄到机密区起点**，再往后接自己的条目即可。
//!
//! 顺序不能乱（`L=1` 一写就锁死，锁住的条目连 `pmpaddr` 都改不了）：
//!
//! ```text
//! 1. 写全部 pmpaddr6..10    ← 此刻还没有 L=1 条目，随便改
//! 2. 写 pmpcfg0             ← 一次让 6/7 生效，其中 7 号变 L=1 并锁死
//! 3. 写 pmpcfg2             ← 8/9/10 号（8 号也是 L=1）
//! ```

use core::arch::asm;

/// 读一个 CSR。编号用字符串字面量，交给汇编器当立即数（助记符要靠汇编器认扩展）。
macro_rules! read_csr {
    ($csr:literal) => {{
        let value: usize;
        // SAFETY: 只读 CSR，无内存副作用；编号由调用方保证是 M 模式可读的 CSR。
        unsafe { asm!(concat!("csrr {}, ", $csr), out(reg) value, options(nomem, nostack)) };
        value
    }};
}

/// 写一个 CSR。
macro_rules! write_csr {
    ($csr:literal, $value:expr) => {{
        let value: usize = $value;
        // SAFETY: M 模式的 CSR 写；编号由调用方保证可写。
        unsafe { asm!(concat!("csrw ", $csr, ", {}"), in(reg) value, options(nostack)) };
    }};
}

/// 机密区边界，由 `linker.ld` 的 `.conf_limits` 段提供。
///
/// **为什么不直接取符号地址**：这四个边界离镜像 4 GiB，`addr_of!(_conf_end)` 会生成
/// PC 相对的 `R_RISCV_PCREL_HI20` 重定位，编译器报 "out of range"（实测）。
/// 链接脚本因此把它们作为 **4 个 `usize` 数据**放进镜像（`.conf_limits`），
/// 本模块按数据读走——单一定义仍在 `linker.ld`，没有第二份常量。
#[derive(Clone, Copy)]
#[repr(C)]
struct Limits {
    conf_start: usize,
    code_region_end: usize,
    tsm_data_end: usize,
    conf_end: usize,
}

unsafe extern "C" {
    /// `linker.ld` 的 `.conf_limits`：四个边界，顺序与 [`Limits`] 一致。
    static _conf_limits: Limits;
}

fn limits() -> Limits {
    // SAFETY: `_conf_limits` 是链接脚本放进镜像的只读数据（4×usize，8 字节对齐），
    // 生命周期与镜像相同；`Copy` 读出来的是一份快照。
    unsafe { core::ptr::read(core::ptr::addr_of!(_conf_limits)) }
}

/// 机密区起点（= 镜像起点，也是 PMP 代码区的下界）。
pub fn confidential_start() -> usize {
    limits().conf_start
}

/// 代码区终点（也是 TSM 数据区的起点）；写保护自检拿它当靶子。
pub fn code_region_end() -> usize {
    limits().code_region_end
}

/// TSM 自有数据区终点；其后是 TVM 机密页池。
pub fn tsm_data_end() -> usize {
    limits().tsm_data_end
}

/// 机密区终点（= RAM 顶，要求 `-m 8G`）。
pub fn confidential_end() -> usize {
    limits().conf_end
}

/// 条目编号分配。
///
/// 0..5 归固件（MMIO、固件三段、以及一条空占位），6 号起归本模块：
/// TOR 链要求地址严格递增，而 `L=1` 条目锁定后不可移动，所以编号必须是常量。
mod slot {
    /// 宿主 RAM 上半段 `[固件末, conf_start)`；固件原来设的是 `[固件末, RAM 顶)`。
    pub const HOST_RAM: usize = 6;
    /// TSM 代码区，`L=1 RX`。
    pub const TSM_CODE: usize = 7;
    /// TSM 数据区（含栈与将来的堆），`L=1 RW`。
    pub const TSM_DATA: usize = 8;
    /// TVM 机密页池，`A=OFF` 占位；将来改成 `L=0 RW` 并在跑 TVM 时开关。
    pub const TVM_POOL: usize = 9;
    /// `conf_end` 以上：不给宿主任何权限。
    pub const ABOVE_RAM: usize = 10;
}

/// 条目地址匹配模式（`pmpcfg.A`，bit4:3）。
#[derive(Clone, Copy, PartialEq, Eq)]
enum Match {
    Off,
    Tor,
    Na4,
    Napot,
}

impl Match {
    fn from_bits(bits: usize) -> Self {
        match bits {
            1 => Self::Tor,
            2 => Self::Na4,
            3 => Self::Napot,
            _ => Self::Off,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Off => "OFF",
            Self::Tor => "TOR",
            Self::Na4 => "NA4",
            Self::Napot => "NAPOT",
        }
    }
}

/// 一个条目的配置字节（`pmpcfgN` 里的 8 位）。
#[derive(Clone, Copy)]
struct Entry {
    index: usize,
    locked: bool,
    match_mode: Match,
    read: bool,
    write: bool,
    execute: bool,
}

impl Entry {
    fn from_byte(index: usize, byte: usize) -> Self {
        Self {
            index,
            locked: byte & 0x80 != 0,
            match_mode: Match::from_bits((byte >> 3) & 0b11),
            read: byte & 0b001 != 0,
            write: byte & 0b010 != 0,
            execute: byte & 0b100 != 0,
        }
    }

    fn permissions(&self) -> &'static str {
        match (self.read, self.write, self.execute) {
            (true, true, true) => "RWX",
            (true, true, false) => "RW",
            (true, false, true) => "RX",
            (false, true, true) => "WX",
            (true, false, false) => "R",
            (false, true, false) => "W",
            (false, false, true) => "X",
            (false, false, false) => "NONE",
        }
    }
}

/// 配置字节：`L<<7 | A<<3 | X<<2 | W<<1 | R`。
const fn config_byte(locked: bool, a: usize, xwr: usize) -> usize {
    ((locked as usize) << 7) | (a << 3) | xwr
}

const TOR: usize = 0b01;
const OFF: usize = 0b00;
const RWX: usize = 0b111;
const RW: usize = 0b011;
const RX: usize = 0b101;
const NONE: usize = 0b000;

/// 打印当前配置，并按 `pmpaddr` 把 TOR 区间还原出来（`[prev<<2, self<<2)`）。
pub fn report() {
    // 注意：这里**不读 `mseccfg`**。QEMU 只在开了 Smepmp 时才实现该 CSR
    // （`target/riscv/csr.c:have_mseccfg`），未开时读一次就是 illegal instruction；
    // 而本设计走经典语义，一个字段都不用它，所以没有读的理由。
    let cfg0 = read_csr!("0x3a0");
    let cfg2 = read_csr!("0x3a2");
    let addresses = [
        read_csr!("0x3b0"), // pmpaddr0
        read_csr!("0x3b1"),
        read_csr!("0x3b2"),
        read_csr!("0x3b3"),
        read_csr!("0x3b4"),
        read_csr!("0x3b5"),
        read_csr!("0x3b6"),
        read_csr!("0x3b7"),
        read_csr!("0x3b8"),
        read_csr!("0x3b9"),
        read_csr!("0x3ba"),
    ];
    for index in 0..16 {
        let entry = entry_at(index, cfg0, cfg2);
        if entry.match_mode == Match::Off {
            continue;
        }
        // TOR 的下界来自**前一条**的 pmpaddr（与那一条是否匹配无关）。
        let lower = if index == 0 {
            0
        } else {
            addresses[index - 1] << 2
        };
        let upper = if index < addresses.len() {
            addresses[index] << 2
        } else {
            usize::MAX
        };
        crate::println!(
            "PMP: entry {:2} [{:#012x}, {:#012x}) {:>5} {:<4} L={}",
            entry.index,
            lower,
            upper,
            entry.match_mode.name(),
            entry.permissions(),
            entry.locked as u8
        );
    }
    crate::println!(
        "PMP: 机密区 [{:#x}, {:#x})：代码 RX 到 {:#x}，TSM 数据 RW 到 {:#x}，其后是 TVM 页池",
        confidential_start(),
        confidential_end(),
        code_region_end(),
        tsm_data_end(),
    );
}

/// 安装条目。失败一律停机（fail-stop）：配置不对时继续跑等于放弃隔离。
pub fn install() {
    // 前置校验：固件那条"宿主 RAM RWX"应当是 6 号、`L=0 RWX`、上界 = RAM 顶。
    // 不满足说明固件的链条形状变了，我们的"只改建 6 号以后"假设不再成立。
    let host_ram_cfg = entry_at(slot::HOST_RAM, read_csr!("0x3a0"), read_csr!("0x3a2"));
    let host_ram_top = read_csr!("0x3b6") << 2;
    if host_ram_cfg.match_mode != Match::Tor
        || host_ram_cfg.locked
        || host_ram_cfg.permissions() != "RWX"
        || host_ram_top != confidential_end()
    {
        crate::println!(
            "\x1b[31m!! PMP: 固件链条形状不符预期（entry {} A={} {} L={} top={:#x}，期望 TOR RWX L=0 top={:#x}）\x1b[0m",
            slot::HOST_RAM,
            host_ram_cfg.match_mode.name(),
            host_ram_cfg.permissions(),
            host_ram_cfg.locked as u8,
            host_ram_top,
            confidential_end()
        );
        crate::halt();
    }

    // 1. 先把所有 pmpaddr 写对（此刻没有任何 `L=1` 条目，可自由修改）。
    write_csr!("0x3b6", confidential_start() >> 2); // 6: 宿主 RAM 止于机密区
    write_csr!("0x3b7", code_region_end() >> 2); // 7: 代码区终点
    write_csr!("0x3b8", tsm_data_end() >> 2); // 8: TSM 数据区终点
    write_csr!("0x3b9", confidential_end() >> 2); // 9: 页池终点（也是 10 号的下界）
    write_csr!("0x3ba", usize::MAX >> 2); // 10: 无上界

    // 2. `pmpcfg0`：保留 0..5 原样，只改 6/7。7 号在这一步变成 `L=1` 并锁死。
    let host_ram = config_byte(false, TOR, RWX);
    let tsm_code = config_byte(true, TOR, RX);
    let cfg0 = replace_byte(read_csr!("0x3a0"), slot::HOST_RAM, host_ram);
    let cfg0 = replace_byte(cfg0, slot::TSM_CODE, tsm_code);
    write_csr!("0x3a0", cfg0);

    // 3. `pmpcfg2`：8 号 TSM 数据（L=1 RW）、9 号页池（OFF 占位）、10 号 RAM 以上（NONE）。
    let tsm_data = config_byte(true, TOR, RW);
    let tvm_pool = config_byte(false, OFF, NONE);
    let above_ram = config_byte(false, TOR, NONE);
    let mut cfg2 = read_csr!("0x3a2");
    cfg2 = replace_byte(cfg2, slot::TSM_DATA - 8, tsm_data);
    cfg2 = replace_byte(cfg2, slot::TVM_POOL - 8, tvm_pool);
    cfg2 = replace_byte(cfg2, slot::ABOVE_RAM - 8, above_ram);
    write_csr!("0x3a2", cfg2);

    // 对账：读回来看是否真写进去了（`L=1` 之后没有第二次机会）。
    if read_csr!("0x3a0") != cfg0 || read_csr!("0x3a2") != cfg2 {
        crate::println!("\x1b[31m!! PMP: 写后读回不一致，配置没有生效\x1b[0m");
        crate::halt();
    }
    crate::println!(
        "PMP: 已装入 {} 条（0..{}），机密区对外封闭",
        11,
        slot::ABOVE_RAM
    );
}

/// 把一个 64 位 `pmpcfg` 里的第 `index` 个字节换成 `byte`。
fn replace_byte(word: usize, index: usize, byte: usize) -> usize {
    let shift = 8 * index;
    (word & !(0xff << shift)) | ((byte & 0xff) << shift)
}

fn entry_at(index: usize, cfg0: usize, cfg2: usize) -> Entry {
    let word = if index < 8 { cfg0 } else { cfg2 };
    Entry::from_byte(index, (word >> (8 * (index % 8))) & 0xff)
}
