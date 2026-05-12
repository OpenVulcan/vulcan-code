//! Shared helper functions extracted from the main application module.
//! 从主应用模块中抽取的共享辅助函数。

use super::*;

/// Builds the initial pane-grid configuration for the three-column workbench.
/// 构建三栏工作台的初始分栏网格配置。
pub(super) fn pane_layout() -> Configuration<PaneKind> {
    Configuration::Split {
        axis: pane_grid::Axis::Vertical,
        ratio: 0.22,
        a: Box::new(Configuration::Pane(PaneKind::Navigator)),
        b: Box::new(Configuration::Split {
            axis: pane_grid::Axis::Vertical,
            ratio: 0.68,
            a: Box::new(Configuration::Pane(PaneKind::Workbench)),
            b: Box::new(Configuration::Pane(PaneKind::Inspector)),
        }),
    }
}

/// Truncates one label to a compact single-line representation with a trailing ellipsis when needed.
/// 在需要时将标签截断为带尾部省略号的紧凑单行表示。
pub(super) fn truncate_with_ellipsis(value: &str, max_width_px: f32) -> String {
    let display_width = value.chars().map(char_display_width_px).sum::<f32>();
    let ellipsis = "…";
    let ellipsis_width = 6.4_f32;

    if display_width <= max_width_px {
        return value.to_string();
    }

    if max_width_px <= ellipsis_width {
        return String::from(ellipsis);
    }

    let mut visible = String::new();
    let mut used_width = 0.0_f32;
    let allowed_width = max_width_px - ellipsis_width;

    for character in value.chars() {
        let character_width = char_display_width_px(character);

        if used_width + character_width > allowed_width {
            break;
        }

        visible.push(character);
        used_width += character_width;
    }

    format!("{visible}{ellipsis}")
}

/// Estimates the visual width in pixels consumed by one character in compact single-line labels.
/// 估算单个字符在紧凑单行标签中消耗的视觉像素宽度。
pub(super) fn char_display_width_px(character: char) -> f32 {
    if character.is_ascii_whitespace() {
        3.6
    } else if character.is_ascii_punctuation() {
        4.9
    } else if character.is_ascii_digit() || character.is_ascii_uppercase() {
        7.9
    } else if character.is_ascii_lowercase() {
        7.0
    } else if matches!(
        character as u32,
        0x1100..=0x115F
            | 0x2329..=0x232A
            | 0x2E80..=0xA4CF
            | 0xAC00..=0xD7A3
            | 0xF900..=0xFAFF
            | 0xFE10..=0xFE19
            | 0xFE30..=0xFE6F
            | 0xFF00..=0xFF60
            | 0xFFE0..=0xFFE6
            | 0x1F300..=0x1FAFF
            | 0x20000..=0x3FFFD
    ) {
        12.8
    } else {
        7.8
    }
}

/// Compacts one relative-time label for narrow task-tree trailing slots.
/// 为狭窄任务树尾部槽位压缩相对时间标签。
pub(super) fn compact_relative_label(value: &str) -> String {
    value.replace(' ', "").trim_end_matches('前').to_string()
}

/// Builds the stable scrollable identifier used by one session transcript surface across all transcript presentation modes.
/// 为单个会话转录表面在全部展示模式下构建稳定滚动容器标识。
pub(super) fn transcript_scrollable_id(session_id: SessionId) -> iced::widget::Id {
    iced::widget::Id::from(format!("session-{session_id}-agent-transcript-scrollable"))
}

/// Returns whether one transcript viewport should still be treated as pinned to the newest tail content.
/// 返回某个转录视口是否仍应被视为贴住最新尾部内容。
pub(super) fn transcript_viewport_is_near_bottom(viewport: scrollable::Viewport) -> bool {
    let absolute_offset = viewport.absolute_offset();
    let viewport_bounds = viewport.bounds();
    let content_bounds = viewport.content_bounds();
    let bottom_gap =
        (content_bounds.height - (absolute_offset.y + viewport_bounds.height)).max(0.0);

    bottom_gap <= TRANSCRIPT_FOLLOW_TAIL_BOTTOM_GAP_THRESHOLD
}

/// Converts one pointer overshoot request into a conventional edge-scroll step that accelerates with distance and caps near the far edge.
/// 将一次指针越界请求转换为常见的边缘滚动步长：随距离增大而加速，并在远离边缘时封顶。
pub(super) fn drag_auto_scroll_step(request: SelectableTranscriptDragAutoScroll) -> f32 {
    let overshoot = request
        .overshoot
        .clamp(0.0, TRANSCRIPT_DRAG_AUTO_SCROLL_MAX_OVERSHOOT);

    if overshoot <= TRANSCRIPT_DRAG_AUTO_SCROLL_DEAD_ZONE {
        return 0.0;
    }

    let normalized = ((overshoot - TRANSCRIPT_DRAG_AUTO_SCROLL_DEAD_ZONE)
        / (TRANSCRIPT_DRAG_AUTO_SCROLL_MAX_OVERSHOOT - TRANSCRIPT_DRAG_AUTO_SCROLL_DEAD_ZONE))
        .clamp(0.0, 1.0);
    let eased = 1.0 - (1.0 - normalized).powi(2);
    let magnitude = TRANSCRIPT_DRAG_AUTO_SCROLL_MIN_STEP
        + (TRANSCRIPT_DRAG_AUTO_SCROLL_MAX_STEP - TRANSCRIPT_DRAG_AUTO_SCROLL_MIN_STEP) * eased;

    match request.direction {
        SelectableTranscriptDragAutoScrollDirection::Up => -magnitude,
        SelectableTranscriptDragAutoScrollDirection::Down => magnitude,
    }
}

/// Parses one Markdown source string and builds the paired selectable buffer used by read-only transcript views.
/// 解析一段 Markdown 源文本，并构建只读转录视图使用的配套可选缓冲区。
pub(super) fn parse_selectable_markdown_body(
    markdown_source: &str,
    theme: &Theme,
) -> (markdown::Content, text_editor::Content) {
    let markdown_content = markdown::Content::parse(markdown_source);
    let markdown_buffer = text_editor::Content::with_text(&build_selectable_markdown_text(
        markdown_source,
        &markdown_content,
        theme,
    ));

    (markdown_content, markdown_buffer)
}

/// Builds one selectable plain-text projection from parsed Markdown while preserving the major reading structure.
/// 从已解析 Markdown 构建一份可选纯文本投影，同时保留主要阅读结构。
pub(super) fn build_selectable_markdown_text(
    markdown_source: &str,
    content: &markdown::Content,
    theme: &Theme,
) -> String {
    if markdown_items_contain_table(content.items()) {
        return markdown_source.to_string();
    }

    let rendered =
        markdown_items_to_selectable_text(content.items(), theme::markdown_style(theme), 0);
    let trimmed = rendered.trim_end().to_string();

    if trimmed.trim().is_empty() {
        markdown_source.to_string()
    } else {
        trimmed
    }
}

/// Returns whether the parsed Markdown tree contains any table node that cannot yet be faithfully projected with the public iced API.
/// 返回已解析 Markdown 树是否包含当前无法借助公开 iced API 完整投影的表格节点。
pub(super) fn markdown_items_contain_table(items: &[markdown::Item]) -> bool {
    items.iter().any(|item| match item {
        markdown::Item::Table { .. } => true,
        markdown::Item::List { bullets, .. } => bullets.iter().any(|bullet| match bullet {
            markdown::Bullet::Point { items } | markdown::Bullet::Task { items, .. } => {
                markdown_items_contain_table(items)
            }
        }),
        markdown::Item::Quote(items) => markdown_items_contain_table(items),
        markdown::Item::Heading(_, _)
        | markdown::Item::Paragraph(_)
        | markdown::Item::CodeBlock { .. }
        | markdown::Item::Image { .. }
        | markdown::Item::Rule => false,
    })
}

/// Converts a parsed Markdown item slice into one selectable plain-text block sequence.
/// 将一组已解析 Markdown 节点转换为一段可选纯文本区块序列。
pub(super) fn markdown_items_to_selectable_text(
    items: &[markdown::Item],
    style: markdown::Style,
    list_depth: usize,
) -> String {
    items
        .iter()
        .map(|item| markdown_item_to_selectable_text(item, style, list_depth))
        .filter(|block| !block.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Converts one parsed Markdown item into its selectable plain-text projection.
/// 将单个已解析 Markdown 节点转换为其可选纯文本投影。
pub(super) fn markdown_item_to_selectable_text(
    item: &markdown::Item,
    style: markdown::Style,
    list_depth: usize,
) -> String {
    match item {
        markdown::Item::Heading(level, text) => format!(
            "{} {}",
            markdown_heading_prefix(level),
            markdown_text_to_selectable_text(text, style)
        ),
        markdown::Item::Paragraph(text) => markdown_text_to_selectable_text(text, style),
        markdown::Item::CodeBlock { language, code, .. } => {
            let fence = language
                .as_deref()
                .map(|language| format!("```{language}"))
                .unwrap_or_else(|| String::from("```"));
            let code_body = code.trim_end_matches('\n');

            if code_body.is_empty() {
                format!("{fence}\n```")
            } else {
                format!("{fence}\n{code_body}\n```")
            }
        }
        markdown::Item::List {
            start: None,
            bullets,
        } => bullets
            .iter()
            .map(|bullet| {
                let indent = "  ".repeat(list_depth);
                let (prefix, body) = match bullet {
                    markdown::Bullet::Point { items } => (
                        format!("{indent}- "),
                        markdown_items_to_selectable_text(items, style, list_depth + 1),
                    ),
                    markdown::Bullet::Task { items, done } => (
                        format!("{indent}- [{}] ", if *done { "x" } else { " " }),
                        markdown_items_to_selectable_text(items, style, list_depth + 1),
                    ),
                };

                prefix_multiline_markdown_block(&body, &prefix, &" ".repeat(prefix.chars().count()))
            })
            .collect::<Vec<_>>()
            .join("\n"),
        markdown::Item::List {
            start: Some(start),
            bullets,
        } => bullets
            .iter()
            .enumerate()
            .map(|(index, bullet)| {
                let indent = "  ".repeat(list_depth);
                let prefix = format!("{indent}{}. ", *start + index as u64);
                let body = match bullet {
                    markdown::Bullet::Point { items } | markdown::Bullet::Task { items, .. } => {
                        markdown_items_to_selectable_text(items, style, list_depth + 1)
                    }
                };

                prefix_multiline_markdown_block(&body, &prefix, &" ".repeat(prefix.chars().count()))
            })
            .collect::<Vec<_>>()
            .join("\n"),
        markdown::Item::Image { url, title, alt } => {
            let alt_text = markdown_text_to_selectable_text(alt, style);

            if title.is_empty() {
                format!("![{alt_text}]({url})")
            } else {
                format!("![{alt_text}]({url} \"{title}\")")
            }
        }
        markdown::Item::Quote(items) => markdown_items_to_selectable_text(items, style, list_depth)
            .lines()
            .map(|line| {
                if line.is_empty() {
                    String::from(">")
                } else {
                    format!("> {line}")
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
        markdown::Item::Rule => String::from("---"),
        markdown::Item::Table { .. } => String::new(),
    }
}

/// Converts one parsed Markdown inline-text payload into a readable plain-text fragment sequence.
/// 将一段已解析 Markdown 行内文本载荷转换为可读的纯文本片段序列。
pub(super) fn markdown_text_to_selectable_text(
    text: &markdown::Text,
    style: markdown::Style,
) -> String {
    let mut output = String::new();
    let mut active_link: Option<String> = None;
    let mut link_buffer = String::new();

    for span in text.spans(style).iter() {
        let span_link = span.link.clone();

        if span_link != active_link {
            flush_markdown_link_buffer(&mut output, &mut active_link, &mut link_buffer);
            active_link = span_link;
        }

        let mut fragment = span.text.as_ref().to_string();
        let is_inline_code = span.highlight.is_some() || span.font == Some(style.inline_code_font);

        if is_inline_code && !fragment.is_empty() {
            fragment = format!("`{fragment}`");
        }

        if active_link.is_some() {
            link_buffer.push_str(&fragment);
        } else {
            output.push_str(&fragment);
        }
    }

    flush_markdown_link_buffer(&mut output, &mut active_link, &mut link_buffer);
    output
}

/// Flushes one buffered Markdown link span group into the selectable plain-text output.
/// 将一组缓存中的 Markdown 链接片段刷新到可选纯文本输出中。
pub(super) fn flush_markdown_link_buffer(
    output: &mut String,
    active_link: &mut Option<String>,
    link_buffer: &mut String,
) {
    let Some(link_url) = active_link.take() else {
        return;
    };

    if link_buffer.trim().is_empty() {
        output.push_str(&link_url);
    } else if link_buffer.trim() == link_url {
        output.push_str(link_buffer);
    } else {
        output.push_str(link_buffer);
        output.push_str(" (");
        output.push_str(&link_url);
        output.push(')');
    }

    link_buffer.clear();
}

/// Prefixes one multiline Markdown-derived block while keeping continuation lines aligned with the first content line.
/// 为多行 Markdown 派生区块补上前缀，并让后续续行与首行内容保持对齐。
pub(super) fn prefix_multiline_markdown_block(
    body: &str,
    first_prefix: &str,
    continuation_prefix: &str,
) -> String {
    let mut lines = body.lines();
    let Some(first_line) = lines.next() else {
        return first_prefix.trim_end().to_string();
    };

    let mut output = String::from(first_prefix);
    output.push_str(first_line);

    for line in lines {
        output.push('\n');
        output.push_str(continuation_prefix);
        output.push_str(line);
    }

    output
}

/// Returns the Markdown heading marker that corresponds to one parsed heading level.
/// 返回与某个已解析标题层级对应的 Markdown 标题标记。
pub(super) fn markdown_heading_prefix(level: &markdown::HeadingLevel) -> &'static str {
    match level {
        markdown::HeadingLevel::H1 => "#",
        markdown::HeadingLevel::H2 => "##",
        markdown::HeadingLevel::H3 => "###",
        markdown::HeadingLevel::H4 => "####",
        markdown::HeadingLevel::H5 => "#####",
        markdown::HeadingLevel::H6 => "######",
    }
}

/// Returns the current Unix timestamp in milliseconds as one text value.
/// 以文本值形式返回当前 Unix 毫秒时间戳。
pub(super) fn current_unix_millis_text() -> String {
    current_unix_millis().to_string()
}

/// Returns the current Unix timestamp in milliseconds as one integer value.
/// 以整数值形式返回当前 Unix 毫秒时间戳。
pub(super) fn current_unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_millis()
}

/// Parses one stored Unix-millis text field into an integer timestamp.
/// 将一个已存储的 Unix 毫秒文本字段解析为整数时间戳。
pub(super) fn parse_unix_millis_text(value: &str) -> Option<u128> {
    value.trim().parse::<u128>().ok()
}

/// Builds one stable runtime-event identifier within the current session event stream.
/// 在当前会话事件流内部构建一条稳定运行时事件标识。
pub(super) fn session_runtime_event_id(
    session_id: SessionId,
    event_index: usize,
    event: &RuntimeEvent,
) -> String {
    format!(
        "session-{session_id}-runtime-event-{event_index}-{}-{}",
        runtime_event_kind_label(event.kind),
        slugify_runtime_event_title(&event.title)
    )
}

/// Parses one tool-start runtime headline into its stable tool-call identifier and visible tool name.
/// 将一条工具开始运行时标题解析为稳定工具调用标识及可见工具名称。
pub(super) fn parse_tool_started_runtime_event_title(title: &str) -> Option<(String, String)> {
    let mut segments = title.split(" · ");
    match (segments.next(), segments.next(), segments.next()) {
        (Some("工具调用"), Some(tool_call_id), Some(tool_name))
            if !tool_call_id.trim().is_empty() && !tool_name.trim().is_empty() =>
        {
            Some((
                tool_call_id.trim().to_string(),
                tool_name.trim().to_string(),
            ))
        }
        _ => None,
    }
}

/// Parses one tool-finish runtime headline into its stable tool-call identifier.
/// 将一条工具结束运行时标题解析为稳定工具调用标识。
pub(super) fn parse_tool_finished_runtime_event_title(title: &str) -> Option<String> {
    let mut segments = title.split(" · ");
    match (segments.next(), segments.next()) {
        (Some("工具返回"), Some(tool_call_id)) if !tool_call_id.trim().is_empty() => {
            Some(tool_call_id.trim().to_string())
        }
        _ => None,
    }
}

/// Returns the compact label rendered for one runtime-event kind inside the transcript stream.
/// 返回转录流中某个运行时事件类型渲染使用的精简标签。
pub(super) fn runtime_event_kind_label(kind: RuntimeEventKind) -> &'static str {
    match kind {
        RuntimeEventKind::TokenDelta => "输出",
        RuntimeEventKind::ToolStarted => "工具开始",
        RuntimeEventKind::ToolFinished => "工具完成",
        RuntimeEventKind::DelegateStarted => "委派开始",
        RuntimeEventKind::DelegateFinished => "委派完成",
        RuntimeEventKind::Failed => "失败",
    }
}

/// Builds one compact slug from a runtime-event title for stable in-session identifiers.
/// 根据运行时事件标题构建一条紧凑 slug，用于会话内稳定标识。
pub(super) fn slugify_runtime_event_title(value: &str) -> String {
    let mut slug = String::new();
    let mut last_was_separator = false;

    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
            last_was_separator = false;
        } else if !last_was_separator {
            slug.push('-');
            last_was_separator = true;
        }
    }

    slug.trim_matches('-').chars().take(24).collect()
}

/// Opens one native folder-picker dialog and returns the selected path when the user confirms.
/// 打开一个原生文件夹选择对话框，并在用户确认时返回所选路径。
pub(super) fn pick_task_project_folder(
    initial_directory: &str,
) -> Result<Option<std::path::PathBuf>, String> {
    let mut dialog = rfd::FileDialog::new().set_title("选择要添加到项目树的目录");
    let trimmed_directory = initial_directory.trim();

    if !trimmed_directory.is_empty() {
        let initial_path = Path::new(trimmed_directory);
        if initial_path.exists() && initial_path.is_dir() {
            dialog = dialog.set_directory(initial_path);
        }
    }

    Ok(dialog.pick_folder())
}

/// Normalizes one project path to a canonical form when the platform can resolve it.
/// 当平台能够解析时，将某个项目路径规范化为标准形式。
pub(super) fn normalize_task_project_path(path: &Path) -> std::path::PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Builds one compact ASCII slug from the project title so generated identifiers stay readable.
/// 从项目标题构建紧凑的 ASCII slug，保证生成标识保持可读。
pub(super) fn slugify_task_project_title(title: &str) -> String {
    slugify_identifier(title)
}

/// Builds one compact ASCII slug from an arbitrary identifier source so generated ids stay readable.
/// 从任意标识来源构建紧凑 ASCII slug，保证生成的 id 保持可读。
pub(super) fn slugify_identifier(value: &str) -> String {
    let mut slug = String::new();
    let mut last_was_separator = false;

    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
            last_was_separator = false;
        } else if !last_was_separator {
            slug.push('-');
            last_was_separator = true;
        }
    }

    slug.trim_matches('-').to_string()
}

/// Converts one optional string field into a trimmed `Option<String>`.
/// 将一个可选字符串字段转换为裁剪后的 `Option<String>`。
pub(super) fn trim_to_option(value: &str) -> Option<String> {
    let trimmed = value.trim();

    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Joins MCP server arguments into one compact editable text field.
/// 将 MCP 服务器参数拼接为一段紧凑的可编辑文本字段。
pub(super) fn join_mcp_server_args(args: &[String]) -> String {
    args.join(" | ")
}

/// Parses one compact MCP server argument text field into its vector representation.
/// 将一段紧凑 MCP 服务器参数文本解析成向量表示。
pub(super) fn parse_mcp_server_args(value: &str) -> Vec<String> {
    value
        .split('|')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

/// Joins MCP server environment variables into one compact editable text field.
/// 将 MCP 服务器环境变量拼接为一段紧凑的可编辑文本字段。
pub(super) fn join_mcp_server_env(env: &BTreeMap<String, String>) -> String {
    env.iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("; ")
}

/// Parses one compact MCP server environment text field into its map representation.
/// 将一段紧凑 MCP 服务器环境变量文本解析成映射表示。
pub(super) fn parse_mcp_server_env(value: &str) -> Result<BTreeMap<String, String>, String> {
    parse_key_value_pairs(value, "环境变量", "请使用 `KEY=VALUE` 并以分号分隔。")
}

/// Returns the MCP transport choices exposed by the settings form.
/// 返回设置表单暴露的 MCP 传输类型选项。
pub(super) fn mcp_transport_choices() -> Vec<PersistedMcpTransportKind> {
    vec![
        PersistedMcpTransportKind::Stdio,
        PersistedMcpTransportKind::Http,
        PersistedMcpTransportKind::Sse,
    ]
}

/// Joins MCP server HTTP headers into one compact editable text field.
/// 将 MCP 服务器 HTTP 请求头拼接为一段紧凑的可编辑文本字段。
pub(super) fn join_mcp_server_headers(headers: &BTreeMap<String, String>) -> String {
    headers
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("; ")
}

/// Parses one compact MCP server HTTP headers field into its map representation.
/// 将一段紧凑 MCP 服务器 HTTP 请求头文本解析成映射表示。
pub(super) fn parse_mcp_server_headers(value: &str) -> Result<BTreeMap<String, String>, String> {
    parse_key_value_pairs(value, "请求头", "请使用 `KEY=VALUE` 并以分号分隔。")
}

/// Parses one semicolon-delimited key-value text field into a map.
/// 将一段使用分号分隔的键值对文本解析成映射。
pub(super) fn parse_key_value_pairs(
    value: &str,
    subject_label: &str,
    format_hint: &str,
) -> Result<BTreeMap<String, String>, String> {
    let mut map = BTreeMap::new();

    for pair in value
        .split(';')
        .map(str::trim)
        .filter(|item| !item.is_empty())
    {
        let Some((raw_key, raw_value)) = pair.split_once('=') else {
            return Err(format!("{subject_label} `{pair}` 格式无效，{format_hint}"));
        };
        let key = raw_key.trim();
        if key.is_empty() {
            return Err(format!("{subject_label}名称不能为空。"));
        }
        map.insert(key.to_string(), raw_value.trim().to_string());
    }

    Ok(map)
}

/// Returns a compact human-readable label for one persisted agent kind.
/// 返回单个持久化代理类型的紧凑人类可读标签。
/// Opens one filesystem path with the host operating system file manager.
/// 使用宿主操作系统的文件管理器打开某个文件系统路径。
pub(super) fn open_path_in_system_file_manager(path: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("explorer");
        command.arg(path);
        command
    };

    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg(path);
        command
    };

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(path);
        command
    };

    command
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Returns the native settings used when opening the main workbench window.
/// 返回打开主工作台窗口时使用的原生窗口配置。
pub(super) fn main_window_settings() -> window::Settings {
    window::Settings {
        size: iced::Size::new(1600.0, 960.0),
        position: window::Position::Centered,
        min_size: Some(iced::Size::new(1280.0, 800.0)),
        decorations: false,
        platform_specific: custom_window_platform_settings(),
        ..window::Settings::default()
    }
}

/// Returns the native settings used when opening the dedicated diff preview window.
/// 返回打开独立 Diff 预览窗口时使用的原生窗口配置。
pub(super) fn diff_window_settings() -> window::Settings {
    window::Settings {
        size: iced::Size::new(1320.0, 900.0),
        position: window::Position::Centered,
        min_size: Some(iced::Size::new(960.0, 640.0)),
        decorations: false,
        platform_specific: custom_window_platform_settings(),
        ..window::Settings::default()
    }
}

/// Returns the platform-specific settings used by custom undecorated windows.
/// 返回自定义无系统装饰窗口使用的平台专属配置。
#[cfg(target_os = "windows")]
pub(super) fn custom_window_platform_settings() -> window::settings::PlatformSpecific {
    window::settings::PlatformSpecific {
        undecorated_shadow: true,
        corner_preference: window::settings::platform::CornerPreference::Round,
        ..window::settings::PlatformSpecific::default()
    }
}

/// Returns the platform-specific settings used by custom undecorated windows.
/// 返回自定义无系统装饰窗口使用的平台专属配置。
#[cfg(not(target_os = "windows"))]
pub(super) fn custom_window_platform_settings() -> window::settings::PlatformSpecific {
    window::settings::PlatformSpecific::default()
}

/// Renders a tooltip-enabled secondary action button.
/// 渲染带提示信息的次级动作按钮。
pub(super) fn action_button<'a>(
    label: &'a str,
    hint: &'a str,
    on_press: Message,
) -> Element<'a, Message> {
    tooltip(
        button(label).padding([10, 14]).on_press(on_press),
        hint,
        tooltip::Position::Bottom,
    )
    .style(container::rounded_box)
    .into()
}

/// Loads a Git diff preview from the target workspace for the temporary testing region.
/// 从目标工作区载入 Git diff 预览，供临时测试区域展示。
pub(super) fn load_workspace_diff_preview(workspace_path: &str) -> DiffPreviewPayload {
    // Normalize user input early so command execution and diagnostics share the same target string.
    // 尽早规范化用户输入，保证命令执行与诊断信息使用同一个目标路径字符串。
    let trimmed_path = workspace_path.trim();

    if trimmed_path.is_empty() {
        let panel = WorkspaceDiffPanel::with_notice(
            String::new(),
            "未提供工作区路径。",
            "请输入一个 Git 仓库目录，然后重新点击扫描工作区。",
        );

        return DiffPreviewPayload {
            status_message: String::from("扫描失败：工作区路径为空。"),
            summary: panel.summary_text(),
            panel,
            changed_files: 0,
            activity_level: ActivityLevel::Warning,
        };
    }

    let path = Path::new(trimmed_path);

    if !path.exists() {
        let panel = WorkspaceDiffPanel::with_notice(
            String::new(),
            "目标路径不存在。",
            format!("无法载入 Diff，因为目录不存在：{trimmed_path}"),
        );

        return DiffPreviewPayload {
            status_message: format!("扫描失败：路径不存在 - {trimmed_path}。"),
            summary: panel.summary_text(),
            panel,
            changed_files: 0,
            activity_level: ActivityLevel::Warning,
        };
    }

    if !path.is_dir() {
        let panel = WorkspaceDiffPanel::with_notice(
            String::new(),
            "目标路径不是目录。",
            format!("无法载入 Diff，因为目标不是目录：{trimmed_path}"),
        );

        return DiffPreviewPayload {
            status_message: format!("扫描失败：目标不是目录 - {trimmed_path}。"),
            summary: panel.summary_text(),
            panel,
            changed_files: 0,
            activity_level: ActivityLevel::Warning,
        };
    }

    // Verify the directory is inside a Git repository before attempting to render any diff preview.
    // 在渲染任何 diff 预览前先验证该目录位于 Git 仓库中。
    let repository_root = match run_git_command(path, &["rev-parse", "--show-toplevel"]) {
        Ok(output) => output.trim().to_string(),
        Err(error) => {
            let panel = WorkspaceDiffPanel::with_notice(
                String::new(),
                "Git 仓库校验失败。",
                format!("无法载入 Git Diff。\n\n目标路径：{trimmed_path}\n\n诊断信息：\n{error}"),
            );

            return DiffPreviewPayload {
                status_message: format!("扫描失败：{trimmed_path} 不是有效的 Git 仓库。"),
                summary: panel.summary_text(),
                panel,
                changed_files: 0,
                activity_level: ActivityLevel::Warning,
            };
        }
    };

    // Capture the tracked diff body first, then append synthesized untracked-file diffs for a fuller review view.
    // 先获取已跟踪差异，再拼接未跟踪文件的合成 diff，以得到更完整的审查视图。
    let tracked_diff_output = match run_git_command(
        path,
        &[
            "diff",
            "--no-ext-diff",
            "--no-color",
            "--patch",
            "--unified=3",
        ],
    ) {
        Ok(output) => output,
        Err(error) => {
            let panel = WorkspaceDiffPanel::with_notice(
                repository_root.clone(),
                "Git Diff 生成失败。",
                format!("Git 命令执行失败。\n\n目标路径：{trimmed_path}\n\n诊断信息：\n{error}"),
            );

            return DiffPreviewPayload {
                status_message: format!("扫描失败：无法为 {trimmed_path} 生成 Git Diff。"),
                summary: panel.summary_text(),
                panel,
                changed_files: 0,
                activity_level: ActivityLevel::Warning,
            };
        }
    };

    let (untracked_diff_output, untracked_file_count) = match load_untracked_file_diffs(Path::new(
        &repository_root,
    )) {
        Ok(payload) => payload,
        Err(error) => {
            let panel = WorkspaceDiffPanel::with_notice(
                repository_root.clone(),
                "未跟踪文件 Diff 生成失败。",
                format!(
                    "已跟踪文件 Diff 已可用，但未跟踪文件的补充 Diff 生成失败。\n\n目标路径：{trimmed_path}\n\n诊断信息：\n{error}"
                ),
            );

            return DiffPreviewPayload {
                status_message: format!("扫描失败：无法补充 {trimmed_path} 中的新增文件 Diff。"),
                summary: panel.summary_text(),
                panel,
                changed_files: 0,
                activity_level: ActivityLevel::Warning,
            };
        }
    };

    let diff_output = combine_diff_sections(&tracked_diff_output, &untracked_diff_output);

    let diff_names = run_git_command(
        path,
        &["diff", "--no-ext-diff", "--no-color", "--name-only"],
    )
    .unwrap_or_default();
    let status_output = run_git_command(path, &["status", "--short"]).unwrap_or_default();

    let tracked_changed_files = diff_names
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count();
    let workspace_status_entries = status_output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count();
    let changed_files = tracked_changed_files + untracked_file_count;

    if !diff_output.trim().is_empty() {
        let panel = WorkspaceDiffPanel::from_unified_diff(repository_root.clone(), &diff_output);

        return DiffPreviewPayload {
            status_message: format!("已载入 Git Diff：{trimmed_path}。"),
            summary: panel.summary_text(),
            panel,
            changed_files,
            activity_level: ActivityLevel::Success,
        };
    }

    // Explain why an empty `git diff` can still happen when untracked files exist in the workspace.
    // 解释为什么在工作区存在未跟踪文件时，`git diff` 仍可能为空。
    let empty_diff_content = if workspace_status_entries > 0 {
        format!(
            "当前 `git diff` 没有可展示的受跟踪改动，但工作区仍然存在状态项。\n\n\
这通常意味着当前只有未跟踪文件，或者变更尚未进入常规 diff 范围。\n\n\
当前 `git status --short` 结果如下：\n{status_output}"
        )
    } else {
        String::from("当前工作区没有未提交差异。")
    };

    let panel = WorkspaceDiffPanel::with_notice(
        repository_root.clone(),
        "当前无可展示的跟踪差异。",
        empty_diff_content,
    );

    DiffPreviewPayload {
        status_message: format!("已扫描工作区：{trimmed_path}。"),
        summary: panel.summary_text(),
        panel,
        changed_files,
        activity_level: ActivityLevel::Info,
    }
}

/// Executes a Git command in the target workspace and returns its UTF-8-decoded stdout.
/// 在目标工作区中执行 Git 命令，并返回按 UTF-8 解码后的标准输出。
pub(super) fn run_git_command(path: &Path, arguments: &[&str]) -> Result<String, String> {
    // Execute Git without involving a shell so paths containing spaces remain safe and predictable.
    // 直接执行 Git 而不经过 shell，保证包含空格的路径依然安全且可预测。
    let output = Command::new("git")
        .args(["-c", "core.quotepath=false"])
        .args(["-c", "i18n.logOutputEncoding=utf-8"])
        .arg("-C")
        .arg(path)
        .args(arguments)
        .output()
        .map_err(|error| format!("无法启动 git 命令：{error}"))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let diagnostic = if !stderr.is_empty() { stderr } else { stdout };

        Err(if diagnostic.is_empty() {
            format!(
                "git {:?} 执行失败，退出码为 {:?}。",
                arguments,
                output.status.code()
            )
        } else {
            diagnostic
        })
    }
}

/// Collects synthesized unified diffs for untracked files so new files appear in the review panel.
/// 收集未跟踪文件的合成 unified diff，使新增文件也能出现在审查面板中。
pub(super) fn load_untracked_file_diffs(repository_root: &Path) -> Result<(String, usize), String> {
    let untracked_files = run_git_command(
        repository_root,
        &["ls-files", "--others", "--exclude-standard"],
    )
    .unwrap_or_default();

    let mut collected_diffs = Vec::new();
    let mut file_count = 0;

    for relative_path in untracked_files
        .lines()
        .map(str::trim)
        .filter(|path| !path.is_empty())
    {
        collected_diffs.push(build_new_file_diff(repository_root, relative_path)?);
        file_count += 1;
    }

    Ok((collected_diffs.join("\n"), file_count))
}

/// Builds a synthetic unified diff for one untracked file.
/// 为单个未跟踪文件构建合成的 unified diff。
pub(super) fn build_new_file_diff(
    repository_root: &Path,
    relative_path: &str,
) -> Result<String, String> {
    let file_path = repository_root.join(relative_path);
    let display_path = relative_path.replace('\\', "/");
    let raw_bytes = fs::read(&file_path)
        .map_err(|error| format!("无法读取新增文件 `{display_path}`：{error}"))?;

    let mut diff_lines = vec![
        format!("diff --git a/{display_path} b/{display_path}"),
        String::from("new file mode 100644"),
        String::from("--- /dev/null"),
        format!("+++ b/{display_path}"),
    ];

    match String::from_utf8(raw_bytes) {
        Ok(content) => {
            let normalized_content = content.replace("\r\n", "\n");
            let content_lines = normalized_content.lines().collect::<Vec<_>>();

            if !content_lines.is_empty() {
                diff_lines.push(format!("@@ -0,0 +1,{} @@", content_lines.len()));
                diff_lines.extend(content_lines.into_iter().map(|line| format!("+{line}")));

                if !normalized_content.ends_with('\n') {
                    diff_lines.push(String::from("\\ No newline at end of file"));
                }
            }
        }
        Err(_) => {
            diff_lines.push(format!(
                "Binary files /dev/null and b/{display_path} differ"
            ));
        }
    }

    Ok(diff_lines.join("\n"))
}

/// Combines tracked and untracked diff sections into one unified payload.
/// 将已跟踪与未跟踪的 diff 片段合并为一个统一载荷。
pub(super) fn combine_diff_sections(
    tracked_diff_output: &str,
    untracked_diff_output: &str,
) -> String {
    match (
        tracked_diff_output.trim().is_empty(),
        untracked_diff_output.trim().is_empty(),
    ) {
        (true, true) => String::new(),
        (false, true) => tracked_diff_output.to_string(),
        (true, false) => untracked_diff_output.to_string(),
        (false, false) => format!("{tracked_diff_output}\n{untracked_diff_output}"),
    }
}

/// Rebuilds wrapped diff-hunk payloads for the current wrap-column estimate.
/// 根据当前折行列数估算结果重建 Diff hunk 折行载荷。
pub(super) fn rebuild_wrapped_diff_hunks(
    panel: &WorkspaceDiffPanel,
    wrap_columns: usize,
) -> BTreeMap<String, code_view::WrappedDiffHunk> {
    let mut wrapped_hunks = BTreeMap::new();

    for file in &panel.files {
        let file_key = file.key();

        for (hunk_index, hunk) in file.hunks.iter().enumerate() {
            wrapped_hunks.insert(
                diff_hunk_buffer_key(&file_key, hunk_index),
                code_view::wrap_diff_hunk(hunk, wrap_columns),
            );
        }
    }

    wrapped_hunks
}

/// Builds one read-only text editor buffer from a virtualized wrapped-hunk slice.
/// 根据虚拟化后的折行 hunk 切片构建只读文本编辑器缓冲区。
pub(super) fn build_diff_slice_buffer(
    wrapped_hunk: &code_view::WrappedDiffHunk,
    slice_range: DiffVisualSliceRange,
) -> text_editor::Content {
    let sliced_text = wrapped_hunk.text_lines[slice_range.start..slice_range.end].join("\n");

    text_editor::Content::with_text(&sliced_text)
}

/// Formats merged diff line numbers for the shared gutter.
/// 为共享 gutter 格式化合并后的 diff 行号。
pub(super) fn format_diff_line_number(
    old_line_number: Option<usize>,
    new_line_number: Option<usize>,
) -> String {
    old_line_number
        .or(new_line_number)
        .map(|value| value.to_string())
        .unwrap_or_default()
}

/// Resolves the zero-based hovered visual line index from one local cursor Y position.
/// 根据局部鼠标 Y 坐标解析从零开始的悬停可视行索引。
pub(super) fn hovered_diff_line_index_from_cursor(local_y: f32, line_count: usize) -> usize {
    let raw_index = (local_y / CODE_EDITOR_LINE_HEIGHT).floor().max(0.0) as usize;

    raw_index.min(line_count.saturating_sub(1))
}

/// Builds the stable buffer key for one hunk editor inside a diff file.
/// 为 diff 文件中的单个 hunk 编辑器构建稳定缓冲区键。
pub(super) fn diff_hunk_buffer_key(file_key: &str, hunk_index: usize) -> String {
    format!("{file_key}::hunk::{hunk_index}")
}

/// Returns the total leading gutter width reserved before one diff code editor block.
/// 返回单个 diff 代码编辑器区块前方预留的总 gutter 宽度。
pub(super) fn diff_gutter_block_width() -> f32 {
    DIFF_GUTTER_ARROW_WIDTH + DIFF_GUTTER_WIDTH + 5.0
}

/// Estimates the omitted unchanged line count between two adjacent hunks.
/// 估算两个相邻 hunk 之间被省略的未变化行数量。
pub(super) fn omitted_context_line_count(
    previous_hunk: &crate::diff::DiffHunk,
    next_hunk: &crate::diff::DiffHunk,
) -> usize {
    let previous_end = previous_hunk
        .lines
        .iter()
        .rev()
        .find_map(preferred_display_line_number);
    let next_start = next_hunk
        .lines
        .iter()
        .find_map(preferred_display_line_number);

    match (previous_end, next_start) {
        (Some(previous_end), Some(next_start)) if next_start > previous_end => {
            next_start.saturating_sub(previous_end + 1)
        }
        _ => 0,
    }
}

/// Picks the most useful visual line number for omission calculations.
/// 为省略行计算选择最有用的可视行号。
pub(super) fn preferred_display_line_number(line: &crate::diff::DiffLine) -> Option<usize> {
    line.new_number.or(line.old_number)
}
