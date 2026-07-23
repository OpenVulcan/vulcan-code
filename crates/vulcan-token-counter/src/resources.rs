//! Local tokenizer resource loading and backend panic containment.
//! 本地 tokenizer 资源加载与后端 panic 隔离。

use std::{
    any::Any,
    panic::{self, AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

use tokie::Tokenizer as TokieTokenizer;
use wordchipper::{
    TokenEncoder, Tokenizer as WordchipperTokenizer, TokenizerOptions, WCError, load_vocab,
    support::resources::{KeyedResource, ResourceLoader},
};

use crate::types::{OpenAiEncoding, TokenCounterError, TokenizerBackend};

/// Global guard used while suppressing tokie's noisy internal panic hook output.
/// 在抑制 tokie 内部 panic hook 噪声输出时使用的全局保护锁。
static TOKIE_COUNT_PANIC_HOOK_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// Byte length at which tokie 0.0.9 enters its UTF-8-unsafe parallel encoder.
/// tokie 0.0.9 进入非 UTF-8 安全并行编码器时的字节长度。
const TOKIE_UNSAFE_PARALLEL_THRESHOLD_BYTES: usize = 10_000;

/// Loaded tokenizer implementation for one OpenAI base encoding.
/// 单个 OpenAI 基础编码已加载的 tokenizer 实现。
pub(super) enum LoadedOpenAiTokenizer {
    /// `wordchipper` tokenizer loaded from a local `.tiktoken` resource.
    /// 从本地 `.tiktoken` 资源加载的 `wordchipper` tokenizer。
    Wordchipper(Arc<WordchipperTokenizer<u32>>),
    /// `tokie` tokenizer loaded from a local `.tkz` resource.
    /// 从本地 `.tkz` 资源加载的 `tokie` tokenizer。
    Tokie(Box<TokieTokenizer>),
}

impl LoadedOpenAiTokenizer {
    /// Counts plain text tokens with the loaded backend.
    /// 使用已加载后端统计纯文本 Token。
    ///
    /// # Parameters
    /// 参数
    /// `backend` identifies the active tokenizer backend for diagnostics.
    /// `backend` 标识用于诊断的当前 tokenizer 后端。
    /// `encoding` identifies the OpenAI base encoding for diagnostics.
    /// `encoding` 标识用于诊断的 OpenAI 基础编码。
    /// `text` is the plain text to count.
    /// `text` 是需要统计的纯文本。
    ///
    /// # Returns
    /// 返回值
    /// The backend token count or a normalized counter error.
    /// 后端 Token 数或规范化统计错误。
    pub(super) fn count(
        &self,
        backend: TokenizerBackend,
        encoding: OpenAiEncoding,
        text: &str,
    ) -> Result<usize, TokenCounterError> {
        match self {
            Self::Wordchipper(tokenizer) => tokenizer
                .try_encode(text, None)
                .map(|tokens| tokens.len())
                .map_err(|error| TokenCounterError::NativeOpenAi {
                    backend,
                    encoding,
                    message: error.to_string(),
                }),
            Self::Tokie(tokenizer) => safe_count_tokie_tokens(tokenizer, text).map_err(|message| {
                TokenCounterError::NativeOpenAi {
                    backend,
                    encoding,
                    message,
                }
            }),
        }
    }
}

/// Counts text with `tokie` while containing tokenizer-internal panics.
/// 在隔离 tokenizer 内部 panic 的情况下使用 `tokie` 统计文本。
///
/// # Parameters
/// 参数
/// `tokenizer` is the already loaded `tokie` tokenizer.
/// `tokenizer` 是已经加载完成的 `tokie` tokenizer。
/// `text` is the plain text payload to count.
/// `text` 是需要统计的纯文本载荷。
///
/// # Returns
/// 返回值
/// The token count, or a diagnostic string when `tokie` panics during counting.
/// Token 数；当 `tokie` 在统计期间 panic 时返回诊断字符串。
pub(super) fn safe_count_tokie_tokens(
    tokenizer: &TokieTokenizer,
    text: &str,
) -> Result<usize, String> {
    // Tokie 0.0.9 splits long input at raw byte offsets and then constructs unchecked UTF-8
    // slices. Keep every call below that branch and preserve the explicit Tokie backend.
    // Tokie 0.0.9 会按原始字节偏移切分长输入，再构造未经检查的 UTF-8 切片。
    // 让每次调用都低于该分支阈值，同时保留显式 Tokie 后端。
    run_tokie_count_with_quiet_panic_hook(|| {
        tokie_safe_text_segments(text)
            .into_iter()
            .map(|segment| tokenizer.count_tokens(segment))
            .sum()
    })
    .map_err(|payload| {
        format!(
            "tokie tokenizer panicked: {}",
            panic_payload_message(&payload)
        )
    })
}

/// Splits text below tokie's unsafe parallel threshold without cutting a UTF-8 code point.
/// 在不切断 UTF-8 码点的前提下，将文本拆分到 Tokie 非安全并行阈值以下。
///
/// Parameters: `text` is one complete Unicode token-counting payload.
/// Returns: ordered non-overlapping segments that reconstruct the exact input.
/// 参数：`text` 是一段完整的 Unicode Token 统计载荷。
/// 返回值：按顺序排列且互不重叠、可精确还原输入的分段。
fn tokie_safe_text_segments(text: &str) -> Vec<&str> {
    if text.len() < TOKIE_UNSAFE_PARALLEL_THRESHOLD_BYTES {
        return vec![text];
    }

    let maximum_segment_bytes = TOKIE_UNSAFE_PARALLEL_THRESHOLD_BYTES - 1;
    let mut segments = Vec::with_capacity(text.len().div_ceil(maximum_segment_bytes));
    let mut segment_start = 0;
    while segment_start < text.len() {
        let mut segment_end = (segment_start + maximum_segment_bytes).min(text.len());
        while !text.is_char_boundary(segment_end) {
            segment_end -= 1;
        }
        if segment_end < text.len()
            && let Some((whitespace_offset, whitespace)) = text[segment_start..segment_end]
                .char_indices()
                .rev()
                .find(|(_, character)| character.is_whitespace())
        {
            let whitespace_end = segment_start + whitespace_offset + whitespace.len_utf8();
            if whitespace_end > segment_start {
                segment_end = whitespace_end;
            }
        }
        segments.push(&text[segment_start..segment_end]);
        segment_start = segment_end;
    }
    segments
}

/// Runs one tokie count closure while hiding tokenizer-internal panic-hook noise.
/// 在隐藏 tokenizer 内部 panic hook 噪声的同时运行单次 tokie 统计闭包。
///
/// # Parameters
/// 参数
/// `count` is the exact tokenizer operation that may panic inside tokie or pretokie.
/// `count` 是可能在 tokie 或 pretokie 内部 panic 的精确 tokenizer 操作。
///
/// # Returns
/// 返回值
/// The closure result, or the captured panic payload when counting fails.
/// 返回闭包结果；统计失败时返回捕获到的 panic 载荷。
fn run_tokie_count_with_quiet_panic_hook<F>(count: F) -> Result<usize, Box<dyn Any + Send>>
where
    F: FnOnce() -> usize,
{
    let lock = TOKIE_COUNT_PANIC_HOOK_LOCK.get_or_init(|| Mutex::new(()));
    let _guard = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let previous_hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));
    let result = catch_unwind(AssertUnwindSafe(count));
    panic::set_hook(previous_hook);
    result
}

/// Converts one panic payload into a compact diagnostic string.
/// 将单个 panic 载荷转换为紧凑诊断字符串。
///
/// # Parameters
/// 参数
/// `payload` is the panic payload captured by `catch_unwind`.
/// `payload` 是 `catch_unwind` 捕获到的 panic 载荷。
///
/// # Returns
/// 返回值
/// A readable panic message when available, otherwise a stable fallback label.
/// 可读 panic 消息；无法读取时返回稳定回退标签。
fn panic_payload_message(payload: &(dyn Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        return (*message).to_string();
    }
    if let Some(message) = payload.downcast_ref::<String>() {
        return message.clone();
    }
    String::from("unknown panic payload")
}

/// Validates that one OpenAI backend resource file exists.
/// 校验单个 OpenAI 后端资源文件存在。
///
/// # Parameters
/// 参数
/// `backend` selects the required resource extension.
/// `backend` 选择必需的资源扩展名。
/// `encoding` selects the OpenAI resource file.
/// `encoding` 选择 OpenAI 资源文件。
/// `resource_root` points to the directory containing tokenizer files.
/// `resource_root` 指向包含 tokenizer 文件的目录。
///
/// # Returns
/// 返回值
/// `Ok(())` when the resource file exists.
/// 当资源文件存在时返回 `Ok(())`。
pub(super) fn validate_openai_resource(
    backend: TokenizerBackend,
    encoding: OpenAiEncoding,
    resource_root: &Path,
) -> Result<(), TokenCounterError> {
    let path = openai_resource_path(backend, encoding, resource_root);
    if path.is_file() {
        Ok(())
    } else {
        Err(TokenCounterError::MissingOpenAiResource {
            backend,
            encoding,
            path,
        })
    }
}

/// Builds the resource path for one backend and OpenAI encoding.
/// 为单个后端与 OpenAI 编码构建资源路径。
///
/// # Parameters
/// 参数
/// `backend` selects the resource extension.
/// `backend` 选择资源扩展名。
/// `encoding` selects the resource file stem.
/// `encoding` 选择资源文件名主体。
/// `resource_root` points to the directory containing tokenizer files.
/// `resource_root` 指向包含 tokenizer 文件的目录。
///
/// # Returns
/// 返回值
/// The expected local resource path.
/// 预期的本地资源路径。
fn openai_resource_path(
    backend: TokenizerBackend,
    encoding: OpenAiEncoding,
    resource_root: &Path,
) -> PathBuf {
    resource_root.join(format!(
        "{}.{}",
        encoding.file_stem(),
        backend.resource_extension()
    ))
}

/// Loads one OpenAI tokenizer through the selected backend.
/// 通过所选后端加载单个 OpenAI tokenizer。
///
/// # Parameters
/// 参数
/// `backend` selects the tokenizer implementation.
/// `backend` 选择 tokenizer 实现。
/// `encoding` selects the OpenAI vocabulary to load.
/// `encoding` 选择需要加载的 OpenAI 词表。
/// `resource_root` points to the directory containing local tokenizer files.
/// `resource_root` 指向包含本地 tokenizer 文件的目录。
///
/// # Returns
/// 返回值
/// A shared loaded tokenizer instance, or a stringified loading error.
/// 一个共享已加载 tokenizer 实例，或字符串化加载错误。
pub(super) fn load_openai_tokenizer(
    backend: TokenizerBackend,
    encoding: OpenAiEncoding,
    resource_root: &Path,
) -> Result<Arc<LoadedOpenAiTokenizer>, String> {
    match backend {
        TokenizerBackend::Wordchipper => load_openai_wordchipper_tokenizer(encoding, resource_root),
        TokenizerBackend::Tokie => load_openai_tokie_tokenizer(encoding, resource_root),
    }
}

/// Loads one OpenAI tokenizer through `wordchipper` and configures it for shared counting.
/// 通过 `wordchipper` 加载单个 OpenAI tokenizer，并配置为共享统计使用。
///
/// # Parameters
/// 参数
/// `encoding` selects the OpenAI vocabulary to load.
/// `encoding` 选择需要加载的 OpenAI 词表。
/// `resource_root` points to the directory containing local `.tiktoken` files.
/// `resource_root` 指向包含本地 `.tiktoken` 文件的目录。
///
/// # Returns
/// 返回值
/// A shared tokenizer instance, or a stringified loading error.
/// 一个共享 tokenizer 实例，或字符串化加载错误。
fn load_openai_wordchipper_tokenizer(
    encoding: OpenAiEncoding,
    resource_root: &Path,
) -> Result<Arc<LoadedOpenAiTokenizer>, String> {
    let mut resource_loader = LocalOpenAiResourceLoader::new(resource_root.to_path_buf());
    let loaded = load_vocab(encoding.wordchipper_vocab_name(), &mut resource_loader)
        .map_err(|error| error.to_string())?;
    let tokenizer = TokenizerOptions::default().build(loaded.vocab().clone());

    Ok(Arc::new(LoadedOpenAiTokenizer::Wordchipper(tokenizer)))
}

/// Loads one OpenAI tokenizer through `tokie` from a local `.tkz` file.
/// 通过 `tokie` 从本地 `.tkz` 文件加载单个 OpenAI tokenizer。
///
/// # Parameters
/// 参数
/// `encoding` selects the OpenAI vocabulary to load.
/// `encoding` 选择需要加载的 OpenAI 词表。
/// `resource_root` points to the directory containing local `.tkz` files.
/// `resource_root` 指向包含本地 `.tkz` 文件的目录。
///
/// # Returns
/// 返回值
/// A shared tokenizer instance, or a stringified loading error.
/// 一个共享 tokenizer 实例，或字符串化加载错误。
fn load_openai_tokie_tokenizer(
    encoding: OpenAiEncoding,
    resource_root: &Path,
) -> Result<Arc<LoadedOpenAiTokenizer>, String> {
    let path = openai_resource_path(TokenizerBackend::Tokie, encoding, resource_root);
    let tokenizer = TokieTokenizer::from_file(&path).map_err(|error| error.to_string())?;

    Ok(Arc::new(LoadedOpenAiTokenizer::Tokie(Box::new(tokenizer))))
}

/// Local resource loader that keeps `wordchipper` OpenAI vocab loading fully offline.
/// 让 `wordchipper` OpenAI 词表加载保持完全离线的本地资源加载器。
struct LocalOpenAiResourceLoader {
    /// Directory that contains bundled OpenAI `.tiktoken` files.
    /// 包含内置 OpenAI `.tiktoken` 文件的目录。
    resource_root: PathBuf,
}

impl LocalOpenAiResourceLoader {
    /// Creates a local OpenAI resource loader.
    /// 创建本地 OpenAI 资源加载器。
    ///
    /// # Parameters
    /// 参数
    /// `resource_root` points to the directory containing `.tiktoken` files.
    /// `resource_root` 指向包含 `.tiktoken` 文件的目录。
    ///
    /// # Returns
    /// 返回值
    /// A loader that maps `wordchipper` keyed resources to local files.
    /// 一个将 `wordchipper` keyed resource 映射到本地文件的加载器。
    fn new(resource_root: PathBuf) -> Self {
        Self { resource_root }
    }
}

impl ResourceLoader for LocalOpenAiResourceLoader {
    /// Resolves one `wordchipper` resource key to a bundled OpenAI file path.
    /// 将单个 `wordchipper` 资源键解析到内置 OpenAI 文件路径。
    ///
    /// # Parameters
    /// 参数
    /// `resource` is the keyed resource requested by `wordchipper`.
    /// `resource` 是 `wordchipper` 请求的 keyed resource。
    ///
    /// # Returns
    /// 返回值
    /// A local path to the bundled tokenizer resource.
    /// 指向内置 tokenizer 资源的本地路径。
    fn load_resource_path(&mut self, resource: &KeyedResource) -> wordchipper::WCResult<PathBuf> {
        let resource_name = resource
            .key
            .last()
            .map(String::as_str)
            .unwrap_or("unknown-openai-tokenizer");
        let path = self.resource_root.join(format!("{resource_name}.tiktoken"));

        if path.is_file() {
            Ok(path)
        } else {
            Err(WCError::ResourceNotFound(format!(
                "missing local OpenAI tokenizer resource: {}",
                path.display()
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        panic,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use tokie::Tokenizer as TokieTokenizer;

    use crate::{TokenCounter, TokenEstimatePrecision, counter::default_tokenizer_resource_dir};

    use super::{
        TOKIE_UNSAFE_PARALLEL_THRESHOLD_BYTES, run_tokie_count_with_quiet_panic_hook,
        safe_count_tokie_tokens, tokie_safe_text_segments,
    };

    /// Verifies tokie panic containment suppresses hook output and restores the previous hook.
    /// 验证 tokie panic 隔离会抑制 hook 输出并恢复先前的 hook。
    #[test]
    fn tokie_quiet_panic_hook_suppresses_and_restores_hook() {
        let observed = Arc::new(AtomicUsize::new(0));
        let previous_hook = panic::take_hook();
        let hook_observed = Arc::clone(&observed);
        panic::set_hook(Box::new(move |_| {
            hook_observed.fetch_add(1, Ordering::SeqCst);
        }));

        let result = run_tokie_count_with_quiet_panic_hook(|| {
            panic!("hidden tokie panic");
        });
        assert!(result.is_err());
        assert_eq!(observed.load(Ordering::SeqCst), 0);

        let visible_result = panic::catch_unwind(|| {
            panic!("visible test panic");
        });
        assert!(visible_result.is_err());
        assert_eq!(observed.load(Ordering::SeqCst), 1);

        panic::set_hook(previous_hook);
    }

    /// Verifies long multilingual context is segmented safely before tokie's parallel encoder runs.
    /// 验证长多语言上下文会在 Tokie 并行编码器运行前完成安全分段。
    #[test]
    fn tokie_long_multilingual_context_uses_safe_utf8_segments() {
        let tokenizer_path = default_tokenizer_resource_dir()
            .join("exact")
            .join("qwen")
            .join("tokenizer.tkz");
        let tokenizer =
            TokieTokenizer::from_file(&tokenizer_path).expect("load bundled Qwen tokenizer");
        let text = "System prompt 用户资料库检索 服务端 用户等级如何获取 ".repeat(600);

        let segments = tokie_safe_text_segments(&text);
        assert!(segments.len() > 1);
        assert!(
            segments
                .iter()
                .all(|segment| segment.len() < TOKIE_UNSAFE_PARALLEL_THRESHOLD_BYTES)
        );
        assert_eq!(segments.concat(), text);
        assert!(
            safe_count_tokie_tokens(&tokenizer, &text)
                .expect("long Tokie input should use safe segments")
                > 0
        );

        let counter = TokenCounter::new().expect("create default token counter");
        let estimate = counter
            .estimate("qwen3.8-max-preview", &text)
            .expect("estimate long Qwen context through safe Tokie segments");
        assert!(estimate.tokens > 0);
        assert_eq!(estimate.precision, TokenEstimatePrecision::Exact);
    }
}
