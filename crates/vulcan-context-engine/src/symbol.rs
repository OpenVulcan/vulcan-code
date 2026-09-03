//! Exact symbol occurrence records and bounded lexical retrieval.
//! 精确符号出现位置记录和有界词法检索。

use crate::identity::{
    chunk_id_with_offset, content_hash, is_sha256_identity, normalize_relative_path,
    sha256_hex_parts,
};
use crate::model::{Chunk, HitKind, SearchHit, SearchSource, SourceRange};
use std::collections::BTreeSet;

/// Identify the structural kind of an exact symbol occurrence.
/// 标识精确符号出现位置的结构类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SymbolKind {
    /// A command, route, or other externally visible endpoint.
    /// 命令、路由或其他对外可见入口。
    Endpoint,
    /// A normal function, type, class, or module definition.
    /// 普通函数、类型、类或模块定义。
    Definition,
    /// A use or reference occurrence.
    /// 符号使用或引用位置。
    Reference,
}

/// One exact symbol occurrence retained independently of a graph database.
/// 独立于图数据库保存的一个精确符号出现位置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolOccurrence {
    /// Stable occurrence identity.
    /// 稳定的出现位置身份。
    pub occurrence_id: String,
    /// Exact identifier text.
    /// 精确的标识符文本。
    pub identifier: String,
    /// Blob identity containing the occurrence.
    /// 包含该出现位置的 Blob 身份。
    pub blob_name: String,
    /// Canonical workspace-relative path.
    /// 规范化的工作区相对路径。
    pub path: String,
    /// Source range of the occurrence.
    /// 出现位置的源码范围。
    pub range: SourceRange,
    /// Unicode-scalar offset within the first source line of the containing Chunk.
    /// 所在代码块首行内的 Unicode 标量偏移。
    pub start_char_offset: usize,
    /// Chunk identity containing the occurrence.
    /// 包含该出现位置的代码块身份。
    pub chunk_id: String,
    /// Source text used to render and rerank this occurrence.
    /// 用于渲染和重排该出现位置的源码文本。
    pub content: String,
    /// Structural symbol kind.
    /// 符号结构类型。
    pub kind: SymbolKind,
}

impl SymbolOccurrence {
    /// Create an exact occurrence from a Chunk and identifier metadata.
    /// 根据代码块和标识符元数据创建精确出现位置。
    pub fn from_chunk(chunk: &Chunk, identifier: impl Into<String>, kind: SymbolKind) -> Self {
        let identifier = identifier.into();
        let occurrence_id = occurrence_id(&chunk.chunk_id, &identifier, kind);
        Self {
            occurrence_id,
            identifier,
            blob_name: chunk.blob_name.clone(),
            path: chunk.path.clone(),
            range: chunk.range,
            start_char_offset: chunk.start_char_offset,
            chunk_id: chunk.chunk_id.clone(),
            content: chunk.content.clone(),
            kind,
        }
    }

    /// Convert the occurrence into a structured exact SearchHit.
    /// 将符号出现位置转换为结构化精确检索命中。
    pub fn to_hit(&self, score: f32) -> SearchHit {
        SearchHit {
            id: self.occurrence_id.clone(),
            blob_name: self.blob_name.clone(),
            path: self.path.clone(),
            range: self.range,
            start_char_offset: self.start_char_offset,
            content: self.content.clone(),
            content_hash: crate::identity::content_hash(&self.content),
            score,
            source: SearchSource::Exact,
            kind: match self.kind {
                SymbolKind::Endpoint => HitKind::Endpoint,
                SymbolKind::Definition => HitKind::Definition,
                SymbolKind::Reference => HitKind::Reference,
            },
        }
    }
}

/// Verify that an exact SearchHit is the occurrence of one requested identifier and symbol kind.
/// 验证精确命中确实属于某个请求标识符及其符号类型的出现位置。
pub(crate) fn matches_exact_hit_identity(hit: &SearchHit, identifiers: &[String]) -> bool {
    let kind = match hit.kind {
        HitKind::Endpoint => SymbolKind::Endpoint,
        HitKind::Definition => SymbolKind::Definition,
        HitKind::Reference => SymbolKind::Reference,
        HitKind::Content => return false,
    };
    let containing_chunk_id = chunk_id_with_offset(
        &hit.blob_name,
        &hit.content_hash,
        hit.range.start_line,
        hit.range.end_line,
        hit.start_char_offset,
    );
    identifiers.iter().any(|identifier| {
        content_contains_identifier(&hit.content, identifier)
            && occurrence_id(&containing_chunk_id, identifier, kind) == hit.id
    })
}

/// Verify a symbol occurrence exactly matches its containing validated Chunk.
/// 验证符号出现位置与其所在的已校验 Chunk 完全一致。
pub(crate) fn occurrence_matches_chunk(occurrence: &SymbolOccurrence, chunk: &Chunk) -> bool {
    occurrence
        == &SymbolOccurrence::from_chunk(chunk, occurrence.identifier.clone(), occurrence.kind)
        && is_symbol_identifier(&occurrence.identifier)
        && content_contains_identifier(&occurrence.content, &occurrence.identifier)
}

/// Verify a standalone occurrence is internally safe for bounded lexical search.
/// 验证独立出现位置在有界词法检索中保持内部安全一致。
fn occurrence_is_self_consistent(occurrence: &SymbolOccurrence) -> bool {
    let normalized_path = normalize_relative_path(&occurrence.path).ok();
    let expected_chunk_id = chunk_id_with_offset(
        &occurrence.blob_name,
        &content_hash(&occurrence.content),
        occurrence.range.start_line,
        occurrence.range.end_line,
        occurrence.start_char_offset,
    );
    normalized_path.as_deref() == Some(occurrence.path.as_str())
        && is_sha256_identity(&occurrence.blob_name)
        && occurrence.range.start_line > 0
        && occurrence.range.end_line >= occurrence.range.start_line
        && occurrence.chunk_id == expected_chunk_id
        && occurrence.occurrence_id
            == occurrence_id(&expected_chunk_id, &occurrence.identifier, occurrence.kind)
        && is_symbol_identifier(&occurrence.identifier)
        && content_contains_identifier(&occurrence.content, &occurrence.identifier)
}

/// Compute the stable identity of one exact symbol occurrence.
/// 计算一个精确符号出现位置的稳定身份。
fn occurrence_id(chunk_id: &str, identifier: &str, kind: SymbolKind) -> String {
    sha256_hex_parts([
        chunk_id.as_bytes(),
        b"\n",
        identifier.as_bytes(),
        b"\n",
        symbol_kind_name(kind).as_bytes(),
    ])
}

/// Search exact symbol occurrences inside an explicit Blob scope.
/// 在明确的 Blob 范围内检索精确符号出现位置。
pub fn search_exact_symbols(
    occurrences: &[SymbolOccurrence],
    identifiers: &[String],
    allowed_blobs: &BTreeSet<String>,
    limit: usize,
) -> Vec<SearchHit> {
    if identifiers.is_empty() || allowed_blobs.is_empty() || limit == 0 {
        return Vec::new();
    }
    let wanted = identifiers.iter().collect::<BTreeSet<_>>();
    let mut hits = occurrences
        .iter()
        .filter(|occurrence| {
            wanted.contains(&occurrence.identifier)
                && allowed_blobs.contains(&occurrence.blob_name)
                && occurrence_is_self_consistent(occurrence)
        })
        .map(|occurrence| {
            let score = match occurrence.kind {
                SymbolKind::Endpoint => 1.0,
                SymbolKind::Definition => 0.9,
                SymbolKind::Reference => 0.7,
            };
            occurrence.to_hit(score)
        })
        .collect::<Vec<_>>();
    hits.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.range.cmp(&right.range))
            .then_with(|| left.start_char_offset.cmp(&right.start_char_offset))
            .then_with(|| left.id.cmp(&right.id))
    });
    let mut seen_occurrences = BTreeSet::new();
    hits.retain(|hit| seen_occurrences.insert(hit.occurrence_key()));
    hits.truncate(limit);
    hits
}

/// Extract conservative definition and endpoint occurrences from prepared source chunks.
/// 从已准备的源码块中保守提取定义和入口出现位置。
pub fn extract_symbols_from_chunks(chunks: &[Chunk]) -> Vec<SymbolOccurrence> {
    let mut occurrences = Vec::new();
    let mut seen = BTreeSet::new();
    // Preserve lexical state only across sequential fragments of the same source Blob.
    // 仅在同一源码 Blob 的连续片段之间保留词法状态。
    let mut previous_chunk: Option<&Chunk> = None;
    // An endpoint annotation may end one Chunk immediately before its definition.
    // 入口注解可能位于一个 Chunk 末尾，而定义紧接在下一个 Chunk 中。
    let mut endpoint_pending = false;
    // Lexical state can span recursive Chunks and fragments of one overlong source line.
    // 词法状态可能跨越递归 Chunk 和同一超长源码行的多个片段。
    let mut lexical_state = LexicalState::default();
    for chunk in chunks {
        let same_line_continuation =
            previous_chunk.is_some_and(|previous| chunks_continue_same_line(previous, chunk));
        if previous_chunk.is_some_and(|previous| !chunks_are_contiguous(previous, chunk)) {
            endpoint_pending = false;
            lexical_state = LexicalState::default();
        } else if !same_line_continuation {
            lexical_state.reset_line_state();
        }
        for (line_index, line) in chunk.content.lines().enumerate() {
            if line_index > 0 {
                lexical_state.reset_line_state();
            }
            let code = strip_non_code_regions(line, &mut lexical_state);
            let trimmed = code.trim();
            if is_endpoint_marker(trimmed) {
                endpoint_pending = true;
            }
            let Some(identifier) = definition_identifier(trimmed) else {
                continue;
            };
            let kind = if endpoint_pending || is_endpoint_definition(trimmed) {
                SymbolKind::Endpoint
            } else {
                SymbolKind::Definition
            };
            endpoint_pending = false;
            let key = (chunk.occurrence_key(), identifier.clone(), kind);
            if seen.insert(key) {
                occurrences.push(SymbolOccurrence::from_chunk(chunk, identifier, kind));
            }
        }
        previous_chunk = Some(chunk);
    }
    occurrences.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.range.cmp(&right.range))
            .then_with(|| left.start_char_offset.cmp(&right.start_char_offset))
            .then_with(|| left.identifier.cmp(&right.identifier))
            .then_with(|| left.kind.cmp(&right.kind))
    });
    occurrences
}

/// Determine whether lexical state may safely flow between two source Chunks.
/// 判断词法状态能否安全地在两个源码 Chunk 之间延续。
fn chunks_are_contiguous(previous: &Chunk, current: &Chunk) -> bool {
    if previous.blob_name != current.blob_name || previous.path != current.path {
        return false;
    }
    let next_line = previous.range.end_line.checked_add(1);
    let adjacent_lines = next_line == Some(current.range.start_line)
        && current.start_char_offset == 0
        && previous.content.ends_with('\n');
    let same_long_line = chunks_continue_same_line(previous, current);
    adjacent_lines || same_long_line
}

/// Determine whether two Chunks are successive fragments of one overlong source line.
/// 判断两个 Chunk 是否为同一超长源码行的连续片段。
fn chunks_continue_same_line(previous: &Chunk, current: &Chunk) -> bool {
    previous.blob_name == current.blob_name
        && previous.path == current.path
        && previous.range.start_line == previous.range.end_line
        && current.range.start_line == current.range.end_line
        && previous.range.end_line == current.range.start_line
        && previous
            .start_char_offset
            .checked_add(previous.content.chars().count())
            == Some(current.start_char_offset)
}

/// Lexical state retained while conservative language-neutral scanning crosses source fragments.
/// 保守的语言无关扫描跨越源码片段时保留的词法状态。
#[derive(Debug, Default)]
struct LexicalState {
    /// Whether scanning is currently inside a C-family block comment.
    /// 扫描当前是否位于 C 系块注释中。
    block_comment: bool,
    /// Active quote delimiter for a string contained on one physical line.
    /// 同一物理行内字符串当前使用的引号分隔符。
    quote: Option<char>,
    /// Whether the previous quoted character escaped the next character.
    /// 上一个引号内字符是否转义了下一个字符。
    escaped: bool,
    /// Whether the remainder of the current physical line is a line comment.
    /// 当前物理行的剩余内容是否为行注释。
    line_comment: bool,
}

impl LexicalState {
    /// Reset state that cannot cross a physical newline while preserving block comments.
    /// 重置不能跨越物理换行的状态，同时保留块注释状态。
    fn reset_line_state(&mut self) {
        self.quote = None;
        self.escaped = false;
        self.line_comment = false;
    }
}

/// Detect an annotation or decorator that promotes the following definition to an endpoint.
/// 检测将后续定义提升为入口的注解或装饰器。
fn is_endpoint_marker(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("tauri::command")
        || lower.starts_with("@app.")
        || lower.starts_with("@router.")
        || lower.contains("#[command")
        || lower.contains("#[route")
}

/// Detect an endpoint definition expressed on the same source line.
/// 检测在同一源码行中表达的入口定义。
fn is_endpoint_definition(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("tauri::command") || lower.contains("#[command") || lower.contains("#[route")
}

/// Extract one conservative definition identifier from a source line.
/// 从一行源码中提取一个保守的定义标识符。
fn definition_identifier(line: &str) -> Option<String> {
    if line.is_empty()
        || line.starts_with("//")
        || (line.starts_with('#') && !line.starts_with("#["))
    {
        return None;
    }
    for keyword in [
        "fn",
        "def",
        "function",
        "struct",
        "enum",
        "trait",
        "class",
        "interface",
        "type",
        "mod",
    ] {
        if let Some(identifier) = declaration_identifier_after_keyword(line, keyword) {
            return Some(identifier);
        }
    }
    if let Some(identifier) = go_function_identifier(line) {
        return Some(identifier);
    }
    c_family_callable_identifier(line)
}

/// Extract a declaration name immediately following one exact language keyword.
/// 提取紧跟在一个精确语言关键字后的声明名称。
fn declaration_identifier_after_keyword(line: &str, keyword: &str) -> Option<String> {
    for (start, _) in line.match_indices(keyword) {
        let prefix = line.get(..start)?;
        let before = prefix.chars().next_back();
        if before.is_some_and(|character| {
            !is_identifier_boundary(character) || matches!(character, '.' | ':' | '>')
        }) {
            continue;
        }
        let suffix_start = start.checked_add(keyword.len())?;
        let Some(suffix) = line.get(suffix_start..) else {
            continue;
        };
        if !suffix.chars().next().is_some_and(char::is_whitespace) {
            continue;
        }
        let mut declaration = suffix.trim_start();
        if keyword == "enum" {
            for qualifier in ["class", "struct"] {
                if let Some(remainder) = declaration.strip_prefix(qualifier)
                    && remainder.chars().next().is_some_and(char::is_whitespace)
                {
                    declaration = remainder.trim_start();
                    break;
                }
            }
        }
        let identifier = declaration
            .chars()
            .take_while(|character| character.is_alphanumeric() || *character == '_')
            .collect::<String>();
        if is_symbol_identifier(&identifier) {
            return Some(identifier);
        }
    }
    None
}

/// Remove quoted text and comments before language-neutral declaration heuristics run.
/// 在语言无关声明启发式处理前移除引号文本与注释。
fn strip_non_code_regions(line: &str, state: &mut LexicalState) -> String {
    let characters = line.chars().collect::<Vec<_>>();
    let mut output = String::with_capacity(line.len());
    let mut index = 0usize;
    while index < characters.len() {
        let character = characters[index];
        let next = characters.get(index + 1).copied();
        if state.line_comment {
            output.push(' ');
            index += 1;
            continue;
        }
        if state.block_comment {
            if character == '*' && next == Some('/') {
                output.push(' ');
                output.push(' ');
                index += 2;
                state.block_comment = false;
            } else {
                output.push(' ');
                index += 1;
            }
            continue;
        }
        if let Some(delimiter) = state.quote {
            output.push(' ');
            if state.escaped {
                state.escaped = false;
            } else if character == '\\' {
                state.escaped = true;
            } else if character == delimiter {
                state.quote = None;
            }
            index += 1;
            continue;
        }
        if character == '/' && next == Some('/') {
            state.line_comment = true;
            break;
        }
        if character == '/' && next == Some('*') {
            output.push(' ');
            output.push(' ');
            index += 2;
            state.block_comment = true;
            continue;
        }
        if character == '#' && next != Some('[') {
            state.line_comment = true;
            break;
        }
        if matches!(character, '\'' | '"' | '`') {
            output.push(' ');
            state.quote = Some(character);
            index += 1;
            continue;
        }
        output.push(character);
        index += 1;
    }
    output
}

/// Split one source line into language-neutral identifier tokens.
/// 将一行源码拆分为语言无关的标识符词元。
fn identifier_tokens(line: &str) -> Vec<String> {
    line.split(|character: char| !(character.is_alphanumeric() || character == '_'))
        .filter(|token| !token.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

/// Extract a Go function or method name while skipping an optional receiver.
/// 提取 Go 函数或方法名称，并跳过可选接收者。
fn go_function_identifier(line: &str) -> Option<String> {
    let declaration = line.trim_start();
    let after_func = declaration.strip_prefix("func")?;
    if !after_func.chars().next().is_some_and(char::is_whitespace) {
        return None;
    }
    let after_func = after_func.trim_start();
    let declaration = if after_func.starts_with('(') {
        let receiver_end = after_func.find(')')?;
        after_func.get(receiver_end + 1..)?.trim_start()
    } else {
        after_func
    };
    let identifier = declaration
        .chars()
        .take_while(|character| character.is_alphanumeric() || *character == '_')
        .collect::<String>();
    is_symbol_identifier(&identifier).then_some(identifier)
}

/// Extract a conservative C-family callable declaration name.
/// 提取一个保守的 C 系可调用声明名称。
fn c_family_callable_identifier(line: &str) -> Option<String> {
    let open = line.find('(')?;
    let prefix = line.get(..open)?.trim_end();
    if prefix.contains('.') || prefix.contains("->") || prefix.contains('=') {
        return None;
    }
    let close = line.get(open + 1..)?.find(')')? + open + 1;
    let suffix = line.get(close + 1..)?.trim_start();
    let has_declaration_prefix = prefix.chars().any(char::is_whitespace);
    if !has_declaration_prefix && !suffix.starts_with('{') {
        return None;
    }
    let first_prefix_token = identifier_tokens(prefix).into_iter().next();
    if first_prefix_token.as_deref().is_some_and(|token| {
        matches!(
            token,
            "return"
                | "await"
                | "yield"
                | "throw"
                | "new"
                | "delete"
                | "sizeof"
                | "if"
                | "for"
                | "while"
                | "switch"
                | "match"
        )
    }) {
        return None;
    }
    let identifier = prefix
        .chars()
        .rev()
        .take_while(|character| character.is_alphanumeric() || *character == '_')
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    if matches!(
        identifier.as_str(),
        "if" | "for" | "while" | "switch" | "return" | "fn" | "func" | "function" | "def"
    ) {
        return None;
    }
    is_symbol_identifier(&identifier).then_some(identifier)
}

/// Check the minimum language-neutral shape required for an exact symbol.
/// 检查精确符号所需的最小语言无关形态。
fn is_symbol_identifier(identifier: &str) -> bool {
    !identifier.is_empty()
        && identifier
            .chars()
            .next()
            .is_some_and(|character| character.is_alphabetic() || character == '_')
        && identifier
            .chars()
            .all(|character| character.is_alphanumeric() || character == '_')
}

/// Check for an exact identifier token rather than a substring with the same spelling.
/// 检查精确标识符词元，而不是仅具有相同拼写的子字符串。
pub(crate) fn content_contains_identifier(content: &str, identifier: &str) -> bool {
    if identifier.is_empty() {
        return false;
    }
    content.match_indices(identifier).any(|(start, _)| {
        let end = start + identifier.len();
        let before = content
            .get(..start)
            .and_then(|prefix| prefix.chars().next_back());
        let after = content.get(end..).and_then(|suffix| suffix.chars().next());
        before.is_none_or(is_identifier_boundary) && after.is_none_or(is_identifier_boundary)
    })
}

/// Return whether one adjacent character delimits an ASCII code identifier.
/// 返回相邻字符是否构成 ASCII 代码标识符边界。
fn is_identifier_boundary(character: char) -> bool {
    !character.is_alphanumeric() && !matches!(character, '_' | '$')
}

/// Return a stable textual name for a symbol kind.
/// 返回符号类型的稳定文本名称。
fn symbol_kind_name(kind: SymbolKind) -> &'static str {
    match kind {
        SymbolKind::Endpoint => "endpoint",
        SymbolKind::Definition => "definition",
        SymbolKind::Reference => "reference",
    }
}

#[cfg(test)]
// Test fixtures intentionally use panic-style extraction to keep invariant failures local.
// 测试夹具有意使用恐慌式提取，使不变量失败保持在本地。
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::model::{Chunk, ChunkKind};

    /// Verify endpoint definitions rank ahead of normal definitions and references.
    /// 验证入口定义会排在普通定义和引用之前。
    #[test]
    fn endpoint_occurrences_rank_first() {
        let blob_name = "a".repeat(64);
        let chunk = Chunk::new(
            blob_name.clone(),
            "src/lib.rs",
            "fn run() {}",
            1,
            1,
            ChunkKind::Recursive,
        )
        .unwrap();
        let occurrences = vec![
            SymbolOccurrence::from_chunk(&chunk, "run", SymbolKind::Reference),
            SymbolOccurrence::from_chunk(&chunk, "run", SymbolKind::Endpoint),
        ];
        let hits = search_exact_symbols(
            &occurrences,
            &["run".to_owned()],
            &[blob_name].into_iter().collect(),
            10,
        );
        assert_eq!(hits[0].kind, HitKind::Endpoint);
    }

    /// Verify exact search never widens beyond the declared Blob scope.
    /// 验证精确检索不会扩大到声明 Blob 范围之外。
    #[test]
    fn exact_search_respects_scope() {
        let blob_a = "a".repeat(64);
        let blob_b = "b".repeat(64);
        let chunk_a = Chunk::new(
            blob_a.clone(),
            "src/a.rs",
            "fn run() {}",
            1,
            1,
            ChunkKind::Recursive,
        )
        .unwrap();
        let chunk_b = Chunk::new(
            blob_b,
            "src/b.rs",
            "fn run() {}",
            1,
            1,
            ChunkKind::Recursive,
        )
        .unwrap();
        let occurrences = vec![
            SymbolOccurrence::from_chunk(&chunk_a, "run", SymbolKind::Definition),
            SymbolOccurrence::from_chunk(&chunk_b, "run", SymbolKind::Definition),
        ];
        let hits = search_exact_symbols(
            &occurrences,
            &["run".to_owned()],
            &[blob_a.clone()].into_iter().collect(),
            10,
        );
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].blob_name, blob_a);
    }

    /// Verify exact recall rejects identifier substrings inside larger code tokens.
    /// 验证精确召回会拒绝更大代码词元内部的标识符子字符串。
    #[test]
    fn exact_search_requires_an_identifier_boundary() {
        let blob_name = "a".repeat(64);
        let chunk = Chunk::new(
            blob_name.clone(),
            "src/lib.rs",
            "fn foobar() {}",
            1,
            1,
            ChunkKind::Recursive,
        )
        .unwrap();
        let occurrence = SymbolOccurrence::from_chunk(&chunk, "foo", SymbolKind::Definition);
        let allowed = [blob_name].into_iter().collect();

        assert!(search_exact_symbols(&[occurrence], &["foo".to_owned()], &allowed, 1).is_empty());
    }

    /// Verify the built-in lexical extractor covers common Rust, Python, Go, and C definitions.
    /// 验证内置词法提取器覆盖常见 Rust、Python、Go 和 C 定义。
    #[test]
    fn extracts_common_language_definitions() {
        let chunk = Chunk::new(
            "blob",
            "src/sample.txt",
            "pub fn rust_fn() {}\ndef python_fn():\nfunc (s *Server) Serve() {}\nint c_function(int value) { return value; }",
            1,
            4,
            ChunkKind::Recursive,
        )
        .unwrap();
        let identifiers = extract_symbols_from_chunks(&[chunk])
            .into_iter()
            .map(|occurrence| occurrence.identifier)
            .collect::<BTreeSet<_>>();
        assert!(identifiers.contains("rust_fn"));
        assert!(identifiers.contains("python_fn"));
        assert!(identifiers.contains("Serve"));
        assert!(identifiers.contains("c_function"));
    }

    /// Verify a plain callable expression is not indexed as a C-family definition.
    /// 验证普通调用表达式不会被索引为 C-family 定义。
    #[test]
    fn does_not_extract_plain_function_calls_as_definitions() {
        let chunk = Chunk::new(
            "blob",
            "src/sample.c",
            "run_task(value);",
            1,
            1,
            ChunkKind::Recursive,
        )
        .unwrap();

        assert!(extract_symbols_from_chunks(&[chunk]).is_empty());
    }

    /// Verify quoted declaration text and expression-prefixed calls do not become symbols.
    /// 验证引号中的声明文本和表达式前缀调用不会变成符号。
    #[test]
    fn ignores_declaration_text_inside_strings_and_expressions() {
        let chunk = Chunk::new(
            "blob",
            "src/lib.rs",
            "let text = \"fn forged() {}\";\nreturn called();\nawait invoked();\n/*\nfn hidden() {}\n*/",
            1,
            6,
            ChunkKind::Recursive,
        )
        .unwrap();

        assert!(extract_symbols_from_chunks(&[chunk]).is_empty());
    }

    /// Verify callable type syntax never becomes a false definition occurrence.
    /// 验证可调用类型语法不会变成错误的定义出现记录。
    #[test]
    fn ignores_rust_and_go_function_types() {
        let chunk = Chunk::new(
            "blob",
            "src/types.txt",
            "let callback: fn(i32) -> i32 = run;\nvar handler func(int) string",
            1,
            2,
            ChunkKind::Recursive,
        )
        .unwrap();

        assert!(extract_symbols_from_chunks(&[chunk]).is_empty());
    }

    /// Verify a scoped enum records its declared name instead of its class qualifier.
    /// 验证限定枚举记录声明名称而不是 class 限定词。
    #[test]
    fn extracts_cpp_scoped_enum_name() {
        let chunk = Chunk::new(
            "blob",
            "src/types.hpp",
            "enum class ConnectionState { Ready, Failed };",
            1,
            1,
            ChunkKind::Recursive,
        )
        .unwrap();

        let symbols = extract_symbols_from_chunks(&[chunk]);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].identifier, "ConnectionState");
    }

    /// Verify one-character declarations remain available to exact retrieval.
    /// 验证单字符声明仍可用于精确检索。
    #[test]
    fn extracts_single_character_definition_names() {
        let chunk = Chunk::new(
            "blob",
            "src/lib.rs",
            "fn f() {}",
            1,
            1,
            ChunkKind::Recursive,
        )
        .unwrap();

        let symbols = extract_symbols_from_chunks(&[chunk]);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].identifier, "f");
    }

    /// Verify endpoint annotations promote the following definition without a graph dependency.
    /// 验证入口注解会在不依赖图谱的情况下提升后续定义。
    #[test]
    fn endpoint_annotation_promotes_following_definition() {
        let chunk = Chunk::new(
            "blob",
            "src/commands.rs",
            "#[tauri::command]\npub async fn open_project() {}",
            1,
            2,
            ChunkKind::Recursive,
        )
        .unwrap();
        let occurrences = extract_symbols_from_chunks(&[chunk]);
        assert_eq!(occurrences.len(), 1);
        assert_eq!(occurrences[0].identifier, "open_project");
        assert_eq!(occurrences[0].kind, SymbolKind::Endpoint);
    }

    /// Verify lexical state crosses adjacent Chunks but never leaks across unrelated Blobs.
    /// 验证词法状态可跨越相邻 Chunk，但不会泄漏到无关 Blob。
    #[test]
    fn preserves_lexical_state_across_contiguous_chunks() {
        let comment_start = Chunk::new(
            "blob-a",
            "src/lib.rs",
            "/* hidden declaration\n",
            1,
            1,
            ChunkKind::Recursive,
        )
        .unwrap();
        let comment_end = Chunk::new(
            "blob-a",
            "src/lib.rs",
            "fn forged() {}\n*/\n#[tauri::command]\n",
            2,
            4,
            ChunkKind::Recursive,
        )
        .unwrap();
        let endpoint = Chunk::new(
            "blob-a",
            "src/lib.rs",
            "pub fn exposed() {}",
            5,
            5,
            ChunkKind::Recursive,
        )
        .unwrap();
        let unrelated = Chunk::new(
            "blob-b",
            "src/other.rs",
            "fn independent() {}",
            1,
            1,
            ChunkKind::Recursive,
        )
        .unwrap();

        let occurrences =
            extract_symbols_from_chunks(&[comment_start, comment_end, endpoint, unrelated]);
        assert_eq!(occurrences.len(), 2);
        assert_eq!(occurrences[0].identifier, "exposed");
        assert_eq!(occurrences[0].kind, SymbolKind::Endpoint);
        assert_eq!(occurrences[1].identifier, "independent");
        assert_eq!(occurrences[1].kind, SymbolKind::Definition);
    }

    /// Verify quote and line-comment state survives fragments of one overlong physical line.
    /// 验证引号和行注释状态可跨越同一超长物理行的多个片段。
    #[test]
    fn preserves_inline_state_across_long_line_fragments() {
        let quote_start = Chunk::new(
            "blob",
            "src/lib.rs",
            "let text = \"",
            1,
            1,
            ChunkKind::Recursive,
        )
        .unwrap();
        let quote_end = Chunk::new_with_start_char_offset(
            "blob",
            12,
            "src/lib.rs",
            "fn forged() {}\";",
            1,
            1,
            ChunkKind::Recursive,
        )
        .unwrap();
        let comment_start = Chunk::new(
            "blob",
            "src/lib.rs",
            "// hidden ",
            2,
            2,
            ChunkKind::Recursive,
        )
        .unwrap();
        let comment_end = Chunk::new_with_start_char_offset(
            "blob",
            10,
            "src/lib.rs",
            "fn forged_two() {}",
            2,
            2,
            ChunkKind::Recursive,
        )
        .unwrap();
        let visible = Chunk::new(
            "blob",
            "src/lib.rs",
            "fn visible() {}",
            3,
            3,
            ChunkKind::Recursive,
        )
        .unwrap();

        let occurrences = extract_symbols_from_chunks(&[
            quote_start,
            quote_end,
            comment_start,
            comment_end,
            visible,
        ]);
        assert_eq!(occurrences.len(), 1);
        assert_eq!(occurrences[0].identifier, "visible");
    }
}
