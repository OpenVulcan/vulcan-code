//! Read-only discovery recovers original journal identities after every process-local handle has been lost.
//! 只读发现用于在全部进程内句柄丢失后恢复原日志身份。

use super::*;
use crate::generation_store::{
    VulcanPluginStoreError as Error, VulcanPluginStoreResult as Result, files,
};
use std::fs;

impl VulcanPluginGenerationStore {
    /// Enumerate plugin namespaces with credential inventories without creating layout or performing backend operations.
    /// 枚举具有凭据清单的插件命名空间，不创建布局，也不执行后端操作。
    /// The result is not a cross-plugin atomic snapshot; each discovered revision must be read and validated separately.
    /// 结果不是跨插件原子快照；每个已发现修订均须独立读取及校验。
    pub fn credential_plugin_ids(&self) -> Result<BTreeSet<String>> {
        self.validate_layout()?;
        // Older stores have no inventory directory until their first explicit preparation.
        // 旧存储在首次显式准备前没有清单目录。
        let root = self.root.join(super::operations::INVENTORY_DIRECTORY);
        if !files::exists(&root, true)? {
            return Ok(BTreeSet::new());
        }
        // Stable ordering makes recovery reports deterministic without guessing a current revision.
        // 稳定排序使恢复报告保持确定性，不猜测当前修订。
        let mut ids = BTreeSet::new();
        for entry in fs::read_dir(&root)
            .map_err(|error| Error::io("Enumerate credential plugin inventories", error))?
        {
            // Every plugin directory must be a validated non-redirected direct child.
            // 每个插件目录必须是已校验且不重定向的直接子项。
            let entry = entry
                .map_err(|error| Error::io("Read credential plugin inventory entry", error))?;
            // Non-UTF-8 names cannot identify a declared plugin and remain explicit corruption.
            // 非 UTF-8 名称无法标识声明插件，保持为显式损坏。
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| Error::invalid("Credential plugin inventory name is not UTF-8."))?;
            crate::catalog::validate_identifier(&name, "credential plugin id")
                .map_err(Error::invalid)?;
            if !files::exists(&entry.path(), true)? {
                return Err(Error::invalid(
                    "Enumerated credential plugin inventory disappeared.",
                ));
            }
            ids.insert(name);
        }
        Ok(ids)
    }

    /// Enumerate original revision IDs for `plugin_id` under its read fence; final JSON records alone own recovery authority.
    /// 在读取屏障下枚举 `plugin_id` 的原修订标识；仅最终 JSON 记录持有恢复权威。
    /// Returns canonical IDs including completed tombstones; ordinary discovery never resumes writes or cleanup.
    /// 返回规范标识，包含已完成停用记录；普通发现绝不恢复写入或清理。
    pub fn credential_revisions(
        &self,
        plugin_id: &str,
    ) -> Result<BTreeSet<VulcanPluginCredentialRevision>> {
        // This is the same namespace lock used when a first inventory or final checkpoint is published.
        // 此命名空间锁与首份清单或最终检查点发布时使用的相同。
        let _namespace = self.namespace_lock(plugin_id, true)?;
        self.credential_revisions_locked(plugin_id)
    }

    /// Enumerate the same canonical journal filenames while an actual caller already retains this plugin namespace.
    /// 实际调用方已保留此插件命名空间期间，枚举相同规范日志文件名。
    /// plugin_id is validated by that retained fence; returns exact final revision IDs without reacquiring a lock or accessing secrets.
    /// plugin_id 由保留屏障校验；返回精确最终修订标识，不重新取得锁或访问秘密。
    pub(in crate::generation_store) fn credential_revisions_locked(
        &self,
        plugin_id: &str,
    ) -> Result<BTreeSet<VulcanPluginCredentialRevision>> {
        self.validate_layout()?;
        // Absence is permitted only for an optional inventory directory that has not been created.
        // 仅尚未创建的可选清单目录允许缺失。
        let Some(directory) = self.credential_directory(plugin_id)? else {
            return Ok(BTreeSet::new());
        };
        // A deterministic set preserves each original identity without selecting a replacement.
        // 确定集合保留每个原身份，不选择替代身份。
        let mut revisions = BTreeSet::new();
        for entry in fs::read_dir(directory)
            .map_err(|error| Error::io("Enumerate credential revisions", error))?
        {
            // Atomic writer temporary files are not journals and do not authorize backend access.
            // 原子写入器临时文件不是日志，不能授权后端访问。
            let path = entry
                .map_err(|error| Error::io("Read credential revision entry", error))?
                .path();
            if path.extension() != Some(std::ffi::OsStr::new("json")) {
                // Case aliases are real journals on case-insensitive filesystems and must never disappear from a complete census.
                // 大小写别名在大小写不敏感文件系统上为实际日志，绝不能从完整清单中消失。
                if path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
                {
                    return Err(Error::invalid(
                        "Credential revision filename has a noncanonical JSON extension.",
                    ));
                }
                continue;
            }
            if !files::exists(&path, false)? {
                return Err(Error::invalid(
                    "Enumerated credential revision disappeared.",
                ));
            }
            // Canonical UUID parsing rejects aliases before a caller can construct any recovery path.
            // 规范 UUID 解析在调用方构造任何恢复路径前拒绝别名。
            let name = path
                .file_stem()
                .and_then(|name| name.to_str())
                .ok_or_else(|| Error::invalid("Credential revision filename is not UTF-8."))?;
            revisions.insert(VulcanPluginCredentialRevision::parse(name).map_err(Error::invalid)?);
        }
        Ok(revisions)
    }
}
