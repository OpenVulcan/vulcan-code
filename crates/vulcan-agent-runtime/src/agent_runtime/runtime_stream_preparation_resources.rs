//! Runtime stream preparation resource-bundle helpers for foundation construction and launch-input ownership.
//! 面向基础运行时构建与启动输入所有权封装的运行时流准备资源辅助模块。

use crate::agent_profile::AgentProfileCatalog;
use crate::agent_runtime::ExtensionGeneration;
use crate::agent_runtime::execution_models::ExecutionPreparationRequest;
use crate::agent_runtime::question_runtime::QuestionRuntime;
use crate::agent_runtime::runtime::RuntimeFoundation;
use crate::agent_runtime::runtime_stream_bootstrap_core::{
    after_precheck_trace, after_precheck_unix_millis,
};
use crate::agent_runtime::task_runtime::TaskRuntime;
use crate::luaskills_runtime::LuaSkillsRuntimeHandle;
use crate::provider_config::ProviderConfigurationFile;
use crate::tooling::CommandSessionRuntime;
use std::sync::Arc;

/// Resource bundle captured before execution preparation begins.
/// 在执行准备开始前捕获的资源打包对象。
pub(crate) struct RuntimePreparationResources {
    /// Runtime foundation built from the provider config and active agent profile catalog.
    /// 根据 Provider 配置与当前代理画像目录构建出的运行时基础架构。
    pub foundation: RuntimeFoundation,
    /// Optional LuaSkills runtime handle that remains available for later bootstrap steps.
    /// 供后续 bootstrap 步骤继续使用的可选 LuaSkills 运行时句柄。
    pub luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    /// Original execution request owned by the current runtime launch.
    /// 归属于当前运行时启动流程的原始执行请求。
    pub request: ExecutionPreparationRequest,
}

/// Builds one resource bundle that owns the runtime foundation, runtime handles, and execution request for one launch attempt.
/// 为一次启动尝试构建一份持有运行时基础架构、运行时句柄与执行请求的资源打包对象。
pub(crate) fn build_runtime_preparation_resources(
    config: &ProviderConfigurationFile,
    agent_profile_catalog: &AgentProfileCatalog,
    luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    vulcan_plugin_runtime: Option<vulcan_plugin_runtime::VulcanPluginRuntimeManager>,
    extension_generation: Arc<ExtensionGeneration>,
    task_runtime: Arc<TaskRuntime>,
    question_runtime: Arc<QuestionRuntime>,
    command_session_runtime: Arc<CommandSessionRuntime>,
    public_ai_runtime: Option<Arc<vulcan_provider_adapters::PublicAiRuntime>>,
    request: ExecutionPreparationRequest,
) -> RuntimePreparationResources {
    // foundationStartedAt measures one complete per-turn runtime-foundation rebuild.
    // foundationStartedAt 测量一次完整的每回合运行时基础设施重建。
    let foundation_started_at = after_precheck_unix_millis();
    after_precheck_trace(
        "after_precheck_24a1_foundation_build_enter",
        format!("session={}", request.session_id),
    );
    let mut foundation =
        RuntimeFoundation::build_from_extension_generation_with_runtimes_and_plugins(
            config,
            agent_profile_catalog,
            luaskills_runtime.clone(),
            vulcan_plugin_runtime,
            extension_generation,
            task_runtime,
            question_runtime,
            command_session_runtime,
        );
    after_precheck_trace(
        "after_precheck_24a9_foundation_build_ready",
        format!(
            "session={} elapsed_ms={}",
            request.session_id,
            after_precheck_unix_millis().saturating_sub(foundation_started_at)
        ),
    );
    if let Some(public_ai_runtime) = public_ai_runtime {
        foundation.install_public_ai_runtime(public_ai_runtime);
    }
    after_precheck_trace(
        "after_precheck_24b_runtime_resources_ready",
        format!("session={}", request.session_id),
    );

    RuntimePreparationResources {
        foundation,
        luaskills_runtime,
        request,
    }
}
