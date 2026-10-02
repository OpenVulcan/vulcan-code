//! Required Lua declarations and version-one plugin result decoding.
//! 必需 Lua 声明及版本一插件结果解码。

use super::VulcanPluginRuntimeError;
use crate::{VulcanPluginLuaRuntimeManifest, VulcanPluginManifest};
use serde_json::Value;

/// Resolves the Lua protocol required by an execution path that cannot operate without it.
/// 解析无法脱离 Lua 协议运行的执行路径所要求的 Lua 协议。
///
/// # Parameters
/// 参数
/// `manifest` is the already validated plugin manifest captured by the current generation.
/// `manifest` 是当前代次捕获且已经校验的插件清单。
///
/// # Returns
/// 返回值
/// Returns the exact Lua declaration or an explicit invariant error for a non-Lua plugin.
/// 返回精确 Lua 声明；若插件不含 Lua 协议则返回显式不变量错误。
pub(super) fn required_lua_runtime(
    manifest: &VulcanPluginManifest,
) -> Result<&VulcanPluginLuaRuntimeManifest, VulcanPluginRuntimeError> {
    manifest.lua_runtime.as_ref().ok_or_else(|| {
        VulcanPluginRuntimeError::new(format!(
            "Vulcan plugin `{}` does not declare a Lua runtime.",
            manifest.id
        ))
    })
}

/// Decode the borrowed original version-one exported JSON frame without an intermediate host wrapper.
/// 借用原版本一导出 JSON 帧直接解码，不创建中间宿主封装。
/// frame is the exact Core result; return its business subtree or the unchanged protocol/plugin error.
/// frame 是精确核心结果；返回业务子树或原协议及插件错误。
pub(super) fn parse_plugin_result(frame: &Value) -> Result<Value, VulcanPluginRuntimeError> {
    let frame = frame.as_str().ok_or_else(|| {
        VulcanPluginRuntimeError::new("Vulcan Plugin runtime eval returned no JSON frame.")
    })?;
    // Own the parsed frame so its business subtree can be transferred without a deep copy.
    // 持有解析后的帧，以便直接转移业务子树，避免深复制。
    let mut plugin: Value = serde_json::from_str(frame).map_err(|error| {
        VulcanPluginRuntimeError::new(format!(
            "Failed to decode Vulcan Plugin JSON frame: {error}"
        ))
    })?;
    if plugin.get("ok").and_then(Value::as_bool) != Some(true) {
        let message = plugin
            .get("error")
            .map(render_error_value)
            .unwrap_or_else(|| String::from("Unknown Vulcan Plugin runtime error."));
        return Err(VulcanPluginRuntimeError::new(format!(
            "Vulcan Plugin request failed: {message}"
        )));
    }
    Ok(plugin
        .get_mut("result")
        .map(Value::take)
        .unwrap_or(Value::Null))
}

/// Renders one structured or scalar runtime error without losing upstream detail.
/// 渲染一个结构化或标量运行时错误，同时不丢失上游细节。
/// Converts the supplied `error` JSON value into its original string or serialized JSON text.
/// 将传入的`error`JSON值转换为原字符串或序列化JSON文本。
fn render_error_value(error: &Value) -> String {
    error
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| error.to_string())
}
