use crate::composer::response::PromptAssemblyOutput;
use crate::source::hashing::stable_text_hash;

/// Compute one stable assembly fingerprint from the final output.
/// 根据最终输出计算稳定装配指纹。
pub fn compute_assembly_fingerprint(output: &PromptAssemblyOutput) -> u64 {
    stable_text_hash(&format!(
        "{}\n---\n{}\n---\n{}\n---\n{}",
        output.render_system_text(),
        output.render_developer_text(),
        output.render_user_text(),
        output
            .tool_metadata_segments
            .iter()
            .map(|segment| segment.body.as_str())
            .collect::<Vec<_>>()
            .join("\n\n")
    ))
}
