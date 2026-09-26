//! Side-effect-free whole-file write counters; content is never retained.
//! 无副作用的整文件写入计数；绝不保留正文。

use serde_json::{Value, json};
use std::{cell::Cell, sync::Arc};
use vulcan_tools_manager::{
    IncrementalJsonEvent, IncrementalJsonPathSegment, ToolRequestParserContext,
    ToolRequestParserFactory, ToolRequestParserSession, ToolRequestPreview,
    ToolRequestProgressPolicy, ToolRequestSensitivityPolicy, ToolRequestStagingPolicy,
    ToolRequestStreamingCapability,
};

/// Maximum path bytes retained solely for a visible write preview.
/// 仅为可见写入预览保留的最大路径字节数。
const PREVIEW_PATH_BYTES: usize = 4096;

/// Returns the declared preview capability, without staging files or storing content.
/// 返回声明的预览能力，不暂存文件或保存正文。
pub(super) fn capability() -> ToolRequestStreamingCapability {
    ToolRequestStreamingCapability {
        parser_factory: Some(Arc::new(WritePreviewFactory)),
        preview_kind: Some("file_write".into()),
        staging_policy: ToolRequestStagingPolicy::Forbidden,
        sensitivity_policy: ToolRequestSensitivityPolicy {
            allowed_preview_fields: vec!["path".into(), "new_line_count".into()],
            ..Default::default()
        },
        progress_policy: ToolRequestProgressPolicy::default(),
    }
}

/// Creates isolated request parsers with no filesystem access.
/// 创建无文件系统访问的隔离请求解析器。
struct WritePreviewFactory;
impl ToolRequestParserFactory for WritePreviewFactory {
    /// Ignores execution context; returns a fresh preview-only parser.
    /// 忽略执行上下文；返回全新的仅预览解析器。
    fn create(&self, _: ToolRequestParserContext) -> Box<dyn ToolRequestParserSession> {
        Box::new(WritePreview::default())
    }
}

/// Tracks only approved header strings and constant-space decoded line counts.
/// 仅跟踪获准的头部字符串及恒定空间的已解码行计数。
#[derive(Default)]
struct WritePreview {
    /// Completed action and path; missing or oversized paths omit only the preview title.
    /// 已完成动作及路径；缺失或超限路径仅省略预览标题。
    action: String,
    path: String,
    /// Current bounded header string and whether it exceeded the display bound.
    /// 当前有界头部字符串及其是否超过显示限制。
    header: String,
    overflow: bool,
    /// Logical newlines, trailing text, and a possible paired CR/LF across fragments.
    /// 逻辑换行、尾部正文及可能跨碎片配对的 CR/LF。
    lines: u64,
    tail: bool,
    previous_break: Option<char>,
    /// Snapshot revision and pending semantic changes.
    /// 快照修订及待发布语义变化。
    revision: Cell<u64>,
    dirty: Cell<bool>,
}

impl WritePreview {
    /// Consumes decoded content in constant space; returns nothing and never writes a file.
    /// 以恒定空间消费已解码正文；不返回值且绝不写文件。
    fn count(&mut self, content: &str) {
        for character in content.chars() {
            if matches!(character, '\r' | '\n') {
                if self
                    .previous_break
                    .is_some_and(|previous| previous != character)
                {
                    self.previous_break = None;
                } else {
                    self.lines += 1;
                    self.previous_break = Some(character);
                }
                self.tail = false;
            } else {
                self.previous_break = None;
                self.tail = true;
            }
        }
        self.dirty.set(true);
    }
}

impl ToolRequestParserSession for WritePreview {
    /// Accepts exact top-level schema fields; returns success without touching execution arguments.
    /// 接受精确顶层 Schema 字段；返回成功且不触碰执行参数。
    fn push_json_event(&mut self, event: &IncrementalJsonEvent) -> Result<(), String> {
        match event {
            IncrementalJsonEvent::StringStarted { path } => match field(path) {
                Some("action" | "path") => {
                    self.header.clear();
                    self.overflow = false;
                }
                Some("content") => {
                    self.lines = 0;
                    self.tail = false;
                    self.previous_break = None;
                }
                _ => {}
            },
            IncrementalJsonEvent::StringContent { path, content } => match field(path) {
                Some("action" | "path") => {
                    if self.header.len().saturating_add(content.len()) > PREVIEW_PATH_BYTES {
                        self.overflow = true;
                    } else if !self.overflow {
                        self.header.push_str(content);
                    }
                }
                Some("content") => self.count(content),
                _ => {}
            },
            IncrementalJsonEvent::StringEnded { path } => {
                match field(path) {
                    Some("action") => {
                        self.action = if self.overflow {
                            String::new()
                        } else {
                            self.header.clone()
                        }
                    }
                    Some("path") => {
                        self.path = if self.overflow {
                            String::new()
                        } else {
                            self.header.clone()
                        }
                    }
                    _ => {}
                }
                self.dirty.set(true);
            }
            _ => {}
        }
        Ok(())
    }

    /// Returns approved path and exact decoded line count after a semantic change; otherwise none.
    /// 语义变化后返回获准路径及精确已解码行数；否则返回空。
    fn snapshot(&self) -> Option<ToolRequestPreview> {
        if !self.dirty.replace(false) || self.action != "write" {
            return None;
        }
        self.revision.set(self.revision.get().saturating_add(1));
        Some(ToolRequestPreview {
            kind: "file_write".into(),
            revision: self.revision.get(),
            // JSON members are unordered: content counters must not wait for a trailing path.
            // JSON 成员无序：正文计数不能等待末尾才到达的路径。
            title_fields: (!self.path.is_empty())
                .then(|| ("path".into(), json!(self.path)))
                .into_iter()
                .collect(),
            progress_fields: [(
                "new_line_count".into(),
                json!(self.lines + u64::from(self.tail)),
            )]
            .into_iter()
            .collect(),
            ..Default::default()
        })
    }

    /// Recounts final schema fields without retaining content; returns the final preview.
    /// 重数最终 Schema 字段但不保留正文；返回最终预览。
    fn finish(&mut self, arguments: &Value) -> Result<Option<ToolRequestPreview>, String> {
        self.action = arguments
            .get("action")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .into();
        self.path = arguments
            .get("path")
            .and_then(Value::as_str)
            .filter(|path| path.len() <= PREVIEW_PATH_BYTES)
            .unwrap_or_default()
            .into();
        self.lines = 0;
        self.tail = false;
        self.previous_break = None;
        self.count(
            arguments
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        );
        Ok(self.snapshot())
    }

    /// Drops all preview state on abort; returns nothing.
    /// 中止时丢弃全部预览状态；不返回值。
    fn abort(&mut self) {
        *self = Self::default();
    }
}

/// Resolves only a declared top-level key from a lexer path, returning none for nested input.
/// 仅从词法路径解析顶层键，嵌套输入返回空。
fn field(path: &[IncrementalJsonPathSegment]) -> Option<&str> {
    match path {
        [IncrementalJsonPathSegment::Key(key)] => Some(key),
        _ => None,
    }
}
