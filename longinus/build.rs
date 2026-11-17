// SPDX-License-Identifier: Apache-2.0

//! 构建脚本：链接脚本 + 重新构建的触发条件。
//!
//! 为什么不用 `.cargo/config.toml` 里的 `rustflags`：那是**按目录继承**的，
//! `AT-Field/.cargo/config.toml` 里的 `-Tlinker.ld` 会被本 crate 连带继承，
//! 与自己的 `-Tlonginus.lds` 打架。

fn main() {
    println!("cargo:rerun-if-changed=longinus.lds");
    println!("cargo:rerun-if-changed=src/start.S");
    println!("cargo:rustc-link-arg=-Tlonginus.lds");
    println!("cargo:rustc-link-arg=--no-relax");
}
