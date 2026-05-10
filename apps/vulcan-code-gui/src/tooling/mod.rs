//! Native tool registration surfaces for the compose runtime foundation.
//! compose 运行时基础架构使用的原生工具注册面。

pub mod native;

pub use native::{NativeToolCatalog, NativeToolDescriptor, build_native_tool_catalog};
