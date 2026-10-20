//! Lossless parsing of a memory file into a node tree, and rendering back to bytes.
//! 将记忆文件无损解析为节点树，并渲染回字节。
//!
//! Untouched nodes keep their original heading, metadata and body bytes, so
//! `render(parse(text)) == text` holds for every input. Only nodes changed by a write
//! are regenerated.
//! 未触及的节点保留原始标题、元数据与正文字节，因此任何输入都满足
//! `render(parse(text)) == text`。只有写入改动的节点才重新生成。

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use sha2::{Digest, Sha256};

use crate::{MAX_NODE_LEVEL, MemoryScope};

/// Who last wrote a node.
/// 最近写入节点的一方。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeSource {
    /// Written by the AI through `memory_write`.
    /// 由 AI 通过 `memory_write` 写入。
    Ai,
    /// Written by a person through the interface.
    /// 由人通过界面写入。
    User,
}

impl NodeSource {
    /// Returns the stable metadata value.
    /// 返回稳定的元数据值。
    pub const fn key(self) -> &'static str {
        match self {
            Self::Ai => "ai",
            Self::User => "user",
        }
    }
}

/// Host-maintained metadata stored on the line right below a heading.
/// 存放在标题下一行、由宿主维护的元数据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeMeta {
    /// Stable random identifier that survives renames and moves.
    /// 改名与移动后保持不变的稳定随机标识。
    pub id: String,
    /// Stable section key, present only on preset or declared H1 sections.
    /// 稳定分区键，仅出现在预置或已声明的 H1 分区上。
    pub section: Option<String>,
    /// Creation time as `YYYY-MM-DDTHH:MMZ`.
    /// 以 `YYYY-MM-DDTHH:MMZ` 表示的创建时间。
    pub created: String,
    /// Last update time as `YYYY-MM-DDTHH:MMZ`.
    /// 以 `YYYY-MM-DDTHH:MMZ` 表示的最近更新时间。
    pub updated: String,
    /// Conversation that last wrote the node, when known.
    /// 最近写入该节点的会话（已知时）。
    pub session: Option<String>,
    /// Writer of the last change.
    /// 最近一次变更的写入方。
    pub source: NodeSource,
}

/// Comment prefix that marks a metadata line.
/// 标记元数据行的注释前缀。
const META_PREFIX: &str = "<!-- mem ";

impl NodeMeta {
    /// Parses one metadata line; malformed lines are ordinary body text.
    /// 解析一行元数据；格式错误的行视为普通正文。
    pub fn parse_line(line: &str) -> Option<Self> {
        let inner = line
            .trim()
            .strip_prefix(META_PREFIX)?
            .strip_suffix("-->")?
            .trim();
        let mut id = None;
        let mut section = None;
        let mut created = None;
        let mut updated = None;
        let mut session = None;
        let mut source = None;
        for pair in inner.split_whitespace() {
            let (key, value) = pair.split_once('=')?;
            if value.is_empty() || !value.chars().all(is_meta_value_char) {
                return None;
            }
            let value = Some(value.to_string());
            match key {
                "id" => id = value,
                "section" => section = value,
                "created" => created = value,
                "updated" => updated = value,
                "session" => session = value,
                "source" => {
                    source = match value.as_deref() {
                        Some("ai") => Some(NodeSource::Ai),
                        Some("user") => Some(NodeSource::User),
                        _ => return None,
                    }
                }
                _ => {}
            }
        }
        Some(Self {
            id: id?,
            section,
            created: created?,
            updated: updated?,
            session,
            source: source?,
        })
    }

    /// Renders the metadata comment without a line terminator.
    /// 渲染不带行终止符的元数据注释。
    pub fn render(&self) -> String {
        let mut line = format!("{META_PREFIX}id={}", self.id);
        if let Some(section) = &self.section {
            line.push_str(&format!(" section={section}"));
        }
        line.push_str(&format!(
            " created={} updated={}",
            self.created, self.updated
        ));
        if let Some(session) = &self.session {
            line.push_str(&format!(" session={session}"));
        }
        line.push_str(&format!(" source={} -->", self.source.key()));
        line
    }
}

/// Returns whether a character may appear in a metadata value.
/// 返回字符是否可以出现在元数据值中。
fn is_meta_value_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.' | ':')
}

/// One heading node with its own body and child nodes.
/// 一个标题节点，含自身正文与子节点。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryNode {
    /// Heading level, 1 to [`MAX_NODE_LEVEL`].
    /// 标题级别，1 至 [`MAX_NODE_LEVEL`]。
    pub level: u8,
    /// Heading text without Markdown heading markers.
    /// 去掉 Markdown 标题标记后的标题文字。
    pub title: String,
    /// Original heading bytes including the line terminator; `None` regenerates it.
    /// 含行终止符的原始标题字节；为 `None` 时重新生成。
    heading_raw: Option<String>,
    /// Parsed metadata, when the node has a valid metadata line.
    /// 节点具备合法元数据行时解析得到的元数据。
    pub meta: Option<NodeMeta>,
    /// Original metadata line including its terminator; `None` regenerates it.
    /// 含终止符的原始元数据行；为 `None` 时重新生成。
    meta_raw: Option<String>,
    /// Own body bytes up to the next node heading.
    /// 直到下一个节点标题之前的自身正文字节。
    body: String,
    /// Child nodes in document order.
    /// 按文档顺序排列的子节点。
    pub children: Vec<MemoryNode>,
}

impl MemoryNode {
    /// Builds a new node whose heading and metadata are rendered on output.
    /// 构建一个在输出时渲染标题与元数据的新节点。
    pub(crate) fn new(level: u8, title: String, meta: NodeMeta, body: String) -> Self {
        Self {
            level,
            title,
            heading_raw: None,
            meta: Some(meta),
            meta_raw: None,
            body,
            children: Vec::new(),
        }
    }

    /// Copies this node without children for insertion into another document: the heading and
    /// metadata lines are regenerated at `level` and the body uses that document's line terminator.
    /// 复制本节点（不含子节点）以插入另一文档：标题与元数据行按 `level` 重新生成，正文使用该文档的
    /// 行终止符。
    pub(crate) fn detached(&self, level: u8, eol: &str) -> Self {
        Self {
            level,
            title: self.title.clone(),
            heading_raw: None,
            meta: self.meta.clone(),
            meta_raw: None,
            body: stored_body(&self.body_text(), eol),
            children: Vec::new(),
        }
    }

    /// Returns the raw own body.
    /// 返回原始自身正文。
    pub fn raw_body(&self) -> &str {
        &self.body
    }

    /// Returns the own body with normalized line endings and no surrounding blank lines.
    /// 返回统一换行且去除首尾空行的自身正文。
    pub fn body_text(&self) -> String {
        normalize_newlines(&self.body)
            .trim_matches('\n')
            .trim_end()
            .to_string()
    }

    /// Replaces the own body with already normalized bytes.
    /// 用已规范化的字节替换自身正文。
    pub(crate) fn set_body(&mut self, body: String) {
        self.body = body;
    }

    /// Renames the node and regenerates its heading.
    /// 重命名节点并重新生成标题。
    pub(crate) fn set_title(&mut self, title: String) {
        self.title = title;
        self.heading_raw = None;
    }

    /// Replaces the metadata and regenerates its line.
    /// 替换元数据并重新生成其行。
    pub(crate) fn set_meta(&mut self, meta: NodeMeta) {
        self.meta = Some(meta);
        self.meta_raw = None;
    }

    /// Shifts this subtree to a new root level; fails when a descendant would pass the deepest level.
    /// 将该子树移到新的根级别；后代超出最深级别时失败。
    pub(crate) fn relevel(&mut self, level: u8) -> Result<(), u8> {
        let depth = self.subtree_depth();
        if level == 0 || level + depth > MAX_NODE_LEVEL {
            return Err(level + depth);
        }
        self.apply_level_shift(i16::from(level) - i16::from(self.level));
        Ok(())
    }

    /// Returns how many levels the deepest descendant sits below this node.
    /// 返回最深后代位于本节点之下的级数。
    fn subtree_depth(&self) -> u8 {
        self.children
            .iter()
            .map(|child| child.level.saturating_sub(self.level) + child.subtree_depth())
            .max()
            .unwrap_or(0)
    }

    /// Applies a level delta to the subtree, regenerating every moved heading.
    /// 对子树应用级别差，重新生成每个移动的标题。
    fn apply_level_shift(&mut self, delta: i16) {
        if delta != 0 {
            self.level = (i16::from(self.level) + delta) as u8;
            self.heading_raw = None;
        }
        for child in &mut self.children {
            child.apply_level_shift(delta);
        }
    }

    /// Returns the revision token of the node's own title and body.
    /// 返回节点自身标题与正文的修订令牌。
    pub fn revision(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.title.trim().as_bytes());
        hasher.update([0u8]);
        hasher.update(self.body_text().as_bytes());
        hex::encode(hasher.finalize())[..8].to_string()
    }

    /// Counts the nodes below this one.
    /// 统计本节点之下的节点数。
    pub fn descendant_count(&self) -> usize {
        self.children
            .iter()
            .map(|child| 1 + child.descendant_count())
            .sum()
    }
}

/// One parsed memory file.
/// 一个已解析的记忆文件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryDocument {
    /// Line terminator used for generated lines, taken from the source.
    /// 生成行使用的行终止符，取自源文件。
    eol: &'static str,
    /// Bytes before the first node heading.
    /// 第一个节点标题之前的字节。
    preamble: String,
    /// Top-level nodes in document order.
    /// 按文档顺序排列的顶层节点。
    pub roots: Vec<MemoryNode>,
}

/// A flattened, read-only view of one node used by search, outline and writes.
/// 供搜索、大纲与写入使用的单个节点扁平只读视图。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeView {
    /// Stable id from metadata, or a path-derived id for nodes written outside Vulcan.
    /// 元数据中的稳定标识；外部手写节点使用路径派生标识。
    pub id: String,
    /// Scope that owns the node.
    /// 节点所属层。
    pub scope: MemoryScope,
    /// Heading level.
    /// 标题级别。
    pub level: u8,
    /// Titles from the top-level ancestor down to this node.
    /// 从顶层祖先到本节点的标题链。
    pub path: Vec<String>,
    /// Child indexes from the document root to this node.
    /// 从文档根到本节点的子节点下标链。
    pub locator: Vec<usize>,
    /// Section key of the top-level ancestor, when it is a declared section.
    /// 顶层祖先为已声明分区时的分区键。
    pub section: Option<String>,
    /// Revision token required to change the node.
    /// 修改节点所需的修订令牌。
    pub revision: String,
    /// Metadata, when present.
    /// 元数据（存在时）。
    pub meta: Option<NodeMeta>,
    /// Normalized own body.
    /// 规范化后的自身正文。
    pub body: String,
    /// Titles of direct children.
    /// 直接子节点的标题。
    pub children: Vec<String>,
    /// Number of nodes below this one.
    /// 本节点之下的节点数。
    pub descendants: usize,
}

impl MemoryDocument {
    /// Builds an empty document that renders to an empty file.
    /// 构建渲染为空文件的空文档。
    pub fn empty() -> Self {
        Self {
            eol: "\n",
            preamble: String::new(),
            roots: Vec::new(),
        }
    }

    /// Parses file text; never fails, unknown structure stays body text.
    /// 解析文件文本；从不失败，未知结构保持为正文。
    pub fn parse(text: &str) -> Self {
        let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
        let headings = scan_headings(text);
        let Some(first) = headings.first() else {
            return Self {
                eol,
                preamble: text.to_string(),
                roots: Vec::new(),
            };
        };
        let preamble = text[..first.start].to_string();
        let mut flat = Vec::with_capacity(headings.len());
        for (index, heading) in headings.iter().enumerate() {
            let own_end = headings
                .get(index + 1)
                .map_or(text.len(), |next| next.start);
            let heading_raw = text[heading.start..heading.after].to_string();
            let mut body_start = heading.after;
            let mut meta = None;
            let mut meta_raw = None;
            if body_start < own_end {
                let line_end = text[body_start..own_end]
                    .find('\n')
                    .map_or(own_end, |offset| body_start + offset + 1);
                if let Some(parsed) = NodeMeta::parse_line(&text[body_start..line_end]) {
                    meta = Some(parsed);
                    meta_raw = Some(text[body_start..line_end].to_string());
                    body_start = line_end;
                }
            }
            flat.push(MemoryNode {
                level: heading.level,
                title: heading_title(&heading_raw, heading.level),
                heading_raw: Some(heading_raw),
                meta,
                meta_raw,
                body: text[body_start..own_end].to_string(),
                children: Vec::new(),
            });
        }
        Self {
            eol,
            preamble,
            roots: build_tree(flat),
        }
    }

    /// Returns the line terminator used for generated lines.
    /// 返回生成行使用的行终止符。
    pub fn eol(&self) -> &'static str {
        self.eol
    }

    /// Renders the document back to file text.
    /// 将文档渲染回文件文本。
    pub fn render(&self) -> String {
        let mut output = self.preamble.clone();
        for node in &self.roots {
            render_node(node, self.eol, &mut output);
        }
        output
    }

    /// Returns flattened views of every node in document order.
    /// 按文档顺序返回每个节点的扁平视图。
    pub fn views(&self, scope: MemoryScope) -> Vec<NodeView> {
        let mut views = Vec::new();
        let mut occurrences = std::collections::BTreeMap::new();
        for (index, node) in self.roots.iter().enumerate() {
            let section = (node.level == 1)
                .then(|| node.meta.as_ref().and_then(|meta| meta.section.clone()))
                .flatten();
            collect_views(
                node,
                scope,
                &mut Vec::new(),
                &mut vec![index],
                section.as_deref(),
                &mut occurrences,
                &mut views,
            );
        }
        views
    }

    /// Returns the node at a locator.
    /// 返回位于定位链的节点。
    pub fn node(&self, locator: &[usize]) -> Option<&MemoryNode> {
        let (first, rest) = locator.split_first()?;
        rest.iter()
            .try_fold(self.roots.get(*first)?, |node, index| {
                node.children.get(*index)
            })
    }

    /// Returns the node at a locator mutably.
    /// 以可变方式返回位于定位链的节点。
    pub(crate) fn node_mut(&mut self, locator: &[usize]) -> Option<&mut MemoryNode> {
        let (first, rest) = locator.split_first()?;
        let mut node = self.roots.get_mut(*first)?;
        for index in rest {
            node = node.children.get_mut(*index)?;
        }
        Some(node)
    }

    /// Removes and returns the subtree at a locator.
    /// 移除并返回位于定位链的子树。
    pub(crate) fn remove(&mut self, locator: &[usize]) -> Option<MemoryNode> {
        let (last, parent) = locator.split_last()?;
        let siblings = if parent.is_empty() {
            &mut self.roots
        } else {
            &mut self.node_mut(parent)?.children
        };
        (*last < siblings.len()).then(|| siblings.remove(*last))
    }

    /// Appends a subtree as the last child of a parent, or as the last top-level node.
    /// 将子树追加为父节点的最后一个子节点，或作为最后一个顶层节点。
    pub(crate) fn append(&mut self, parent: Option<&[usize]>, node: MemoryNode) -> bool {
        match parent {
            None => {
                self.roots.push(node);
                true
            }
            Some(locator) => match self.node_mut(locator) {
                Some(parent) => {
                    parent.children.push(node);
                    true
                }
                None => false,
            },
        }
    }
}

/// Converts written text into stored body bytes: own line terminators, one trailing blank line.
/// 将写入文本转换为存储正文字节：使用文件行终止符，末尾保留一个空行。
pub(crate) fn stored_body(text: &str, eol: &str) -> String {
    let normalized = normalize_newlines(text);
    let trimmed = normalized.trim_matches('\n').trim_end();
    if trimmed.is_empty() {
        return eol.to_string();
    }
    let mut body = trimmed.replace('\n', eol);
    body.push_str(eol);
    body.push_str(eol);
    body
}

/// Returns the levels of node-forming headings found in text written as a body.
/// 返回作为正文写入的文本中会形成节点的标题级别。
pub(crate) fn node_heading_levels(text: &str) -> Vec<u8> {
    scan_headings(text)
        .into_iter()
        .map(|heading| heading.level)
        .collect()
}

/// Returns whether text renders as exactly one ATX heading with the given title.
/// 返回文本是否恰好渲染为一个带给定标题的 ATX 标题。
pub(crate) fn title_round_trips(level: u8, title: &str) -> bool {
    let line = format!("{} {title}\n", "#".repeat(usize::from(level)));
    let headings = scan_headings(&line);
    headings.len() == 1
        && headings[0].level == level
        && heading_title(&line[headings[0].start..headings[0].after], level) == title
}

/// Normalizes CRLF and lone CR line endings to LF.
/// 将 CRLF 与单独 CR 换行统一为 LF。
fn normalize_newlines(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// One top-level heading found in source text.
/// 在源文本中找到的一个顶层标题。
struct ScannedHeading {
    /// Byte offset where the heading starts.
    /// 标题起始字节偏移。
    start: usize,
    /// Byte offset just after the heading's last line terminator.
    /// 紧跟标题最后一个行终止符之后的字节偏移。
    after: usize,
    /// Heading level.
    /// 标题级别。
    level: u8,
}

/// Finds top-level H1–H4 headings with the CommonMark parser; fenced `#` lines are not headings.
/// 用 CommonMark 解析器查找顶层 H1–H4 标题；围栏内的 `#` 行不是标题。
fn scan_headings(text: &str) -> Vec<ScannedHeading> {
    let mut headings = Vec::new();
    let mut depth = 0usize;
    for (event, range) in Parser::new_ext(text, Options::empty()).into_offset_iter() {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                let level = heading_level_number(level);
                if depth == 0 && level <= MAX_NODE_LEVEL {
                    let last = range.end.saturating_sub(1).max(range.start);
                    let after = text[last..]
                        .find('\n')
                        .map_or(text.len(), |offset| last + offset + 1);
                    headings.push(ScannedHeading {
                        start: range.start,
                        after,
                        level,
                    });
                }
                depth += 1;
            }
            Event::End(TagEnd::Heading(_)) => depth = depth.saturating_sub(1),
            Event::Start(_) => depth += 1,
            Event::End(_) => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    headings
}

/// Converts a parser heading level into its number.
/// 将解析器标题级别转换为数字。
fn heading_level_number(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// Extracts the raw title text from heading source, for ATX and setext forms.
/// 从标题源码中提取原始标题文字，支持 ATX 与 setext 两种形式。
fn heading_title(raw: &str, level: u8) -> String {
    let lines = raw.lines().collect::<Vec<_>>();
    let first = lines.first().copied().unwrap_or_default();
    let stripped = first.trim_start_matches(' ');
    if first.len() - stripped.len() <= 3
        && let Some(rest) = stripped.strip_prefix(&"#".repeat(usize::from(level)))
        && (rest.is_empty() || rest.starts_with([' ', '\t']))
    {
        let content = rest.trim();
        let without_closing = content.trim_end_matches('#');
        return if without_closing.is_empty() {
            String::new()
        } else if without_closing.ends_with([' ', '\t']) {
            without_closing.trim_end().to_string()
        } else {
            content.to_string()
        };
    }
    lines[..lines.len().saturating_sub(1)]
        .iter()
        .map(|line| line.trim())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Nests a flat heading list into a tree by level.
/// 按级别把扁平标题列表嵌套成树。
fn build_tree(flat: Vec<MemoryNode>) -> Vec<MemoryNode> {
    let mut roots: Vec<MemoryNode> = Vec::new();
    let mut stack: Vec<MemoryNode> = Vec::new();
    for node in flat {
        while stack.last().is_some_and(|open| open.level >= node.level) {
            let closed = stack.pop().expect("stack checked non-empty");
            attach(&mut stack, &mut roots, closed);
        }
        stack.push(node);
    }
    while let Some(closed) = stack.pop() {
        attach(&mut stack, &mut roots, closed);
    }
    roots
}

/// Attaches a closed node to the open parent on the stack, or to the roots.
/// 将已闭合节点挂到栈上的打开父节点，或挂到根列表。
fn attach(stack: &mut [MemoryNode], roots: &mut Vec<MemoryNode>, node: MemoryNode) {
    match stack.last_mut() {
        Some(parent) => parent.children.push(node),
        None => roots.push(node),
    }
}

/// Appends a line terminator when output does not end at a line start.
/// 输出未停在行首时追加行终止符。
fn ensure_line_start(output: &mut String, eol: &str) {
    if !output.is_empty() && !output.ends_with('\n') {
        output.push_str(eol);
    }
}

/// Renders one subtree, reusing original bytes wherever they are still valid.
/// 渲染一个子树，原始字节仍有效时直接复用。
fn render_node(node: &MemoryNode, eol: &str, output: &mut String) {
    ensure_line_start(output, eol);
    match &node.heading_raw {
        Some(raw) => output.push_str(raw),
        None => {
            output.push_str(&"#".repeat(usize::from(node.level)));
            output.push(' ');
            output.push_str(&node.title);
            output.push_str(eol);
        }
    }
    if let Some(meta) = &node.meta {
        ensure_line_start(output, eol);
        match &node.meta_raw {
            Some(raw) => output.push_str(raw),
            None => {
                output.push_str(&meta.render());
                output.push_str(eol);
            }
        }
    }
    if !node.body.is_empty() {
        ensure_line_start(output, eol);
        output.push_str(&node.body);
    }
    for child in &node.children {
        render_node(child, eol, output);
    }
}

/// Collects views for a subtree in document order.
/// 按文档顺序收集一个子树的视图。
fn collect_views(
    node: &MemoryNode,
    scope: MemoryScope,
    path: &mut Vec<String>,
    locator: &mut Vec<usize>,
    section: Option<&str>,
    occurrences: &mut std::collections::BTreeMap<Vec<String>, usize>,
    views: &mut Vec<NodeView>,
) {
    path.push(node.title.clone());
    let id = match &node.meta {
        Some(meta) => meta.id.clone(),
        None => {
            let occurrence = occurrences.entry(path.clone()).or_insert(0);
            *occurrence += 1;
            derived_node_id(scope, path, *occurrence)
        }
    };
    views.push(NodeView {
        id,
        scope,
        level: node.level,
        path: path.clone(),
        locator: locator.clone(),
        section: section.map(str::to_string),
        revision: node.revision(),
        meta: node.meta.clone(),
        body: node.body_text(),
        children: node
            .children
            .iter()
            .map(|child| child.title.clone())
            .collect(),
        descendants: node.descendant_count(),
    });
    for (index, child) in node.children.iter().enumerate() {
        locator.push(index);
        collect_views(child, scope, path, locator, section, occurrences, views);
        locator.pop();
    }
    path.pop();
}

/// Derives a stable id for a node without metadata from its scope, title path and occurrence.
/// 由层、标题路径与出现序号为无元数据节点派生稳定标识。
fn derived_node_id(scope: MemoryScope, path: &[String], occurrence: usize) -> String {
    let mut hasher = Sha256::new();
    hasher.update(scope.key().as_bytes());
    for title in path {
        hasher.update([0x1f]);
        hasher.update(title.as_bytes());
    }
    hasher.update([0x1f]);
    hasher.update(occurrence.to_string().as_bytes());
    format!("h-{}", &hex::encode(hasher.finalize())[..10])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Representative files, including fences, setext headings, CRLF and no final newline, round-trip exactly.
    /// 代表性文件（含围栏、setext 标题、CRLF 与无结尾换行）均能精确往返。
    #[test]
    fn parse_render_round_trips_exact_bytes() {
        let samples = [
            "",
            "plain text only\n",
            "intro\n\n# Base\n<!-- mem id=m-1 section=base created=2026-10-01T00:00Z updated=2026-10-02T00:00Z source=ai -->\nbody\n\n## Child\ntext\n```\n# not a heading\n```\n",
            "Title\n=====\n\nsetext body\n\nSub\n---\nmore\n",
            "# A\r\nline\r\n## B\r\nx\r\n",
            "# Tail without newline",
            "> # quoted heading\n\n- # list heading\n\n##### deep\n# Real\n",
            "## Starts at two\n# Then one\n### Skips two\n",
        ];
        for sample in samples {
            assert_eq!(MemoryDocument::parse(sample).render(), sample, "{sample:?}");
        }
    }

    /// Nesting follows heading levels; quoted, listed and H5 headings stay body text.
    /// 嵌套遵循标题级别；引用、列表与 H5 标题保持为正文。
    #[test]
    fn builds_tree_from_top_level_headings_only() {
        let document = MemoryDocument::parse(
            "> # quoted\n# One\n## Two\n### Three\n##### Five\n## Two b\n# Uno\n",
        );
        let views = document.views(MemoryScope::Project);
        let paths = views
            .iter()
            .map(|view| view.path.join("/"))
            .collect::<Vec<_>>();
        assert_eq!(
            paths,
            ["One", "One/Two", "One/Two/Three", "One/Two b", "Uno"]
        );
        assert!(views[2].body.contains("##### Five"));
        assert!(document.preamble.contains("quoted"));
    }

    /// Metadata is parsed from the line below a heading; malformed lines stay body text.
    /// 元数据从标题下一行解析；格式错误的行保持为正文。
    #[test]
    fn reads_metadata_and_keeps_malformed_lines_in_body() {
        let document = MemoryDocument::parse(
            "# S\n<!-- mem id=m-a section=base created=2026-10-01T00:00Z updated=2026-10-02T00:00Z session=abc source=user -->\nbody\n## T\n<!-- mem id=m-b -->\nrest\n",
        );
        let views = document.views(MemoryScope::Space);
        let meta = views[0].meta.as_ref().expect("section metadata");
        assert_eq!(meta.id, "m-a");
        assert_eq!(meta.section.as_deref(), Some("base"));
        assert_eq!(meta.source, NodeSource::User);
        assert_eq!(views[0].section.as_deref(), Some("base"));
        assert_eq!(views[0].id, "m-a");
        assert!(views[1].meta.is_none());
        assert!(views[1].id.starts_with("h-"));
        assert!(views[1].body.contains("<!-- mem id=m-b -->"));
        assert_eq!(views[1].section.as_deref(), Some("base"));
    }

    /// Titles with closing hashes and inline code are extracted as written.
    /// 带结尾井号与行内代码的标题按书写原样提取。
    #[test]
    fn extracts_raw_titles() {
        let document = MemoryDocument::parse("# Use `memory_write` ##\n## C#\n### ###\n");
        let views = document.views(MemoryScope::User);
        assert_eq!(views[0].title_text(), "Use `memory_write`");
        assert_eq!(views[1].title_text(), "C#");
        assert_eq!(views[2].title_text(), "");
        assert!(title_round_trips(2, "Use `memory_write`"));
        assert!(!title_round_trips(2, "Trailing #"));
        assert!(!title_round_trips(2, "Two\nlines"));
    }

    /// Releveling regenerates moved headings and refuses subtrees that would exceed H4.
    /// 调整级别会重新生成移动的标题，并拒绝超出 H4 的子树。
    #[test]
    fn relevel_shifts_subtree_within_limit() {
        let mut document = MemoryDocument::parse("# A\n## B\n### C\n");
        let mut node = document.remove(&[0, 0]).expect("subtree");
        assert!(node.relevel(4).is_err());
        node.relevel(3).expect("deepest fit");
        node.relevel(1).expect("fits");
        assert!(document.append(None, node));
        assert_eq!(document.render(), "# A\n# B\n## C\n");
    }

    /// Revision changes with the body and ignores line endings and surrounding blank lines.
    /// 修订随正文变化，且不受换行风格与首尾空行影响。
    #[test]
    fn revision_tracks_title_and_body() {
        let lf = MemoryDocument::parse("# A\nbody\n\n");
        let crlf = MemoryDocument::parse("# A\r\n\r\nbody\r\n");
        let changed = MemoryDocument::parse("# A\nother\n");
        assert_eq!(lf.roots[0].revision(), crlf.roots[0].revision());
        assert_ne!(lf.roots[0].revision(), changed.roots[0].revision());
    }

    impl NodeView {
        /// Returns the node's own title.
        /// 返回节点自身标题。
        fn title_text(&self) -> &str {
            self.path.last().map(String::as_str).unwrap_or_default()
        }
    }
}
