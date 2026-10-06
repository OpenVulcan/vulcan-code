//! Path-document construction for the auxiliary path retrieval channel.
//! 辅助路径检索通道使用的路径文档构建逻辑。

use crate::identity::{PathError, normalize_relative_path, sha256_hex_parts};
use crate::model::{Chunk, ChunkError, ChunkKind, HitKind, SearchHit, SearchSource};
use std::error::Error;
use std::fmt::{Display, Formatter};

/// A provider-independent document representing one source path.
/// 表示一个源码路径的不依赖供应商的路径文档。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PathDocument {
    /// Stable path occurrence identity.
    /// 稳定的路径出现位置身份。
    pub path_id: String,
    /// Blob identity owning the path document.
    /// 拥有该路径文档的 Blob 身份。
    pub blob_name: String,
    /// Canonical workspace-relative path.
    /// 规范化的工作区相对路径。
    pub path: String,
    /// Text describing path components and neutral file-type semantics.
    /// 描述路径组成和中立文件类型语义的文本。
    pub document: String,
}

impl PathDocument {
    /// Build a path document without injecting repository-specific prior knowledge.
    /// 构建路径文档且不注入仓库专用的先验知识。
    pub fn new(blob_name: impl Into<String>, path: &str) -> Result<Self, PathDocumentError> {
        let path = normalize_relative_path(path).map_err(PathDocumentError::InvalidPath)?;
        let blob_name = blob_name.into();
        let file_name = path.rsplit('/').next().unwrap_or(path.as_str());
        let stem = file_name
            .rsplit_once('.')
            .map(|(stem, _)| stem)
            .unwrap_or(file_name);
        let directory = path
            .rsplit_once('/')
            .map(|(directory, _)| directory)
            .unwrap_or("");
        let tokens = tokenize_path(&path);
        let extension_semantics = extension_semantics(file_name);
        let document = format!(
            "path: {path}\nfile: {file_name}\nstem: {stem}\ndirectory: {directory}\ntokens: {}\ntype: {extension_semantics}",
            tokens.join(" ")
        );
        let path_id = sha256_hex_parts([blob_name.as_bytes(), b"\n", path.as_bytes()]);
        Ok(Self {
            path_id,
            blob_name,
            path,
            document,
        })
    }

    /// Convert the path document into a structured auxiliary SearchHit.
    /// 将路径文档转换为结构化辅助检索命中。
    pub fn to_hit(&self, score: f32) -> Result<SearchHit, PathDocumentError> {
        let chunk = Chunk::new(
            self.blob_name.clone(),
            &self.path,
            self.document.clone(),
            1,
            1,
            ChunkKind::Path,
        )
        .map_err(PathDocumentError::InvalidChunk)?;
        Ok(SearchHit::from_chunk(
            &chunk,
            score,
            SearchSource::Path,
            HitKind::Content,
        ))
    }
}

/// Errors raised while constructing an auxiliary path document.
/// 构建辅助路径文档时产生的错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathDocumentError {
    /// The path is not a valid workspace-relative path.
    /// 路径不是有效的工作区相对路径。
    InvalidPath(PathError),
    /// The generated path hit could not be constructed.
    /// 生成的路径命中无法构建。
    InvalidChunk(ChunkError),
}

impl Display for PathDocumentError {
    /// Formats a path-document error for deterministic diagnostics.
    /// 将路径文档错误格式化为确定性的诊断文本。
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPath(error) => write!(formatter, "invalid path document path: {error}"),
            Self::InvalidChunk(error) => write!(formatter, "invalid path document hit: {error}"),
        }
    }
}

impl Error for PathDocumentError {}

/// Tokenize path separators, punctuation, underscores, dashes, and camelCase boundaries.
/// 按路径分隔符、标点、下划线、连字符和 camelCase 边界切分路径词元。
pub fn tokenize_path(path: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut previous_was_lowercase = false;
    for character in path.chars() {
        if !character.is_alphanumeric() {
            push_token(&mut tokens, &mut current);
            previous_was_lowercase = false;
            continue;
        }
        if character.is_uppercase() && previous_was_lowercase {
            push_token(&mut tokens, &mut current);
        }
        current.push(character.to_ascii_lowercase());
        previous_was_lowercase = character.is_lowercase();
    }
    push_token(&mut tokens, &mut current);
    tokens
}

/// Map common file extensions to neutral semantic descriptions.
/// 将常见文件扩展名映射为中立的语义描述。
fn extension_semantics(file_name: &str) -> &'static str {
    let extension = file_name.rsplit_once('.').map(|(_, extension)| extension);
    match extension.map(str::to_ascii_lowercase).as_deref() {
        Some("rs") => "Rust source",
        Some("c") | Some("h") | Some("cpp") | Some("hpp") => "C or C++ source",
        Some("ts") | Some("tsx") | Some("js") | Some("jsx") => "JavaScript or TypeScript source",
        Some("go") => "Go source",
        Some("py") => "Python source",
        Some("lua") => "Lua source",
        Some("json") | Some("toml") | Some("yaml") | Some("yml") => "configuration",
        Some("md") | Some("rst") | Some("txt") => "documentation",
        Some(_) => "source or project file",
        None => "extensionless project file",
    }
}

/// Append one non-empty token while preserving first-seen order.
/// 添加非空词元并保留首次出现顺序。
fn push_token(tokens: &mut Vec<String>, current: &mut String) {
    if !current.is_empty() && !tokens.iter().any(|token| token == current) {
        tokens.push(std::mem::take(current));
    } else {
        current.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify path documents include neutral path and extension semantics.
    /// 验证路径文档包含中立的路径和扩展名语义。
    #[test]
    fn builds_neutral_path_document() {
        let document = PathDocument::new("blob", "src/codeKitSearch.rs").unwrap();
        assert_eq!(document.path, "src/codeKitSearch.rs");
        assert!(document.document.contains("code kit search"));
        assert!(document.document.contains("Rust source"));
    }

    /// Verify path identity changes with Blob ownership.
    /// 验证路径身份会随所属 Blob 改变。
    #[test]
    fn path_identity_is_blob_scoped() {
        let first = PathDocument::new("blob-a", "src/lib.rs").unwrap();
        let second = PathDocument::new("blob-b", "src/lib.rs").unwrap();
        assert_ne!(first.path_id, second.path_id);
    }
}
