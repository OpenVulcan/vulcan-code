use super::HostRenderOptions;
use crate::runtime_assets;
use std::collections::HashMap;
use std::fs;

/// Load template text with skill-local templates taking precedence over shared fallback templates.
/// 按“技能本地模板优先，共享模板兜底”的顺序加载模板文本。
pub(super) fn load_template_text(
    skill_name: Option<&str>,
    template_name: &str,
    render_options: &HostRenderOptions,
) -> Option<String> {
    let mut candidates = Vec::new();

    if let Some(skill_name) = skill_name {
        for skill_root in &render_options.template_skill_roots {
            candidates.push(
                skill_root
                    .join(skill_name)
                    .join("overflow_templates")
                    .join(template_name),
            );
        }
    }

    if let Some(resources_root) = render_options
        .template_resources_root
        .clone()
        .or_else(|| Some(runtime_assets::resolve_app_resources_directory()))
    {
        candidates.push(
            resources_root
                .join("overflow_templates")
                .join(template_name),
        );
    }

    for candidate in candidates {
        if candidate.exists()
            && let Ok(text) = fs::read_to_string(&candidate)
        {
            return Some(text);
        }
    }

    None
}

/// Replace simple `{{name}}` placeholders with host-provided context values.
/// 使用宿主提供的上下文值替换简单的 `{{name}}` 占位符。
pub(super) fn render_template_text(
    template_text: &str,
    context: &HashMap<&'static str, String>,
) -> String {
    let mut rendered = template_text.to_string();
    for (key, value) in context {
        rendered = rendered.replace(&format!("{{{{{}}}}}", key), value);
    }
    rendered
}

/// Render the built-in fallback page template when no external template is available.
/// 当外部模板不可用时，渲染内建兜底分页模板。
pub(super) fn render_page_default(context: &HashMap<&'static str, String>) -> String {
    render_template_text(
        "# LARGE RESULT POINTER\n\n## Raw File\n- raw_file: {{raw_file}}\n- format: {{format}}\n- total_bytes: {{total_bytes}}\n- total_lines: {{total_lines}}\n- safe_inline_limit_bytes: {{safe_inline_limit_bytes}}\n- safe_inline_limit_lines: {{safe_inline_limit_lines}}\n- safe_read_limit_bytes: {{safe_read_limit_bytes}}\n- safe_read_limit_lines: {{safe_read_limit_lines}}\n- chunk_count: {{chunk_count}}\n\n{{host_safe_read_chunks_section}}\n\n{{read_strategy_section}}",
        context,
    )
}
