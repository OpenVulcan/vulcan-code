//! Ordered execution of a validated chain.
//! 已校验链条的顺序执行。

use serde_json::Value;
use std::future::Future;

use crate::expr::{EvalContext, evaluate, truthy};
use crate::plan::ToolChainPlan;
use crate::template::{ArgNode, resolve_arguments};

/// Largest total size of command streams and structured results one chain keeps for references.
/// 单个链条为引用而保留的命令流与结构化结果的最大总大小。
///
/// Every command is already capped on its own; this bounds the whole chain, so many compliant
/// steps cannot together hold an unbounded amount of memory.
/// 每条命令已各自受限；此上限约束整个链条，使许多合规步骤合起来也无法占用无界内存。
pub const MAX_KEPT_VALUE_BYTES: usize = 16 * 1024 * 1024;

/// Largest serialized size of one step's arguments after its expressions are resolved.
/// 单个步骤在表达式解析后参数的最大序列化大小。
///
/// Resolved arguments are recorded with the step, so copying a large stream into them would
/// enlarge the session history; large data should travel through a file.
/// 已解析参数随步骤记录，把大量流内容复制进去会使会话历史膨胀；大数据应经由文件传递。
pub const MAX_STEP_ARGUMENT_BYTES: usize = 1024 * 1024;

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
        /// Decoded standard output and error of command tools, already redacted like `output`.
        /// 命令类工具解码后的标准输出与标准错误，已与 `output` 同样脱敏。
        streams: Option<(String, String)>,
        /// Structured value the tool returned to the model instead of text, if any.
        /// 工具以结构化值而非文本返回给模型的结果（如有）。
        result: Option<Value>,
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

/// Values of one step that later expressions may read but that are never reported.
/// 后续表达式可以读取、但从不进入报告的单步取值。
#[derive(Debug, Clone, Default)]
pub(crate) enum KeptValues {
    /// The tool did not run.
    /// 工具未运行。
    #[default]
    None,
    /// Values kept for references.
    /// 为引用保留的取值。
    Kept {
        /// Command streams, for command tools.
        /// 命令流，仅命令类工具有。
        streams: Option<(String, String)>,
        /// Structured result, when the tool returned one.
        /// 结构化结果（如工具返回）。
        result: Option<Value>,
    },
    /// The chain-wide budget was used up, so nothing was kept.
    /// 链条级预算已用尽，因此未保留任何内容。
    OverBudget,
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
    /// Whether a failure of this step was allowed by `continue_on_error`.
    /// 本步骤的失败是否由 `continue_on_error` 允许。
    pub continue_on_error: bool,
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
    /// In-memory values for `steps.<id>.stdout`, `.stderr` and `.result`.
    /// 供 `steps.<id>.stdout`、`.stderr` 与 `.result` 使用的内存取值。
    pub(crate) kept: KeptValues,
}

impl StepRecord {
    /// Whether this step failed in a way that makes `failure()` true and skips default steps.
    /// 本步骤的失败是否使 `failure()` 为真并跳过默认条件的步骤。
    pub(crate) fn counts_as_failure(&self) -> bool {
        self.outcome == StepOutcome::Failure && !self.continue_on_error
    }
}

/// Runs every step in order and returns one record per step.
/// 按顺序运行每个步骤并为每个步骤返回一条记录。
///
/// A denial always halts the chain, whatever later conditions or `continue_on_error` say. A user
/// stop drops this future, so no later step can start either.
/// 拒绝总会使链条停止，无论后续条件或 `continue_on_error` 如何。用户停止会丢弃此 Future，
/// 因此后续步骤同样无法开始。
pub async fn run_tool_chain(plan: &ToolChainPlan, executor: &impl StepExecutor) -> Vec<StepRecord> {
    let mut records: Vec<StepRecord> = Vec::with_capacity(plan.steps.len());
    let mut halted = false;
    let mut kept_bytes = 0usize;
    for (index, step) in plan.steps.iter().enumerate() {
        let mut record = StepRecord {
            id: step.id.clone(),
            tool: step.tool.clone(),
            outcome: StepOutcome::NotRun,
            continue_on_error: step.continue_on_error,
            output: String::new(),
            exit_code: None,
            detail: None,
            arguments: None,
            display: None,
            kept: KeptValues::None,
        };
        if halted {
            records.push(record);
            continue;
        }

        let context = EvalContext { earlier: &records };
        let should_run = match &step.condition {
            None => Ok(!context.any_failed()),
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
            Ok(true) => resolve_arguments(&step.arguments, &context, step.policy.command_arguments)
                .and_then(|arguments| {
                    // Arguments written without expressions are what a direct call would send.
                    // 不含表达式的参数与直接调用发送的内容相同。
                    if matches!(step.arguments, ArgNode::Static(_)) {
                        Ok(arguments)
                    } else {
                        check_argument_size(arguments)
                    }
                }),
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
                streams,
                result,
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
                let size = streams
                    .as_ref()
                    .map_or(0, |(stdout, stderr)| stdout.len() + stderr.len())
                    + result.as_ref().map_or(0, |value| value.to_string().len());
                record.kept = if kept_bytes + size > MAX_KEPT_VALUE_BYTES {
                    KeptValues::OverBudget
                } else {
                    kept_bytes += size;
                    KeptValues::Kept { streams, result }
                };
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

/// Rejects resolved arguments larger than [`MAX_STEP_ARGUMENT_BYTES`].
/// 拒绝大于 [`MAX_STEP_ARGUMENT_BYTES`] 的已解析参数。
fn check_argument_size(arguments: Value) -> Result<Value, String> {
    let size = arguments.to_string().len();
    if size > MAX_STEP_ARGUMENT_BYTES {
        return Err(format!(
            "the resolved arguments are {size} bytes, over the {MAX_STEP_ARGUMENT_BYTES}-byte limit for one step; write large data to a file in one step and read it in the next"
        ));
    }
    Ok(arguments)
}

/// Builds the detail for a step whose expressions failed; earlier outputs already appear in the
/// report, so they are not repeated here.
/// 为表达式失败的步骤构建说明；更早步骤的输出已在报告中出现，此处不重复。
fn expression_failure(error: &str) -> String {
    format!("expression error {error}; the tool was not called")
}
