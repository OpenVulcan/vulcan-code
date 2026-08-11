//! Bounded-stack codec boundary for the large persisted session runtime state.
//! 大型持久化会话运行态的有界栈编解码边界。

use crate::model::SessionRuntimeStateEntry;
use serde_json::Value;

/// Stack reservation used only while one runtime-state codec operation is active.
/// 仅在单次运行态编解码操作期间使用的栈预留字节数。
const SESSION_RUNTIME_STATE_CODEC_STACK_BYTES: usize = 8 * 1024 * 1024;

/// Stable worker name used by runtime-state codec diagnostics.
/// 运行态编解码诊断使用的稳定工作线程名称。
const SESSION_RUNTIME_STATE_CODEC_THREAD_NAME: &str = "vulcan-session-state-codec";

/// Deserializes one persisted runtime-state value outside the caller's Tokio worker stack.
/// 在调用方 Tokio 工作线程栈之外反序列化一份持久化运行态值。
///
/// # Parameters
/// 参数
/// `value` is the already size-bounded JSON value loaded from the segmented state document.
/// `value` 是从分段状态文档载入且已受大小限制的 JSON 值。
/// `error_prefix` scopes parse, worker creation, and worker termination diagnostics.
/// `error_prefix` 限定解析、工作线程创建与工作线程终止诊断范围。
///
/// # Returns
/// 返回值
/// The typed runtime state, or a controlled codec-boundary error.
/// 返回类型化运行态，或受控的编解码边界错误。
pub(in crate::workspace_store) fn deserialize_session_runtime_state_entry(
    value: Value,
    error_prefix: impl Into<String>,
) -> Result<SessionRuntimeStateEntry, String> {
    run_session_runtime_state_codec(error_prefix.into(), move || {
        serde_json::from_value::<SessionRuntimeStateEntry>(value)
    })
}

/// Serializes one persisted runtime state outside the caller's Tokio worker stack.
/// 在调用方 Tokio 工作线程栈之外序列化一份持久化运行态。
///
/// # Parameters
/// 参数
/// `runtime_state` is the exact state borrowed for the duration of the scoped worker.
/// `runtime_state` 是在作用域工作线程期间被精确借用的运行态。
/// `error_prefix` scopes serialization, worker creation, and worker termination diagnostics.
/// `error_prefix` 限定序列化、工作线程创建与工作线程终止诊断范围。
///
/// # Returns
/// 返回值
/// The complete JSON value, or a controlled codec-boundary error.
/// 返回完整 JSON 值，或受控的编解码边界错误。
pub(in crate::workspace_store) fn serialize_session_runtime_state_entry(
    runtime_state: &SessionRuntimeStateEntry,
    error_prefix: impl Into<String>,
) -> Result<Value, String> {
    run_session_runtime_state_codec(error_prefix.into(), || serde_json::to_value(runtime_state))
}

/// Runs one serde operation on a dedicated scoped thread with a stable stack reservation.
/// 在具有稳定栈预留的专用作用域线程上运行一次 serde 操作。
///
/// # Parameters
/// 参数
/// `error_prefix` identifies the owning storage operation in every controlled failure.
/// `error_prefix` 在每个受控失败中标识所属存储操作。
/// `operation` performs exactly one runtime-state serialization or deserialization.
/// `operation` 精确执行一次运行态序列化或反序列化。
///
/// # Returns
/// 返回值
/// The serde result after joining the scoped worker, or a deterministic boundary error.
/// 返回加入作用域工作线程后的 serde 结果，或确定性的边界错误。
fn run_session_runtime_state_codec<T, F>(error_prefix: String, operation: F) -> Result<T, String>
where
    T: Send,
    F: FnOnce() -> Result<T, serde_json::Error> + Send,
{
    std::thread::scope(|scope| {
        let worker = std::thread::Builder::new()
            .name(String::from(SESSION_RUNTIME_STATE_CODEC_THREAD_NAME))
            .stack_size(SESSION_RUNTIME_STATE_CODEC_STACK_BYTES)
            .spawn_scoped(scope, operation)
            .map_err(|error| format!("{error_prefix}无法创建运行态编解码工作线程：{error}"))?;
        let result = worker
            .join()
            .map_err(|_| format!("{error_prefix}运行态编解码工作线程异常终止。"))?;
        result.map_err(|error| format!("{error_prefix}{error}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the codec remains independent of a deliberately small caller stack.
    /// 验证编解码过程不依赖刻意缩小的调用方栈。
    #[test]
    fn runtime_state_codec_uses_dedicated_stack_boundary() {
        let caller = std::thread::Builder::new()
            .name(String::from("runtime-state-codec-small-caller"))
            .stack_size(512 * 1024)
            .spawn(|| {
                let expected = SessionRuntimeStateEntry::default();
                let expected_value = serialize_session_runtime_state_entry(
                    &expected,
                    "serialize runtime-state fixture failed: ",
                )?;
                let actual = deserialize_session_runtime_state_entry(
                    expected_value.clone(),
                    "deserialize runtime-state fixture failed: ",
                )?;
                let actual_value = serialize_session_runtime_state_entry(
                    &actual,
                    "reserialize runtime-state fixture failed: ",
                )?;
                if actual_value != expected_value {
                    return Err(String::from(
                        "runtime-state codec round trip changed the fixture",
                    ));
                }
                Ok::<(), String>(())
            })
            .expect("small caller thread should start");

        caller
            .join()
            .expect("small caller thread should not panic")
            .expect("dedicated codec boundary should complete");
    }
}
