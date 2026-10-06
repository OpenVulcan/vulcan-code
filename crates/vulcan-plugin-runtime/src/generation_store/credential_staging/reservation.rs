//! The original credential file and parent ownership exist before the first inventory writer executes.
//! 原凭据文件及父所有权在首次清单写入器执行之前存在。

use super::operations::{INVENTORY_DIRECTORY, validate_record};
use super::*;
use crate::generation_store::{
    PinnedVulcanPluginGeneration, VulcanPluginStoreError as Error,
    VulcanPluginStoreErrorCode as Code, VulcanPluginStoreResult as Result, files,
};

mod inspection;

/// Classify invalid original-owned record substitutions as Conflict; preserve actual storage and lease errors.
/// 将无效原所有者记录替换分类为 Conflict；保留实际存储及租约错误。
/// The input is one strict record validation/read error; the returned error retains no foreign secret-bearing bytes.
/// 输入为单个严格记录校验／读取错误；返回错误不保留外部含秘密字节。
pub(super) fn owned_record_error(error: Error) -> Error {
    if error.code == Code::Invalid {
        Error::new(
            Code::Conflict,
            "Original owned credential inventory failed strict record validation.",
        )
    } else {
        error
    }
}

/// A non-cloneable actual staging writer reserved before its intended inventory is durable.
/// 预定清单持久化之前预留的不可克隆实际暂存写入者。
/// The inner exclusive revision File never changes ownership identity or becomes an Arc.
/// 内部排他修订 File 绝不改变所有权身份，也不转换为 Arc。
pub struct VulcanPluginCredentialStagingReservation {
    /// Original actual writer and immutable non-secret inventory; not exposed as an already durable record.
    /// 原实际写入者及不可变非秘密清单；不暴露为已经持久化的记录。
    staging: VulcanPluginCredentialStaging,
    /// Actual namespace file exists until first inventory confirmation, without a parallel publication flag.
    /// 实际命名空间文件存在直到首次清单确认，不设并行发布标记。
    namespace: Option<File>,
}

impl std::fmt::Debug for VulcanPluginCredentialStagingReservation {
    /// Format plugin/revision identities through formatter; return status without intended phases, keys or values.
    /// 通过 formatter 格式化插件／修订身份；返回状态，不包含预定阶段、键或值。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("VulcanPluginCredentialStagingReservation")
            .field("plugin_id", &self.staging.record.plugin_id)
            .field("revision", &self.staging.record.revision)
            .finish_non_exhaustive()
    }
}

impl VulcanPluginCredentialStagingReservation {
    /// Observe whether this owner currently retains its actual first-publication namespace File; return physical presence only.
    /// 观察此所有者当前是否保留实际首次发布命名空间 File；仅返回物理存在性。
    /// This observation grants no lock or publication/recovery authority and proves no durable credential phase.
    /// 此观察不授予锁或发布／恢复权威，也不证明持久凭据阶段。
    pub fn holds_publication_namespace(&self) -> bool {
        self.namespace.is_some()
    }

    /// Borrow the originally reserved revision; this grants no durable inventory claim or backend access.
    /// 借用原预留修订；不授予持久清单声明或后端访问。
    pub fn revision(&self) -> &VulcanPluginCredentialRevision {
        &self.staging.record.revision
    }

    /// Confirm the same inner writer's actual record after namespace retirement; return actual non-secret progress only.
    /// 命名空间退役后确认同一内部写入者实际记录；仅返回实际非秘密进度。
    /// A still-held first-publication namespace is Busy; no backend operation or lock reopening is performed here.
    /// 仍持有首次发布命名空间时返回 Busy；此处不执行后端操作或重新打开锁。
    pub fn reconcile_owned(&mut self) -> Result<VulcanPluginCredentialRecord> {
        if self.namespace.is_some() {
            return Err(Error::new(
                Code::Busy,
                "Credential inventory publication still owns its original namespace.",
            ));
        }
        self.staging.reconcile_owned()
    }
}

impl VulcanPluginGenerationStore {
    /// Reserve exact original preparation/generation/values before any first inventory write; return the actual writer owner.
    /// 在任何首份清单写入之前预留精确原 preparation／generation／values；返回实际写入者所有者。
    /// Values are validated but never retained, serialized or included in diagnostics; the service must freeze its original map.
    /// 值被校验但绝不保留、序列化或写入诊断；服务必须固定其原映射。
    pub fn reserve_activation_credential_staging(
        &self,
        preparation: &crate::VulcanPluginActivationPreparation,
        generation: &PinnedVulcanPluginGeneration,
        values: &VulcanPluginCredentialValues,
    ) -> Result<VulcanPluginCredentialStagingReservation> {
        self.reserve_owned_credential_staging(
            generation,
            preparation.intent().credential_revision().clone(),
            values,
            Some(preparation),
        )
    }

    /// Preserve the existing standalone or original-parent validations while reserving the real namespace and revision files.
    /// 预留真实命名空间及修订文件时，保留既有独立或原父级校验。
    /// Arguments fix package/revision/values/optional original parent; return an owner without writing its intended inventory.
    /// 参数固定插件包／修订／值／可选原父级；返回所有者而不写入其预定清单。
    pub(super) fn reserve_owned_credential_staging(
        &self,
        generation: &PinnedVulcanPluginGeneration,
        revision: VulcanPluginCredentialRevision,
        values: &VulcanPluginCredentialValues,
        preparation: Option<&crate::VulcanPluginActivationPreparation>,
    ) -> Result<VulcanPluginCredentialStagingReservation> {
        if generation.store_root != self.root {
            return Err(Error::invalid(
                "Credential candidate belongs to another generation store.",
            ));
        }
        // A real namespace spans service retention and publication, including when no inventory exists yet.
        // 真实命名空间覆盖服务保留及发布，包括清单尚不存在时。
        let namespace = self.namespace_lock(&generation.identity.plugin_id, false)?;
        if self
            .read_activation(&generation.identity.plugin_id)?
            .as_ref()
            .and_then(|record| record.credential_revision())
            == Some(&revision)
        {
            return Err(Error::new(
                Code::Conflict,
                "A referenced credential revision cannot become a new staging candidate.",
            ));
        }
        // Retain the exact original parent File before any inventory or secure-backend effect becomes visible.
        // 在任何清单或安全后端副作用可见之前保留精确原父 File。
        let activation_request_lease = match preparation {
            Some(preparation) => Some(self.retain_preparation_credentials(
                preparation,
                &generation.identity,
                &revision,
            )?),
            None => {
                self.require_unreserved_credential_revision(
                    &generation.identity.plugin_id,
                    &revision,
                )?;
                None
            }
        };
        // Verify actual package bytes before deriving the canonical inventory and configured subset.
        // 派生规范清单及已配置子集之前校验实际插件包字节。
        let verified = self.pin_locked(&generation.identity)?;
        // Only declared manifest keys may enter the original non-secret cleanup inventory.
        // 仅声明清单键可进入原非秘密清理清单。
        let keys = verified.credential_keys;
        if values.keys().ne(keys.iter()) || values.values().flatten().any(String::is_empty) {
            return Err(Error::invalid(
                "Credential values must name every declared slot exactly; configured values cannot be empty.",
            ));
        }
        // Phase is merely intended inside this private owner until the actual first writer is confirmed.
        // 阶段在实际首次写入器被确认之前，仅在此私有所有者内属于预定内容。
        let record = VulcanPluginCredentialRecord {
            schema_version: if preparation.is_some() {
                ACTIVATION_CREDENTIAL_RECORD_VERSION
            } else {
                CREDENTIAL_RECORD_VERSION
            },
            activation_request: preparation.map(|original| original.request_id()),
            plugin_id: generation.identity.plugin_id.clone(),
            package: VulcanPluginCredentialPackage::Generation {
                identity: generation.identity.clone(),
            },
            revision,
            keys,
            configured_keys: configured_keys(values).into_iter().collect(),
            phase: VulcanPluginCredentialPhase::Preparing,
        };
        self.reserve_credential_record_locked(namespace, record, activation_request_lease)
    }

    /// Stage a new credential revision for the bundled package of `plugin_id`; return the confirmed writer.
    /// 为 `plugin_id` 的内建插件包暂存新凭据修订；返回已确认写入者。
    /// `keys` is the bundled package's declared inventory from the host catalog and `values` must name each exactly.
    /// `keys` 为宿主目录提供的内建插件包声明清单，`values` 必须逐一精确对应。
    /// A namespace owned by a managed selection is a conflict; no secure backend is called here.
    /// 由受管选择占用的命名空间视为冲突；此处不调用安全后端。
    pub fn begin_bundled_credential_staging(
        &self,
        plugin_id: &str,
        keys: Vec<VulcanPluginCredentialKey>,
        revision: VulcanPluginCredentialRevision,
        values: &VulcanPluginCredentialValues,
    ) -> Result<VulcanPluginCredentialStaging> {
        let namespace = self.namespace_lock(plugin_id, false)?;
        let current = self.read_activation(plugin_id)?;
        if current
            .as_ref()
            .is_some_and(|record| record.package != crate::VulcanPluginPackageSelection::Bundled {})
        {
            return Err(Error::new(
                Code::Conflict,
                "Credentials of a non-bundled selection are staged for its generation.",
            ));
        }
        if current
            .as_ref()
            .and_then(|record| record.credential_revision())
            == Some(&revision)
        {
            return Err(Error::new(
                Code::Conflict,
                "A referenced credential revision cannot become a new staging candidate.",
            ));
        }
        self.require_unreserved_credential_revision(plugin_id, &revision)?;
        if values.keys().ne(keys.iter()) || values.values().flatten().any(String::is_empty) {
            return Err(Error::invalid(
                "Credential values must name every declared slot exactly; configured values cannot be empty.",
            ));
        }
        let record = VulcanPluginCredentialRecord {
            schema_version: CREDENTIAL_RECORD_VERSION,
            activation_request: None,
            plugin_id: plugin_id.to_owned(),
            package: VulcanPluginCredentialPackage::Bundled {},
            revision,
            keys,
            configured_keys: configured_keys(values).into_iter().collect(),
            phase: VulcanPluginCredentialPhase::Preparing,
        };
        let mut reservation = self.reserve_credential_record_locked(namespace, record, None)?;
        self.publish_owned_credential_staging_retained(&mut reservation, None)?;
        self.confirmed_credential_staging_reservation(reservation)
    }

    /// Reserve the intended `record` under the caller's exclusive `namespace`; return its owner without writing it.
    /// 在调用方排他 `namespace` 下预留预定 `record`；返回其所有者而不写入记录。
    /// `activation_request_lease` is the already retained original parent, if any.
    /// `activation_request_lease` 为已保留的原父请求（如有）。
    fn reserve_credential_record_locked(
        &self,
        namespace: File,
        record: VulcanPluginCredentialRecord,
        activation_request_lease: Option<Arc<File>>,
    ) -> Result<VulcanPluginCredentialStagingReservation> {
        validate_record(&record, &record.plugin_id, &record.revision)?;
        // Preserve the original exclusive File rather than duplicating or reopening its revision ownership.
        // 保留原排他 File，而非复制或重新打开其修订所有权。
        let lease = self.credential_lease(&record.plugin_id, &record.revision, false)?;
        // Explicit reservation initializes only the existing optional inventory layout, without publishing a record.
        // 明确预留仅初始化既有可选清单布局，不发布记录。
        let root = files::ensure_child(&self.root, INVENTORY_DIRECTORY)?;
        // Every immutable revision belongs to its one validated plugin component.
        // 每个不可变修订属于其唯一已校验插件组件。
        let directory = files::ensure_child(&root, &record.plugin_id)?;
        if files::exists(
            &directory.join(format!("{}.json", record.revision.as_str())),
            false,
        )? {
            return Err(Error::new(
                Code::Conflict,
                "Credential revision already has a durable inventory; inspect that original revision.",
            ));
        }
        files::sync_directory(&root)?;
        files::sync_directory(&self.root)?;
        Ok(VulcanPluginCredentialStagingReservation {
            staging: VulcanPluginCredentialStaging {
                store: self.clone(),
                record,
                _lease: lease,
                _activation_request_lease: activation_request_lease,
            },
            namespace: Some(namespace),
        })
    }

    /// Publish or confirm reservation's original inventory using its actual preparation; return a borrow of the same inner writer.
    /// 使用实际 preparation 发布或确认 reservation 原清单；返回同一内部写入者借用。
    /// Success retires the actual first-publication namespace before backend work; errors retain all original physical owners.
    /// 成功在后端工作之前退役实际首次发布命名空间；错误保留全部原物理所有者。
    pub fn publish_activation_credential_staging_retained<'reservation>(
        &self,
        reservation: &'reservation mut VulcanPluginCredentialStagingReservation,
        preparation: &crate::VulcanPluginActivationPreparation,
    ) -> Result<&'reservation mut VulcanPluginCredentialStaging> {
        self.publish_owned_credential_staging_retained(reservation, Some(preparation))
    }

    /// Share one actual first inventory writer for declared standalone or original-parent staging.
    /// 为声明独立或原父级暂存共用单个实际首次清单写入器。
    /// Return the same inner File owner after exact readback; never recreate vanished inventory after namespace retirement.
    /// 精确回读后返回同一内部 File 所有者；命名空间退役之后绝不重新创建消失清单。
    pub(super) fn publish_owned_credential_staging_retained<'reservation>(
        &self,
        reservation: &'reservation mut VulcanPluginCredentialStagingReservation,
        preparation: Option<&crate::VulcanPluginActivationPreparation>,
    ) -> Result<&'reservation mut VulcanPluginCredentialStaging> {
        if reservation.staging.store.root != self.root {
            return Err(Error::new(
                Code::Conflict,
                "Credential reservation belongs to another original store.",
            ));
        }
        if reservation.staging.record.phase != VulcanPluginCredentialPhase::Preparing {
            return Err(Error::new(
                Code::Conflict,
                "Credential inventory publication requires its original Preparing owner.",
            ));
        }
        validate_record(
            &reservation.staging.record,
            &reservation.staging.record.plugin_id,
            &reservation.staging.record.revision,
        )
        .map_err(owned_record_error)?;
        // A present actual File already owns the namespace; only a retired File requires a short new observation fence.
        // 存在实际 File 时已经持有命名空间；仅 File 已退役时才要求短新观测屏障。
        let _observation_namespace = if reservation.namespace.is_none() {
            Some(self.namespace_lock(&reservation.staging.record.plugin_id, false)?)
        } else {
            None
        };
        match (&reservation.staging._activation_request_lease, preparation) {
            (Some(retained), Some(original))
                if reservation.staging.record.activation_request == Some(original.request_id()) =>
            {
                // Existing validation checks the complete original intent, predecessor and index before this pointer proof.
                // 既有校验在此指针证明之前检查完整原意图、前驱及索引。
                let identity = reservation.staging.record.generation().ok_or_else(|| {
                    Error::invalid("A parented credential inventory names no generation.")
                })?;
                let observed_parent = self.retain_preparation_credentials(
                    original,
                    identity,
                    &reservation.staging.record.revision,
                )?;
                // A copied UUID alone cannot replace the actual original request File shared by the reserved writer.
                // 仅复制 UUID 不能替换预留写入者共享的实际原请求 File。
                let same_parent = Arc::ptr_eq(retained, &observed_parent);
                drop(observed_parent);
                if !same_parent {
                    return Err(Error::new(
                        Code::Conflict,
                        "Credential reservation no longer shares its exact original parent file.",
                    ));
                }
            }
            (None, None) if reservation.staging.record.activation_request.is_none() => {
                self.require_unreserved_credential_revision(
                    &reservation.staging.record.plugin_id,
                    &reservation.staging.record.revision,
                )?;
            }
            _ => {
                return Err(Error::new(
                    Code::Conflict,
                    "Credential reservation differs from its declared original parent ownership.",
                ));
            }
        }
        #[cfg(test)]
        observation_before_readback(&self.root, &reservation.staging.record)?;
        // Genuine absence permits only the first retained namespace owner to publish; existing exact Preparing is read-only.
        // 真实缺失仅允许首次保留命名空间所有者发布；既有精确 Preparing 属于只读确认。
        let observed = self
            .read_credential_record_locked(
                &reservation.staging.record.plugin_id,
                &reservation.staging.record.revision,
            )
            .map_err(owned_record_error)?;
        match observed {
            None if reservation.namespace.is_some() => {
                self.write_credential_record_locked(&reservation.staging.record)?;
                #[cfg(test)]
                observation_after_write(&self.root, &reservation.staging.record)?;
            }
            Some(record) if record == reservation.staging.record => {}
            _ => {
                return Err(Error::new(
                    Code::Conflict,
                    "Credential inventory differs from its exact reserved Preparing record.",
                ));
            }
        }
        #[cfg(test)]
        observation_before_readback(&self.root, &reservation.staging.record)?;
        if self
            .read_credential_record_locked(
                &reservation.staging.record.plugin_id,
                &reservation.staging.record.revision,
            )
            .map_err(owned_record_error)?
            .as_ref()
            != Some(&reservation.staging.record)
        {
            return Err(Error::new(
                Code::Conflict,
                "First credential inventory lacks its exact original Preparing readback.",
            ));
        }
        // Drop the actual namespace before exposing the same writer to existing backend code that takes its own short fences.
        // 向会取得自身短屏障的既有后端代码暴露同一写入者之前，释放实际命名空间。
        drop(reservation.namespace.take());
        Ok(&mut reservation.staging)
    }

    /// Consume only a successfully confirmed compatibility reservation; return its same inner exclusive writer File.
    /// 仅消费成功确认的兼容预留；返回其同一内部排他写入者 File。
    /// Private callers must complete the sole publication method before invoking this move.
    /// 私有调用方必须在调用此移动之前完成唯一发布方法。
    pub(super) fn confirmed_credential_staging_reservation(
        &self,
        reservation: VulcanPluginCredentialStagingReservation,
    ) -> Result<VulcanPluginCredentialStaging> {
        if reservation.staging.store.root != self.root || reservation.namespace.is_some() {
            return Err(Error::new(
                Code::Conflict,
                "Credential staging reservation was not confirmed by its original store.",
            ));
        }
        Ok(reservation.staging)
    }
}

/// Apply a scoped acknowledgement loss only after root/record's real inventory writer already succeeded.
/// 仅在 root／record 真实清单写入器已经成功之后应用作用域确认丢失。
#[cfg(test)]
pub(super) fn observation_after_write(
    root: &std::path::Path,
    record: &VulcanPluginCredentialRecord,
) -> Result<()> {
    observation::after_write(root, record)
}

/// Apply the exact scoped original-owned readback loss without changing storage or physical ownership.
/// 应用精确作用域原已持有所有者回读丢失，不改变存储或物理所有权。
#[cfg(test)]
pub(super) fn observation_before_readback(
    root: &std::path::Path,
    record: &VulcanPluginCredentialRecord,
) -> Result<()> {
    observation::before_readback(root, record)
}

impl VulcanPluginGenerationStore {
    /// Arm plugin/revision's successful real Preparing or Staged acknowledgement loss; return same-thread cleanup.
    /// 启用 plugin／revision 成功真实 Preparing 或 Staged 确认丢失；返回同线程清理。
    /// The returned scope owns its complete key and captures no input borrow, so original owners remain mutable.
    /// 返回作用域持有完整键且不捕获输入借用，使原所有者仍可变。
    #[cfg(test)]
    pub(crate) fn lose_owned_credential_checkpoint_once_for_test(
        &self,
        plugin: &str,
        revision: &VulcanPluginCredentialRevision,
        phase: VulcanPluginCredentialPhase,
        panic: bool,
        lose_readback: bool,
    ) -> impl Drop + use<> {
        observation::arm(&self.root, plugin, revision, phase, panic, lose_readback)
    }

    /// Read scoped plugin/revision actual inventory writer counts; return no production status or authority.
    /// 读取作用域 plugin／revision 实际清单写入器次数；不返回生产状态或权威。
    #[cfg(test)]
    pub(crate) fn owned_credential_checkpoint_write_counts_for_test(
        &self,
        plugin: &str,
        revision: &VulcanPluginCredentialRevision,
    ) -> (usize, usize) {
        observation::writes(&self.root, plugin, revision)
    }
}

/// Test-only successful actual checkpoint observations preserve the real first and Staged inventory writers.
/// 仅测试的成功实际检查点观测保留真实首次及 Staged 清单写入器。
#[cfg(test)]
mod observation {
    use super::*;
    use std::path::{Path, PathBuf};

    /// One exact original revision checkpoint loss and its successful physical write counts.
    /// 单个精确原修订检查点丢失及其成功物理写入次数。
    struct Fault {
        /// Original store/plugin/revision exclude unrelated writers and observers.
        /// 原存储／插件／修订排除无关写入者及观察器。
        key: (PathBuf, String, VulcanPluginCredentialRevision),
        /// Select the actual Preparing or Staged checkpoint, without any production phase substitute.
        /// 选择实际 Preparing 或 Staged 检查点，不提供任何生产阶段替代。
        phase: VulcanPluginCredentialPhase,
        /// Raise a worker panic instead of returning one post-success acknowledgement error when true.
        /// 为 true 时触发工作 panic，而非返回一次成功后确认错误。
        panic: bool,
        /// Matching writer success activates this once and consumes its acknowledgement loss.
        /// 匹配写入器成功启用此字段一次，并消费其确认丢失。
        written: bool,
        /// Lose exactly one owned readback after the real matching writer succeeds.
        /// 真实匹配写入器成功之后，精确丢失一次已持有所有者回读。
        lose_readback: bool,
        /// Count only successfully executed actual Preparing and Staged writers.
        /// 仅统计成功执行的实际 Preparing 及 Staged 写入器。
        writes: (usize, usize),
    }
    std::thread_local! {
        /// One scoped original observation exists on this thread; production exposes no controls.
        /// 此线程存在单个作用域原观测；生产不暴露任何控制。
        static ARMED: std::cell::RefCell<Option<Fault>> = const { std::cell::RefCell::new(None) };
    }

    /// Same-thread cleanup retains an original observation across worker unwinding.
    /// 同线程清理跨工作展开保留原观测。
    pub(super) struct Scope {
        /// Clear only this exact original's remaining fault.
        /// 仅清除此精确原请求剩余故障。
        key: (PathBuf, String, VulcanPluginCredentialRevision),
        /// Cleanup cannot move to a different thread than the original observation.
        /// 清理不能移动到与原观测不同的线程。
        _thread: std::marker::PhantomData<std::rc::Rc<()>>,
    }
    impl Drop for Scope {
        /// Clear this original scope on exit; accepts no input and returns no value.
        /// 退出时清除此原作用域；无输入且不返回值。
        fn drop(&mut self) {
            ARMED.with(|armed| {
                let mut armed = armed.borrow_mut();
                if armed.as_ref().is_some_and(|fault| fault.key == self.key) {
                    armed.take();
                }
            });
        }
    }
    /// Arm root/plugin/revision's actual checkpoint loss and optional readback loss; return same-thread cleanup.
    /// 启用 root／plugin／revision 实际检查点丢失及可选回读丢失；返回同线程清理。
    pub(super) fn arm(
        root: &Path,
        plugin: &str,
        revision: &VulcanPluginCredentialRevision,
        phase: VulcanPluginCredentialPhase,
        panic: bool,
        lose_readback: bool,
    ) -> Scope {
        assert!(matches!(
            phase,
            VulcanPluginCredentialPhase::Preparing | VulcanPluginCredentialPhase::Staged
        ));
        // Observation allocates no credential inventory or physical owner.
        // 观测不分配凭据清单或物理所有者。
        let key = (root.to_path_buf(), plugin.to_owned(), revision.clone());
        ARMED.with(|armed| {
            let mut armed = armed.borrow_mut();
            assert!(
                armed.is_none(),
                "credential checkpoint observation scopes overlap"
            );
            *armed = Some(Fault {
                key: key.clone(),
                phase,
                panic,
                written: false,
                lose_readback,
                writes: (0, 0),
            });
        });
        Scope {
            key,
            _thread: std::marker::PhantomData,
        }
    }
    /// Count root/record's real successful writer and lose its matching acknowledgement once; return explicit status.
    /// 统计 root／record 真实成功写入器并丢失其匹配确认一次；返回明确状态。
    pub(super) fn after_write(root: &Path, record: &VulcanPluginCredentialRecord) -> Result<()> {
        // The real bounded writer already succeeded before this test observer can activate.
        // 此测试观察器可启用之前，真实有界写入器已经成功。
        let loss = ARMED.with(|armed| {
            let mut armed = armed.borrow_mut();
            if let Some(fault) = armed.as_mut()
                && fault.key.0 == root
                && fault.key.1 == record.plugin_id
                && fault.key.2 == record.revision
            {
                match record.phase {
                    VulcanPluginCredentialPhase::Preparing => fault.writes.0 += 1,
                    VulcanPluginCredentialPhase::Staged => fault.writes.1 += 1,
                    _ => {}
                }
                if !fault.written && fault.phase == record.phase {
                    fault.written = true;
                    Some(fault.panic)
                } else {
                    None
                }
            } else {
                None
            }
        });
        match loss {
            Some(true) => panic!("Controlled post-success original credential worker panic."),
            Some(false) => Err(Error::new(
                Code::Storage,
                "Controlled loss of successful original credential checkpoint observation.",
            )),
            None => Ok(()),
        }
    }
    /// Lose only the first exact owned root/record readback after its matching real writer; return explicit status.
    /// 仅丢失匹配真实写入器之后首次精确已持有所有者 root／record 回读；返回明确状态。
    pub(super) fn before_readback(
        root: &Path,
        record: &VulcanPluginCredentialRecord,
    ) -> Result<()> {
        // Canonical public store inspection remains unaffected; only original-owned observations use this scope.
        // 规范公开存储检查保持不受影响；仅原已持有所有者观测使用此作用域。
        let lost = ARMED.with(|armed| {
            let mut armed = armed.borrow_mut();
            if let Some(fault) = armed.as_mut()
                && fault.key.0 == root
                && fault.key.1 == record.plugin_id
                && fault.key.2 == record.revision
                && fault.written
                && fault.lose_readback
            {
                fault.lose_readback = false;
                true
            } else {
                false
            }
        });
        if lost {
            Err(Error::new(
                Code::Storage,
                "Controlled loss of original owned credential readback.",
            ))
        } else {
            Ok(())
        }
    }
    /// Read scoped root/plugin/revision actual Preparing/Staged write counts; reject absent or foreign scopes.
    /// 读取作用域 root／plugin／revision 实际 Preparing／Staged 写入次数；拒绝缺失或外部作用域。
    pub(super) fn writes(
        root: &Path,
        plugin: &str,
        revision: &VulcanPluginCredentialRevision,
    ) -> (usize, usize) {
        ARMED.with(|armed| {
            let armed = armed.borrow();
            let fault = armed
                .as_ref()
                .expect("credential checkpoint observation remains armed");
            assert_eq!(fault.key.0, root);
            assert_eq!(fault.key.1, plugin);
            assert_eq!(&fault.key.2, revision);
            fault.writes
        })
    }
}
