//! Host-side tool-result rendering helpers with unified truncate and page behavior.
//! 宿主侧工具结果渲染辅助模块，统一处理截断与分页行为。

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::time::{SystemTime, UNIX_EPOCH};

mod render_budget;
mod template_cache;
mod templates;

/// A bounded preview, not an unbounded table describing an already oversized result.
/// 对已超限结果提供有界预览，而不是生成另一个无界分块表。
const MAX_INLINE_CHUNK_ROWS: usize = 64;

use templates::{DEFAULT_PAGE_TEMPLATE, load_template_text};

/// Stable host-owned overflow mode reused by native and Lua-backed tool rendering.
/// 供原生工具与 Lua 工具共同复用的稳定宿主超限模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostToolOverflowMode {
    /// Truncate the visible content to fit the current tool-result budget.
    /// 将可见内容截断到当前工具结果预算范围内。
    Truncate,
    /// Persist the full content into one spill file and expose safe page reads.
    /// 将完整内容落入超限文件并暴露安全分页读取能力。
    Page,
}

/// Unified host-side text render input shared by native and Lua-backed tool results.
/// 由原生工具与 Lua 工具结果共享的统一宿主文本渲染输入。
#[derive(Debug, Clone)]
pub struct HostTextRenderInput {
    /// Raw text content produced by the tool before host overflow handling.
    /// 工具在宿主超限处理前产出的原始文本内容。
    pub content: String,
    /// Optional overflow mode requested by the producing runtime.
    /// 产出运行时请求使用的可选超限模式。
    pub overflow_mode: Option<HostToolOverflowMode>,
    /// Optional explicit overflow-template hint.
    /// 可选的显式超限模板提示。
    pub template_hint: Option<String>,
    /// Stable byte count captured by the producing runtime.
    /// 由产出运行时记录的稳定字节数。
    pub content_bytes: usize,
    /// Stable line count captured by the producing runtime.
    /// 由产出运行时记录的稳定行数。
    pub content_lines: usize,
}

/// Unified English error returned when even one single line cannot fit the current host budget.
/// 当连单行内容都无法放入当前宿主预算时返回的统一英文错误提示。
const TOOL_OUTPUT_EXCEEDS_LIMIT_ERROR: &str =
    "Tool output exceeds the current VulcanCode client limit.";

/// Explicit error returned when page mode needs persistence but no host spill directory was selected.
/// 当分页模式需要持久化但宿主未选择超限目录时返回的显式错误。
const PAGE_SPILL_ROOT_REQUIRED_ERROR: &str =
    "Page-mode tool output requires a host-managed spill directory.";

/// Default trailing notice appended by the truncate template when content exceeds the host budget.
/// 当内容超出宿主预算时，由截断模板附加的默认尾部提示。
const DEFAULT_TRUNCATE_NOTICE: &str =
    "Content has been truncated because it exceeds the current VulcanCode client limit.";

/// Default inline byte budget for host-rendered tool results before page-mode spill takes over.
/// 宿主渲染工具结果在切换为分页落盘前使用的默认内联字节预算。
///
/// This is the single definition shared by shell tools and LuaSkills default rendering.
/// 这是 shell 工具与 LuaSkills 默认渲染共享的唯一定义。
pub const DEFAULT_INLINE_TOOL_RESULT_BYTES: u64 = 200_000;

/// Host-owned render options that decide where page-mode spill files and overflow templates are resolved.
/// 决定分页模式超限文件与超限模板解析位置的宿主自有渲染选项。
#[derive(Debug, Clone, Default)]
pub struct HostRenderOptions {
    /// Host-managed spill directory used only when page mode persists oversized output.
    /// 仅在分页模式落盘超长输出时使用的宿主管理超限目录。
    pub spill_root: Option<PathBuf>,
    /// Ordered skill roots used to resolve skill-local overflow templates for the current render call.
    /// 当前渲染调用用来解析技能本地超限模板的有序技能根目录集合。
    pub template_skill_roots: Vec<PathBuf>,
    /// Optional shared resources root used to resolve fallback overflow templates for the current render call.
    /// 当前渲染调用用来解析兜底超限模板的可选共享资源根目录。
    pub template_resources_root: Option<PathBuf>,
}

/// Host-side budget snapshot used when rendering one tool result for AI consumption.
/// 为 AI 消费渲染单次工具调用结果时使用的宿主侧预算快照。
#[derive(Debug, Clone)]
pub struct ToolResultBudgetSnapshot {
    /// Effective inline tool-result budget used for the immediate AI-visible return payload.
    /// 用于即时 AI 可见返回载荷的实际内联工具结果预算。
    pub tool_result: EffectiveBudgetScope,
    /// Effective file-read budget used when page mode needs to expose safe chunk ranges.
    /// 当分页模式需要暴露安全分块范围时使用的实际文件读取预算。
    pub file_read: EffectiveBudgetScope,
}

impl ToolResultBudgetSnapshot {
    /// Builds one snapshot whose inline and file-read scopes share the same byte limit without a line limit.
    /// 构建内联与文件读取范围共享同一字节上限且不限行数的预算快照。
    pub fn uniform_bytes(bytes: u64) -> Self {
        Self {
            tool_result: EffectiveBudgetScope { bytes, lines: -1 },
            file_read: EffectiveBudgetScope { bytes, lines: -1 },
        }
    }
}

/// One effective budget scope containing byte and line limits after host resolution.
/// 宿主解析后的单个实际预算范围，包含字节与行数限制。
#[derive(Debug, Clone)]
pub struct EffectiveBudgetScope {
    /// Maximum bytes allowed by the current scope.
    /// 当前范围允许的最大字节数。
    pub bytes: u64,
    /// Maximum lines allowed by the current scope, where `-1` means unlimited.
    /// 当前范围允许的最大行数，其中 `-1` 表示无限制。
    pub lines: i64,
}

/// Final overflow mode chosen by the host after reading the tool render input.
/// 宿主读取工具渲染输入后选出的最终超限模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OverflowMode {
    Truncate,
    Page,
}

/// Final overflow policy composed of the selected mode and template name.
/// 由选中模式与模板名组成的最终超限策略。
#[derive(Debug, Clone)]
struct OverflowPolicy {
    mode: OverflowMode,
    template_name: String,
}

/// Safe chunk plan emitted by page mode so the AI can read oversized content in bounded slices.
/// 由分页模式生成的安全分块计划，使 AI 能按有界切片读取超长内容。
#[derive(Debug, Clone)]
struct OverflowChunkPlan {
    chunks: Vec<OverflowChunk>,
    chunk_count: usize,
    total_lines: usize,
    total_bytes: usize,
    safe_limit_bytes: u64,
    safe_limit_lines: i64,
}

/// Offset and line-span metadata for one safe read chunk emitted by page mode.
/// 分页模式发出的单个安全读取块的偏移与行区间元数据。
#[derive(Debug, Clone)]
struct OverflowChunk {
    offset: usize,
    limit: usize,
    start_line: usize,
    end_line: usize,
    byte_count: usize,
}

/// Renders one tool text payload into final AI-visible text according to the host policy.
/// 按宿主策略把单次工具文本载荷渲染成最终面向 AI 的可见文本。
///
/// # Parameters
/// # 参数
/// - `output`: Stable text and overflow metadata produced by the tool runtime.
/// - `output`：工具运行时生成的稳定文本与超限元数据。
/// - `skill_name`: Optional LuaSkills identity used for template resolution.
/// - `skill_name`：用于解析模板的可选 LuaSkills 标识。
/// - `client_budget`: Effective inline and page-read budgets.
/// - `client_budget`：生效的内联与分页读取预算。
/// - `render_options`: Host-owned spill and template locations.
/// - `render_options`：宿主拥有的超限文件与模板位置。
///
/// # Returns
/// # 返回值
/// Returns rendered text, or the exact budget, template-read, spill-path or write failure.
/// 成功时返回渲染文本；失败时返回精确的预算、模板读取、超限路径或写入错误。
pub fn render_tool_result_text(
    output: &HostTextRenderInput,
    skill_name: Option<&str>,
    client_budget: &ToolResultBudgetSnapshot,
    render_options: &HostRenderOptions,
) -> Result<String, String> {
    let policy = resolve_overflow_policy(skill_name, output);
    match policy.mode {
        OverflowMode::Truncate => {
            render_truncate_text(output, skill_name, &policy, client_budget, render_options)
        }
        OverflowMode::Page => {
            render_page_text(output, skill_name, &policy, client_budget, render_options)
        }
    }
}

/// Resolve the final host overflow policy from the render input and tool metadata.
/// 根据渲染输入与工具元数据解析最终宿主超限策略。
fn resolve_overflow_policy(
    _skill_name: Option<&str>,
    output: &HostTextRenderInput,
) -> OverflowPolicy {
    let mode = match output.overflow_mode {
        Some(HostToolOverflowMode::Page) => OverflowMode::Page,
        Some(HostToolOverflowMode::Truncate) | None => OverflowMode::Truncate,
    };

    OverflowPolicy {
        mode,
        template_name: resolve_template_name(output.template_hint.as_deref(), mode),
    }
}

/// Resolve the final template name, preferring the explicit runtime hint before falling back to the shared default.
/// 解析最终模板名，优先使用运行时显式提示，其次回退到共享默认模板。
fn resolve_template_name(explicit_template_name: Option<&str>, mode: OverflowMode) -> String {
    explicit_template_name
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(match mode {
            OverflowMode::Truncate => "overflow_truncate.md",
            OverflowMode::Page => "overflow_page.md",
        })
        .to_string()
}

/// Renders truncate-mode text and rejects content that cannot fit even one complete line.
/// 渲染截断模式文本，并拒绝连一整行都无法容纳的内容。
///
/// # Parameters
/// # 参数
/// - `output`: Source text and stable size metadata.
/// - `output`：源文本与稳定大小元数据。
/// - `skill_name`: Optional skill identity used for template lookup.
/// - `skill_name`：用于查找模板的可选技能标识。
/// - `policy`: Resolved truncate policy and template name.
/// - `policy`：已解析的截断策略与模板名称。
/// - `client_budget`: Effective inline and read budgets.
/// - `client_budget`：生效的内联与读取预算。
/// - `render_options`: Host template lookup locations.
/// - `render_options`：宿主模板查找位置。
///
/// # Returns
/// # 返回值
/// Returns normalized or truncated text, or the exact host-budget or template-read error.
/// 返回规范化或截断后的文本，或精确的宿主预算或模板读取错误。
fn render_truncate_text(
    output: &HostTextRenderInput,
    skill_name: Option<&str>,
    policy: &OverflowPolicy,
    client_budget: &ToolResultBudgetSnapshot,
    render_options: &HostRenderOptions,
) -> Result<String, String> {
    let tool_result_budget = &client_budget.tool_result;
    if content_fits_budget(output, tool_result_budget) {
        return Ok(normalize_text(&output.content).into_owned());
    }

    let template = load_template_text(skill_name, policy.template_name.as_str(), render_options)?
        .unwrap_or_else(|| "{{truncated_content}}\n...\n# {{truncate_notice}}".into());
    let mut context = HashMap::from([("truncate_notice", DEFAULT_TRUNCATE_NOTICE.to_string())]);
    let normalized = normalize_text(&output.content);
    let prefix = render_budget::truncate(&normalized, &template, &context, tool_result_budget)?;
    context.insert("truncated_content", prefix);
    render_budget::render(&template, &context, tool_result_budget)
}

/// Renders page-mode text and persists oversized content into the selected host spill directory.
/// 渲染分页模式文本，并将超长内容持久化到选定的宿主超限目录。
///
/// # Parameters
/// # 参数
/// - `output`: Source text and stable size metadata.
/// - `output`：源文本与稳定大小元数据。
/// - `skill_name`: Optional skill identity used for template lookup.
/// - `skill_name`：用于查找模板的可选技能标识。
/// - `policy`: Resolved page policy and spill-file template name.
/// - `policy`：已解析的分页策略与超限文件模板名称。
/// - `client_budget`: Effective inline and safe-read budgets.
/// - `client_budget`：生效的内联与安全读取预算。
/// - `render_options`: Host spill and template lookup locations.
/// - `render_options`：宿主超限文件与模板查找位置。
///
/// # Returns
/// # 返回值
/// Returns inline text or a page pointer, or the exact chunk-planning, template-read or persistence failure.
/// 返回内联文本或分页指针，或精确的分块规划、模板读取或持久化失败。
fn render_page_text(
    output: &HostTextRenderInput,
    skill_name: Option<&str>,
    policy: &OverflowPolicy,
    client_budget: &ToolResultBudgetSnapshot,
    render_options: &HostRenderOptions,
) -> Result<String, String> {
    let tool_result_budget = &client_budget.tool_result;
    if content_fits_budget(output, tool_result_budget) {
        return Ok(normalize_text(&output.content).into_owned());
    }

    let file_read_budget = &client_budget.file_read;
    let chunk_plan = build_chunk_plan(&output.content, file_read_budget)?;

    let spill_root = render_options
        .spill_root
        .as_deref()
        .ok_or_else(|| PAGE_SPILL_ROOT_REQUIRED_ERROR.to_string())?;

    let pending_file = stage_overflow_text_file(&output.content, spill_root)?;
    let raw_file = pending_file.path();

    let mut context = HashMap::new();
    context.insert(
        "raw_file",
        vulcan_runtime_assets::native_host_path_text_from_path(raw_file),
    );
    context.insert("format", "markdown".to_string());
    context.insert("total_bytes", chunk_plan.total_bytes.to_string());
    context.insert("total_lines", chunk_plan.total_lines.to_string());
    context.insert(
        "safe_inline_limit_bytes",
        tool_result_budget.bytes.to_string(),
    );
    context.insert(
        "safe_inline_limit_lines",
        tool_result_budget.lines.to_string(),
    );
    context.insert(
        "safe_read_limit_bytes",
        chunk_plan.safe_limit_bytes.to_string(),
    );
    context.insert(
        "safe_read_limit_lines",
        chunk_plan.safe_limit_lines.to_string(),
    );
    context.insert("chunk_count", chunk_plan.chunk_count.to_string());
    context.insert(
        "read_strategy_section",
        "## Read Strategy\n- Read the raw_file directly.\n- Read by line ranges using the chunk table below.\n- Use the chunk table offset as the 0-based line offset and limit as the line count.".to_string(),
    );
    context.insert("optional_summary_section", String::new());
    context.insert("scope_advice_section", String::new());

    let template = load_template_text(skill_name, policy.template_name.as_str(), render_options)?
        .unwrap_or_else(|| DEFAULT_PAGE_TEMPLATE.into());
    let mut rows = MAX_INLINE_CHUNK_ROWS.min(chunk_plan.chunks.len());
    let rendered = loop {
        context.insert(
            "host_safe_read_chunks_section",
            render_chunk_lines(&chunk_plan, rows),
        );
        match render_budget::render(&template, &context, tool_result_budget) {
            Ok(rendered) => break rendered,
            Err(_) if rows > 0 => rows /= 2,
            Err(error) => return Err(error),
        }
    };
    // A failed render drops its private staging file instead of leaving an unreachable result.
    // 渲染失败会删除私有暂存文件，而不会留下无法访问的结果。
    pending_file
        .keep()
        .map_err(|error| format!("Failed to retain the overflow file: {error}."))?;
    Ok(rendered)
}

/// Decide whether the original content already fits inside the current budget and can bypass host overflow handling.
/// 判断原始内容是否已经满足当前预算，从而绕过宿主超限处理。
fn content_fits_budget(output: &HostTextRenderInput, budget: &EffectiveBudgetScope) -> bool {
    render_budget::fits(&normalize_text(&output.content), budget)
}

/// Truncate content on real line boundaries and return `None` when even the first line cannot fit.
/// 按真实行边界截断内容；若连第一行都无法放入预算则返回 `None`。
#[cfg(test)]
fn truncate_content_at_line_boundary(
    content: &str,
    budget: &EffectiveBudgetScope,
) -> Option<String> {
    render_budget::truncate(
        &normalize_text(content),
        "{{truncated_content}}",
        &HashMap::new(),
        budget,
    )
    .ok()
}

/// Build a safe page-mode chunk plan and reject any source line that already exceeds the file-read budget.
/// 构建安全的分页模式分块计划，并拒绝任何已经超出文件读取预算的原始行。
fn build_chunk_plan(
    content: &str,
    file_read_budget: &EffectiveBudgetScope,
) -> Result<OverflowChunkPlan, String> {
    let total_lines = if content.is_empty() {
        0
    } else {
        content.bytes().filter(|byte| *byte == b'\n').count() + 1
    };
    let total_bytes = content.len();
    let safe_limit_bytes = file_read_budget.bytes;
    let safe_limit_lines = file_read_budget.lines;

    let mut chunks = Vec::new();
    let mut chunk_count = 0usize;
    let mut current_start: Option<usize> = None;
    let mut current_bytes = 0usize;
    let mut current_line_count = 0usize;

    let mut flush_chunk = |chunks: &mut Vec<OverflowChunk>,
                           current_start: &mut Option<usize>,
                           current_bytes: &mut usize,
                           current_line_count: &mut usize,
                           end_line: usize| {
        if let Some(start_line) = *current_start
            && *current_line_count > 0
        {
            chunk_count += 1;
            if chunks.len() <= MAX_INLINE_CHUNK_ROWS {
                chunks.push(OverflowChunk {
                    offset: start_line - 1,
                    limit: *current_line_count,
                    start_line,
                    end_line,
                    byte_count: *current_bytes,
                });
            }
        }
        *current_start = None;
        *current_bytes = 0;
        *current_line_count = 0;
    };

    for (index, line) in content.split('\n').take(total_lines).enumerate() {
        let mut line_bytes = line.len();
        if index + 1 < total_lines {
            line_bytes += 1;
        }
        if line_bytes > safe_limit_bytes as usize {
            return Err(TOOL_OUTPUT_EXCEEDS_LIMIT_ERROR.to_string());
        }

        let lines_limit_hit =
            safe_limit_lines > 0 && current_line_count >= safe_limit_lines as usize;
        let bytes_limit_hit = current_start.is_some()
            && current_bytes.saturating_add(line_bytes) > safe_limit_bytes as usize;
        if bytes_limit_hit || lines_limit_hit {
            flush_chunk(
                &mut chunks,
                &mut current_start,
                &mut current_bytes,
                &mut current_line_count,
                index,
            );
        }

        if current_start.is_none() {
            current_start = Some(index + 1);
        }
        current_bytes += line_bytes;
        current_line_count += 1;
    }

    flush_chunk(
        &mut chunks,
        &mut current_start,
        &mut current_bytes,
        &mut current_line_count,
        total_lines,
    );

    Ok(OverflowChunkPlan {
        chunk_count,
        chunks,
        total_lines,
        total_bytes,
        safe_limit_bytes,
        safe_limit_lines,
    })
}

/// Render the page-mode chunk plan into template-ready Markdown lines.
/// 将分页模式分块计划渲染为模板可直接使用的 Markdown 行。
fn render_chunk_lines(chunk_plan: &OverflowChunkPlan, maximum: usize) -> String {
    let mut rows = chunk_plan
        .chunks
        .iter()
        .take(maximum)
        .enumerate()
        .map(|(index, chunk)| {
            format!(
                "- read_{:02}: offset={}, limit={}, start_line={}, end_line={}, bytes={}",
                index + 1,
                chunk.offset,
                chunk.limit,
                chunk.start_line,
                chunk.end_line,
                chunk.byte_count
            )
        })
        .collect::<Vec<_>>();
    if maximum < chunk_plan.chunk_count {
        let next = &chunk_plan.chunks[maximum];
        rows.push(format!("- Further ranges omitted. Continue from line offset {} in raw_file; obey safe_read_limit_bytes and safe_read_limit_lines.", next.offset));
    }
    rows.join("\n")
}

/// Persist oversized content into the host-managed spill directory for later page-mode reading.
/// 将超长内容持久化到宿主管理的超限目录，供后续分页模式读取。
fn stage_overflow_text_file(
    content: &str,
    spill_root: &Path,
) -> Result<tempfile::NamedTempFile, String> {
    use std::io::Write;

    fs::create_dir_all(spill_root)
        .map_err(|error| format!("Failed to create the overflow directory: {error}."))?;
    // Exclusive allocation, not clock precision, prevents concurrent results from aliasing.
    // 依靠独占分配而非时钟精度，避免并发结果指向同一文件。
    let mut file = tempfile::Builder::new()
        .prefix("tool_output_")
        .suffix(".md")
        .tempfile_in(spill_root)
        .map_err(|error| format!("Failed to allocate the overflow file: {error}."))?;
    file.write_all(content.as_bytes())
        .and_then(|()| file.flush())
        .map_err(|error| format!("Failed to write the overflow file: {error}."))?;
    // Failed writes are removed by the temporary-file guard; publish only a complete file.
    // 写入失败由临时文件守卫清理；只发布写入完整的文件。
    Ok(file)
}

#[cfg(test)]
fn write_overflow_text_file(content: &str, spill_root: &Path) -> Result<PathBuf, String> {
    stage_overflow_text_file(content, spill_root)?
        .keep()
        .map(|(_, path)| path)
        .map_err(|error| format!("Failed to retain the overflow file: {error}."))
}

/// Normalize line endings so CRLF does not affect host-side line counting or chunking.
/// 统一换行风格，避免 CRLF 影响宿主侧按行统计与分块。
fn normalize_text(content: &str) -> std::borrow::Cow<'_, str> {
    if content.contains("\r\n") {
        content.replace("\r\n", "\n").into()
    } else {
        content.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Linear truncation preserves byte, line, Unicode and CRLF boundary behavior.
    /// 线性截断保留字节、行数、Unicode 及 CRLF 边界行为。
    #[test]
    fn truncation_preserves_exact_boundary_semantics() {
        for source in ["", "a", "a\n", "\n\n", "a\r\nb", "中文\ntext", "a\rb"] {
            let normalized = source.replace("\r\n", "\n");
            for bytes in 0..32 {
                for lines in [-1, 0, 1, 2, 3] {
                    let budget = EffectiveBudgetScope { bytes, lines };
                    let expected = if normalized.is_empty() {
                        Some(String::new())
                    } else {
                        let mut accepted = Vec::new();
                        for line in normalized.split('\n') {
                            if lines > 0 && accepted.len() >= lines as usize {
                                break;
                            }
                            let mut candidate = accepted.clone();
                            candidate.push(line);
                            if candidate.join("\n").len() > bytes as usize {
                                break;
                            }
                            accepted.push(line);
                        }
                        (!accepted.is_empty()).then(|| accepted.join("\n"))
                    };
                    assert_eq!(
                        truncate_content_at_line_boundary(source, &budget),
                        expected,
                        "source={source:?}; bytes={bytes}; lines={lines}"
                    );
                }
            }
        }
    }

    /// Many short lines require only one accepted-prefix copy, not a growing join per line.
    /// 大量短行只需复制一次已接受前缀，不逐行拼接增长中的正文。
    #[test]
    fn truncation_handles_many_short_lines() {
        let content = "x\n".repeat(100_000);
        let budget = EffectiveBudgetScope {
            bytes: 100_001,
            lines: -1,
        };
        let rendered =
            truncate_content_at_line_boundary(&content, &budget).expect("accepted prefix");
        assert_eq!(rendered.len(), 100_001);
        assert!(content.starts_with(&rendered));
    }

    /// Concurrent renders retain distinct, immutable output files independent of wall-clock time.
    /// 并发渲染保留互不相同且内容不变的输出文件，不依赖墙上时钟。
    #[test]
    fn overflow_files_are_exclusively_allocated() {
        let directory = tempfile::tempdir().expect("create isolated spill directory");
        let outputs = std::thread::scope(|scope| {
            let workers = (0..4)
                .map(|worker| {
                    let root = directory.path();
                    scope.spawn(move || {
                        (0..32)
                            .map(|sequence| {
                                let content = format!("worker={worker}; result={sequence}");
                                let path = write_overflow_text_file(&content, root)
                                    .expect("allocate one immutable spill file");
                                (path, content)
                            })
                            .collect::<Vec<_>>()
                    })
                })
                .collect::<Vec<_>>();
            workers
                .into_iter()
                .flat_map(|worker| worker.join().expect("spill worker completes"))
                .collect::<Vec<_>>()
        });
        let paths = outputs
            .iter()
            .map(|(path, _)| path)
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            paths.len(),
            outputs.len(),
            "spill file identities must be unique"
        );
        for (path, expected) in outputs {
            assert_eq!(
                fs::read_to_string(path).expect("read retained output"),
                expected
            );
        }
    }

    /// Verifies page overflow without a host spill path returns a protocol-level error.
    /// 验证分页超限缺少宿主超限路径时返回协议级错误。
    ///
    /// # Parameters
    /// # 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// # 返回值
    /// This test returns no value and fails if a missing spill path becomes normal model text.
    /// 此测试无返回值；若缺失超限路径退化为普通模型文本则失败。
    #[test]
    fn page_render_requires_host_spill_root() {
        // output forces page mode while keeping every source line valid for the read budget.
        // output 强制进入分页模式，同时保证每一行均满足读取预算。
        let output = HostTextRenderInput {
            content: String::from("alpha\nbeta"),
            overflow_mode: Some(HostToolOverflowMode::Page),
            template_hint: None,
            content_bytes: 10,
            content_lines: 2,
        };
        // budget rejects inline output but permits one source line per safe page chunk.
        // budget 拒绝内联输出，但允许每个安全分页块容纳一行源码。
        let budget = ToolResultBudgetSnapshot {
            tool_result: EffectiveBudgetScope {
                bytes: 4,
                lines: -1,
            },
            file_read: EffectiveBudgetScope {
                bytes: 6,
                lines: -1,
            },
        };

        let error = render_tool_result_text(&output, None, &budget, &HostRenderOptions::default())
            .expect_err("page overflow should require a spill root");

        assert_eq!(error, PAGE_SPILL_ROOT_REQUIRED_ERROR);
    }

    /// Verifies a non-directory spill root preserves the concrete directory-creation failure.
    /// 验证非目录超限路径保留具体的目录创建失败。
    ///
    /// # Parameters
    /// # 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// # 返回值
    /// This test returns no value and fails if persistence errors collapse into a size message.
    /// 此测试无返回值；若持久化错误退化为大小提示则失败。
    #[test]
    fn page_render_preserves_spill_directory_creation_error() {
        // spill_root_file is an existing file and therefore cannot become the required directory.
        // spill_root_file 是一个既有文件，因此无法成为所需目录。
        let spill_root_file = std::env::temp_dir().join(format!(
            "vulcan-render-spill-file-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("test clock should follow the Unix epoch")
                .as_nanos()
        ));
        fs::write(&spill_root_file, b"not a directory")
            .expect("spill-root fixture file should be created");
        let output = HostTextRenderInput {
            content: String::from("alpha\nbeta"),
            overflow_mode: Some(HostToolOverflowMode::Page),
            template_hint: None,
            content_bytes: 10,
            content_lines: 2,
        };
        let budget = ToolResultBudgetSnapshot {
            tool_result: EffectiveBudgetScope {
                bytes: 4,
                lines: -1,
            },
            file_read: EffectiveBudgetScope {
                bytes: 6,
                lines: -1,
            },
        };

        let error = render_tool_result_text(
            &output,
            None,
            &budget,
            &HostRenderOptions {
                spill_root: Some(spill_root_file.clone()),
                ..HostRenderOptions::default()
            },
        )
        .expect_err("a file cannot serve as the spill directory");
        fs::remove_file(&spill_root_file).expect("spill-root fixture file should be removed");

        assert!(error.starts_with("Failed to create the overflow directory:"));
        assert!(!error.contains(TOOL_OUTPUT_EXCEEDS_LIMIT_ERROR));
    }

    /// Verifies successful page rendering creates the selected directory and exact overflow file.
    /// 验证成功分页渲染会创建选定目录和内容精确的超限文件。
    ///
    /// # Parameters
    /// # 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// # 返回值
    /// This test returns no value and fails if lazy persistence or pointer rendering regresses.
    /// 此测试无返回值；若延迟持久化或指针渲染退化则失败。
    #[test]
    fn page_render_lazily_persists_exact_overflow_content() {
        // spill_root starts absent so the page writer owns directory creation.
        // spill_root 初始不存在，因此目录创建由分页写入器负责。
        let spill_root = std::env::temp_dir().join(format!(
            "vulcan-render-spill-dir-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("test clock should follow the Unix epoch")
                .as_nanos()
        ));
        let output = HostTextRenderInput {
            content: "alpha\nbeta\n".repeat(1000),
            overflow_mode: Some(HostToolOverflowMode::Page),
            template_hint: None,
            content_bytes: 11_000,
            content_lines: 2_001,
        };
        let budget = ToolResultBudgetSnapshot {
            tool_result: EffectiveBudgetScope {
                bytes: 1_500,
                lines: -1,
            },
            file_read: EffectiveBudgetScope {
                bytes: 6,
                lines: -1,
            },
        };

        let rendered = render_tool_result_text(
            &output,
            None,
            &budget,
            &HostRenderOptions {
                spill_root: Some(spill_root.clone()),
                ..HostRenderOptions::default()
            },
        )
        .expect("page overflow should persist successfully");
        // overflow_files contains the one file created by this isolated render call.
        // overflow_files 包含本次隔离渲染调用创建的唯一文件。
        let overflow_files = fs::read_dir(&spill_root)
            .expect("spill directory should exist")
            .collect::<Result<Vec<_>, _>>()
            .expect("spill directory entries should be readable");

        assert!(rendered.contains("# LARGE RESULT POINTER"));
        assert_eq!(overflow_files.len(), 1);
        assert_eq!(
            fs::read_to_string(overflow_files[0].path()).expect("overflow file should be readable"),
            output.content
        );
        fs::remove_dir_all(&spill_root).expect("spill directory should be removed");
    }

    #[test]
    fn inline_budget_uses_actual_text_even_when_size_metadata_is_stale() {
        let output = HostTextRenderInput {
            content: "a\nb\nc".into(),
            content_bytes: 0,
            content_lines: 0,
            overflow_mode: Some(HostToolOverflowMode::Page),
            template_hint: None,
        };
        let result = render_tool_result_text(
            &output,
            None,
            &ToolResultBudgetSnapshot::uniform_bytes(2),
            &HostRenderOptions::default(),
        );
        assert_eq!(
            result.expect_err("actual content must trigger overflow"),
            PAGE_SPILL_ROOT_REQUIRED_ERROR
        );
    }

    #[test]
    fn final_truncated_result_includes_notice_bytes_and_lines() {
        let output = HostTextRenderInput {
            content: "abc\n".repeat(100),
            content_bytes: 400,
            content_lines: 101,
            overflow_mode: Some(HostToolOverflowMode::Truncate),
            template_hint: None,
        };
        let root = tempfile::tempdir().expect("template root");
        for bytes in [100, 120, 150] {
            let budget = ToolResultBudgetSnapshot {
                tool_result: EffectiveBudgetScope { bytes, lines: 5 },
                file_read: EffectiveBudgetScope { bytes, lines: -1 },
            };
            let rendered = render_tool_result_text(
                &output,
                None,
                &budget,
                &HostRenderOptions {
                    template_resources_root: Some(root.path().to_owned()),
                    ..HostRenderOptions::default()
                },
            )
            .expect("body plus footer fits");
            assert!(render_budget::fits(&rendered, &budget.tool_result));
            assert!(rendered.contains(DEFAULT_TRUNCATE_NOTICE));
        }
    }

    #[test]
    fn oversized_page_wrapper_does_not_publish_or_leak_a_spill_file() {
        let root = tempfile::tempdir().expect("spill root");
        let output = HostTextRenderInput {
            content: "a\n".repeat(100),
            content_bytes: 200,
            content_lines: 101,
            overflow_mode: Some(HostToolOverflowMode::Page),
            template_hint: None,
        };
        let error = render_tool_result_text(
            &output,
            None,
            &ToolResultBudgetSnapshot::uniform_bytes(8),
            &HostRenderOptions {
                spill_root: Some(root.path().to_owned()),
                template_resources_root: Some(root.path().to_owned()),
                ..HostRenderOptions::default()
            },
        )
        .expect_err("there is not enough space for a valid pointer");
        assert_eq!(error, TOOL_OUTPUT_EXCEEDS_LIMIT_ERROR);
        assert_eq!(
            fs::read_dir(root.path()).expect("read spill root").count(),
            0
        );
    }

    #[test]
    fn page_read_budget_counts_persisted_crlf_and_zero_limits() {
        let source = "a\r\nb\r\n";
        let plan = build_chunk_plan(source, &EffectiveBudgetScope { bytes: 3, lines: 1 })
            .expect("one CRLF row per chunk");
        assert_eq!(plan.total_bytes, source.len());
        assert_eq!(
            plan.chunks
                .iter()
                .map(|chunk| chunk.byte_count)
                .sum::<usize>(),
            source.len()
        );
        assert!(plan.chunks.iter().all(|chunk| chunk.byte_count <= 3));
        assert!(build_chunk_plan(source, &EffectiveBudgetScope { bytes: 2, lines: 1 }).is_err());
        assert!(
            build_chunk_plan(
                "a",
                &EffectiveBudgetScope {
                    bytes: 0,
                    lines: -1
                }
            )
            .is_err()
        );
    }

    #[test]
    fn chunk_preview_is_bounded_even_when_every_line_is_a_chunk() {
        let content = "x\n".repeat(100_000);
        let plan = build_chunk_plan(&content, &EffectiveBudgetScope { bytes: 2, lines: 1 })
            .expect("bounded preview");
        assert_eq!(plan.chunk_count, 100_001);
        assert_eq!(plan.chunks.len(), MAX_INLINE_CHUNK_ROWS + 1);
        let preview = render_chunk_lines(&plan, MAX_INLINE_CHUNK_ROWS);
        assert!(preview.contains("Further ranges omitted"));
        assert_eq!(preview.lines().count(), MAX_INLINE_CHUNK_ROWS + 1);
    }
}
