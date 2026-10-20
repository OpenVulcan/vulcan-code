use super::*;
use std::collections::BTreeMap;

/// Re-export the Vulcan-owned LuaSkills runtime settings wire contract.
/// 重新导出 Vulcan 自有 LuaSkills 运行时设置传输契约。
pub use vulcan_luaskills_config::LuaSkillsRuntimeSettings;

/// WebUI-visible result of the latest managed Python and Node bootstrap attempt.
/// WebUI 可见的最近一次受管 Python 与 Node 初始化尝试结果。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LuaSkillsManagedRuntimeBootstrapSummary {
    /// Stable status key: ready, installed, unsupported, or unavailable.
    /// 稳定状态键：ready、installed、unsupported 或 unavailable。
    pub status: String,
    /// Native platform key evaluated by the bootstrapper when available.
    /// 初始化器在可用时判定的原生平台键。
    pub platform: Option<String>,
    /// Whether the native installer attempted to update the runtime layout.
    /// 原生安装器是否尝试更新运行时目录布局。
    pub install_attempted: bool,
    /// Bounded diagnostic for the completed attempt.
    /// 当前已完成尝试的有界诊断信息。
    pub diagnostic: String,
}

/// WebUI-visible state of one independently repairable managed runtime.
/// WebUI 可见的单个可独立修复受管运行时状态。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LuaSkillsManagedRuntimeComponentSummary {
    /// Exact packaged runtime version.
    /// 随包提供的精确运行时版本。
    pub version: String,
    /// Stable status key: ready, installed, unsupported, or unavailable.
    /// 稳定状态键：ready、installed、unsupported 或 unavailable。
    pub status: String,
    /// Exact retained validation failure for diagnostics.
    /// 为诊断保留的精确校验失败信息。
    pub diagnostic: String,
}

/// WebUI-visible native managed-runtime installation progress.
/// WebUI 可见的原生受管运行时安装进度。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LuaSkillsManagedRuntimeProgressSummary {
    /// Stable native installation phase key.
    /// 稳定的原生安装阶段键。
    pub phase: String,
    /// Runtime component currently being processed.
    /// 当前正在处理的运行时组件。
    pub component: Option<String>,
    /// Human-readable progress message.
    /// 可读的进度消息。
    pub message: String,
    /// Bytes transferred for the active download.
    /// 当前下载已传输的字节数。
    pub completed_bytes: Option<u64>,
    /// Total bytes advertised for the active download.
    /// 当前下载声明的总字节数。
    pub total_bytes: Option<u64>,
    /// Overall installation completion percentage.
    /// 整体安装完成百分比。
    pub overall_percent: Option<u8>,
}

/// WebUI-visible progress of one LuaSkills install or update operation.
/// WebUI 可见的单次 LuaSkills 安装或更新操作进度。
///
/// The service generates the correlation id before issuing the request, so a WebUI
/// refresh or reconnect resumes the operation by that stable id instead of
/// re-deriving state from catalog contents or timestamps.
/// 服务在发起请求前生成关联标识，因此 WebUI 刷新或重连用该稳定标识恢复操作，而不是从目录
/// 内容或时间戳重新推断状态。
///
/// Free-form upstream text is replaced by a finite host-owned vocabulary when the event
/// is retained, and `source_locator` is deliberately absent: either can carry a private
/// source URL, a local absolute directory, or an embedded token.
/// 事件保留时会把上游自由文本替换为有限的宿主自有词表，且刻意不含 `source_locator`：
/// 二者都可能携带私有来源 URL、本地绝对目录或内嵌令牌。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LuaSkillsOperationProgressSummary {
    /// Service-generated correlation identity for this exact request.
    /// 为本次精确请求生成的服务关联身份。
    pub correlation_id: String,
    /// Stable lifecycle action label: install, update, reload, uninstall, enable, or disable.
    /// 稳定生命周期动作标签：install、update、reload、uninstall、enable 或 disable。
    pub action: String,
    /// Whether the operation is still inside its bound span.
    /// 该操作是否仍在其绑定区间内。
    pub in_flight: bool,
    /// Authoritative request outcome: succeeded or failed, once the request returned.
    /// 请求返回后的权威请求结果：succeeded 或 failed。
    pub outcome: Option<String>,
    /// Whether the retained event queue overflowed and dropped events.
    /// 保留事件队列是否溢出并丢弃过事件。
    pub overflowed: bool,
    /// Whether an upstream terminal event was observed.
    /// 是否观察到上游终态事件。
    pub saw_upstream_terminal: bool,
    /// Number of retained events.
    /// 保留的事件数。
    pub phases: usize,
    /// Most recent machine-readable phase name.
    /// 最近的机器可读阶段名称。
    pub latest_phase: Option<String>,
    /// Most recent machine-readable status.
    /// 最近的机器可读状态。
    pub latest_status: Option<String>,
    /// Target skill package identifier when the event carries one.
    /// 事件携带时的目标技能包标识。
    pub skill_id: Option<String>,
    /// Target named skill root when the event carries one.
    /// 事件携带时的目标命名技能根。
    pub root_name: Option<String>,
    /// Completed byte count for download phases.
    /// 下载阶段的已完成字节数。
    pub bytes_done: Option<u64>,
    /// Total byte count for download phases.
    /// 下载阶段的总字节数。
    pub bytes_total: Option<u64>,
    /// Whole-number completion percentage for download phases.
    /// 下载阶段的整数完成百分比。
    pub percent: Option<u8>,
    /// Sanitized human-readable progress message.
    /// 脱敏后的人类可读进度消息。
    pub message: Option<String>,
    /// Upstream operation identity bound by the first observed event.
    /// 由首个观察到的事件绑定的上游操作身份。
    ///
    /// Exposed so a caller can detect an operation whose events came from a source
    /// the management lock did not serialize.
    /// 暴露该值，使调用方能够察觉事件来自管理锁未能串行化的来源的操作。
    pub upstream_operation_id: Option<String>,
}

/// Full WebUI summary for the embedded Lua Skills catalog.
/// 嵌入式 Lua Skills 目录的完整 WebUI 摘要。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LuaSkillsCatalogSummary {
    /// Runtime root used by the embedded Lua Skills host.
    /// 嵌入式 Lua Skills 宿主使用的运行时根目录。
    pub runtime_root: String,
    /// Host-owned shared root containing managed Python and Node distributions.
    /// 宿主持有的共享根目录，包含受管 Python 与 Node 发行包。
    pub managed_runtime_distribution_root: String,
    /// LuaSkills-owned writable root containing reusable managed environments.
    /// LuaSkills 持有的可写根目录，包含可复用受管环境。
    pub managed_runtime_environment_root: String,
    /// Latest managed Python and Node bootstrap state and retained diagnostics.
    /// 最近一次受管 Python 与 Node 初始化状态及保留诊断。
    pub managed_runtime_bootstrap: LuaSkillsManagedRuntimeBootstrapSummary,
    /// Independently repairable Python runtime state.
    /// 可独立修复的 Python 运行时状态。
    pub managed_python_runtime: LuaSkillsManagedRuntimeComponentSummary,
    /// Independently repairable Node.js runtime state.
    /// 可独立修复的 Node.js 运行时状态。
    pub managed_node_runtime: LuaSkillsManagedRuntimeComponentSummary,
    /// Current native managed-runtime installation progress when active.
    /// 活动时的当前原生受管运行时安装进度。
    pub managed_runtime_progress: Option<LuaSkillsManagedRuntimeProgressSummary>,
    /// Current LuaSkills install or update progress when active.
    /// 活动时的当前 LuaSkills 安装或更新进度。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_progress: Option<LuaSkillsOperationProgressSummary>,
    /// System Lua library directory used by Lua Skills.
    /// Lua Skills 使用的系统 Lua 库目录。
    pub system_lua_lib: String,
    /// Vulcan-owned runtime settings file used by LuaSkills.
    /// LuaSkills 使用的 Vulcan 自有运行时设置文件。
    pub runtime_settings_file: String,
    /// Complete Vulcan-owned startup settings saved for LuaSkills.
    /// 为 LuaSkills 保存的完整 Vulcan 自有启动设置。
    pub runtime_settings: LuaSkillsRuntimeSettings,
    /// Whether saved startup settings require an application restart to take effect.
    /// 已保存启动设置是否需要重启应用后才能生效。
    pub restart_required: bool,
    /// Total visible runtime entry count.
    /// 可见运行时入口总数。
    pub entry_count: usize,
    /// Human-readable notice for the current catalog state.
    /// 当前目录状态的人类可读提示。
    pub notice: String,
    /// Skill package rows visible to the host.
    /// 宿主当前可见的技能包行。
    pub skills: Vec<LuaSkillSummary>,
}

/// Revisioned full-domain response for one LuaSkills catalog mutation.
/// 单次 LuaSkills 目录变更对应的带版本完整领域响应。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LuaSkillsCatalogUpdate {
    /// Workbench generation committed together with this catalog projection.
    /// 与当前目录投影一并提交的工作台版本。
    pub revision: u64,
    /// Complete LuaSkills catalog after the committed mutation.
    /// 已提交变更后的完整 LuaSkills 目录。
    pub lua_skills: LuaSkillsCatalogSummary,
    /// Plugin rows derived from the same committed LuaSkills and Vulcan Plugin catalogs.
    /// 从同一份已提交 LuaSkills 与 Vulcan 插件目录派生的插件行。
    pub plugins: Vec<PluginSummary>,
}

/// One Lua Skill package row projected for the WebUI.
/// 投影给 WebUI 的单个 Lua Skill 包行。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LuaSkillSummary {
    /// Stable skill identifier.
    /// 稳定技能标识。
    pub skill_id: String,
    /// Human-facing display name.
    /// 面向用户的显示名称。
    pub display_name: String,
    /// Skill package version.
    /// 技能包版本。
    pub version: String,
    /// Runtime root layer name that owns this effective skill.
    /// 拥有该生效技能的运行时根层名称。
    pub root_name: String,
    /// Filesystem directory that contains this skill.
    /// 包含该技能的文件系统目录。
    pub skill_dir: String,
    /// Whether this skill is currently enabled.
    /// 该技能当前是否已启用。
    pub enabled: bool,
    /// Optional disabled reason from the lifecycle record.
    /// 生命周期记录中的可选停用原因。
    pub disabled_reason: Option<String>,
    /// Whether this skill has a managed install record.
    /// 该技能是否拥有受管安装记录。
    pub managed: bool,
    /// Optional install source type label.
    /// 可选安装来源类型标签。
    pub source_type: Option<String>,
    /// Optional install source locator.
    /// 可选安装来源定位值。
    pub source_locator: Option<String>,
    /// Number of visible runtime entries owned by this skill.
    /// 该技能拥有的可见运行时入口数量。
    pub entry_count: usize,
    /// Whether this skill belongs to the host-owned ROOT layer.
    /// 该技能是否属于宿主自有 ROOT 层。
    pub system_owned: bool,
    /// Tool rows exposed by this skill.
    /// 该技能暴露的工具行。
    pub tools: Vec<LuaSkillToolSummary>,
    /// Optional package configuration summary declared by this skill.
    /// 当前技能声明的可选技能包配置摘要。
    pub config: Option<LuaSkillConfigSummary>,
}

/// Compact package configuration status projected into one Lua Skill row.
/// 投影到单个 Lua Skill 行中的精简技能包配置状态。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LuaSkillConfigSummary {
    /// Whether every required and configured item is currently valid.
    /// 当前所有必填项和已配置项是否均合法。
    pub complete: bool,
    /// Revision of the immutable configuration snapshot.
    /// 不可变配置快照的修订号。
    pub revision: String,
    /// Persisted store scope selected by the effective package root.
    /// 生效技能包根选择的持久化存储作用域。
    pub store_scope: String,
    /// Number of declared configuration items.
    /// 已声明配置项数量。
    pub item_count: usize,
    /// Number of missing required configuration items.
    /// 缺失的必填配置项数量。
    pub missing_count: usize,
    /// Number of invalid persisted declared values.
    /// 非法的已持久化声明值数量。
    pub invalid_count: usize,
    /// Number of cross-field business validation issues.
    /// 跨字段业务校验问题数量。
    pub business_issue_count: usize,
    /// Number of persisted keys no longer declared by the package.
    /// 技能包不再声明的持久化键数量。
    pub orphaned_count: usize,
}

/// One package configuration enumeration option.
/// 单个技能包配置枚举选项。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LuaSkillConfigEnumOption {
    /// Stable persisted machine value.
    /// 稳定的持久化机器值。
    pub value: String,
    /// Human-readable option label.
    /// 人类可读的选项名称。
    pub label: String,
    /// Human-readable option description.
    /// 人类可读的选项说明。
    pub description: String,
}

/// Structured package configuration validation issue.
/// 结构化技能包配置校验问题。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LuaSkillConfigIssue {
    /// Optional configuration key associated with the issue.
    /// 与当前问题关联的可选配置键。
    pub key: Option<String>,
    /// Stable machine-readable issue code.
    /// 稳定的机器可读问题代码。
    pub code: String,
    /// Human-readable issue message without secret values.
    /// 不包含秘密值的人类可读问题消息。
    pub message: String,
}

/// One host-safe package configuration item descriptor.
/// 单个宿主安全的技能包配置项描述。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LuaSkillConfigItem {
    /// Stable package-level configuration key.
    /// 稳定的包级配置键。
    pub key: String,
    /// Declared configuration value type.
    /// 声明的配置值类型。
    #[serde(rename = "type")]
    pub value_type: String,
    /// Whether one effective value is required.
    /// 是否要求存在一个有效值。
    pub required: bool,
    /// Whether the value is sensitive under host disclosure policy.
    /// 当前值在宿主披露策略下是否敏感。
    pub sensitive: bool,
    /// Human-readable declaration description.
    /// 人类可读的声明说明。
    pub description: String,
    /// Type-specific declaration constraints.
    /// 类型专属声明约束。
    pub constraints: serde_json::Value,
    /// Enumeration options for enum declarations.
    /// 枚举声明使用的选项。
    pub options: Vec<LuaSkillConfigEnumOption>,
    /// Optional typed default value declared by the package.
    /// 技能包声明的可选类型化默认值。
    pub default: Option<serde_json::Value>,
    /// Optional short display title.
    /// 可选的简短显示标题。
    pub title: Option<String>,
    /// Optional host-facing group label.
    /// 可选的宿主侧分组名称。
    pub group: Option<String>,
    /// Optional display order inside the group.
    /// 分组内的可选显示顺序。
    pub order: Option<i32>,
    /// Whether hosts should place this item in an advanced section.
    /// 宿主是否应把当前项放入高级区域。
    pub advanced: bool,
    /// Optional input placeholder.
    /// 可选输入占位文本。
    pub placeholder: Option<String>,
    /// Optional typed example value.
    /// 可选类型化示例值。
    pub example: Option<serde_json::Value>,
    /// Optional host rendering format.
    /// 可选宿主渲染格式。
    pub format: Option<String>,
    /// Whether changing this item may require a restart.
    /// 修改当前项是否可能需要重启。
    pub restart_required: bool,
    /// Whether this declaration is deprecated.
    /// 当前声明是否已弃用。
    pub deprecated: bool,
    /// Optional deprecation guidance.
    /// 可选弃用说明。
    pub deprecation_message: Option<String>,
    /// Unambiguous runtime item state.
    /// 无歧义的运行时配置项状态。
    pub state: String,
    /// Whether this item satisfies package completeness.
    /// 当前配置项是否满足技能包完整性。
    pub satisfied: bool,
    /// Optional validation issue for an invalid persisted value.
    /// 非法持久化值对应的可选校验问题。
    pub validation_error: Option<LuaSkillConfigIssue>,
    /// Optional effective value disclosed only for non-sensitive items.
    /// 仅为非敏感项披露的可选有效值。
    pub value: Option<String>,
}

/// Full host-safe package configuration descriptor for the WebUI.
/// 面向 WebUI 的完整宿主安全技能包配置描述。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LuaSkillConfigDescriptor {
    /// Stable skill package identifier.
    /// 稳定技能包标识。
    pub skill_id: String,
    /// Semantic package version owning the declaration.
    /// 拥有当前声明的语义化技能包版本。
    pub skill_version: String,
    /// Whether the package configuration is complete and valid.
    /// 当前技能包配置是否完整且合法。
    pub complete: bool,
    /// Revision of the immutable configuration snapshot.
    /// 不可变配置快照的修订号。
    pub revision: String,
    /// Persisted store scope selected by the effective package root.
    /// 生效技能包根选择的持久化存储作用域。
    pub store_scope: String,
    /// Number of missing required declarations.
    /// 缺失的必填声明数量。
    pub missing_count: usize,
    /// Number of invalid persisted declared values.
    /// 非法的已持久化声明值数量。
    pub invalid_count: usize,
    /// Number of cross-field business issues.
    /// 跨字段业务问题数量。
    pub business_issue_count: usize,
    /// Number of persisted keys no longer declared by the package.
    /// 技能包不再声明的持久化键数量。
    pub orphaned_count: usize,
    /// Persisted keys no longer declared by the package.
    /// 技能包不再声明的持久化键。
    pub orphaned: Vec<String>,
    /// Declared package configuration items.
    /// 已声明的技能包配置项。
    pub items: Vec<LuaSkillConfigItem>,
}

/// Full package configuration completeness and validity result.
/// 完整的技能包配置完整性与合法性结果。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LuaSkillConfigValidation {
    /// Stable skill package identifier.
    /// 稳定技能包标识。
    pub skill_id: String,
    /// Whether every required and configured item is valid.
    /// 所有必填项与已配置项是否均合法。
    pub complete: bool,
    /// Revision of the validated immutable snapshot.
    /// 已校验不可变快照的修订号。
    pub revision: String,
    /// Persisted store scope selected by the effective package root.
    /// 生效技能包根选择的持久化存储作用域。
    pub store_scope: String,
    /// Missing required declaration issues.
    /// 缺失必填声明问题。
    pub missing: Vec<LuaSkillConfigIssue>,
    /// Invalid persisted declared-value issues.
    /// 非法已持久化声明值问题。
    pub invalid: Vec<LuaSkillConfigIssue>,
    /// Cross-field business validation issues.
    /// 跨字段业务校验问题。
    pub business_issues: Vec<LuaSkillConfigIssue>,
    /// Persisted keys no longer declared by the package.
    /// 技能包不再声明的持久化键。
    pub orphaned: Vec<String>,
    /// Number of persisted keys no longer declared by the package.
    /// 技能包不再声明的持久化键数量。
    pub orphaned_count: usize,
}

/// Typed non-empty batch request for updating package configuration.
/// 更新技能包配置使用的类型化非空批量请求。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LuaSkillConfigSetRequest {
    /// Stable skill package identifier.
    /// 稳定技能包标识。
    pub skill_id: String,
    /// Revision observed when the editor was opened.
    /// 打开编辑器时观察到的修订号。
    pub expected_revision: String,
    /// Typed scalar values keyed by declared configuration key.
    /// 按已声明配置键索引的类型化标量值。
    pub values: BTreeMap<String, serde_json::Value>,
}

/// Host-safe result of one atomic package configuration update.
/// 单次原子技能包配置更新的宿主安全结果。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LuaSkillConfigSetResult {
    /// Fresh host-safe descriptor after the transaction.
    /// 事务完成后的最新宿主安全描述。
    pub descriptor: LuaSkillConfigDescriptor,
    /// Revision visible after the transaction.
    /// 事务完成后可见的修订号。
    pub revision: String,
    /// Whether persisted data changed.
    /// 持久化数据是否发生变化。
    pub changed: bool,
    /// Canonical submitted values retained only for non-sensitive declarations.
    /// 仅为非敏感声明保留的规范提交值。
    pub values: BTreeMap<String, String>,
    /// Stable keys whose persisted values changed.
    /// 持久化值发生变化的稳定键。
    pub changed_keys: Vec<String>,
    /// Changed keys whose declarations recommend a restart.
    /// 声明建议重启的已变更键。
    pub restart_required_keys: Vec<String>,
}

/// CAS-protected request for deleting one explicit or orphaned value.
/// 删除单个显式值或遗留值使用的 CAS 保护请求。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LuaSkillConfigDeleteRequest {
    /// Stable skill package identifier.
    /// 稳定技能包标识。
    pub skill_id: String,
    /// Stable declared or orphaned configuration key.
    /// 稳定的已声明或遗留配置键。
    pub key: String,
    /// Revision observed when the editor was opened.
    /// 打开编辑器时观察到的修订号。
    pub expected_revision: String,
}

/// Host-safe result of one CAS-protected package configuration deletion.
/// 单次 CAS 保护技能包配置删除的宿主安全结果。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LuaSkillConfigDeleteResult {
    /// Fresh host-safe descriptor after the deletion.
    /// 删除完成后的最新宿主安全描述。
    pub descriptor: LuaSkillConfigDescriptor,
    /// Revision visible after the deletion.
    /// 删除完成后可见的修订号。
    pub revision: String,
    /// Whether one persisted value was removed.
    /// 是否移除了一个持久化值。
    pub deleted: bool,
    /// Exact declared or orphaned key targeted by the request.
    /// 请求定位的精确已声明或遗留键。
    pub key: String,
}

/// Strict host request for explicitly refreshing one or both package configuration stores.
/// 显式刷新一个或两个技能包配置存储的严格宿主请求。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LuaSkillConfigRefreshRequest {
    /// Optional exact store scope; omission refreshes both configured stores.
    /// 可选的精确存储作用域；省略时刷新两个已配置存储。
    pub store_scope: Option<String>,
}

/// Result of explicitly refreshing one package configuration store.
/// 显式刷新单个技能包配置存储的结果。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LuaSkillConfigStoreRefresh {
    /// Refreshed store scope.
    /// 已刷新的存储作用域。
    pub store_scope: String,
    /// Revision visible after refresh.
    /// 刷新后可见的修订号。
    pub revision: String,
    /// Whether a newer immutable snapshot was installed.
    /// 是否安装了更新的不可变快照。
    pub changed: bool,
}

/// One ordered LuaSkills package configuration event.
/// 单个有序 LuaSkills 技能包配置事件。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LuaSkillConfigEvent {
    /// Monotonic engine-local event sequence.
    /// 引擎内单调事件序号。
    pub sequence: String,
    /// Stable event type.
    /// 稳定事件类型。
    #[serde(rename = "type")]
    pub event_type: String,
    /// Persisted store scope.
    /// 持久化存储作用域。
    pub store_scope: String,
    /// Optional changed skill package identifier.
    /// 可选的已变更技能包标识。
    pub skill_id: Option<String>,
    /// Last known valid store revision.
    /// 最后一个已知合法存储修订号。
    pub revision: String,
    /// Stable changed configuration keys.
    /// 稳定的已变更配置键。
    pub changed_keys: Vec<String>,
    /// Event source.
    /// 事件来源。
    pub source: String,
    /// Changed keys whose declarations recommend a restart.
    /// 声明建议重启的已变更键。
    pub restart_required_keys: Vec<String>,
    /// Optional package completeness after the event.
    /// 事件后的可选技能包完整性。
    pub complete: Option<bool>,
    /// Optional structured reload or watcher failure.
    /// 可选的结构化重载或监听失败。
    pub error: Option<LuaSkillConfigIssue>,
}

/// Bounded ordered LuaSkills package configuration event batch.
/// 有界有序的 LuaSkills 技能包配置事件批次。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LuaSkillConfigEventBatch {
    /// Events newer than the requested cursor.
    /// 比请求游标更新的事件。
    pub events: Vec<LuaSkillConfigEvent>,
    /// Highest sequence observed while producing this batch.
    /// 生成当前批次时观察到的最高序号。
    pub next_sequence: String,
}

/// One Lua Skill runtime tool row with registration state.
/// 带注册状态的单个 Lua Skill 运行时工具行。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LuaSkillToolSummary {
    /// Canonical host-visible tool name.
    /// 宿主可见的规范工具名称。
    pub canonical_name: String,
    /// Local tool name inside the skill package.
    /// 技能包内部的本地工具名称。
    pub local_name: String,
    /// Tool description from the runtime entry descriptor.
    /// 来自运行时入口描述符的工具说明。
    pub description: String,
    /// Persisted registration state key.
    /// 持久化注册状态键。
    pub registration_state: String,
    /// Human-readable registration state label.
    /// 人类可读的注册状态标签。
    pub registration_label: String,
    /// Short help text for the current registration state.
    /// 当前注册状态的简短说明。
    pub registration_help: String,
}

/// Full WebUI summary for prompt-based Agent Skills.
/// 基于提示词的 Agent Skills 完整 WebUI 摘要。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentSkillsCatalogSummary {
    /// System skill root used by host-managed Agent Skills.
    /// 宿主管理 Agent Skills 使用的系统技能根目录。
    pub system_root: String,
    /// User skill root used by user-installed Agent Skills.
    /// 用户安装 Agent Skills 使用的用户技能根目录。
    pub user_root: String,
    /// Human-readable notice for the current catalog state.
    /// 当前目录状态的人类可读提示。
    pub notice: String,
    /// Complete source-aware Agent Skill records.
    /// 完整且包含来源关系的 Agent Skill 记录。
    pub records: Vec<AgentSkillRecordSummary>,
    /// Logical category rows synchronized with the current records.
    /// 与当前记录同步的逻辑分类行。
    pub categories: Vec<AgentSkillCategorySummary>,
    /// Non-fatal loading issues.
    /// 非致命加载问题。
    pub errors: Vec<AgentSkillLoadIssueSummary>,
}

/// Revisioned full-domain response for one Agent Skills mutation.
/// 单次 Agent Skills 变更对应的带版本完整领域响应。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentSkillsCatalogUpdate {
    /// Workbench generation committed together with this catalog projection.
    /// 与当前目录投影一并提交的工作台版本。
    pub revision: u64,
    /// Complete Agent Skills catalog after the committed mutation.
    /// 已提交变更后的完整 Agent Skills 目录。
    pub agent_skills: AgentSkillsCatalogSummary,
}

/// Full WebUI summary for the agent and expert catalog.
/// 智能体与专家目录的完整 WebUI 摘要。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubagentCatalogSummary {
    /// System agent definition root shipped with the application.
    /// 随应用发布的系统智能体定义根目录。
    pub system_root: String,
    /// User-owned agent definition and override root.
    /// 用户拥有的智能体定义与覆盖根目录。
    pub user_root: String,
    /// Human-readable notice for the current catalog state.
    /// 当前目录状态的人类可读提示。
    pub notice: String,
    /// Public tool ids that child agents cannot execute directly.
    /// 智能体不能直接执行的公开工具标识。
    pub child_agent_forbidden_tool_ids: Vec<String>,
    /// Merged agent and expert definition rows.
    /// 合并后的智能体与专家定义行。
    pub definitions: Vec<SubagentDefinitionSummary>,
    /// Parsed category rows.
    /// 已解析的分类行。
    pub categories: Vec<SubagentCategorySummary>,
    /// Non-fatal loading issues.
    /// 非致命加载问题。
    pub errors: Vec<SubagentCatalogIssueSummary>,
}

/// Revisioned full-domain response for one agent catalog mutation.
/// 单次智能体目录变更对应的带版本完整领域响应。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubagentCatalogUpdate {
    /// Workbench generation committed together with this catalog projection.
    /// 与当前目录投影一并提交的工作台版本。
    pub revision: u64,
    /// Complete agent catalog after the committed mutation.
    /// 已提交变更后的完整智能体目录。
    pub subagent_catalog: SubagentCatalogSummary,
}

/// One agent catalog category projected for the WebUI.
/// 投影给 WebUI 的单个智能体目录分类。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubagentCategorySummary {
    /// Stable category id.
    /// 稳定分类标识。
    pub category_id: String,
    /// Human-facing category name.
    /// 面向用户的分类名称。
    pub name: String,
    /// Optional category description.
    /// 可选分类说明。
    pub description: Option<String>,
    /// Localized display metadata keyed by locale tag.
    /// 按语言标签索引的本地化展示元数据。
    #[serde(default)]
    pub locales: std::collections::BTreeMap<String, SubagentCategoryLocaleSummary>,
    /// Human-facing category emoji.
    /// 面向用户的分类图标。
    pub emoji: String,
    /// Human-facing category color token or value.
    /// 面向用户的分类颜色标识或颜色值。
    pub color: String,
    /// Optional category order.
    /// 可选分类排序值。
    pub order: Option<i32>,
}

/// Localized WebUI metadata for one agent catalog category.
/// 单个智能体目录分类的本地化 WebUI 元数据。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubagentCategoryLocaleSummary {
    /// Optional localized category name.
    /// 可选本地化分类名称。
    pub name: Option<String>,
    /// Optional localized category description.
    /// 可选本地化分类说明。
    pub description: Option<String>,
}

/// One agent or expert definition row projected for the WebUI.
/// 投影给 WebUI 的单个智能体或专家定义行。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubagentDefinitionSummary {
    /// Stable agent id, normally the Markdown file stem.
    /// 稳定智能体标识，通常为 Markdown 文件 stem。
    pub agent_id: String,
    /// Human-facing display name.
    /// 面向用户的显示名称。
    pub name: String,
    /// Optional human-facing description.
    /// 可选的人类可读说明。
    pub description: Option<String>,
    /// Localized display metadata keyed by locale tag.
    /// 按语言标签索引的本地化展示元数据。
    #[serde(default)]
    pub locales: std::collections::BTreeMap<String, SubagentDefinitionLocaleSummary>,
    /// Optional upstream or license source metadata.
    /// 可选上游或协议来源元数据。
    pub source_origin: Option<SubagentOriginSummary>,
    /// Human-facing emoji.
    /// 面向用户的图标。
    pub emoji: String,
    /// Human-facing color token or color value.
    /// 面向用户的颜色标识或颜色值。
    pub color: String,
    /// Optional short tone or persona hint.
    /// 可选简短语气或人格提示。
    pub vibe: Option<String>,
    /// Stable source key for this definition row.
    /// 当前定义行的稳定来源键。
    pub source_key: String,
    /// Optional canonical category id.
    /// 可选规范分类标识。
    pub category_id: Option<String>,
    /// Optional package id.
    /// 可选包标识。
    pub package_id: Option<String>,
    /// Source scope label.
    /// 来源作用域标签。
    pub scope: String,
    /// Stable source scope key used by UI logic.
    /// UI 逻辑使用的稳定来源作用域键。
    pub scope_key: String,
    /// Optional owning plugin identifier for plugin-contributed definitions.
    /// 插件贡献定义的可选所属插件标识。
    #[serde(default)]
    pub owning_plugin_id: Option<String>,
    /// Whether the definition may be edited directly.
    /// 当前定义是否允许直接编辑。
    #[serde(default)]
    pub can_edit: bool,
    /// Whether the definition may be deleted directly.
    /// 当前定义是否允许直接删除。
    #[serde(default)]
    pub can_delete: bool,
    /// Whether the definition may be toggled independently.
    /// 当前定义是否允许独立启停。
    #[serde(default)]
    pub can_toggle: bool,
    /// Whether the definition may be copied into a user record.
    /// 当前定义是否允许复制为用户记录。
    #[serde(default)]
    pub can_copy: bool,
    /// Whether this definition is exposed to models and spawn_agent.
    /// 该定义是否暴露给模型与 spawn_agent。
    pub enabled: bool,
    /// Filesystem path to the Markdown definition file.
    /// 指向 Markdown 定义文件的文件系统路径。
    pub definition_file: String,
    /// Optional filesystem path to the sidecar metadata file.
    /// 指向 sidecar 元数据文件的可选文件系统路径。
    pub sidecar_file: Option<String>,
    /// Optional remote URL that supplied this user definition.
    /// 提供该用户定义的可选远程 URL。
    pub remote_url: Option<String>,
    /// Optional system definition id overridden by this row.
    /// 被当前行覆盖的可选系统定义标识。
    pub override_of: Option<String>,
    /// Whether the user layer can be removed to restore a system definition.
    /// 是否可以移除用户层以恢复系统定义。
    #[serde(default)]
    pub can_restore_default: bool,
    /// Optional source hash for audit and cache diagnostics.
    /// 用于审计与缓存诊断的可选来源哈希。
    pub source_hash: Option<String>,
    /// Optional source version string.
    /// 可选来源版本字符串。
    pub version: Option<String>,
    /// Optional last-updated timestamp string.
    /// 可选最近更新时间戳字符串。
    pub updated_at: Option<String>,
    /// Unified tool policy mode.
    /// 统一工具策略模式。
    pub tool_policy_mode: String,
    /// Unified tool policy selectors.
    /// 统一工具策略选择器。
    pub tool_policy_tools: Vec<String>,
    /// Number of explicitly allowed host tools.
    /// 显式允许的宿主工具数量。
    pub allowed_tool_count: usize,
    /// Number of explicitly disabled host tools.
    /// 显式禁用的宿主工具数量。
    pub disabled_tool_count: usize,
    /// Number of enabled prompt skills declared by this definition.
    /// 当前定义声明启用的提示词技能数量。
    pub enabled_prompt_skill_count: usize,
    /// Number of disabled prompt skills declared by this definition.
    /// 当前定义声明禁用的提示词技能数量。
    pub disabled_prompt_skill_count: usize,
    /// Unified prompt-skill policy mode.
    /// 统一提示词技能策略模式。
    pub prompt_skill_policy_mode: String,
    /// Unified prompt-skill policy selectors.
    /// 统一提示词技能策略选择器。
    pub prompt_skill_policy_skills: Vec<String>,
    /// Optional fixed provider/model selection for this agent.
    /// 当前智能体可选的固定供应商/模型选择。
    #[serde(default)]
    pub provider_model: SubagentProviderModelSummary,
    /// Explicitly allowed host tool selectors.
    /// 显式允许的宿主工具选择器。
    pub allowed_tools: Vec<String>,
    /// Explicitly disabled host tool selectors.
    /// 显式禁用的宿主工具选择器。
    pub disabled_tools: Vec<String>,
    /// Explicitly allowed AI capability selectors.
    /// 显式允许的 AI 能力选择器。
    pub allowed_ai_capabilities: Vec<String>,
    /// Explicitly disabled AI capability selectors.
    /// 显式禁用的 AI 能力选择器。
    pub disabled_ai_capabilities: Vec<String>,
    /// LuaSkills-specific exposure override payload.
    /// LuaSkills 专属可见性覆盖载荷。
    pub luaskills_overrides: serde_json::Value,
    /// MCP-specific exposure override payload.
    /// MCP 专属可见性覆盖载荷。
    pub mcp_server_overrides: serde_json::Value,
    /// Explicitly enabled prompt skill names.
    /// 显式启用的提示词技能名称。
    pub enabled_prompt_skills: Vec<String>,
    /// Explicitly disabled prompt skill names.
    /// 显式禁用的提示词技能名称。
    pub disabled_prompt_skills: Vec<String>,
}

/// Prompt text of one agent definition, read on demand by the agent detail and editor views.
/// 单个智能体定义的提示词正文，由智能体详情与编辑视图按需读取。
///
/// Prompts stay out of [`SubagentDefinitionSummary`] because the catalog travels with every
/// workbench snapshot and the bundled prompts alone exceed several megabytes.
/// 提示词不进入 [`SubagentDefinitionSummary`]，因为目录随每个工作台快照传输，
/// 而内置提示词合计就超过数 MB。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubagentDefinitionPrompts {
    /// Agent id the prompts belong to.
    /// 提示词所属的智能体标识。
    pub agent_id: String,
    /// Editable base system prompt.
    /// 可编辑基础系统提示词。
    pub system_prompt: Option<String>,
    /// Editable appended system prompt.
    /// 可编辑追加系统提示词。
    pub system_prompt_append: Option<String>,
}

/// Provider/model selection projected for one subagent definition.
/// 投影给单个子代理定义的供应商/模型选择。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubagentProviderModelSummary {
    /// Optional provider identifier.
    /// 可选供应商标识。
    #[serde(default)]
    pub provider_id: Option<String>,
    /// Optional model identifier.
    /// 可选模型标识。
    #[serde(default)]
    pub model_id: Option<String>,
    /// Effective display mode for this agent's model selection.
    /// 当前智能体模型选择的有效展示模式。
    #[serde(default)]
    pub mode: String,
    /// Stable provider/model selector when a fixed model is configured.
    /// 配置固定模型时使用的稳定 provider/model 选择器。
    #[serde(default)]
    pub selector: Option<String>,
    /// Whether the fixed model is currently selectable.
    /// 固定模型当前是否可被选择。
    #[serde(default)]
    pub available: Option<bool>,
    /// Human-readable reason when the fixed model is unavailable.
    /// 固定模型不可用时的人类可读原因。
    #[serde(default)]
    pub unavailable_reason: Option<String>,
}

/// Localized WebUI metadata for one agent definition.
/// 单个智能体定义的本地化 WebUI 元数据。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubagentDefinitionLocaleSummary {
    /// Optional localized agent name.
    /// 可选本地化智能体名称。
    pub name: Option<String>,
    /// Optional localized agent description.
    /// 可选本地化智能体说明。
    pub description: Option<String>,
}

/// Upstream or license source metadata projected for the WebUI.
/// 投影给 WebUI 的上游或协议来源元数据。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubagentOriginSummary {
    /// Source kind such as github, system, or other.
    /// 来源类型，例如 github、system 或 other。
    pub kind: Option<String>,
    /// Optional human-facing source label.
    /// 可选的人类可读来源标签。
    pub label: Option<String>,
    /// Optional icon key used by WebUI.
    /// WebUI 使用的可选图标键。
    pub icon: Option<String>,
    /// Optional source URL.
    /// 可选来源链接。
    pub url: Option<String>,
}

/// Tri-state text patch used by partial agent metadata saves.
/// 智能体元数据局部保存使用的三态文本补丁。
#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub enum OptionalTextPatch {
    /// The request omitted this field and the previous value must be kept.
    /// 请求未提交该字段，必须保留旧值。
    #[default]
    Missing,
    /// The request explicitly sent null and the previous value must be cleared.
    /// 请求显式提交 null，必须清空旧值。
    Clear,
    /// The request supplied a replacement text value.
    /// 请求提交了用于替换的文本值。
    Set(String),
}

impl OptionalTextPatch {
    /// Returns true when the patch carries no requested change.
    /// 当补丁不携带变更请求时返回 true。
    pub fn is_missing(&self) -> bool {
        matches!(self, Self::Missing)
    }
}

impl Serialize for OptionalTextPatch {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::Missing | Self::Clear => serializer.serialize_none(),
            Self::Set(value) => serializer.serialize_some(value),
        }
    }
}

impl<'de> Deserialize<'de> for OptionalTextPatch {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Option::<String>::deserialize(deserializer)
            .map(|value| value.map(Self::Set).unwrap_or(Self::Clear))
    }
}

/// Save request for one user-owned agent definition.
/// 单个用户拥有的智能体保存请求。
///
/// User-owned definitions carry one name and description in the author's language; only
/// bundled system definitions provide per-locale display metadata.
/// 用户拥有的定义只携带作者所用语言的一组名称与说明；仅内置系统定义提供按语言的展示元数据。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubagentDefinitionSaveRequest {
    /// Stable agent id, normally the Markdown file stem.
    /// 稳定智能体标识，通常为 Markdown 文件 stem。
    pub agent_id: String,
    /// Optional original agent id when saving a moved definition.
    /// 保存已转移定义时使用的可选原始智能体标识。
    #[serde(default)]
    pub original_agent_id: Option<String>,
    /// Optional user package id assigned to this definition.
    /// 分配给当前定义的可选用户包标识。
    #[serde(default)]
    pub package_id: Option<String>,
    /// Human-facing display name.
    /// 面向用户的显示名称。
    pub name: String,
    /// Optional human-facing description.
    /// 可选的人类可读说明。
    pub description: Option<String>,
    /// Optional human-facing emoji override.
    /// 可选的人类可读图标覆盖。
    #[serde(default, skip_serializing_if = "OptionalTextPatch::is_missing")]
    pub emoji: OptionalTextPatch,
    /// Optional human-facing color token or value override.
    /// 可选的人类可读颜色标识或颜色值覆盖。
    #[serde(default, skip_serializing_if = "OptionalTextPatch::is_missing")]
    pub color: OptionalTextPatch,
    /// Optional short tone or persona hint override.
    /// 可选简短语气或人格提示覆盖。
    #[serde(default, skip_serializing_if = "OptionalTextPatch::is_missing")]
    pub vibe: OptionalTextPatch,
    /// Whether this definition is exposed to models and spawn_agent.
    /// 该定义是否暴露给模型与 spawn_agent。
    pub enabled: bool,
    /// Base system prompt saved for the agent.
    /// 为该智能体保存的基础系统提示词。
    pub system_prompt: Option<String>,
    /// Appended system prompt saved for the agent.
    /// 为该智能体保存的追加系统提示词。
    pub system_prompt_append: Option<String>,
    /// Explicitly allowed host tool selectors.
    /// 显式允许的宿主工具选择器。
    pub allowed_tools: Vec<String>,
    /// Explicitly disabled host tool selectors.
    /// 显式禁用的宿主工具选择器。
    pub disabled_tools: Vec<String>,
    /// Unified tool policy mode.
    /// 统一工具策略模式。
    #[serde(default)]
    pub tool_policy_mode: Option<String>,
    /// Unified tool policy selectors.
    /// 统一工具策略选择器。
    #[serde(default)]
    pub tool_policy_tools: Vec<String>,
    /// Explicitly allowed AI capability selectors.
    /// 显式允许的 AI 能力选择器。
    #[serde(default)]
    pub allowed_ai_capabilities: Vec<String>,
    /// Explicitly disabled AI capability selectors.
    /// 显式禁用的 AI 能力选择器。
    #[serde(default)]
    pub disabled_ai_capabilities: Vec<String>,
    /// LuaSkills-specific exposure override payload.
    /// LuaSkills 专属可见性覆盖载荷。
    #[serde(default)]
    pub luaskills_overrides: serde_json::Value,
    /// MCP-specific exposure override payload.
    /// MCP 专属可见性覆盖载荷。
    #[serde(default)]
    pub mcp_server_overrides: serde_json::Value,
    /// Explicitly enabled prompt skill names.
    /// 显式启用的提示词技能名称。
    pub enabled_prompt_skills: Vec<String>,
    /// Explicitly disabled prompt skill names.
    /// 显式禁用的提示词技能名称。
    pub disabled_prompt_skills: Vec<String>,
    /// Unified prompt-skill policy mode.
    /// 统一提示词技能策略模式。
    #[serde(default)]
    pub prompt_skill_policy_mode: Option<String>,
    /// Unified prompt-skill policy selectors.
    /// 统一提示词技能策略选择器。
    #[serde(default)]
    pub prompt_skill_policy_skills: Vec<String>,
    /// Optional fixed provider/model selection for this agent.
    /// 当前智能体可选的固定供应商/模型选择。
    #[serde(default)]
    pub provider_model: SubagentProviderModelSummary,
}

/// One agent catalog loading issue projected for diagnostics.
/// 投影给诊断界面的单个智能体目录加载问题。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubagentCatalogIssueSummary {
    /// Path related to the loading issue.
    /// 与加载问题相关的路径。
    pub path: String,
    /// Human-readable diagnostic message.
    /// 人类可读诊断信息。
    pub message: String,
}

/// One source-aware prompt Agent Skill record projected for the WebUI.
/// 投影给 WebUI 的单个来源感知提示词 Agent Skill 记录。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentSkillRecordSummary {
    /// Stable internal key composed from scope and standard name.
    /// 由作用域与标准名称组成的稳定内部键。
    pub skill_key: String,
    /// Standard canonical skill name from frontmatter.
    /// 来自 frontmatter 的标准规范技能名称。
    pub name: String,
    /// Human-facing display name.
    /// 面向用户的显示名称。
    pub display_name: String,
    /// Human-facing short description.
    /// 面向用户的简短说明。
    pub description: String,
    /// Localized display metadata keyed by locale tag.
    /// 按语言标签索引的本地化展示元数据。
    pub locales: std::collections::BTreeMap<String, AgentSkillLocaleSummary>,
    /// Stable source scope key.
    /// 稳定来源作用域键。
    pub scope_key: String,
    /// Optional owning plugin identifier for plugin-contributed skills.
    /// 插件贡献技能的可选所属插件标识。
    #[serde(default)]
    pub owning_plugin_id: Option<String>,
    /// Whether the skill may be edited directly.
    /// 当前技能是否允许直接编辑。
    #[serde(default)]
    pub can_edit: bool,
    /// Whether the skill may be deleted directly.
    /// 当前技能是否允许直接删除。
    #[serde(default)]
    pub can_delete: bool,
    /// Whether the skill may be toggled independently.
    /// 当前技能是否允许独立启停。
    #[serde(default)]
    pub can_toggle: bool,
    /// Whether the skill may be copied into a user package.
    /// 当前技能是否允许复制为用户包。
    #[serde(default)]
    pub can_copy: bool,
    /// Whether a user override may be removed to restore its system source.
    /// 是否允许移除用户覆盖以恢复系统来源。
    #[serde(default)]
    pub can_restore_default: bool,
    /// Whether this source record is effective after same-name resolution.
    /// 同名解析后当前来源记录是否生效。
    pub effective: bool,
    /// Optional system record key overridden by this user record.
    /// 当前用户记录覆盖的可选系统记录键。
    pub override_of: Option<String>,
    /// Whether this effective Agent Skill is enabled.
    /// 当前有效 Agent Skill 是否启用。
    pub enabled: bool,
    /// Stable primary logical category identifier.
    /// 稳定主逻辑分类标识。
    pub category_id: String,
    /// Stable searchable tags.
    /// 稳定可搜索标签。
    pub tags: Vec<String>,
    /// Complete standard frontmatter metadata.
    /// 完整标准 frontmatter 元数据。
    pub standard: AgentSkillStandardMetadataSummary,
    /// Auditable source provenance.
    /// 可审计来源信息。
    pub origin: AgentSkillOriginSummary,
    /// Standard optional resource counts.
    /// 标准可选资源数量。
    pub resources: AgentSkillResourceSummary,
    /// Filesystem path to the SKILL.md file.
    /// 指向 SKILL.md 文件的文件系统路径。
    pub skill_file: String,
    /// Directory that owns this Agent Skill and bundled resources.
    /// 拥有该 Agent Skill 与附带资源的目录。
    pub base_dir: String,
    /// Optional emoji rendered by the agent-style marker.
    /// 由智能体同款标记渲染的可选表情图标。
    pub emoji: Option<String>,
    /// Optional hexadecimal marker color.
    /// 可选十六进制标记颜色。
    pub color: Option<String>,
    /// Markdown body without YAML frontmatter.
    /// 去除 YAML frontmatter 后的 Markdown 正文。
    pub body: String,
}

/// Localized display metadata for one Agent Skill record.
/// 单个 Agent Skill 记录的本地化展示元数据。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentSkillLocaleSummary {
    /// Optional localized display name.
    /// 可选本地化显示名称。
    pub display_name: Option<String>,
    /// Optional localized compact description.
    /// 可选本地化紧凑说明。
    pub short_description: Option<String>,
}

/// Standard Agent Skills frontmatter metadata projected for details and diagnostics.
/// 为详情与诊断投影的标准 Agent Skills frontmatter 元数据。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentSkillStandardMetadataSummary {
    /// Model-facing trigger description.
    /// 面向模型的触发说明。
    pub description: String,
    /// Optional license identifier or bundled file reference.
    /// 可选协议标识或包内文件引用。
    pub license: Option<String>,
    /// Optional environment compatibility statement.
    /// 可选环境兼容性说明。
    pub compatibility: Option<String>,
    /// Standard string extension metadata.
    /// 标准字符串扩展元数据。
    pub metadata: std::collections::BTreeMap<String, String>,
}

/// Auditable origin metadata for one Agent Skill record.
/// 单个 Agent Skill 记录的可审计来源元数据。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentSkillOriginSummary {
    /// Stable origin kind.
    /// 稳定来源类型。
    pub kind: String,
    /// Optional source label.
    /// 可选来源标签。
    pub label: Option<String>,
    /// Optional source URL.
    /// 可选来源链接。
    pub url: Option<String>,
    /// Optional source version.
    /// 可选来源版本。
    pub version: Option<String>,
    /// Optional immutable revision.
    /// 可选不可变修订号。
    pub revision: Option<String>,
    /// Whether the source lifecycle is host-managed.
    /// 来源生命周期是否由宿主管理。
    pub managed: bool,
    /// Optional integrity digest.
    /// 可选完整性摘要。
    pub integrity: Option<String>,
    /// Optional installation timestamp.
    /// 可选安装时间戳。
    pub installed_at: Option<String>,
    /// Optional last synchronization timestamp.
    /// 可选最近同步时间戳。
    pub updated_at: Option<String>,
}

/// Standard optional resource counts for one Agent Skill record.
/// 单个 Agent Skill 记录的标准可选资源数量。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentSkillResourceSummary {
    /// Number of script files.
    /// 脚本文件数量。
    pub scripts: usize,
    /// Number of reference files.
    /// 引用文件数量。
    pub references: usize,
    /// Number of asset files.
    /// 资产文件数量。
    pub assets: usize,
}

/// One logical Agent Skills category projected for the WebUI.
/// 投影给 WebUI 的单个逻辑 Agent Skills 分类。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentSkillCategorySummary {
    /// Stable category identifier.
    /// 稳定分类标识。
    pub category_id: String,
    /// Default category name.
    /// 默认分类名称。
    pub name: String,
    /// Optional default category description.
    /// 可选默认分类说明。
    pub description: Option<String>,
    /// Category emoji.
    /// 分类图标。
    pub emoji: String,
    /// Category color.
    /// 分类颜色。
    pub color: String,
    /// Stable display order.
    /// 稳定展示顺序。
    pub order: i32,
    /// Whether the category ships with the host.
    /// 当前分类是否由宿主内置。
    pub builtin: bool,
    /// Localized category metadata keyed by locale tag.
    /// 按语言标签索引的本地化分类元数据。
    pub locales: std::collections::BTreeMap<String, AgentSkillCategoryLocaleSummary>,
}

/// Localized metadata for one logical Agent Skills category.
/// 单个逻辑 Agent Skills 分类的本地化元数据。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentSkillCategoryLocaleSummary {
    /// Localized category name.
    /// 本地化分类名称。
    pub name: String,
    /// Optional localized category description.
    /// 可选本地化分类说明。
    pub description: Option<String>,
}

/// Complete user-authored Agent Skill save request.
/// 完整的用户创作 Agent Skill 保存请求。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentSkillSaveRequest {
    /// Optional original standard name when renaming one existing user skill.
    /// 重命名现有用户技能时使用的可选原标准名称。
    pub original_name: Option<String>,
    /// Optional exact source-aware record key used to preserve or copy bundled resources.
    /// 用于保留或复制附带资源的可选精确来源记录键。
    pub source_skill_key: Option<String>,
    /// Standard canonical skill name.
    /// 标准规范技能名称。
    pub name: String,
    /// Model-facing trigger description.
    /// 面向模型的触发说明。
    pub description: String,
    /// Optional license identifier or bundled file reference.
    /// 可选协议标识或包内文件引用。
    pub license: Option<String>,
    /// Optional environment compatibility statement.
    /// 可选环境兼容性说明。
    pub compatibility: Option<String>,
    /// Standard string extension metadata.
    /// 标准字符串扩展元数据。
    pub metadata: std::collections::BTreeMap<String, String>,
    /// Markdown instruction body without frontmatter.
    /// 不含 frontmatter 的 Markdown 说明正文。
    pub body: String,
    /// Complete UI-facing metadata.
    /// 完整界面展示元数据。
    pub interface: AgentSkillInterfaceSaveRequest,
    /// Stable primary logical category identifier.
    /// 稳定主逻辑分类标识。
    pub category_id: String,
    /// Stable searchable tags.
    /// 稳定可搜索标签。
    pub tags: Vec<String>,
    /// Whether the saved skill is enabled.
    /// 保存后的技能是否启用。
    pub enabled: bool,
}

/// UI-facing metadata included in one Agent Skill save request.
/// 单次 Agent Skill 保存请求包含的界面展示元数据。
///
/// User-authored skills carry one display name and description; only bundled system skills are localized.
/// 用户创作的技能只携带一组显示名称与说明；仅内置系统技能提供本地化。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentSkillInterfaceSaveRequest {
    /// Optional display name.
    /// 可选显示名称。
    pub display_name: Option<String>,
    /// Optional compact description.
    /// 可选紧凑说明。
    pub short_description: Option<String>,
    /// Optional emoji rendered by the skill marker.
    /// 由技能标记渲染的可选表情图标。
    pub emoji: Option<String>,
    /// Optional hexadecimal marker color.
    /// 可选十六进制标记颜色。
    pub color: Option<String>,
}

/// User-defined logical Agent Skills category save request.
/// 用户定义逻辑 Agent Skills 分类保存请求。
///
/// User categories carry one name and description; only built-in categories are localized.
/// 用户分类只携带一组名称与说明；仅内置分类提供本地化。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentSkillCategorySaveRequest {
    /// Stable category identifier.
    /// 稳定分类标识。
    pub category_id: String,
    /// Category name.
    /// 分类名称。
    pub name: String,
    /// Optional category description.
    /// 可选分类说明。
    pub description: Option<String>,
    /// Category emoji.
    /// 分类图标。
    pub emoji: String,
    /// Category color.
    /// 分类颜色。
    pub color: String,
    /// Stable display order.
    /// 稳定展示顺序。
    pub order: i32,
}

/// One Agent Skill loading issue projected for diagnostics.
/// 投影给诊断界面的单个 Agent Skill 加载问题。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentSkillLoadIssueSummary {
    /// Path related to the loading issue.
    /// 与加载问题相关的路径。
    pub path: String,
    /// Human-readable diagnostic message.
    /// 人类可读诊断信息。
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Builds a minimal valid subagent save request payload for serde tests.
    /// 为 serde 测试构建最小有效智能体保存请求载荷。
    fn minimal_save_request_payload() -> serde_json::Value {
        json!({
            "agentId": "reviewer",
            "name": "Reviewer",
            "enabled": true,
            "systemPrompt": null,
            "systemPromptAppend": null,
            "allowedTools": [],
            "disabledTools": [],
            "enabledPromptSkills": [],
            "disabledPromptSkills": []
        })
    }

    /// Verifies omitted optional text fields are preserved as missing patches.
    /// 验证省略的可选文本字段会保留为缺失补丁。
    #[test]
    fn optional_text_patch_deserializes_missing_fields() {
        let request: SubagentDefinitionSaveRequest =
            serde_json::from_value(minimal_save_request_payload())
                .expect("request payload should deserialize");

        assert_eq!(request.emoji, OptionalTextPatch::Missing);
        assert_eq!(request.color, OptionalTextPatch::Missing);
        assert_eq!(request.vibe, OptionalTextPatch::Missing);
    }

    /// Verifies explicit null optional text fields become clear patches.
    /// 验证显式 null 的可选文本字段会成为清空补丁。
    #[test]
    fn optional_text_patch_deserializes_null_fields() {
        let mut payload = minimal_save_request_payload();
        payload["emoji"] = serde_json::Value::Null;
        payload["color"] = serde_json::Value::Null;
        payload["vibe"] = serde_json::Value::Null;

        let request: SubagentDefinitionSaveRequest =
            serde_json::from_value(payload).expect("request payload should deserialize");

        assert_eq!(request.emoji, OptionalTextPatch::Clear);
        assert_eq!(request.color, OptionalTextPatch::Clear);
        assert_eq!(request.vibe, OptionalTextPatch::Clear);
    }

    /// Verifies string optional text fields become set patches.
    /// 验证字符串可选文本字段会成为设置补丁。
    #[test]
    fn optional_text_patch_deserializes_string_fields() {
        let mut payload = minimal_save_request_payload();
        payload["emoji"] = json!("🧪");
        payload["color"] = json!("green");
        payload["vibe"] = json!("focused");

        let request: SubagentDefinitionSaveRequest =
            serde_json::from_value(payload).expect("request payload should deserialize");

        assert_eq!(request.emoji, OptionalTextPatch::Set(String::from("🧪")));
        assert_eq!(request.color, OptionalTextPatch::Set(String::from("green")));
        assert_eq!(
            request.vibe,
            OptionalTextPatch::Set(String::from("focused"))
        );
    }

    /// Verifies the LuaSkills configuration wire contract uses camel-case host fields.
    /// 验证 LuaSkills 配置线协议使用 camelCase 宿主字段。
    #[test]
    fn luaskill_config_descriptor_serializes_host_field_names() {
        let descriptor = LuaSkillConfigDescriptor {
            skill_id: String::from("fixture"),
            skill_version: String::from("1.0.0"),
            complete: false,
            revision: String::from("3"),
            store_scope: String::from("skills"),
            missing_count: 1,
            items: vec![LuaSkillConfigItem {
                key: String::from("api_token"),
                value_type: String::from("string"),
                sensitive: true,
                ..LuaSkillConfigItem::default()
            }],
            ..LuaSkillConfigDescriptor::default()
        };
        let value = serde_json::to_value(descriptor).expect("serialize descriptor");

        assert_eq!(value["skillId"], json!("fixture"));
        assert_eq!(value["storeScope"], json!("skills"));
        assert_eq!(value["missingCount"], json!(1));
        assert_eq!(value["items"][0]["type"], json!("string"));
        assert!(value["items"][0]["value"].is_null());
    }

    /// Verifies package configuration mutations reject fields outside the explicit RPC contract.
    /// 验证技能包配置变更会拒绝显式 RPC 契约之外的字段。
    #[test]
    fn luaskill_config_mutations_reject_unknown_fields() {
        let set_request = serde_json::from_value::<LuaSkillConfigSetRequest>(json!({
            "skillId": "fixture",
            "expectedRevision": "3",
            "values": { "endpoint": "https://example.com" },
            "includeValues": true
        }));
        let delete_request = serde_json::from_value::<LuaSkillConfigDeleteRequest>(json!({
            "skillId": "fixture",
            "key": "endpoint",
            "expectedRevision": "3",
            "deleteAll": true
        }));
        let refresh_request = serde_json::from_value::<LuaSkillConfigRefreshRequest>(json!({
            "storeScope": "skills",
            "includeValues": true
        }));

        assert!(set_request.is_err());
        assert!(delete_request.is_err());
        assert!(refresh_request.is_err());
    }
}

/// One change to a declared MCP secret of a Vulcan plugin; a null `value` clears the secret.
/// 对 Vulcan 插件一个声明 MCP 秘密的变更；`value` 为空时清除该秘密。
/// Debug output never includes the value.
/// Debug 输出绝不包含该值。
#[derive(Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VulcanPluginCredentialChange {
    /// Declaring MCP server within the plugin.
    /// 插件内声明该秘密的 MCP 服务器。
    pub server_id: String,
    /// Declared secret key.
    /// 声明的秘密键。
    pub secret_key: String,
    /// New nonempty value, or null to clear the secret.
    /// 新的非空值；为空时清除该秘密。
    pub value: Option<String>,
}

impl std::fmt::Debug for VulcanPluginCredentialChange {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("VulcanPluginCredentialChange")
            .field("server_id", &self.server_id)
            .field("secret_key", &self.secret_key)
            .field("value", &self.value.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}
