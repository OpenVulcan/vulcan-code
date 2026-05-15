//! Shared public tool-name normalization helpers used by host-managed tool surfaces.
//! 宿主管理工具面对外工具名标准化所复用的共享辅助逻辑。

use std::collections::BTreeSet;

/// Maximum public tool-name length kept compatible with the strictest currently targeted provider families.
/// 与当前目标中最严格 Provider 家族兼容的公开工具名最大长度。
pub const MAX_PUBLIC_TOOL_NAME_LEN: usize = 64;

/// Returns whether one public tool name already satisfies the current strict host-wide compatibility rule.
/// 返回单个公开工具名是否已经满足当前宿主统一采用的严格兼容规则。
pub fn public_tool_name_is_valid(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    if !first.is_ascii_lowercase() || name.len() > MAX_PUBLIC_TOOL_NAME_LEN {
        return false;
    }

    chars.all(|character| {
        character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
    })
}

/// Sanitizes one raw name fragment into a conservative lowercase ASCII token made of letters, digits, and underscores only.
/// 将单个原始名称片段规整为仅包含小写字母、数字与下划线的保守 ASCII token。
pub fn sanitize_public_tool_name_fragment(raw: &str) -> String {
    let mut sanitized = String::new();
    let mut previous_was_underscore = false;

    for character in raw.chars() {
        let normalized = if character.is_ascii_alphanumeric() {
            character.to_ascii_lowercase()
        } else {
            '_'
        };

        if normalized == '_' {
            if sanitized.is_empty() || previous_was_underscore {
                continue;
            }
            previous_was_underscore = true;
            sanitized.push(normalized);
            continue;
        }

        previous_was_underscore = false;
        sanitized.push(normalized);
    }

    sanitized.trim_matches('_').to_string()
}

/// Builds one direct public tool name from a raw source name while ensuring a leading ASCII letter and a bounded final length.
/// 根据原始来源名称构建单个直接公开工具名，并确保首字符为 ASCII 字母且最终长度受限。
pub fn build_direct_public_tool_name(raw: &str) -> String {
    finalize_public_tool_name(sanitize_public_tool_name_fragment(raw))
}

/// Builds one prefixed public tool name from one mandatory prefix and one ordered fragment list.
/// 根据必选前缀与有序片段列表构建单个带前缀的公开工具名。
pub fn build_prefixed_public_tool_name(prefix: &str, fragments: &[&str]) -> String {
    let mut parts = Vec::new();

    let prefix_fragment = sanitize_public_tool_name_fragment(prefix);
    if !prefix_fragment.is_empty() {
        parts.push(prefix_fragment);
    }

    for fragment in fragments {
        let sanitized = sanitize_public_tool_name_fragment(fragment);
        if !sanitized.is_empty() {
            parts.push(sanitized);
        }
    }

    finalize_public_tool_name(parts.join("_"))
}

/// Allocates one unique public tool name by appending stable numeric suffixes when the preferred base name is already occupied.
/// 当首选基础名称已被占用时，通过追加稳定数字后缀分配一个唯一公开工具名。
pub fn allocate_unique_public_tool_name(
    preferred_base_name: &str,
    occupied_names: &mut BTreeSet<String>,
) -> String {
    let base_name = finalize_public_tool_name(preferred_base_name.to_string());
    if occupied_names.insert(base_name.clone()) {
        return base_name;
    }

    for index in 1usize.. {
        let suffix = format!("_{index}");
        let truncated_base = truncate_public_tool_name_base(&base_name, suffix.len());
        let candidate = format!("{truncated_base}{suffix}");
        if occupied_names.insert(candidate.clone()) {
            return candidate;
        }
    }

    unreachable!("公开工具名分配器应始终能找到可用编号后缀。");
}

/// Finalizes one partially sanitized name into the host-wide canonical public format.
/// 将一段部分规整的名称最终收敛为宿主统一使用的规范公开格式。
fn finalize_public_tool_name(candidate: String) -> String {
    if public_tool_name_is_valid(&candidate) {
        return candidate;
    }

    let mut finalized = candidate;
    if finalized.is_empty() {
        finalized = String::from("tool");
    }

    if !finalized
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_lowercase())
    {
        finalized = format!("tool_{finalized}");
    }

    if finalized.len() > MAX_PUBLIC_TOOL_NAME_LEN {
        finalized = truncate_public_tool_name_base(&finalized, 0);
    }

    if finalized.is_empty() {
        return String::from("tool");
    }

    finalized
}

/// Truncates one canonical public-tool-name base while preserving a safe suffix budget and trimming trailing underscores.
/// 在保留安全后缀预算的同时截断单个规范公开工具名基础部分，并去掉尾部下划线。
fn truncate_public_tool_name_base(base_name: &str, reserved_suffix_len: usize) -> String {
    let maximum_base_len = MAX_PUBLIC_TOOL_NAME_LEN.saturating_sub(reserved_suffix_len);
    let mut truncated = base_name
        .chars()
        .take(maximum_base_len.max(1))
        .collect::<String>();

    while truncated.ends_with('_') {
        truncated.pop();
    }

    if truncated.is_empty() {
        String::from("tool")
    } else {
        truncated
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_PUBLIC_TOOL_NAME_LEN, allocate_unique_public_tool_name, build_direct_public_tool_name,
        build_prefixed_public_tool_name, public_tool_name_is_valid,
        sanitize_public_tool_name_fragment,
    };
    use std::collections::BTreeSet;

    /// Verifies that the shared validator accepts the host-wide lowercase underscore naming contract only.
    /// 验证共享校验器仅接受宿主统一采用的小写下划线命名契约。
    #[test]
    fn public_tool_name_validator_accepts_only_strict_safe_names() {
        assert!(public_tool_name_is_valid("mcp_server_read_file"));
        assert!(!public_tool_name_is_valid("workspace.describe"));
        assert!(!public_tool_name_is_valid("ReadFile"));
        assert!(!public_tool_name_is_valid("1tool"));
    }

    /// Verifies that raw fragments are normalized into lowercase underscore-separated ASCII tokens.
    /// 验证原始片段会被规整为小写下划线分隔的 ASCII token。
    #[test]
    fn sanitize_public_tool_name_fragment_collapses_invalid_sequences() {
        assert_eq!(
            sanitize_public_tool_name_fragment("GitHub.com/server::list-issues"),
            "github_com_server_list_issues"
        );
        assert_eq!(sanitize_public_tool_name_fragment("---"), "");
    }

    /// Verifies that direct names gain a safe leading token automatically when the raw source starts with digits.
    /// 验证当原始来源以数字开头时，直接名称会自动获得安全前导 token。
    #[test]
    fn direct_public_tool_name_recovers_leading_letter() {
        assert_eq!(
            build_direct_public_tool_name("9patch-tool"),
            "tool_9patch_tool"
        );
    }

    /// Verifies that prefixed names keep the requested prefix and remain compatible after normalization.
    /// 验证带前缀名称会保留请求前缀，并在规整后继续保持兼容。
    #[test]
    fn prefixed_public_tool_name_preserves_prefix() {
        let name = build_prefixed_public_tool_name("mcp", &["server/a", "read-file"]);
        assert_eq!(name, "mcp_server_a_read_file");
        assert!(public_tool_name_is_valid(&name));
    }

    /// Verifies that duplicate allocation uses readable numbered suffixes and still respects the length cap.
    /// 验证重复分配会使用可读数字后缀，并且依旧遵守长度上限。
    #[test]
    fn unique_public_tool_name_allocator_uses_numbered_suffixes() {
        let mut occupied = BTreeSet::new();
        let base_name = "mcp_server_read_file";

        assert_eq!(
            allocate_unique_public_tool_name(base_name, &mut occupied),
            "mcp_server_read_file"
        );
        assert_eq!(
            allocate_unique_public_tool_name(base_name, &mut occupied),
            "mcp_server_read_file_1"
        );

        let long_name = "a".repeat(MAX_PUBLIC_TOOL_NAME_LEN + 32);
        let first = allocate_unique_public_tool_name(&long_name, &mut occupied);
        let second = allocate_unique_public_tool_name(&long_name, &mut occupied);
        assert!(first.len() <= MAX_PUBLIC_TOOL_NAME_LEN);
        assert!(second.len() <= MAX_PUBLIC_TOOL_NAME_LEN);
        assert!(second.ends_with("_1"));
    }
}
