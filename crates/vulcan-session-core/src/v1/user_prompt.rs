//! Host-owned media delivery parameters attached to one chronological user prompt.
//! 附着在单条时序用户提示词上的宿主媒体投递参数。

use serde::{Deserialize, Serialize};

use super::{ClientActionId, DomainError, ResourceId, Seq};

/// Versioned prompt parameters; absence in an old event means unknown history, never new intent.
/// 版本化提示词参数；旧事件缺失该字段表示历史未知，绝不表示新意图。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserPromptParameters {
    /// Exact supported parameter contract version.
    /// 精确受支持的参数契约版本。
    pub version: u32,
    /// Immutable preparation facts handed in with this prompt, in exact resource order; empty means no prior preparation proof.
    /// 随本提示词交入的不可变准备事实，按精确资源顺序排列；空表示没有先前准备证明。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prepared_resources: Vec<Option<UserPromptResourceDelivery>>,
    /// Ordered positions inherited without original delivery proof; these may only expose attachment metadata, never become fresh upload intent.
    /// 缺少原投递证明的继承位置，按序排列；只能暴露附件元数据，绝不能成为新上传意图。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unproven_resource_positions: Vec<usize>,
    /// Original media preparation state owned only by this prompt occurrence.
    /// 仅由本次提示词出现位置持有的原媒体准备状态。
    pub media_delivery: UserPromptMediaDelivery,
}

/// Forward-only preparation state with write-once resource origins, independent of later request-local attachment projections.
/// 单向推进且资源原身份只写一次的准备状态，独立于后续请求局部附件投影。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum UserPromptMediaDelivery {
    /// A newly admitted prompt whose first physical preparation has not been frozen yet.
    /// 新准入提示词，其首次物理准备尚未冻结。
    Pending,
    /// Original choices and completed resource occurrences retained before a first request is permitted; undecided positions remain explicit.
    /// 首次请求获准之前保留的原选择及已完成资源出现；尚未决定的位置保持显式。
    Preparing {
        /// Exact ordered results, including unchanged admission proof; no prompt-wide request route is claimed yet.
        /// 精确有序结果，包含未变的准入证明；尚不声称整条提示词的请求路线。
        resources: Vec<Option<UserPromptResourceDelivery>>,
    },
    /// The immutable original route and ordered per-resource delivery facts.
    /// 不可变的原路线及有序逐资源投递事实。
    Frozen {
        /// Exact facts frozen before the first physical API following this prompt.
        /// 在本提示词之后首次物理 API 之前冻结的精确事实。
        origin: UserPromptMediaOrigin,
    },
}

/// One prompt's first request route and original resource occurrences; no media bytes or provider URLs are stored here.
/// 单条提示词的首次请求路线及原资源出现位置；此处不存储媒体字节或供应商URL。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserPromptMediaOrigin {
    /// Exact route of the first physical request; individual resources may have been prepared earlier under other models.
    /// 首次物理请求的精确路线；各个资源可能更早在其他模型下准备。
    pub route: UserPromptMediaRoute,
    /// One result for each resource occurrence in the prompt's original order.
    /// 按提示词原始顺序为每次资源出现保存一个结果。
    pub resources: Vec<UserPromptResourceDelivery>,
}

/// Non-secret original route identity used for reversible A-to-B-to-A projection.
/// 用于可逆A到B到A投影的非秘密原路线身份。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserPromptMediaRoute {
    /// Provider that owns the original request.
    /// 持有原请求的供应商。
    pub provider_id: String,
    /// Exact service entry selected for the original request.
    /// 原请求选择的精确服务入口。
    pub service_entry_id: String,
    /// Original credential connection identity, not its secret or token.
    /// 原凭据连接身份，不是其秘密或令牌。
    pub connection_id: String,
    /// Exact model identity; models sharing a provider remain different targets.
    /// 精确模型身份；共享供应商的模型仍是不同目标。
    pub model_id: String,
    /// Exact original protocol profile without protocol fallback.
    /// 精确原协议配置，不进行协议回退。
    pub protocol_profile_id: String,
}

/// One ordered resource occurrence and its immutable original representation.
/// 一次有序资源出现及其不可变原表示形式。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserPromptResourceDelivery {
    /// Stable project resource identity at this exact position in the prompt.
    /// 提示词此精确位置上的稳定项目资源身份。
    pub resource_id: ResourceId,
    /// Exact route that originally selected this occurrence's delivery, not the prompt-wide first request or latest cache owner.
    /// 最初选择本次出现投递的精确路线，不是整条提示词首次请求或最新缓存所有者。
    /// For an explicitly unproven position this describes only the first address projection; it does not claim an original native-media route.
    /// 对显式未经证明的位置，此值仅描述首次地址投影，不声称原原生媒体路线。
    pub original_route: UserPromptMediaRoute,
    /// Representation actually selected for this prompt, not a later replay view.
    /// 为本提示词实际选择的表示，不是后续重放视图。
    pub delivery: UserPromptResourceDeliveryKind,
}

/// Closed original delivery forms; uploaded references remain in the existing private reference store.
/// 封闭的原投递形式；上传引用仍留在现有私有引用存储中。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum UserPromptResourceDeliveryKind {
    /// Original source bytes were selected for inline delivery.
    /// 原始源字节被选择为内联投递。
    Inline,
    /// Original selection captured before a validated reference existed; an exact draft link may later resolve private completion without rewriting this proof.
    /// 已验证引用出现前捕获的原选择；精确草稿关联之后可解析私有完成，不改写本证明。
    UploadPending {
        /// Exact original draft intent for late receipt handoff; absent for direct V1 preparation and legacy history.
        /// 晚到回执交接使用的精确原草稿意图；直接V1准备及旧历史缺失此字段。
        /// Boxed immutable text preserves the wire string while keeping the surrounding hot-path enum compact.
        /// 装箱不可变文本保留线路字符串形态，同时保持外围热路径枚举紧凑。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        draft_intent_id: Option<Box<str>>,
        /// Exact manifest channel selected by this occurrence's original preparation.
        /// 本次出现原准备选择的精确清单通道。
        upload_channel_id: String,
        /// SHA-256 fingerprint of the selected immutable upload contract.
        /// 已选不可变上传契约的SHA-256指纹。
        upload_contract_fingerprint: String,
    },
    /// An exact existing upload receipt was selected, identified without exposing its provider reference.
    /// 选择了一个精确已有上传回执，通过身份定位但不暴露其供应商引用。
    Uploaded {
        /// Manifest channel that produced the original reference.
        /// 产出原引用的清单通道。
        upload_channel_id: String,
        /// SHA-256 fingerprint of the exact upload contract.
        /// 精确上传契约的SHA-256指纹。
        upload_contract_fingerprint: String,
        /// SHA-256 identity of the original private upload receipt, not a URL or file identifier.
        /// 原私有上传回执的SHA-256身份，不是URL或文件标识。
        reference_fingerprint: String,
    },
    /// The first request exposed only safe attachment metadata and no native media.
    /// 首次请求仅暴露安全附件元数据，没有原生媒体。
    Attachment,
}

impl UserPromptResourceDelivery {
    /// Returns whether this completed result only fills the missing receipt of the exact `selected` upload occurrence.
    /// 返回本已完成结果是否仅补齐精确`selected`上传出现所缺回执。
    /// Resource, original route and selected contract must remain identical; this never authorizes a new upload or historical rewrite.
    /// 资源、原路线及所选契约必须保持相同；此方法绝不授权新上传或历史改写。
    pub(crate) fn completes_upload(&self, selected: &Self) -> bool {
        if self.resource_id != selected.resource_id
            || self.original_route != selected.original_route
        {
            return false;
        }
        match (&selected.delivery, &self.delivery) {
            (
                UserPromptResourceDeliveryKind::UploadPending {
                    draft_intent_id: None,
                    upload_channel_id,
                    upload_contract_fingerprint,
                },
                UserPromptResourceDeliveryKind::Uploaded {
                    upload_channel_id: completed_channel,
                    upload_contract_fingerprint: completed_contract,
                    ..
                },
            ) => {
                upload_channel_id == completed_channel
                    && upload_contract_fingerprint == completed_contract
            }
            _ => false,
        }
    }
}

/// Host-supplied binding for one exact prompt event in a strictly increasing chronological batch.
/// 宿主为严格递增时序批次中的一个精确提示词事件提供的绑定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserPromptMediaBinding {
    /// Existing user prompt event sequence, never a cache timestamp or latest-message guess.
    /// 既有用户提示词事件序号，绝不是缓存时间戳或对最近消息的猜测。
    pub prompt_seq: Seq,
    /// The complete original route and ordered results to freeze once.
    /// 将被一次性冻结的完整原路线及有序结果。
    pub origin: UserPromptMediaOrigin,
}

/// One selected or completed resource fact addressed to its original prompt occurrence, never to a file-global cache entry.
/// 定位到原提示词出现的一项已选或已完成资源事实，绝不定位到文件全局缓存条目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserPromptResourceBinding {
    /// Existing chronological user input event that owns this result.
    /// 持有本结果的既有时序用户输入事件。
    pub prompt_seq: Seq,
    /// Zero-based resource occurrence; equal resource ids at other positions are distinct intentions.
    /// 零基资源出现位置；其他位置相同资源ID仍是独立意图。
    pub resource_position: usize,
    /// Actual original choice or result with its exact route and, only after completion, opaque receipt identity.
    /// 带精确路线的真实原选择或结果；仅在完成后具有不透明回执身份。
    pub resource: UserPromptResourceDelivery,
}

/// Exact resource occurrence in an accepted user input belonging to the current session.
/// 当前会话中一条已接纳用户输入的精确资源出现。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserPromptResourceSource {
    /// Immutable client action owning the source input; never a latest-message selector.
    /// 持有来源输入的不可变客户端动作，绝不是最近消息选择器。
    pub client_action_id: ClientActionId,
    /// Zero-based position in that input's ordered ResourceIds, preserving duplicate resource occurrences.
    /// 该输入有序ResourceId中的零基位置，保留重复资源出现。
    pub resource_position: usize,
}

/// Host-only result of resolving one source occurrence without guessing unknown historical delivery.
/// 解析一次来源出现的仅宿主结果，不猜测未知历史投递。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserPromptResourcePreparation {
    /// The source input explicitly owns a new intent that has not selected a representation yet.
    /// 来源输入明确持有尚未选择表示形式的新意图。
    Pending,
    /// Exact known original preparation, including the selected route and opaque receipt identity.
    /// 精确已知原准备，包含选中路线及不透明回执身份。
    Prepared(UserPromptResourceDelivery),
    /// The source has no verifiable original preparation and must remain an address-only attachment.
    /// 来源没有可验证原准备，必须保持仅地址附件。
    Unproven,
}

impl UserPromptParameters {
    /// Builds a newly admitted prompt from exact ordered `resource_ids` and host-resolved `preparations` without converting unknown history into new intent.
    /// 根据精确有序`resource_ids`和宿主解析的`preparations`构建新准入提示词，不将未知历史转换为新意图。
    /// Returns absent parameters for text-only input, otherwise validated pending proof with explicit unknown positions.
    /// 纯文本输入返回无参数，否则返回带显式未知位置的已校验待准备证明。
    pub fn for_resource_preparations(
        resource_ids: &[ResourceId],
        preparations: Vec<UserPromptResourcePreparation>,
    ) -> Result<Option<Self>, DomainError> {
        if resource_ids.len() != preparations.len() {
            return Err(invalid_prompt_media("source_preparation_count"));
        }
        // Empty media input remains text-only, preserving the existing absent-parameter contract.
        // 空媒体输入保持纯文本，保留既有无参数契约。
        let Some(mut parameters) = Self::for_new_resources(resource_ids) else {
            return Ok(None);
        };
        for (position, preparation) in preparations.into_iter().enumerate() {
            match preparation {
                UserPromptResourcePreparation::Prepared(resource) => {
                    parameters.prepared_resources.push(Some(resource))
                }
                UserPromptResourcePreparation::Pending => parameters.prepared_resources.push(None),
                UserPromptResourcePreparation::Unproven => {
                    parameters.prepared_resources.push(None);
                    parameters.unproven_resource_positions.push(position);
                }
            }
        }
        if parameters.prepared_resources.iter().all(Option::is_none) {
            parameters.prepared_resources.clear();
        }
        parameters.validate_pending(resource_ids)?;
        Ok(Some(parameters))
    }

    /// Creates pending parameters only for a newly admitted input with `resource_ids`; text-only inputs need none.
    /// 仅为带有`resource_ids`的新准入输入创建待准备参数；纯文本输入无需参数。
    /// This constructor must never be used to hydrate old events whose parameter field is absent.
    /// 绝不能使用此构造器补水参数字段缺失的旧事件。
    pub fn for_new_resources(resource_ids: &[ResourceId]) -> Option<Self> {
        (!resource_ids.is_empty()).then_some(Self {
            version: 1,
            prepared_resources: Vec::new(),
            unproven_resource_positions: Vec::new(),
            media_delivery: UserPromptMediaDelivery::Pending,
        })
    }

    /// Validates this parameter contract against the exact ordered `resource_ids`, returning a domain error on drift.
    /// 根据精确有序`resource_ids`校验本参数契约，漂移时返回领域错误。
    pub fn validate(&self, resource_ids: &[ResourceId]) -> Result<(), DomainError> {
        if self.version != 1 || resource_ids.is_empty() {
            return Err(invalid_prompt_media("version_or_empty_resources"));
        }
        if !self.prepared_resources.is_empty() {
            if self.prepared_resources.len() != resource_ids.len() {
                return Err(invalid_prompt_media("prepared_resource_count"));
            }
            for (prepared, resource_id) in self.prepared_resources.iter().zip(resource_ids) {
                if let Some(prepared) = prepared {
                    validate_resource_delivery(prepared, resource_id)?;
                }
            }
        }
        if self
            .unproven_resource_positions
            .windows(2)
            .any(|positions| positions[0] >= positions[1])
            || self.unproven_resource_positions.iter().any(|position| {
                *position >= resource_ids.len()
                    || self
                        .prepared_resources
                        .get(*position)
                        .is_some_and(Option::is_some)
            })
        {
            return Err(invalid_prompt_media("unproven_resource_positions"));
        }
        match &self.media_delivery {
            UserPromptMediaDelivery::Pending => {}
            UserPromptMediaDelivery::Preparing { resources } => {
                if resources.len() != resource_ids.len() || resources.iter().all(Option::is_none) {
                    return Err(invalid_prompt_media("preparing_resource_count"));
                }
                for (position, (resource, resource_id)) in
                    resources.iter().zip(resource_ids).enumerate()
                {
                    if let Some(resource) = resource {
                        validate_resource_delivery(resource, resource_id)?;
                        self.validate_unproven_delivery(position, resource)?;
                    }
                }
                for (prepared, resource) in self.prepared_resources.iter().zip(resources) {
                    if prepared.is_some() && prepared != resource {
                        return Err(invalid_prompt_media("prepared_delivery_is_immutable"));
                    }
                }
            }
            UserPromptMediaDelivery::Frozen { origin } => {
                origin.validate(resource_ids)?;
                self.validate_preserved_preparation(origin)?;
            }
        }
        Ok(())
    }

    /// Validates preparation parameters on an admitted but not yet dispatched input, rejecting forged frozen request history.
    /// 校验已准入但尚未分派输入上的准备参数，拒绝伪造的冻结请求历史。
    pub fn validate_pending(&self, resource_ids: &[ResourceId]) -> Result<(), DomainError> {
        self.validate(resource_ids)?;
        if !matches!(self.media_delivery, UserPromptMediaDelivery::Pending) {
            return Err(invalid_prompt_media("input_already_frozen"));
        }
        Ok(())
    }

    /// Checks that final `origin` preserves every already prepared occurrence; later request projections may not replace them.
    /// 检查最终`origin`保留每个已经准备的出现；后续请求投影不能替换这些事实。
    pub(crate) fn validate_preserved_preparation(
        &self,
        origin: &UserPromptMediaOrigin,
    ) -> Result<(), DomainError> {
        if !self.prepared_resources.is_empty()
            && self.prepared_resources.len() != origin.resources.len()
        {
            return Err(invalid_prompt_media("prepared_resource_count"));
        }
        for (prepared, frozen) in self.prepared_resources.iter().zip(&origin.resources) {
            if prepared.as_ref().is_some_and(|prepared| prepared != frozen) {
                return Err(invalid_prompt_media("prepared_delivery_is_immutable"));
            }
        }
        if let UserPromptMediaDelivery::Preparing { resources } = &self.media_delivery {
            if resources.len() != origin.resources.len() {
                return Err(invalid_prompt_media("preparing_resource_count"));
            }
            for (prepared, frozen) in resources.iter().zip(&origin.resources) {
                if prepared.as_ref().is_some_and(|prepared| prepared != frozen) {
                    return Err(invalid_prompt_media("completed_delivery_is_immutable"));
                }
            }
        }
        for position in &self.unproven_resource_positions {
            if !origin.resources.get(*position).is_some_and(|resource| {
                resource.delivery == UserPromptResourceDeliveryKind::Attachment
            }) {
                return Err(invalid_prompt_media("unproven_resource_native_delivery"));
            }
        }
        Ok(())
    }

    /// Returns the known original result at `position` from the exact declared preparation state, or none for unfinished intent.
    /// 从精确声明的准备状态返回`position`处已知原结果；尚未完成的意图返回空。
    /// Callers validate resource order first; this does not grant unknown or historical positions new preparation authority.
    /// 调用方须先校验资源顺序；此方法不向未知或历史位置授予新准备权限。
    pub fn original_resource(&self, position: usize) -> Option<&UserPromptResourceDelivery> {
        match &self.media_delivery {
            UserPromptMediaDelivery::Pending => self
                .prepared_resources
                .get(position)
                .and_then(Option::as_ref),
            UserPromptMediaDelivery::Preparing { resources } => {
                resources.get(position).and_then(Option::as_ref)
            }
            UserPromptMediaDelivery::Frozen { origin } => origin.resources.get(position),
        }
    }

    /// Rejects native `resource` results at an explicitly unproven `position`, returning a safe domain invariant error.
    /// 拒绝显式未经证明`position`上的原生`resource`结果，返回安全领域不变量错误。
    fn validate_unproven_delivery(
        &self,
        position: usize,
        resource: &UserPromptResourceDelivery,
    ) -> Result<(), DomainError> {
        if self
            .unproven_resource_positions
            .binary_search(&position)
            .is_ok()
            && resource.delivery != UserPromptResourceDeliveryKind::Attachment
        {
            return Err(invalid_prompt_media("unproven_resource_native_delivery"));
        }
        Ok(())
    }
}

/// Compares immutable admission proof in `left` and `right`, independently of later physical-request freezing.
/// 比较`left`与`right`中的不可变准入证明，独立于后续物理请求冻结。
/// Legacy absent parameters and an empty proof both mean no claimed prior preparation; neither rewrites history.
/// 旧缺失参数及空证明均表示没有声称先前准备；二者均不改写历史。
pub(crate) fn initial_preparation_matches(
    left: Option<&UserPromptParameters>,
    right: Option<&UserPromptParameters>,
) -> bool {
    left.map(|parameters| parameters.prepared_resources.as_slice())
        .unwrap_or(&[])
        == right
            .map(|parameters| parameters.prepared_resources.as_slice())
            .unwrap_or(&[])
        && left
            .map(|parameters| parameters.unproven_resource_positions.as_slice())
            .unwrap_or(&[])
            == right
                .map(|parameters| parameters.unproven_resource_positions.as_slice())
                .unwrap_or(&[])
}

impl UserPromptMediaOrigin {
    /// Checks route identity, exact resource order and opaque upload identities against `resource_ids`.
    /// 根据`resource_ids`检查路线身份、精确资源顺序及不透明上传身份。
    /// Returns an error before any document mutation; no provider URL or payload is accepted.
    /// 在任何文档修改前返回错误；不接受供应商URL或载荷。
    pub fn validate(&self, resource_ids: &[ResourceId]) -> Result<(), DomainError> {
        self.route.validate()?;
        if resource_ids.is_empty() || self.resources.len() != resource_ids.len() {
            return Err(invalid_prompt_media("resource_count"));
        }
        for (entry, resource_id) in self.resources.iter().zip(resource_ids) {
            validate_resource_delivery(entry, resource_id)?;
        }
        Ok(())
    }
}

/// Validates one exact occurrence `entry` against `resource_id`, including original route and opaque receipt identity.
/// 根据`resource_id`校验一个精确出现`entry`，包括原路线及不透明回执身份。
fn validate_resource_delivery(
    entry: &UserPromptResourceDelivery,
    resource_id: &ResourceId,
) -> Result<(), DomainError> {
    entry.original_route.validate()?;
    if &entry.resource_id != resource_id {
        return Err(invalid_prompt_media("resource_order"));
    }
    if let UserPromptResourceDeliveryKind::Uploaded {
        upload_channel_id,
        upload_contract_fingerprint,
        ..
    }
    | UserPromptResourceDeliveryKind::UploadPending {
        upload_channel_id,
        upload_contract_fingerprint,
        ..
    } = &entry.delivery
    {
        validate_identity(upload_channel_id)?;
        // Contract identity uses the adapter's exact prefix while private receipt identity remains opaque hex.
        // 契约身份使用适配器精确前缀，私有回执身份仍为不透明十六进制。
        let digest = upload_contract_fingerprint
            .strip_prefix("sha256:")
            .ok_or_else(|| invalid_prompt_media("upload_contract_fingerprint"))?;
        validate_fingerprint(digest)?;
    }
    if let UserPromptResourceDeliveryKind::Uploaded {
        reference_fingerprint,
        ..
    } = &entry.delivery
    {
        validate_fingerprint(reference_fingerprint)?;
    }
    if let UserPromptResourceDeliveryKind::UploadPending {
        draft_intent_id: Some(intent_id),
        ..
    } = &entry.delivery
        && uuid::Uuid::parse_str(intent_id).is_err()
    {
        return Err(invalid_prompt_media("draft_intent_id"));
    }
    Ok(())
}

impl UserPromptMediaRoute {
    /// Validates all exact non-secret route fields without normalization or fallback, returning a domain error on invalid identity.
    /// 不使用规范化或回退地校验全部精确非秘密路线字段，身份无效时返回领域错误。
    pub fn validate(&self) -> Result<(), DomainError> {
        for value in [
            &self.provider_id,
            &self.service_entry_id,
            &self.connection_id,
            &self.model_id,
            &self.protocol_profile_id,
        ] {
            validate_identity(value)?;
        }
        Ok(())
    }
}

/// Validates a bounded non-empty identity `value`; it does not guess or normalize a different identity.
/// 校验有界非空身份`value`，不猜测或规范化为其他身份。
fn validate_identity(value: &str) -> Result<(), DomainError> {
    if value.is_empty()
        || value.len() > 1024
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(invalid_prompt_media("route_identity"));
    }
    Ok(())
}

/// Validates a lowercase SHA-256 `value` so private upload URLs cannot enter an opaque identity field.
/// 校验小写SHA-256值，防止私有上传URL进入不透明身份字段。
fn validate_fingerprint(value: &str) -> Result<(), DomainError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(invalid_prompt_media("upload_fingerprint"));
    }
    Ok(())
}

/// Returns the existing typed invariant error for the exact prompt-media `field` without sensitive values.
/// 为精确提示词媒体`field`返回既有强类型不变量错误，不携带敏感值。
pub(crate) fn invalid_prompt_media(field: &'static str) -> DomainError {
    DomainError::ImmutableFieldMutation {
        entity: "user_prompt_media",
        field,
    }
}
