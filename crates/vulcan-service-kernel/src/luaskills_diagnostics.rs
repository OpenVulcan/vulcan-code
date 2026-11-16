//! Existing native observation APIs exposed to the process-owned service collector.
//! 向进程拥有的服务采集器暴露既有原生观测接口。

use std::{cell::RefCell, collections::BTreeSet};

pub use luaskills::runtime_logging::emit as emit_runtime_log;
pub use luaskills::{RuntimeLogCallback, RuntimeLogEvent, RuntimeLogLevel, set_log_callback};
pub use vulcan_plugin_runtime::{
    VulcanPluginNativeIdentityCallback, VulcanPluginNativeIdentityEvent,
    VulcanPluginNativeIdentitySubscription, install_native_identity_observer,
};

thread_local! {
    // The synchronous bootstrap and LuaSkills 0.9 dependency diagnostics run on the same thread.
    // 同步启动引导和 LuaSkills 0.9 的依赖诊断在同一线程执行。
    static STARTUP_DEPENDENCY_FAILURES: RefCell<Option<BTreeSet<String>>> = const { RefCell::new(None) };
}

/// Clears the bootstrap observation scope if it unwinds before returning a result.
/// 引导在返回结果前展开时，清除引导观察作用域。
struct StartupDependencyCapture;

impl Drop for StartupDependencyCapture {
    fn drop(&mut self) {
        STARTUP_DEPENDENCY_FAILURES.with(|failures| *failures.borrow_mut() = None);
    }
}

/// Runs synchronous bootstrap work and returns its result and the skill IDs that failed dependency preparation.
/// 执行同步引导工作，返回其结果和依赖准备失败的技能标识。
pub(crate) fn capture_startup_dependency_failures<T>(
    bootstrap: impl FnOnce() -> T,
) -> (T, Vec<String>) {
    STARTUP_DEPENDENCY_FAILURES.with(|failures| *failures.borrow_mut() = Some(BTreeSet::new()));
    // Clear the capture even if the startup worker unwinds.
    // 即使启动工作线程展开，也会清除观察状态。
    let _capture = StartupDependencyCapture;
    let result = bootstrap();
    // Only the final catalog attempt controls the user-facing unavailable state.
    // 只有最终目录加载尝试决定面向用户的不可用状态。
    let failures = STARTUP_DEPENDENCY_FAILURES
        .with(|failures| failures.borrow_mut().take().into_iter().flatten().collect());
    (result, failures)
}

/// Starts a fresh observation window for the final load without removing already written diagnostics.
/// 为最终加载开启新观察窗口，保留已写入的诊断。
pub(crate) fn begin_final_startup_skill_load() {
    STARTUP_DEPENDENCY_FAILURES.with(|failures| {
        if let Some(active) = failures.borrow_mut().as_mut() {
            active.clear();
        }
    });
}

/// Projects the exact LuaSkills 0.9 dependency-preparation error into a safe boot diagnostic.
/// 将 LuaSkills 0.9 的精确依赖准备错误投影为安全的启动诊断。
pub fn observe_startup_dependency_failure(event: &RuntimeLogEvent) {
    if event.level != RuntimeLogLevel::Error {
        return;
    }
    // LuaSkills 0.9 exposes this failure only through its log callback. Never forward the error body.
    // LuaSkills 0.9 仅通过日志回调暴露此失败。错误正文不进入启动诊断。
    let Some((skill_id, _)) = event
        .message
        .strip_prefix("[LuaSkill] Failed to prepare dependencies for ")
        .and_then(|message| message.split_once(": "))
    else {
        return;
    };
    if luaskills::lua_skill::validate_luaskills_identifier(skill_id, "skill_id").is_err() {
        return;
    }
    // A failure is accepted only within the bootstrap that owns this LuaSkills load.
    // 仅接收来自拥有本次 LuaSkills 加载的引导作用域的失败。
    let observed = STARTUP_DEPENDENCY_FAILURES.with(|failures| {
        failures
            .borrow_mut()
            .as_mut()
            .is_some_and(|active| active.insert(skill_id.to_owned()))
    });
    if observed {
        tracing::warn!(
            skill_id,
            stage = "luaskills_dependency_preparation",
            code = "skill_dependency_preparation_failed",
            "LuaSkills could not prepare required skill dependencies during startup"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Captures only dependency errors from the final load and never retains the upstream error body.
    /// 只采集最终加载的依赖错误，且不保留上游错误正文。
    #[test]
    fn startup_capture_distinguishes_dependency_failures_from_package_failures() {
        let (_, failures) = capture_startup_dependency_failures(|| {
            for message in [
                "[LuaSkill] Failed to prepare dependencies for recovered: early failure",
                "Failed to install the ROOT default skill `unavailable`: archive request failed",
            ] {
                observe_startup_dependency_failure(&RuntimeLogEvent {
                    level: RuntimeLogLevel::Error,
                    message: message.into(),
                });
            }
            begin_final_startup_skill_load();
            for message in [
                "[LuaSkill] Failed to prepare dependencies for vulcan-codekit: failed with private diagnostic",
                "[LuaSkill] Failed to prepare dependencies for vulcan-codekit: repeated diagnostic",
                "[LuaSkill] Failed to prepare dependencies for invalid skill: invalid identity",
            ] {
                observe_startup_dependency_failure(&RuntimeLogEvent {
                    level: RuntimeLogLevel::Error,
                    message: message.into(),
                });
            }
        });
        assert_eq!(failures, ["vulcan-codekit"]);
        assert!(capture_startup_dependency_failures(|| ()).1.is_empty());
    }
}
