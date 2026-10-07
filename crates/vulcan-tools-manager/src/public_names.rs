use super::*;
use crate::public_name::{
    allocate_unique_public_tool_name, build_direct_public_tool_name,
    build_prefixed_public_tool_name,
};

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
        | VulcanToolSourceKind::VulcanManaged
        | VulcanToolSourceKind::VulcanPlugin
        | VulcanToolSourceKind::LuaSkills
        | VulcanToolSourceKind::AiHostBinding => build_direct_public_tool_name(&descriptor.id),
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
