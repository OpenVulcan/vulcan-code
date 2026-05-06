//! Git diff parsing models for the review-style panel.
//! 面向审查面板的 Git diff 解析模型。

use std::path::Path;

/// Structured diff data rendered in the workbench panel.
/// 渲染在工作台面板中的结构化 diff 数据。
#[derive(Debug, Clone, Default)]
pub struct WorkspaceDiffPanel {
    /// Repository root used to generate the diff payload.
    /// 生成 diff 载荷时对应的仓库根目录。
    pub repository_root: String,
    /// Parsed diff files ready for file-level rendering.
    /// 已解析完成、可按文件级展示的 diff 文件列表。
    pub files: Vec<DiffFile>,
    /// Human-readable notice displayed when no structured diff files are available.
    /// 当没有结构化 diff 文件可展示时显示的人类可读提示。
    pub notice: String,
    /// Fallback plain text shown for placeholders or exceptional cases.
    /// 用于占位或异常场景回退显示的纯文本内容。
    pub fallback_text: String,
    /// Total addition count across all files.
    /// 全部文件累计的新增行数量。
    pub additions: usize,
    /// Total deletion count across all files.
    /// 全部文件累计的删除行数量。
    pub deletions: usize,
}

impl WorkspaceDiffPanel {
    /// Creates the initial placeholder panel before the first scan runs.
    /// 创建首次扫描前展示的初始占位面板。
    pub fn placeholder() -> Self {
        Self {
            repository_root: String::new(),
            files: Vec::new(),
            notice: String::from("尚未载入 Git Diff 结果。"),
            fallback_text: String::from(
                "使用方式：\n1. 在左侧“工作区入口”中填写一个 Git 仓库目录。\n2. 点击“立即扫描”或顶部“扫描工作区”。\n3. 中间区域会载入当前工作区的结构化 Diff 面板。",
            ),
            additions: 0,
            deletions: 0,
        }
    }

    /// Creates a notice-only panel for empty or exceptional states.
    /// 为无差异或异常状态创建仅提示信息的面板。
    pub fn with_notice(
        repository_root: impl Into<String>,
        notice: impl Into<String>,
        fallback_text: impl Into<String>,
    ) -> Self {
        Self {
            repository_root: repository_root.into(),
            files: Vec::new(),
            notice: notice.into(),
            fallback_text: fallback_text.into(),
            additions: 0,
            deletions: 0,
        }
    }

    /// Parses a unified diff string into file, hunk, and line structures.
    /// 将 unified diff 文本解析为文件、hunk 与行级结构。
    pub fn from_unified_diff(repository_root: impl Into<String>, patch: &str) -> Self {
        let mut panel = Self {
            repository_root: repository_root.into(),
            files: Vec::new(),
            notice: String::new(),
            fallback_text: patch.to_string(),
            additions: 0,
            deletions: 0,
        };

        let mut current_file: Option<DiffFile> = None;
        let mut current_hunk: Option<DiffHunk> = None;
        let mut cursor = HunkCursorAdapter::default();

        for raw_line in patch.lines() {
            if raw_line.starts_with("diff --git ") {
                flush_hunk(&mut current_file, &mut current_hunk);
                flush_file(&mut panel, current_file.take());

                let (previous_path, path) = parse_diff_git_header(raw_line);
                current_file = Some(DiffFile::new(path, Some(previous_path)));
                continue;
            }

            if let Some(file) = current_file.as_mut() {
                if raw_line.starts_with("@@ ") {
                    flush_hunk(&mut current_file, &mut current_hunk);
                    cursor = parse_hunk_cursor(raw_line);
                    current_hunk = Some(DiffHunk::new());
                    continue;
                }

                if raw_line.starts_with("--- ") {
                    let source_path =
                        normalize_git_path(raw_line.trim_start_matches("--- ").trim());

                    if source_path != "/dev/null" {
                        file.previous_path = Some(source_path);
                    }

                    continue;
                }

                if raw_line.starts_with("+++ ") {
                    let target_path =
                        normalize_git_path(raw_line.trim_start_matches("+++ ").trim());

                    if target_path != "/dev/null" {
                        file.path = target_path;
                    }

                    continue;
                }

                if raw_line.starts_with("rename from ") {
                    file.previous_path =
                        Some(raw_line.trim_start_matches("rename from ").to_string());
                    file.metadata.push(String::from("RENAME"));
                    continue;
                }

                if raw_line.starts_with("rename to ") {
                    file.path = raw_line.trim_start_matches("rename to ").to_string();
                    continue;
                }

                if raw_line.starts_with("new file mode ") {
                    file.metadata.push(String::from("NEW FILE"));
                    continue;
                }

                if raw_line.starts_with("deleted file mode ") {
                    file.metadata.push(String::from("DELETED"));
                    continue;
                }

                if raw_line.starts_with("Binary files ") {
                    file.metadata.push(String::from("BINARY"));
                    file.metadata.push(raw_line.to_string());
                    continue;
                }

                if let Some(hunk) = current_hunk.as_mut() {
                    let diff_line = parse_diff_line(raw_line, &mut cursor);

                    if matches!(diff_line.kind, DiffLineKind::Added) {
                        file.additions += 1;
                    } else if matches!(diff_line.kind, DiffLineKind::Removed) {
                        file.deletions += 1;
                    }

                    hunk.lines.push(diff_line);
                }
            }
        }

        flush_hunk(&mut current_file, &mut current_hunk);
        flush_file(&mut panel, current_file.take());
        panel.recalculate_totals();

        if panel.files.is_empty() {
            panel.notice = String::from("Diff 已载入，但当前没有解析出可展示的结构化文件节点。");
        }

        panel
    }

    /// Returns whether the panel contains at least one parsed diff file.
    /// 返回面板中是否至少包含一个已解析 diff 文件。
    pub fn has_files(&self) -> bool {
        !self.files.is_empty()
    }

    /// Returns the number of parsed files.
    /// 返回已解析文件数量。
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Builds the compact summary string rendered in the diff panel header.
    /// 构建渲染在 diff 面板头部的紧凑摘要字符串。
    pub fn summary_text(&self) -> String {
        if self.has_files() {
            format!(
                "未暂存 {}  +{}  -{}",
                self.file_count(),
                self.additions,
                self.deletions
            )
        } else if self.notice.is_empty() {
            String::from("当前无可展示的 Git Diff")
        } else {
            self.notice.clone()
        }
    }

    /// Recomputes aggregate addition and deletion counts from file payloads.
    /// 根据文件载荷重新计算总新增与总删除数量。
    fn recalculate_totals(&mut self) {
        self.additions = self.files.iter().map(|file| file.additions).sum();
        self.deletions = self.files.iter().map(|file| file.deletions).sum();
    }
}

/// File-level diff data for one changed path.
/// 单个变更路径对应的文件级 diff 数据。
#[derive(Debug, Clone, Default)]
pub struct DiffFile {
    /// Current display path of the file.
    /// 文件当前展示路径。
    pub path: String,
    /// Previous path before rename, if any.
    /// 如果发生重命名则记录之前的路径。
    pub previous_path: Option<String>,
    /// File-level metadata badges or notes.
    /// 文件级元信息徽标或注记。
    pub metadata: Vec<String>,
    /// Parsed hunks contained by the file.
    /// 文件中包含的已解析 hunk 列表。
    pub hunks: Vec<DiffHunk>,
    /// Count of added lines in the file.
    /// 该文件中的新增行数。
    pub additions: usize,
    /// Count of removed lines in the file.
    /// 该文件中的删除行数。
    pub deletions: usize,
}

impl DiffFile {
    /// Creates a new file-level diff node.
    /// 创建新的文件级 diff 节点。
    pub fn new(path: impl Into<String>, previous_path: Option<String>) -> Self {
        Self {
            path: path.into(),
            previous_path,
            metadata: Vec::new(),
            hunks: Vec::new(),
            additions: 0,
            deletions: 0,
        }
    }

    /// Returns the review header label for the file card.
    /// 返回文件卡片头部使用的审查标题文本。
    pub fn header_title(&self) -> String {
        if let Some(previous_path) = &self.previous_path {
            if previous_path != &self.path {
                return format!("{previous_path} -> {}", self.path);
            }
        }

        self.path.clone()
    }

    /// Returns a stable identity string for UI state such as collapse toggles.
    /// 返回供折叠等界面状态使用的稳定标识字符串。
    pub fn key(&self) -> String {
        match &self.previous_path {
            Some(previous_path) if previous_path != &self.path => {
                format!("{previous_path} -> {}", self.path)
            }
            _ => self.path.clone(),
        }
    }

    /// Returns whether the file is treated as a newly added file.
    /// 返回该文件是否被视为新增文件。
    pub fn is_new_file(&self) -> bool {
        self.metadata.iter().any(|metadata| metadata == "NEW FILE")
    }

    /// Builds the full display path shown below the file title.
    /// 构建显示在文件标题下方的完整路径文本。
    pub fn full_path_label(&self, repository_root: &str) -> String {
        if repository_root.is_empty() {
            return self.path.clone();
        }

        Path::new(repository_root)
            .join(&self.path)
            .display()
            .to_string()
    }
}

/// Hunk-level diff data grouped by unified hunk header.
/// 按 unified hunk 头部分组的 hunk 级 diff 数据。
#[derive(Debug, Clone, Default)]
pub struct DiffHunk {
    /// Parsed lines contained by the hunk.
    /// hunk 中包含的已解析行列表。
    pub lines: Vec<DiffLine>,
}

impl DiffHunk {
    /// Creates a new hunk node ready to collect parsed lines.
    /// 创建一个用于承载已解析行的新 hunk 节点。
    pub fn new() -> Self {
        Self { lines: Vec::new() }
    }
}

/// Line-level diff row ready for panel rendering.
/// 可直接用于面板渲染的行级 diff 数据。
#[derive(Debug, Clone)]
pub struct DiffLine {
    /// Semantic kind of the diff line.
    /// 该 diff 行的语义类型。
    pub kind: DiffLineKind,
    /// Old-side line number when available.
    /// 若存在则记录旧侧行号。
    pub old_number: Option<usize>,
    /// New-side line number when available.
    /// 若存在则记录新侧行号。
    pub new_number: Option<usize>,
    /// Raw text content without the unified diff leading sign.
    /// 去除 unified diff 前导符号后的原始文本内容。
    pub content: String,
}

impl DiffLine {
    /// Creates a new line node from resolved display parts.
    /// 根据已解析的展示片段创建新的行节点。
    pub fn new(
        kind: DiffLineKind,
        old_number: Option<usize>,
        new_number: Option<usize>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            old_number,
            new_number,
            content: content.into(),
        }
    }
}

/// Semantic categories of line-level diff rows.
/// 行级 diff 行的语义分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLineKind {
    /// Unchanged context line.
    /// 未变化的上下文行。
    Context,
    /// Added line on the new side.
    /// 新侧新增行。
    Added,
    /// Removed line from the old side.
    /// 旧侧删除行。
    Removed,
    /// Note line such as newline diagnostics.
    /// 例如换行诊断信息之类的注记行。
    Note,
}

/// Parses a `diff --git` header line into previous and current paths.
/// 将 `diff --git` 头部行解析为旧路径与新路径。
fn parse_diff_git_header(header: &str) -> (String, String) {
    let raw_paths = header.trim_start_matches("diff --git ").trim();
    let mut parts = raw_paths.split_whitespace();
    let previous_path = normalize_git_path(parts.next().unwrap_or_default());
    let current_path = normalize_git_path(parts.next().unwrap_or_default());

    (
        previous_path.clone(),
        if current_path.is_empty() {
            previous_path
        } else {
            current_path
        },
    )
}

/// Parses the unified hunk header into the starting line number cursor.
/// 将 unified hunk 头部解析为起始行号游标。
fn parse_hunk_cursor(header: &str) -> HunkCursorAdapter {
    let mut parts = header.split_whitespace();
    let _leading = parts.next();
    let old_range = parts.next().unwrap_or("-0,0");
    let new_range = parts.next().unwrap_or("+0,0");

    HunkCursorAdapter {
        old_line_number: parse_range_start(old_range),
        new_line_number: parse_range_start(new_range),
    }
}

/// Adapts hunk cursor creation to keep helper signatures compact.
/// 适配 hunk 游标的构建，以保持辅助函数签名紧凑。
#[derive(Debug, Clone, Copy, Default)]
struct HunkCursorAdapter {
    /// Current old-side line number.
    /// 当前旧侧行号。
    old_line_number: usize,
    /// Current new-side line number.
    /// 当前新侧行号。
    new_line_number: usize,
}

/// Parses the numeric start portion from a unified diff range token.
/// 从 unified diff 的区间标记中解析出起始数字。
fn parse_range_start(range_token: &str) -> usize {
    range_token
        .trim_start_matches(['-', '+'])
        .split(',')
        .next()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0)
}

/// Parses a single hunk line into a structured diff row while advancing the cursor.
/// 在推进行号游标的同时，将单个 hunk 行解析为结构化 diff 行。
fn parse_diff_line(raw_line: &str, cursor: &mut impl IntoMutableHunkCursor) -> DiffLine {
    if raw_line.starts_with('+') {
        let new_number = cursor.new_number();
        cursor.advance_new();

        return DiffLine::new(
            DiffLineKind::Added,
            None,
            Some(new_number),
            sanitize_diff_content(raw_line.trim_start_matches('+')),
        );
    }

    if raw_line.starts_with('-') {
        let old_number = cursor.old_number();
        cursor.advance_old();

        return DiffLine::new(
            DiffLineKind::Removed,
            Some(old_number),
            None,
            sanitize_diff_content(raw_line.trim_start_matches('-')),
        );
    }

    if raw_line.starts_with(' ') {
        let old_number = cursor.old_number();
        let new_number = cursor.new_number();
        cursor.advance_old();
        cursor.advance_new();

        return DiffLine::new(
            DiffLineKind::Context,
            Some(old_number),
            Some(new_number),
            sanitize_diff_content(raw_line.trim_start_matches(' ')),
        );
    }

    DiffLine::new(
        DiffLineKind::Note,
        None,
        None,
        sanitize_diff_content(raw_line),
    )
}

/// Normalizes Git-prefixed paths for display in the review panel.
/// 规范化 Git 前缀路径，供审查面板展示。
fn normalize_git_path(path: &str) -> String {
    let trimmed = path.trim_matches('"');

    if trimmed == "/dev/null" {
        return String::from("/dev/null");
    }

    trimmed
        .strip_prefix("a/")
        .or_else(|| trimmed.strip_prefix("b/"))
        .unwrap_or(trimmed)
        .to_string()
}

/// Sanitizes diff content for monospaced review rendering.
/// 对 diff 文本内容进行清洗，以适配等宽审查渲染。
fn sanitize_diff_content(content: &str) -> String {
    content.replace('\t', "    ")
}

/// Flushes the current hunk into the active file, if both exist.
/// 如存在当前 hunk 与活动文件，则将 hunk 刷入文件节点。
fn flush_hunk(current_file: &mut Option<DiffFile>, current_hunk: &mut Option<DiffHunk>) {
    if let Some(hunk) = current_hunk.take() {
        if let Some(file) = current_file.as_mut() {
            file.hunks.push(hunk);
        }
    }
}

/// Flushes the completed file into the diff panel while keeping insertion order.
/// 在保持插入顺序的前提下，将已完成文件刷入 diff 面板。
fn flush_file(panel: &mut WorkspaceDiffPanel, file: Option<DiffFile>) {
    if let Some(file) = file {
        panel.files.push(file);
    }
}

/// Internal mutable cursor abstraction for line-number progression helpers.
/// 供行号推进辅助函数使用的内部可变游标抽象。
trait IntoMutableHunkCursor {
    /// Returns the current old-side line number.
    /// 返回当前旧侧行号。
    fn old_number(&self) -> usize;
    /// Returns the current new-side line number.
    /// 返回当前新侧行号。
    fn new_number(&self) -> usize;
    /// Advances the old-side line number cursor.
    /// 推进旧侧行号游标。
    fn advance_old(&mut self);
    /// Advances the new-side line number cursor.
    /// 推进新侧行号游标。
    fn advance_new(&mut self);
}

impl IntoMutableHunkCursor for HunkCursorAdapter {
    /// Returns the current old-side line number.
    /// 返回当前旧侧行号。
    fn old_number(&self) -> usize {
        self.old_line_number
    }

    /// Returns the current new-side line number.
    /// 返回当前新侧行号。
    fn new_number(&self) -> usize {
        self.new_line_number
    }

    /// Advances the old-side line number cursor.
    /// 推进旧侧行号游标。
    fn advance_old(&mut self) {
        self.old_line_number = self.old_line_number.saturating_add(1);
    }

    /// Advances the new-side line number cursor.
    /// 推进新侧行号游标。
    fn advance_new(&mut self) {
        self.new_line_number = self.new_line_number.saturating_add(1);
    }
}
