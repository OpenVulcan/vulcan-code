//! Shared host-tool infrastructure reused by the WebUI native-tool integration layer.
//! 供WebUI原生工具集成层复用的共享宿主工具基础设施。

// Test fixtures may spawn helper processes directly; production code goes through the shared factories.
// 测试夹具可直接启动辅助进程；生产代码一律经由共享工厂。
#![cfg_attr(test, allow(clippy::disallowed_methods))]

pub mod command_session_runtime;
pub mod file_runtime;
pub mod judge_host_client;
pub mod native_host_binary;
pub mod output_decode;
pub mod process_creation;
pub mod process_tree;
pub mod project_temp_maintenance;
pub mod project_terminal_runtime;
pub mod result_format;
pub mod rtk;
pub mod shell_host;
pub mod terminal_host_client;
pub mod text_document;
pub mod text_encoding;
pub use vulcan_terminal_host_protocol as terminal_host_protocol;

pub(crate) use vulcan_runtime_assets as runtime_assets;
pub(crate) use vulcan_workbench_model as model;
