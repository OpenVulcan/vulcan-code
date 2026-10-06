//! Deterministic query intent classification and facet planning.
//! 确定性的查询意图分类和分面规划。

use std::error::Error;
use std::fmt::{Display, Formatter};

/// Maximum Unicode scalar count accepted for one LLM rewrite candidate.
/// 单个 LLM 改写候选允许的最大 Unicode 标量数量。
const MAX_REWRITE_CHARS: usize = 80;

/// Prompt scaffolding fragments that must never enter semantic retrieval.
/// 绝不能进入语义检索的提示词脚手架片段。
const PROMPT_ECHO_MARKERS: &[&str] = &[
    "改写策略",
    "用户查询",
    "改写后的查询",
    "搜索关键词",
    "召回率",
    "每行一个",
    "不要编号",
    "文件名版本",
    "英文关键词版本",
    "功能描述版本",
    "查询改写助手",
    "要求:",
];

/// Semantic intent used to choose retrieval behavior.
/// 用于选择检索行为的语义意图。
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum QueryIntent {
    /// Locate a named symbol or its definition.
    /// 定位具名符号或其定义。
    Symbol,
    /// Follow a call direction or execution chain.
    /// 追踪调用方向或执行链路。
    CallChain,
    /// Locate usages or references of a symbol.
    /// 定位符号的使用或引用。
    Reference,
    /// Identify files or paths before selecting content.
    /// 先定位文件或路径，再选择内容。
    Path,
    /// Find implementation details for a described feature.
    /// 根据功能描述定位实现细节。
    Feature,
    /// Find architecture, mechanism, or lifecycle explanations.
    /// 定位架构、机制或生命周期说明。
    Overview,
    /// A reserved multi-facet strategy for future explicit planners.
    /// 为未来明确规划器保留的多条件策略。
    Compound,
}

/// Explicit limits for heuristic query planning.
/// 启发式查询规划的明确限制。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueryPlannerConfig {
    /// Maximum number of query variants including the original query.
    /// 包含原始查询在内的最大查询变体数量。
    pub max_queries: usize,
    /// Minimum character count for a facet to enter planning.
    /// 分面进入规划所需的最小字符数量。
    pub min_facet_chars: usize,
}

impl Default for QueryPlannerConfig {
    /// Return the documented OCE-inspired planning limits.
    /// 返回 OCE 思路中记录的默认规划限制。
    fn default() -> Self {
        Self {
            max_queries: 4,
            min_facet_chars: 8,
        }
    }
}

/// A planned query and the retrieval features it is allowed to activate.
/// 一个规划后的查询以及它允许激活的检索特性。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryPlan {
    /// Original user query after whitespace normalization.
    /// 空白规范化后的用户原始查询。
    pub original_query: String,
    /// Heuristically classified intent.
    /// 通过启发式分类得到的查询意图。
    pub intent: QueryIntent,
    /// Extracted code identifiers used by exact retrieval and endpoint promotion.
    /// 用于精确检索和入口提升的代码标识符。
    pub identifiers: Vec<String>,
    /// Ordered query variants, always retaining the original query first.
    /// 有序查询变体，始终将原始查询保留在第一位。
    pub queries: Vec<String>,
    /// Whether the path index should participate.
    /// 是否应参与路径索引。
    pub enable_path_index: bool,
    /// Whether an external LLM query rewrite may be requested.
    /// 是否允许请求外部 LLM 查询改写。
    pub enable_query_rewrite: bool,
    /// Whether the host Rerank capability may be requested.
    /// 是否允许请求宿主重排能力。
    pub enable_llm_rerank: bool,
    /// Whether the query explicitly asks to locate a symbol definition or implementation.
    /// 查询是否明确要求定位符号定义或实现位置。
    pub promote_endpoint_definitions: bool,
}

/// Errors raised when planning an empty or invalid query.
/// 规划空查询或无效查询时产生的错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryError {
    /// The query has no non-whitespace content.
    /// 查询不包含非空白内容。
    EmptyQuery,
    /// The planner configuration contains a zero limit.
    /// 规划器配置包含零限制。
    InvalidConfig,
}

impl Display for QueryError {
    /// Formats a query planning error for deterministic diagnostics.
    /// 将查询规划错误格式化为确定性的诊断文本。
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyQuery => formatter.write_str("query must not be empty"),
            Self::InvalidConfig => formatter.write_str("query planner configuration is invalid"),
        }
    }
}

impl Error for QueryError {}

/// Plan queries without invoking an LLM or a provider.
/// 不调用 LLM 或供应商完成查询规划。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueryPlanner {
    /// Explicit planner limits.
    /// 明确的规划器限制。
    pub config: QueryPlannerConfig,
}

impl QueryPlanner {
    /// Construct a planner after validating its limits.
    /// 校验规划限制后创建查询规划器。
    pub fn new(config: QueryPlannerConfig) -> Result<Self, QueryError> {
        if config.max_queries == 0 || config.min_facet_chars == 0 {
            return Err(QueryError::InvalidConfig);
        }
        Ok(Self { config })
    }

    /// Classify a query and extract its exact-search identifiers.
    /// 分类查询并提取用于精确搜索的标识符。
    pub fn classify(&self, query: &str) -> Result<(QueryIntent, Vec<String>), QueryError> {
        let normalized = normalize_query(query)?;
        let identifiers = extract_identifiers(&normalized);
        let unquoted = remove_backtick_contents(&normalized).to_lowercase();
        let has_symbol = has_symbol_anchor(&normalized);
        let has_call_verb = contains_terms(
            &unquoted,
            &[
                "call",
                "invoke",
                "trigger",
                "execute",
                "from",
                "to",
                "path",
                "flow",
                "pipeline",
                "调用",
                "触发",
                "执行",
                "从",
                "到",
                "路径",
                "流程",
                "完整",
                "如何被",
                "如何从",
            ],
        );
        let has_reference_verb = contains_terms(
            &unquoted,
            &[
                "使用",
                "引用",
                "导入",
                "依赖",
                "消费",
                "接收",
                "use",
                "used",
                "uses",
                "usage",
                "reference",
                "references",
                "referenced",
                "import",
                "imports",
                "imported",
                "depend",
                "depends",
                "dependent",
                "consume",
                "consumes",
                "consumed",
                "receive",
                "receives",
                "received",
            ],
        );
        let has_path_keyword = contains_terms(
            &unquoted,
            &[
                "file",
                "config",
                "where",
                "location",
                "dependency",
                "文件",
                "配置",
                "在哪里",
                "在哪",
                "哪个文件",
                "翻译文件",
                "依赖",
            ],
        );
        let has_feature_marker = contains_terms(
            &unquoted,
            &[
                "feature",
                "implement",
                "implementation",
                "logic",
                "code",
                "mechanism",
                "strategy",
                "behavior",
                "how",
                "功能",
                "实现",
                "逻辑",
                "代码",
                "机制",
                "策略",
                "如何",
                "怎样",
                "怎么",
            ],
        );
        let has_overview_signal = contains_terms(
            &unquoted,
            &[
                "architecture",
                "implementation",
                "event handling",
                "state management",
                "scheduling",
                "dispatch",
                "mechanism",
                "workflow",
                "架构",
                "实现",
                "事件处理",
                "状态管理",
                "调度",
                "机制",
                "流程",
            ],
        );

        let intent = if has_symbol && has_call_verb {
            QueryIntent::CallChain
        } else if has_symbol && has_reference_verb {
            QueryIntent::Reference
        } else if has_symbol {
            QueryIntent::Symbol
        } else if has_file_extension_token(&normalized) || (has_path_keyword && !has_feature_marker)
        {
            QueryIntent::Path
        } else if has_overview_signal {
            QueryIntent::Overview
        } else {
            QueryIntent::Feature
        };
        Ok((intent, identifiers))
    }

    /// Build a deterministic original-query-plus-facets plan.
    /// 构建确定性的原始查询加分面查询计划。
    pub fn plan(&self, query: &str) -> Result<QueryPlan, QueryError> {
        let original_query = normalize_query(query)?;
        let (intent, identifiers) = self.classify(&original_query)?;
        let mut queries = vec![original_query.clone()];
        let mut facets = Vec::new();
        let mut seen = vec![original_query.to_lowercase()];
        for facet in split_facets(query) {
            let key = facet.to_lowercase();
            if facet.chars().count() < self.config.min_facet_chars || seen.contains(&key) {
                continue;
            }
            seen.push(key);
            facets.push(facet);
            if facets.len() >= self.config.max_queries.max(2) {
                break;
            }
        }
        if facets.len() >= 2 {
            queries.extend(
                facets
                    .into_iter()
                    .take(self.config.max_queries.saturating_sub(1)),
            );
        }
        let promote_endpoint_definitions =
            intent == QueryIntent::Symbol && has_definition_location_marker(&original_query);

        Ok(QueryPlan {
            original_query,
            intent,
            identifiers,
            queries,
            enable_path_index: intent == QueryIntent::Path,
            enable_query_rewrite: matches!(
                intent,
                QueryIntent::Symbol
                    | QueryIntent::Reference
                    | QueryIntent::Path
                    | QueryIntent::Feature
                    | QueryIntent::Compound
            ),
            enable_llm_rerank: matches!(
                intent,
                QueryIntent::Symbol
                    | QueryIntent::CallChain
                    | QueryIntent::Path
                    | QueryIntent::Feature
                    | QueryIntent::Overview
                    | QueryIntent::Compound
            ),
            promote_endpoint_definitions,
        })
    }

    /// Merge externally supplied rewrites without allowing them to replace the original query.
    /// 合并外部提供的改写，同时不允许改写替换原始查询。
    pub fn merge_rewrites(
        &self,
        original_query: &str,
        rewrites: impl IntoIterator<Item = String>,
    ) -> Result<Vec<String>, QueryError> {
        let original_query = normalize_query(original_query)?;
        let mut seen = vec![original_query.to_lowercase()];
        let mut queries = vec![original_query];
        for rewrite in rewrites {
            if queries.len() >= self.config.max_queries {
                break;
            }
            let cleaned = clean_rewrite(&rewrite);
            let key = cleaned.to_lowercase();
            if !is_valid_rewrite(&cleaned) || seen.contains(&key) {
                continue;
            }
            seen.push(key);
            queries.push(cleaned);
        }
        Ok(queries)
    }
}

/// Normalize whitespace while preserving meaningful query punctuation.
/// 规范化空白，同时保留有意义的查询标点。
pub fn normalize_query(query: &str) -> Result<String, QueryError> {
    let normalized = query.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        Err(QueryError::EmptyQuery)
    } else {
        Ok(normalized)
    }
}

/// Extract backtick, snake-case, qualified, and PascalCase identifiers.
/// 提取反引号、snake_case、限定路径和 PascalCase 标识符。
pub fn extract_identifiers(query: &str) -> Vec<String> {
    let mut identifiers = Vec::new();
    let mut in_backticks = false;
    let mut quoted = String::new();
    for character in query.chars() {
        if character == '`' {
            if in_backticks {
                push_valid_identifier(&mut identifiers, &quoted);
                quoted.clear();
            }
            in_backticks = !in_backticks;
            continue;
        }
        if in_backticks {
            quoted.push(character);
        }
    }

    let mut token = String::new();
    for character in query.chars() {
        if is_identifier_character(character) {
            token.push(character);
        } else {
            push_structural_identifier(&mut identifiers, &token);
            token.clear();
        }
    }
    push_structural_identifier(&mut identifiers, &token);
    push_contextual_type_identifiers(&mut identifiers, query);
    identifiers
}

/// Detect a structural symbol anchor without treating contextual PascalCase type hints as symbols.
/// 检测结构化符号锚点，同时不把带上下文的 PascalCase 类型提示直接当成符号意图。
fn has_symbol_anchor(query: &str) -> bool {
    let mut in_backticks = false;
    let mut quoted = String::new();
    for character in query.chars() {
        if character == '`' {
            if in_backticks && is_valid_identifier(quoted.trim()) {
                return true;
            }
            quoted.clear();
            in_backticks = !in_backticks;
        } else if in_backticks {
            quoted.push(character);
        }
    }

    identifier_tokens_with_offsets(query)
        .into_iter()
        .any(|(token, _, _)| is_structural_identifier(token))
}

/// Detect the explicit location language required before endpoint definitions receive priority.
/// 检测入口定义获得优先级前所需的明确位置表达。
fn has_definition_location_marker(query: &str) -> bool {
    let normalized = query.to_lowercase();
    let has_localized_marker = [
        "函数位置",
        "定义位置",
        "实现位置",
        "源码位置",
        "哪个文件",
        "在哪个文件",
        "在哪里定义",
        "哪里定义",
        "函数在哪",
    ]
    .iter()
    .any(|marker| normalized.contains(marker));
    has_localized_marker
        || contains_terms(&normalized, &["defined", "definition", "implementation"])
}

/// Remove quoted content before evaluating natural-language verb signals.
/// 在判断自然语言动词信号前移除反引号内容。
fn remove_backtick_contents(query: &str) -> String {
    let mut output = String::new();
    let mut in_backticks = false;
    for character in query.chars() {
        if character == '`' {
            in_backticks = !in_backticks;
            output.push(' ');
        } else if !in_backticks {
            output.push(character);
        } else {
            output.push(' ');
        }
    }
    output
}

/// Split a normalized query into meaningful sentence or list facets.
/// 将规范化查询拆分为有意义的句子或列表分面。
fn split_facets(query: &str) -> Vec<String> {
    let characters = query.chars().collect::<Vec<_>>();
    let mut facets = Vec::new();
    let mut current = String::new();
    for (index, character) in characters.iter().copied().enumerate() {
        let sentence_period = character == '.'
            && characters
                .get(index + 1)
                .is_none_or(|next| next.is_whitespace());
        if matches!(
            character,
            '\r' | '\n' | '!' | '?' | ';' | '。' | '！' | '？' | '；'
        ) || sentence_period
        {
            let facet = trim_list_marker(&current);
            if !facet.is_empty() {
                facets.push(facet);
            }
            current.clear();
        } else {
            current.push(character);
        }
    }
    let facet = trim_list_marker(&current);
    if !facet.is_empty() {
        facets.push(facet);
    }
    facets
}

/// Remove common list markers from a query facet.
/// 移除查询分面开头常见的列表标记。
fn trim_list_marker(value: &str) -> String {
    let trimmed = value.trim();
    let bytes = trimmed.as_bytes();
    let marker_end = if bytes.len() >= 2
        && matches!(bytes[0], b'-' | b'*' | b'+')
        && bytes[1].is_ascii_whitespace()
    {
        Some(1)
    } else {
        let digit_end = bytes
            .iter()
            .position(|byte| !byte.is_ascii_digit())
            .unwrap_or(bytes.len());
        if digit_end > 0
            && digit_end + 1 < bytes.len()
            && matches!(bytes[digit_end], b'.' | b')')
            && bytes[digit_end + 1].is_ascii_whitespace()
        {
            Some(digit_end + 1)
        } else {
            None
        }
    };
    let content = marker_end.map_or(trimmed, |end| &trimmed[end..]);
    content.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Clean one line of an externally generated rewrite.
/// 清理一行外部生成的查询改写。
fn clean_rewrite(value: &str) -> String {
    let unquoted = value
        .trim()
        .trim_matches(|character| character == '`' || character == '"');
    let without_ascii_marker = trim_list_marker(unquoted);
    let without_bullet = without_ascii_marker
        .strip_prefix('•')
        .and_then(|rest| {
            rest.chars()
                .next()
                .is_some_and(char::is_whitespace)
                .then_some(rest)
        })
        .unwrap_or(&without_ascii_marker);
    without_bullet
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Reject tiny, explanatory, or prompt-echo rewrite candidates before Embedding.
/// 在向量化前拒绝过短、解释性或回显提示词的改写候选。
fn is_valid_rewrite(candidate: &str) -> bool {
    let length = candidate.chars().count();
    (3..=MAX_REWRITE_CHARS).contains(&length)
        && !PROMPT_ECHO_MARKERS
            .iter()
            .any(|marker| candidate.contains(marker))
}

/// Return whether a character can belong to a code identifier token.
/// 判断字符是否可以属于代码标识符词元。
fn is_identifier_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '_' | ':' | '$')
}

/// Add a structurally anchored identifier while preserving first-seen order.
/// 添加具有结构锚点的标识符，同时保留首次出现顺序。
fn push_structural_identifier(identifiers: &mut Vec<String>, token: &str) {
    let token = token.trim_matches(':');
    if is_structural_identifier(token) && !identifiers.iter().any(|existing| existing == token) {
        identifiers.push(token.to_owned());
        push_qualified_leaf_identifier(identifiers, token);
    }
}

/// Add any syntactically valid identifier while preserving first-seen order.
/// 添加任意语法有效的标识符，同时保留首次出现顺序。
fn push_valid_identifier(identifiers: &mut Vec<String>, token: &str) {
    let token = token.trim();
    if is_valid_identifier(token) && !identifiers.iter().any(|existing| existing == token) {
        identifiers.push(token.to_owned());
        push_qualified_leaf_identifier(identifiers, token);
    }
}

/// Add the leaf of a qualified identifier for fallback indexes that store declaration names.
/// 为仅存储声明名称的回退索引添加限定标识符的叶子名称。
fn push_qualified_leaf_identifier(identifiers: &mut Vec<String>, token: &str) {
    let Some((_, leaf)) = token.rsplit_once("::") else {
        return;
    };
    if is_valid_identifier(leaf) && !identifiers.iter().any(|existing| existing == leaf) {
        identifiers.push(leaf.to_owned());
    }
}

/// Check whether a token carries the snake-case or qualified-path anchor used by OCE.
/// 检查词元是否带有 OCE 使用的 snake_case 或限定路径锚点。
fn is_structural_identifier(token: &str) -> bool {
    if !is_valid_identifier(token) {
        return false;
    }
    if token.contains("::") {
        return true;
    }
    let bytes = token.as_bytes();
    bytes.first().is_some_and(u8::is_ascii_lowercase)
        && bytes
            .iter()
            .position(|byte| *byte == b'_')
            .is_some_and(|index| {
                index > 0
                    && index + 1 < bytes.len()
                    && bytes[..index]
                        .iter()
                        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
                    && bytes[index + 1..].iter().all(|byte| {
                        byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'_'
                    })
            })
}

/// Check the ASCII identifier grammar shared by quoted, snake-case, and qualified names.
/// 检查反引号、snake_case 与限定名称共同使用的 ASCII 标识符语法。
fn is_valid_identifier(token: &str) -> bool {
    let mut segments = token.split("::");
    let valid_segment = |segment: &str| {
        let mut characters = segment.chars();
        characters.next().is_some_and(|character| {
            character.is_ascii_alphabetic() || matches!(character, '_' | '$')
        }) && characters
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '$'))
    };
    let mut count = 0usize;
    for segment in &mut segments {
        if !valid_segment(segment) {
            return false;
        }
        count += 1;
    }
    count > 0
}

/// Return identifier-like ASCII tokens together with their byte ranges in the query.
/// 返回查询中的 ASCII 标识符词元及其字节范围。
fn identifier_tokens_with_offsets(query: &str) -> Vec<(&str, usize, usize)> {
    let mut tokens = Vec::new();
    let mut start = None;
    for (index, character) in query.char_indices() {
        if is_identifier_character(character) {
            start.get_or_insert(index);
        } else if let Some(token_start) = start.take() {
            tokens.push((&query[token_start..index], token_start, index));
        }
    }
    if let Some(token_start) = start {
        tokens.push((&query[token_start..], token_start, query.len()));
    }
    tokens
}

/// Extract PascalCase names only when the following text identifies a type declaration concept.
/// 仅在后续文本明确表示类型声明概念时提取 PascalCase 名称。
fn push_contextual_type_identifiers(identifiers: &mut Vec<String>, query: &str) {
    for (token, _, end) in identifier_tokens_with_offsets(query) {
        if !token
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_uppercase())
            || !is_valid_identifier(token)
            || token.contains("::")
        {
            continue;
        }
        let mut suffix = query[end..].trim_start();
        if let Some(remainder) = suffix.strip_prefix('的') {
            suffix = remainder;
        }
        if let Some(remainder) = suffix.strip_prefix("前后端") {
            suffix = remainder;
        }
        suffix = suffix.trim_start();
        let lowered = suffix.to_ascii_lowercase();
        let has_type_context = ["类型", "类", "接口", "结构", "定义"]
            .iter()
            .any(|marker| suffix.starts_with(marker))
            || [
                "type",
                "interface",
                "struct",
                "enum",
                "trait",
                "class",
                "definition",
            ]
            .iter()
            .any(|marker| starts_with_ascii_term(&lowered, marker));
        if has_type_context {
            push_valid_identifier(identifiers, token);
        }
    }
}

/// Check a text for Unicode substrings or ASCII terms bounded on both sides.
/// 使用 Unicode 子串或两侧均有词边界的 ASCII 词项检查文本。
fn contains_terms(text: &str, signals: &[&str]) -> bool {
    signals.iter().any(|signal| {
        if signal.is_ascii() {
            text.match_indices(signal).any(|(index, _)| {
                let end = index + signal.len();
                let has_leading_boundary = index == 0
                    || !text[..index].chars().next_back().is_some_and(|character| {
                        character.is_ascii_alphanumeric() || character == '_'
                    });
                let has_trailing_boundary = end == text.len()
                    || !text[end..].chars().next().is_some_and(|character| {
                        character.is_ascii_alphanumeric() || character == '_'
                    });
                has_leading_boundary && has_trailing_boundary
            })
        } else {
            text.contains(signal)
        }
    })
}

/// Check whether text starts with an ASCII term and does not continue an identifier prefix.
/// 检查文本是否以 ASCII 词项开头且不会延续成标识符前缀。
fn starts_with_ascii_term(text: &str, term: &str) -> bool {
    text.strip_prefix(term).is_some_and(|remainder| {
        !remainder
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_alphanumeric() || character == '_')
    })
}

/// Detect a filename extension-like token for path-oriented queries.
/// 检测类似文件扩展名的词元以识别路径查询。
fn has_file_extension_token(query: &str) -> bool {
    query.split_whitespace().any(|token| {
        let token = token.trim_matches(|character: char| {
            !character.is_ascii_alphanumeric() && !matches!(character, '_' | '-' | '.')
        });
        let file_name = token.rsplit(['/', '\\']).next().unwrap_or(token);
        let Some((stem, extension)) = file_name.rsplit_once('.') else {
            return false;
        };
        let stem = stem.rsplit('.').next().unwrap_or(stem);
        !stem.is_empty()
            && stem.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '_' | '-')
            })
            && (1..=8).contains(&extension.len())
            && extension
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphabetic)
            && extension
                .chars()
                .all(|character| character.is_ascii_alphanumeric())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify symbol, call-chain, reference, path, and overview classification priority.
    /// 验证符号、调用链、引用、路径和概览分类优先级。
    #[test]
    fn classifies_documented_intents() {
        let planner = QueryPlanner::new(QueryPlannerConfig::default()).unwrap();
        assert_eq!(
            planner
                .classify("where is `UserService::run` defined")
                .unwrap()
                .0,
            QueryIntent::Symbol
        );
        assert_eq!(
            planner
                .classify("trace `UserService::run` call chain")
                .unwrap()
                .0,
            QueryIntent::CallChain
        );
        assert_eq!(
            planner.classify("where is `run_function` used").unwrap().0,
            QueryIntent::Reference
        );
        assert_eq!(
            planner.classify("find src/runtime.rs").unwrap().0,
            QueryIntent::Path
        );
        assert_eq!(
            planner
                .classify("explain the architecture lifecycle")
                .unwrap()
                .0,
            QueryIntent::Overview
        );
    }

    /// Verify natural-language capitalization, substrings, and version numbers are not code anchors.
    /// 验证自然语言大写、子串和版本号不会被误判为代码锚点。
    #[test]
    fn avoids_natural_language_and_version_false_positives() {
        let planner = QueryPlanner::new(QueryPlannerConfig::default()).unwrap();
        assert_eq!(
            planner.classify("How does authentication work").unwrap().0,
            QueryIntent::Feature
        );
        assert_eq!(
            planner.classify("show profile behavior").unwrap().0,
            QueryIntent::Feature
        );
        assert_eq!(
            planner.classify("release 3.13 behavior").unwrap().0,
            QueryIntent::Feature
        );
        assert!(!has_file_extension_token(".json"));
    }

    /// Verify identifier prefixes do not impersonate complete ASCII intent terms.
    /// 验证标识符前缀不会冒充完整的 ASCII 意图词项。
    #[test]
    fn requires_both_ascii_term_boundaries() {
        let planner = QueryPlanner::new(QueryPlannerConfig::default()).unwrap();

        assert_eq!(
            planner.classify("show user_config behavior").unwrap().0,
            QueryIntent::Symbol
        );
        assert_eq!(
            planner.classify("callback_handler location").unwrap().0,
            QueryIntent::Symbol
        );
    }

    /// Verify filename and non-feature path signals still select path retrieval.
    /// 验证文件名和非功能类路径信号仍会选择路径检索。
    #[test]
    fn preserves_strong_path_signals() {
        let planner = QueryPlanner::new(QueryPlannerConfig::default()).unwrap();
        assert_eq!(
            planner.classify("where is config.json?").unwrap().0,
            QueryIntent::Path
        );
        assert_eq!(
            planner.classify("where is the config file").unwrap().0,
            QueryIntent::Path
        );
    }

    /// Verify endpoint promotion is limited to explicit symbol-location requests.
    /// 验证入口提升仅限明确的符号位置查询。
    #[test]
    fn endpoint_promotion_requires_a_location_marker() {
        let planner = QueryPlanner::new(QueryPlannerConfig::default()).unwrap();
        assert!(
            planner
                .plan("where is `open_project` definition")
                .unwrap()
                .promote_endpoint_definitions
        );
        assert!(
            !planner
                .plan("explain `open_project` behavior")
                .unwrap()
                .promote_endpoint_definitions
        );
        assert!(
            !planner
                .plan("why is `open_project` undefined")
                .unwrap()
                .promote_endpoint_definitions
        );
    }

    /// Verify exact identifiers follow structural anchors and contextual type hints.
    /// 验证精确标识符遵循结构锚点与带上下文的类型提示。
    #[test]
    fn extracts_only_supported_exact_identifier_shapes() {
        assert_eq!(
            extract_identifiers("find `run` and parse_config"),
            vec!["run", "parse_config"]
        );
        assert_eq!(
            extract_identifiers("UserService type definition"),
            vec!["UserService"]
        );
        assert!(extract_identifiers("UserService typename").is_empty());
        assert!(extract_identifiers("How does authentication work").is_empty());
        assert_eq!(
            extract_identifiers("trace `UserService::run`"),
            vec!["UserService::run", "run"]
        );
    }

    /// Verify facets keep the original query and obey the configured limit.
    /// 验证分面保留原始查询并遵守配置限制。
    #[test]
    fn plans_original_query_and_facets() {
        let planner = QueryPlanner::new(QueryPlannerConfig {
            max_queries: 3,
            min_facet_chars: 4,
        })
        .unwrap();
        let plan = planner
            .plan("find the parser; explain the index; explain the storage")
            .unwrap();
        assert_eq!(plan.queries[0], plan.original_query);
        assert_eq!(plan.queries.len(), 3);
    }

    /// Verify raw newlines and sentence punctuation create facets without splitting filenames.
    /// 验证原始换行与句子标点会创建分面，同时不会拆分文件名。
    #[test]
    fn plans_documented_sentence_boundaries() {
        let planner = QueryPlanner::new(QueryPlannerConfig {
            max_queries: 4,
            min_facet_chars: 4,
        })
        .unwrap();
        let plan = planner
            .plan("Find parser details.\nExplain index flow? Describe src/lib.rs behavior.")
            .unwrap();
        assert_eq!(plan.queries.len(), 4);
        assert_eq!(plan.queries[1], "Find parser details");
        assert_eq!(plan.queries[3], "Describe src/lib.rs behavior");

        let single = planner.plan("Only one fragment.").unwrap();
        assert_eq!(single.queries, vec!["Only one fragment."]);
    }

    /// Verify list cleanup follows OCE marker boundaries without deleting leading numeric content.
    /// 验证列表清理遵循 OCE 标记边界，不会删除开头的数字正文。
    #[test]
    fn list_markers_require_documented_delimiters() {
        assert_eq!(trim_list_marker("-  first   facet"), "first facet");
        assert_eq!(trim_list_marker("2) second facet"), "second facet");
        assert_eq!(trim_list_marker("2024 architecture"), "2024 architecture");
        assert_eq!(trim_list_marker("-literal"), "-literal");
    }

    /// Verify rewrites cannot remove the original query or duplicate variants.
    /// 验证改写不能移除原始查询或产生重复变体。
    #[test]
    fn merges_rewrites_deterministically() {
        let planner = QueryPlanner::new(QueryPlannerConfig {
            max_queries: 3,
            min_facet_chars: 1,
        })
        .unwrap();
        let queries = planner
            .merge_rewrites(
                "original query",
                ["1. rewritten query".to_owned(), "original query".to_owned()],
            )
            .unwrap();
        assert_eq!(queries, vec!["original query", "rewritten query"]);
    }

    /// Verify explanatory and unbounded LLM output never becomes an Embedding query.
    /// 验证解释性和无界 LLM 输出绝不会成为向量查询。
    #[test]
    fn rejects_invalid_llm_rewrite_candidates() {
        let planner = QueryPlanner::new(QueryPlannerConfig {
            max_queries: 5,
            min_facet_chars: 1,
        })
        .unwrap();
        let queries = planner
            .merge_rewrites(
                "original query",
                [
                    "1. ok rewrite".to_owned(),
                    "no".to_owned(),
                    "改写策略: repeat the prompt".to_owned(),
                    "x".repeat(MAX_REWRITE_CHARS + 1),
                ],
            )
            .unwrap();
        assert_eq!(queries, vec!["original query", "ok rewrite"]);
    }

    /// Verify rewrite cleanup preserves years and deduplicates equivalent casing.
    /// 验证改写清洗会保留年份并去除仅大小写不同的重复项。
    #[test]
    fn rewrite_cleanup_preserves_numeric_queries_and_deduplicates_case() {
        let planner = QueryPlanner::new(QueryPlannerConfig::default()).unwrap();
        let queries = planner
            .merge_rewrites(
                "Find architecture",
                [
                    "2024 architecture".to_owned(),
                    "1. Trace Routing".to_owned(),
                    "trace routing".to_owned(),
                ],
            )
            .unwrap();

        assert_eq!(
            queries,
            vec!["Find architecture", "2024 architecture", "Trace Routing"]
        );
    }

    /// Verify the original query consumes the only slot in a one-query plan.
    /// 验证单查询计划的唯一槽位由原始查询占用。
    #[test]
    fn rewrite_merge_respects_single_query_budget() {
        let planner = QueryPlanner::new(QueryPlannerConfig {
            max_queries: 1,
            min_facet_chars: 1,
        })
        .unwrap();

        assert_eq!(
            planner
                .merge_rewrites("original", ["additional".to_owned()])
                .unwrap(),
            vec!["original"]
        );
    }
}
