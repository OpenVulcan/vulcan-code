//! Explicit per-tool reuse, hygiene and ordering declarations for production pools.
//! 面向生产池的显式逐工具复用、清理及顺序声明。

use super::*;
use serde::Serialize;

/// Exact ASCII identifier shape used by the production Lua callback validator.
/// 生产 Lua 回调校验器采用的精确 ASCII 标识符形状。
#[cfg(feature = "manifest-schema")]
const LUA_CALLBACK_IDENTIFIER_PATTERN: &str = "^[A-Za-z_][A-Za-z0-9_]*$";

/// Ordering inside one exact controller, workspace, plugin generation and tool partition.
/// 单个精确控制器、工作区、插件代次及工具分区内的顺序。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "manifest-schema", derive(schemars::JsonSchema))]
pub enum VulcanPluginToolOrdering {
    /// Serialize calls in admission order inside this exact partition.
    /// 在此精确分区内按入场顺序串行化调用。
    KeySerial,
    /// Permit concurrent independent instances within the host-approved resident and running limits.
    /// 在宿主批准的常驻及执行上限内允许独立实例并行。
    Parallel,
}

/// Opt-in module contract; callbacks must clear request state and confirm reuse before another request runs.
/// 主动声明的模块契约；回调必须清除请求状态，并在其他请求运行前确认可复用。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "manifest-schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "manifest-schema", schemars(extend("allOf" = [{
    "if": {"required": ["ordering"], "properties": {"ordering": {"const": VulcanPluginToolOrdering::KeySerial}}},
    "then": {"properties": {"max_running_calls": {"const": 1}}}
}])))]
pub struct VulcanPluginReusableToolManifest {
    /// Declared module function called with this invocation's envelope before business; must return true.
    /// 在业务前使用本次调用信封调用的模块声明函数；必须返回 true。
    #[cfg_attr(
        feature = "manifest-schema",
        schemars(pattern(LUA_CALLBACK_IDENTIFIER_PATTERN))
    )]
    pub before_call: String,
    /// Declared module function called with the envelope and business success flag; must return true.
    /// 使用信封及业务执行成功标志调用的模块声明函数；必须返回 true。
    #[cfg_attr(
        feature = "manifest-schema",
        schemars(pattern(LUA_CALLBACK_IDENTIFIER_PATTERN))
    )]
    pub after_call: String,
    /// Declared argument-free module function called after cleanup; only true permits reuse.
    /// 清理后调用的无参数模块声明函数；仅 true 允许复用。
    #[cfg_attr(
        feature = "manifest-schema",
        schemars(pattern(LUA_CALLBACK_IDENTIFIER_PATTERN))
    )]
    pub reuse_check: String,
    /// Explicit ordering applies to the same immutable partition, never all plugin conversations implicitly.
    /// 显式顺序作用于同一不可变分区，绝不隐式作用于插件全部会话。
    pub ordering: VulcanPluginToolOrdering,
    /// Requested positive per-domain resident ceiling, required to fit the host purpose policy.
    /// 请求的正数域常驻上限，必须满足宿主用途策略。
    #[cfg_attr(feature = "manifest-schema", schemars(with = "std::num::NonZeroUsize"))]
    pub max_resident_vms: usize,
    /// Requested positive per-domain running ceiling; key_serial requires exactly one.
    /// 请求的正数域执行上限；key_serial 要求恰好为一。
    #[cfg_attr(feature = "manifest-schema", schemars(with = "std::num::NonZeroUsize"))]
    pub max_running_calls: usize,
    /// Positive idle duration before retirement; pool metadata is retired after its users leave too.
    /// 退役前的正数空闲时长；用户离开后池元数据也会退役。
    #[cfg_attr(feature = "manifest-schema", schemars(with = "std::num::NonZeroU64"))]
    pub idle_ttl_ms: u64,
    /// Positive successful invocation count before an instance is retired.
    /// 实例退役前允许的正数成功调用次数。
    #[cfg_attr(feature = "manifest-schema", schemars(with = "std::num::NonZeroU64"))]
    pub max_uses: u64,
}

/// Decode a present reuse contract; return Some and reject null rather than treating it as omission.
/// 解码已出现复用契约；返回 Some，并拒绝空值，不将其视为省略。
pub(super) fn deserialize_present_reuse<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<VulcanPluginReusableToolManifest>, D::Error> {
    VulcanPluginReusableToolManifest::deserialize(deserializer).map(Some)
}
