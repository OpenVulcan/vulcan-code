//! Strict journal records, single pending index and durable resource-retention checks.
//! 严格日志记录、单一待完成索引及持久资源保留检查。

use super::*;
use crate::generation_store::{
    VulcanPluginStoreError as Error, VulcanPluginStoreErrorCode as Code,
};
use std::collections::BTreeSet;

/// Sole pending-request index; completed request bodies remain independently queryable.
/// 唯一待完成请求索引；已完成请求正文仍可独立查询。
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TransactionIndex {
    /// Stable index version is independent of the referenced request's preparation stage format.
    /// 稳定索引版本独立于所引用请求的准备阶段格式。
    schema_version: u32,
    /// Exact namespace must match the containing directory.
    /// 精确命名空间必须匹配所在目录。
    plugin_id: String,
    /// Original canonical request identity, never inferred from a directory scan.
    /// 原规范请求身份，绝不从目录扫描推断。
    #[serde(deserialize_with = "deserialize_request_id")]
    request_id: Uuid,
}

/// Decode the required predecessor from `deserializer`; an absent field is invalid while explicit null is permitted.
/// 从 `deserializer` 解码必需前驱；字段缺失无效，允许显式空值。
pub(super) fn deserialize_predecessor<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<VulcanPluginActivation>, D::Error> {
    Option::<VulcanPluginActivation>::deserialize(deserializer)
}

/// Decode canonical non-nil text from `deserializer`, returning its exact request identity without alias normalization.
/// 从 `deserializer` 解码规范非空文本，返回精确请求身份，不规范化别名。
pub(super) fn deserialize_request_id<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Uuid, D::Error> {
    // Preserve textual identity rather than accepting the UUID crate's alternate encodings.
    // 保留文本身份，而不接受 UUID 库的其他编码。
    let value = String::deserialize(deserializer)?;
    // The same identity selects both the journal filename and its exclusive lock.
    // 相同身份共同选择日志文件名及其排他锁。
    let request_id = Uuid::parse_str(&value).map_err(serde::de::Error::custom)?;
    if request_id.is_nil() || request_id.to_string() != value {
        return Err(serde::de::Error::custom(
            "Noncanonical activation request identity.",
        ));
    }
    Ok(request_id)
}

impl VulcanPluginGenerationStore {
    /// Read the declared index for either preparation or configured activation under the namespace fence.
    /// 在命名空间屏障下读取准备或已配置激活的声明索引。
    /// Return the exact validated original record or actual index absence.
    /// 返回精确已校验原记录或实际索引缺失。
    pub(super) fn indexed_request_locked(
        &self,
        plugin_id: &str,
    ) -> VulcanPluginStoreResult<Option<VulcanPluginActivationRequestRecord>> {
        // Resolve only the validated namespace directory; absence never creates storage on a read.
        // 仅解析已验证命名空间目录；读取时缺失绝不创建存储。
        let Some(directory) = self.transaction_directory(plugin_id, false)? else {
            return Ok(None);
        };
        // Bounded journal bytes remain private and must not enter decoder diagnostics.
        // 有界日志字节保持私密，不得进入解码器诊断。
        let Some(bytes) = files::read_bytes(&directory.join("current"), MAX_RECORD_BYTES)? else {
            return Ok(None);
        };
        // Decode the explicit index rather than selecting a request from directory order.
        // 解码显式索引，而不按目录顺序选择请求。
        let index: TransactionIndex = serde_json::from_slice(&bytes)
            .map_err(|_| Error::invalid("Invalid activation transaction index."))?;
        if index.schema_version != TRANSACTION_RECORD_VERSION || index.plugin_id != plugin_id {
            return Err(Error::invalid(
                "Activation transaction index identity is invalid.",
            ));
        }
        self.read_request_locked(plugin_id, index.request_id)?
            .ok_or_else(|| Error::invalid("Indexed activation transaction is missing."))
            .map(Some)
    }

    /// Index the already durable original `plugin_id` and `request_id`; do not select a request by scanning.
    /// 索引已经持久化的原 `plugin_id` 及 `request_id`；不通过扫描选择请求。
    pub(super) fn index_request_locked(
        &self,
        plugin_id: &str,
        request_id: Uuid,
    ) -> VulcanPluginStoreResult<()> {
        // Resolve only the validated namespace directory; absence never creates storage on a read.
        // 仅解析已验证命名空间目录；读取时缺失绝不创建存储。
        let directory = self
            .transaction_directory(plugin_id, false)?
            .ok_or_else(|| Error::invalid("Activation transaction directory disappeared."))?;
        // Both initial and replacement publication must account for every unfinished original before changing authority.
        // 初始及替换发布都必须在变更权威之前核对每个未完成原请求。
        self.check_index_recovery(plugin_id, request_id, &directory)?;
        files::write_record(
            &directory.join("current"),
            &TransactionIndex {
                schema_version: TRANSACTION_RECORD_VERSION,
                plugin_id: plugin_id.to_owned(),
                request_id,
            },
        )
    }

    /// Resolve the validated journal directory for `plugin_id`; `create` explicitly permits missing directory creation.
    /// 解析 `plugin_id` 的已验证日志目录；`create` 明确允许创建缺失目录。
    pub(super) fn transaction_directory(
        &self,
        plugin_id: &str,
        create: bool,
    ) -> VulcanPluginStoreResult<Option<PathBuf>> {
        crate::catalog::validate_identifier(plugin_id, "activation transaction plugin id")
            .map_err(Error::invalid)?;
        self.validate_layout()?;
        // Optional journals must not make ordinary legacy reads mutate the filesystem.
        // 可选日志不能使普通旧格式读取修改文件系统。
        let root = self.root.join("transactions");
        if create {
            files::ensure_child(&self.root, "transactions")?;
            return files::ensure_child(&root, plugin_id).map(Some);
        }
        if !files::exists(&root, true)? {
            return Ok(None);
        }
        // Each ancestor is validated before any journal or pointer beneath it is accessed.
        // 访问下方任何日志或指针前，逐级校验祖先目录。
        let directory = root.join(plugin_id);
        Ok(files::exists(&directory, true)?.then_some(directory))
    }

    /// Read and validate `plugin_id`/`request_id` under the caller's namespace fence; return actual absence only.
    /// 在调用方命名空间屏障内读取并校验 `plugin_id`／`request_id`；仅返回实际缺失。
    pub(super) fn read_transaction_locked(
        &self,
        plugin_id: &str,
        request_id: Uuid,
    ) -> VulcanPluginStoreResult<Option<VulcanPluginActivationTransactionRecord>> {
        match self.read_request_locked(plugin_id, request_id)? {
            None => Ok(None),
            Some(VulcanPluginActivationRequestRecord::Activation(record)) => Ok(Some(record)),
            Some(VulcanPluginActivationRequestRecord::Preparation(_)) => Err(Error::new(
                Code::Busy,
                "Original request has no configured target; inspect its preparation stage.",
            )),
        }
    }

    /// Validate `record` against its containing `plugin_id` and `request_id` before both persistence and use.
    /// 在持久保存及使用前，按所在 `plugin_id` 及 `request_id` 校验 `record`。
    pub(super) fn validate_transaction_record(
        record: &VulcanPluginActivationTransactionRecord,
        plugin_id: &str,
        request_id: Uuid,
    ) -> VulcanPluginStoreResult<()> {
        if !matches!(
            record.schema_version,
            TRANSACTION_RECORD_VERSION | PREPARATION_RECORD_VERSION | UPDATE_RECORD_VERSION
        ) || record.phase == VulcanPluginActivationTransactionPhase::Preparing
            || record.request_id != request_id
            || record.target.schema_version != CONFIGURED_ACTIVATION_VERSION
            || record.target.generation.is_none()
        {
            return Err(Error::invalid(
                "Activation transaction identity or format is invalid.",
            ));
        }
        match (record.schema_version, record.preparation.as_ref()) {
            (TRANSACTION_RECORD_VERSION, None) => {}
            (PREPARATION_RECORD_VERSION | UPDATE_RECORD_VERSION, Some(intent))
                if intent.matches_target(&record.target) =>
            {
                intent.validate_schema(record.schema_version)?;
                Self::validate_preparation_intent(intent, record.predecessor.as_ref(), plugin_id)?;
            }
            _ => {
                return Err(Error::invalid(
                    "Activation target does not preserve its declared preparation intent.",
                ));
            }
        }
        for activation in record
            .predecessor
            .iter()
            .chain(std::iter::once(&record.target))
        {
            // Stored nested selections share the existing activation validator and its per-format limits.
            // 存储的嵌套选择共用既有激活校验器及按格式划分的限制。
            let encoded_size = serde_json::to_vec(activation)
                .map_err(|_| Error::invalid("Cannot encode transaction selection."))?
                .len() as u64;
            Self::validate_activation_record(activation, plugin_id, encoded_size)?;
        }
        if record
            .predecessor
            .as_ref()
            .map_or(0, |previous| previous.revision)
            .checked_add(1)
            != Some(record.target.revision)
        {
            return Err(Error::invalid(
                "Activation transaction target is not the exact successor.",
            ));
        }
        Ok(())
    }

    /// Persist the exact `record` using the shared atomic writer; failures require querying the original request.
    /// 使用共同原子写入器持久保存精确 `record`；失败需要查询原请求。
    pub(super) fn write_transaction_locked(
        &self,
        record: &VulcanPluginActivationTransactionRecord,
    ) -> VulcanPluginStoreResult<()> {
        Self::validate_transaction_record(record, &record.target.plugin_id, record.request_id)?;
        // Directory creation belongs only to begin; checkpointing cannot repair vanished storage.
        // 目录创建仅属于开始操作；检查点写入不能修复消失的存储。
        let directory = self
            .transaction_directory(&record.target.plugin_id, false)?
            .ok_or_else(|| Error::invalid("Activation transaction directory disappeared."))?;
        // Bounded journal bytes remain private and must not enter decoder diagnostics.
        // 有界日志字节保持私密，不得进入解码器诊断。
        let bytes = serde_json::to_vec(record)
            .map_err(|_| Error::invalid("Cannot encode activation transaction."))?;
        files::write_encoded_record(
            &directory.join(format!("{}.json", record.request_id)),
            &bytes,
            MAX_TRANSACTION_BYTES,
        )
    }

    /// Read the one declared index under its namespace fence, validating its referenced record even when terminal.
    /// 在命名空间屏障内读取唯一声明索引，即使终态也校验其引用记录。
    pub(super) fn indexed_transaction_locked(
        &self,
        plugin_id: &str,
    ) -> VulcanPluginStoreResult<Option<VulcanPluginActivationTransactionRecord>> {
        match self.indexed_request_locked(plugin_id)? {
            None => Ok(None),
            Some(VulcanPluginActivationRequestRecord::Activation(record)) => Ok(Some(record)),
            Some(VulcanPluginActivationRequestRecord::Preparation(_)) => Err(Error::new(
                Code::Busy,
                "Indexed original request is still a preparation record.",
            )),
        }
    }

    /// Point the namespace at an already durable `record`; a failed write never implies the index remained unchanged.
    /// 将命名空间指向已经持久化的 `record`；写入失败绝不表示索引保持未变。
    pub(super) fn index_transaction_locked(
        &self,
        record: &VulcanPluginActivationTransactionRecord,
    ) -> VulcanPluginStoreResult<()> {
        self.index_request_locked(&record.target.plugin_id, record.request_id)
    }

    /// Permit missing-index restoration only when no other unfinished journal exists in the exact `directory`.
    /// 仅精确 `directory` 不存在其他未完成日志时允许重建缺失索引。
    /// `plugin_id` and `request_id` identify the explicit request; enumeration never chooses a replacement authority.
    /// `plugin_id` 及 `request_id` 标识明确请求；枚举绝不选择替代权威。
    fn check_index_recovery(
        &self,
        plugin_id: &str,
        request_id: Uuid,
        directory: &Path,
    ) -> VulcanPluginStoreResult<()> {
        // Every durable body must be accounted for before a missing pointer can be reconstructed.
        // 重建缺失指针前，须核对每个持久正文。
        let entries = std::fs::read_dir(directory)
            .map_err(|error| Error::io("Inspect transaction index recovery", error))?;
        for entry in entries {
            // Ignore only non-record artifacts; JSON records must have canonical names and valid bodies.
            // 仅忽略非记录产物；JSON 记录必须具有规范名称及有效正文。
            let path = entry
                .map_err(|error| Error::io("Read transaction recovery entry", error))?
                .path();
            if path.extension() != Some(std::ffi::OsStr::new("json")) {
                continue;
            }
            // The filename selects an exact historical request, never an alternate package or path.
            // 文件名选择精确历史请求，绝不选择替代插件包或路径。
            let name = path
                .file_stem()
                .and_then(|name| name.to_str())
                .ok_or_else(|| Error::invalid("Invalid transaction recovery filename."))?;
            // Noncanonical identities remain errors rather than normalization candidates.
            // 非规范身份保持为错误，不成为规范化候选。
            let other_id = Uuid::parse_str(name)
                .map_err(|_| Error::invalid("Invalid transaction recovery identity."))?;
            if other_id.to_string() != name || other_id.is_nil() {
                return Err(Error::invalid(
                    "Noncanonical transaction recovery identity.",
                ));
            }
            // Validate all other records so a corrupt body cannot silently relinquish ownership.
            // 校验全部其他记录，使损坏正文不能静默放弃所有权。
            let other = self
                .read_request_locked(plugin_id, other_id)?
                .ok_or_else(|| Error::invalid("Transaction recovery record disappeared."))?;
            if other_id != request_id && !other.phase().terminal() {
                return Err(Error::new(
                    Code::Busy,
                    "Another unfinished request prevents transaction index recovery.",
                ));
            }
        }
        // Explicit index recovery may retain its own newly acquired file, but cannot replace another actual terminal owner.
        // 明确索引恢复可保留自有新取得文件，但不能替换另一实际终态所有者。
        self.ensure_terminal_original_owners_retired_locked(plugin_id, Some(request_id))
    }

    /// Reject actual terminal owners in the complete strict inventory, excluding only the explicit `caller` request.
    /// 拒绝完整严格清单中的实际终态所有者，仅排除显式 `caller` 请求。
    /// The caller holds namespace exclusion; this single rule serves new writers and every original index recovery.
    /// 调用方持有命名空间互斥；此唯一规则服务新写入者及每个原索引恢复。
    fn ensure_terminal_original_owners_retired_locked(
        &self,
        plugin_id: &str,
        caller: Option<Uuid>,
    ) -> VulcanPluginStoreResult<()> {
        for request_id in self.transaction_ids_locked(plugin_id)? {
            // Decode every canonical full body before exclusion; no copied UUID or current pointer can hide corruption.
            // 排除前解码每个规范完整正文；复制 UUID 或当前指针不能隐藏损坏。
            let record = self
                .read_request_locked(plugin_id, request_id)?
                .ok_or_else(|| Error::invalid("Original terminal inventory record disappeared."))?;
            if Some(request_id) != caller
                && record.phase().terminal()
                && self.terminal_transaction_owner_is_live_locked(plugin_id, request_id)?
            {
                return Err(Error::new(
                    Code::Busy,
                    "Terminal activation still retains its original live owner.",
                ));
            }
        }
        Ok(())
    }

    /// Reject unfinished or actually still-owned terminal originals for `plugin_id` under the caller's namespace fence.
    /// 在调用方命名空间屏障下，拒绝 `plugin_id` 未完成或实际仍自有的终态原请求。
    pub(in crate::generation_store) fn ensure_no_pending_activation_locked(
        &self,
        plugin_id: &str,
    ) -> VulcanPluginStoreResult<()> {
        // Original Aborted does not retire its pending private predecessor handoff or still-live transfer guard.
        // 原 Aborted 不会退役其待处理私有前驱交接或仍存活移交守卫。
        self.ensure_no_predecessor_handoff_locked(plugin_id)?;
        self.check_unindexed_transaction_records(plugin_id)?;
        if self
            .indexed_request_locked(plugin_id)?
            .is_some_and(|record| !record.phase().terminal())
        {
            return Err(Error::new(
                Code::Busy,
                "Plugin activation has an unfinished transaction.",
            ));
        }
        // New writers must exclude all actual terminal owners, including strict unindexed historical records.
        // 新写入者必须排除全部实际终态所有者，包括严格未索引历史记录。
        self.ensure_terminal_original_owners_retired_locked(plugin_id, None)
    }

    /// Protect the exact package `identity` referenced by either side of the pending handoff, including after a crash.
    /// 保护待完成交接任一侧引用的精确插件包 `identity`，包括崩溃后。
    pub(in crate::generation_store) fn check_transaction_package_retention(
        &self,
        identity: &VulcanPluginGenerationIdentity,
    ) -> VulcanPluginStoreResult<()> {
        // A terminal target abort cannot make old package bytes collectible while their original handoff is pending.
        // 原交接仍待处理时，目标终态中止不能使旧包字节变为可回收。
        self.check_predecessor_handoff_package_retention_locked(identity)?;
        self.check_unindexed_transaction_records(&identity.plugin_id)?;
        if let Some(record) = self.indexed_request_locked(&identity.plugin_id)?
            && !record.phase().terminal()
            && (record.candidate() == Some(identity)
                || record
                    .predecessor()
                    .is_some_and(|activation| activation.generation.as_ref() == Some(identity)))
        {
            return Err(Error::new(
                Code::Busy,
                "Package is retained by an unfinished activation request.",
            ));
        }
        Ok(())
    }

    /// Protect `plugin_id`/`revision` retained by an unfinished handoff before any credential deletion begins.
    /// 在任何凭据删除开始前保护未完成交接保留的 `plugin_id`／`revision`。
    pub(in crate::generation_store) fn check_transaction_credential_retention(
        &self,
        plugin_id: &str,
        revision: &VulcanPluginCredentialRevision,
    ) -> VulcanPluginStoreResult<()> {
        // Pending original predecessor ownership protects the exact old revision after target activation aborts.
        // 目标激活中止后，待处理原前驱所有权保护精确旧修订。
        self.check_predecessor_handoff_credential_retention_locked(plugin_id, revision)?;
        self.check_unindexed_transaction_records(plugin_id)?;
        if let Some(record) = self.indexed_request_locked(plugin_id)?
            && !record.phase().terminal()
            && (record.credential_revision() == Some(revision)
                || record
                    .predecessor()
                    .is_some_and(|activation| activation.credential_revision() == Some(revision)))
        {
            return Err(Error::new(
                Code::Busy,
                "Credentials are retained by an unfinished activation request.",
            ));
        }
        Ok(())
    }

    /// Reject writes and collection for missing or stale indexes with unfinished originals; only explicit recovery repairs authority.
    /// 索引缺失或陈旧且存在未完成原请求时拒绝写入及回收；仅明确恢复修复权威。
    fn check_unindexed_transaction_records(&self, plugin_id: &str) -> VulcanPluginStoreResult<()> {
        // A failed index replacement may retain an older terminal pointer beside a newer unfinished original.
        // 索引替换失败可能使较旧终态指针与较新未完成原请求并存。
        let indexed = self.indexed_request_locked(plugin_id)?;
        // Enumerating originals establishes exclusion only; it never elects a replacement authority.
        // 枚举原请求仅建立互斥，不选举替代权威。
        let requests = self.transaction_ids_locked(plugin_id)?;
        if indexed.is_none() && !requests.is_empty() {
            return Err(Error::new(
                Code::Busy,
                "Activation transaction index is missing; inspect the original request.",
            ));
        }
        for request_id in requests {
            if indexed
                .as_ref()
                .is_some_and(|record| record.request_id() == request_id)
            {
                continue;
            }
            // Historical originals remain strict even when a different valid index exists.
            // 即使存在另一有效索引，历史原请求仍接受严格检查。
            let record = self
                .read_request_locked(plugin_id, request_id)?
                .ok_or_else(|| {
                    Error::invalid("Unindexed original disappeared during inspection.")
                })?;
            if !record.phase().terminal() {
                return Err(Error::new(
                    Code::Busy,
                    "An unfinished unindexed activation request requires explicit original recovery.",
                ));
            }
        }
        Ok(())
    }

    /// Query original `plugin_id`/`request_id` without resuming, taking ownership, or replaying any side effects.
    /// 查询原 `plugin_id`／`request_id`，不恢复、不取得所有权、不重放任何副作用。
    pub fn activation_transaction(
        &self,
        plugin_id: &str,
        request_id: Uuid,
    ) -> VulcanPluginStoreResult<Option<VulcanPluginActivationTransactionRecord>> {
        // The namespace fence orders this operation against every selection writer.
        // 命名空间屏障将此操作与全部选择写入者排序。
        let _namespace = self.namespace_lock(plugin_id, true)?;
        self.read_transaction_locked(plugin_id, request_id)
    }

    /// Discover original request IDs for `plugin_id`, including unindexed preparation and historical terminal receipts.
    /// 发现 `plugin_id` 的原请求标识，包括未索引准备及历史终态回执。
    /// Return only canonical filenames; each body must still be queried and validated by its exact ID.
    /// 仅返回规范文件名；每个正文仍须按精确标识查询并校验。
    pub fn activation_transaction_ids(
        &self,
        plugin_id: &str,
    ) -> VulcanPluginStoreResult<BTreeSet<Uuid>> {
        // Keep one namespace fence while enumerating the exact original request filenames.
        // 枚举精确原请求文件名期间保持同一命名空间屏障。
        let _namespace = self.namespace_lock(plugin_id, true)?;
        self.transaction_ids_locked(plugin_id)
    }

    /// Enumerate canonical request IDs for `plugin_id` while the caller holds its namespace fence.
    /// 在调用方持有命名空间屏障时枚举 `plugin_id` 的规范请求标识。
    /// Return exact filenames without selecting or repairing transaction authority.
    /// 返回精确文件标识，不选择或修复事务权威。
    pub(super) fn transaction_ids_locked(
        &self,
        plugin_id: &str,
    ) -> VulcanPluginStoreResult<BTreeSet<Uuid>> {
        // Missing optional storage remains a read-only empty inventory.
        // 可选存储缺失时保持只读空清单。
        let Some(directory) = self.transaction_directory(plugin_id, false)? else {
            return Ok(BTreeSet::new());
        };
        // Enumeration failures must not become an empty journal.
        // 枚举失败不得变成空日志。
        let entries = std::fs::read_dir(directory)
            .map_err(|error| Error::io("Enumerate activation transactions", error))?;
        // Stable ordering is for inspection only, never request selection.
        // 稳定排序仅用于检查，绝不用于选择请求。
        let mut requests = BTreeSet::new();
        for entry in entries {
            // Inspect literal filenames under the already validated namespace.
            // 检查已校验命名空间中的字面文件名。
            let path = entry
                .map_err(|error| Error::io("Read activation transaction entry", error))?
                .path();
            // ASCII aliases may resolve to the same file on Windows; discovery must reject them instead of hiding history.
            // ASCII 别名在 Windows 可能解析到同一文件；发现时须拒绝，不能隐藏历史。
            let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
                continue;
            };
            // The declared journal suffix is accepted only in its exact canonical spelling.
            // 声明日志后缀仅接受精确规范拼写。
            let canonical_extension = "json";
            if !extension.eq_ignore_ascii_case(canonical_extension) {
                continue;
            }
            if extension != canonical_extension {
                return Err(Error::invalid(
                    "Noncanonical activation transaction extension.",
                ));
            }
            // Canonical spelling prevents aliases for the original request.
            // 规范拼写防止原请求出现别名。
            let name = path
                .file_stem()
                .and_then(|name| name.to_str())
                .ok_or_else(|| Error::invalid("Invalid activation transaction filename."))?;
            // Nil identifiers are never valid durable request identities.
            // 空标识绝不是有效的持久请求身份。
            let request_id = Uuid::parse_str(name)
                .map_err(|_| Error::invalid("Invalid activation transaction filename."))?;
            if request_id.is_nil()
                || request_id.to_string() != name
                || !files::exists(&path, false)?
            {
                return Err(Error::invalid("Noncanonical activation transaction entry."));
            }
            requests.insert(request_id);
        }
        Ok(requests)
    }

    /// Inspect original unfinished records for `plugin_id` under one namespace fence.
    /// 在同一命名空间屏障内检查 `plugin_id` 的原始未完成记录。
    /// Return validated pending originals; reject damaged indexes and orphan installation receipts.
    /// 返回已校验的待处理原请求；拒绝损坏索引及孤立安装回执。
    pub fn activation_recovery_records(
        &self,
        plugin_id: &str,
    ) -> VulcanPluginStoreResult<Vec<VulcanPluginActivationRequestRecord>> {
        // Inventory and index validation must describe the same namespace observation.
        // 清单和索引校验必须描述同一次命名空间观察。
        let _namespace = self.namespace_lock(plugin_id, true)?;
        self.activation_recovery_records_locked(plugin_id)
    }

    /// Inspect every original record for plugin_id using the caller's actual retained namespace fence.
    /// 使用调用方实际保留的命名空间屏障检查 plugin_id 的全部原记录。
    /// Return the same validated unfinished inventory; never reopen the namespace or omit historical corruption checks.
    /// 返回相同已校验未完成清单；绝不重开命名空间或省略历史损坏检查。
    pub(in crate::generation_store) fn activation_recovery_records_locked(
        &self,
        plugin_id: &str,
    ) -> VulcanPluginStoreResult<Vec<VulcanPluginActivationRequestRecord>> {
        // Exact filenames do not grant recovery ownership.
        // 精确文件名不授予恢复所有权。
        let requests = self.transaction_ids_locked(plugin_id)?;
        // A dangling or corrupt index is an error even when no JSON entries exist.
        // 即使不存在 JSON 条目，悬空或损坏的索引也属于错误。
        let indexed = self.indexed_request_locked(plugin_id)?;
        // Retain only unfinished bodies after validating all historical originals.
        // 校验全部历史原请求后仅保留未完成正文。
        let mut pending = Vec::new();
        for request_id in &requests {
            // A disappearing original must never be interpreted as completion.
            // 原请求消失绝不能解释为完成。
            let record = self
                .read_request_locked(plugin_id, *request_id)?
                .ok_or_else(|| {
                    Error::invalid("Activation transaction disappeared during inspection.")
                })?;
            if !record.phase().terminal() {
                pending.push(record);
            }
        }
        if indexed.is_none() && !requests.is_empty() && pending.is_empty() {
            return Err(Error::invalid(
                "Activation index is missing from a terminal transaction history.",
            ));
        }
        if let Some(directory) = self.transaction_directory(plugin_id, false)? {
            // Candidate effect evidence cannot outlive its authoritative original request unnoticed.
            // 候选副作用证据失去权威原请求后不能被忽略。
            let entries = std::fs::read_dir(directory)
                .map_err(|error| Error::io("Inspect candidate effect receipts", error))?;
            for entry in entries {
                // Only the declared receipt extension belongs to this inventory.
                // 仅声明的回执扩展名属于此清单。
                let path = entry
                    .map_err(|error| Error::io("Read candidate effect receipt entry", error))?
                    .path();
                // The extension declares which strict decoder owns this sidecar.
                // 扩展名声明此附属文件归哪个严格解码器所有。
                let is_installation = match path.extension().and_then(|value| value.to_str()) {
                    Some("install") => true,
                    Some("effects") => false,
                    _ => continue,
                };
                // Validate canonical identity before relating the receipt to its original.
                // 关联回执与原请求之前校验规范身份。
                let name = path
                    .file_stem()
                    .and_then(|name| name.to_str())
                    .ok_or_else(|| Error::invalid("Invalid candidate effect receipt filename."))?;
                // A receipt is meaningful only together with its exact original record.
                // 回执仅与精确原始记录一起才有意义。
                let request_id = Uuid::parse_str(name)
                    .map_err(|_| Error::invalid("Invalid candidate effect receipt filename."))?;
                if request_id.is_nil()
                    || request_id.to_string() != name
                    || !files::exists(&path, false)?
                    || !requests.contains(&request_id)
                {
                    return Err(Error::invalid(
                        "Candidate effect receipt has no valid original transaction.",
                    ));
                }
                // Early intent cannot own installation evidence; configured historical receipts must match their full original.
                // 早期意图不能拥有候选副作用证据；已配置历史回执必须匹配其完整原请求。
                let Some(VulcanPluginActivationRequestRecord::Activation(original)) =
                    self.read_request_locked(plugin_id, request_id)?
                else {
                    return Err(Error::invalid(
                        "Candidate effect receipt belongs to an unconfigured original.",
                    ));
                };
                // Both formats retain the same complete original; neither is a replacement pointer.
                // 两种格式均保留相同完整原请求；均不属于替代指针。
                let missing = if is_installation {
                    self.read_installation_locked(&original)?.is_none()
                } else {
                    self.read_effects_locked(&original)?.is_none()
                };
                if missing {
                    return Err(Error::invalid(
                        "Candidate effect receipt disappeared during recovery inspection.",
                    ));
                }
            }
        }
        Ok(pending)
    }

    /// Discover journal namespaces without creating storage or replaying unfinished requests during startup.
    /// 发现日志命名空间，不创建存储，也不在启动期间重放未完成请求。
    /// Return canonical plugin IDs; each namespace and original request must be inspected separately.
    /// 返回规范插件标识；每个命名空间及原请求仍须分别检查。
    pub fn activation_transaction_plugin_ids(&self) -> VulcanPluginStoreResult<BTreeSet<String>> {
        self.validate_layout()?;
        // Optional absence remains read-only for all existing installations.
        // 对全部既有安装，可选缺失保持只读。
        let root = self.root.join("transactions");
        if !files::exists(&root, true)? {
            return Ok(BTreeSet::new());
        }
        // Each child is one literal namespace, not an arbitrary recursive path.
        // 每个子项是单个字面命名空间，而非任意递归路径。
        let entries = std::fs::read_dir(root)
            .map_err(|error| Error::io("Enumerate transaction namespaces", error))?;
        // Sorted IDs give recovery supervisors a deterministic inspection order.
        // 排序标识为恢复监督器提供确定的检查顺序。
        let mut plugin_ids = BTreeSet::new();
        for entry in entries {
            // Filesystem failures never become an empty discovery result.
            // 文件系统错误绝不变成空发现结果。
            let path = entry
                .map_err(|error| Error::io("Read transaction namespace", error))?
                .path();
            // Validate the literal name before accessing any descendant.
            // 访问任何后代前先校验字面名称。
            let plugin_id = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| Error::invalid("Transaction namespace is not UTF-8."))?;
            crate::catalog::validate_identifier(plugin_id, "transaction plugin id")
                .map_err(Error::invalid)?;
            if !files::exists(&path, true)? {
                return Err(Error::invalid("Transaction namespace disappeared."));
            }
            plugin_ids.insert(plugin_id.to_owned());
        }
        Ok(plugin_ids)
    }
}
