//! Exact physical ownership discovery for destructive session lifecycle operations.
//! 破坏性会话生命周期操作使用的精确物理归属发现。

use super::*;
use std::collections::BTreeSet;

impl SessionV1StorageLocator {
    /// Enumerates all physically owned embedded sessions below this exact locator.
    /// 枚举此精确定位器下物理归属的全部嵌入会话。
    /// Returns validated locators, including children no longer referenced by the current business generation.
    /// 返回已验证定位器，包括当前业务代次已不再引用的子会话。
    /// The caller must fence lifecycle changes and repeat discovery before destructive publication.
    /// 调用方必须隔离生命周期变化，并在破坏性发布之前重复发现。
    pub fn embedded_task_tree_locators(&self) -> Result<Vec<Self>, SessionV1StoreError> {
        self.read_indexed_metadata()?;
        // Iterate the declared layout only; never search alternate roots or infer identity from titles.
        // 仅迭代声明布局；绝不搜索替代根或从标题推断身份。
        let mut pending = vec![self.clone()];
        // Physical identity cannot occur twice, even at different nesting levels.
        // 即使位于不同嵌套层，物理身份也不能重复。
        let mut seen = BTreeSet::from([self.session_id.clone()]);
        // Results retain each opaque locator rather than exposing path assembly to the service.
        // 结果保留每个不透明定位器，不向服务暴露路径拼装。
        let mut children = Vec::new();
        while let Some(parent) = pending.pop() {
            // This partition is the same constant used by embedded_task_locator.
            // 本分区与 embedded_task_locator 使用同一常量。
            let partition = parent.session_directory().join(EMBEDDED_TASK_PARTITION);
            match fs::symlink_metadata(&partition) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => {
                    return Err(SessionV1StorageError::io(&partition, error.to_string()).into());
                }
                Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
                Ok(_) => {
                    return Err(tree_error(
                        &parent,
                        "Embedded partition is not a regular directory.",
                    ));
                }
            }
            for entry in fs::read_dir(&partition)
                .map_err(|error| SessionV1StorageError::io(&partition, error.to_string()))?
            {
                // Never skip unreadable or unknown entries inside a directory about to be deleted.
                // 对即将删除目录中的不可读或未知条目绝不跳过。
                let entry = entry
                    .map_err(|error| SessionV1StorageError::io(&partition, error.to_string()))?;
                // File type is checked without following a symbolic link.
                // 检查文件类型时不跟随符号链接。
                let kind = entry
                    .file_type()
                    .map_err(|error| SessionV1StorageError::io(entry.path(), error.to_string()))?;
                if !kind.is_dir() || kind.is_symlink() {
                    return Err(tree_error(
                        &parent,
                        "Embedded child is not a regular directory.",
                    ));
                }
                // Decode the exact name produced by the authoritative layout constructor.
                // 解码权威布局构造器产生的精确名称。
                let name = entry.file_name();
                // Non-UTF8 or malformed names are explicit corruption, not absent sessions.
                // 非 UTF8 或畸形名称属于明确损坏，不视为会话不存在。
                let id = name
                    .to_str()
                    .and_then(|name| name.strip_prefix(&format!("{EMBEDDED_TASK_BUCKET}-")))
                    .ok_or_else(|| {
                        tree_error(&parent, "Malformed embedded session directory name.")
                    })?;
                // Derivation validates the identity and preserves the root-owned project directory.
                // 派生校验身份，并保留根所属项目目录。
                let child = embedded_task_locator(&parent, id)?;
                if child.session_directory() != entry.path() || !seen.insert(id.to_owned()) {
                    return Err(tree_error(
                        &parent,
                        "Duplicate or mismatched embedded session identity.",
                    ));
                }
                child.read_indexed_metadata()?;
                pending.push(child.clone());
                children.push(child);
            }
        }
        children.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
        Ok(children)
    }
}

/// Attributes invalid physical tree structure to its exact `parent` locator.
/// 将无效物理树结构归因于精确 `parent` 定位器。
/// `detail` is a fixed diagnostic; returns the existing typed locator error.
/// `detail` 是固定诊断；返回已有强类型定位器错误。
fn tree_error(parent: &SessionV1StorageLocator, detail: &str) -> SessionV1StoreError {
    SessionIndexV1Error::InvalidLocator {
        session_id: Some(parent.session_id.clone()),
        detail: detail.to_owned(),
    }
    .into()
}
