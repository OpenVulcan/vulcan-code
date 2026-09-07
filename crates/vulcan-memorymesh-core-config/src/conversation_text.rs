//! External-compatible media/thought stripping with preserved memory line boundaries.
//! 与外置兼容的媒体及思考内容剥离，保留记忆文本行边界。

use crate::resource_path::resource_url_path;
use crate::{MachineTextCleanerConfig, clean_machine_text};
use regex::{Captures, Regex};
use std::sync::LazyLock;

/// Holds fixed external regexes in pipeline order; ASCII boundaries match Go regexp semantics.
/// 按流水线顺序保存固定外置正则；ASCII 边界与 Go regexp 语义一致。
static MEDIA_PATTERNS: LazyLock<[Regex; 7]> = LazyLock::new(|| {
    [
        r"!\[([^\]]*)\]\(([^)]+)\)",
        r"\[([^\]]*)\]\(([^)]+)\)",
        r"(?is)<a(?-u:\b)([^>]*)>(.*?)</a>",
        r"(?is)<(img|source)(?-u:\b)([^>]*)>",
        r"(?is)<(video|audio|object|embed|iframe)(?-u:\b)([^>]*)>.*?</(?:video|audio|object|embed|iframe)>",
        r"(?i)data:[a-z0-9.+-]+/[a-z0-9.+-]+(?:;[a-z0-9=._+-]+)*;base64,[a-z0-9+/=\r\n]+",
        r#"(?i)(?-u:\b)(?:https?://|blob:|file:/+)[^\t\n\f\r <>"')]+"#,
    ].map(|pattern| Regex::new(pattern).expect("static external media pattern"))
});

/// Removes the five external hidden-thought tag families in their original order.
/// 按原有顺序移除外置的五类隐藏思考标签。
static THOUGHT_PATTERNS: LazyLock<[Regex; 5]> = LazyLock::new(|| {
    ["think", "analysis", "reasoning", "scratchpad", "reflection"]
        .map(|tag| Regex::new(&format!("(?is)<{tag}>.*?</{tag}>")).expect("static thought pattern"))
});

/// Strips HTML from anchor labels, not from arbitrary user text.
/// 从锚点标签中剥离 HTML，不作用于任意用户正文。
static HTML_TAG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<[^>]+>").expect("static HTML tag pattern"));

/// Collapses Go regexp's ASCII whitespace set for short resource labels.
/// 为短资源标签折叠 Go regexp 的 ASCII 空白集合。
static LABEL_SPACE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\t\n\f\r ]+").expect("static label whitespace pattern"));

/// Normalizes CR/LF and line edges while retaining at most one blank separator; returns owned text.
/// 规范化 CR/LF 及行边空白，最多保留一个空行分隔；返回拥有所有权的文本。
pub fn normalize_memory_whitespace(text: &str) -> String {
    // Keep internal whitespace inside each nonblank line intact for the external machine classifier.
    // 保留非空行内部空白，供外置同规则的机器文本分类器使用。
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    // Build output without emitting a leading or trailing blank block.
    // 构造输出时不写入开头或结尾空行块。
    let mut output = String::new();
    // Defer one separator until the next nonblank line proves it is internal.
    // 暂存一个分隔符，等待下一个非空行确认它属于内部空行。
    let mut blank = false;
    for line in normalized.split('\n').map(str::trim) {
        if line.is_empty() {
            blank = true;
            continue;
        }
        if !output.is_empty() {
            output.push_str(if blank { "\n\n" } else { "\n" });
        }
        output.push_str(line);
        blank = false;
    }
    output
}

/// Replaces media references and hidden thoughts in one fragment without flattening useful lines.
/// 替换单段文本中的媒体引用及隐藏思考，不压平有用的行结构。
pub fn clean_conversation_text_for_memory(text: &str) -> String {
    // Process images before links so their leading exclamation mark cannot leak into placeholders.
    // 先处理图片再处理链接，避免感叹号残留在占位符前。
    let mut output = MEDIA_PATTERNS[0]
        .replace_all(text, |capture: &Captures<'_>| {
            // Markdown images explicitly default to Image when the target has no recognized extension.
            // Markdown 图片在目标无已知扩展名时明确默认归类为 Image。
            let kind = resource_kind(capture[2].trim());
            placeholder(
                if kind.is_empty() { "Image" } else { kind },
                &normalize_memory_whitespace(&capture[1]),
                capture[2].trim(),
            )
        })
        .into_owned();
    output = MEDIA_PATTERNS[1]
        .replace_all(&output, |capture: &Captures<'_>| {
            // Ordinary web links remain intact; only recognized resources are replaced.
            // 普通网页链接保持不变；仅替换已识别资源。
            let kind = resource_kind(capture[2].trim());
            if kind.is_empty() {
                capture[0].to_owned()
            } else {
                placeholder(
                    kind,
                    &normalize_memory_whitespace(&capture[1]),
                    capture[2].trim(),
                )
            }
        })
        .into_owned();
    output = MEDIA_PATTERNS[2]
        .replace_all(&output, |capture: &Captures<'_>| {
            // External anchor processing keeps the visible label even for non-resource links.
            // 外置锚点处理即使面对非资源链接，也保留可见标签。
            let target = html_attribute(&capture[1], "href");
            // Classify the actual href, not the visible anchor label.
            // 对实际 href 分类，不使用可见锚点标签分类。
            let kind = resource_kind(&target);
            // Strip nested tags before normalizing the anchor's human-readable label.
            // 在规范化锚点可读标签之前剥离嵌套标签。
            let label = normalize_memory_whitespace(&HTML_TAG.replace_all(&capture[2], " "));
            if kind.is_empty() {
                label
            } else {
                placeholder(kind, &label, &target)
            }
        })
        .into_owned();
    for pattern in &MEDIA_PATTERNS[3..5] {
        output = pattern
            .replace_all(&output, media_tag_placeholder)
            .into_owned();
    }
    output = MEDIA_PATTERNS[5]
        .replace_all(&output, |capture: &Captures<'_>| {
            placeholder(resource_kind(&capture[0]), "", &capture[0])
        })
        .into_owned();
    output = MEDIA_PATTERNS[6]
        .replace_all(&output, |capture: &Captures<'_>| {
            // Unknown raw URLs are intentionally retained, matching the external media-only filter.
            // 明确保留未知原始 URL，与外置仅过滤媒体的行为一致。
            let kind = resource_kind(&capture[0]);
            if kind.is_empty() {
                capture[0].to_owned()
            } else {
                placeholder(kind, "", &capture[0])
            }
        })
        .into_owned();
    for pattern in THOUGHT_PATTERNS.iter() {
        output = pattern.replace_all(&output, "\n").into_owned();
    }
    normalize_memory_whitespace(&output)
}

/// Applies the external default media/folding/whitespace/token pipeline; returns cleaned text or a config error.
/// 应用外置默认媒体、折叠、空白及 Token 流水线；返回清洗文本或配置错误。
pub fn sanitize_postaction_text(text: &str) -> Result<String, String> {
    sanitize_postaction_text_with_policy(text, &crate::PostActionTextPolicy::default())
}

/// Cleans one fragment using an admitted policy while always stripping media payloads and reasoning tags.
/// 使用已准入策略清洗一个片段，始终移除媒体载荷和思考标签内容。
/// Returns reduced text or a policy/rule error; disabling folding never bypasses mandatory cleanup.
/// 返回缩减文本或策略/规则错误；禁用折叠绝不绕过强制清理。
pub fn sanitize_postaction_text_with_policy(
    text: &str,
    policy: &crate::PostActionTextPolicy,
) -> Result<String, String> {
    policy.validate()?;
    // Folding must observe the media-cleaned line structure, before persistence token clipping.
    // 折叠必须读取媒体清理后的行结构，并先于持久化 Token 裁剪。
    let media = clean_conversation_text_for_memory(text);
    // Use the already differential-tested external default machine rules.
    // 使用已通过差异对照的外置默认机器文本规则。
    let folded = if policy.fold_machine_text {
        clean_machine_text(&media, &MachineTextCleanerConfig::default())?
    } else {
        media
    };
    Ok(normalize_memory_whitespace(
        &crate::token_budget::clip_memory_text_to_budget(
            &normalize_memory_whitespace(&folded),
            policy.max_fragment_tokens,
        ),
    ))
}

/// Builds one media-tag placeholder from the regex's exact tag and attribute captures.
/// 从正则准确捕获的标签及属性构造一个媒体标签占位符。
fn media_tag_placeholder(capture: &Captures<'_>) -> String {
    // External tags declare src, then data, then href precedence; this is not guessed compatibility.
    // 外置标签明确规定 src、data、href 的优先级；这不是猜测式兼容。
    let target = ["src", "data", "href"]
        .into_iter()
        .map(|name| html_attribute(&capture[2], name))
        .find(|value| !value.is_empty())
        .unwrap_or_default();
    // URL classification wins over the HTML tag's fallback semantic kind.
    // URL 分类优先于 HTML 标签的默认语义类别。
    let classified = resource_kind(&target);
    // Only known media tags reach this function through the fixed capture grammar.
    // 只有已知媒体标签能够通过固定捕获语法进入此函数。
    let kind = if !classified.is_empty() {
        classified
    } else {
        match capture[1].to_ascii_lowercase().as_str() {
            "img" => "Image",
            "video" => "Video",
            "audio" => "Audio",
            _ => "Attachment",
        }
    };
    placeholder(
        kind,
        &normalize_memory_whitespace(&html_attribute(&capture[2], "alt")),
        &target,
    )
}

/// Reads one attribute with external textutil's quoted/unquoted grammar; absent or unclosed values are empty.
/// 按外置 textutil 的带引号或无引号语法读取属性；缺失或引号未闭合时返回空串。
fn html_attribute(attributes: &str, name: &str) -> String {
    // Case-insensitive ASCII matching preserves byte offsets even when preceding labels contain Unicode.
    // ASCII 大小写不敏感匹配保留字节偏移，即使前方标签包含 Unicode 也不会切错字符。
    let needle = format!("{name}=");
    // Names are fixed ASCII literals provided only by this module.
    // 名称仅由本模块提供，都是固定 ASCII 字面值。
    let Some(index) = attributes.to_ascii_lowercase().find(&needle) else {
        return String::new();
    };
    // Trim outside the exact value before determining quoting.
    // 在判断引号形式前裁剪准确值外部的空白。
    let rest = attributes[index + needle.len()..].trim();
    if let Some(quote @ ('\'' | '"')) = rest.chars().next() {
        return rest[1..]
            .find(quote)
            .map_or_else(String::new, |end| rest[1..1 + end].trim().to_owned());
    }
    rest.split([' ', '\t', '\r', '\n', '>'])
        .next()
        .unwrap_or_default()
        .trim()
        .to_owned()
}

/// Classifies a resource target using external MIME prefixes and its exact extension allowlist.
/// 按外置 MIME 前缀及准确的扩展名允许列表对资源目标分类。
fn resource_kind(target: &str) -> &'static str {
    // External classification is case-insensitive, independently from displayed filename case.
    // 外置分类大小写不敏感，与文件名显示大小写相互独立。
    let lower = target.trim().to_lowercase();
    for (prefix, kind) in [
        ("data:image/", "Image"),
        ("data:video/", "Video"),
        ("data:audio/", "Audio"),
        ("data:", "Attachment"),
    ] {
        if lower.starts_with(prefix) {
            return kind;
        }
    }
    // External resourceExtension explicitly uses the raw target only when URL.Parse fails.
    // 外置 resourceExtension 明确规定仅在 URL.Parse 失败时使用原始目标。
    let path = resource_url_path(&lower).unwrap_or(lower.clone());
    // path.Ext examines only the final slash-separated element, including an empty final element.
    // path.Ext 仅检查最后一个斜杠分隔元素，包括空的末尾元素。
    let final_segment = path.rsplit('/').next().unwrap_or_default();
    // Keep the dot in the extension; leading dot files follow external path.Ext semantics too.
    // 扩展名保留点号；以点开头的文件同样遵循外置 path.Ext 语义。
    let extension = final_segment
        .rfind('.')
        .map_or("", |dot| &final_segment[dot..])
        .to_lowercase();
    if extension.is_empty() && (lower.starts_with("blob:") || lower.starts_with("file:")) {
        return "Attachment";
    }
    match extension.as_str() {
        ".png" | ".jpg" | ".jpeg" | ".gif" | ".webp" | ".bmp" | ".svg" | ".ico" => "Image",
        ".mp4" | ".mov" | ".avi" | ".webm" | ".mkv" => "Video",
        ".mp3" | ".wav" | ".ogg" | ".flac" | ".aac" | ".m4a" => "Audio",
        ".zip" | ".rar" | ".7z" | ".tar" | ".gz" | ".tgz" | ".bz2" | ".xz" => "Archive",
        ".pdf" | ".doc" | ".docx" | ".xls" | ".xlsx" | ".ppt" | ".pptx" | ".csv" | ".txt"
        | ".md" => "File",
        _ => "",
    }
}

/// Builds a short semantic placeholder, deriving a filename only when the visible label is empty.
/// 构造短语义占位符，仅在可见标签为空时提取文件名。
fn placeholder(kind: &str, label: &str, target: &str) -> String {
    // External placeholder labels flatten ASCII whitespace even though the body keeps line boundaries.
    // 外置占位符标签压平 ASCII 空白，但正文保留行边界。
    let mut label = LABEL_SPACE.replace_all(label.trim(), " ").trim().to_owned();
    if label.is_empty()
        && !target.trim().to_ascii_lowercase().starts_with("data:")
        && let Some(path) = resource_url_path(target.trim())
    {
        // path.Base strips trailing slashes but does not normalize dot segments.
        // path.Base 去除末尾斜杠，但不规范化点路径段。
        let base = path
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or_default();
        if base != "." {
            label = LABEL_SPACE.replace_all(base.trim(), " ").trim().to_owned();
        }
    }
    // A resource family is normally known; raw data URLs also explicitly classify as Attachment.
    // 通常已有资源类别；原始 data URL 也会明确分类为 Attachment。
    let kind = if kind.is_empty() { "Resource" } else { kind };
    if label.is_empty() {
        format!("[{kind} filtered]")
    } else {
        format!("[{kind}: {label}]")
    }
}
