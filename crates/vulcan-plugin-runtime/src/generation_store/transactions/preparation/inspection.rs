//! Inspect actual request history through the reservation's already retained namespace owner.
//! 通过预留已保留的命名空间所有者检查实际请求历史。

use super::*;
use std::collections::{BTreeMap, BTreeSet};

/// Read-only actual checkpoint and complete request-sidecar history, without granting mutation ownership.
/// 只读实际检查点和完整请求附属历史，不授予变更所有权。
#[derive(Clone)]
pub struct VulcanPluginActivationPreparationInspection {
    /// Actual original Preparing body; absence remains absence before its first durable write.
    /// 实际原 Preparing 正文；首次持久写入之前缺失保持缺失。
    original: Option<VulcanPluginActivationPreparationRecord>,
    /// Actual complete indexed request, including the historical baseline before publication.
    /// 实际完整索引请求，包括发布之前的历史基线。
    indexed: Option<VulcanPluginActivationRequestRecord>,
    /// All actual original journal bodies, indexed by their canonical request identities.
    /// 全部实际原日志正文，按其规范请求身份索引。
    requests: BTreeMap<Uuid, VulcanPluginActivationRequestRecord>,
    /// Actual effects for every journal; None means its validated physical absence.
    /// 每份日志的实际副作用；None 表示已校验的物理缺失。
    effects: BTreeMap<Uuid, Option<VulcanPluginActivationEffects>>,
    /// Actual installation observations for every journal; None means validated absence.
    /// 每份日志的实际安装观察；None 表示已校验缺失。
    installations: BTreeMap<Uuid, Option<VulcanPluginInstallationReceipt>>,
    /// Actual target-hook initialization observations for every journal.
    /// 每份日志的实际目标钩子初始化观察。
    target_hooks: BTreeMap<Uuid, Option<VulcanPluginTargetHookInitializationReceipt>>,
    /// Actual target-MCP initialization observations for every journal.
    /// 每份日志的实际目标 MCP 初始化观察。
    target_mcp: BTreeMap<Uuid, Option<VulcanPluginTargetMcpInitializationReceipt>>,
}

impl std::fmt::Debug for VulcanPluginActivationPreparationInspection {
    /// Render only checkpoint identities and history size, without printing configuration bodies.
    /// 仅呈现检查点身份和历史数量，不打印配置正文。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("VulcanPluginActivationPreparationInspection")
            .field(
                "original",
                &self.original.as_ref().map(|record| record.request_id),
            )
            .field(
                "indexed",
                &self.indexed.as_ref().map(|record| record.request_id()),
            )
            .field("request_count", &self.requests.len())
            .finish()
    }
}

impl VulcanPluginActivationPreparationInspection {
    /// Return the actual original body, or None when no original journal has been written.
    /// 返回实际原正文；尚未写入原日志时返回 None。
    pub fn original(&self) -> Option<&VulcanPluginActivationPreparationRecord> {
        self.original.as_ref()
    }

    /// Return the actual full index observation, without manufacturing a new request.
    /// 返回实际完整索引观察，不构造新请求。
    pub fn indexed(&self) -> Option<&VulcanPluginActivationRequestRecord> {
        self.indexed.as_ref()
    }

    /// Return all validated historical original bodies, including terminal requests.
    /// 返回全部已校验历史原正文，包括终态请求。
    pub fn requests(&self) -> &BTreeMap<Uuid, VulcanPluginActivationRequestRecord> {
        &self.requests
    }

    /// Return actual effect observations by journal identity; each None is confirmed absence.
    /// 按日志身份返回实际副作用观察；每个 None 均为已确认缺失。
    pub fn effects(&self) -> &BTreeMap<Uuid, Option<VulcanPluginActivationEffects>> {
        &self.effects
    }

    /// Return actual installation observations by journal identity.
    /// 按日志身份返回实际安装观察。
    pub fn installations(&self) -> &BTreeMap<Uuid, Option<VulcanPluginInstallationReceipt>> {
        &self.installations
    }

    /// Return actual target-hook initialization observations by journal identity.
    /// 按日志身份返回实际目标钩子初始化观察。
    pub fn target_hooks(
        &self,
    ) -> &BTreeMap<Uuid, Option<VulcanPluginTargetHookInitializationReceipt>> {
        &self.target_hooks
    }

    /// Return actual target-MCP initialization observations by journal identity.
    /// 按日志身份返回实际目标 MCP 初始化观察。
    pub fn target_mcp(
        &self,
    ) -> &BTreeMap<Uuid, Option<VulcanPluginTargetMcpInitializationReceipt>> {
        &self.target_mcp
    }
}

/// A declared request-sidecar format, used only to validate actual directory observations.
/// 声明的请求附属格式，仅用于校验实际目录观察。
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Sidecar {
    /// Original callback-effect receipt.
    /// 原回调副作用回执。
    Effects,
    /// Original installation receipt.
    /// 原安装回执。
    Installation,
    /// Original target-hook initialization receipt.
    /// 原目标钩子初始化回执。
    TargetHook,
    /// Original target-MCP initialization receipt.
    /// 原目标 MCP 初始化回执。
    TargetMcp,
}

impl VulcanPluginGenerationStore {
    /// Project actual history for this original Preparing owner under its caller's already held namespace File.
    /// 在调用方已经持有命名空间 File 下，为此原 Preparing 所有者投影实际历史。
    /// Validate full original ownership and borrow the actual indexed record; no namespace/request lock or temporary Arc is acquired.
    /// 校验完整原所有权并借用实际索引记录；不取得命名空间／请求锁或临时 Arc。
    pub(in crate::generation_store) fn inspect_activation_preparation_owner_locked(
        &self,
        preparation: &VulcanPluginActivationPreparation,
    ) -> VulcanPluginStoreResult<VulcanPluginActivationPreparationInspection> {
        self.check_preparation_owner(preparation)?;
        // The complete actual indexed body was verified against this original owner under the same physical fence.
        // 完整实际索引正文已在同一物理屏障下与此原所有者核对。
        let indexed = self.indexed_request_locked(&preparation.intent().generation().plugin_id)?;
        // Projection starts from the actual decoded indexed Preparing body, never from an intended credential phase.
        // 投影从实际解码索引 Preparing 正文开始，绝不从预定凭据阶段开始。
        let original = match indexed.as_ref() {
            Some(VulcanPluginActivationRequestRecord::Preparation(record))
                if record == &preparation.record =>
            {
                Some(record.clone())
            }
            _ => {
                return Err(Error::new(
                    Code::Conflict,
                    "Original indexed Preparing record changed during owned inspection.",
                ));
            }
        };
        self.preparation_inspection_history_locked(
            &preparation.intent().generation().plugin_id,
            original,
            indexed,
        )
    }

    /// Project complete strict history from already verified actual original/index observations under a held namespace.
    /// 在持有命名空间下，从已经验证实际原请求／索引观察投影完整严格历史。
    /// Original may be absent only for the existing unpersisted preparation reservation contract; no lock or record is created.
    /// 原请求仅可在既有未持久准备预留契约中缺失；不创建锁或记录。
    fn preparation_inspection_history_locked(
        &self,
        plugin_id: &str,
        original: Option<VulcanPluginActivationPreparationRecord>,
        indexed: Option<VulcanPluginActivationRequestRecord>,
    ) -> VulcanPluginStoreResult<VulcanPluginActivationPreparationInspection> {
        // Inventory includes terminal history and excludes an unpersisted intended original.
        // 清单包含终态历史，并排除尚未持久保存的预定原请求。
        let request_ids = self.transaction_ids_locked(plugin_id)?;
        // Each observed sidecar must belong to a canonical actual journal, even when its decoder would otherwise be skipped.
        // 每份已观察附属文件均须属于规范实际日志，即使其解码器原本会被跳过。
        let sidecars = self.enumerate_preparation_sidecars_locked(plugin_id, &request_ids)?;
        // Maps retain actual complete bodies and validated per-original absence together.
        // 映射一并保留实际完整正文及已校验逐原请求缺失。
        let mut inspection = VulcanPluginActivationPreparationInspection {
            original,
            indexed,
            requests: BTreeMap::new(),
            effects: BTreeMap::new(),
            installations: BTreeMap::new(),
            target_hooks: BTreeMap::new(),
            target_mcp: BTreeMap::new(),
        };
        for request_id in request_ids {
            // A disappeared observed journal is an error, not a projected intended body.
            // 已观察日志消失属于错误，而不是投影预定正文。
            let request = self
                .read_request_locked(plugin_id, request_id)?
                .ok_or_else(|| {
                    Error::invalid("Original journal disappeared during reserved inspection.")
                })?;
            // All four readers use their unique existing paths and full-original validators.
            // 四个读取器均使用各自唯一既有路径及完整原请求校验器。
            let (effects, installation, hooks, mcp) = match &request {
                VulcanPluginActivationRequestRecord::Activation(record) => (
                    self.read_effects_locked(record)?,
                    self.read_installation_locked(record)?,
                    self.read_target_hook_locked(record)?,
                    self.read_target_mcp_locked(record)?,
                ),
                VulcanPluginActivationRequestRecord::Preparation(_) => {
                    if sidecars.contains_key(&request_id) {
                        return Err(Error::invalid(
                            "Early preparation cannot own request sidecars.",
                        ));
                    }
                    (None, None, None, None)
                }
            };
            if let Some(observed) = sidecars.get(&request_id)
                && ((observed.contains(&Sidecar::Effects) && effects.is_none())
                    || (observed.contains(&Sidecar::Installation) && installation.is_none())
                    || (observed.contains(&Sidecar::TargetHook) && hooks.is_none())
                    || (observed.contains(&Sidecar::TargetMcp) && mcp.is_none()))
            {
                return Err(Error::invalid(
                    "Observed request sidecar disappeared during reserved inspection.",
                ));
            }
            inspection.effects.insert(request_id, effects);
            inspection.installations.insert(request_id, installation);
            inspection.target_hooks.insert(request_id, hooks);
            inspection.target_mcp.insert(request_id, mcp);
            inspection.requests.insert(request_id, request);
        }
        Ok(inspection)
    }

    /// Enumerate actual request sidecars under the caller-held namespace using the unique canonical suffix and identity rules.
    /// 在调用方持有命名空间内按唯一规范后缀及身份规则枚举实际请求附属文件。
    /// Return canonical per-original evidence kinds; reject ASCII aliases, orphan identities and nonregular entries without writing.
    /// 返回规范逐原请求证据类型；拒绝 ASCII 别名、孤立身份及非普通条目，不执行写入。
    pub(super) fn enumerate_preparation_sidecars_locked(
        &self,
        plugin_id: &str,
        request_ids: &BTreeSet<Uuid>,
    ) -> VulcanPluginStoreResult<BTreeMap<Uuid, BTreeSet<Sidecar>>> {
        // Physical entry observations are grouped only by their validated original request identities.
        // 物理条目观察仅按已校验原请求身份分组。
        let mut sidecars: BTreeMap<Uuid, BTreeSet<Sidecar>> = BTreeMap::new();
        if let Some(directory) = self.transaction_directory(plugin_id, false)? {
            // Reading this directory cannot allocate or repair any storage.
            // 读取该目录不能分配或修复任何存储。
            let entries = std::fs::read_dir(directory)
                .map_err(|error| Error::io("Inspect reserved request sidecars", error))?;
            for entry in entries {
                // The literal extension determines the existing strict decoder, without probing alternate paths.
                // 字面扩展名确定既有严格解码器，不探测替代路径。
                let path = entry
                    .map_err(|error| Error::io("Read reserved request sidecar entry", error))?
                    .path();
                // ASCII aliases are discovered only to reject them; canonical readers must never conceal their physical evidence.
                // 发现 ASCII 别名仅用于拒绝；规范读取器绝不能隐藏其物理证据。
                let Some(extension) = path.extension().and_then(|extension| extension.to_str())
                else {
                    continue;
                };
                // Each declared suffix has one spelling; case-insensitive discovery confers no decoding permission.
                // 每个声明后缀具有唯一拼写；不区分大小写的发现不授予解码许可。
                let Some((canonical_extension, kind)) = [
                    ("effects", Sidecar::Effects),
                    ("install", Sidecar::Installation),
                    ("target-hook", Sidecar::TargetHook),
                    ("target-mcp", Sidecar::TargetMcp),
                ]
                .into_iter()
                .find(|(canonical, _)| extension.eq_ignore_ascii_case(canonical)) else {
                    continue;
                };
                if extension != canonical_extension {
                    return Err(Error::invalid(
                        "Noncanonical reserved request sidecar extension.",
                    ));
                }
                // Canonical names prevent aliases from concealing unmatched durable evidence.
                // 规范名称防止别名隐藏不匹配持久证据。
                let name = path
                    .file_stem()
                    .and_then(|name| name.to_str())
                    .ok_or_else(|| Error::invalid("Invalid reserved request sidecar filename."))?;
                // Nil identities and receipts without an actual original are always invalid.
                // 空身份及没有实际原请求的回执始终无效。
                let request_id = Uuid::parse_str(name)
                    .map_err(|_| Error::invalid("Invalid reserved request sidecar filename."))?;
                if request_id.is_nil()
                    || request_id.to_string() != name
                    || !files::exists(&path, false)?
                    || !request_ids.contains(&request_id)
                {
                    return Err(Error::invalid(
                        "Reserved request sidecar has no canonical actual original.",
                    ));
                }
                sidecars.entry(request_id).or_default().insert(kind);
            }
        }
        Ok(sidecars)
    }

    /// Inspect reservation's actual checkpoint and all request-sidecar history inside its retained namespace file.
    /// 在预留保留的命名空间文件内检查其实际检查点及全部请求附属历史。
    /// Return only decoded durable evidence; reject changed ownership, orphan receipts and impossible early effects.
    /// 仅返回已解码持久证据；拒绝所有权变化、孤立回执及不可能的早期副作用。
    pub fn inspect_activation_preparation_reservation(
        &self,
        reservation: &VulcanPluginActivationPreparationReservation,
    ) -> VulcanPluginStoreResult<VulcanPluginActivationPreparationInspection> {
        // Original absence and historical index retain their exact first-publication checkpoint semantics.
        // 原缺失及历史索引保持精确首次发布检查点语义。
        let (original, indexed) = self.preparation_reservation_checkpoint(reservation)?;
        self.preparation_inspection_history_locked(
            &reservation.intent().generation().plugin_id,
            original,
            indexed,
        )
    }
}
