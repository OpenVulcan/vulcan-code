//! Metadata-first project resource reads with exact source bounds.
//! 先元数据后内容、具有精确源边界的项目资源读取。

use super::*;
use sha2::{Digest, Sha256};
use std::io::Read;

/// Formats exact project-scoped `resource_id` as a model-safe locator without any physical host path.
/// 将精确项目级`resource_id`格式化为模型安全定位符，不包含任何宿主物理路径。
pub fn project_resource_locator(resource_id: ResourceId) -> String {
    format!("project-resource://{}", resource_id.value())
}

/// Parses a canonical project-resource `reference`, distinguishing other declared locator families from malformed project locators.
/// 解析规范项目资源`reference`，区分其他已声明定位符族与损坏的项目定位符。
/// Returns an exact bounded id, absence for another family, or a safe syntax error without echoing the input.
/// 返回精确有界ID；其他族返回空；语法错误返回不回显输入的安全错误。
pub fn parse_project_resource_locator(reference: &str) -> Result<Option<ResourceId>, String> {
    if !reference.starts_with("project-resource:") {
        return Ok(None);
    }
    // Canonical decimal IDs permit no authorities, separators, escapes, query strings or alternate spellings.
    // 规范十进制ID不允许宿主、分隔符、转义、查询串或其他拼写。
    let id = reference
        .strip_prefix("project-resource://")
        .filter(|id| {
            !id.is_empty() && !id.starts_with('0') && id.bytes().all(|byte| byte.is_ascii_digit())
        })
        .ok_or_else(|| String::from("Invalid project resource locator."))?;
    let value = id
        .parse::<u64>()
        .map_err(|_| String::from("Invalid project resource locator."))?;
    ResourceId::try_new(value)
        .map(Some)
        .map_err(|_| String::from("Invalid project resource locator."))
}

/// One metadata-checked source; the private path never belongs in model-visible attachment text.
/// 一份通过元数据检查的来源；私有路径绝不能进入模型可见附件文本。
#[derive(Debug, Clone)]
pub struct ProjectResourceSource {
    /// Immutable catalog identity and display metadata.
    /// 不可变目录身份及显示元数据。
    pub entry: ResourceCatalogEntry,
    /// Exact host-owned entity path, used only after policy admission.
    /// 精确宿主持有实体路径，仅在策略准入后使用。
    pub path: PathBuf,
}

/// Resolves `resource_id` in exact `project_dir`/`catalog` without allocating or reading media bytes.
/// 在精确`project_dir`及`catalog`中解析`resource_id`，不分配或读取媒体字节。
/// Returns absence for missing entities and explicit errors for redirects, non-files or size drift.
/// 实体缺失时返回空，重定向、非文件或大小漂移时返回明确错误。
pub fn resolve_project_resource_source(
    project_dir: &Path,
    catalog: &ProjectResourceCatalog,
    resource_id: ResourceId,
) -> Result<Option<ProjectResourceSource>, ProjectResourceCatalogError> {
    validate_project_resource_catalog(catalog)?;
    // Only the exact logical id selects metadata; names and file paths are not alternate lookup keys.
    // 只有精确逻辑ID选择元数据；名称和文件路径不是备用查询键。
    let Some(entry) = catalog
        .entries
        .iter()
        .find(|entry| entry.resource_id == resource_id)
    else {
        return Ok(None);
    };
    // Canonical containment rejects an entity or resources directory redirected outside its exact project owner.
    // 规范路径包含校验拒绝被重定向至精确项目所有者之外的实体或resources目录。
    let project =
        fs::canonicalize(project_dir).map_err(|error| source_io_error(project_dir, error))?;
    // The digest-derived path belongs to the exact canonical project partition.
    // 摘要派生路径属于精确规范项目分区。
    let path = canonical_entity_path(&project, &entry.sha256);
    // Resolve filesystem redirects before accepting any source handle or content allocation.
    // 接受任何来源句柄或内容分配之前先解析文件系统重定向。
    let resolved = match fs::canonicalize(&path) {
        Ok(resolved) => resolved,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(source_io_error(&path, error)),
    };
    if resolved != path {
        return Err(source_corrupt(
            &path,
            "project resource entity is redirected",
        ));
    }
    // Source metadata is checked without following an entity-level symbolic link.
    // 检查来源元数据时不跟随实体级符号链接。
    let metadata = fs::symlink_metadata(&path).map_err(|error| source_io_error(&path, error))?;
    if !metadata.is_file() || metadata.len() != entry.size {
        return Err(source_corrupt(
            &path,
            "project resource type or size changed",
        ));
    }
    Ok(Some(ProjectResourceSource {
        entry: entry.clone(),
        path,
    }))
}

impl ProjectResourceSource {
    /// Reads exact admitted source bytes up to `max_bytes`, checking the final size and SHA-256 before returning them.
    /// 读取不超过`max_bytes`的精确已准入源字节，返回前检查最终大小及SHA-256。
    /// Oversized metadata is rejected before opening the file; growing files can contribute at most one extra byte.
    /// 超限元数据在打开文件前被拒绝；增长文件最多额外读取一个字节。
    pub fn read_bounded(&self, max_bytes: u64) -> Result<Vec<u8>, ProjectResourceCatalogError> {
        self.read_bounded_cancellable(max_bytes, || false)
    }

    /// Reads this exact source up to `max_bytes`, checking host-owned `is_cancelled` between bounded chunks.
    /// 在`max_bytes`内读取本精确来源，在有界分块之间检查宿主持有的`is_cancelled`。
    /// Returns bytes only after exact length/digest validation; cancellation is an interrupted I/O error, never a partial success.
    /// 仅在精确长度/摘要校验后返回字节；取消是中断IO错误，绝不返回部分成功。
    pub fn read_bounded_cancellable(
        &self,
        max_bytes: u64,
        is_cancelled: impl Fn() -> bool,
    ) -> Result<Vec<u8>, ProjectResourceCatalogError> {
        if is_cancelled() {
            return Err(source_cancelled(&self.path));
        }
        if self.entry.size > max_bytes {
            return Err(source_corrupt(
                &self.path,
                "project resource exceeds admitted read bound",
            ));
        }
        // Recheck the captured identity against the file handle opened for this read.
        // 对本次读取打开的文件句柄重新检查捕获身份。
        let mut file = self.open_exact()?.take(self.entry.size.saturating_add(1));
        // No catalog-controlled preallocation: the reader grows only from actually read admitted bytes.
        // 不使用目录控制的预分配：读取器仅随实际读取的已准入字节增长。
        let mut bytes = Vec::new();
        // Bound each synchronous read so cancellation can stop a dropped worker without retaining the entire remainder.
        // 约束每次同步读取，使取消可停止被丢弃的工作线程，不保留全部剩余内容。
        let mut buffer = [0_u8; 64 * 1024];
        // Hash each retained chunk once instead of an uncancellable second whole-buffer pass.
        // 每个保留块仅哈希一次，不进行无法取消的第二次整缓冲区遍历。
        let mut digest = Sha256::new();
        loop {
            if is_cancelled() {
                return Err(source_cancelled(&self.path));
            }
            // The opened source retains its exact catalog identity and one-byte growth detector.
            // 已打开来源保留精确目录身份及一字节增长检测器。
            let count = file
                .read(&mut buffer)
                .map_err(|error| source_io_error(&self.path, error))?;
            if is_cancelled() {
                return Err(source_cancelled(&self.path));
            }
            if count == 0 {
                break;
            }
            if bytes.len() as u64 + count as u64 > self.entry.size {
                return Err(source_corrupt(
                    &self.path,
                    "project resource grew beyond its immutable catalog size",
                ));
            }
            digest.update(&buffer[..count]);
            bytes.extend_from_slice(&buffer[..count]);
        }
        if bytes.len() as u64 != self.entry.size
            || hex::encode(digest.finalize()) != self.entry.sha256
        {
            return Err(source_corrupt(
                &self.path,
                "project resource content differs from its immutable catalog identity",
            ));
        }
        Ok(bytes)
    }

    /// Verifies an admitted source up to `max_bytes` using a fixed buffer and returns no media allocation.
    /// 使用固定缓冲区验证不超过`max_bytes`的已准入源，不返回媒体内存分配。
    /// Returns an explicit error on missing, growing or corrupted source bytes before an upload may start.
    /// 来源缺失、增长或损坏时在上传可开始之前返回明确错误。
    pub fn verify_bounded(&self, max_bytes: u64) -> Result<(), ProjectResourceCatalogError> {
        if self.entry.size > max_bytes {
            return Err(source_corrupt(
                &self.path,
                "project resource exceeds admitted verification bound",
            ));
        }
        // One extra byte detects growth without traversing the unadmitted tail.
        // 一个额外字节检测增长，不遍历未准入尾部。
        let mut file = self.open_exact()?.take(self.entry.size.saturating_add(1));
        // The fixed stack buffer keeps verification memory independent of upload size.
        // 固定栈缓冲区使验证内存与上传大小无关。
        let mut buffer = [0_u8; 64 * 1024];
        // Incremental hashing covers the exact bytes read from this handle.
        // 增量哈希覆盖从本句柄读取的精确字节。
        let mut digest = Sha256::new();
        // Actual bytes are counted independently of the catalog declaration.
        // 实际字节计数独立于目录声明。
        let mut size = 0_u64;
        loop {
            // Each bounded chunk contributes once to both length and digest.
            // 每个有界块仅一次计入长度及摘要。
            let count = file
                .read(&mut buffer)
                .map_err(|error| source_io_error(&self.path, error))?;
            if count == 0 {
                break;
            }
            size += count as u64;
            digest.update(&buffer[..count]);
        }
        if size != self.entry.size || hex::encode(digest.finalize()) != self.entry.sha256 {
            return Err(source_corrupt(
                &self.path,
                "project resource content differs from its immutable catalog identity",
            ));
        }
        Ok(())
    }

    /// Opens this exact admitted entity and rechecks current containment, type and handle size.
    /// 打开本精确已准入实体，并重新检查当前包含关系、类型及句柄大小。
    /// Returns only a regular file whose current metadata still matches the captured source.
    /// 仅返回当前元数据仍匹配捕获来源的普通文件。
    fn open_exact(&self) -> Result<fs::File, ProjectResourceCatalogError> {
        // A source may have been redirected since metadata capture; do not silently follow it.
        // 来源可能在元数据捕获后被重定向，不能静默跟随。
        let resolved =
            fs::canonicalize(&self.path).map_err(|error| source_io_error(&self.path, error))?;
        if resolved != self.path {
            return Err(source_corrupt(
                &self.path,
                "project resource entity is redirected",
            ));
        }
        // Open only the validated private entity, never a caller-supplied fallback path.
        // 仅打开已校验私有实体，绝不打开调用方提供的备用路径。
        let file =
            fs::File::open(&self.path).map_err(|error| source_io_error(&self.path, error))?;
        // Handle metadata detects file replacement or growth before the first content read.
        // 句柄元数据在首次内容读取前检测文件替换或增长。
        let metadata = file
            .metadata()
            .map_err(|error| source_io_error(&self.path, error))?;
        if !metadata.is_file() || metadata.len() != self.entry.size {
            return Err(source_corrupt(
                &self.path,
                "project resource type or size changed",
            ));
        }
        Ok(file)
    }
}

/// Maps one IO `error` on exact `path` to the existing explicit catalog diagnostic.
/// 把精确`path`上的IO错误映射至既有显式目录诊断。
fn source_io_error(path: &Path, error: std::io::Error) -> ProjectResourceCatalogError {
    ProjectResourceCatalogError::CatalogRead {
        path: path.to_path_buf(),
        detail: error.to_string(),
    }
}

/// Returns an interrupted source-read error for exact private `path`, without exposing partial resource bytes.
/// 为精确私有`path`返回中断来源读取错误，不暴露部分资源字节。
fn source_cancelled(path: &Path) -> ProjectResourceCatalogError {
    source_io_error(
        path,
        std::io::Error::new(
            std::io::ErrorKind::Interrupted,
            "project resource read cancelled",
        ),
    )
}

/// Reports one source invariant `detail` for exact `path` without reading further bytes.
/// 报告精确`path`的来源不变量`detail`，不继续读取字节。
fn source_corrupt(path: &Path, detail: &str) -> ProjectResourceCatalogError {
    ProjectResourceCatalogError::EntityCorrupt {
        path: path.to_path_buf(),
        detail: detail.to_string(),
    }
}

#[cfg(test)]
mod tests;
