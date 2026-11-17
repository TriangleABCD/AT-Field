# longinus — S 态测试程序

> 名字取自《新世纪福音战士》的**朗基努斯之枪**（Lance of Longinus，ロンギヌスの槍）：
> 一把能穿透 AT Field 的枪。AT Field 是我们的 TSM，这把枪就是**从 S 态去戳它**的测试程序。

## 这份文档要解决什么

`longinus` 是什么、跑在哪、怎么和 TSM 说话、现在做到哪一步。设计细节等有代码之后再往
`docs/` 里搬。

## 它是什么

一个**独立的 S 态镜像**，不是 TSM 的一部分：

- TSM（AT Field）跑 M 模式，`longinus` 跑 S 模式，两者通过 **SBI ecall** 交互
  （v0 先用 base/console 这类标准扩展，COVH 有了之后再加）。
- 它验证的是**"TSM 承诺的行为是否真的成立"**：机密区读不到、TVM 生命周期正确、
  失败路径干净停机。这类检查只能从 S 态发起——从 M 态看自己是看不出隔离的。

## 仿照 TELLUS（Salus 的测试程序）

Salus 的 `test-workloads/` 是现成的参照（`upstream/salus/test-workloads/`），它同时产出
`tellus`（宿主侧 S 态进程）和 `guestvm`（TVM 侧），用一个二进制拼接脚本把两者打包。
我们只取**最基本的框架**，对应关系：

| TELLUS | 我们的 longinus | 说明 |
| --- | --- | --- |
| `tellus.lds`：链在 `0x80200000` | 同样链在 `0x80200000` | 与 QEMU/OpenSBI 的 kernel 约定一致，落在宿主 RAM（`0x8000_0000..0x1_8000_0000`）内 |
| `start.S`：设 gp、清 `sstatus`/`sie`、清 `.bss`、设 sp、调 `kernel_init` | 同样做，只有这一步 + 打印 | 不许假设 M 态替我们清好了什么 |
| `src/bin/tellus.rs` 里 `kernel_init` 的测试编排 | `src/main.rs` | 我们从"打印 + 读一个地址"开始 |
| `sbi-rs` 的 `ecall_send` / `SbiMessage` | 先手写 `ecall` 包装，够用再抽 crate | 避免为了 framework 先引入依赖 |
| `s-mode-utils` 的 `print!` / `SbiConsole` | 同样思路：S 态没有 UART 访问权（PMP 里 MMIO 给的是 RWX，但走 SBI console 更省事） | 不直接碰 16550，避免和 M 态抢同一个 THR |
| `test-system`（结果汇总、退出码） | 先最简：`PASS: ...` / `FAIL: ...` + 退出方式待定 | 完整的测试框架不在 v0 范围 |
| `guestvm`（TVM 侧）、`create_guest_image` | **不做** | 等到有 COVH/COVG 再谈 |
| Bazel `BUILD` | Cargo（`AT-Field` 的 workspace 成员） | 跟主仓库一致 |

## 现在做到哪一步（2026-11-18）

**骨架已跑通**：固件跳 `0x80200000` → longinus 清 `sstatus`/`sie`、清 `.bss`、设 `sp` →
打印一行、读 `sstatus`/`satp`，然后 `wfi` 停机。

```text
[RustSBI] Redirecting hart 0 to 0x00000080200000 in Supervisor mode
HART 0: AT Field init (primary)      ← TSM 装 PMP 后返回
[RustSBI] AT Field returned control to the firmware
[longinus] S 态已接管：hart 0，FDT @ 0x17ff80000
[longinus] sstatus = 0x200000000，satp = 0x0
[longinus] 骨架就绪（下一步：宿主内存可读写 / 机密区不可读）
```

`satp = 0`（bare，恒等映射）、`sstatus.UXL = 2`（64 位）、`sstatus.SIE = 0`——都符合预期。

## 三个已定的问题

1. **加载方式**：`-bios` 装固件、`-kernel` 装 longinus、`-device loader` 装 TSM（三件套）。
   QEMU 把 `-kernel` 放在 `ALIGN_UP(固件末, 2 MiB)` = `0x8020_0000`，并在 mask ROM 写一份
   `fw_dynamic_info`（`next_addr = 0x80200000`、`next_mode = S`）通过 `a2` 交给固件；
   所以固件保持 **dynamic 模式** 即可，不需要 jump/payload 模式。
2. **输出通道**：**直接写 UART MMIO**（`0x1000_0000`），不走 SBI console——少一层依赖，
   TSM 接管 trap 后这条路仍然可用。代价是和 M 态共用一个 THR，无跨组件锁。
3. **违例处理**：**先不装 `stvec`**。违规访问直接把整机打停，靠"停在预期位置"当证据；
   等测试框架成形再补 S 态 trap 处理。

## 构建与运行

```bash
cd ../run_cove
./build.sh          # 固件(dynamic) + AT Field + longinus → images/
./run.sh -s 1       # -bios 固件 / -kernel longinus / loader 装 TSM
```

链接脚本 `longinus.lds` 把入口钉在 `0x8020_0000` 并 `ASSERT`；`build.sh` 会核对入口地址。
链接选项来自 `build.rs` 而不是 `.cargo/config.toml`——后者按目录继承，
`../.cargo/config.toml` 里的 `-Tlinker.ld` 会和这里的 `-Tlonginus.lds` 打架。

## 下一批检查（按"能证明什么"排序）

1. **宿主内存可读写**：写读一段宿主 RAM，确认 PMP 没误伤自己。
2. **机密区读不到**：读 AT Field 的 `_conf_start`（`0x1_8000_0000`）。S 模式没有 `stvec`，
   所以这次读会把整机打停、trap 到 M 态（固件的 `mtvec`），靠固件打印的现场证明。
3. **标准 SBI 仍可用**：调 `base` 扩展取版本号，确认 TSM 没有把固件的 SBI 顶掉。
4. 之后再谈把结果码传出去（QEMU `testdev` 或 `sbi_reset`），那是测试框架的事。

## 目录内容

```text
longinus/
├── README.md
├── Cargo.toml          独立 workspace（自带 [workspace]，不并进 AT-Field 的）
├── build.rs            链接脚本 + rerun-if-changed
├── longinus.lds        链在 0x80200000，入口有 ASSERT
└── src/
    ├── main.rs         rust_main(hartid, fdt)：打印 + 读 sstatus/satp
    ├── start.S         _start：gp / sstatus·sie / bss / sp / call rust_main
    └── uart.rs         NS16550 直接写（print!/println! 宏）
```
