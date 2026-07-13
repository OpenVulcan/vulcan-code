use super::*;

/// Legacy RTK settings that can be moved into the `vulcan-rtk` plugin once.
/// 可一次性迁移到 `vulcan-rtk` 插件的旧 RTK 设置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyRtkPluginConfiguration {
    /// Complete plugin configuration produced from the exact legacy fields.
    /// 根据精确旧字段生成的完整插件配置。
    pub plugin_config: Value,
}

/// Legacy RTK routing mode serialized by the former host-owned configuration.
/// 旧宿主持有配置序列化的 RTK 路由模式。
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum LegacyRtkRouteMode {
    /// Disable every RTK command route.
    /// 禁用全部 RTK 命令路由。
    Disabled,
    /// Enable only explicitly selected routes.
    /// 仅启用显式选择的路由。
    #[default]
    Selected,
    /// Enable every route except explicitly disabled routes.
    /// 启用除显式禁用路由以外的全部路由。
    All,
}

/// Exact persisted shape of the retired host-owned RTK settings.
/// 已退役宿主持有 RTK 设置的精确持久化结构。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyRtkToolConfiguration {
    /// Whether command routing was enabled.
    /// 命令路由此前是否启用。
    #[serde(default = "legacy_rtk_enabled_default")]
    enabled: bool,
    /// Former route selection mode.
    /// 原路由选择模式。
    #[serde(default)]
    route_mode: LegacyRtkRouteMode,
    /// Former explicitly enabled routes.
    /// 原显式启用路由集合。
    #[serde(default = "legacy_rtk_enabled_routes_default")]
    enabled_routes: BTreeSet<String>,
    /// Former explicitly disabled routes.
    /// 原显式禁用路由集合。
    #[serde(default)]
    disabled_routes: BTreeSet<String>,
    /// Whether telemetry was disabled.
    /// 遥测此前是否禁用。
    #[serde(default = "legacy_rtk_telemetry_disabled_default")]
    telemetry_disabled: bool,
}

impl Default for LegacyRtkToolConfiguration {
    /// Builds the exact defaults used by the retired host RTK settings.
    /// 构建已退役宿主 RTK 设置使用的精确默认值。
    fn default() -> Self {
        Self {
            enabled: legacy_rtk_enabled_default(),
            route_mode: LegacyRtkRouteMode::default(),
            enabled_routes: legacy_rtk_enabled_routes_default(),
            disabled_routes: BTreeSet::new(),
            telemetry_disabled: legacy_rtk_telemetry_disabled_default(),
        }
    }
}

/// Reads the exact retired RTK fields without mutating the provider configuration file.
/// 在不修改供应商配置文件的情况下读取精确的已退役 RTK 字段。
///
/// # Returns
/// 返回值
/// Returns a complete plugin configuration only when at least one retired field exists.
/// 仅当至少一个已退役字段存在时返回完整插件配置。
pub fn load_legacy_rtk_plugin_configuration() -> Result<Option<LegacyRtkPluginConfiguration>, String>
{
    let path = configuration_file_path();
    if !path.exists() {
        return Ok(None);
    }
    let text =
        fs::read_to_string(&path).map_err(|error| format!("读取旧 RTK 配置文件失败：{error}。"))?;
    let root: Value = serde_json::from_str(&text)
        .map_err(|error| format!("解析旧 RTK 配置文件失败：{error}。"))?;
    legacy_rtk_plugin_configuration_from_value(&root)
}

/// Converts only the exact retired fields from one parsed provider configuration value.
/// 仅转换一份已解析供应商配置中的精确已退役字段。
fn legacy_rtk_plugin_configuration_from_value(
    root: &Value,
) -> Result<Option<LegacyRtkPluginConfiguration>, String> {
    let root_object = root
        .as_object()
        .ok_or_else(|| String::from("旧 RTK 配置迁移要求根配置为 JSON 对象。"))?;
    let legacy_rtk_value = root_object
        .get("hostTools")
        .and_then(Value::as_object)
        .and_then(|host_tools| host_tools.get("rtk"));
    let legacy_git_flag = root_object
        .get("gitCommit")
        .and_then(Value::as_object)
        .and_then(|git_commit| git_commit.get("useRtkSummary"));
    if legacy_rtk_value.is_none() && legacy_git_flag.is_none() {
        return Ok(None);
    }

    let legacy_rtk = legacy_rtk_value
        .cloned()
        .map(serde_json::from_value::<LegacyRtkToolConfiguration>)
        .transpose()
        .map_err(|error| format!("解析旧 RTK 路由配置失败：{error}。"))?
        .unwrap_or_default();
    let git_commit_context_enabled = legacy_git_flag
        .map(|value| {
            value
                .as_bool()
                .ok_or_else(|| String::from("旧字段 gitCommit.useRtkSummary 必须是布尔值。"))
        })
        .transpose()?
        .unwrap_or(true);
    let route_mode = match legacy_rtk.route_mode {
        LegacyRtkRouteMode::Disabled => "disabled",
        LegacyRtkRouteMode::Selected => "selected",
        LegacyRtkRouteMode::All => "all",
    };
    Ok(Some(LegacyRtkPluginConfiguration {
        plugin_config: json!({
            "enabled": legacy_rtk.enabled,
            "routeMode": route_mode,
            "enabledRoutes": legacy_rtk.enabled_routes,
            "disabledRoutes": legacy_rtk.disabled_routes,
            "telemetryDisabled": legacy_rtk.telemetry_disabled,
            "gitCommitContextEnabled": git_commit_context_enabled,
        }),
    }))
}

/// Removes only the exact retired RTK fields after plugin configuration is durable.
/// 在插件配置持久化后仅移除精确的已退役 RTK 字段。
///
/// # Returns
/// 返回值
/// Returns `true` when the provider configuration file was rewritten.
/// 当供应商配置文件被重写时返回 `true`。
pub fn remove_legacy_rtk_configuration_fields() -> Result<bool, String> {
    let path = configuration_file_path();
    if !path.exists() {
        return Ok(false);
    }
    let text =
        fs::read_to_string(&path).map_err(|error| format!("读取旧 RTK 配置文件失败：{error}。"))?;
    let mut root: Value = serde_json::from_str(&text)
        .map_err(|error| format!("解析旧 RTK 配置文件失败：{error}。"))?;
    let changed = remove_legacy_rtk_fields_from_value(&mut root)?;
    if !changed {
        return Ok(false);
    }
    let encoded = serde_json::to_vec_pretty(&root)
        .map_err(|error| format!("序列化已清理的供应商配置失败：{error}。"))?;
    crate::atomic_file::write_file_atomically(&path, &encoded)
        .map_err(|error| format!("写入已清理的供应商配置失败：{error}。"))?;
    Ok(true)
}

/// Removes only the two exact retired RTK paths from one parsed configuration value.
/// 仅从一份已解析配置值中移除两条精确的已退役 RTK 路径。
fn remove_legacy_rtk_fields_from_value(root: &mut Value) -> Result<bool, String> {
    let root_object = root
        .as_object_mut()
        .ok_or_else(|| String::from("旧 RTK 配置清理要求根配置为 JSON 对象。"))?;
    // changed records whether either exact retired leaf was removed.
    // changed 记录是否移除了任一精确已退役叶节点。
    let mut changed = false;
    if let Some(host_tools) = root_object
        .get_mut("hostTools")
        .and_then(Value::as_object_mut)
    {
        changed |= host_tools.remove("rtk").is_some();
        if host_tools.is_empty() {
            root_object.remove("hostTools");
        }
    }
    if let Some(git_commit) = root_object
        .get_mut("gitCommit")
        .and_then(Value::as_object_mut)
    {
        changed |= git_commit.remove("useRtkSummary").is_some();
        if git_commit.is_empty() {
            root_object.remove("gitCommit");
        }
    }
    Ok(changed)
}

/// Returns the former default enabled state.
/// 返回原默认启用状态。
const fn legacy_rtk_enabled_default() -> bool {
    true
}

/// Returns the former default telemetry-disabled state.
/// 返回原默认遥测禁用状态。
const fn legacy_rtk_telemetry_disabled_default() -> bool {
    true
}

/// Returns the exact former selected-route defaults.
/// 返回精确的原精选路由默认值。
fn legacy_rtk_enabled_routes_default() -> BTreeSet<String> {
    [
        "cargo", "find", "git", "go", "grep", "jest", "ls", "mypy", "npm", "npx", "pnpm", "pytest",
        "read", "ruff", "tree", "tsc", "vitest", "wc",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Confirms the private migration shape accepts the exact historical field names.
    /// 确认私有迁移结构接受精确的历史字段名。
    #[test]
    fn legacy_rtk_shape_uses_exact_historical_names() {
        let parsed: LegacyRtkToolConfiguration = serde_json::from_value(json!({
            "enabled": false,
            "routeMode": "all",
            "enabledRoutes": ["cargo"],
            "disabledRoutes": ["git"],
            "telemetryDisabled": false
        }))
        .expect("legacy RTK settings should parse");
        assert!(!parsed.enabled);
        assert!(matches!(parsed.route_mode, LegacyRtkRouteMode::All));
        assert_eq!(
            parsed.enabled_routes,
            BTreeSet::from([String::from("cargo")])
        );
        assert_eq!(
            parsed.disabled_routes,
            BTreeSet::from([String::from("git")])
        );
        assert!(!parsed.telemetry_disabled);
    }

    /// Confirms migration reads exact legacy leaves and preserves unrelated sibling values.
    /// 确认迁移读取精确旧叶节点并保留无关同级值。
    #[test]
    fn legacy_rtk_value_migration_is_exact_and_non_destructive() {
        let mut root = json!({
            "hostTools": {
                "rtk": { "enabled": false, "routeMode": "all" },
                "nativeTools": { "shell_command": true }
            },
            "gitCommit": {
                "useRtkSummary": false,
                "extraPrompt": "keep"
            },
            "unrelated": { "value": 7 }
        });
        let migrated = legacy_rtk_plugin_configuration_from_value(&root)
            .expect("migrate parsed legacy RTK value")
            .expect("legacy RTK fields should exist");
        assert_eq!(migrated.plugin_config["enabled"], false);
        assert_eq!(migrated.plugin_config["routeMode"], "all");
        assert_eq!(migrated.plugin_config["gitCommitContextEnabled"], false);
        assert!(remove_legacy_rtk_fields_from_value(&mut root).expect("remove legacy fields"));
        assert!(root["hostTools"].get("rtk").is_none());
        assert_eq!(root["hostTools"]["nativeTools"]["shell_command"], true);
        assert!(root["gitCommit"].get("useRtkSummary").is_none());
        assert_eq!(root["gitCommit"]["extraPrompt"], "keep");
        assert_eq!(root["unrelated"]["value"], 7);
    }
}
