//! Node search and outline, modelled on `rg` → owner and `markdown-menu` without depending on them.
//! 节点搜索与大纲，借鉴 `rg` → 所属结构与 `markdown-menu`，但不依赖它们。

use std::collections::BTreeSet;

use regex::{Regex, RegexBuilder};
use serde::Deserialize;

use crate::{
    DEFAULT_SEARCH_LIMIT, MAX_NODE_LEVEL, MAX_SEARCH_LIMIT, MAX_SEARCH_PATTERNS, MemoryError,
    MemoryScope, NODE_PAGE_BYTES, NodeView, ScopeDocuments,
};

/// Arguments of `memory_search`.
/// `memory_search` 的参数。
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchRequest {
    /// Patterns combined with OR.
    /// 以 OR 组合的模式。
    #[serde(default)]
    pub patterns: Vec<String>,
    /// Treats patterns as literal text.
    /// 将模式视为字面文本。
    #[serde(default)]
    pub fixed_strings: bool,
    /// Case-insensitive matching; defaults to true.
    /// 不区分大小写匹配；默认开启。
    #[serde(default)]
    pub ignore_case: Option<bool>,
    /// Scopes to search; empty means every available scope.
    /// 要搜索的层；为空表示所有可用层。
    #[serde(default)]
    pub scopes: Vec<MemoryScope>,
    /// Section key or H1 title filter.
    /// 分区键或 H1 标题过滤。
    #[serde(default)]
    pub section: Option<String>,
    /// Title path prefix filter, segments separated by `/` or `›`.
    /// 标题路径前缀过滤，段之间以 `/` 或 `›` 分隔。
    #[serde(default)]
    pub path_prefix: Option<String>,
    /// Exact node ids to read instead of patterns.
    /// 代替模式进行精确读取的节点标识。
    #[serde(default)]
    pub ids: Vec<String>,
    /// Nodes per page.
    /// 每页节点数。
    #[serde(default)]
    pub limit: Option<usize>,
    /// Continuation cursor from a previous result.
    /// 来自上一次结果的续读游标。
    #[serde(default)]
    pub cursor: Option<String>,
}

/// One matched or requested node.
/// 一个命中或被请求的节点。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    /// Node view.
    /// 节点视图。
    pub view: NodeView,
    /// Number of distinct patterns that matched.
    /// 命中的不同模式数。
    pub matched_patterns: usize,
    /// Whether a pattern matched the title.
    /// 是否有模式命中标题。
    pub title_hit: bool,
    /// Zero-based body line indexes with a match.
    /// 有命中的正文行下标（从零开始）。
    pub hit_lines: BTreeSet<usize>,
    /// One-based page shown.
    /// 显示的页码（从一开始）。
    pub page: usize,
    /// Page count of the body.
    /// 正文总页数。
    pub pages: usize,
    /// Body text of the shown page with hit lines marked.
    /// 显示页的正文，命中行带标记。
    pub text: String,
}

/// One page of search results.
/// 一页搜索结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchPage {
    /// Hits on this page.
    /// 本页命中。
    pub hits: Vec<SearchHit>,
    /// Total matched nodes.
    /// 命中节点总数。
    pub total: usize,
    /// Offset of the first hit on this page.
    /// 本页第一个命中的偏移。
    pub offset: usize,
    /// Number of patterns in the query; zero for an id read.
    /// 查询中的模式数；按标识读取时为零。
    pub pattern_count: usize,
    /// Notes about unavailable scopes.
    /// 关于不可用层的说明。
    pub notes: Vec<String>,
}

/// Arguments of `memory_outline`.
/// `memory_outline` 的参数。
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OutlineRequest {
    /// Scopes to list; empty means every available scope.
    /// 要列出的层；为空表示所有可用层。
    #[serde(default)]
    pub scopes: Vec<MemoryScope>,
    /// Section key or H1 title filter.
    /// 分区键或 H1 标题过滤。
    #[serde(default)]
    pub section: Option<String>,
    /// Title path prefix filter.
    /// 标题路径前缀过滤。
    #[serde(default)]
    pub path_prefix: Option<String>,
    /// Deepest heading level to list, 1 to 4.
    /// 列出的最深标题级别，1 至 4。
    #[serde(default)]
    pub depth: Option<u8>,
}

/// Marker placed before body lines that contain a match.
/// 放在含命中正文行之前的标记。
const HIT_MARKER: &str = ">> ";

/// Searches the visible scopes and returns one page of hits.
/// 搜索可见层并返回一页命中。
pub fn search_nodes(
    documents: &ScopeDocuments,
    request: &SearchRequest,
) -> Result<SearchPage, MemoryError> {
    let has_patterns = request.patterns.iter().any(|pattern| !pattern.is_empty());
    if has_patterns == !request.ids.is_empty() {
        return Err(MemoryError::invalid(
            "Give either `patterns` or `ids`, not both and not neither.",
        ));
    }
    if request.patterns.len() > MAX_SEARCH_PATTERNS {
        return Err(MemoryError::invalid(format!(
            "At most {MAX_SEARCH_PATTERNS} patterns are allowed."
        )));
    }
    let limit = request
        .limit
        .unwrap_or(DEFAULT_SEARCH_LIMIT)
        .clamp(1, MAX_SEARCH_LIMIT);
    let (views, notes) = visible_views(documents, &request.scopes)?;
    let views = filter_views(
        views,
        request.section.as_deref(),
        request.path_prefix.as_deref(),
    );

    if !request.ids.is_empty() {
        let page_request = parse_cursor(request.cursor.as_deref())?;
        if page_request > 1 && request.ids.len() != 1 {
            return Err(MemoryError::invalid(
                "A page cursor with `ids` needs exactly one id.",
            ));
        }
        let mut hits = Vec::new();
        for id in &request.ids {
            let view = views
                .iter()
                .find(|view| &view.id == id)
                .cloned()
                .ok_or_else(|| MemoryError::NodeNotFound(id.clone()))?;
            hits.push(paged_hit(
                view,
                0,
                false,
                BTreeSet::new(),
                Some(page_request.max(1)),
            )?);
        }
        let total = hits.len();
        return Ok(SearchPage {
            hits,
            total,
            offset: 0,
            pattern_count: 0,
            notes,
        });
    }

    let ignore_case = request.ignore_case.unwrap_or(true);
    let regexes = request
        .patterns
        .iter()
        .filter(|pattern| !pattern.is_empty())
        .map(|pattern| compile(pattern, request.fixed_strings, ignore_case))
        .collect::<Result<Vec<_>, _>>()?;
    let mut matches = Vec::new();
    for view in views {
        let title = view.path.last().cloned().unwrap_or_default();
        let mut matched = BTreeSet::new();
        let mut title_hit = false;
        let mut hit_lines = BTreeSet::new();
        for (index, regex) in regexes.iter().enumerate() {
            if regex.is_match(&title) {
                matched.insert(index);
                title_hit = true;
            }
            for (line_index, line) in view.body.lines().enumerate() {
                if regex.is_match(line) {
                    matched.insert(index);
                    hit_lines.insert(line_index);
                }
            }
        }
        if !matched.is_empty() {
            matches.push((matched.len(), title_hit, hit_lines, view));
        }
    }
    matches.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| right.1.cmp(&left.1))
            .then_with(|| right.3.scope.priority().cmp(&left.3.scope.priority()))
            .then_with(|| updated(&right.3).cmp(updated(&left.3)))
    });
    let total = matches.len();
    let offset = parse_cursor(request.cursor.as_deref())?.saturating_sub(1);
    let hits = matches
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|(count, title_hit, hit_lines, view)| {
            paged_hit(view, count, title_hit, hit_lines, None)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(SearchPage {
        hits,
        total,
        offset,
        pattern_count: regexes.len(),
        notes,
    })
}

/// Renders a search page as model-facing text.
/// 将一页搜索结果渲染为面向模型的文本。
pub fn render_search_page(page: &SearchPage) -> String {
    let mut lines = page.notes.clone();
    if page.total == 0 {
        lines.push(String::from(
            "No memory node matched. Try other wording or synonyms, or list titles with memory_outline.",
        ));
        return lines.join("\n");
    }
    if page.pattern_count > 0 {
        lines.push(format!(
            "{} node(s) matched; showing {}-{}.",
            page.total,
            page.offset + 1,
            page.offset + page.hits.len()
        ));
    }
    for hit in &page.hits {
        let view = &hit.view;
        lines.push(String::new());
        lines.push(format!(
            "## [{}] {}",
            view.scope.key(),
            view.path.join(" › ")
        ));
        let mut facts = vec![
            format!("id: {}", view.id),
            format!("revision: {}", view.revision),
        ];
        if let Some(meta) = &view.meta {
            facts.push(format!(
                "updated: {}",
                meta.updated.replace('T', " ").trim_end_matches('Z')
            ));
            facts.push(format!("source: {}", meta.source.key()));
        }
        if page.pattern_count > 0 {
            facts.push(format!(
                "matched: {}/{} patterns{}",
                hit.matched_patterns,
                page.pattern_count,
                if hit.title_hit { " (title)" } else { "" }
            ));
        }
        lines.push(facts.join(" · "));
        if !hit.text.is_empty() {
            lines.push(hit.text.clone());
        }
        if !view.children.is_empty() {
            lines.push(format!("children: {}", view.children.join(" · ")));
        }
        if hit.pages > 1 {
            let next = if hit.page < hit.pages {
                format!(
                    "; continue with ids=[\"{}\"] and cursor=\"{}\"",
                    view.id,
                    hit.page + 1
                )
            } else {
                String::new()
            };
            lines.push(format!("page {}/{}{next}", hit.page, hit.pages));
        }
    }
    let shown = page.offset + page.hits.len();
    if page.pattern_count > 0 && shown < page.total {
        lines.push(String::new());
        lines.push(format!(
            "More nodes matched; continue with cursor=\"{}\".",
            shown + 1
        ));
    }
    lines.join("\n")
}

/// Renders the heading tree of the visible scopes.
/// 渲染可见层的标题树。
pub fn render_outline(
    documents: &ScopeDocuments,
    request: &OutlineRequest,
) -> Result<String, MemoryError> {
    let depth = request.depth.unwrap_or(MAX_NODE_LEVEL);
    if !(1..=MAX_NODE_LEVEL).contains(&depth) {
        return Err(MemoryError::invalid(format!(
            "`depth` must be between 1 and {MAX_NODE_LEVEL}."
        )));
    }
    let (views, notes) = visible_views(documents, &request.scopes)?;
    let views = filter_views(
        views,
        request.section.as_deref(),
        request.path_prefix.as_deref(),
    );
    let mut lines = notes;
    for scope in MemoryScope::ALL {
        let scope_views = views
            .iter()
            .filter(|view| view.scope == scope)
            .collect::<Vec<_>>();
        if scope_views.is_empty() {
            continue;
        }
        let label = match (scope, &documents.project) {
            (MemoryScope::Project, Some((project, _))) => format!("project {}", project.name),
            _ => scope.key().to_string(),
        };
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.push(format!("[{label}] {} node(s)", scope_views.len()));
        for view in scope_views.into_iter().filter(|view| view.level <= depth) {
            let mut facts = vec![view.id.clone(), format!("r:{}", view.revision)];
            if let Some(meta) = &view.meta {
                facts.push(meta.updated.chars().take(10).collect());
                if let Some(section) = &meta.section {
                    facts.push(format!("section:{section}"));
                }
            }
            if !view.body.is_empty() {
                facts.push(format!("{} B", view.body.len()));
            }
            if view.level == depth && view.descendants > 0 {
                facts.push(format!("+{} below", view.descendants));
            }
            lines.push(format!(
                "{} {} ({})",
                "#".repeat(usize::from(view.level)),
                view.path.last().map_or("", String::as_str),
                facts.join(" · ")
            ));
        }
    }
    if lines.is_empty() {
        lines.push(String::from(
            "Memory is empty. Record durable facts with memory_write.",
        ));
    }
    Ok(lines.join("\n"))
}

/// Collects views of the requested scopes and notes for unavailable ones.
/// 收集请求层的视图，并为不可用层生成说明。
fn visible_views(
    documents: &ScopeDocuments,
    scopes: &[MemoryScope],
) -> Result<(Vec<NodeView>, Vec<String>), MemoryError> {
    let requested = if scopes.is_empty() {
        MemoryScope::ALL.to_vec()
    } else {
        scopes.to_vec()
    };
    let mut views = Vec::new();
    let mut notes = Vec::new();
    for scope in requested {
        match documents.document(scope) {
            Some(document) => views.extend(document.views(scope)),
            None if !scopes.is_empty() => notes.push(String::from(
                "[project] No project memory exists yet; it is created by the first project write.",
            )),
            None => {}
        }
    }
    Ok((views, notes))
}

/// Applies section and path-prefix filters.
/// 应用分区与路径前缀过滤。
fn filter_views(
    views: Vec<NodeView>,
    section: Option<&str>,
    path_prefix: Option<&str>,
) -> Vec<NodeView> {
    let section = section.map(str::trim).filter(|section| !section.is_empty());
    let prefix = path_prefix
        .map(|prefix| {
            prefix
                .split(['/', '›'])
                .map(|segment| segment.trim().to_lowercase())
                .filter(|segment| !segment.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    views
        .into_iter()
        .filter(|view| {
            section.is_none_or(|wanted| {
                view.section.as_deref() == Some(wanted)
                    || view
                        .path
                        .first()
                        .is_some_and(|title| title.eq_ignore_ascii_case(wanted))
            })
        })
        .filter(|view| {
            prefix.len() <= view.path.len()
                && prefix
                    .iter()
                    .zip(&view.path)
                    .all(|(wanted, title)| title.to_lowercase() == *wanted)
        })
        .collect()
}

/// Compiles one pattern with the `rg`-compatible Rust regex engine.
/// 用与 `rg` 兼容的 Rust 正则引擎编译单个模式。
fn compile(pattern: &str, fixed: bool, ignore_case: bool) -> Result<Regex, MemoryError> {
    let source = if fixed {
        regex::escape(pattern)
    } else {
        pattern.to_string()
    };
    RegexBuilder::new(&source)
        .case_insensitive(ignore_case)
        .size_limit(1 << 20)
        .build()
        .map_err(|error| {
            MemoryError::invalid(format!(
                "Pattern `{pattern}` is not a valid regex ({error}); set fixedStrings for literal text."
            ))
        })
}

/// Parses a one-based cursor; absent means 1.
/// 解析从一开始的游标；缺省为 1。
fn parse_cursor(cursor: Option<&str>) -> Result<usize, MemoryError> {
    match cursor.map(str::trim).filter(|cursor| !cursor.is_empty()) {
        None => Ok(1),
        Some(text) => text
            .parse::<usize>()
            .ok()
            .filter(|value| *value >= 1)
            .ok_or_else(|| MemoryError::invalid(format!("Cursor `{text}` is not a page number."))),
    }
}

/// Returns a node's update time for ordering; nodes without metadata sort last.
/// 返回用于排序的节点更新时间；无元数据节点排在最后。
fn updated(view: &NodeView) -> &str {
    view.meta.as_ref().map_or("", |meta| meta.updated.as_str())
}

/// Builds a hit showing the requested page, or the page of the first hit line.
/// 构建显示指定页（或首个命中行所在页）的命中。
fn paged_hit(
    view: NodeView,
    matched_patterns: usize,
    title_hit: bool,
    hit_lines: BTreeSet<usize>,
    requested_page: Option<usize>,
) -> Result<SearchHit, MemoryError> {
    let lines = view.body.lines().collect::<Vec<_>>();
    let pages = page_ranges(&lines);
    let page = match requested_page {
        Some(page) if page > pages.len().max(1) => {
            return Err(MemoryError::invalid(format!(
                "Node `{}` has {} page(s).",
                view.id,
                pages.len().max(1)
            )));
        }
        Some(page) => page,
        None => hit_lines
            .first()
            .and_then(|first| pages.iter().position(|range| range.contains(first)))
            .map_or(1, |index| index + 1),
    };
    let text = pages
        .get(page - 1)
        .map(|range| {
            range
                .clone()
                .map(|index| {
                    if hit_lines.contains(&index) {
                        format!("{HIT_MARKER}{}", lines[index])
                    } else {
                        lines[index].to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();
    Ok(SearchHit {
        pages: pages.len().max(1),
        view,
        matched_patterns,
        title_hit,
        hit_lines,
        page,
        text,
    })
}

/// Splits body lines into pages of at most [`NODE_PAGE_BYTES`], breaking only between lines.
/// 将正文行拆分为最多 [`NODE_PAGE_BYTES`] 的页，只在行之间断开。
fn page_ranges(lines: &[&str]) -> Vec<std::ops::Range<usize>> {
    let mut pages = Vec::new();
    let mut start = 0;
    let mut bytes = 0;
    for (index, line) in lines.iter().enumerate() {
        let cost = line.len() + 1;
        if index > start && bytes + cost > NODE_PAGE_BYTES {
            pages.push(start..index);
            start = index;
            bytes = 0;
        }
        bytes += cost;
    }
    if start < lines.len() {
        pages.push(start..lines.len());
    }
    pages
}

#[cfg(test)]
mod tests;
