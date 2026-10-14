//! Integration coverage for exact account selection, signed bindings, and generation isolation.
//! 精确账号选择、签名绑定与代际隔离的集成覆盖。

mod accounts;
mod custom_bindings;
mod generations;
// Verify OpenDesign selection through the production request encoder.
// 通过生产请求编码器验证 OpenDesign 选择链。
mod open_design;
mod system_bindings;
