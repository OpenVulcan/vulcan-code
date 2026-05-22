//! About-page license catalog discovery and on-demand body loading helpers.
//! 关于页面许可证目录发现与按需正文加载辅助逻辑。

use super::*;
use crate::runtime_support::runtime_assets;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

/// File name used by generated Rust third-party notice Markdown output.
/// 生成的 Rust 第三方许可证 Markdown 输出使用的文件名。
const GENERATED_RUST_THIRD_PARTY_NOTICES_FILE_NAME: &str = "THIRD_PARTY_NOTICES.md";

/// File name used by the generated Rust third-party notice section index.
/// 生成的 Rust 第三方许可证分段索引使用的文件名。
const GENERATED_RUST_THIRD_PARTY_NOTICES_INDEX_FILE_NAME: &str = "THIRD_PARTY_NOTICES_INDEX.json";

/// Stable identifier used by the embedded application license entry.
/// 内嵌应用许可证条目使用的稳定标识。
const APPLICATION_LICENSE_DOCUMENT_ID: &str = "application-license";

/// Embedded application MIT license text kept available even when no external source tree is present.
/// 即使外部源码树不可用也始终可见的内嵌应用 MIT 许可证文本。
const APPLICATION_LICENSE_EMBEDDED_TEXT: &str = include_str!("../../../../LICENSE");

/// One About-page document category used to render compact summaries and grouping hints.
/// 关于页面文档的分类，用于渲染紧凑摘要与分组提示。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AboutLicenseDocumentKind {
    /// Application-level project license bundled with the app itself.
    /// 应用本体自带的项目级许可证。
    Application,
    /// Bundled static asset notice such as embedded font licenses.
    /// 内嵌静态资源声明，例如字体许可证。
    BundledAsset,
    /// Generated Rust dependency notice section derived from cargo-about output.
    /// 由 cargo-about 输出拆分得到的 Rust 依赖许可证分段。
    RustDependency,
}

impl AboutLicenseDocumentKind {
    /// Returns one localized category label used by compact document list rows.
    /// 返回用于紧凑文档列表行的本地化分类标签。
    pub(super) fn localized_label(self, locale: AppLocale) -> &'static str {
        match (self, locale) {
            (Self::Application, AppLocale::EnUs) => "Application",
            (Self::Application, _) => "应用本体",
            (Self::BundledAsset, AppLocale::EnUs) => "Bundled Asset",
            (Self::BundledAsset, _) => "内置资源",
            (Self::RustDependency, AppLocale::EnUs) => "Rust Dependency",
            (Self::RustDependency, _) => "Rust 依赖",
        }
    }

    /// Returns one localized summary sentence for the provided document metadata.
    /// 根据给定文档元数据返回一条本地化摘要说明。
    pub(super) fn localized_summary(
        self,
        locale: AppLocale,
        used_by_count: Option<usize>,
    ) -> String {
        match (self, locale, used_by_count) {
            (Self::Application, AppLocale::EnUs, _) => {
                String::from("Project application license embedded with the desktop app.")
            }
            (Self::Application, _, _) => String::from("桌面应用本体随附的项目许可证。"),
            (Self::BundledAsset, AppLocale::EnUs, _) => {
                String::from("Bundled asset notice loaded on demand.")
            }
            (Self::BundledAsset, _, _) => String::from("内置资源许可证，按需查看正文。"),
            (Self::RustDependency, AppLocale::EnUs, Some(count)) => {
                format!("One generated Rust dependency license section covering {count} crates.")
            }
            (Self::RustDependency, _, Some(count)) => {
                format!("一条生成的 Rust 依赖许可证分段，覆盖 {count} 个 crate。")
            }
            (Self::RustDependency, AppLocale::EnUs, None) => {
                String::from("One generated Rust dependency license section.")
            }
            (Self::RustDependency, _, None) => String::from("一条生成的 Rust 依赖许可证分段。"),
        }
    }
}

/// One concrete source used to materialize a document body only when the user opens the detail modal.
/// 仅在用户打开详情弹窗时才实际物化正文的具体来源。
#[derive(Debug, Clone)]
pub(super) enum AboutLicenseDocumentBodySource {
    /// Body is embedded in the binary and can be returned immediately.
    /// 正文已内嵌进二进制，可立即返回。
    EmbeddedText {
        /// Embedded body text kept in the executable image.
        /// 保存在可执行镜像中的内嵌正文文本。
        body: &'static str,
    },
    /// Body should be read from the whole external file.
    /// 正文应从整个外部文件读取。
    WholeFile {
        /// Concrete file path containing the full document body.
        /// 包含完整文档正文的具体文件路径。
        path: PathBuf,
    },
    /// Body should be read from one concrete line range of an external file.
    /// 正文应从外部文件的某个具体行范围读取。
    FileLineRange {
        /// Concrete file path that stores the generated aggregated content.
        /// 存放生成聚合内容的具体文件路径。
        path: PathBuf,
        /// Inclusive 1-based start line of the body slice.
        /// 正文切片的起始行号，采用从 1 开始的闭区间。
        start_line: usize,
        /// Inclusive 1-based end line of the body slice.
        /// 正文切片的结束行号，采用从 1 开始的闭区间。
        end_line: usize,
    },
}

impl AboutLicenseDocumentBodySource {
    /// Loads and normalizes the body text for one concrete document source.
    /// 读取并归一化某个具体文档来源对应的正文文本。
    pub(super) fn load_body_text(&self) -> Result<String, String> {
        match self {
            Self::EmbeddedText { body } => Ok(normalize_license_body_text((*body).to_string())),
            Self::WholeFile { path } => read_whole_license_body(path),
            Self::FileLineRange {
                path,
                start_line,
                end_line,
            } => read_license_body_line_range(path, *start_line, *end_line),
        }
    }
}

/// One About-page license list entry that can lazily resolve its full body text later.
/// 一条关于页面许可证列表项，后续可以按需懒加载其完整正文。
#[derive(Debug, Clone)]
pub(super) struct AboutLicenseDocument {
    /// Stable document identifier used by modal state and read-only body-buffer caches.
    /// 供弹窗状态与只读正文缓冲缓存使用的稳定文档标识。
    pub(super) id: String,
    /// Human-readable document title shown in list rows and detail modal headers.
    /// 展示在列表行与详情弹窗头部的人类可读文档标题。
    pub(super) title: String,
    /// Semantic document category used for compact summaries and labels.
    /// 用于紧凑摘要与标签的语义文档分类。
    pub(super) kind: AboutLicenseDocumentKind,
    /// Total visible body line count shown in the list metadata.
    /// 展示在列表元数据中的正文可见总行数。
    pub(super) line_count: usize,
    /// Optional dependent crate count attached to generated Rust dependency sections.
    /// 附加在生成 Rust 依赖分段上的可选依赖 crate 数量。
    pub(super) used_by_count: Option<usize>,
    /// Concrete crate list covered by this document when one generated Rust dependency entry exposes indexed details.
    /// 当生成的 Rust 依赖条目暴露索引明细时，该文档覆盖的具体 crate 列表。
    pub(super) used_by_entries: Vec<String>,
    /// Deferred body source consumed only when the user opens the detail modal.
    /// 仅在用户打开详情弹窗时才会消费的延迟正文来源。
    pub(super) body_source: AboutLicenseDocumentBodySource,
    /// Optional external file path that can be opened with the system default application.
    /// 可选的外部文件路径，可交由系统默认应用打开。
    pub(super) external_path: Option<PathBuf>,
}

impl AboutLicenseDocument {
    /// Returns one localized summary string derived from the current document kind and metadata.
    /// 根据当前文档分类与元数据返回一条本地化摘要说明。
    pub(super) fn localized_summary(&self, locale: AppLocale) -> String {
        self.kind.localized_summary(locale, self.used_by_count)
    }

    /// Loads the body text represented by the current document source.
    /// 加载当前文档来源所代表的正文文本。
    pub(super) fn load_body_text(&self) -> Result<String, String> {
        self.body_source.load_body_text()
    }
}

/// One generated Rust notice section entry read from the sidecar JSON index file.
/// 从旁路 JSON 索引文件读取的一条生成 Rust 许可证分段记录。
#[derive(Debug, Clone, Deserialize)]
struct GeneratedRustThirdPartyNoticeIndexEntry {
    /// Stable generated document identifier.
    /// 生成文档的稳定标识。
    id: String,
    /// Human-readable license title.
    /// 人类可读的许可证标题。
    title: String,
    /// Number of crates that reference this license section.
    /// 引用该许可证分段的 crate 数量。
    used_by_count: usize,
    /// Concrete crate labels covered by this license section.
    /// 该许可证分段覆盖的具体 crate 标签集合。
    #[serde(default)]
    used_by_entries: Vec<String>,
    /// Inclusive 1-based start line of the license body inside the Markdown file.
    /// 许可证正文在 Markdown 文件中的起始行号，采用从 1 开始的闭区间。
    start_line: usize,
    /// Inclusive 1-based end line of the license body inside the Markdown file.
    /// 许可证正文在 Markdown 文件中的结束行号，采用从 1 开始的闭区间。
    end_line: usize,
    /// Cached body line count stored beside the line range for quick list rendering.
    /// 与行范围一并缓存的正文行数，供列表快速渲染使用。
    #[serde(default)]
    line_count: usize,
}

/// One generated Rust notice sidecar index file payload.
/// 一份生成 Rust 许可证旁路索引文件的载荷结构。
#[derive(Debug, Clone, Deserialize)]
struct GeneratedRustThirdPartyNoticeIndexFile {
    /// Indexed license section records emitted during `./make licenses`.
    /// 在 `./make licenses` 期间输出的许可证分段索引记录。
    #[serde(default)]
    licenses: Vec<GeneratedRustThirdPartyNoticeIndexEntry>,
}

/// Loaded About-page license catalog plus helper paths reused by the GUI surface.
/// 已加载的关于页面许可证目录及其界面复用的辅助路径。
#[derive(Debug, Clone, Default)]
pub(super) struct AboutLicenseCatalog {
    /// Visible document list rendered by the About-page license section.
    /// 由关于页面许可证分区渲染的可见文档列表。
    pub(super) documents: Vec<AboutLicenseDocument>,
    /// Preferred directory exposed to the “open license folder” action.
    /// 暴露给“打开许可证目录”动作的首选目录。
    preferred_directory: Option<PathBuf>,
}

impl AboutLicenseCatalog {
    /// Returns one empty catalog used as the initial view state before any background load begins.
    /// 返回一份空目录，供后台加载开始前作为初始视图状态使用。
    pub(super) fn empty() -> Self {
        Self::default()
    }

    /// Resolves and loads the full About-page license catalog metadata without materializing heavy bodies.
    /// 解析并加载完整的关于页面许可证目录元数据，但不提前物化较重的正文内容。
    pub(super) fn load() -> Result<Self, String> {
        let generated_license_directory = resolve_generated_license_resource_directory();
        let source_license_directory = resolve_static_license_resource_directory();
        let preferred_directory = generated_license_directory
            .clone()
            .or_else(|| source_license_directory.clone());

        let mut documents = vec![build_application_license_document()];

        documents.extend(load_bundled_asset_license_documents(
            generated_license_directory.as_deref(),
            source_license_directory.as_deref(),
        )?);

        if let Some(generated_directory) = generated_license_directory.as_deref() {
            let generated_notice_markdown_path =
                generated_directory.join(GENERATED_RUST_THIRD_PARTY_NOTICES_FILE_NAME);
            if generated_notice_markdown_path.is_file() {
                let generated_notice_index_path =
                    generated_directory.join(GENERATED_RUST_THIRD_PARTY_NOTICES_INDEX_FILE_NAME);
                let generated_entries = load_generated_rust_notice_index_entries(
                    &generated_notice_markdown_path,
                    generated_notice_index_path.as_path(),
                )?;
                if !generated_entries.is_empty() {
                    documents.extend(generated_entries.into_iter().map(|entry| {
                        build_generated_rust_notice_document(&generated_notice_markdown_path, entry)
                    }));
                }
            }
        }

        Ok(Self {
            documents,
            preferred_directory,
        })
    }

    /// Looks up one document by its stable identifier.
    /// 根据稳定标识查找一份文档。
    pub(super) fn document(&self, document_id: &str) -> Option<&AboutLicenseDocument> {
        self.documents
            .iter()
            .find(|document| document.id == document_id)
    }

    /// Returns the preferred directory that should be opened from the About-page action row.
    /// 返回应由关于页面动作栏打开的首选目录。
    pub(super) fn preferred_directory_to_open(&self) -> Option<&Path> {
        self.preferred_directory.as_deref()
    }
}

/// Builds one application-license document that always stays available through an embedded fallback body.
/// 构建一份应用本体许可证文档，并通过内嵌回退正文保证始终可用。
fn build_application_license_document() -> AboutLicenseDocument {
    let external_path = application_license_file_path().filter(|path| path.is_file());
    AboutLicenseDocument {
        id: String::from(APPLICATION_LICENSE_DOCUMENT_ID),
        title: String::from("Vulcan Code MIT License"),
        kind: AboutLicenseDocumentKind::Application,
        line_count: count_text_lines(APPLICATION_LICENSE_EMBEDDED_TEXT),
        used_by_count: None,
        used_by_entries: Vec::new(),
        body_source: AboutLicenseDocumentBodySource::EmbeddedText {
            body: APPLICATION_LICENSE_EMBEDDED_TEXT,
        },
        external_path,
    }
}

/// Loads the bundled asset-license documents from the resolved runtime license directory.
/// 从解析后的运行时许可证目录加载内置资源许可证文档。
fn load_bundled_asset_license_documents(
    generated_license_directory: Option<&Path>,
    source_license_directory: Option<&Path>,
) -> Result<Vec<AboutLicenseDocument>, String> {
    let bundled_specs = [
        (
            "Roboto-LICENSE.txt",
            "Roboto Font License",
            "font-roboto-license",
        ),
        (
            "NotoSansCJK-LICENSE.txt",
            "Noto Sans CJK Font License",
            "font-noto-sans-cjk-license",
        ),
        (
            "VictorMono-LICENSE.txt",
            "Victor Mono Font License",
            "font-victor-mono-license",
        ),
    ];

    let mut documents = Vec::new();
    for (file_name, title, document_id) in bundled_specs {
        let path = resolve_bundled_asset_license_path(
            generated_license_directory,
            source_license_directory,
            file_name,
        );
        let Some(path) = path else {
            continue;
        };
        if !path.is_file() {
            continue;
        }
        let body = fs::read_to_string(&path)
            .map_err(|error| format!("读取许可证文件 `{}` 失败：{error}", path.display()))?;
        documents.push(AboutLicenseDocument {
            id: String::from(document_id),
            title: String::from(title),
            kind: AboutLicenseDocumentKind::BundledAsset,
            line_count: count_text_lines(&body),
            used_by_count: None,
            used_by_entries: Vec::new(),
            body_source: AboutLicenseDocumentBodySource::WholeFile { path: path.clone() },
            external_path: Some(path),
        });
    }

    Ok(documents)
}

/// Resolves one bundled asset license file path, preferring output-side resources and falling back to source resources.
/// 解析单个内置资源许可证文件路径，优先使用 output 侧资源，缺失时回退到源码资源。
fn resolve_bundled_asset_license_path(
    generated_license_directory: Option<&Path>,
    source_license_directory: Option<&Path>,
    file_name: &str,
) -> Option<PathBuf> {
    if let Some(generated_directory) = generated_license_directory {
        let candidate = generated_directory.join(file_name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    source_license_directory
        .map(|directory| directory.join(file_name))
        .filter(|candidate| candidate.is_file())
}

/// Loads generated Rust dependency notice index entries, preferring the JSON sidecar and falling back to Markdown parsing.
/// 加载生成的 Rust 依赖许可证索引记录，优先读取 JSON 旁路文件，缺失时回退为 Markdown 解析。
fn load_generated_rust_notice_index_entries(
    markdown_path: &Path,
    index_path: &Path,
) -> Result<Vec<GeneratedRustThirdPartyNoticeIndexEntry>, String> {
    if index_path.is_file() {
        if let Ok(payload) = fs::read_to_string(index_path) {
            if let Ok(mut parsed) =
                serde_json::from_str::<GeneratedRustThirdPartyNoticeIndexFile>(&payload)
            {
                parsed
                    .licenses
                    .retain(|entry| entry.start_line > 0 && entry.end_line >= entry.start_line);
                for entry in &mut parsed.licenses {
                    if entry.line_count == 0 {
                        entry.line_count = entry.end_line.saturating_sub(entry.start_line) + 1;
                    }
                    if entry.used_by_count == 0 {
                        entry.used_by_count = entry.used_by_entries.len();
                    }
                }
                if !parsed.licenses.is_empty() {
                    return Ok(parsed.licenses);
                }
            }
        }
    }

    parse_generated_rust_notice_sections_from_markdown(markdown_path)
}

/// Builds one lazily-loaded About-page document from one generated Rust notice index entry.
/// 根据一条生成的 Rust 许可证索引记录构建一份懒加载关于页面文档。
fn build_generated_rust_notice_document(
    markdown_path: &Path,
    entry: GeneratedRustThirdPartyNoticeIndexEntry,
) -> AboutLicenseDocument {
    AboutLicenseDocument {
        id: entry.id,
        title: entry.title,
        kind: AboutLicenseDocumentKind::RustDependency,
        line_count: entry
            .line_count
            .max(entry.end_line.saturating_sub(entry.start_line) + 1),
        used_by_count: Some(entry.used_by_count),
        used_by_entries: entry.used_by_entries,
        body_source: AboutLicenseDocumentBodySource::FileLineRange {
            path: markdown_path.to_path_buf(),
            start_line: entry.start_line,
            end_line: entry.end_line,
        },
        external_path: Some(markdown_path.to_path_buf()),
    }
}

/// Parses generated Rust notice sections directly from the Markdown file when the JSON sidecar is not available yet.
/// 当 JSON 旁路文件尚不可用时，直接从 Markdown 文件解析生成的 Rust 许可证分段。
fn parse_generated_rust_notice_sections_from_markdown(
    markdown_path: &Path,
) -> Result<Vec<GeneratedRustThirdPartyNoticeIndexEntry>, String> {
    let body = fs::read_to_string(markdown_path).map_err(|error| {
        format!(
            "读取生成的 Rust 许可证 Markdown `{}` 失败：{error}",
            markdown_path.display()
        )
    })?;
    let lines = body.lines().map(String::from).collect::<Vec<_>>();
    let mut parsed_sections: Vec<GeneratedRustThirdPartyNoticeIndexEntry> = Vec::new();
    let mut merged_indices_by_signature = BTreeMap::<String, usize>::new();
    let mut line_index = 0usize;

    while line_index < lines.len() {
        let current_line = lines[line_index].trim();
        let Some(raw_title) = current_line.strip_prefix("### ") else {
            line_index += 1;
            continue;
        };
        let title = raw_title.trim().to_string();
        let mut used_by_count = 0usize;
        let mut used_by_entries = Vec::new();
        let mut scan_index = line_index + 1;
        let mut start_line = None;
        let mut end_line = None;

        while scan_index < lines.len() {
            let scanned_line = lines[scan_index].trim();
            if scanned_line.starts_with("### ") {
                break;
            }
            if scanned_line == "Used by:" {
                scan_index += 1;
                while scan_index < lines.len() {
                    let used_by_line = lines[scan_index].trim();
                    if used_by_line == "```text" {
                        break;
                    }
                    if used_by_line.starts_with("- ") {
                        used_by_count = used_by_count.saturating_add(1);
                        used_by_entries.push(used_by_line.trim_start_matches("- ").to_string());
                    }
                    scan_index += 1;
                }
                continue;
            }
            if scanned_line == "```text" {
                let mut first_line = scan_index + 2;
                let mut last_line = scan_index + 1;
                scan_index += 1;
                while scan_index < lines.len() {
                    if lines[scan_index].trim() == "```" {
                        last_line = scan_index;
                        break;
                    }
                    scan_index += 1;
                }
                if last_line >= first_line {
                    while first_line <= last_line
                        && lines[first_line.saturating_sub(1)].trim().is_empty()
                    {
                        first_line += 1;
                    }
                    while last_line >= first_line
                        && lines[last_line.saturating_sub(1)].trim().is_empty()
                    {
                        last_line = last_line.saturating_sub(1);
                    }
                    if first_line <= last_line {
                        start_line = Some(first_line);
                        end_line = Some(last_line);
                    }
                }
                break;
            }
            scan_index += 1;
        }

        if let (Some(start_line), Some(end_line)) = (start_line, end_line) {
            let merge_key = title.clone();
            if let Some(existing_index) = merged_indices_by_signature.get(&merge_key).copied() {
                parsed_sections[existing_index].used_by_count = parsed_sections[existing_index]
                    .used_by_count
                    .saturating_add(used_by_count);
                parsed_sections[existing_index]
                    .used_by_entries
                    .extend(used_by_entries);
            } else {
                merged_indices_by_signature.insert(merge_key, parsed_sections.len());
                parsed_sections.push(GeneratedRustThirdPartyNoticeIndexEntry {
                    id: String::new(),
                    title,
                    used_by_count,
                    used_by_entries,
                    start_line,
                    end_line,
                    line_count: end_line.saturating_sub(start_line) + 1,
                });
            }
        }

        line_index = scan_index.saturating_add(1);
    }

    let mut seen_ids = BTreeMap::<String, usize>::new();
    for section in &mut parsed_sections {
        let base_id = format!(
            "rust-dependency-license-{}",
            slugify_license_identifier(&section.title)
        );
        let counter = seen_ids.entry(base_id.clone()).or_insert(0);
        *counter += 1;
        section.id = if *counter == 1 {
            base_id
        } else {
            format!("{base_id}-{}", *counter)
        };
    }

    Ok(parsed_sections)
}

/// Reads and normalizes one whole-file license body from disk.
/// 从磁盘读取并归一化一份整文件许可证正文。
fn read_whole_license_body(path: &Path) -> Result<String, String> {
    let body = fs::read_to_string(path)
        .map_err(|error| format!("读取许可证文件 `{}` 失败：{error}", path.display()))?;
    Ok(normalize_license_body_text(body))
}

/// Reads and normalizes one concrete inclusive line range from a generated aggregated license file.
/// 从生成聚合许可证文件中读取并归一化一段具体的闭区间行范围。
fn read_license_body_line_range(
    path: &Path,
    start_line: usize,
    end_line: usize,
) -> Result<String, String> {
    if start_line == 0 || end_line < start_line {
        return Err(format!(
            "许可证正文行范围无效：start_line={start_line}, end_line={end_line}"
        ));
    }

    let file = fs::File::open(path)
        .map_err(|error| format!("打开许可证文件 `{}` 失败：{error}", path.display()))?;
    let reader = BufReader::new(file);
    let mut body_lines = Vec::new();
    let mut last_line_seen = 0usize;

    for (zero_based_index, line_result) in reader.lines().enumerate() {
        let line_number = zero_based_index + 1;
        if line_number < start_line {
            continue;
        }
        if line_number > end_line {
            break;
        }
        let line = line_result
            .map_err(|error| format!("读取许可证文件 `{}` 失败：{error}", path.display()))?;
        body_lines.push(line);
        last_line_seen = line_number;
    }

    if body_lines.is_empty() || last_line_seen < end_line {
        return Err(format!(
            "许可证正文行范围超出文件实际内容：`{}` 请求 {}-{}。",
            path.display(),
            start_line,
            end_line
        ));
    }

    Ok(normalize_license_body_text(body_lines.join("\n")))
}

/// Trims surrounding blank lines while keeping internal formatting intact for selectable raw-text rendering.
/// 在保留内部排版的前提下裁剪首尾空白行，以便可选原文渲染更紧凑。
fn normalize_license_body_text(body: String) -> String {
    let mut lines = body.lines().collect::<Vec<_>>();
    while lines.first().is_some_and(|line| line.trim().is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

/// Counts visible lines in one body text payload for compact list metadata.
/// 统计一份正文文本中的可见行数，用于紧凑列表元数据。
fn count_text_lines(body: &str) -> usize {
    normalize_license_body_text(body.to_string())
        .lines()
        .count()
        .max(1)
}

/// Returns the repository-root application license file path when it still exists on disk.
/// 当仓库根应用许可证文件仍存在于磁盘上时，返回其路径。
fn application_license_file_path() -> Option<PathBuf> {
    Some(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("LICENSE"),
    )
}

/// Resolves the generated license resource directory rooted in the active `output` bundle when available.
/// 在可用时解析出以活动 `output` 包为根的生成许可证资源目录。
fn resolve_generated_license_resource_directory() -> Option<PathBuf> {
    runtime_assets::resolve_luaskills_runtime_root()
        .map(|root| root.join("resources").join("licenses"))
        .filter(|path| path.is_dir())
}

/// Resolves the static application license resource directory used by local development or packaged runs.
/// 解析本地开发或打包运行时使用的静态应用许可证资源目录。
fn resolve_static_license_resource_directory() -> Option<PathBuf> {
    let path = runtime_assets::resolve_app_resources_directory().join("licenses");
    path.is_dir().then_some(path)
}

/// Converts one free-form license title into a stable lowercase identifier fragment.
/// 将自由格式的许可证标题转换为稳定的小写标识片段。
fn slugify_license_identifier(title: &str) -> String {
    let mut slug = String::new();
    let mut last_was_separator = false;
    for character in title.chars() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Creates one unique temporary test directory below the process temp root and cleans it up on drop.
    /// 在进程临时目录下创建唯一测试目录，并在析构时自动清理。
    struct TempTestDir {
        /// Concrete directory path owned by the temporary test directory helper.
        /// 临时测试目录辅助对象持有的具体目录路径。
        path: PathBuf,
    }

    impl TempTestDir {
        /// Creates one fresh temporary test directory path unique to the current test process.
        /// 创建一条对当前测试进程唯一的新临时测试目录路径。
        fn new() -> Self {
            let unique_suffix = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos();
            let path = std::env::temp_dir()
                .join(format!("vulcan-code-about-license-tests-{unique_suffix}"));
            fs::create_dir_all(&path).expect("create temp test directory");
            Self { path }
        }

        /// Returns the owned temporary directory path.
        /// 返回当前持有的临时目录路径。
        fn path(&self) -> &Path {
            self.path.as_path()
        }
    }

    impl Drop for TempTestDir {
        /// Removes the owned temporary directory tree after the test finishes.
        /// 在测试结束后移除当前持有的临时目录树。
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    /// Verifies that Markdown fallback parsing extracts one stable per-license body range and crate count.
    /// 验证 Markdown 回退解析能够提取稳定的逐许可证正文范围与 crate 数量。
    #[test]
    fn about_license_markdown_fallback_extracts_license_ranges() {
        let temp_dir = TempTestDir::new();
        let markdown_path = temp_dir.path().join("THIRD_PARTY_NOTICES.md");
        fs::write(
            &markdown_path,
            r#"# Rust Third-Party Notices

## License Details

### Apache License 2.0

Used by:
- crate-a 0.1.0
- crate-b 0.2.0

```text

Apache body line 1
Apache body line 2

```

### Apache License 2.0

Used by:
- crate-c 0.3.0

```text
Apache replacement body line 1
Apache replacement body line 2
```

### MIT License

Used by:
- crate-c 1.0.0

```text
MIT body
```
"#,
        )
        .expect("write markdown");

        let parsed = parse_generated_rust_notice_sections_from_markdown(&markdown_path)
            .expect("parse sections");
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].title, "Apache License 2.0");
        assert_eq!(parsed[0].used_by_count, 3);
        assert_eq!(parsed[0].line_count, 2);
        assert_eq!(parsed[0].used_by_entries.len(), 3);
        assert_eq!(parsed[0].used_by_entries[0], "crate-a 0.1.0");
        assert_eq!(parsed[0].used_by_entries[2], "crate-c 0.3.0");
        assert_eq!(parsed[1].title, "MIT License");
        assert_eq!(parsed[1].used_by_count, 1);
        assert_eq!(
            parsed[1].used_by_entries,
            vec![String::from("crate-c 1.0.0")]
        );
        assert_eq!(parsed[1].line_count, 1);
    }

    /// Verifies that line-range loading trims outer blank lines and keeps only the requested slice.
    /// 验证按行范围加载会裁剪外层空白行，并且只保留请求的切片内容。
    #[test]
    fn about_license_line_range_reader_trims_requested_body() {
        let temp_dir = TempTestDir::new();
        let body_path = temp_dir.path().join("LICENSE_SECTION.txt");
        fs::write(&body_path, "line 1\n\nbody line 1\nbody line 2\n\nline 6\n")
            .expect("write body");

        let body = read_license_body_line_range(&body_path, 2, 5).expect("read range");
        assert_eq!(body, "body line 1\nbody line 2");
    }
}
