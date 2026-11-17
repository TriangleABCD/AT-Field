// SPDX-License-Identifier: Apache-2.0

//! 构建脚本：只做一件事——让 `linker.ld` 变化时重新链接。
//!
//! 不加这个的话，改完链接脚本 `cargo build` 会因为"没有源文件变化"而跳过链接，
//! 产出仍是旧镜像（实测踩过：改了 `_conf_start`，跑出来还是老地址）。

fn main() {
    println!("cargo:rerun-if-changed=linker.ld");
    println!("cargo:rerun-if-changed=src/start.S");
    println!("cargo:rerun-if-changed=src/trap.S");
}
