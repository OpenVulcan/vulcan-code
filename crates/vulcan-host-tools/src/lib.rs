//! Shared host-tool infrastructure reused by the WebUI native-tool integration layer.
//! 供WebUI原生工具集成层复用的共享宿主工具基础设施。

pub mod command_session_runtime;
pub mod output_decode;
pub mod process_creation;
pub mod project_temp_maintenance;
pub mod project_terminal_runtime;
pub mod public_name;
pub mod result_format;
pub mod shell_host;
pub mod terminal_host_client;
pub use vulcan_terminal_host_protocol as terminal_host_protocol;

pub(crate) use vulcan_runtime_assets as runtime_assets;
pub(crate) use vulcan_workbench_model as model;
