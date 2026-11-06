//! `tool_chain` execution: each step goes through the same plan-mode and permission gates as a
//! direct call, in order, inside the chain's own durable invocation.
//! `tool_chain` 执行：每个步骤按顺序经过与直接调用相同的计划模式与权限闸门，并处于链条自身的
//! 可靠调用之内。
//!
//! The chain container itself bypasses the permission gate, because the gate would consume a
//! declared intent meant for a real call; every step is admitted on its own instead.
//! 链条容器本身绕过权限闸门，因为闸门会消耗为真实调用声明的意图；改为逐个准入每个步骤。

use super::*;
use crate::tooling::native::{CHAINABLE_TOOL_IDS, SHELL_COMMAND_TOOL_ID};
use std::time::Instant;
use vulcan_tool_chain::{
    DEFAULT_TOOL_CHAIN_MAX_STEPS, StepExecution, StepExecutor, StepToolPolicy,
    TOOL_CHAIN_HOST_RESULT_KIND, parse_tool_chain_plan, render_report, run_tool_chain,
};
use vulcan_tools_manager::{
    PLAN_MODE_BLOCKED_MESSAGE, VulcanToolRawStreams, VulcanToolSourceKind, VulcanToolsManager,
};

/// Host result kind of one-shot commands, whose exit code decides step failure.
/// 一次性命令的宿主结果类型，其退出码决定步骤是否失败。
const COMMAND_RUN_HOST_RESULT_KIND: &str = "command_run";

/// Live `tool_chain` settings of one session, read when a chain is called.
/// 单个会话的实时 `tool_chain` 设置，在调用链条时读取。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolChainSettings {
    /// Whether the session may run chains now.
    /// 会话当前是否可以运行链条。
    pub enabled: bool,
    /// Largest number of steps in one chain.
    /// 单个链条的最大步骤数。
    pub max_steps: u32,
}

impl Default for ToolChainSettings {
    /// Enabled with the default step limit, used when no host is installed.
    /// 启用并使用默认步骤上限，用于未安装宿主时。
    fn default() -> Self {
        Self {
            enabled: true,
            max_steps: DEFAULT_TOOL_CHAIN_MAX_STEPS,
        }
    }
}

/// Service-side source of live `tool_chain` settings.
/// 服务侧的实时 `tool_chain` 设置来源。
pub trait ToolChainHost: Send + Sync {
    /// Returns the effective settings of `session_id` at call time.
    /// 返回 `session_id` 在调用时的生效设置。
    fn settings(&self, session_id: SessionId) -> Result<ToolChainSettings, String>;
}

/// Whether a resolved tool may be used as a chain step.
/// 已解析工具能否用作链条步骤。
fn tool_is_chainable(definition: &ResolvedToolDefinition) -> bool {
    match definition.descriptor.source {
        VulcanToolSourceKind::Native | VulcanToolSourceKind::VulcanManaged => {
            CHAINABLE_TOOL_IDS.contains(&definition.descriptor.id.as_str())
        }
        VulcanToolSourceKind::Mcp
        | VulcanToolSourceKind::LuaSkills
        | VulcanToolSourceKind::VulcanPlugin
        | VulcanToolSourceKind::AiHostBinding => false,
    }
}

impl VulcanManagedToolRuntimeBridge {
    /// Runs one `tool_chain` request and renders its per-step report.
    /// 运行一次 `tool_chain` 请求并渲染其逐步报告。
    pub(super) async fn execute_tool_chain(
        &self,
        request: &CoreToolInvocationRequest,
        context: VulcanToolInvocationContext,
    ) -> CoreToolInvocationResult {
        let settings = match self.tool_chain_host.as_ref() {
            Some(host) => match host.settings(self.session_id) {
                Ok(settings) => settings,
                Err(message) => return build_tool_failure_invocation_result(request, message),
            },
            None => ToolChainSettings::default(),
        };
        if !settings.enabled {
            return build_tool_failure_invocation_result(
                request,
                "tool_chain is disabled for this session. Make the calls directly as normal tool calls.",
            );
        }
        let plan = match parse_tool_chain_plan(
            &request.arguments,
            usize::try_from(settings.max_steps).unwrap_or(usize::MAX),
            |tool| self.check_chain_step(tool),
        ) {
            Ok(plan) => plan,
            Err(message) => return build_tool_failure_invocation_result(request, message),
        };
        let executor = BridgeStepExecutor {
            bridge: self,
            request,
            context,
        };
        let records = run_tool_chain(&plan, &executor).await;
        let report = render_report(&records);
        CoreToolInvocationResult {
            tool_call_id: request.tool_call_id.clone(),
            tool_name: request.tool_name.clone(),
            model_output: report.model_output,
            host_result: Some(json!({
                "kind": TOOL_CHAIN_HOST_RESULT_KIND,
                "payload": report.host_payload,
            })),
            is_error: false,
        }
    }

    /// Explains why a tool name cannot be a step, or accepts it with its policy.
    /// 说明工具名为何不能作为步骤，或连同其策略接受它。
    fn check_chain_step(&self, tool: &str) -> Result<StepToolPolicy, String> {
        match self.definitions_by_id.get(tool) {
            None => Err(format!("`{tool}` is not an available tool")),
            Some(definition) if !tool_is_chainable(definition) => Err(format!(
                "`{tool}` cannot be used inside tool_chain; call it directly. Only these tools can be steps: {}",
                CHAINABLE_TOOL_IDS.join(", ")
            )),
            Some(definition) => Ok(step_tool_policy(definition)),
        }
    }

    /// Redacts one step's result and kept values before later expressions can read them, so a
    /// chain passes on only what a direct call would have shown.
    /// 在后续表达式读取之前对单步结果及保留取值脱敏，使链条只传递直接调用会显示的内容。
    fn redact_step(&self, step: RawStep) -> Result<RawStep, VulcanCoreError> {
        let canonical = self.redact_sensitive_tool_result(step.canonical)?;
        let streams = match step.streams {
            Some(streams) => {
                let mut pair = json!([streams.stdout, streams.stderr]);
                self.redact_sensitive_value(&mut pair)?;
                let text = |index: usize| pair[index].as_str().unwrap_or_default().to_string();
                Some(VulcanToolRawStreams {
                    stdout: text(0),
                    stderr: text(1),
                })
            }
            None => None,
        };
        Ok(RawStep { canonical, streams })
    }
}

/// Policy of one chainable tool; only `shell_command` runs an argument as shell syntax.
/// 单个可链接工具的策略；只有 `shell_command` 将参数作为 shell 语法运行。
fn step_tool_policy(definition: &ResolvedToolDefinition) -> StepToolPolicy {
    match definition.descriptor.source {
        VulcanToolSourceKind::Native | VulcanToolSourceKind::VulcanManaged
            if definition.descriptor.id == SHELL_COMMAND_TOOL_ID =>
        {
            StepToolPolicy {
                command_arguments: &["command"],
            }
        }
        _ => StepToolPolicy::default(),
    }
}

/// One step result before it becomes the chain's step execution.
/// 成为链条步骤执行结果之前的单步结果。
struct RawStep {
    /// Canonical result, as a direct call would return it.
    /// 与直接调用返回相同的规范结果。
    canonical: CoreToolInvocationResult,
    /// Raw command streams, held in memory only.
    /// 仅在内存中保留的原始命令流。
    streams: Option<VulcanToolRawStreams>,
}

/// Runs chain steps through the bridge using the chain's invocation identity.
/// 使用链条的调用身份经由桥执行链条步骤。
struct BridgeStepExecutor<'a> {
    bridge: &'a VulcanManagedToolRuntimeBridge,
    request: &'a CoreToolInvocationRequest,
    context: VulcanToolInvocationContext,
}

impl StepExecutor for BridgeStepExecutor<'_> {
    fn execute(
        &self,
        _index: usize,
        tool: &str,
        arguments: Value,
    ) -> impl Future<Output = StepExecution> + Send {
        let tool = tool.to_string();
        async move {
            let started = Instant::now();
            let step_request = CoreToolInvocationRequest {
                tool_name: tool.clone(),
                arguments: arguments.clone(),
                ..self.request.clone()
            };
            let failure = |message: String| RawStep {
                canonical: build_tool_failure_invocation_result(&step_request, message),
                streams: None,
            };
            let Some(definition) = self.bridge.definitions_by_id.get(&tool) else {
                return self.finish(
                    failure(format!(
                        "Tool `{tool}` is not registered in the resolved definition set."
                    )),
                    started,
                );
            };
            // A Plan-mode block is a refusal of this step, so the chain stops instead of continuing.
            // 计划模式拦截即本步被拒，链随之停止而不是继续执行。
            if VulcanToolsManager::plan_mode_blocks(
                definition,
                &arguments,
                &self.bridge.execution_mode,
            ) {
                return StepExecution::Denied {
                    message: String::from(PLAN_MODE_BLOCKED_MESSAGE),
                };
            }
            let raw_arguments = arguments.to_string();
            let texts = PermissionTexts {
                raw_arguments: Some(raw_arguments.as_str()),
                user_request: self.request.user_request.text(),
            };
            let admitted = self
                .bridge
                .admit_definition(definition, &arguments, texts, &self.context)
                .await;
            let result = match admitted {
                Ok(None) => {
                    self.bridge
                        .manager
                        .invoke_tool(
                            definition,
                            arguments,
                            self.context.clone(),
                            &self.bridge.execution_mode,
                        )
                        .await
                }
                Ok(Some(message)) => return StepExecution::Denied { message },
                Err(message) => Err(message),
            };
            let step = match result {
                Ok(mut result) => {
                    if returns_media(&result.model_output) {
                        result.model_output = Value::String(format!(
                            "`{tool}` returned images, which tool_chain cannot pass on. Call it directly to see them."
                        ));
                    }
                    let streams = result.raw_streams.take();
                    RawStep {
                        canonical: canonicalize_tool_invocation_result(&step_request, result),
                        streams,
                    }
                }
                Err(message) => failure(message),
            };
            self.finish(step, started)
        }
    }
}

impl BridgeStepExecutor<'_> {
    /// Redacts one step and converts it; a broken filter fails the step and passes nothing on.
    /// 对单步脱敏并转换；过滤器损坏时该步骤失败且不传出任何内容。
    fn finish(&self, step: RawStep, started: Instant) -> StepExecution {
        match self.bridge.redact_step(step) {
            Ok(step) => completed_step(step, started),
            Err(error) => StepExecution::Completed {
                output: error.to_string(),
                failed: true,
                exit_code: None,
                streams: None,
                display: None,
            },
        }
    }
}

/// Whether a structured tool output carries media blocks.
/// 结构化工具输出是否携带媒体块。
fn returns_media(model_output: &Value) -> bool {
    crate::agent_runtime::tool_result_media::has_image_blocks(model_output)
}

/// Converts one canonical step result into the chain's step execution.
/// 将一个规范步骤结果转换为链条的步骤执行结果。
fn completed_step(step: RawStep, started: Instant) -> StepExecution {
    let result = step.canonical;
    let command = result
        .host_result
        .as_ref()
        .filter(|host| {
            host.get("kind").and_then(Value::as_str) == Some(COMMAND_RUN_HOST_RESULT_KIND)
        })
        .and_then(|host| host.get("payload"));
    let exit_code = command
        .and_then(|payload| payload.get("exit_code"))
        .and_then(Value::as_i64);
    let command_failed = command.is_some_and(|payload| {
        exit_code != Some(0) || payload.get("status").and_then(Value::as_str) != Some("completed")
    });
    StepExecution::Completed {
        failed: result.is_error || command_failed,
        exit_code,
        streams: step.streams.map(|streams| (streams.stdout, streams.stderr)),
        display: Some(json!({
            "model_output": result.model_output,
            "host_result": result.host_result,
            "is_error": result.is_error,
            "duration_ms": u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        })),
        output: result.model_output,
    }
}
