//! An actual dormant flat source is captured before callbacks and collected through its existing local-state original.
//! 实际休眠平铺来源在回调之前捕获，并通过既有本地状态原实例回收。

use super::*;

/// Pure exact flat-source metadata shares the state original's actual namespace/environment authority without another file owner.
/// 纯精确平铺来源元数据共享状态原实例实际命名空间／环境权威，不增加文件所有者。
pub(super) struct FlatPredecessor {
    /// The actual canonical user root bounds the one dormant package child throughout removal.
    /// 实际规范用户根全程限定单个休眠包子级。
    user_root: PathBuf,
    /// The actual plugin identity fixes an absent child as well as a present descriptor.
    /// 实际插件身份同时固定缺失子级及存在描述符。
    plugin_id: String,
    /// A real inspected descriptor or explicit original absence precedes lifecycle and policy effects.
    /// 真实已检查描述符或明确原缺失先于生命周期及策略效果。
    source: Option<crate::VulcanPluginDescriptor>,
    /// The sole exact hidden child derives from this original request and cannot become another plugin's discoverable source.
    /// 唯一精确隐藏子级来自此原请求，不能成为另一插件的可发现来源。
    tombstone: PathBuf,
    /// The same rename intent survives system failure or successful effects with lost acknowledgement.
    /// 相同重命名意图跨系统失败或成功效果但确认丢失存续。
    hide_started: bool,
    /// Irreversible deletion admission survives partial tree removal and worker unwinding.
    /// 不可逆删除准入跨部分目录树移除及工作者展开存续。
    cleanup_started: bool,
    /// Actual terminal absence is recorded before returning; resurrection never authorizes deletion replay.
    /// 实际终态缺失在返回之前记录；复活绝不授权删除重放。
    collected: bool,
}

impl FlatPredecessor {
    /// Capture only user_root/plugin_id's actual plain child under the caller's original namespace; returns inert complete metadata.
    /// 在调用方原命名空间内仅捕获 user_root／plugin_id 实际普通子级；返回未启动完整元数据。
    /// Explicit absence is retained; invalid or redirected sources return errors before effects and are never guessed from current routing.
    /// 明确缺失被保留；无效或重定向来源在效果之前返回错误，绝不从当前路由猜测。
    pub(super) fn capture(
        user_root: &Path,
        plugin_id: &str,
        request_id: Uuid,
    ) -> Result<Self, VulcanPluginRuntimeError> {
        crate::catalog::validate_identifier(plugin_id, "dormant flat plugin id")
            .map_err(VulcanPluginRuntimeError::new)?;
        require_plain_directory(user_root)?;
        // The host's actual root is canonicalized once before constructing this sole declared child.
        // 宿主实际根在构造此唯一声明子级之前被规范化一次。
        let user_root = user_root
            .canonicalize()
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        // This explicit child is the original flat catalog contract, not a candidate-path search.
        // 此明确子级为原平铺目录契约，而非候选路径搜索。
        let path = user_root.join(plugin_id);
        // Reserve one original hidden destination even when actual source absence was captured.
        // 即使捕获实际来源缺失，仍预留单个原隐藏目标。
        let tombstone = uninstallation_tombstone(&user_root, plugin_id, request_id);
        require_absent(&tombstone)?;
        // The actual independent inspector retains manifest-declared legacy slots without acquiring managed reader pins.
        // 实际独立检查器保留清单声明旧槽位，不取得受管读者固定。
        let source = if plain_directory_presence(&path)? {
            if path
                .canonicalize()
                .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?
                != path
            {
                return Err(VulcanPluginRuntimeError::new(
                    "Dormant flat source has a noncanonical boundary.",
                ));
            }
            // Inspect the actual dormant package even though current routing selects a managed generation.
            // 即使当前路由选择受管代次，仍检查实际休眠包。
            let descriptor =
                crate::inspect_user_plugin_package(&path).map_err(VulcanPluginRuntimeError::new)?;
            if descriptor.id != plugin_id
                || descriptor.package_root != path
                || descriptor.origin != VulcanPluginPackageOrigin::UserInstalled
                || descriptor.managed_package().is_some()
            {
                return Err(VulcanPluginRuntimeError::new(
                    "Dormant flat source does not match its exact original child.",
                ));
            }
            Some(descriptor)
        } else {
            None
        };
        // Both source presence states enter the same inert local original before any publication admission.
        // 两种来源存在状态在任何发布准入之前进入相同未启动本地原实例。
        let captured = Self {
            user_root,
            plugin_id: plugin_id.into(),
            source,
            tombstone,
            hide_started: false,
            cleanup_started: false,
            collected: false,
        };
        captured.validate(&captured.user_root)?;
        Ok(captured)
    }

    /// Borrow the actual pre-callback descriptor or explicit absence; no parameters and no deletion authority is granted.
    /// 借用实际回调前描述符或明确缺失；无参数，不授予删除权威。
    pub(super) fn source(&self) -> Option<&crate::VulcanPluginDescriptor> {
        self.source.as_ref()
    }

    /// Validate the same actual root and original physical phase without adopting foreign bytes or clearing business-data poison.
    /// 验证相同实际根及原物理阶段，不接纳外来字节或清除业务数据污染。
    /// user_root is the reserving manager's actual root; returns unit or a retained boundary/phase conflict.
    /// user_root 为预留管理器实际根；返回空值或保留边界／阶段冲突。
    pub(super) fn validate(&self, user_root: &Path) -> Result<(), VulcanPluginRuntimeError> {
        require_plain_directory(user_root)?;
        if user_root
            .canonicalize()
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?
            != self.user_root
        {
            return Err(VulcanPluginRuntimeError::new(
                "Dormant flat original belongs to another canonical root.",
            ));
        }
        // This fixed path remains authoritative after partial deletion; a newly discovered source can never replace it.
        // 此固定路径在部分删除之后仍具权威；新发现来源绝不能替代它。
        let path = self.user_root.join(&self.plugin_id);
        if self.collected || self.source.is_none() {
            require_absent(&path)?;
            return require_absent(&self.tombstone);
        }
        if self.cleanup_started {
            require_absent(&path)?;
            plain_directory_presence(&self.tombstone)?;
            return Ok(());
        }
        // The discoverable and hidden paths are explicit fixed protocol states, never candidate recovery searches.
        // 可发现及隐藏路径为明确固定协议状态，绝非候选恢复搜索。
        match (
            plain_directory_presence(&path)?,
            plain_directory_presence(&self.tombstone)?,
        ) {
            (true, false) => self.verify_package(&path),
            (false, true) if self.hide_started => self.verify_package(&self.tombstone),
            _ => Err(VulcanPluginRuntimeError::new(
                "Dormant flat original paths have an unowned result.",
            )),
        }
    }

    /// Verify complete original bytes at one declared source/hidden path; path must remain an ordinary canonical child of the captured root.
    /// 在单个声明来源／隐藏路径验证完整原字节；path 必须保持为捕获根的普通规范子级。
    /// Returns unit or a retained source conflict before rename or first deletion admission.
    /// 重命名或首次删除准入之前返回空值或保留来源冲突。
    fn verify_package(&self, path: &Path) -> Result<(), VulcanPluginRuntimeError> {
        require_plain_directory(path)?;
        // The present branch has an actual captured descriptor; no alternate source or current catalog is consulted.
        // 存在分支具有实际捕获描述符；不查询替代来源或当前目录。
        let source = self.source.as_ref().ok_or_else(|| {
            VulcanPluginRuntimeError::new("Dormant flat original descriptor is absent.")
        })?;
        if path.parent() != Some(self.user_root.as_path())
            || path
                .canonicalize()
                .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?
                != path
            || crate::catalog::compute_package_fingerprint(path)
                .map_err(VulcanPluginRuntimeError::new)?
                != source.content_fingerprint
        {
            return Err(VulcanPluginRuntimeError::new(
                "Dormant flat original package bytes changed.",
            ));
        }
        Ok(())
    }

    /// Collect only this fixed dormant child after the caller proves exact published selection and retains the same exclusive environment File.
    /// 调用方证明精确已发布选择并保留相同排他环境文件之后，仅回收此固定休眠子级。
    /// user_root rechecks the manager boundary and validate_original guards each admitted attempt; returns true only for newly confirmed terminal absence.
    /// user_root 重新检查管理器边界，validate_original 保护每个已准入尝试；仅新确认终态缺失返回真。
    /// The test-only observe_hidden follows a real rename and grants no production cleanup authority.
    /// 仅测试 observe_hidden 在真实重命名之后执行，不授予生产清理权威。
    pub(super) fn collect(
        &mut self,
        user_root: &Path,
        validate_original: impl Fn() -> Result<(), VulcanPluginRuntimeError>,
        #[cfg(test)] observe_hidden: impl Fn() -> Result<(), VulcanPluginRuntimeError>,
    ) -> Result<bool, VulcanPluginRuntimeError> {
        self.validate(user_root)?;
        validate_original()?;
        if self.collected {
            return Ok(false);
        }
        if self.source.is_some() && !self.cleanup_started {
            // A successful hide is recovered at this sole fixed path; changed original source bytes cannot be adopted after a failed rename.
            // 在此唯一固定路径恢复成功隐藏；失败重命名之后改变原来源字节不能被接纳。
            let path = self.user_root.join(&self.plugin_id);
            if plain_directory_presence(&path)? {
                self.verify_package(&path)?;
                require_absent(&self.tombstone)?;
                validate_original()?;
                self.hide_started = true;
                fs::rename(&path, &self.tombstone)
                    .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
                #[cfg(test)]
                observe_hidden()?;
            }
            require_absent(&path)?;
            self.verify_package(&self.tombstone)?;
            validate_original()?;
        }
        // Revalidate on every admitted attempt before removal, including recovery after an actual partial tree deletion.
        // 在每次已准入尝试的移除之前重新验证，包括实际部分目录树删除之后的恢复。
        validate_original()?;
        require_absent(&self.user_root.join(&self.plugin_id))?;
        // Deletion targets only the original hidden child, so a new discoverable package blocks rather than joins partial cleanup.
        // 删除仅寻址原隐藏子级，因此新可发现包阻挡而非加入部分清理。
        self.cleanup_started = true;
        if self.source.is_some() {
            remove_plain_owned_tree(&self.tombstone, &self.user_root)?;
        } else {
            require_absent(&self.tombstone)?;
        }
        // The sole tree producer has confirmed actual hidden absence: save terminal evidence before any fallible readback or acknowledgement.
        // 唯一目录树生产者已确认实际隐藏缺失：在任何可失败回读或确认之前保存终态证据。
        self.collected = true;
        validate_original()?;
        self.validate(user_root)?;
        Ok(true)
    }

    /// Confirm only an already collected original's actual absence without replaying deletion or introducing a new checkpoint.
    /// 仅确认已回收原实例实际缺失，不重放删除或引入新检查点。
    /// user_root is the same manager boundary; returns unit only for retained terminal evidence and current absence.
    /// user_root 为相同管理器边界；仅保留终态证据及当前缺失返回空值。
    pub(super) fn confirm(&self, user_root: &Path) -> Result<(), VulcanPluginRuntimeError> {
        if !self.collected {
            return Err(VulcanPluginRuntimeError::new(
                "Dormant flat original collection is not terminal.",
            ));
        }
        self.validate(user_root)
    }
}
