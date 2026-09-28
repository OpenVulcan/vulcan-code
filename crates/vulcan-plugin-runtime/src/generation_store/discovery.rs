//! Discovery of existing managed selections without creating a generation store.
//! 发现既有受管选择，不创建代次存储。

use super::{STORE_DIRECTORY, VulcanPluginGenerationStore, VulcanPluginStoreError as Error};
use super::{VulcanPluginStoreResult as Result, files};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

impl VulcanPluginGenerationStore {
    /// Opens the existing store below `user_root`; absence returns `None`, invalid storage returns an error.
    /// 打开 `user_root` 下的既有存储；缺失返回 `None`，无效存储返回错误。
    /// This discovery operation never creates directories or repairs incomplete metadata.
    /// 此发现操作不创建目录，也不修复不完整的元数据。
    pub fn open_existing(user_root: &Path) -> Result<Option<Self>> {
        if !files::exists(user_root, true)? {
            return Ok(None);
        }
        // Canonicalize only after rejecting a redirected literal root.
        // 先拒绝字面根路径重定向，再取得规范路径。
        let user_root = user_root
            .canonicalize()
            .map_err(|error| Error::io("Resolve user plugin root", error))?;
        // The fixed layout is the only recognized storage location.
        // 固定布局是唯一识别的存储位置。
        let root = user_root.join(STORE_DIRECTORY);
        if !files::exists(&root, true)? {
            return Ok(None);
        }
        // Validate every required directory without initializing missing children.
        // 校验全部必需目录，不初始化缺失的子目录。
        let store = Self { root };
        store.validate_layout()?;
        Ok(Some(store))
    }

    /// Lists canonical activation namespaces, including tombstones; callers must pin each actual selection.
    /// 列出规范激活命名空间，包含停用记录；调用者必须进一步固定实际选择。
    /// Returns an error for enumeration or namespace failures; this list is not an atomic catalog snapshot.
    /// 枚举或命名空间失败时返回错误；此列表不是原子的目录快照。
    pub fn activation_plugin_ids(&self) -> Result<BTreeSet<String>> {
        self.validate_layout()?;
        // Only final JSON records are authoritative; atomic writer staging files end in `.tmp`.
        // 仅最终 JSON 记录具有权威性；原子写入器的暂存文件以 `.tmp` 结尾。
        let entries = fs::read_dir(self.root.join("active"))
            .map_err(|error| Error::io("Enumerate plugin activations", error))?;
        // Stable ordering makes independent catalog projections deterministic.
        // 稳定排序使独立目录投影保持确定性。
        let mut plugin_ids = BTreeSet::new();
        for entry in entries {
            // Enumeration failures are not evidence of an absent activation.
            // 枚举失败不表示激活记录缺失。
            let path = entry
                .map_err(|error| Error::io("Read plugin activation entry", error))?
                .path();
            if path.extension() != Some(std::ffi::OsStr::new("json")) {
                continue;
            }
            // The filename must identify exactly one valid plugin before its body is read.
            // 读取正文前，文件名必须精确标识一个有效插件。
            let plugin_id = path
                .file_stem()
                .and_then(|name| name.to_str())
                .ok_or_else(|| Error::invalid("Activation record filename is not UTF-8."))?;
            crate::catalog::validate_identifier(plugin_id, "activation plugin id")
                .map_err(Error::invalid)?;
            plugin_ids.insert(plugin_id.to_owned());
        }
        Ok(plugin_ids)
    }

    /// Identifies the reserved store entry by its literal name, even when that entry is invalid.
    /// 按字面名称识别存储保留条目，即使该条目无效。
    /// `entry` is a direct user-root child; the result prevents treating private storage as a plugin.
    /// `entry` 是用户根的直接子条目；返回值用于避免把私有存储当作插件。
    pub(crate) fn is_store_entry(entry: &Path) -> bool {
        entry.file_name() == Some(std::ffi::OsStr::new(STORE_DIRECTORY))
    }
}
