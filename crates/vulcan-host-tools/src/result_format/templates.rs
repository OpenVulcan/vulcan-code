use super::HostRenderOptions;
use crate::runtime_assets;
use std::collections::HashMap;
use std::sync::Arc;

/// Load template text with skill-local templates taking precedence over shared fallback templates.
/// 按“技能本地模板优先，共享模板兜底”的顺序加载模板文本。
pub(super) fn load_template_text(
    skill_name: Option<&str>,
    template_name: &str,
    render_options: &HostRenderOptions,
) -> Result<Option<Arc<str>>, String> {
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
        match super::template_cache::read(&candidate) {
            Ok(text) => return Ok(Some(text)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(format!(
                    "Failed to read tool-result template `{}`: {error}",
                    candidate.display()
                ));
            }
        }
    }
    Ok(None)
}

/// Replace placeholders in the original template without interpreting inserted values.
/// 只替换原始模板中的占位符，不再解释已经插入的值。
/// Unknown and unterminated placeholders remain literal; map iteration order is irrelevant.
/// 未知及未闭合占位符保持原文；结果与映射遍历顺序无关。
pub(super) fn render_template_text(
    template_text: &str,
    context: &HashMap<&'static str, String>,
) -> String {
    let mut rendered = String::with_capacity(template_text.len());
    visit_template(template_text, |key, literal| {
        rendered.push_str(
            key.and_then(|name| context.get(name))
                .map(String::as_str)
                .unwrap_or(literal),
        );
    });
    rendered
}

/// Visits only original template tokens; replacement values never become template source.
/// 仅访问原始模板标记；替换值绝不成为模板源码。
pub(super) fn visit_template(template: &str, mut visit: impl FnMut(Option<&str>, &str)) {
    let mut remaining = template;
    while let Some(start) = remaining.find("{{") {
        visit(None, &remaining[..start]);
        let placeholder = &remaining[start..];
        let Some(end) = placeholder[2..].find("}}") else {
            visit(None, placeholder);
            return;
        };
        visit(Some(&placeholder[2..end + 2]), &placeholder[..end + 4]);
        remaining = &placeholder[end + 4..];
    }
    visit(None, remaining);
}

pub(super) const DEFAULT_PAGE_TEMPLATE: &str = "# LARGE RESULT POINTER\n\n## Raw File\n- raw_file: {{raw_file}}\n- format: {{format}}\n- total_bytes: {{total_bytes}}\n- total_lines: {{total_lines}}\n- safe_inline_limit_bytes: {{safe_inline_limit_bytes}}\n- safe_inline_limit_lines: {{safe_inline_limit_lines}}\n- safe_read_limit_bytes: {{safe_read_limit_bytes}}\n- safe_read_limit_lines: {{safe_read_limit_lines}}\n- chunk_count: {{chunk_count}}\n\n{{host_safe_read_chunks_section}}\n\n{{read_strategy_section}}";

#[cfg(test)]
mod tests {
    use super::{HostRenderOptions, load_template_text, render_template_text};
    use std::collections::HashMap;

    /// Inserted file paths and tool text must never be parsed as another template.
    /// 插入的文件路径及工具正文绝不能被当作另一份模板解析。
    #[test]
    fn inserted_values_are_literal_in_every_context_order() {
        let entries = [
            ("raw_file", "/tmp/{{format}}.txt".to_string()),
            ("format", "markdown".to_string()),
            ("body", "literal {{raw_file}} and {{body}}".to_string()),
        ];
        for reverse in [false, true] {
            let mut context = HashMap::new();
            if reverse {
                context.extend(entries.iter().rev().cloned());
            } else {
                context.extend(entries.iter().cloned());
            }
            assert_eq!(
                render_template_text("{{raw_file}}|{{format}}|{{body}}", &context),
                "/tmp/{{format}}.txt|markdown|literal {{raw_file}} and {{body}}"
            );
        }
    }

    /// Adjacent, repeated and Unicode placeholders use the original template only.
    /// 相邻、重复及 Unicode 占位符只消费原始模板。
    #[test]
    fn repeated_and_unicode_placeholders_are_rendered_once() {
        let context = HashMap::from([("name", "世界".to_string()), ("格式", "文本".to_string())]);
        assert_eq!(
            render_template_text("你好 {{name}}{{name}} / {{格式}}", &context),
            "你好 世界世界 / 文本"
        );
    }

    /// Missing keys and incomplete placeholders are preserved without dropping source text.
    /// 缺失键及未闭合占位符保持原文，不丢弃源文本。
    #[test]
    fn unknown_and_unterminated_placeholders_remain_literal() {
        let context = HashMap::from([("known", "value".to_string())]);
        assert_eq!(
            render_template_text("{{unknown}} / {{known}} / {{unfinished", &context),
            "{{unknown}} / value / {{unfinished"
        );
        assert_eq!(render_template_text("plain text", &context), "plain text");
        assert_eq!(render_template_text("", &context), "");
        assert_eq!(render_template_text("{{}}", &context), "{{}}");
    }

    #[test]
    fn new_higher_priority_templates_override_cached_fallback_without_negative_caching() {
        let root = tempfile::tempdir().expect("isolated template roots");
        let skills = root.path().join("skills");
        let shared = root.path().join("shared");
        std::fs::create_dir_all(shared.join("overflow_templates")).expect("shared directory");
        std::fs::write(shared.join("overflow_templates/page.md"), "fallback").expect("fallback");
        let options = HostRenderOptions {
            spill_root: None,
            template_skill_roots: vec![skills.clone()],
            template_resources_root: Some(shared),
        };
        assert_eq!(
            load_template_text(Some("example"), "page.md", &options)
                .expect("load")
                .expect("fallback")
                .as_ref(),
            "fallback"
        );
        let local = skills.join("example/overflow_templates/page.md");
        std::fs::create_dir_all(local.parent().expect("parent")).expect("local root");
        std::fs::write(&local, "local").expect("local template");
        assert_eq!(
            load_template_text(Some("example"), "page.md", &options)
                .expect("load")
                .expect("local")
                .as_ref(),
            "local"
        );
        std::fs::write(&local, [0xff]).expect("invalid UTF-8");
        assert!(
            load_template_text(Some("example"), "page.md", &options).is_err(),
            "an unreadable preferred template must not silently select a fallback"
        );
    }
}
