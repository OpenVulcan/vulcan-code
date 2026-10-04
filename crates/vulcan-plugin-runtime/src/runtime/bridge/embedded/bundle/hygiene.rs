//! Trusted per-call hygiene wrapper around captured original module functions.
//! 捕获的原始模块函数周围的可信逐次调用清理包装。

use super::*;

/// Build trusted loader source from encoded arguments and optional declared hygiene; return executable Lua.
/// 使用已编码参数及可选声明清理构造可信加载器源码；返回可执行 Lua。
/// Callback names remain quoted data, and a failed callback raises an error so the core retires the instance.
/// 回调名称保持为引用数据，回调失败抛出错误，使核心退役实例。
pub(super) fn bundle_source_with_hygiene(
    bridge_source: &str,
    load_arguments: &str,
    invoke: &str,
    reusable: Option<&crate::VulcanPluginReusableToolManifest>,
) -> String {
    // The original loader captures functions before untrusted module globals can change them.
    // 原加载器在不可信模块全局变量可能改变函数前捕获函数。
    let mut source = format!(
        "local bridge = (function()\n{bridge_source}\nend)()\nlocal exports = bridge.load_module(vulcan.json.decode({}))\n",
        lua_string_literal(load_arguments)
    );
    if let Some(reuse) = reusable {
        // Every callback name crosses the wrapper boundary as encoded data.
        // 每个回调名称以编码数据跨越包装边界。
        let names = json!([
            invoke,
            reuse.before_call,
            reuse.after_call,
            reuse.reuse_check
        ])
        .to_string();
        source.push_str(&format!(
            "local names = vulcan.json.decode({})\n",
            lua_string_literal(&names)
        ));
        source.push_str(r#"
-- Capture each callback once; subsequent module mutations cannot bypass hygiene.
-- 仅捕获每个回调一次；后续模块变更不能绕过清理。
local invoke, before, after, check = exports[names[1]], exports[names[2]], exports[names[3]], exports[names[4]]
-- Wrap one request envelope and return its original frame after mandatory cleanup.
-- 包装单个请求信封，在强制清理后返回其原始帧。
exports[names[1]] = function(envelope)
    -- A failed preparation poisons this instance; core retirement still owns shutdown.
    -- 准备失败使此实例失效；核心退役仍拥有关闭。
    if before(envelope) ~= true then error("Reusable tool before_call must return true.") end
    -- Preserve the original business failure while always attempting declared cleanup.
    -- 始终尝试声明清理，同时保留原始业务失败。
    local ok, result = pcall(invoke, envelope)
    -- Success means no Lua exception; application-level ok=false frames remain business results.
    -- 成功表示没有 Lua 异常；应用层 ok=false 帧仍为业务结果。
    local cleaned, cleanup = pcall(after, envelope, ok)
    if not cleaned or cleanup ~= true then
        error("Reusable tool after_call failed: " .. tostring(cleanup) .. (ok and "" or "; business: " .. tostring(result)))
    end
    -- The final affirmative check permits core reuse only after declared request state is cleared.
    -- 仅在声明请求状态已清理后，最终肯定检查才允许核心复用。
    if check() ~= true then error("Reusable tool reuse_check must return true.") end
    if not ok then error(result) end
    return result
end
-- Hygiene callbacks are private and cannot be submitted as ordinary exports.
-- 清理回调为私有函数，不能作为普通导出提交。
exports[names[2]], exports[names[3]], exports[names[4]] = nil, nil, nil
"#);
    }
    source.push_str("return exports");
    source
}
