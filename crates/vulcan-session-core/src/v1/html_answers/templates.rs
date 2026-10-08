//! Version-two content drafts: deterministic, offline templates with bounded, escaped components.
//! 版本二内容草稿：确定性的离线模板，包含有界且经过转义的组件。

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd, html};
use serde::Deserialize;

use super::{HTML_ANSWER_MAX_PAGE_BYTES, HTML_ANSWER_MAX_TITLE_CHARS, HtmlAnswerUnavailableReason};

/// Maximum authored sections or component blocks per draft.
/// 每份草稿允许的最大作者章节或组件块数量。
pub const DRAFT_MAX_BLOCKS: usize = 64;
/// Maximum rows, graph nodes or tabs in a single component.
/// 单个组件允许的最大行、流程节点或页签数量。
pub const DRAFT_MAX_ITEMS: usize = 128;
/// Maximum parsed Markdown events across the complete draft, including component bodies.
/// 完整草稿（含组件正文）允许的 Markdown 解析事件总数。
const DRAFT_MAX_EVENTS: usize = 16_384;
/// Generated markup budget, derived from the canonical source budget to bound expansion.
/// 从权威源码预算派生的生成标记预算，用于限制展开量。
pub const DRAFT_MAX_RENDERED_BYTES: usize = HTML_ANSWER_MAX_PAGE_BYTES * 4;

/// Fixed version-two layout; later incompatible renderers must use a new protocol version.
/// 固定的版本二布局；后续不兼容渲染器必须使用新协议版本。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HtmlAnswerTemplate {
    /// Linear explanation with a comfortable reading measure.
    /// 使用适宜阅读宽度的线性说明。
    Document,
    /// Responsive overview cards, one per level-two heading.
    /// 响应式概览卡片，每个二级标题对应一张卡片。
    Dashboard,
    /// Side-by-side sections with wide comparison tables.
    /// 并排章节及宽幅对比表格。
    Comparison,
}

impl HtmlAnswerTemplate {
    /// Return the stable wire and CSS layout identifier for this template.
    /// 返回此模板稳定的协议名称及 CSS 布局标识。
    pub fn name(self) -> &'static str {
        match self {
            Self::Document => "document",
            Self::Dashboard => "dashboard",
            Self::Comparison => "comparison",
        }
    }
}

/// Mutable per-render counters; never shared across answers or persisted with a conversation.
/// 每次渲染的可变计数器；不跨回答共享，也不随会话持久化。
#[derive(Default)]
struct RenderBudget {
    /// Total custom component count, also used for collision-free local element IDs.
    /// 自定义组件总数，同时用于生成无冲突的页面局部元素标识。
    components: usize,
    /// Total consumed Markdown events, including nested textual content.
    /// 已消费的 Markdown 事件总数，包含嵌套文字内容。
    events: usize,
}

/// Compile `source` using the declared template and plain-text title, returning bounded standalone HTML.
/// 根据声明的模板和纯文本标题编译 `source`，返回有界的独立 HTML。
/// All content remains data: only bundled styles/scripts execute; malformed drafts return stable errors.
/// 全部内容保持数据属性：只有内置样式和脚本执行；非法草稿返回稳定错误。
pub fn render_draft(
    template: HtmlAnswerTemplate,
    title: &str,
    source: &str,
) -> Result<String, HtmlAnswerUnavailableReason> {
    if source.len() > HTML_ANSWER_MAX_PAGE_BYTES {
        return Err(HtmlAnswerUnavailableReason::PageTooLarge);
    }
    if source.trim().is_empty() {
        return Err(HtmlAnswerUnavailableReason::EmptySource);
    }
    if title.trim().is_empty()
        || title.chars().count() > HTML_ANSWER_MAX_TITLE_CHARS
        || title.chars().any(char::is_control)
    {
        return Err(HtmlAnswerUnavailableReason::InvalidTitle);
    }
    let mut budget = RenderBudget::default();
    // Keep one parser for the entire draft so reference definitions survive section boundaries.
    // 为整份草稿保留同一解析器，使引用定义跨章节边界仍然有效。
    let body = render_blocks(source, &mut budget, true)?;
    let output = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><meta name=\"color-scheme\" content=\"light dark\"><title>{title}</title><style>{css}</style></head><body data-template=\"{template}\"><main><header><h1 dir=\"auto\">{title}</h1></header><div class=content>{body}</div></main><script>{js}</script></body></html>",
        title = escape(title),
        template = template.name(),
        css = include_str!("templates/v2.css"),
        js = include_str!("templates/v2.js"),
    );
    check_output(&output)?;
    Ok(output)
}

/// Return only the Markdown extensions supported by this fixed draft grammar.
/// 返回此固定草稿语法支持的 Markdown 扩展。
fn options() -> Options {
    Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH
}

/// Return a stable invalid-draft rejection for any failed grammar or complexity condition.
/// 对语法或复杂度条件失败返回稳定的非法草稿拒绝。
fn invalid<T>() -> Result<T, HtmlAnswerUnavailableReason> {
    Err(HtmlAnswerUnavailableReason::InvalidDraft)
}

/// Enforce the generated-byte limit for `output`; return no value on success.
/// 对 `output` 强制执行生成字节上限；成功时不返回值。
fn check_output(output: &str) -> Result<(), HtmlAnswerUnavailableReason> {
    if output.len() > DRAFT_MAX_RENDERED_BYTES {
        Err(HtmlAnswerUnavailableReason::PageTooLarge)
    } else {
        Ok(())
    }
}

/// Escape text and attribute values; no model-authored markup enters the generated DOM.
/// 转义文字与属性值；模型生成的标记不会进入生成的 DOM。
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Render Markdown and root component fences; `components` forbids recursive executable grammars.
/// 渲染 Markdown 和根级组件围栏；`components` 禁止递归的可执行语法。
/// Links/images remain readable text, raw HTML is escaped, and code remains ordinary code.
/// 链接和图片保留可读文字，原始 HTML 转义，代码保持普通代码。
/// The outer draft becomes panels: text before the first `##` is the lead, each `##` opens a
/// lettered panel with a head bar.
/// 外层草稿转为面板：首个 `##` 之前的文字是导语，每个 `##` 打开一个带字母标识及标题栏的面板。
fn render_blocks(
    source: &str,
    budget: &mut RenderBudget,
    components: bool,
) -> Result<String, HtmlAnswerUnavailableReason> {
    let mut parser = Parser::new_ext(source, options()).into_offset_iter();
    let mut output = String::new();
    let mut pending = Vec::new();
    let mut depth = 0;
    // Only the outer draft creates layout panels; component Markdown stays inside its own container.
    // 仅外层草稿创建布局面板；组件 Markdown 保持在自身容器内。
    let mut sections = 0;
    let mut lead = false;
    let mut section_open = false;
    let mut heading_head = false;
    // A table cell's first text may be a status word rendered as a badge.
    // 表格单元格的首段文字可以是渲染为徽标的状态词。
    let mut cell_start = false;
    let mut labeled_code = false;
    while let Some((event, range)) = parser.next() {
        budget.events += 1;
        if budget.events > DRAFT_MAX_EVENTS {
            return invalid();
        }
        if components && depth == 0 {
            let heading = matches!(
                event,
                Event::Start(Tag::Heading {
                    level: HeadingLevel::H2,
                    ..
                })
            );
            if heading || !section_open {
                sections += 1;
                if sections > DRAFT_MAX_BLOCKS {
                    return invalid();
                }
                html::push_html(&mut output, pending.drain(..));
                if section_open {
                    output.push_str("</div></section>");
                }
                section_open = true;
                if heading {
                    heading_head = true;
                    output.push_str(&format!(
                        "<section class=panel><header class=panel-head><span class=panel-id aria-hidden=true>{}</span>",
                        panel_letter(sections - usize::from(lead))
                    ));
                } else {
                    lead = true;
                    output.push_str("<section class=\"panel lead\"><div class=panel-body>");
                }
            }
        }
        if let Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) = &event
            && (info.starts_with("ui-") || info.as_ref() == "diff")
        {
            let component = info.starts_with("ui-");
            if component && (!components || depth != 0) {
                return invalid();
            }
            if component {
                budget.components += 1;
                if budget.components > DRAFT_MAX_BLOCKS {
                    return invalid();
                }
            }
            // CommonMark implicitly closes at EOF; a component requires a physical matching closer.
            // CommonMark 会在文件末尾隐式闭合；组件要求物理匹配的闭合行。
            if component
                && (source.as_bytes()[range.start] != b'`'
                    || super::closed_source_range(source, &range).is_none())
            {
                return invalid();
            }
            let mut body = String::new();
            for (part, _) in parser.by_ref() {
                budget.events += 1;
                if budget.events > DRAFT_MAX_EVENTS {
                    return invalid();
                }
                match part {
                    Event::End(TagEnd::CodeBlock) => break,
                    Event::Text(text) => body.push_str(&text),
                    _ => return invalid(),
                }
            }
            html::push_html(&mut output, pending.drain(..));
            output.push_str(&if component {
                render_component(info, &body, budget)?
            } else {
                render_diff(&body)
            });
            check_output(&output)?;
            continue;
        }
        match &event {
            Event::Start(_) => depth += 1,
            Event::End(_) => depth -= 1,
            _ => {}
        }
        let closes_head =
            heading_head && depth == 0 && matches!(event, Event::End(TagEnd::Heading(_)));
        let first_in_cell = std::mem::take(&mut cell_start);
        let event = match event {
            Event::Start(Tag::TableCell) => {
                cell_start = true;
                Event::Start(Tag::TableCell)
            }
            Event::Text(text) if first_in_cell => {
                status_badge(&text).map_or(Event::Text(text), |badge| Event::Html(badge.into()))
            }
            Event::Html(value) | Event::InlineHtml(value) => Event::Text(value),
            Event::Start(Tag::Link { dest_url, .. })
            | Event::Start(Tag::Image { dest_url, .. }) => Event::Html(
                format!("<span class=reference title=\"{}\">", escape(&dest_url)).into(),
            ),
            Event::End(TagEnd::Link | TagEnd::Image) => Event::Html("</span>".into()),
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) => {
                let language = info.split_whitespace().next().unwrap_or("");
                labeled_code = !language.is_empty();
                if labeled_code {
                    pending.push(Event::Html(
                        format!(
                            "<figure class=code><figcaption>{}</figcaption>",
                            escape(language)
                        )
                        .into(),
                    ));
                }
                Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info)))
            }
            other => other,
        };
        let code_end = matches!(event, Event::End(TagEnd::CodeBlock));
        pending.push(event);
        if code_end && labeled_code {
            labeled_code = false;
            pending.push(Event::Html("</figure>".into()));
        }
        if closes_head {
            heading_head = false;
            html::push_html(&mut output, pending.drain(..));
            output.push_str("</header><div class=panel-body>");
        }
    }
    html::push_html(&mut output, pending.into_iter());
    if section_open {
        output.push_str("</div></section>");
    }
    check_output(&output)?;
    Ok(output)
}

/// Return the panel identifier for the one-based `index`: A…Z, then AA, AB and so on.
/// 返回从一开始的 `index` 对应的面板标识：A…Z，之后为 AA、AB 等。
fn panel_letter(index: usize) -> String {
    let mut value = index;
    let mut letters = Vec::new();
    while value > 0 {
        value -= 1;
        letters.push(char::from(b'A' + (value % 26) as u8));
        value /= 26;
    }
    letters.iter().rev().collect()
}

/// Map a leading table status word (`ok`, `no`, `warn`, optionally followed by text) to a badge.
/// 将表格开头的状态词（`ok`、`no`、`warn`，可跟随文字）映射为徽标。
/// Returns escaped markup, or None for ordinary text.
/// 返回已转义标记；普通文字返回 None。
fn status_badge(text: &str) -> Option<String> {
    let (word, rest) = text
        .trim_start()
        .split_once(' ')
        .unwrap_or((text.trim(), ""));
    let (class, symbol) = match word {
        "ok" => ("ok", "✓"),
        "no" => ("no", "✗"),
        "warn" => ("warn", "!"),
        _ => return None,
    };
    Some(format!(
        "<span class=\"status {class}\" aria-label={class}>{symbol}</span>{}",
        if rest.trim().is_empty() {
            String::new()
        } else {
            format!(" {}", escape(rest.trim()))
        }
    ))
}

/// Render a unified diff with added, removed, hunk and context rows; content stays escaped text.
/// 渲染统一差异，区分新增、删除、区块头与上下文行；内容保持转义文字。
fn render_diff(source: &str) -> String {
    let mut output =
        String::from("<figure class=\"code diff\"><figcaption>diff</figcaption><pre><code>");
    for line in source.lines() {
        let class = if line.starts_with("@@") {
            "hunk"
        } else if line.starts_with("+++") || line.starts_with("---") {
            "file"
        } else if line.starts_with('+') {
            "add"
        } else if line.starts_with('-') {
            "del"
        } else {
            "ctx"
        };
        output.push_str(&format!("<span class={class}>{}</span>", escape(line)));
    }
    output.push_str("</code></pre></figure>");
    output
}

// Parse nonempty pipe-delimited rows with an exact column count and bounded item count.
/// 解析列数精确且条目数量有界的非空竖线分隔行。
fn rows(source: &str, columns: usize) -> Result<Vec<Vec<&str>>, HtmlAnswerUnavailableReason> {
    let parsed: Vec<_> = source
        .lines()
        .filter(|line| !line.trim().is_empty())
        .take(DRAFT_MAX_ITEMS + 1)
        .map(|line| {
            line.split('|')
                .take(columns + 1)
                .map(str::trim)
                .collect::<Vec<_>>()
        })
        .collect();
    if parsed.is_empty()
        || parsed.len() > DRAFT_MAX_ITEMS
        || parsed
            .iter()
            .any(|row| row.len() != columns || row.iter().any(|value| value.is_empty()))
    {
        return invalid();
    }
    Ok(parsed)
}

/// Compile one registered `info` fence and its content; unknown names or arguments are rejected.
/// 编译一个已注册 `info` 围栏及内容；拒绝未知名称或参数。
fn render_component(
    info: &str,
    source: &str,
    budget: &mut RenderBudget,
) -> Result<String, HtmlAnswerUnavailableReason> {
    let (name, args) = info
        .split_once(char::is_whitespace)
        .map_or((info, ""), |(name, args)| (name, args.trim()));
    if source.trim().is_empty() {
        return invalid();
    }
    match name {
        "ui-kv" if args.is_empty() => {
            let mut output = String::from("<dl class=kv>");
            for row in rows(source, 2)? {
                output.push_str(&format!(
                    "<div><dt dir=auto>{}</dt><dd dir=auto>{}</dd></div>",
                    escape(row[0]),
                    escape(row[1])
                ));
            }
            output.push_str("</dl>");
            Ok(output)
        }
        "ui-metrics" if args.is_empty() => {
            let mut output = String::from("<div class=metrics>");
            for row in rows(source, 4)? {
                let value = row[1]
                    .parse::<f64>()
                    .map_err(|_| HtmlAnswerUnavailableReason::InvalidDraft)?;
                let maximum = row[2]
                    .parse::<f64>()
                    .map_err(|_| HtmlAnswerUnavailableReason::InvalidDraft)?;
                if !value.is_finite()
                    || !maximum.is_finite()
                    || value < 0.0
                    || maximum <= 0.0
                    || value > maximum
                {
                    return invalid();
                }
                output.push_str(&format!("<div><span dir=auto>{label}</span><strong>{value} / {maximum} {unit}</strong><meter min=0 max=\"{maximum}\" value=\"{value}\" aria-label=\"{label}\"></meter></div>", label = escape(row[0]), unit = escape(row[3])));
            }
            output.push_str("</div>");
            Ok(output)
        }
        "ui-timeline" if args.is_empty() => {
            let mut output = String::from("<ol class=timeline>");
            for row in rows(source, 3)? {
                output.push_str(&format!("<li><span dir=auto>{}</span><div><strong dir=auto>{}</strong><p dir=auto>{}</p></div></li>", escape(row[0]), escape(row[1]), escape(row[2])));
            }
            output.push_str("</ol>");
            Ok(output)
        }
        "ui-flow" => diagrams::render_flow(source, args, budget.components),
        "ui-sequence" => diagrams::render_sequence(source, args),
        "ui-tree" if args.is_empty() => diagrams::render_tree(source),
        "ui-callout" => {
            let (kind, title) = args
                .split_once(char::is_whitespace)
                .ok_or(HtmlAnswerUnavailableReason::InvalidDraft)?;
            if !matches!(kind, "info" | "success" | "warning" | "danger") || title.trim().is_empty()
            {
                return invalid();
            }
            Ok(format!(
                "<aside class=\"callout {kind}\"><strong dir=auto>{}</strong>{}</aside>",
                escape(title.trim()),
                render_blocks(source, budget, false)?
            ))
        }
        "ui-details" if !args.is_empty() => Ok(format!(
            "<details><summary dir=auto>{}</summary>{}</details>",
            escape(args),
            render_blocks(source, budget, false)?
        )),
        "ui-tabs" if !args.is_empty() => render_tabs(source, args, budget),
        _ => invalid(),
    }
}

/// Split tab panels at root-level H3 headings and render keyboard-operable, locally scoped controls.
/// 按根级三级标题切分页签面板，渲染可键盘操作且作用域局限于当前组件的控件。
/// `label` names the tab group; return an error for missing headings, empty panels or nested components.
/// `label` 为页签组名称；缺少标题、面板为空或嵌套组件时返回错误。
fn render_tabs(
    source: &str,
    label: &str,
    budget: &mut RenderBudget,
) -> Result<String, HtmlAnswerUnavailableReason> {
    let mut starts = Vec::new();
    let mut depth = 0;
    for (event, range) in Parser::new_ext(source, options()).into_offset_iter() {
        match event {
            Event::Start(tag) => {
                if depth == 0
                    && matches!(
                        tag,
                        Tag::Heading {
                            level: HeadingLevel::H3,
                            ..
                        }
                    )
                {
                    starts.push(range);
                    if starts.len() > DRAFT_MAX_ITEMS {
                        return invalid();
                    }
                }
                depth += 1;
            }
            Event::End(_) => depth -= 1,
            _ => {}
        }
    }
    if starts.is_empty() || !source[..starts[0].start].trim().is_empty() {
        return invalid();
    }
    let id = budget.components;
    let mut buttons = String::new();
    let mut panels = String::new();
    for (index, heading) in starts.iter().enumerate() {
        // Let the same Markdown parser distinguish closing markers from content such as C#.
        // 由同一个 Markdown 解析器区分闭合标记与 C# 等真实内容。
        let title = Parser::new(&source[heading.clone()])
            .filter_map(|event| match event {
                Event::Text(text) | Event::Code(text) => Some(text.into_string()),
                Event::SoftBreak | Event::HardBreak => Some(" ".to_owned()),
                _ => None,
            })
            .collect::<String>();
        if title.trim().is_empty() {
            return invalid();
        }
        let end = starts
            .get(index + 1)
            .map_or(source.len(), |next| next.start);
        let content = &source[heading.end..end];
        if content.trim().is_empty() {
            return invalid();
        }
        buttons.push_str(&format!("<button type=button role=tab id=\"t{id}-{index}\" aria-controls=\"p{id}-{index}\" aria-selected=\"{selected}\" tabindex=\"{tabindex}\" dir=auto>{}</button>", escape(&title), selected = index == 0, tabindex = if index == 0 { 0 } else { -1 }));
        panels.push_str(&format!("<div role=tabpanel id=\"p{id}-{index}\" aria-labelledby=\"t{id}-{index}\" tabindex=0 {hidden}>{}</div>", render_blocks(content, budget, false)?, hidden = if index == 0 { "" } else { "hidden" }));
    }
    Ok(format!(
        "<div class=tabs><div role=tablist aria-label=\"{}\">{buttons}</div>{panels}</div>",
        escape(label)
    ))
}

#[cfg(test)]
mod tests;

mod diagrams;
