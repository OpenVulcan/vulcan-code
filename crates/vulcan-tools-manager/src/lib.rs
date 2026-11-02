//! Host-owned unified tool manager that resolves the final AI-visible tool surface and dispatches tool calls.
//! 负责解析最终 AI 可见工具面并调度工具调用的宿主自有统一工具管理器。

use serde_json::{Map, Value};
use std::collections::BTreeSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use vulcan_session_core::v1::ids::{ApiCallId, RequestStreamId, Seq, ToolCallId, TurnId};

mod manager;
pub mod public_name;
mod public_names;
mod request_streaming;
pub mod result_markdown;
mod schema_projection;
#[cfg(test)]
mod tests;
mod types;

pub use manager::VulcanToolsManager;
pub use public_name::public_tool_name_is_valid;
pub use request_streaming::{
    IncrementalJsonEvent, IncrementalJsonPathSegment, IncrementalJsonScalar,
    ToolRequestParserContext, ToolRequestParserFactory, ToolRequestParserSession,
    ToolRequestPreview, ToolRequestPreviewLine, ToolRequestProgressPolicy,
    ToolRequestSensitivityPolicy, ToolRequestStagingPolicy, ToolRequestStagingStatus,
    ToolRequestStreamingCapability,
};
pub use schema_projection::project_tool_parameters_schema;
pub use types::{
    ResolvedToolDefinition, ResolvedToolSet, VULCAN_TOOL_ERROR_DETAIL_KIND, VulcanPlanToolPolicy,
    VulcanToolCandidate, VulcanToolDescriptor, VulcanToolHostResult, VulcanToolInvocationContext,
    VulcanToolInvocationResult, VulcanToolInvokeFuture, VulcanToolInvokeHandler, VulcanToolMode,
    VulcanToolOrigin, VulcanToolRawStreams, VulcanToolSchemaProjectionProfile,
    VulcanToolSourceIdentity, VulcanToolSourceKind,
};

#[cfg(test)]
use manager::tool_is_allowed_in_plan;
pub use manager::{text_looks_read_only, tool_is_allowed_in_plan_with_args};
use public_names::normalize_candidate_public_tool_names;
