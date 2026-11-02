//! Ordered, dependent tool calls for the `tool_chain` model tool.
//! `tool_chain` 模型工具的有序、相互依赖的工具调用。
//!
//! Parallel tool calls cover independent work; a chain covers calls whose arguments or whether
//! they run depend on earlier calls. This crate owns everything that does not touch the host:
//! the schema and description, plan validation, the expression language and ordered execution
//! against a host-provided [`StepExecutor`].
//! 并行工具调用覆盖相互独立的工作；链条覆盖参数或是否运行取决于更早调用的工具调用。本 crate
//! 负责一切不触及宿主的部分：Schema 与说明、计划校验、表达式语言，以及基于宿主提供的
//! [`StepExecutor`] 的顺序执行。

mod expr;
mod plan;
mod render;
mod run;
mod schema;
mod template;

#[cfg(test)]
mod tests;

pub use plan::{ToolChainPlan, parse_tool_chain_plan};
pub use render::{ToolChainReport, render_report};
pub use run::{StepExecution, StepExecutor, StepOutcome, StepRecord, run_tool_chain};
pub use schema::{tool_chain_description, tool_chain_input_schema};

/// Stable tool id and model-visible name.
/// 稳定的工具标识与模型可见名称。
pub const TOOL_CHAIN_TOOL_ID: &str = "tool_chain";

/// Host result kind carrying the per-step summary.
/// 承载逐步摘要的宿主结果类型。
pub const TOOL_CHAIN_HOST_RESULT_KIND: &str = "tool_chain";

/// Step limit used when the user has not chosen one.
/// 用户未选择时使用的步骤上限。
pub const DEFAULT_TOOL_CHAIN_MAX_STEPS: u32 = 16;

/// Smallest step limit a user may choose.
/// 用户可选择的最小步骤上限。
pub const MIN_TOOL_CHAIN_MAX_STEPS: u32 = 2;

/// Largest step limit a user may choose.
/// 用户可选择的最大步骤上限。
pub const MAX_TOOL_CHAIN_MAX_STEPS: u32 = 64;
