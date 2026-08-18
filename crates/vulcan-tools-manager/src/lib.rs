//! Host-owned unified tool manager that resolves the final AI-visible tool surface and dispatches tool calls.
//! 负责解析最终 AI 可见工具面并调度工具调用的宿主自有统一工具管理器。

use serde_json::{Map, Value};
use std::collections::BTreeSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Maximum public tool-name length kept compatible with strict provider validators.
/// 与严格 Provider 校验器兼容的公开工具名最大长度。
const MAX_PUBLIC_TOOL_NAME_LEN: usize = 64;

mod manager;
mod public_names;
mod request_streaming;
mod schema_projection;
#[cfg(test)]
mod tests;
mod types;

pub use manager::VulcanToolsManager;
pub use public_names::public_tool_name_is_valid;
pub use request_streaming::{
    IncrementalJsonEvent, IncrementalJsonPathSegment, IncrementalJsonScalar,
    ToolRequestParserContext, ToolRequestParserFactory, ToolRequestParserSession,
    ToolRequestPreview, ToolRequestProgressPolicy, ToolRequestSensitivityPolicy,
    ToolRequestStagingPolicy, ToolRequestStagingStatus, ToolRequestStreamingCapability,
};
pub use schema_projection::project_tool_parameters_schema;
pub use types::{
    ResolvedToolDefinition, ResolvedToolSet, VULCAN_TOOL_ERROR_DETAIL_KIND, VulcanPlanToolPolicy,
    VulcanToolCandidate, VulcanToolDescriptor, VulcanToolHostResult, VulcanToolInvocationResult,
    VulcanToolInvokeFuture, VulcanToolInvokeHandler, VulcanToolMode,
    VulcanToolSchemaProjectionProfile, VulcanToolSourceKind,
};

#[cfg(test)]
use manager::tool_is_allowed_in_plan;
use public_names::normalize_candidate_public_tool_names;
