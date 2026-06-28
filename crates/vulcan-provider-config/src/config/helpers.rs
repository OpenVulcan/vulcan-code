use super::*;

pub(super) fn resolve_env_backed_secret(
    raw_value: Option<&str>,
    field_label: &str,
) -> Result<Option<String>, String> {
    let Some(trimmed_value) = raw_value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };

    let Some(env_name) = parse_env_placeholder(trimmed_value) else {
        if looks_like_env_placeholder(trimmed_value) {
            return Err(format!(
                "{field_label} 的环境变量占位符格式无效，请使用 `{{env:NAME}}`。"
            ));
        }

        return Ok(Some(trimmed_value.to_string()));
    };

    let env_value = std::env::var(env_name).map_err(|_| {
        format!("{field_label} 引用了环境变量 `{env_name}`，但当前进程中未找到该变量。")
    })?;
    let resolved_value = env_value.trim();

    if resolved_value.is_empty() {
        return Err(format!(
            "{field_label} 引用了环境变量 `{env_name}`，但该变量当前为空。"
        ));
    }

    Ok(Some(resolved_value.to_string()))
}

/// Parses one `{env:NAME}` placeholder into the referenced environment variable name.
/// 将一个 `{env:NAME}` 占位符解析为引用的环境变量名称。
pub(super) fn parse_env_placeholder(value: &str) -> Option<&str> {
    let inner = value.strip_prefix("{env:")?.strip_suffix('}')?.trim();

    if inner.is_empty() { None } else { Some(inner) }
}

/// Returns whether one value looks like an environment placeholder envelope.
/// 返回某个值是否看起来像环境变量占位符外壳。
pub(super) fn looks_like_env_placeholder(value: &str) -> bool {
    value.starts_with("{env:") && value.ends_with('}')
}

/// Returns the default boolean value used by enabled flags in persisted runtime sections.
/// 返回持久化运行时分区启用标记使用的默认布尔值。
pub(super) fn default_true() -> bool {
    true
}

/// Returns the default number of search results requested from external web-search providers.
/// 返回外部网页搜索供应商默认请求的搜索结果数量。
pub(super) fn default_web_search_max_results() -> usize {
    5
}

/// Returns whether one value still equals its type default for compact serialization.
/// 返回某个值是否仍等于其类型默认值，以便紧凑序列化。
pub(super) fn is_default<T>(value: &T) -> bool
where
    T: Default + PartialEq,
{
    *value == T::default()
}

/// Returns whether one boolean value is false for compact serialization.
/// 返回单个布尔值是否为 false，以便紧凑序列化。
pub(super) fn is_false(value: &bool) -> bool {
    !*value
}

/// Returns whether one boolean value is true for compact serialization.
/// 返回单个布尔值是否为 true，以便紧凑序列化。
pub(super) fn is_true(value: &bool) -> bool {
    *value
}

/// Returns whether one AI-tool policy is the default automatic policy.
/// 返回单个 AI 工具策略是否为默认自动策略。
pub(super) fn is_default_ai_tool_policy(policy: &AiToolPolicy) -> bool {
    *policy == AiToolPolicy::Auto
}
