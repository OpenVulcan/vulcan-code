//! Behavioral coverage for the fixed content-draft grammar and its safety budgets.
//! 固定内容草稿语法及其安全预算的行为覆盖。

use super::*;

/// Every built-in template accepts all registered components and emits self-contained local controls.
/// 每种内置模板都接受全部已注册组件，并生成自包含的局部控件。
#[test]
fn html_answers_templates_render_all_components() {
    let source = include_str!("example.md");
    for template in [
        HtmlAnswerTemplate::Document,
        HtmlAnswerTemplate::Dashboard,
        HtmlAnswerTemplate::Comparison,
    ] {
        let html = render_draft(template, "示例", source).unwrap();
        for marker in [
            "<dl",
            "<meter",
            "class=timeline",
            "class=flow",
            "<aside",
            "<details",
            "role=tablist",
            "role=tabpanel",
            "<table>",
        ] {
            assert!(
                html.contains(marker),
                "Missing generated behavior: {marker}"
            );
        }
        assert!(html.contains(&format!("data-template=\"{}\"", template.name())));
        assert!(
            source.len() < html.len(),
            "The draft must omit renderer boilerplate"
        );
    }
}

/// Raw markup and labels remain data; no model-authored script, resource or navigation is introduced.
/// 原始标记与标签保持数据属性；不引入模型生成的脚本、资源或导航。
#[test]
fn html_answers_templates_escape_model_content() {
    let html = render_draft(HtmlAnswerTemplate::Document, "<script>x</script>", "<script>alert(1)</script>\n\n[click](javascript:alert(2))\n\n![image](https://example.invalid/a)\n\n```ui-kv\n<svg onload=x> | \" onclick=\"bad\n```\n").unwrap();
    assert!(!html.contains("<svg"));
    assert!(!html.contains("<img"));
    assert!(!html.contains("<a "));
    assert!(!html.contains("<script>alert"));
    assert!(html.contains("&lt;script&gt;"));
    assert!(html.contains("&quot; onclick=&quot;bad"));
    let references = render_draft(
        HtmlAnswerTemplate::Dashboard,
        "x",
        "[reference][r]\n\n## Later\n\n[r]: https://example.invalid/data\n",
    )
    .unwrap();
    assert!(
        references.contains("class=reference title=\"https://example.invalid/data\""),
        "Markdown references must survive panel boundaries"
    );
}

/// Unknown grammar, malformed rows, impossible metrics and incomplete/nested component fences fail.
/// 未知语法、非法行、不可能的指标及不完整或嵌套组件围栏均失败。
#[test]
fn html_answers_templates_reject_invalid_components() {
    for source in [
        "```ui-unknown\nx\n```",
        "```ui-kv columns=3\na | b\n```",
        "```ui-kv\na\n```",
        "```ui-kv\na | b | c\n```",
        "```ui-kv\na | b",
        "```ui-metrics\na | NaN | 2 | ms\n```",
        "```ui-metrics\na | 3 | 2 | ms\n```",
        "```ui-metrics\na | 0 | 0 | ms\n```",
        "```ui-metrics\na | 1 | inf | ms\n```",
        "```ui-flow\nA\n```",
        "```ui-flow\nA -> \n```",
        "```ui-flow TB\nA -> B\n```",
        "```ui-flow\n+ A\n```",
        "```ui-flow\nA -> {}\n```",
        "```ui-sequence\nnote A: only a note\n```",
        "```ui-sequence\nA - B: no arrow\n```",
        "```ui-sequence\n== ==\nA -> B\n```",
        "```ui-sequence verbose\nA -> B\n```",
        "```ui-tree list\nroot\n```",
        "```ui-tree\nroot |\n```",
        "```ui-timeline\nnow | then\n```",
        "```ui-callout warn Heading\ntext\n```",
        "```ui-details\ntext\n```",
        "```ui-tabs Group\nmissing heading\n```",
        "```ui-tabs Group\n### First\n```",
        "```ui-tabs Group\nintro\n### First\nbody\n```",
        "> ```ui-kv\n> a | b\n> ```",
        "````ui-details More\n```ui-kv\na | b\n```\n````",
    ] {
        assert_eq!(
            render_draft(HtmlAnswerTemplate::Document, "x", source).err(),
            Some(HtmlAnswerUnavailableReason::InvalidDraft),
            "Accepted invalid component: {source}"
        );
    }
}

/// Counts and byte limits reject bounded excess while ordinary code fences and Unicode stay readable.
/// 数量和字节限制拒绝有界超额，而普通代码围栏及 Unicode 保持可读。
#[test]
fn html_answers_templates_enforce_budgets() {
    for source in [
        format!(
            "```ui-kv\n{}```",
            "key | value\n".repeat(DRAFT_MAX_ITEMS + 1)
        ),
        "```ui-kv\na | b\n```\n\n".repeat(DRAFT_MAX_BLOCKS + 1),
        "## Section\n\nbody\n\n".repeat(DRAFT_MAX_BLOCKS + 1),
    ] {
        assert!(render_draft(HtmlAnswerTemplate::Document, "x", &source).is_err());
    }
    assert_eq!(
        render_draft(
            HtmlAnswerTemplate::Document,
            "x",
            &"x".repeat(HTML_ANSWER_MAX_PAGE_BYTES + 1)
        )
        .err(),
        Some(HtmlAnswerUnavailableReason::PageTooLarge)
    );
    assert_eq!(
        render_draft(
            HtmlAnswerTemplate::Document,
            "x",
            &"&".repeat(HTML_ANSWER_MAX_PAGE_BYTES)
        )
        .err(),
        Some(HtmlAnswerUnavailableReason::PageTooLarge)
    );
    let html = render_draft(
        HtmlAnswerTemplate::Document,
        "🐉",
        "```rust\nlet x = 1;\n```\n\n普通文字",
    )
    .unwrap();
    assert!(html.contains("language-rust"));
    assert!(html.contains("普通文字"));
}

/// Independent tab components have unique ARIA bindings and one initial selected panel per group.
/// 独立页签组件具有唯一 ARIA 绑定，且每组只有一个初始选中面板。
#[test]
fn html_answers_templates_tabs_have_stable_local_identity() {
    let source = "```ui-tabs Choice\n### First\nAlpha\n### Second\nBeta\n```\n\n".repeat(2);
    let html = render_draft(HtmlAnswerTemplate::Comparison, "x", &source).unwrap();
    for id in [
        "t1-0", "t1-1", "t2-0", "t2-1", "p1-0", "p1-1", "p2-0", "p2-1",
    ] {
        assert_eq!(html.matches(&format!("id=\"{id}\"")).count(), 1);
    }
    assert_eq!(html.matches("aria-selected=\"true\"").count(), 2);
    assert_eq!(html.matches(" hidden>").count(), 2);
    let heading = render_draft(
        HtmlAnswerTemplate::Document,
        "x",
        "```ui-tabs Languages\n### **C#**\ncontent\n```",
    )
    .unwrap();
    assert!(heading.contains(">C#</button>"));
}
