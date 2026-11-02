//! Validation of `tool_chain` arguments into an executable plan.
//! 将 `tool_chain` 参数校验为可执行计划。

use regex::Regex;
use serde_json::Value;
use std::sync::LazyLock;

use crate::expr::{Expr, parse_expression, referenced_steps};
use crate::template::{ArgNode, argument_references, parse_arguments};

/// Step id pattern, matching the ids models already write in workflow files.
/// 步骤标识格式，与模型在工作流文件中常写的标识一致。
static STEP_ID_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[A-Za-z_][A-Za-z0-9_-]*$")
        .unwrap_or_else(|_| unreachable!("static step id pattern"))
});

/// Keys allowed on one step object.
/// 单个步骤对象允许的键。
const STEP_KEYS: [&str; 4] = ["id", "tool", "if", "arguments"];

/// Validated chain ready to run.
/// 已校验、可运行的链条。
#[derive(Debug, Clone)]
pub struct ToolChainPlan {
    pub(crate) steps: Vec<PlannedStep>,
}

impl ToolChainPlan {
    /// Number of steps in the chain.
    /// 链条中的步骤数。
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// Whether the chain has no steps; validation never produces one.
    /// 链条是否没有步骤；校验从不产生空链条。
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// Tool names in step order.
    /// 按步骤顺序排列的工具名。
    pub fn tools(&self) -> impl Iterator<Item = &str> {
        self.steps.iter().map(|step| step.tool.as_str())
    }
}

/// One validated step.
/// 一个已校验的步骤。
#[derive(Debug, Clone)]
pub(crate) struct PlannedStep {
    /// Optional step id used by later references.
    /// 供后续引用的可选步骤标识。
    pub(crate) id: Option<String>,
    /// Tool name as the model sees it.
    /// 模型可见的工具名。
    pub(crate) tool: String,
    /// Parsed `if` condition; `None` means `success()`.
    /// 已解析的 `if` 条件；`None` 表示 `success()`。
    pub(crate) condition: Option<Expr>,
    /// Arguments with parsed templates.
    /// 已解析模板的参数。
    pub(crate) arguments: ArgNode,
}

/// Validates raw `tool_chain` arguments; every problem is reported in one message.
/// 校验原始 `tool_chain` 参数；所有问题在一条消息中一并报告。
///
/// `check_tool` decides whether a tool name may be used as a step and explains why not.
/// `check_tool` 判定工具名能否用作步骤，并说明不能的原因。
pub fn parse_tool_chain_plan(
    arguments: &Value,
    max_steps: usize,
    check_tool: impl Fn(&str) -> Result<(), String>,
) -> Result<ToolChainPlan, String> {
    let root = arguments.as_object().ok_or_else(|| {
        String::from("tool_chain arguments must be an object with a `steps` array")
    })?;
    let mut errors = Vec::new();
    for key in root.keys().filter(|key| key.as_str() != "steps") {
        errors.push(format!(
            "unknown top-level field `{key}`; only `steps` is allowed"
        ));
    }
    let steps = match root.get("steps") {
        Some(Value::Array(steps)) => steps,
        Some(Value::String(_)) => {
            return Err(String::from(
                "`steps` must be a JSON array of step objects, not a string containing JSON",
            ));
        }
        Some(_) => return Err(String::from("`steps` must be an array of step objects")),
        None => return Err(String::from("`steps` is required")),
    };
    if steps.is_empty() {
        errors.push(String::from("`steps` must contain at least one step"));
    }
    if steps.len() > max_steps {
        errors.push(format!(
            "the chain has {} steps but at most {max_steps} are allowed; split it into several chains",
            steps.len()
        ));
    }

    let mut planned = Vec::with_capacity(steps.len());
    let mut known_ids: Vec<String> = Vec::new();
    for (index, raw) in steps.iter().enumerate() {
        let label = format!("step {}", index + 1);
        let Some(step) = raw.as_object() else {
            errors.push(format!(
                "{label}: must be an object with `tool` and `arguments`"
            ));
            continue;
        };
        let label = match step.get("id").and_then(Value::as_str) {
            Some(id) => format!("step {} (`{id}`)", index + 1),
            None => label,
        };
        for key in step.keys().filter(|key| !STEP_KEYS.contains(&key.as_str())) {
            errors.push(format!(
                "{label}: unknown field `{key}`; allowed fields are id, tool, if, arguments"
            ));
        }

        let id = match step.get("id") {
            None => None,
            Some(Value::String(id)) if !STEP_ID_PATTERN.is_match(id) => {
                errors.push(format!(
                    "{label}: id must start with a letter or `_` and contain only letters, digits, `_` and `-`"
                ));
                None
            }
            Some(Value::String(id)) if known_ids.contains(id) => {
                errors.push(format!(
                    "{label}: id `{id}` is already used by an earlier step"
                ));
                None
            }
            Some(Value::String(id)) => Some(id.clone()),
            Some(_) => {
                errors.push(format!("{label}: id must be a string"));
                None
            }
        };

        let tool = match step.get("tool") {
            Some(Value::String(tool)) if !tool.trim().is_empty() => {
                if let Err(reason) = check_tool(tool) {
                    errors.push(format!("{label}: {reason}"));
                }
                tool.clone()
            }
            _ => {
                errors.push(format!("{label}: `tool` must be a non-empty tool name"));
                String::new()
            }
        };

        let condition = match step.get("if") {
            None | Some(Value::Null) => None,
            Some(Value::Bool(flag)) => Some(Expr::Literal(Value::Bool(*flag))),
            Some(Value::String(source)) => match parse_expression(strip_wrapper(source)) {
                Ok(expr) => Some(expr),
                Err(error) => {
                    errors.push(format!("{label}: in `if`: {error}"));
                    None
                }
            },
            Some(_) => {
                errors.push(format!(
                    "{label}: `if` must be an expression string such as \"failure()\""
                ));
                None
            }
        };

        let arguments = match step.get("arguments") {
            Some(value @ Value::Object(_)) => {
                let mut argument_errors = Vec::new();
                let node = parse_arguments(value, "arguments", &mut argument_errors);
                errors.extend(
                    argument_errors
                        .into_iter()
                        .map(|error| format!("{label}: {error}")),
                );
                node
            }
            Some(Value::String(_)) => {
                errors.push(format!(
                    "{label}: `arguments` must be a JSON object, not a string containing JSON"
                ));
                ArgNode::Static(Value::Null)
            }
            Some(_) => {
                errors.push(format!("{label}: `arguments` must be an object"));
                ArgNode::Static(Value::Null)
            }
            None => {
                errors.push(format!(
                    "{label}: `arguments` is required; use {{}} for a tool without arguments"
                ));
                ArgNode::Static(Value::Null)
            }
        };

        let mut references = Vec::new();
        if let Some(condition) = &condition {
            referenced_steps(condition, &mut references);
        }
        argument_references(&arguments, &mut references);
        references.sort_unstable();
        references.dedup();
        for reference in references {
            if !known_ids.iter().any(|known| known == reference) {
                let later = steps
                    .iter()
                    .skip(index)
                    .any(|other| other.get("id").and_then(Value::as_str) == Some(reference));
                errors.push(if later {
                    format!("{label}: `steps.{reference}` refers to this or a later step; only earlier steps can be referenced")
                } else {
                    format!("{label}: `steps.{reference}` does not match the id of any earlier step")
                });
            }
        }

        if let Some(id) = &id {
            known_ids.push(id.clone());
        }
        planned.push(PlannedStep {
            id,
            tool,
            condition,
            arguments,
        });
    }

    if errors.is_empty() {
        Ok(ToolChainPlan { steps: planned })
    } else {
        Err(format!(
            "tool_chain was not run because the plan is invalid:\n- {}",
            errors.join("\n- ")
        ))
    }
}

/// Accepts `if` written with or without the `${{ }}` wrapper, as GitHub Actions does.
/// 与 GitHub Actions 一样，接受带或不带 `${{ }}` 包裹的 `if`。
fn strip_wrapper(source: &str) -> &str {
    let trimmed = source.trim();
    trimmed
        .strip_prefix("${{")
        .and_then(|inner| inner.strip_suffix("}}"))
        .unwrap_or(trimmed)
}
