//! Vulcan Code terminal client.
//! Vulcan Code 终端客户端。
//!
//! `backend` is the only data boundary (core service or scripted mock), `app` holds state and
//! input handling, `ui` draws. The library exposes only the binary entry points.
//! `backend` 是唯一的数据边界（核心服务或脚本化模拟），`app` 保存状态与输入处理，`ui` 负责绘制。
//! 库只公开二进制入口。

mod app;
mod backend;
mod cli;
mod i18n;
mod model;
mod service_api;
mod ui;

#[cfg(test)]
mod render_smoke_tests;

pub use cli::main;
