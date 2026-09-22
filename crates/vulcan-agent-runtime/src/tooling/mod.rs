//! Native tool registration surfaces for the compose runtime foundation.
//! compose 运行时基础架构使用的原生工具注册面。

pub mod bridge;
pub mod collaboration;
pub mod native;

pub use command_session_runtime::CommandSessionRuntime;
pub use native::{
    NativeToolCatalog, NativeToolDescriptor, NativeToolExecutionRequest, build_native_tool_catalog,
};
pub use public_name::{
    allocate_unique_public_tool_name, build_direct_public_tool_name,
    build_prefixed_public_tool_name,
};
pub use vulcan_change_set as change_set;
pub use vulcan_host_tools::{
    command_session_runtime, output_decode, public_name, result_format, shell_host,
};
