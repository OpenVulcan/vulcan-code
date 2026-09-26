//! Final byte and line accounting includes all template expansion, before allocation.
//! 在分配之前核算最终字节与行数，涵盖全部模板展开。

use super::templates::{render_template_text, visit_template};
use super::{EffectiveBudgetScope, TOOL_OUTPUT_EXCEEDS_LIMIT_ERROR};
use std::collections::HashMap;

/// Measures actual text instead of trusting caller-supplied byte/line projections.
/// 核算真实文本，不依赖调用方提供的字节与行数投影。
pub(super) fn fits(text: &str, budget: &EffectiveBudgetScope) -> bool {
    fits_counts(
        text.len(),
        text.bytes().filter(|byte| *byte == b'\n').count(),
        budget,
    )
}

fn fits_counts(bytes: usize, newlines: usize, budget: &EffectiveBudgetScope) -> bool {
    u64::try_from(bytes).is_ok_and(|bytes| bytes <= budget.bytes)
        && (budget.lines <= 0
            || (if bytes == 0 {
                0
            } else {
                newlines.saturating_add(1)
            }) as u128
                <= budget.lines as u128)
}

/// Rejects oversized templates before copying replacement values into the output.
/// 在将替换值复制到输出之前拒绝超大模板。
pub(super) fn render(
    template: &str,
    context: &HashMap<&'static str, String>,
    budget: &EffectiveBudgetScope,
) -> Result<String, String> {
    let mut bytes = Some(0usize);
    let mut newlines = 0usize;
    visit_template(template, |key, literal| {
        let value = key
            .and_then(|name| context.get(name))
            .map(String::as_str)
            .unwrap_or(literal);
        bytes = bytes.and_then(|count| count.checked_add(value.len()));
        newlines = newlines.saturating_add(value.bytes().filter(|byte| *byte == b'\n').count());
    });
    if !bytes.is_some_and(|bytes| fits_counts(bytes, newlines, budget)) {
        return Err(TOOL_OUTPUT_EXCEEDS_LIMIT_ERROR.into());
    }
    Ok(render_template_text(template, context))
}

/// Selects a source prefix in one pass, reserving repeated placeholders and all fixed markup.
/// 单次遍历选择源文本前缀，为重复占位符及所有固定标记预留预算。
pub(super) fn truncate(
    source: &str,
    template: &str,
    context: &HashMap<&'static str, String>,
    budget: &EffectiveBudgetScope,
) -> Result<String, String> {
    let mut fixed_bytes = Some(0usize);
    let mut fixed_newlines = 0usize;
    let mut copies = 0usize;
    visit_template(template, |key, literal| {
        if key == Some("truncated_content") {
            copies += 1;
        } else {
            let value = key
                .and_then(|name| context.get(name))
                .map(String::as_str)
                .unwrap_or(literal);
            fixed_bytes = fixed_bytes.and_then(|count| count.checked_add(value.len()));
            fixed_newlines =
                fixed_newlines.saturating_add(value.bytes().filter(|byte| *byte == b'\n').count());
        }
    });
    let Some(fixed_bytes) = fixed_bytes else {
        return Err(TOOL_OUTPUT_EXCEEDS_LIMIT_ERROR.into());
    };
    let mut accepted = None;
    let mut prefix_bytes = 0usize;
    for (index, line) in source.split('\n').enumerate() {
        prefix_bytes = prefix_bytes
            .saturating_add(usize::from(index > 0))
            .saturating_add(line.len());
        let Some(bytes) = prefix_bytes
            .checked_mul(copies)
            .and_then(|bytes| bytes.checked_add(fixed_bytes))
        else {
            break;
        };
        let newlines = index.saturating_mul(copies).saturating_add(fixed_newlines);
        if !fits_counts(bytes, newlines, budget) {
            break;
        }
        accepted = Some(prefix_bytes);
    }
    accepted
        .map(|bytes| source[..bytes].to_owned())
        .ok_or_else(|| TOOL_OUTPUT_EXCEEDS_LIMIT_ERROR.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_markup_and_repeated_values_are_counted() {
        let template = "[{{truncated_content}}]\n{{truncated_content}} {{truncate_notice}}";
        let mut context = HashMap::from([("truncate_notice", "end".to_owned())]);
        for bytes in 0..48 {
            for lines in [1, 2, 3, 4, -1] {
                let budget = EffectiveBudgetScope { bytes, lines };
                match truncate(
                    "中\nx\nyyyyyyyyyyyyyyyyyyyyyyyyyyyy",
                    template,
                    &context,
                    &budget,
                ) {
                    Ok(prefix) => {
                        context.insert("truncated_content", prefix);
                        let output = render(template, &context, &budget)
                            .expect("selected prefix fits final expansion");
                        assert!(fits(&output, &budget));
                    }
                    Err(_) => {
                        context.insert("truncated_content", "中".into());
                        assert!(render(template, &context, &budget).is_err());
                    }
                }
            }
        }
    }

    #[test]
    fn inserted_placeholders_and_utf8_are_literal_for_size_accounting() {
        let context = HashMap::from([
            ("raw_file", "中/{{format}}".into()),
            ("format", "md".into()),
        ]);
        let expected = "中/{{format}} md";
        let budget = EffectiveBudgetScope {
            bytes: expected.len() as u64,
            lines: 1,
        };
        assert_eq!(
            render("{{raw_file}} {{format}}", &context, &budget).expect("exact bound"),
            expected
        );
        assert!(
            render(
                "{{raw_file}} {{format}}",
                &context,
                &EffectiveBudgetScope {
                    bytes: budget.bytes - 1,
                    lines: 1
                }
            )
            .is_err()
        );
    }
}
