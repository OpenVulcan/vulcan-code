//! Native tool registration surfaces for the compose runtime foundation.
//! compose 运行时基础架构使用的原生工具注册面。

pub mod bridge;
pub mod change_set;
pub mod command_session_runtime;
pub mod native;
pub mod output_decode;
pub mod public_name;
pub mod result_format;
pub mod shell_host;

pub use command_session_runtime::CommandSessionRuntime;
pub use native::{
    NativeToolCatalog, NativeToolDescriptor, NativeToolExecutionRequest, build_native_tool_catalog,
};
pub use public_name::{
    allocate_unique_public_tool_name, build_direct_public_tool_name,
    build_prefixed_public_tool_name,
};
