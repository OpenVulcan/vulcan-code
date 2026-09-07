//! Source-aligned machine-text folding for native PostAction admission.
//! 对齐外置来源的原生 PostAction 准入机器文本折叠。

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

/// Configures one contiguous block's eligibility and retained excerpts.
/// 配置连续文本块的折叠条件及保留摘录。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MachineBlockRules {
    /// Minimum total lines eligible for folding.
    /// 允许折叠的最少总行数。
    pub min_lines: usize,
    /// Minimum Unicode scalar count eligible for folding.
    /// 允许折叠的最少 Unicode 字符数。
    pub min_runes: usize,
    /// Maximum Han ratio before unstructured prose is protected.
    /// 非结构化自然语言保护前允许的最大汉字比例。
    pub max_han_ratio: f64,
    /// Number of leading lines to preserve.
    /// 保留的开头行数。
    pub head_lines: usize,
    /// Number of trailing lines to preserve.
    /// 保留的结尾行数。
    pub tail_lines: usize,
    /// Minimum omitted non-anchor lines required to justify folding.
    /// 值得折叠所需省略的最少非锚点行数。
    pub min_omitted_lines: usize,
}

/// Configures the external-compatible plain, fenced and horizontal compression rules.
/// 配置与外置兼容的普通文本、围栏文本及水平压缩规则。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MachineTextCleanerConfig {
    /// Protects fragments shorter than this Unicode scalar count.
    /// 保护 Unicode 字符数低于此值的片段。
    pub short_text_immunity_runes: usize,
    /// Marks the minimum width considered for horizontal truncation.
    /// 标识考虑水平截断的最小宽度。
    pub max_line_runes: usize,
    /// Preserves this many characters at a clipped line's start.
    /// 保留截断行开头的字符数。
    pub long_line_head_runes: usize,
    /// Preserves this many characters at a clipped line's end.
    /// 保留截断行结尾的字符数。
    pub long_line_tail_runes: usize,
    /// Rules used inside paired backtick fences.
    /// 成对反引号围栏内部使用的规则。
    pub fenced: MachineBlockRules,
    /// Rules used outside paired backtick fences.
    /// 成对反引号围栏外部使用的规则。
    pub plain: MachineBlockRules,
}

impl Default for MachineTextCleanerConfig {
    /// Returns the exact external PostAction cleaner thresholds.
    /// 返回外置 PostAction 清洗器的准确阈值。
    fn default() -> Self {
        Self {
            short_text_immunity_runes: 100,
            max_line_runes: 1000,
            long_line_head_runes: 240,
            long_line_tail_runes: 120,
            fenced: MachineBlockRules {
                min_lines: 10,
                min_runes: 300,
                max_han_ratio: 0.15,
                head_lines: 3,
                tail_lines: 2,
                min_omitted_lines: 4,
            },
            plain: MachineBlockRules {
                min_lines: 12,
                min_runes: 400,
                max_han_ratio: 0.15,
                head_lines: 3,
                tail_lines: 2,
                min_omitted_lines: 4,
            },
        }
    }
}

/// Caches Unicode script/category predicates used by the source classifier.
/// 缓存来源分类器使用的 Unicode 脚本及类别谓词。
static HAN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\p{Han}$").expect("static Han regex"));
/// Matches Unicode punctuation and symbols, not arbitrary non-ASCII characters.
/// 匹配 Unicode 标点及符号，而非任意非 ASCII 字符。
static SYMBOL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[\p{P}\p{S}]$").expect("static symbol regex"));
/// Matches Unicode letters using the same category as the source word counter.
/// 使用与来源词数统计相同的 Unicode 字母类别。
static LETTER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\p{L}$").expect("static letter regex"));

/// Classifies a line as blank, prose, machine-like or neutral.
/// 将一行分类为空白、自然语言、机器文本或中性文本。
#[derive(Clone, Copy, PartialEq)]
enum LineKind {
    Blank,
    Natural,
    Machine,
    Neutral,
}

/// Retains only line facts consumed by block classification and rendering.
/// 仅保留文本块分类及渲染实际使用的行事实。
struct Line<'a> {
    /// Original line including its newline.
    /// 包含换行的原始行。
    raw: &'a str,
    /// Classified line kind.
    /// 已分类的行类型。
    kind: LineKind,
    /// Unicode scalar count after trimming.
    /// 裁剪后的 Unicode 字符数。
    runes: usize,
    /// Han script character count.
    /// 汉字脚本字符数。
    han: usize,
    /// Punctuation and symbol count.
    /// 标点及符号数。
    symbols: usize,
    /// Stack, log, JSON and code signal flags in that order.
    /// 按顺序表示堆栈、日志、JSON 及代码信号。
    signals: [bool; 4],
    /// Protects root-cause lines from omission.
    /// 保护根因行免于省略。
    anchor: bool,
    /// Requests horizontal clipping when block folding is not selected.
    /// 未选中块折叠时请求水平截断。
    horizontal_clip: bool,
}

/// Aggregates the exact counters used by the source block predicates.
/// 汇总来源文本块谓词实际使用的准确计数。
#[derive(Default)]
struct BlockStats {
    /// Total lines, including blanks.
    /// 包含空白的总行数。
    lines: usize,
    /// Nonblank lines.
    /// 非空行数。
    nonblank: usize,
    /// Total trimmed Unicode scalars.
    /// 裁剪后的总 Unicode 字符数。
    runes: usize,
    /// Total Han characters.
    /// 总汉字数。
    han: usize,
    /// Total punctuation and symbols.
    /// 总标点及符号数。
    symbols: usize,
    /// Natural-language lines.
    /// 自然语言行数。
    natural: usize,
    /// Machine-like lines.
    /// 机器文本行数。
    machine: usize,
    /// Stack, log, JSON and code signal counts.
    /// 堆栈、日志、JSON 及代码信号计数。
    signals: [usize; 4],
}

/// Applies validated source-compatible machine folding while retaining prose and error anchors.
/// 应用经过校验且与来源兼容的机器文本折叠，同时保留自然语言及错误锚点。
/// Returns a configuration error for invalid ratios or zero clipping width rather than guessing defaults.
/// 对无效比例或零截断宽度返回配置错误，不猜测默认值。
pub fn clean_machine_text(raw: &str, cfg: &MachineTextCleanerConfig) -> Result<String, String> {
    if cfg.max_line_runes == 0
        || [&cfg.plain, &cfg.fenced].iter().any(|rules| {
            !rules.max_han_ratio.is_finite() || !(0.0..=1.0).contains(&rules.max_han_ratio)
        })
    {
        return Err(String::from(
            "machine cleaner requires a positive line width and finite Han ratios in [0, 1]",
        ));
    }
    if raw.chars().count() < cfg.short_text_immunity_runes {
        return Ok(raw.to_owned());
    }
    // lines preserves original line endings and does not append an artificial final empty line.
    // lines 保留原始换行，且不追加人为的尾部空行。
    let lines = raw.split_inclusive('\n').collect::<Vec<_>>();
    // output accumulates separately processed plain segments and paired fences.
    // output 累计分别处理的普通片段及成对围栏。
    let mut output = String::with_capacity(raw.len());
    // plain_start and index delimit the not-yet-emitted plain segment.
    // plain_start 与 index 界定尚未输出的普通片段。
    let (mut plain_start, mut index) = (0, 0);
    while index < lines.len() {
        if !lines[index].trim().starts_with("```") {
            index += 1;
            continue;
        }
        // close is the next fence line, matching the source's deliberately simple pairing rule.
        // close 是下一条围栏行，与来源有意采用的简单配对规则一致。
        let close = ((index + 1)..lines.len()).find(|next| lines[*next].trim().starts_with("```"));
        let Some(close) = close else {
            index += 1;
            continue;
        };
        output.push_str(&process_lines(&lines[plain_start..index], cfg, &cfg.plain));
        output.push_str(lines[index]);
        // content classifies the fenced body once before applying the short-block protection.
        // content 在应用短块保护前仅分类一次围栏正文。
        let content = lines[index + 1..close]
            .iter()
            .map(|line| classify_line(line, cfg))
            .collect::<Vec<_>>();
        // stats establishes the exact fenced immunity condition before processing machine runs.
        // stats 在处理机器文本段前建立准确的围栏免疫条件。
        let stats = analyze(&content);
        if !content.iter().any(|line| line.horizontal_clip)
            && stats.lines < cfg.fenced.min_lines
            && stats.runes < cfg.fenced.min_runes
        {
            output.extend(lines[index + 1..close].iter().copied());
        } else {
            output.push_str(&process_infos(&content, cfg, &cfg.fenced));
        }
        output.push_str(lines[close]);
        index = close + 1;
        plain_start = index;
    }
    output.push_str(&process_lines(&lines[plain_start..], cfg, &cfg.plain));
    Ok(output)
}

/// Classifies a plain segment and returns its folded representation.
/// 分类一个普通片段并返回其折叠表示。
fn process_lines(
    lines: &[&str],
    cfg: &MachineTextCleanerConfig,
    rules: &MachineBlockRules,
) -> String {
    process_infos(
        &lines
            .iter()
            .map(|line| classify_line(line, cfg))
            .collect::<Vec<_>>(),
        cfg,
        rules,
    )
}

/// Folds runs starting at machine lines and ending before natural prose, preserving trailing blanks.
/// 折叠从机器行开始、在自然语言前结束的文本段，并保留尾部空行。
fn process_infos(
    lines: &[Line<'_>],
    cfg: &MachineTextCleanerConfig,
    rules: &MachineBlockRules,
) -> String {
    // output and index track emitted content and the next unprocessed line.
    // output 与 index 跟踪已输出内容及下一条待处理行。
    let (mut output, mut index) = (String::new(), 0);
    while index < lines.len() {
        if lines[index].kind != LineKind::Machine {
            output.push_str(&render_line(&lines[index], cfg));
            index += 1;
            continue;
        }
        // end bounds the maximal non-prose run; cut excludes its trailing blanks from omission.
        // end 界定最大非自然语言段；cut 将其尾部空白排除在省略范围外。
        let mut end = index + 1;
        while end < lines.len() && lines[end].kind != LineKind::Natural {
            end += 1;
        }
        let mut cut = end;
        while cut > index && lines[cut - 1].kind == LineKind::Blank {
            cut -= 1;
        }
        if let Some(folded) = compress_run(&lines[index..cut], rules) {
            output.push_str(&folded);
            for line in &lines[cut..end] {
                output.push_str(&render_line(line, cfg));
            }
        } else {
            for line in &lines[index..end] {
                output.push_str(&render_line(line, cfg));
            }
        }
        index = end;
    }
    output
}

/// Renders a single line with UTF-8-safe horizontal clipping unless it contains an error anchor.
/// 渲染单行，除含错误锚点外按 UTF-8 安全边界进行水平截断。
fn render_line(line: &Line<'_>, cfg: &MachineTextCleanerConfig) -> String {
    if !line.horizontal_clip || line.anchor {
        return line.raw.to_owned();
    }
    // text excludes only the source newline suffix, keeping indentation in retained excerpts.
    // text 仅排除来源换行后缀，使保留摘录仍包含缩进。
    let text = strip_line_ending(line.raw);
    // chars ensures prefix and suffix slicing never breaks a Unicode scalar.
    // chars 确保前后缀切片不会破坏 Unicode 字符。
    let chars = text.chars().collect::<Vec<_>>();
    if chars.len() <= cfg.max_line_runes {
        return line.raw.to_owned();
    }
    // head and tail are bounded disjoint source excerpts.
    // head 与 tail 是有界且不重叠的来源摘录。
    let head = cfg.long_line_head_runes.min(chars.len());
    let tail = cfg.long_line_tail_runes.min(chars.len() - head);
    format!(
        "{}...[long machine line clipped]...{}{}",
        chars[..head].iter().collect::<String>(),
        chars[chars.len() - tail..].iter().collect::<String>(),
        &line.raw[text.len()..]
    )
}

/// Removes one LF and then one CR suffix as the source classifier does.
/// 与来源分类器一致，依次移除一个 LF 及一个 CR 后缀。
fn strip_line_ending(raw: &str) -> &str {
    // without_lf removes exactly one suffix, matching Go strings.TrimSuffix.
    // without_lf 仅移除一个后缀，与 Go strings.TrimSuffix 一致。
    let without_lf = raw.strip_suffix('\n').unwrap_or(raw);
    without_lf.strip_suffix('\r').unwrap_or(without_lf)
}

/// Returns a compressed run only when thresholds and actual non-anchor omissions justify it.
/// 仅当阈值及实际非锚点省略数量满足条件时返回压缩段。
fn compress_run(run: &[Line<'_>], rules: &MachineBlockRules) -> Option<String> {
    // stats and bounds retain source block eligibility and excerpt boundaries.
    // stats 及边界保留来源文本块资格判断与摘录边界。
    let stats = analyze(run);
    if !should_compress(&stats, rules) {
        return None;
    }
    let head = rules.head_lines.min(run.len());
    let tail = run.len().saturating_sub(rules.tail_lines).max(head);
    if run[head..tail].iter().filter(|line| !line.anchor).count() < rules.min_omitted_lines {
        return None;
    }
    // newline follows the first recorded line ending, not a platform default.
    // newline 遵循首先记录的换行格式，而非平台默认值。
    let newline = run
        .iter()
        .find_map(|line| {
            if line.raw.ends_with("\r\n") {
                Some("\r\n")
            } else if line.raw.ends_with('\n') {
                Some("\n")
            } else {
                None
            }
        })
        .unwrap_or("\n");
    // label matches the source priority when several structured signals coexist.
    // label 在多个结构信号共存时遵循来源优先级。
    let [stack, log, json, code] = stats.signals;
    let label = if stack >= log.max(json.max(code)) && stack >= 1 {
        "long error logs"
    } else if json >= log.max(code) && json >= 2 {
        "long structured data"
    } else if log >= json.max(code) && log >= 2 {
        "long runtime logs"
    } else if code >= 1 {
        "long code"
    } else {
        "long machine text"
    };
    // output and previous retain selected anchors and insert markers only for omitted intervals.
    // output 与 previous 保留选中锚点，并仅为省略区间插入标记。
    let mut output = run[..head].iter().map(|line| line.raw).collect::<String>();
    let mut previous = head;
    for (index, line) in run.iter().enumerate().take(tail).skip(head) {
        if !line.anchor {
            continue;
        }
        write_fold_marker(&mut output, index - previous, label, newline);
        output.push_str(line.raw);
        if !line.raw.ends_with('\n') && (index + 1 < tail || tail < run.len()) {
            output.push_str(newline);
        }
        previous = index + 1;
    }
    write_fold_marker(&mut output, tail - previous, label, newline);
    output.extend(run[tail..].iter().map(|line| line.raw));
    Some(output)
}

/// Appends the exact external fold marker for a positive omitted-line count.
/// 为正数省略行数追加准确的外置折叠标记。
fn write_fold_marker(output: &mut String, omitted: usize, label: &str, newline: &str) {
    if omitted > 0 {
        output.push_str(&format!(
            "... [{omitted} lines of {label} folded by policy to save context] ...{newline}"
        ));
    }
}

/// Aggregates classified lines without treating neutral or blank lines as machine evidence.
/// 汇总分类行，不将中性行或空白行当作机器文本证据。
fn analyze(lines: &[Line<'_>]) -> BlockStats {
    // stats is the aggregate used by the same source thresholds and signal priority.
    // stats 是相同来源阈值及信号优先级使用的汇总。
    let mut stats = BlockStats {
        lines: lines.len(),
        ..BlockStats::default()
    };
    for line in lines {
        stats.runes += line.runes;
        stats.han += line.han;
        stats.symbols += line.symbols;
        stats.nonblank += usize::from(line.kind != LineKind::Blank);
        stats.natural += usize::from(line.kind == LineKind::Natural);
        stats.machine += usize::from(line.kind == LineKind::Machine);
        for (total, signal) in stats.signals.iter_mut().zip(line.signals) {
            *total += usize::from(signal);
        }
    }
    stats
}

/// Applies the external block-size, language-protection and structure-density predicates.
/// 应用外置文本块大小、语言保护及结构密度谓词。
fn should_compress(s: &BlockStats, r: &MachineBlockRules) -> bool {
    if s.lines < r.min_lines
        || s.runes < r.min_runes
        || s.nonblank == 0
        || s.lines
            <= r.head_lines
                .saturating_add(r.tail_lines)
                .saturating_add(r.min_omitted_lines)
    {
        return false;
    }
    // ratios use nonblank lines for language evidence and scalar counts for symbol density.
    // 比例使用非空行统计语言证据，使用字符数统计符号密度。
    let machine = s.machine as f64 / s.nonblank as f64;
    let natural = s.natural as f64 / s.nonblank as f64;
    let symbols = s.symbols as f64 / s.runes.max(1) as f64;
    let [stack, log, json, code] = s.signals;
    if json * 100 >= s.nonblank.max(1) * 60 || log * 100 >= s.nonblank.max(1) * 60 {
        return true;
    }
    if s.han as f64 / s.runes.max(1) as f64 >= r.max_han_ratio
        || natural >= 0.35
        || (natural > 0.20 && s.machine < s.natural * 3)
    {
        return false;
    }
    // strong counts signal families, not individual matching lines.
    // strong 统计信号族，而非逐条匹配行。
    let strong = usize::from(stack >= 2)
        + usize::from(log >= 3)
        + usize::from(json >= 4)
        + usize::from(code >= 4.max(s.lines / 3));
    machine >= 0.80
        || (machine >= 0.65 && symbols >= 0.18 && strong >= 1)
        || (stack >= 2 && machine >= 0.55)
        || (log >= 4 && machine >= 0.55)
        || (json >= 6 && machine >= 0.50)
        || (code >= 6.max(s.lines / 2) && machine >= 0.60 && symbols >= 0.16)
}

/// Extracts the source line features and preserves natural prose before machine overrides.
/// 提取来源行特征，在机器规则覆盖前保护自然语言。
fn classify_line<'a>(raw: &'a str, cfg: &MachineTextCleanerConfig) -> Line<'a> {
    // text and trimmed retain original indentation separately from classified content.
    // text 与 trimmed 将原始缩进和分类正文分开保留。
    let text = strip_line_ending(raw);
    let trimmed = text.trim();
    // counters follow source precedence: Han, space, ASCII letter, ASCII digit, symbol.
    // 计数遵循来源优先级：汉字、空白、ASCII 字母、ASCII 数字、符号。
    let (mut runes, mut han, mut symbols, mut ascii, mut digits, mut spaces) =
        (0usize, 0usize, 0usize, 0usize, 0usize, 0usize);
    for ch in trimmed.chars() {
        runes += 1;
        // encoded is a temporary UTF-8 buffer for Unicode category matching.
        // encoded 是用于 Unicode 类别匹配的临时 UTF-8 缓冲区。
        let mut encoded = [0u8; 4];
        if HAN.is_match(ch.encode_utf8(&mut encoded)) {
            han += 1;
        } else if ch.is_whitespace() {
            spaces += 1;
        } else if ch.is_ascii_alphabetic() {
            ascii += 1;
        } else if ch.is_ascii_digit() {
            digits += 1;
        } else if SYMBOL.is_match(ch.encode_utf8(&mut encoded)) {
            symbols += 1;
        }
    }
    // ratios, words and flags carry the source's independent evidence channels.
    // 比例、词数及标记携带来源相互独立的证据通道。
    let symbol_ratio = symbols as f64 / runes.max(1) as f64;
    let han_ratio = han as f64 / runes.max(1) as f64;
    let words = natural_words(trimmed);
    let indent = text.starts_with("  ") || text.starts_with('\t');
    let lower = trimmed.to_lowercase();
    let anchor = [
        "caused by:",
        "panic:",
        "fatal error:",
        "runtime error:",
        "root cause:",
    ]
    .iter()
    .any(|signal| lower.contains(signal));
    let stack = stack_signal(trimmed);
    let log = log_signal(trimmed);
    let json = matches!(trimmed, "{" | "}" | "[" | "]" | "}," | "],")
        || trimmed.starts_with(['{', '['])
        || (trimmed.starts_with('"') && trimmed.contains("\":"))
        || (trimmed.starts_with('\'') && trimmed.contains("':"));
    let code = code_signal(trimmed, indent, han_ratio);
    let structured = log
        || (json && (symbol_ratio >= 0.08 || indent || trimmed.starts_with(['{', '[', '"', '\''])));
    let chinese = !structured && (han >= 4 || (han_ratio >= 0.15 && runes >= 20));
    let english_rich = han == 0 && words >= 8 && ascii >= 24 && !stack && !log && !json;
    let english_very_rich =
        han == 0 && words >= 12 && ascii >= 30.max(runes / 3) && !stack && !log && !json;
    let english = (words >= 6 && symbol_ratio < 0.18 && !stack && !log && !json)
        || (english_rich && symbol_ratio < 0.32)
        || (english_very_rich && symbol_ratio < 0.40);
    let sentence = (trimmed.contains(['。', '！', '？', '?', '!']) && (han >= 2 || words >= 5))
        || (trimmed.ends_with('.') && words >= 8 && !stack && !log && !json);
    let heavy = symbol_ratio >= 0.30 && runes >= 24;
    let code_density = symbol_ratio >= 0.18
        && (trimmed.contains(['{', '}', '[', ']', '(', ')', ';', '=', '<', '>']) || indent);
    let machine = stack
        || structured
        || (code && han_ratio < 0.08 && (code_density || heavy || indent))
        || (han_ratio < 0.08 && heavy && (indent || digits > 0 || ascii > 0));
    let kind = if trimmed.is_empty() {
        LineKind::Blank
    } else if machine {
        LineKind::Machine
    } else if chinese || english || sentence {
        LineKind::Natural
    } else {
        LineKind::Neutral
    };
    let density = (ascii + digits + symbols) as f64 / runes.max(1) as f64;
    let token = !trimmed.is_empty()
        && trimmed
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"+/=-_".contains(&byte));
    let horizontal_clip = !anchor
        && runes >= cfg.max_line_runes
        && (json
            || log
            || stack
            || (spaces == 0 && token)
            || (han == 0
                && words <= 8
                && spaces <= 8
                && density >= 0.92
                && (symbol_ratio >= 0.10 || code))
            || (han == 0 && words <= 4 && density >= 0.97));
    Line {
        raw,
        kind,
        runes,
        han,
        symbols,
        signals: [stack, log, json, code],
        anchor,
        horizontal_clip,
    }
}

/// Counts source-compatible words containing at least two Unicode letters.
/// 统计至少含两个 Unicode 字母且与来源兼容的词。
fn natural_words(text: &str) -> usize {
    text.split(|ch: char| ch.is_whitespace() || ",.;:!?，。；：！？()[]{}<>/\\\"'`|".contains(ch))
        .filter(|word| {
            word.chars()
                .filter(|ch| {
                    // encoded permits exact Unicode letter-category testing without byte truncation.
                    // encoded 允许准确判断 Unicode 字母类别而不截断字节。
                    let mut encoded = [0u8; 4];
                    LETTER.is_match(ch.encode_utf8(&mut encoded))
                })
                .count()
                >= 2
        })
        .count()
}

/// Detects the same stack trace prefixes, locations and qualified calls as the external cleaner.
/// 检测与外置清洗器相同的堆栈前缀、位置及限定调用。
fn stack_signal(text: &str) -> bool {
    // lower supports the source's case-insensitive stack prefixes and suffix checks.
    // lower 支持来源不区分大小写的堆栈前缀及后缀检查。
    let lower = text.to_lowercase();
    [
        "goroutine ",
        "panic:",
        "fatal error:",
        "traceback",
        "caused by:",
        "runtime error:",
    ]
    .iter()
    .any(|prefix| lower.starts_with(prefix))
        || (lower.contains("exception") && text.contains(['.', ':']))
        || (text.starts_with("at ") && text.contains('(') && text.contains(':'))
        || [".go:", ".java:", ".py:", ".ts:", ".js:"]
            .iter()
            .any(|suffix| lower.contains(suffix))
        || (!text.contains(' ')
            && text.contains('.')
            && ["()", "(...)", "]:"]
                .iter()
                .any(|suffix| text.ends_with(suffix)))
        || (text.starts_with("File ") && text.contains(", line "))
}

/// Detects source log prefixes, including byte-positioned ISO-style timestamps.
/// 检测来源日志前缀，包括按字节位置匹配的 ISO 风格时间戳。
fn log_signal(text: &str) -> bool {
    // bytes and upper retain source timestamp and case-insensitive severity rules.
    // bytes 与 upper 保留来源时间戳及不区分大小写的日志等级规则。
    let bytes = text.as_bytes();
    let upper = text.to_uppercase();
    let timestamp = bytes.len() >= 19
        && [0, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15, 17, 18]
            .iter()
            .all(|index| bytes[*index].is_ascii_digit())
        && b"-/".contains(&bytes[4])
        && b"-/".contains(&bytes[7])
        && b"T ".contains(&bytes[10])
        && bytes[13] == b':'
        && bytes[16] == b':';
    timestamp
        || (text.starts_with('[')
            && ["INFO", "WARN", "ERROR", "DEBUG"]
                .iter()
                .any(|level| upper.contains(level)))
        || ["INFO ", "WARN ", "ERROR ", "DEBUG "]
            .iter()
            .any(|prefix| upper.starts_with(prefix))
        || [" LEVEL=", " MSG=", " TRACE="]
            .iter()
            .any(|marker| upper.contains(marker))
}

/// Detects source code signals without overriding the source's Han-ratio protection.
/// 检测源码信号，同时不绕过来源的汉字比例保护。
fn code_signal(text: &str, indent: bool, han_ratio: f64) -> bool {
    if han_ratio >= 0.15 {
        return false;
    }
    // lower, structural and hits are independent source code-classification facts.
    // lower、structural 及 hits 是相互独立的来源代码分类事实。
    let lower = text.to_lowercase();
    let structural = text.contains(['{', '}', '[', ']', '(', ')', ';'])
        || [":=", "=>", "==", "!=", " <= ", ">="]
            .iter()
            .any(|symbol| text.contains(symbol));
    let hits = [
        "package ",
        "import ",
        "func ",
        "return ",
        "if ",
        "for ",
        "switch ",
        "case ",
        "var ",
        "const ",
        "type ",
        "class ",
        "public ",
        "private ",
        "protected ",
        "static ",
        "void ",
        "new ",
        "def ",
        "let ",
        "const ",
        "select ",
        "insert ",
        "update ",
        "delete ",
        "from ",
        "where ",
        "try ",
        "catch ",
        "finally ",
        "throw ",
    ]
    .iter()
    .filter(|keyword| lower.contains(**keyword))
    .count();
    (hits >= 2 && structural)
        || (hits >= 1 && (structural || indent))
        || (structural && indent && lower.contains(['(', '[', '.']))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the external structured-log fixture folds exactly its middle nine lines.
    /// 验证外置结构化日志样本准确折叠中间九行。
    #[test]
    fn structured_logs_keep_exact_head_and_tail() {
        // lines reproduces the external sample with stable per-line trace characters.
        // lines 使用逐行稳定追踪字符重现外置样本。
        let lines = ('a'..='n')
            .enumerate()
            .map(|(index, ch)| {
                format!(
                    "{{\"level\":\"info\",\"msg\":\"{}\",\"trace\":\"{}\",\"module\":\"sync\"}}",
                    if index == 0 {
                        "boot"
                    } else if index == 13 {
                        "finish"
                    } else {
                        "sync"
                    },
                    ch.to_string().repeat(32)
                )
            })
            .collect::<Vec<_>>();
        // expected fixes the source marker, omitted count and preserved source lines.
        // expected 固定来源标记、省略计数及保留的来源行。
        let expected = format!(
            "{}\n... [9 lines of long structured data folded by policy to save context] ...\n{}",
            lines[..3].join("\n"),
            lines[12..].join("\n")
        );
        assert_eq!(
            clean_machine_text(&lines.join("\n"), &MachineTextCleanerConfig::default())
                .expect("clean"),
            expected
        );
    }

    /// Verifies prose, short fences and unmatched fences remain intact.
    /// 验证自然语言、短围栏及未配对围栏保持原样。
    #[test]
    fn human_facts_and_short_code_are_preserved() {
        for text in [
            "我喜欢西瓜和葡萄。",
            "```rust\nlet fruit = watermelon;\n```\nUser likes grapes.",
            "Unclosed example:\n```\nkeep this user fact",
        ] {
            assert_eq!(
                clean_machine_text(text, &MachineTextCleanerConfig::default()).expect("clean"),
                text
            );
        }
        // prose crosses the short-fragment threshold but still must never become machine text.
        // prose 超过短片段阈值，但仍绝不能被当作机器文本。
        let prose = "用户明确表示喜欢西瓜和葡萄，希望保留这些饮食偏好。\n".repeat(30);
        assert_eq!(
            clean_machine_text(&prose, &MachineTextCleanerConfig::default()).expect("clean"),
            prose
        );
    }

    /// Verifies horizontal clipping respects source limits, line endings and root-cause anchors.
    /// 验证水平截断遵循来源限制、换行及根因锚点。
    #[test]
    fn long_machine_lines_clip_without_removing_root_causes() {
        // token is a dense source-shaped machine line over the horizontal threshold.
        // token 是超过水平阈值的高密度来源形态机器行。
        let token = format!("{}\r\n", "a".repeat(1200));
        assert_eq!(
            clean_machine_text(&token, &MachineTextCleanerConfig::default()).expect("clean"),
            format!(
                "{}...[long machine line clipped]...{}\r\n",
                "a".repeat(240),
                "a".repeat(120)
            )
        );
        // anchor is deliberately oversized but represents protected diagnostic evidence.
        // anchor 有意超长，但代表受保护的诊断证据。
        let anchor = format!("panic: {}", "a".repeat(1200));
        assert_eq!(
            clean_machine_text(&anchor, &MachineTextCleanerConfig::default()).expect("clean"),
            anchor
        );
    }
}
