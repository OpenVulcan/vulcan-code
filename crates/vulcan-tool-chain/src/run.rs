//! Ordered execution of a validated chain.
//! 已校验链条的顺序执行。

use serde_json::Value;
use std::future::Future;

use crate::expr::{EvalContext, evaluate, truthy};
use crate::plan::ToolChainPlan;
use crate::template::resolve_arguments;

/// Final state of one step.
/// 单个步骤的最终状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepOutcome {
    /// The tool ran and succeeded.
    /// 工具已运行并成功。
    Success,
    /// The tool ran and failed, or the step's expressions could not be evaluated.
    /// 工具已运行但失败，或步骤表达式无法求值。
    Failure,
    /// The `if` condition was false.
    /// `if` 条件为假。
    Skipped,
    /// Permission was not granted; the chain stopped here.
    /// 未获得权限；链条在此停止。
    Denied,
    /// An earlier step was denied, so this step was never considered.
    /// 更早的步骤被拒绝，因此本步骤从未被考虑。
    NotRun,
}

impl StepOutcome {
    /// Stable wire name used in host results.
    /// 宿主结果中使用的稳定线上名称。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "failure",
            Self::Skipped => "skipped",
            Self::Denied => "denied",
            Self::NotRun => "not_run",
        }
    }

    /// Value of `steps.<id>.outcome` in expressions.
    /// 表达式中 `steps.<id>.outcome` 的取值。
    pub(crate) fn expression_name(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure | Self::Denied => "failure",
            Self::Skipped | Self::NotRun => "skipped",
        }
    }

    /// Whether the step produced output.
    /// 步骤是否产生了输出。
    pub(crate) fn ran(self) -> bool {
        matches!(self, Self::Success | Self::Failure)
    }
}

/// Result reported by the host for one step it was asked to run.
/// 宿主对一个被要求运行的步骤报告的结果。
#[derive(Debug, Clone)]
pub enum StepExecution {
    /// The tool ran to completion, successfully or not.
    /// 工具运行结束，无论成功与否。
    Completed {
        /// Text the model would see for a direct call.
        /// 直接调用时模型会看到的文本。
        output: String,
        /// Whether the call counts as failed.
        /// 调用是否计为失败。
        failed: bool,
        /// Exit code for command tools.
        /// 命令类工具的退出码。
        exit_code: Option<i64>,
        /// Host data needed to render this step as its own tool card; opaque to the chain.
        /// 宿主将本步骤渲染为独立工具卡片所需的数据；对链条不透明。
        display: Option<Value>,
    },
    /// Permission was not granted; the chain stops.
    /// 未获得权限；链条停止。
    Denied {
        /// Reason shown to the model.
        /// 向模型展示的原因。
        message: String,
    },
}

/// Host side of chain execution: runs one resolved step through the normal tool pipeline.
/// 链条执行的宿主侧：经由常规工具管线运行一个已解析的步骤。
pub trait StepExecutor {
    /// Runs one step; `index` is zero-based.
    /// 运行一个步骤；`index` 从零开始。
    fn execute(
        &self,
        index: usize,
        tool: &str,
        arguments: Value,
    ) -> impl Future<Output = StepExecution> + Send;
}

/// Record of one step after the chain finished.
/// 链条结束后单个步骤的记录。
#[derive(Debug, Clone)]
pub struct StepRecord {
    /// Step id, when the model gave one.
    /// 模型给出的步骤标识。
    pub id: Option<String>,
    /// Tool name.
    /// 工具名。
    pub tool: String,
    /// Final state.
    /// 最终状态。
    pub outcome: StepOutcome,
    /// Tool output; empty when the tool did not run.
    /// 工具输出；工具未运行时为空。
    pub output: String,
    /// Exit code for command tools.
    /// 命令类工具的退出码。
    pub exit_code: Option<i64>,
    /// Explanation for skipped, denied or expression-failed steps.
    /// 跳过、拒绝或表达式失败步骤的说明。
    pub detail: Option<String>,
    /// Resolved arguments the tool was called with.
    /// 调用工具时使用的已解析参数。
    pub arguments: Option<Value>,
    /// Host display data returned with the step's result.
    /// 随步骤结果返回的宿主展示数据。
    pub display: Option<Value>,
}

/// Runs every step in order and returns one record per step.
/// 按顺序运行每个步骤并为每个步骤返回一条记录。
pub async fn run_tool_chain(plan: &ToolChainPlan, executor: &impl StepExecutor) -> Vec<StepRecord> {
    let mut records: Vec<StepRecord> = Vec::with_capacity(plan.steps.len());
    let mut halted = false;
    for (index, step) in plan.steps.iter().enumerate() {
        let mut record = StepRecord {
            id: step.id.clone(),
            tool: step.tool.clone(),
            outcome: StepOutcome::NotRun,
            output: String::new(),
            exit_code: None,
            detail: None,
            arguments: None,
            display: None,
        };
        if halted {
            records.push(record);
            continue;
        }

        let context = EvalContext { earlier: &records };
        let should_run = match &step.condition {
            None => Ok(!records
                .iter()
                .any(|earlier| earlier.outcome == StepOutcome::Failure)),
            Some(condition) => evaluate(condition, &context).map(|value| truthy(&value)),
        };
        let arguments = match should_run {
            Ok(false) => {
                record.outcome = StepOutcome::Skipped;
                record.detail = Some(String::from(match step.condition {
                    None => "skipped because an earlier step failed",
                    Some(_) => "skipped because its `if` condition was false",
                }));
                records.push(record);
                continue;
            }
            Ok(true) => resolve_arguments(&step.arguments, &context),
            Err(error) => Err(format!("in `if`: {error}")),
        };
        let arguments = match arguments {
            Ok(arguments) => arguments,
            Err(error) => {
                record.outcome = StepOutcome::Failure;
                record.detail = Some(expression_failure(&error));
                records.push(record);
                continue;
            }
        };

        record.arguments = Some(arguments.clone());
        match executor.execute(index, &step.tool, arguments).await {
            StepExecution::Completed {
                output,
                failed,
                exit_code,
                display,
            } => {
                record.outcome = if failed {
                    StepOutcome::Failure
                } else {
                    StepOutcome::Success
                };
                record.output = output;
                record.exit_code = exit_code;
                record.display = display;
            }
            StepExecution::Denied { message } => {
                record.outcome = StepOutcome::Denied;
                record.detail = Some(message);
                halted = true;
            }
        }
        records.push(record);
    }
    records
}

/// Builds the detail for a step whose expressions failed; earlier outputs already appear in the
/// report, so they are not repeated here.
/// 为表达式失败的步骤构建说明；更早步骤的输出已在报告中出现，此处不重复。
fn expression_failure(error: &str) -> String {
    format!("expression error {error}; the tool was not called")
}
