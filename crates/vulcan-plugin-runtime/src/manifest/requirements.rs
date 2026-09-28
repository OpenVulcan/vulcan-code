//! Versioned Lua runtime requirements checked before activation, without granting resources.
//! 在激活前校验的版本化 Lua 运行时必需条件，不授予资源。

use super::*;
use crate::VulcanPluginCapacityPurpose;
use luaskills::ffi_embedded::embedded_core_description;
use luaskills::runtime::embedded::ExecutionBackend;
use luaskills::runtime::embedded::capabilities::{CapabilityExecution, CapabilityScope};

/// Required compatibility and capacity floors; host policy must satisfy these without being rewritten.
/// 必需的兼容性及容量下限；宿主策略必须满足这些条件，且不能被自动改写。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "manifest-schema", derive(schemars::JsonSchema))]
pub struct VulcanPluginRuntimeRequirements {
    /// Exact embedded core protocol identity, independent from the package manifest version.
    /// 精确嵌入式核心协议身份，独立于插件包清单版本。
    #[cfg_attr(feature = "manifest-schema", schemars(with = "std::num::NonZeroU32", range(max = u32::MAX)))]
    pub protocol_version: u32,
    /// Required execution backend; unsupported backends are rejected without substitution.
    /// 必需执行后端；不支持的后端直接拒绝，不替代。
    pub backend: ExecutionBackend,
    /// Exact versioned semantic feature names advertised by the linked core, never permission grants.
    /// 链接核心声明的精确版本化语义功能名，绝不表示权限授予。
    #[cfg_attr(feature = "manifest-schema", schemars(inner(length(min = 1)), extend("uniqueItems" = true)))]
    pub core_capabilities: Vec<String>,
    /// Exact registered host contracts required for business admission; omission declares no callback requirement.
    /// 业务入场所需的精确已注册宿主契约；省略表示不声明回调要求。
    #[serde(default)]
    #[cfg_attr(feature = "manifest-schema", schemars(extend("uniqueItems" = true)))]
    pub host_capabilities: Vec<VulcanPluginHostCapabilityRequirement>,
    /// Distinct purpose requirements; an empty list explicitly requests no additional resource floors.
    /// 不重复的用途要求；空列表明确表示不要求额外资源下限。
    pub capacities: Vec<VulcanPluginCapacityRequirement>,
}

/// One required registered host contract, independent from permission grants or external service readiness.
/// 单个必需的已注册宿主契约，独立于权限授予或外部服务就绪状态。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "manifest-schema", derive(schemars::JsonSchema))]
pub struct VulcanPluginHostCapabilityRequirement {
    /// Exact namespaced callback name; aliases and alternate names are never probed.
    /// 精确命名空间回调名称；绝不探测别名或备用名称。
    #[cfg_attr(feature = "manifest-schema", schemars(length(min = 1)))]
    pub name: String,
    /// Exact semantic interface version; a range or a newer version does not implicitly match.
    /// 精确语义接口版本；版本范围或更新版本不隐式匹配。
    #[cfg_attr(feature = "manifest-schema", schemars(length(min = 1)))]
    pub version: String,
    /// Declared core dispatch transport, checked against the actual captured descriptor.
    /// 声明的核心分发传输，针对实际捕获描述符校验。
    pub execution: CapabilityExecution,
    /// Required trusted caller scope, independently enforced by core invocation admission.
    /// 必需的可信调用方作用域，由核心调用入场独立执行。
    pub scope: CapabilityScope,
}

/// Minimum host policy needed by one execution purpose, separate from current free capacity.
/// 单个执行用途所需的最小宿主策略，独立于当前空闲容量。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "manifest-schema", derive(schemars::JsonSchema))]
pub struct VulcanPluginCapacityRequirement {
    /// Exact production purpose whose frozen configuration must satisfy these floors.
    /// 冻结配置必须满足这些下限的精确生产用途。
    pub purpose: VulcanPluginCapacityPurpose,
    /// Required non-lendable guarantee; zero requires no reservation or prewarming.
    /// 必需的不可出借保证；零表示不要求预留或预热。
    pub reserved_resident_vms: usize,
    /// Minimum acceptable resident limit, positive and not a request to allocate instances.
    /// 可接受的最小常驻上限，必须为正，且不表示申请分配实例。
    #[cfg_attr(feature = "manifest-schema", schemars(with = "std::num::NonZeroUsize"))]
    pub resident_vms: usize,
    /// Minimum acceptable running-call limit, positive and bounded by resident_vms.
    /// 可接受的最小执行调用上限，必须为正且不超过 resident_vms。
    #[cfg_attr(feature = "manifest-schema", schemars(with = "std::num::NonZeroUsize"))]
    pub running_calls: usize,
    /// Minimum acceptable pending-call limit, positive and never interpreted as unlimited.
    /// 可接受的最小等待调用上限，必须为正，绝不解释为无限。
    #[cfg_attr(feature = "manifest-schema", schemars(with = "std::num::NonZeroUsize"))]
    pub queued_calls: usize,
    /// Minimum acceptable pending-input byte limit, positive and never interpreted as unlimited.
    /// 可接受的最小等待输入字节上限，必须为正，绝不解释为无限。
    #[cfg_attr(feature = "manifest-schema", schemars(with = "std::num::NonZeroUsize"))]
    pub queued_bytes: usize,
}

/// Decode a present field using its concrete type; return Some or reject null and malformed values.
/// 按具体类型解码已出现字段；返回 Some，或拒绝空值及畸形值。
/// The parent default handles omission, so explicit null cannot bypass version-one field rejection.
/// 父类型默认值处理省略，因此显式空值不能绕过第一版字段拒绝。
pub(super) fn deserialize_present_requirements<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<VulcanPluginRuntimeRequirements>, D::Error> {
    VulcanPluginRuntimeRequirements::deserialize(deserializer).map(Some)
}

impl VulcanPluginManifest {
    /// Validate this manifest's exact version and runtime requirements against the linked core.
    /// 针对链接核心校验此清单的精确版本及运行时必需条件。
    /// Return an English diagnostic before package publication; no Lua or resource allocation occurs.
    /// 在插件包发布前返回英文诊断；不执行 Lua 或分配资源。
    pub(crate) fn validate_runtime_requirements(&self) -> Result<(), String> {
        match self.schema_version {
            VULCAN_PLUGIN_LEGACY_MANIFEST_SCHEMA_VERSION => {
                if self.runtime_requirements.is_some() {
                    return Err("Manifest version one does not permit runtime_requirements.".into());
                }
                return Ok(());
            }
            VULCAN_PLUGIN_MANIFEST_SCHEMA_VERSION => {}
            _ => {
                return Err(format!(
                    "Unsupported Vulcan Plugin manifest schema version `{}`.",
                    self.schema_version
                ));
            }
        }
        // Version two binds requirements only to the existing Lua protocol, never MCP or prompt content.
        // 第二版仅将必需条件绑定至既有 Lua 协议，不绑定 MCP 或提示内容。
        let requirements = match (&self.lua_runtime, &self.runtime_requirements) {
            (Some(_), Some(requirements)) => requirements,
            (None, None) => return Ok(()),
            (Some(_), None) => {
                return Err(
                    "Manifest version two requires runtime_requirements for Lua plugins.".into(),
                );
            }
            (None, Some(_)) => return Err("runtime_requirements requires lua_runtime.".into()),
        };
        // The core descriptor owns core support; the host separately owns its actual route backend.
        // 核心描述拥有核心支持的权威；宿主独立拥有真实路由后端的权威。
        let core = embedded_core_description();
        if requirements.protocol_version != core.protocol_version {
            return Err(format!(
                "Required embedded protocol {} is unavailable; linked core provides {}.",
                requirements.protocol_version, core.protocol_version
            ));
        }
        if !core.execution_backends.contains(&requirements.backend)
            || requirements.backend != crate::runtime::VULCAN_PLUGIN_EXECUTION_BACKEND
        {
            return Err(format!(
                "Required execution backend {:?} is unavailable.",
                requirements.backend
            ));
        }
        // Distinct exact identifiers prevent a duplicate from hiding an incompatible requirement.
        // 不重复的精确标识避免重复项掩盖不兼容要求。
        let mut capabilities = BTreeSet::new();
        for name in &requirements.core_capabilities {
            if !capabilities.insert(name) {
                return Err(format!("Duplicate required core capability `{name}`."));
            }
            if !core.capabilities.contains(&name.as_str()) {
                return Err(format!("Required core capability `{name}` is unavailable."));
            }
        }
        // Callback identities are requirements only; this structural check cannot grant any authority.
        // 回调身份仅是必需条件；此结构检查不能授予任何权威。
        let mut host_capabilities = BTreeSet::new();
        for required in &requirements.host_capabilities {
            if required.name.trim().is_empty() || required.name.contains('\0') {
                return Err("Required host capability name must be nonempty without NUL.".into());
            }
            if !host_capabilities.insert(&required.name) {
                return Err(format!(
                    "Duplicate required host capability `{}`.",
                    required.name
                ));
            }
            semver::Version::parse(&required.version).map_err(|_| {
                format!(
                    "Required host capability `{}` must declare an exact semantic version.",
                    required.name
                )
            })?;
        }
        // Purpose identities have the same type as the actual production routing and host policy.
        // 用途身份与真实生产路由及宿主策略使用同一类型。
        let mut purposes = BTreeSet::new();
        for capacity in &requirements.capacities {
            if !purposes.insert(capacity.purpose) {
                return Err(format!(
                    "Duplicate runtime capacity requirement {:?}.",
                    capacity.purpose
                ));
            }
            if capacity.resident_vms == 0
                || capacity.running_calls == 0
                || capacity.queued_calls == 0
                || capacity.queued_bytes == 0
                || capacity.reserved_resident_vms > capacity.resident_vms
                || capacity.running_calls > capacity.resident_vms
            {
                return Err(format!(
                    "Invalid runtime capacity requirement {:?}: positive limits must cover running calls and reserved residents.",
                    capacity.purpose
                ));
            }
        }
        Ok(())
    }
}
