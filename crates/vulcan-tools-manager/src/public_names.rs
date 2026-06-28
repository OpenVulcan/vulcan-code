use super::*;

pub(super) fn normalize_candidate_public_tool_names(
    candidates: Vec<VulcanToolCandidate>,
) -> Vec<VulcanToolCandidate> {
    let mut occupied_names = BTreeSet::new();
    candidates
        .into_iter()
        .map(|candidate| normalize_candidate_public_tool_name(candidate, &mut occupied_names))
        .collect()
}

/// Normalizes one candidate descriptor and preserves its invocation closure as the origin mapping.
/// 归一化单个候选工具描述，并保留其调用闭包作为真实来源映射。
fn normalize_candidate_public_tool_name(
    mut candidate: VulcanToolCandidate,
    occupied_names: &mut BTreeSet<String>,
) -> VulcanToolCandidate {
    let preferred_name = preferred_public_tool_name_for_descriptor(&candidate.descriptor);
    let public_name = if candidate.descriptor.source == VulcanToolSourceKind::LuaSkills
        && occupied_names.contains(&preferred_name)
    {
        allocate_unique_public_tool_name(
            &build_prefixed_public_tool_name("ls", &[&candidate.descriptor.id]),
            occupied_names,
        )
    } else {
        allocate_unique_public_tool_name(&preferred_name, occupied_names)
    };

    candidate.descriptor.id = public_name.clone();
    candidate.descriptor.name = public_name;
    candidate
}

/// Builds the preferred public tool name for one descriptor before collision suffixes are applied.
/// 在应用冲突后缀前为单条描述构建首选公开工具名。
fn preferred_public_tool_name_for_descriptor(descriptor: &VulcanToolDescriptor) -> String {
    match descriptor.source {
        VulcanToolSourceKind::Native
        | VulcanToolSourceKind::LuaSkills
        | VulcanToolSourceKind::AiFallback => build_direct_public_tool_name(&descriptor.id),
        VulcanToolSourceKind::Mcp => {
            let direct_name = build_direct_public_tool_name(&descriptor.id);
            if direct_name.starts_with("mcp_") {
                direct_name
            } else {
                build_prefixed_public_tool_name("mcp", &[&descriptor.id])
            }
        }
    }
}

/// Returns whether one public tool name satisfies the strict host-wide compatibility rule.
/// 返回单个公开工具名是否满足宿主统一采用的严格兼容规则。
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

/// Sanitizes one raw name fragment into a lowercase ASCII token made of letters, digits, and underscores.
/// 将单个原始名称片段规整为仅包含小写字母、数字与下划线的 ASCII token。
fn sanitize_public_tool_name_fragment(raw: &str) -> String {
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

/// Builds one direct public tool name from a raw source name while ensuring a safe leading letter.
/// 根据原始来源名称构建直接公开工具名，并确保具备安全的前导字母。
fn build_direct_public_tool_name(raw: &str) -> String {
    finalize_public_tool_name(sanitize_public_tool_name_fragment(raw))
}

/// Builds one prefixed public tool name from a mandatory prefix and ordered raw fragments.
/// 根据必选前缀与有序原始片段构建带前缀的公开工具名。
fn build_prefixed_public_tool_name(prefix: &str, fragments: &[&str]) -> String {
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

/// Allocates one unique public tool name by appending stable numeric suffixes when needed.
/// 在需要时通过追加稳定数字后缀分配唯一公开工具名。
fn allocate_unique_public_tool_name(
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

/// Finalizes one partially sanitized name into the canonical host-wide public format.
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
        String::from("tool")
    } else {
        finalized
    }
}

/// Truncates one public-tool-name base while preserving suffix budget and trimming trailing underscores.
/// 在保留后缀预算并去除尾部下划线的同时截断公开工具名基础部分。
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
