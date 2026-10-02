//! The same original guard records callback configuration intent before actual atomic publication.
//! 同一原屏障在实际原子发布之前记录回调配置意图。

use super::*;
use std::fs;
use std::path::PathBuf;

/// Only an actual first-install reservation may bind originally absent mutable configuration.
/// 仅实际首次安装预留能够绑定原缺失可变配置。
pub(super) struct InitialConfigurationOwnership {
    /// Exact host-selected state path is never reconstructed from a public status.
    /// 精确宿主所选状态路径绝不从公共状态重建。
    path: PathBuf,
    /// Actual bytes observed before the latest admitted write are one permitted uncertain outcome.
    /// 最新已准入写入之前观测的实际字节为单个允许不确定结果。
    before: Option<Vec<u8>>,
    /// Exact latest intended bytes are retained before the writer can publish them.
    /// 精确最新期望字节在写入器能够发布之前保留。
    intended: Option<Vec<u8>>,
    /// Compensation admits absence only after its actual delete has been authorized.
    /// 仅实际删除已获授权之后，补偿才准入缺失。
    removal_started: bool,
    /// Confirmed original absence rejects all later writes through the same finished owner.
    /// 已确认原缺失拒绝通过同一已结束所有者进行全部随后写入。
    restored: bool,
}

/// Read exact regular state bytes, rejecting redirects and every error other than actual absence.
/// 读取精确普通状态字节，拒绝重定向及实际缺失之外的每个错误。
/// path is the retained original host path; returns optional actual bytes without mutation.
/// path 为保留原宿主路径；返回可选实际字节，不进行变更。
fn read_original(path: &Path) -> Result<Option<Vec<u8>>> {
    // Both immediate host state directories remain plain before file access.
    // 文件访问之前，两个直接宿主状态目录保持普通目录。
    for parent in path
        .parent()
        .into_iter()
        .chain(path.parent().and_then(Path::parent))
    {
        match fs::symlink_metadata(parent) {
            Ok(metadata)
                if !crate::metadata_is_link_or_reparse_point(&metadata) && metadata.is_dir() => {}
            Ok(_) => {
                return Err(Error::new(
                    Code::Conflict,
                    "Initial configuration parent is not a plain directory.",
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(Error::io("Inspect original configuration parent", error)),
        }
    }
    match fs::symlink_metadata(path) {
        Ok(metadata)
            if !crate::metadata_is_link_or_reparse_point(&metadata) && metadata.is_file() =>
        {
            fs::read(path)
                .map(Some)
                .map_err(|error| Error::io("Read original configuration", error))
        }
        Ok(_) => Err(Error::new(
            Code::Conflict,
            "Initial configuration is not a plain file.",
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(Error::io("Inspect original configuration", error)),
    }
}

impl VulcanPluginLegacyMutationGuard {
    /// Bind the actual initial reservation to its absent configuration before copying or callback registration.
    /// 在复制或登记回调之前，将实际初始预留绑定至其缺失配置。
    /// path is the manager's exact state path; returns unit only for absent configuration and a previously unbound owner.
    /// path 为管理器精确状态路径；仅配置缺失且所有者此前未绑定时返回空值。
    pub(crate) fn bind_initial_installation_configuration(&self, path: &Path) -> Result<()> {
        self.validate_plugin(&self.plugin_id)?;
        if self.original.is_some() || read_original(path)?.is_some() {
            return Err(Error::new(
                Code::Conflict,
                "Initial configuration binding requires actual original absence.",
            ));
        }
        // The same physical owner carries a single non-serialized configuration ledger.
        // 同一物理所有者携带单个不可序列化配置记录。
        let mut configuration = self.configuration.lock().map_err(|_| {
            Error::new(
                Code::Conflict,
                "Original configuration ownership is poisoned.",
            )
        })?;
        if configuration.is_some() {
            return Err(Error::new(
                Code::Conflict,
                "Original initial configuration was already bound.",
            ));
        }
        *configuration = Some(InitialConfigurationOwnership {
            path: path.to_owned(),
            before: None,
            intended: None,
            removal_started: false,
            restored: false,
        });
        Ok(())
    }

    /// Record exact original-or-intended bytes before this guard's sole actual atomic configuration write.
    /// 在此屏障唯一实际原子配置写入之前记录精确原字节或期望字节。
    /// Unbound ordinary guards use the same writer; bound initial guards reject changed files and retain every uncertain write intent.
    /// 未绑定普通屏障使用同一写入器；已绑定初始屏障拒绝变更文件，并保留每个不确定写入意图。
    pub(crate) fn write_legacy_configuration_atomically(
        &self,
        path: &Path,
        encoded: &[u8],
    ) -> Result<()> {
        self.validate_plugin(&self.plugin_id)?;
        // Publication and compensation borrow the same short ledger exclusion across the actual synchronous effect.
        // 发布及补偿跨实际同步副作用借用同一短暂记录排他区间。
        let mut configuration = self.configuration.lock().map_err(|_| {
            Error::new(
                Code::Conflict,
                "Original configuration ownership is poisoned.",
            )
        })?;
        if let Some(original) = configuration.as_mut() {
            if original.path != path || original.restored || original.removal_started {
                return Err(Error::new(
                    Code::Conflict,
                    "Original initial configuration cannot admit this write.",
                ));
            }
            // Precisely the last before-image and intended image are legitimate uncertain publication outcomes.
            // 精确最新前像及期望像为合法不确定发布结果。
            let actual = read_original(path)?;
            if actual != original.before && actual != original.intended {
                return Err(Error::new(
                    Code::Conflict,
                    "Original initial configuration changed outside its retained write intent.",
                ));
            }
            original.before = actual;
            original.intended = Some(encoded.to_vec());
        }
        crate::atomic_file::write_file_atomically(path, encoded)
            .map_err(|error| Error::new(Code::Storage, error))
    }

    /// Restore bound original absence only from actual retained write outcomes after native and callback retirement.
    /// 原生及回调退役之后，仅从实际保留写入结果恢复已绑定原缺失。
    /// path must match the same first-install binding; conflicts preserve both file bytes and original ownership.
    /// path 必须匹配同一首次安装绑定；冲突同时保留文件字节及原所有权。
    pub(crate) fn restore_initial_installation_configuration(&self, path: &Path) -> Result<()> {
        self.validate_plugin(&self.plugin_id)?;
        // No subsequent callback may write through this owner once actual absence is confirmed.
        // 实际缺失确认之后，随后回调不得通过此所有者写入。
        let mut configuration = self.configuration.lock().map_err(|_| {
            Error::new(
                Code::Conflict,
                "Original configuration ownership is poisoned.",
            )
        })?;
        // Original authority or outcome is retained rather than reconstructed.
        // 原权威或结果被保留，而非重建。
        let original = configuration.as_mut().ok_or_else(|| {
            Error::new(
                Code::Conflict,
                "Original initial configuration binding is absent.",
            )
        })?;
        if original.path != path {
            return Err(Error::new(
                Code::Conflict,
                "Original initial configuration path changed.",
            ));
        }
        // Actual original storage evidence is required before confirmation.
        // 确认之前需要实际原存储证据。
        let actual = read_original(path)?;
        if original.restored {
            return if actual.is_none() {
                Ok(())
            } else {
                Err(Error::new(
                    Code::Conflict,
                    "Restored initial configuration is no longer absent.",
                ))
            };
        }
        if !(original.removal_started && actual.is_none())
            && actual != original.before
            && actual != original.intended
        {
            return Err(Error::new(
                Code::Conflict,
                "Original initial configuration has an unowned current image.",
            ));
        }
        original.removal_started = true;
        if actual.is_some() {
            fs::remove_file(path)
                .map_err(|error| Error::io("Remove original installation configuration", error))?;
        }
        if read_original(path)?.is_some() {
            return Err(Error::new(
                Code::Conflict,
                "Original initial configuration absence was not confirmed.",
            ));
        }
        original.restored = true;
        Ok(())
    }
}
