//! Original aborted updates retain their private predecessor handoff independently of activation phases.
//! 原已中止更新独立于激活阶段保留其私有前驱交接。

use super::*;
use crate::generation_store::{
    VulcanPluginStoreError as Error, VulcanPluginStoreErrorCode as Code,
};
use VulcanPluginActivationTransactionPhase as ActivationPhase;

/// Independent handoff format stores only the full original non-secret request and resource-transfer evidence.
/// 独立交接格式仅存储完整原非秘密请求及资源移交证据。
const PREDECESSOR_HANDOFF_VERSION: u32 = 1;
/// One strict directory keeps reservation records separate from the canonical activation request inventory.
/// 单个严格目录使保留记录独立于规范激活请求清单。
const PREDECESSOR_HANDOFF_DIRECTORY: &str = "predecessor-handoffs";

/// Actual predecessor resource transfer, distinct from whether its target activation was aborted.
/// 实际前驱资源移交，与目标激活是否中止相互独立。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VulcanPluginPredecessorHandoffPhase {
    /// The original predecessor remains closed until its same live private owners are acknowledged.
    /// 在确认相同存活私有所有者前，原前驱保持关闭。
    Pending,
    /// The original manager acknowledged the actual complete private resource transfer.
    /// 原管理器已确认实际完整私有资源移交。
    Transferred,
}

/// Read-only durable evidence never grants a new instance, callback replay, or release permission.
/// 只读持久证据绝不授予新实例、回调重放或释放权限。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VulcanPluginPredecessorHandoffRecord {
    /// Independent schema is validated before any ownership decision.
    /// 任何所有权决策前都校验独立模式。
    schema_version: u32,
    /// The complete immutable original includes its exact predecessor, target and preparation policy.
    /// 完整不可变原记录包含精确前驱、目标及准备策略。
    original: VulcanPluginActivationTransactionRecord,
    /// Only actual resource handoff changes this evidence; activation Aborted remains untouched.
    /// 仅实际资源交接改变此证据；激活 Aborted 保持不变。
    phase: VulcanPluginPredecessorHandoffPhase,
}

impl VulcanPluginPredecessorHandoffRecord {
    /// Return the sole original request ID; reading it grants no physical ownership.
    /// 返回唯一原请求标识；读取它不授予物理所有权。
    pub fn request_id(&self) -> Uuid {
        self.original.request_id
    }

    /// Borrow the entire frozen original rather than reconstructing an owner from UUID or plugin ID.
    /// 借用整个固定原记录，不从 UUID 或插件 ID 重建所有者。
    pub fn original(&self) -> &VulcanPluginActivationTransactionRecord {
        &self.original
    }

    /// Return confirmed durable handoff evidence, independently of the original activation phase.
    /// 返回已确认持久交接证据，独立于原激活阶段。
    pub fn phase(&self) -> VulcanPluginPredecessorHandoffPhase {
        self.phase
    }
}

/// Non-cloneable native reservation retains actual old files; no public constructor or UUID resumption exists.
/// 不可克隆原生保留权持有实际旧文件；不存在公开构造器或 UUID 恢复入口。
#[derive(Debug)]
pub(crate) struct PredecessorHandoffReservation {
    /// Exact store binding prevents applying this owner to a foreign root.
    /// 精确存储绑定防止将此所有者用于外部根目录。
    store_root: PathBuf,
    /// Expected full durable record remains available after an unknown write result.
    /// 未知写入结果后仍可获得预期完整持久记录。
    record: VulcanPluginPredecessorHandoffRecord,
    /// Independent real exclusive file preserves abort's sole request-readiness retirement rule.
    /// 独立真实排他文件保留中止的唯一请求就绪退役规则。
    _lease: File,
    /// The old physical package cannot be replaced by a copied identity token.
    /// 复制身份令牌不能替代旧物理插件包。
    generation: PinnedVulcanPluginGeneration,
    /// The exact old secure-store revision stays pinned through unknown completion.
    /// 精确旧安全存储修订在未知完成期间保持固定。
    credentials: PinnedVulcanPluginCredentials,
}

impl PredecessorHandoffReservation {
    /// Borrow the original frozen record for the actual manager's scoped handoff checks.
    /// 借用原固定记录，供实际管理器进行作用域交接校验。
    pub(crate) fn original(&self) -> &VulcanPluginActivationTransactionRecord {
        &self.record.original
    }
}

/// Sealed transfer capability borrows the original reservation held by the composite resource guards.
/// 密封移交能力借用组合资源守卫持有的原预留权。
#[derive(Debug)]
pub(crate) struct PredecessorHandoffTransferAuthority<'owner> {
    /// Borrowing the same non-cloneable reservation prevents a UUID-only completion capability.
    /// 借用相同不可克隆保留权，防止仅基于 UUID 的完成能力。
    reservation: &'owner PredecessorHandoffReservation,
}

impl<'owner> PredecessorHandoffTransferAuthority<'owner> {
    /// Borrow reservation only after actual Hook/native guards have been prepared; no UUID can substitute for its File.
    /// 仅在实际 Hook／原生守卫已准备后借用 reservation；UUID 不能替代其文件。
    /// Return a short capability that cannot outlive the same locked non-cloneable reservation.
    /// 返回不能超过相同已锁定不可克隆预留权寿命的短能力。
    pub(crate) fn from_prepared_reservation(
        reservation: &'owner PredecessorHandoffReservation,
    ) -> Self {
        Self { reservation }
    }
}

impl VulcanPluginGenerationStore {
    /// Allocate real predecessor ownership for exact `transaction`/`original`, without writing a Pending record.
    /// 为精确 `transaction`、`original` 分配真实前驱所有权，不写入 Pending 记录。
    /// Returns the non-cloneable owner before persistence, so every uncertain write retains its same file lock.
    /// 在持久化前返回不可克隆所有者，使每次不确定写入保留相同文件锁。
    pub(crate) fn prepare_predecessor_handoff_reservation(
        &self,
        transaction: &mut VulcanPluginActivationTransaction,
        original: &VulcanPluginActivationTransactionRecord,
    ) -> VulcanPluginStoreResult<PredecessorHandoffReservation> {
        if transaction.store_root != self.root || transaction.record != *original {
            return Err(Error::new(
                Code::Conflict,
                "Predecessor reservation owner changed.",
            ));
        }
        // One namespace spans ownership, exact selection and both actual old physical pins.
        // 单个命名空间覆盖所有权、精确选择及两个实际旧物理固定句柄。
        let _namespace = self.namespace_lock(&original.target.plugin_id, false)?;
        Self::require_readiness_retired(transaction)?;
        self.check_transaction_owner(transaction)?;
        Self::validate_predecessor_handoff_original(original)?;
        if self.read_activation(&original.target.plugin_id)? != original.predecessor {
            return Err(Error::new(
                Code::Conflict,
                "Predecessor reservation selection changed.",
            ));
        }
        self.ensure_no_predecessor_handoff_locked(&original.target.plugin_id)?;
        // A historical same-request record cannot be reused to manufacture a replacement live instance.
        // 不得复用相同请求的历史记录来制造替换存活实例。
        if self
            .read_predecessor_handoff_locked(&original.target.plugin_id, original.request_id)?
            .is_some()
        {
            return Err(Error::new(
                Code::Conflict,
                "Original predecessor reservation already exists.",
            ));
        }
        // The independent exclusive file can remain owned while abort requires a unique original request Arc.
        // 中止要求唯一原请求 Arc 时，独立排他文件仍可保持自有。
        let lease = files::lock(&self.predecessor_handoff_lease_path(original), false)?;
        // Validation guarantees a managed, configuration-bearing exact old selection before pinning.
        // 固定前，校验确保存在托管且携带配置的精确旧选择。
        let predecessor = original
            .predecessor
            .as_ref()
            .expect("validated predecessor");
        // Retain verified immutable old bytes, not the ordinary mutable user plugin directory.
        // 保留已验证不可变旧字节，而非普通可变用户插件目录。
        let generation = self.pin_locked(
            predecessor
                .generation
                .as_ref()
                .expect("validated generation"),
        )?;
        if predecessor.approved_permissions() != generation.required_permissions.as_slice() {
            return Err(Error::invalid(
                "Predecessor reservation permissions differ from its package.",
            ));
        }
        generation.validate_configuration(predecessor.configuration())?;
        // Exact revision pinning validates its package binding and acquires the real secure-store reader lease.
        // 精确修订固定校验包绑定，并取得真实安全存储读者租约。
        let credentials =
            self.pin_credentials_locked(&generation, predecessor.credential_revision())?;
        Ok(PredecessorHandoffReservation {
            store_root: self.root.clone(),
            record: VulcanPluginPredecessorHandoffRecord {
                schema_version: PREDECESSOR_HANDOFF_VERSION,
                original: original.clone(),
                phase: VulcanPluginPredecessorHandoffPhase::Pending,
            },
            _lease: lease,
            generation,
            credentials,
        })
    }

    /// Persist `reservation` once or confirm its exact original Pending bytes, retaining the owner on any error.
    /// 持久保存 `reservation` 一次或确认精确原 Pending 字节，任何错误都保留所有者。
    /// The caller must retain the supplied guard after failure; this method never recreates resources or callbacks.
    /// 失败后调用方必须保留所提供守卫；此方法绝不重建资源或回调。
    pub(crate) fn persist_predecessor_handoff_reservation(
        &self,
        reservation: &mut PredecessorHandoffReservation,
    ) -> VulcanPluginStoreResult<()> {
        self.validate_predecessor_handoff_owner(reservation)?;
        // The same namespace spans the durable activation owner, existing sidecar and exact write readback.
        // 相同命名空间覆盖持久激活所有者、既有附属记录及精确写入回读。
        let _namespace =
            self.namespace_lock(&reservation.record.original.target.plugin_id, false)?;
        self.require_predecessor_handoff_original_locked(&reservation.record.original, true)?;
        if reservation.record.phase != VulcanPluginPredecessorHandoffPhase::Pending {
            return Err(Error::new(
                Code::Conflict,
                "Transferred predecessor reservation cannot restart.",
            ));
        }
        // A same-owner retry only confirms identical persisted bytes; it never overwrites another record.
        // 相同所有者重试仅确认相同持久字节；绝不覆盖另一记录。
        match self.read_predecessor_handoff_locked(
            &reservation.record.original.target.plugin_id,
            reservation.record.original.request_id,
        )? {
            Some(record) if record == reservation.record => return Ok(()),
            Some(_) => {
                return Err(Error::new(
                    Code::Conflict,
                    "Original predecessor reservation changed.",
                ));
            }
            None => {}
        }
        self.ensure_no_predecessor_handoff_locked(&reservation.record.original.target.plugin_id)?;
        // Creating only this sidecar directory does not alter the original request's canonical index or Aborted phase.
        // 仅创建此附属目录，不改变原请求的规范索引或 Aborted 阶段。
        self.predecessor_handoff_directory(&reservation.record.original.target.plugin_id, true)?;
        self.write_predecessor_handoff_confirmed(&reservation.record)
    }

    /// Confirm the exact Pending record and original Aborted selection for an already retained physical owner.
    /// 为已保留物理所有者确认精确 Pending 记录及原 Aborted 选择。
    /// Returns no execution grant; a crash-reopened disk record cannot call this without its original live guard.
    /// 不返回执行授权；崩溃后重开的磁盘记录没有原存活守卫便不能调用此方法。
    pub(crate) fn validate_aborted_predecessor_handoff_owner(
        &self,
        reservation: &PredecessorHandoffReservation,
    ) -> VulcanPluginStoreResult<()> {
        self.validate_predecessor_handoff_owner(reservation)?;
        // Closed staging and actual resource preparation only proceed under the original namespace exclusion.
        // 关闭暂存及实际资源准备仅在原命名空间互斥下推进。
        let _namespace =
            self.namespace_lock(&reservation.record.original.target.plugin_id, false)?;
        self.require_predecessor_handoff_original_locked(&reservation.record.original, true)?;
        // Aborted is required independently of the sidecar; neither receipt substitutes for the other.
        // 独立于附属记录要求 Aborted；两个回执不能相互替代。
        let mut aborted = reservation.record.original.clone();
        aborted.phase = ActivationPhase::Aborted;
        if self
            .indexed_transaction_locked(&aborted.target.plugin_id)?
            .as_ref()
            != Some(&aborted)
            || reservation.record.phase != VulcanPluginPredecessorHandoffPhase::Pending
            || self
                .read_predecessor_handoff_locked(&aborted.target.plugin_id, aborted.request_id)?
                .as_ref()
                != Some(&reservation.record)
        {
            return Err(Error::new(
                Code::Conflict,
                "Original aborted predecessor handoff is not confirmed.",
            ));
        }
        Ok(())
    }

    /// Confirm Transferred using the sealed same-owner `authority`; activation Aborted is never rewritten.
    /// 使用密封相同所有者 `authority` 确认 Transferred；绝不改写激活 Aborted。
    /// Only the composite actual Hook/MCP completion borrows its retained reservation for this capability.
    /// 仅实际 Hook／MCP 组合完成为此能力借用其保留预留权。
    pub(crate) fn complete_predecessor_handoff_owned(
        &self,
        authority: &PredecessorHandoffTransferAuthority<'_>,
    ) -> VulcanPluginStoreResult<()> {
        // A capability borrows its sole actual reservation; callers cannot replace that owner with a public UUID.
        // 能力借用其唯一实际保留权；调用方不能用公开 UUID 替换所有者。
        let reservation = authority.reservation;
        self.validate_predecessor_handoff_owner(reservation)?;
        // One namespace spans current old selection, original Aborted and the independent resource transfer write.
        // 单个命名空间覆盖当前旧选择、原 Aborted 及独立资源移交写入。
        let _namespace =
            self.namespace_lock(&reservation.record.original.target.plugin_id, false)?;
        self.require_predecessor_handoff_original_locked(&reservation.record.original, true)?;
        // Exact original Aborted remains the sole acceptable activation image even on a retry.
        // 即使重试，精确原 Aborted 仍是唯一可接受激活映像。
        let mut aborted = reservation.record.original.clone();
        aborted.phase = ActivationPhase::Aborted;
        if self
            .indexed_transaction_locked(&aborted.target.plugin_id)?
            .as_ref()
            != Some(&aborted)
        {
            return Err(Error::new(
                Code::Conflict,
                "Predecessor handoff requires the exact original abort.",
            ));
        }
        // Construct the sole complete durable image without fabricating instance IDs, counters or a new activation.
        // 构造唯一完整持久映像，不伪造实例 ID、计数器或新激活。
        let mut transferred = reservation.record.clone();
        transferred.phase = VulcanPluginPredecessorHandoffPhase::Transferred;
        match self.read_predecessor_handoff_locked(&aborted.target.plugin_id, aborted.request_id)? {
            Some(record) if record == transferred => Ok(()),
            Some(record) if record == reservation.record => {
                self.write_predecessor_handoff_confirmed(&transferred)
            }
            _ => Err(Error::new(
                Code::Conflict,
                "Original predecessor handoff reservation changed.",
            )),
        }
    }

    /// Read explicit `plugin_id`/`request_id` evidence without taking, releasing or recreating any live resource.
    /// 读取显式 `plugin_id`、`request_id` 证据，不取得、释放或重建任何存活资源。
    pub fn predecessor_handoff_record(
        &self,
        plugin_id: &str,
        request_id: Uuid,
    ) -> VulcanPluginStoreResult<Option<VulcanPluginPredecessorHandoffRecord>> {
        if request_id.is_nil() {
            return Err(Error::invalid("Predecessor handoff request cannot be nil."));
        }
        // Read-only lookup never creates the namespace lease or any sidecar directory for missing evidence.
        // 只读查询绝不为缺失证据创建命名空间租约或任何附属目录。
        let Some(_namespace) = self.predecessor_handoff_reader_namespace(plugin_id)? else {
            return Ok(None);
        };
        self.read_predecessor_handoff_locked(plugin_id, request_id)
    }

    /// Return whether `plugin_id` remains closed by original handoff evidence or its actual unretired lease.
    /// 返回 `plugin_id` 是否仍被原交接证据或实际未退役租约关闭。
    /// This read-only query creates no file, directory or record; missing evidence is false, corruption is an error.
    /// 此只读查询不创建文件、目录或记录；缺失证据为 false，损坏返回错误。
    pub fn predecessor_handoff_blocks_admission(
        &self,
        plugin_id: &str,
    ) -> VulcanPluginStoreResult<bool> {
        // A real writer holding this namespace is a normal temporary closed state, never a reason to bypass it.
        // 真实写入者持有此命名空间是正常临时关闭状态，绝不是绕过它的理由。
        let _namespace = match self.predecessor_handoff_reader_namespace(plugin_id) {
            Ok(Some(namespace)) => namespace,
            Ok(None) => return Ok(false),
            Err(error) if error.code == Code::Busy => return Ok(true),
            Err(error) => return Err(error),
        };
        self.predecessor_handoff_blocks_admission_locked(plugin_id)
    }

    /// Apply the sole Pending/actual-lease rule under caller-held namespace exclusion, without creating files.
    /// 在调用方持有命名空间互斥下应用唯一 Pending、实际租约规则，不创建文件。
    pub(in crate::generation_store) fn predecessor_handoff_blocks_admission_locked(
        &self,
        plugin_id: &str,
    ) -> VulcanPluginStoreResult<bool> {
        for record in self.predecessor_handoff_records_locked(plugin_id)? {
            // Persisted evidence requires its original actual lease file even after the owning process exits.
            // 即使所属进程退出，持久证据仍要求其原实际租约文件。
            let path = self.predecessor_handoff_lease_path(&record.original);
            if record.phase == VulcanPluginPredecessorHandoffPhase::Pending {
                return Ok(true);
            }
            // The same native owner may still retain this real exclusive lease after an unknown Transferred write.
            // 未知 Transferred 写入后，相同原生所有者仍可保留此真实排他租约。
            let file = File::open(&path)
                .map_err(|error| Error::io("Open original predecessor handoff lease", error))?;
            files::exists(&path, false)?;
            match file.try_lock() {
                Ok(()) => {}
                Err(std::fs::TryLockError::WouldBlock) => return Ok(true),
                Err(std::fs::TryLockError::Error(error)) => {
                    return Err(Error::io(
                        "Inspect original predecessor handoff lease",
                        error,
                    ));
                }
            }
        }
        Ok(false)
    }

    /// Borrow only an existing canonical reader lease for `plugin_id`; missing optional evidence creates nothing.
    /// 为 `plugin_id` 仅借用既有规范读者租约；缺失可选证据时不创建任何内容。
    fn predecessor_handoff_reader_namespace(
        &self,
        plugin_id: &str,
    ) -> VulcanPluginStoreResult<Option<File>> {
        crate::catalog::validate_identifier(plugin_id, "generation plugin id")
            .map_err(Error::invalid)?;
        if !files::exists(&self.root, true)?
            || self
                .predecessor_handoff_directory(plugin_id, false)?
                .is_none()
        {
            return Ok(None);
        }
        // A sidecar without its canonical namespace lease is corruption, never permission to fabricate a new lock.
        // 附属记录没有规范命名空间租约属于损坏，绝不是制造新锁的许可。
        let path = self.root.join("locks").join(format!("{plugin_id}.lock"));
        if !files::exists(&path, false)? {
            return Err(Error::invalid(
                "Original predecessor namespace lease disappeared.",
            ));
        }
        // Opening read-only preserves existing bytes and never uses OpenOptions create or write permissions.
        // 只读打开保留既有字节，绝不使用 OpenOptions 创建或写入权限。
        let file = File::open(&path)
            .map_err(|error| Error::io("Open predecessor namespace reader", error))?;
        files::exists(&path, false)?;
        match file.try_lock_shared() {
            Ok(()) => Ok(Some(file)),
            Err(std::fs::TryLockError::WouldBlock) => Err(Error::new(
                Code::Busy,
                "Original predecessor namespace writer is active.",
            )),
            Err(std::fs::TryLockError::Error(error)) => {
                Err(Error::io("Lock predecessor namespace reader", error))
            }
        }
    }

    /// Reject every unfinished predecessor handoff under the caller's namespace fence before a new writer starts.
    /// 在调用方命名空间屏障下，新写入者开始前拒绝任何未完成前驱交接。
    pub(in crate::generation_store) fn ensure_no_predecessor_handoff_locked(
        &self,
        plugin_id: &str,
    ) -> VulcanPluginStoreResult<()> {
        if self.predecessor_handoff_blocks_admission_locked(plugin_id)? {
            return Err(Error::new(
                Code::Busy,
                "Original predecessor private handoff remains pending.",
            ));
        }
        Ok(())
    }

    /// Protect actual `identity` retained by a Pending old handoff under the caller-held namespace fence.
    /// 在调用方持有命名空间屏障下，保护 Pending 旧交接保留的实际 `identity`。
    pub(in crate::generation_store) fn check_predecessor_handoff_package_retention_locked(
        &self,
        identity: &VulcanPluginGenerationIdentity,
    ) -> VulcanPluginStoreResult<()> {
        for record in self.predecessor_handoff_records_locked(&identity.plugin_id)? {
            if record.phase == VulcanPluginPredecessorHandoffPhase::Pending
                && record
                    .original
                    .predecessor
                    .as_ref()
                    .and_then(|old| old.generation.as_ref())
                    == Some(identity)
            {
                return Err(Error::new(
                    Code::Busy,
                    "Package is retained by the original predecessor handoff.",
                ));
            }
        }
        Ok(())
    }

    /// Protect exact old `revision` retained by Pending evidence before secure-store cleanup begins.
    /// 安全存储清理开始前，保护 Pending 证据保留的精确旧 `revision`。
    pub(in crate::generation_store) fn check_predecessor_handoff_credential_retention_locked(
        &self,
        plugin_id: &str,
        revision: &VulcanPluginCredentialRevision,
    ) -> VulcanPluginStoreResult<()> {
        for record in self.predecessor_handoff_records_locked(plugin_id)? {
            if record.phase == VulcanPluginPredecessorHandoffPhase::Pending
                && record
                    .original
                    .predecessor
                    .as_ref()
                    .is_some_and(|old| old.credential_revision() == revision)
            {
                return Err(Error::new(
                    Code::Busy,
                    "Credentials are retained by the original predecessor handoff.",
                ));
            }
        }
        Ok(())
    }

    /// Validate full `original` and require explicit immutable managed configuration and credentials.
    /// 校验完整 `original`，并要求显式不可变托管配置及凭据。
    fn validate_predecessor_handoff_original(
        original: &VulcanPluginActivationTransactionRecord,
    ) -> VulcanPluginStoreResult<()> {
        Self::validate_transaction_record(
            original,
            &original.target.plugin_id,
            original.request_id,
        )?;
        if !matches!(
            original.phase,
            ActivationPhase::Prepared | ActivationPhase::Committing
        ) || !original
            .predecessor
            .as_ref()
            .is_some_and(|old| old.generation.is_some())
        {
            return Err(Error::invalid(
                "Predecessor handoff requires the complete immutable old managed selection.",
            ));
        }
        Ok(())
    }

    /// Compare actual owner pins and exact root before opening any foreign namespace or writing evidence.
    /// 打开任何外部命名空间或写入证据前，比较实际所有者固定句柄及精确根目录。
    fn validate_predecessor_handoff_owner(
        &self,
        reservation: &PredecessorHandoffReservation,
    ) -> VulcanPluginStoreResult<()> {
        Self::validate_predecessor_handoff_original(&reservation.record.original)?;
        // Complete selection comparison includes its real package and credential revision, not only original UUID.
        // 完整选择比较包含真实包及凭据修订，不仅是原 UUID。
        let old = reservation
            .record
            .original
            .predecessor
            .as_ref()
            .expect("validated predecessor");
        if reservation.store_root != self.root
            || reservation.generation.store_root != self.root
            || old.generation.as_ref() != Some(reservation.generation.identity())
            || old.credential_revision() != reservation.credentials.record().revision()
        {
            return Err(Error::new(
                Code::Conflict,
                "Predecessor reservation physical owner differs from its original.",
            ));
        }
        Ok(())
    }

    /// Require unchanged complete indexed `original` and optional selected-old equality under caller exclusion.
    /// 在调用方互斥下要求完整已索引 `original` 未变，并按需要求所选旧选择相等。
    fn require_predecessor_handoff_original_locked(
        &self,
        original: &VulcanPluginActivationTransactionRecord,
        require_selection: bool,
    ) -> VulcanPluginStoreResult<()> {
        // Only the exact original or its exact derived Aborted image belongs to this reservation.
        // 仅精确原记录或其精确派生 Aborted 映像属于此保留权。
        let actual = self
            .read_transaction_locked(&original.target.plugin_id, original.request_id)?
            .ok_or_else(|| Error::invalid("Original predecessor request disappeared."))?;
        // No phase ordering or arbitrary normalization can make a different observed original acceptable.
        // 任何阶段排序或任意规范化都不能使不同观测原记录可接受。
        let mut aborted = original.clone();
        aborted.phase = ActivationPhase::Aborted;
        if (actual != *original && actual != aborted)
            || (require_selection
                && (self
                    .indexed_transaction_locked(&original.target.plugin_id)?
                    .as_ref()
                    != Some(&actual)
                    || self.read_activation(&original.target.plugin_id)? != original.predecessor))
        {
            return Err(Error::new(
                Code::Conflict,
                "Original predecessor handoff authority changed.",
            ));
        }
        Ok(())
    }

    /// Resolve one validated optional sidecar directory; `create` belongs only to the original live owner.
    /// 解析单个已校验可选附属目录；`create` 仅属于原存活所有者。
    fn predecessor_handoff_directory(
        &self,
        plugin_id: &str,
        create: bool,
    ) -> VulcanPluginStoreResult<Option<PathBuf>> {
        // Reuse strict ancestor and canonical plugin validation from the original request directory.
        // 复用原请求目录的严格祖先及规范插件校验。
        let Some(parent) = self.transaction_directory(plugin_id, false)? else {
            return Ok(None);
        };
        if create {
            return files::ensure_child(&parent, PREDECESSOR_HANDOFF_DIRECTORY).map(Some);
        }
        // Checking existence remains read-only and rejects links or non-directory replacements.
        // 检查存在性保持只读，并拒绝链接或非目录替换。
        let path = parent.join(PREDECESSOR_HANDOFF_DIRECTORY);
        Ok(files::exists(&path, true)?.then_some(path))
    }

    /// Resolve the sole independent exclusive file from the complete `original`, without a new request UUID.
    /// 从完整 `original` 解析唯一独立排他文件，不创建新请求 UUID。
    fn predecessor_handoff_lease_path(
        &self,
        original: &VulcanPluginActivationTransactionRecord,
    ) -> PathBuf {
        self.root.join("locks").join(format!(
            "{}.predecessor-handoff.{}.lock",
            original.target.plugin_id, original.request_id
        ))
    }

    /// Derive the single sidecar record filename for `request_id` inside its already validated `directory`.
    /// 在已校验 `directory` 内，为 `request_id` 派生唯一附属记录文件名。
    fn predecessor_handoff_record_path(directory: &Path, request_id: Uuid) -> PathBuf {
        directory.join(format!("{request_id}.json"))
    }

    /// Read strict bounded evidence for explicit `plugin_id`/`request_id` under the caller's namespace fence.
    /// 在调用方命名空间屏障下，为显式 `plugin_id`、`request_id` 读取严格有界证据。
    fn read_predecessor_handoff_locked(
        &self,
        plugin_id: &str,
        request_id: Uuid,
    ) -> VulcanPluginStoreResult<Option<VulcanPluginPredecessorHandoffRecord>> {
        // Ordinary reads never allocate directories or select a request from candidates.
        // 普通读取绝不分配目录，也不从候选选择请求。
        let Some(directory) = self.predecessor_handoff_directory(plugin_id, false)? else {
            return Ok(None);
        };
        // The existing full transaction budget bounds this single full original plus metadata allowance.
        // 既有完整事务额度限制此单个完整原记录及元数据额度。
        let Some(bytes) = files::read_bytes(
            &Self::predecessor_handoff_record_path(&directory, request_id),
            MAX_TRANSACTION_BYTES,
        )?
        else {
            return Ok(None);
        };
        // Decoder failures never include private configuration bytes in public diagnostics.
        // 解码失败绝不在公开诊断中包含私有配置字节。
        let record: VulcanPluginPredecessorHandoffRecord = serde_json::from_slice(&bytes)
            .map_err(|_| Error::invalid("Invalid predecessor handoff record."))?;
        if record.schema_version != PREDECESSOR_HANDOFF_VERSION
            || record.original.target.plugin_id != plugin_id
            || record.original.request_id != request_id
        {
            return Err(Error::invalid(
                "Predecessor handoff identity or format is invalid.",
            ));
        }
        Self::validate_predecessor_handoff_original(&record.original)?;
        // Every known durable record retains its actual original lease file; read-only status never repairs absence.
        // 每个已知持久记录保留实际原租约文件；只读状态绝不修复缺失。
        if !files::exists(
            &self.predecessor_handoff_lease_path(&record.original),
            false,
        )? {
            return Err(Error::invalid(
                "Original predecessor handoff lease file disappeared.",
            ));
        }
        self.require_predecessor_handoff_original_locked(
            &record.original,
            record.phase == VulcanPluginPredecessorHandoffPhase::Pending,
        )?;
        if record.phase == VulcanPluginPredecessorHandoffPhase::Transferred
            && self
                .read_transaction_locked(plugin_id, request_id)?
                .is_none_or(|original| original.phase != ActivationPhase::Aborted)
        {
            return Err(Error::invalid(
                "Transferred predecessor handoff requires the original Aborted receipt.",
            ));
        }
        Ok(Some(record))
    }

    /// Enumerate every strict handoff record for `plugin_id`; corruption never becomes an empty pending inventory.
    /// 为 `plugin_id` 枚举每个严格交接记录；损坏绝不变为空待处理清单。
    fn predecessor_handoff_records_locked(
        &self,
        plugin_id: &str,
    ) -> VulcanPluginStoreResult<Vec<VulcanPluginPredecessorHandoffRecord>> {
        // Missing optional storage is the only accepted empty directory case.
        // 可选存储缺失是唯一接受的空目录情况。
        let Some(directory) = self.predecessor_handoff_directory(plugin_id, false)? else {
            return Ok(Vec::new());
        };
        // Enumerating actual entries prevents terminal indexed activation from hiding a pending older handoff.
        // 枚举实际条目，防止已索引终态激活隐藏更早待处理交接。
        let entries = std::fs::read_dir(directory)
            .map_err(|error| Error::io("Enumerate predecessor handoffs", error))?;
        // Records carry complete originals; ordering grants no ownership or preferred request.
        // 记录携带完整原请求；排序不授予所有权或优选请求。
        let mut records = Vec::new();
        for entry in entries {
            // Every file must use one canonical original UUID and the single declared extension.
            // 每个文件必须使用单个规范原 UUID 及唯一声明扩展名。
            let path = entry
                .map_err(|error| Error::io("Read predecessor handoff entry", error))?
                .path();
            // Literal stem validation rejects alternate names, links and directories in this strict inventory.
            // 字面主干校验拒绝严格清单中的替代名称、链接及目录。
            let stem = path
                .file_stem()
                .and_then(|value| value.to_str())
                .ok_or_else(|| Error::invalid("Invalid predecessor handoff filename."))?;
            // Nil UUIDs never represent actual original requests.
            // 空 UUID 绝不代表实际原请求。
            let request_id = Uuid::parse_str(stem)
                .map_err(|_| Error::invalid("Invalid predecessor handoff filename."))?;
            if request_id.is_nil()
                || request_id.to_string() != stem
                || path.extension() != Some(std::ffi::OsStr::new("json"))
                || !files::exists(&path, false)?
            {
                return Err(Error::invalid("Noncanonical predecessor handoff entry."));
            }
            records.push(
                self.read_predecessor_handoff_locked(plugin_id, request_id)?
                    .ok_or_else(|| Error::invalid("Predecessor handoff entry disappeared."))?,
            );
        }
        Ok(records)
    }

    /// Write `record` atomically and confirm only its exact immediate readback after an uncertain result.
    /// 原子写入 `record`，不确定结果后仅确认其精确即时回读。
    fn write_predecessor_handoff_confirmed(
        &self,
        record: &VulcanPluginPredecessorHandoffRecord,
    ) -> VulcanPluginStoreResult<()> {
        // The original directory must still exist; completion cannot repair removed reservation storage.
        // 原目录必须仍存在；完成不能修复已删除保留权存储。
        let directory = self
            .predecessor_handoff_directory(&record.original.target.plugin_id, false)?
            .ok_or_else(|| Error::invalid("Predecessor handoff directory disappeared."))?;
        // Encoding is bounded by the one existing full-original transaction budget.
        // 编码受唯一既有完整原事务额度限制。
        let bytes = serde_json::to_vec(record)
            .map_err(|_| Error::invalid("Cannot encode predecessor handoff record."))?;
        // A real atomic write runs once; uncertainty never causes reconstruction or unconditional replacement.
        // 真实原子写入仅执行一次；不确定性绝不导致重建或无条件替换。
        // Resolve the actual writer destination once, including for scoped fixture obstruction.
        // 仅解析一次实际写入器目标，包含作用域夹具阻挡。
        let record_path =
            Self::predecessor_handoff_record_path(&directory, record.original.request_id);
        #[cfg(test)]
        // A fixture creates a real empty directory at this exact writer path, before any bytes are committed.
        // 夹具在提交任何字节前，于此精确写入器路径创建真实空目录。
        handoff_observation_fault::obstruct(&self.root, record.original.request_id, &record_path)?;
        let result = files::write_encoded_record(&record_path, &bytes, MAX_TRANSACTION_BYTES);
        #[cfg(test)]
        // Exact scoped fixture loss occurs only after the actual requested bytes were written.
        // 精确作用域夹具丢失仅在实际请求字节写入后发生。
        let lost = handoff_observation_fault::consume(
            &self.root,
            record.original.request_id,
            result.is_ok(),
        );
        #[cfg(test)]
        // Persist real bytes while losing both immediate observations; the supplied owner remains retained.
        // 持久保存真实字节，同时丢失两个即时观测；提供的所有者仍保持保留。
        let result = if lost {
            Err(Error::new(
                Code::Storage,
                "Controlled loss of predecessor handoff write observation.",
            ))
        } else {
            result
        };
        if let Err(write_error) = result {
            #[cfg(test)]
            if lost {
                return Err(Error::new(
                    Code::Storage,
                    "Controlled loss of predecessor handoff immediate readback.",
                ));
            }
            if self
                .read_predecessor_handoff_locked(
                    &record.original.target.plugin_id,
                    record.original.request_id,
                )?
                .as_ref()
                != Some(record)
            {
                return Err(write_error);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
impl VulcanPluginGenerationStore {
    /// Arm a real write obstruction for this original request only; the same-thread scope clears unused intent.
    /// 仅为此原请求启用真实写入阻挡；相同线程作用域清除未使用意图。
    pub(crate) fn obstruct_predecessor_handoff_write_once(&self, request: Uuid) -> impl Drop {
        handoff_observation_fault::arm_obstruction(&self.root, request)
    }

    /// Borrow the sole production lease-path derivation for an actual `reservation`; never create the file.
    /// 为实际 `reservation` 借用唯一生产租约路径派生；绝不创建文件。
    pub(crate) fn predecessor_handoff_test_lease_path(
        &self,
        reservation: &PredecessorHandoffReservation,
    ) -> PathBuf {
        self.predecessor_handoff_lease_path(reservation.original())
    }

    /// Return the actual existing sidecar writer path for `reservation`, without creating or repairing its directory.
    /// 为 `reservation` 返回实际既有附属记录写入器路径，不创建或修复其目录。
    pub(crate) fn predecessor_handoff_test_record_path(
        &self,
        reservation: &PredecessorHandoffReservation,
    ) -> VulcanPluginStoreResult<PathBuf> {
        let original = reservation.original();
        let directory = self
            .predecessor_handoff_directory(&original.target.plugin_id, false)?
            .ok_or_else(|| Error::invalid("Original fixture handoff directory is missing."))?;
        Ok(Self::predecessor_handoff_record_path(
            &directory,
            original.request_id,
        ))
    }

    /// Arm only this original's successful real sidecar write loss; return same-thread scope cleanup.
    /// 仅启用此原请求成功真实附属记录写入丢失；返回相同线程作用域清理。
    pub(crate) fn lose_predecessor_handoff_observation_once(&self, request: Uuid) -> impl Drop {
        handoff_observation_fault::arm(&self.root, request)
    }

    /// Fixture-only capability models completed actual guards; production has no UUID-based constructor.
    /// 仅夹具能力模拟已完成实际守卫；生产不存在基于 UUID 的构造器。
    pub(crate) fn predecessor_handoff_transfer_test_authority<'owner>(
        &self,
        reservation: &'owner PredecessorHandoffReservation,
    ) -> PredecessorHandoffTransferAuthority<'owner> {
        PredecessorHandoffTransferAuthority { reservation }
    }

    /// Clone actual reader pins for a fixture's receiving resource owner, without exposing any lock file.
    /// 为夹具接收资源所有者克隆实际读者固定句柄，不暴露任何锁文件。
    pub(crate) fn predecessor_handoff_test_resource_pins(
        &self,
        reservation: &PredecessorHandoffReservation,
    ) -> (PinnedVulcanPluginGeneration, PinnedVulcanPluginCredentials) {
        (
            reservation.generation.clone(),
            reservation.credentials.clone(),
        )
    }
}

#[cfg(test)]
mod handoff_observation_fault {
    use super::*;
    use std::cell::RefCell;

    /// One exact scoped fault distinguishes lost observation from real filesystem write obstruction.
    /// 单个精确作用域故障区分观测丢失与真实文件系统写入阻挡。
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Fault {
        /// Real committed bytes lose their two immediate observations.
        /// 真实已提交字节丢失其两个即时观测。
        Observation,
        /// The exact real writer encounters an existing empty directory at its record destination.
        /// 精确真实写入器在记录目标处遇到既有空目录。
        Obstruction,
    }

    thread_local! {
        /// Exact single-thread fault never affects canonical public query or another original request.
        /// 精确单线程故障绝不影响规范公开查询或另一原请求。
        static ARMED: RefCell<Option<(PathBuf, Uuid, Fault)>> = const { RefCell::new(None) };
    }

    /// Non-Send scope restores this thread's exact fault slot when the test ends.
    /// 测试结束时，不可发送作用域恢复此线程精确故障槽位。
    pub(super) struct Scope {
        /// Exact armed identity prevents one scope from clearing a subsequently armed original.
        /// 精确启用身份防止一个作用域清除后来启用的原请求。
        original: (PathBuf, Uuid, Fault),
        /// Thread affinity keeps cleanup within the actual fixture slot.
        /// 线程亲和性使清理保持在实际夹具槽位内。
        _thread: std::marker::PhantomData<std::rc::Rc<()>>,
    }

    impl Drop for Scope {
        /// Clear only this thread's armed fixture; no production ownership is released.
        /// 仅清除此线程已启用夹具；不释放生产所有权。
        fn drop(&mut self) {
            ARMED.with(|slot| {
                if slot.borrow().as_ref() == Some(&self.original) {
                    *slot.borrow_mut() = None;
                }
            });
        }
    }

    /// Arm exact `root`/`request`; return same-thread cleanup, rejecting nested fixture ambiguity.
    /// 启用精确 `root`、`request`；返回相同线程清理，并拒绝嵌套夹具歧义。
    pub(super) fn arm(root: &Path, request: Uuid) -> Scope {
        arm_fault(root, request, Fault::Observation)
    }

    /// Arm only the exact `root`/`request` writer obstruction; return same-thread RAII cleanup.
    /// 仅启用精确 `root`、`request` 写入器阻挡；返回相同线程 RAII 清理。
    pub(super) fn arm_obstruction(root: &Path, request: Uuid) -> Scope {
        arm_fault(root, request, Fault::Obstruction)
    }

    /// Bind `fault` to the actual `root`/`request`; reject nested slots and preserve thread affinity.
    /// 将 `fault` 绑定至实际 `root`、`request`；拒绝嵌套槽位并保留线程亲和性。
    fn arm_fault(root: &Path, request: Uuid, fault: Fault) -> Scope {
        ARMED.with(|slot| {
            assert!(
                slot.borrow().is_none(),
                "predecessor handoff fixture already armed"
            );
            *slot.borrow_mut() = Some((root.to_path_buf(), request, fault));
        });
        Scope {
            original: (root.to_path_buf(), request, fault),
            _thread: std::marker::PhantomData,
        }
    }

    /// Consume only matching successful real `root`/`request` writes; return whether their observation was lost.
    /// 仅消耗匹配成功真实 `root`、`request` 写入；返回其观测是否丢失。
    pub(super) fn consume(root: &Path, request: Uuid, written: bool) -> bool {
        ARMED.with(|slot| {
            if written
                && slot
                    .borrow()
                    .as_ref()
                    .is_some_and(|(expected_root, expected_request, fault)| {
                        expected_root == root
                            && *expected_request == request
                            && *fault == Fault::Observation
                    })
            {
                *slot.borrow_mut() = None;
                true
            } else {
                false
            }
        })
    }

    /// Obstruct matching `root`/`request` using its exact real `path`; unmatched writers remain unchanged.
    /// 使用精确真实 `path` 阻挡匹配 `root`、`request`；不匹配写入器保持不变。
    /// Return the real filesystem result, without inventing an IO error or allocating any ownership lease.
    /// 返回真实文件系统结果，不虚构 IO 错误或分配任何所有权租约。
    pub(super) fn obstruct(root: &Path, request: Uuid, path: &Path) -> VulcanPluginStoreResult<()> {
        // Consumption occurs before the actual writer; ordinary observation faults stay armed for their own boundary.
        // 消耗发生在实际写入器之前；普通观测故障保持启用直至其自身边界。
        let armed = ARMED.with(|slot| {
            if slot
                .borrow()
                .as_ref()
                .is_some_and(|(expected_root, expected_request, fault)| {
                    expected_root == root
                        && *expected_request == request
                        && *fault == Fault::Obstruction
                })
            {
                *slot.borrow_mut() = None;
                true
            } else {
                false
            }
        });
        if armed {
            std::fs::create_dir(path)
                .map_err(|failure| Error::io("Create fixture writer obstruction", failure))?;
        }
        Ok(())
    }
}
