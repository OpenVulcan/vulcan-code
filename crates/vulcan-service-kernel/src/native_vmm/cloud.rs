//! Local consent and dispatch boundary for native VMM's dedicated cloud channel.
//! 原生 VMM 专用云端通道的本机确认与分派边界。

use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex, Weak},
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use vulcan_agent_runtime::agent_runtime::provider_request_telemetry::{
    ProviderRequestCompletedUsage, ProviderRequestTelemetryEvent, ProviderRequestTelemetryEventKind,
};
use vulcan_ai_contract::{
    AiOperation, DistanceMetric, EmbeddingCapabilities, EmbeddingEncoding, EmbeddingNormalization,
    EmbeddingOutputKind, EmbeddingTask, ExecutionRouteSnapshot, HostAiCallContext,
    HostAiCapabilityHandle, HostAiCapabilityRegistration, HostAiError, HostAiErrorCode,
    INFERENCE_CONTRACT_VERSION, InferenceCapabilitySnapshot, InferenceOperationCapability,
    InferenceUsageScope,
};
use vulcan_memorymesh_core::{PiiScrubber, embedded_pii_scrubber};
use vulcan_provider_adapters::{
    MemoryCatalog, MemoryOffer, MemoryPurpose, MemoryTransportError, MemoryUsage, OpenVulcanRegion,
    request_openvulcan_memory_catalog, request_openvulcan_memory_inference,
};

use crate::{
    public_ai_settings::openvulcan_account::signed_in_credential,
    state::{ServiceBridgeState, SharedServiceBridgeState},
};

use super::resources::NativeVmmResourceStore;

/// Stable ledger and execution identifiers for the free channel.
/// 免费通道的稳定用量与执行标识。
pub(super) const CHANNEL: &str = "openvulcan-memory";
/// User consent file format. Absence means the user has not chosen cloud memory.
/// 用户同意文件格式。文件缺失表示用户没有选择云端记忆。
const CONSENT_VERSION: u32 = 1;
/// Maximum embedding inputs accepted by the deployed memory API.
/// 已部署的记忆 API 接受的向量输入数上限。
const MAX_EMBEDDING_INPUTS: u32 = 10;
/// Maximum candidates accepted by the deployed rerank API.
/// 已部署的重排 API 接受的候选数上限。
pub(super) const MAX_RERANK_DOCUMENTS: u32 = 100;

/// Public fields submitted only after the user reads the provider disclosures.
/// 用户阅读供应商披露后才提交的公开字段。
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CloudMemoryChoice {
    /// Published model and exact acknowledged disclosure revision.
    /// 已发布模型及准确确认的披露版本。
    model_id: String,
    consent_revision: String,
    /// Opaque identity of the account that received the disclosure.
    /// 收到披露的账号的不透明标识。
    account_context: String,
    /// Explicit local PII rule region and rerank choice.
    /// 明确选择的本机 PII 规则地区与重排开关。
    pii_region: String,
    rerank: bool,
}

/// Durable consent keeps the exact offer visible even after withdrawal.
/// 持久化同意保留准确发布信息，下架后仍可展示。
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CloudConsent {
    /// Accepted provider and model snapshot.
    /// 已确认的供应商与模型快照。
    offer: MemoryOffer,
    /// Selection bound to the account that explicitly approved it.
    /// 绑定到明确批准它的账号的选择。
    account: String,
    region: OpenVulcanRegion,
    /// Local privacy and optional reranking settings.
    /// 本机隐私与可选重排设置。
    pii_region: String,
    rerank: bool,
}

/// Versioned file, independent of the saved self-hosted model routes.
/// 独立于已保存自有模型路由的版本化文件。
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ConsentFile {
    /// Exact version and optional accepted offer.
    /// 精确版本及可选的已确认模型。
    version: u32,
    consent: Option<CloudConsent>,
}

/// Pairs the saved disclosure with its compiled local PII rules.
/// 将已保存的披露与已编译的本机 PII 规则配对。
struct ActiveConsent {
    /// The exact persisted choice and its local filter.
    /// 准确的持久化选择及其本机过滤器。
    choice: CloudConsent,
    scrubber: PiiScrubber,
}

impl ActiveConsent {
    /// Compiles the selected filter before publishing a usable cloud route.
    /// 在发布可用的云端路由之前编译所选过滤器。
    fn compile(choice: CloudConsent) -> Result<Arc<Self>, String> {
        let (scrubber, resolved) = embedded_pii_scrubber(&choice.pii_region)
            .map_err(|_| String::from("Could not load the selected local PII filter."))?;
        if resolved != choice.pii_region {
            return Err(String::from(
                "The selected local PII filter region is unsupported.",
            ));
        }
        Ok(Arc::new(Self { choice, scrubber }))
    }
}

/// Setup snapshot carries catalog metadata only, never a credential or memory content.
/// 引导快照只携带目录元数据，不含凭据或记忆内容。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CloudMemorySetup {
    /// User-facing status for the current account and selection.
    /// 当前账号与选择的用户可见状态。
    status: &'static str,
    /// Last accepted display name and input preferences.
    /// 上次确认的显示名称和输入偏好。
    selected_name: Option<String>,
    selected: Option<CloudMemoryChoice>,
    /// Published offers and the authoritative list of local filter regions.
    /// 已发布模型及本机过滤地区的权威列表。
    catalog: Option<MemoryCatalog>,
    /// Account binding to echo when submitting the displayed disclosures.
    /// 提交已展示披露时回传的账号绑定。
    account_context: Option<String>,
    pii_regions: Vec<String>,
}

/// Owns the durable consent and the non-owning service reference used for credential leases.
/// 持有持久化同意和用于凭据租约的非拥有服务引用。
pub(crate) struct CloudMemoryRuntime {
    /// Resolves the live account and ledger independently of the stored disclosure.
    /// 独立于已保存披露解析实时账号和用量账本。
    state: Weak<Mutex<ServiceBridgeState>>,
    /// Owns safe atomic reads and writes below the local VMM configuration directory.
    /// 负责本机 VMM 配置目录下的安全读写。
    resources: NativeVmmResourceStore,
    /// Broadcasts changes to the current workbench when the cloud selection is withdrawn.
    /// 云端选择被撤回时向当前工作台广播变化。
    events: vulcan_service_api::ServiceEventEmitter,
    /// Serializes consent updates with status changes from in-flight calls.
    /// 串行化用户同意更新与执行中调用触发的状态变更。
    change_gate: tokio::sync::Mutex<()>,
    /// Caches the last durable record and its verified local rules.
    /// 缓存最近的持久化记录及其已校验本机规则。
    active: Mutex<Result<Option<Arc<ActiveConsent>>, String>>,
    /// Remembers a server-rejected selection without discarding the user's consent record.
    /// 记住服务端拒绝的选择，并保留用户的同意记录。
    rejected: Mutex<Option<String>>,
}

impl CloudMemoryRuntime {
    /// Builds a runtime backed by the host's current account and local configuration root.
    /// 构建由宿主当前账号与本机配置根目录支持的运行时。
    pub(super) fn new(
        state: &SharedServiceBridgeState,
        resources: NativeVmmResourceStore,
        events: &vulcan_service_api::ServiceEventEmitter,
    ) -> Self {
        let active = Self::load_consent(&resources);
        Self {
            state: Arc::downgrade(state),
            resources,
            events: events.clone(),
            change_gate: tokio::sync::Mutex::new(()),
            active: Mutex::new(active),
            rejected: Mutex::new(None),
        }
    }

    /// Loads the durable choice, rejecting unsupported versions or inconsistent offers.
    /// 读取持久化选择，并拒绝不支持的版本或不一致的模型。
    fn load_consent(
        resources: &NativeVmmResourceStore,
    ) -> Result<Option<Arc<ActiveConsent>>, String> {
        let Some(bytes) = resources.read_cloud_consent()? else {
            return Ok(None);
        };
        let file: ConsentFile = serde_json::from_slice(&bytes)
            .map_err(|_| String::from("Invalid cloud memory consent file."))?;
        if file.version != CONSENT_VERSION
            || file.consent.as_ref().is_some_and(|choice| {
                !choice.offer.is_valid()
                    || choice.account.is_empty()
                    || choice.pii_region.is_empty()
                    || (choice.rerank && choice.offer.provider_for(MemoryPurpose::Rerank).is_none())
            })
        {
            return Err(String::from("Unsupported cloud memory consent record."));
        }
        file.consent.map(ActiveConsent::compile).transpose()
    }

    /// Takes a stable reference to the current compiled consent.
    /// 获取当前已编译同意的稳定引用。
    fn consent(&self) -> Result<Option<Arc<ActiveConsent>>, String> {
        self.active.lock().map_err(|_| service_closed())?.clone()
    }

    /// Resolves metadata for setup without sending memory content.
    /// 解析引导元数据，不发送记忆内容。
    pub(crate) async fn setup(&self) -> Result<CloudMemorySetup, String> {
        let active = self.consent()?;
        let saved = active.as_ref().map(|active| &active.choice);
        let regions = NativeVmmResourceStore::cloud_pii_regions()?;
        let state = self.state.upgrade().ok_or_else(service_closed)?;
        let (status, catalog, account_context) = match signed_in_credential(&state).await {
            Ok(None) => ("signed_out", None, None),
            Err(_) => ("unavailable", None, None),
            Ok(Some((account, secret))) => match request_openvulcan_memory_catalog(&secret).await {
                Ok(catalog) => {
                    let mut stale = false;
                    if let Some(saved) = saved {
                        let same_account =
                            saved.matches_account(&account, secret.openvulcan_region());
                        let still_offered = catalog.models.iter().any(|offer| {
                            offer.id == saved.offer.id
                                && offer.consent_revision == saved.offer.consent_revision
                        });
                        if same_account && !still_offered {
                            if let Some(active) = &active {
                                self.reject(active, &state).await;
                            }
                        }
                        stale = !same_account || !still_offered || self.is_rejected(saved);
                    }
                    let status = if stale {
                        "selection_changed"
                    } else {
                        "available"
                    };
                    (
                        status,
                        Some(catalog),
                        secret
                            .openvulcan_region()
                            .map(|region| account_context_id(&account, region)),
                    )
                }
                Err(MemoryTransportError::SignIn) => ("signed_out", None, None),
                Err(_) if saved.is_some_and(|saved| self.is_rejected(saved)) => {
                    ("selection_changed", None, None)
                }
                Err(_) => ("unavailable", None, None),
            },
        };
        Ok(CloudMemorySetup {
            status,
            selected_name: saved.map(|saved| saved.offer.name.clone()),
            selected: saved.map(|saved| CloudMemoryChoice {
                model_id: saved.offer.id.clone(),
                consent_revision: saved.offer.consent_revision.clone(),
                account_context: account_context_id(&saved.account, saved.region),
                pii_region: saved.pii_region.clone(),
                rerank: saved.rerank,
            }),
            catalog,
            account_context,
            pii_regions: regions,
        })
    }

    /// Persists only a freshly verified offer from the account's catalog, or an explicit switch to self-hosted AI.
    /// 只保存从账号目录重新校验的模型，或用户明确切换到自有模型的决定。
    pub(crate) async fn save(&self, choice: Option<CloudMemoryChoice>) -> Result<(), String> {
        let _gate = self.change_gate.lock().await;
        if choice.is_none() && self.consent()?.is_none() {
            return Ok(());
        }
        let consent = if let Some(choice) = choice {
            let state = self.state.upgrade().ok_or_else(service_closed)?;
            let (account, secret) = signed_in_credential(&state).await?.ok_or_else(|| {
                String::from("Sign in to OpenVulcan before enabling cloud memory.")
            })?;
            let region = secret
                .openvulcan_region()
                .ok_or_else(|| String::from("The OpenVulcan account has no region."))?;
            if account_context_id(&account, region) != choice.account_context {
                return Err(transport_error(MemoryTransportError::SelectionChanged).message);
            }
            let catalog = request_openvulcan_memory_catalog(&secret)
                .await
                .map_err(|error| transport_error(error).message)?;
            let offer = catalog
                .models
                .into_iter()
                .find(|offer| {
                    offer.id == choice.model_id && offer.consent_revision == choice.consent_revision
                })
                .ok_or_else(|| transport_error(MemoryTransportError::SelectionChanged).message)?;
            if !NativeVmmResourceStore::cloud_pii_regions()?.contains(&choice.pii_region) {
                return Err(String::from("Select a supported local PII filter region."));
            }
            if choice.rerank && offer.provider_for(MemoryPurpose::Rerank).is_none() {
                return Err(String::from(
                    "The selected cloud memory model does not provide reranking.",
                ));
            }
            Some(CloudConsent {
                offer,
                account,
                region,
                pii_region: choice.pii_region,
                rerank: choice.rerank,
            })
        } else {
            None
        };
        let active = consent.clone().map(ActiveConsent::compile).transpose()?;
        let bytes = serde_json::to_vec(&ConsentFile {
            version: CONSENT_VERSION,
            consent,
        })
        .map_err(|_| String::from("Could not encode cloud memory consent."))?;
        self.resources.write_cloud_consent(&bytes)?;
        *self.active.lock().map_err(|_| service_closed())? = Ok(active);
        *self.rejected.lock().map_err(|_| service_closed())? = None;
        Ok(())
    }

    /// Returns a snapshot for the saved route; withdrawn selections cannot fall back to personal models.
    /// 返回已保存路由的快照；已下架的选择不能回退到自有模型。
    pub(super) fn capability(
        &self,
        registration: HostAiCapabilityRegistration,
    ) -> Result<Option<HostAiCapabilityHandle>, HostAiError> {
        let Some(active) = self.consent().map_err(configuration_error)? else {
            return Ok(None);
        };
        if self.is_rejected(&active.choice) {
            return Err(transport_error(MemoryTransportError::SelectionChanged));
        }
        active.choice.capability(registration).map(Some)
    }

    /// Executes a single frozen phase after filtering all business text locally.
    /// 在本机过滤全部业务文本后执行一个冻结阶段。
    pub(super) async fn infer(
        &self,
        route: &ExecutionRouteSnapshot,
        context: &HostAiCallContext,
        purpose: MemoryPurpose,
        mut input: Value,
    ) -> Result<Value, HostAiError> {
        let state = self
            .state
            .upgrade()
            .ok_or_else(|| configuration_error(service_closed()))?;
        let configuration = crate::service_state_access::provider_configuration_from_state(&state)
            .map_err(configuration_error)?;
        let mode =
            vulcan_agent_runtime::tooling::current_memory_mode().map_err(configuration_error)?;
        let active_layer =
            vulcan_agent_runtime::tooling::project_vmm_layer_for_mode(&configuration.vmm, mode);
        let effective = vulcan_vmm::VmmEffectiveConfig::resolve(&active_layer, None, None);
        if !effective.enabled || effective.backend_mode != vulcan_vmm::VmmBackendMode::Native {
            return Err(configuration_error("Native cloud memory is inactive."));
        }
        let active = self
            .consent()
            .map_err(configuration_error)?
            .filter(|active| active.choice.key() == route.capability_revision)
            .ok_or_else(|| transport_error(MemoryTransportError::SelectionChanged))?;
        let saved = &active.choice;
        if self.is_rejected(saved) || (purpose == MemoryPurpose::Rerank && !saved.rerank) {
            return Err(transport_error(MemoryTransportError::SelectionChanged));
        }
        let (account, secret) = signed_in_credential(&state)
            .await
            .map_err(|_| transport_error(MemoryTransportError::SignIn))?
            .ok_or_else(|| transport_error(MemoryTransportError::SignIn))?;
        if !saved.matches_account(&account, secret.openvulcan_region()) {
            return Err(transport_error(MemoryTransportError::SignIn));
        }
        scrub_business_value(&active.scrubber, &mut input, None, true)?;
        if self
            .consent()
            .map_err(configuration_error)?
            .is_none_or(|current| !Arc::ptr_eq(&current, &active))
            || self.is_rejected(saved)
        {
            return Err(transport_error(MemoryTransportError::SelectionChanged));
        }
        let start = time::OffsetDateTime::now_utc();
        let pending = request_openvulcan_memory_inference(&secret, &saved.offer, purpose, input);
        // Use the native effect deadline when present, in addition to the transport's own limit.
        // 存在原生 Effect 截止时间时同时执行该期限与传输自身上限。
        let deadline = async {
            if let Some(deadline) = context.deadline_unix_ms {
                let now =
                    u64::try_from(start.unix_timestamp_nanos() / 1_000_000).unwrap_or(u64::MAX);
                tokio::time::sleep(std::time::Duration::from_millis(
                    deadline.saturating_sub(now),
                ))
                .await;
            } else {
                std::future::pending::<()>().await;
            }
        };
        let response = tokio::select! {
            biased;
            () = context.cancellation.cancelled() => {
                return Err(HostAiError::new(HostAiErrorCode::Cancelled, "Cloud memory call cancelled."));
            }
            () = deadline => {
                return Err(HostAiError::new(HostAiErrorCode::Timeout, "Cloud memory effect deadline elapsed."));
            }
            result = pending => result,
        };
        match response {
            Ok(response) => {
                record_usage(&state, route, purpose, context, start, response.usage).await;
                Ok(response.result)
            }
            Err(error) => {
                if error == MemoryTransportError::SelectionChanged {
                    self.reject(&active, &state).await;
                }
                Err(transport_error(error))
            }
        }
    }

    /// Reports whether the exact saved selection was rejected during this process.
    /// 报告当前进程中准确保存的选择是否遭到拒绝。
    fn is_rejected(&self, consent: &CloudConsent) -> bool {
        self.rejected
            .lock()
            .map_or(true, |rejected| rejected.as_ref() == Some(&consent.key()))
    }

    /// Returns the blocked state only while the rejected consent still owns the current route.
    /// 仅在被拒绝的同意仍控制当前路由时返回阻断状态。
    pub(super) fn needs_reselection(&self) -> bool {
        self.consent()
            .ok()
            .flatten()
            .is_some_and(|saved| self.is_rejected(&saved.choice))
    }

    /// Marks a confirmed model change once and broadcasts the new readiness state.
    /// 将已确认的模型变化标记一次，并广播新的就绪状态。
    async fn reject(&self, active: &Arc<ActiveConsent>, state: &SharedServiceBridgeState) {
        {
            let _gate = self.change_gate.lock().await;
            if self
                .consent()
                .ok()
                .flatten()
                .is_none_or(|current| !Arc::ptr_eq(&current, active))
            {
                return;
            }
            let key = active.choice.key();
            let Ok(mut rejected) = self.rejected.lock() else {
                return;
            };
            if rejected.as_ref() == Some(&key) {
                return;
            }
            *rejected = Some(key);
        }
        if let Ok(snapshot) =
            crate::workbench_refresh::refresh_workbench_snapshot(state.clone()).await
        {
            crate::settings_runtime_projection::emit_workbench_event(&self.events, &snapshot);
        }
    }
}

impl CloudConsent {
    /// Checks the current credential against the consented account and regional deployment.
    /// 将当前凭据与用户确认的账号和区域部署进行匹配。
    fn matches_account(&self, account: &str, region: Option<OpenVulcanRegion>) -> bool {
        account == self.account && region == Some(self.region)
    }

    /// Returns the identity of this exact account, offer, and consent revision.
    /// 返回此账号、模型与同意版本的精确标识。
    fn key(&self) -> String {
        hex::encode(Sha256::digest(format!(
            "{}\n{}\n{}\n{}\n{}",
            account_context_id(&self.account, self.region),
            self.offer.id,
            self.offer.consent_revision,
            self.pii_region,
            self.rerank
        )))
    }

    /// Projects cloud metadata into the native readiness and vector-identity contract.
    /// 将云端元数据投影到原生就绪状态与向量身份协议。
    fn capability(
        &self,
        registration: HostAiCapabilityRegistration,
    ) -> Result<HostAiCapabilityHandle, HostAiError> {
        let operations = registration
            .requirements
            .into_iter()
            .map(|requirement| {
                let purpose = match (requirement.operation, requirement.purpose_id.as_deref()) {
                    (AiOperation::ConversationRespond, Some("precheck_l1")) => MemoryPurpose::Pcl1,
                    (AiOperation::ConversationRespond, Some("precheck_l2")) => MemoryPurpose::Pcl2,
                    (AiOperation::ConversationRespond, Some("postaction_l1")) => {
                        MemoryPurpose::Ptl1
                    }
                    (AiOperation::ConversationRespond, Some("postaction_l2")) => {
                        MemoryPurpose::Ptl2
                    }
                    (AiOperation::ConversationRespond, Some("profile_instruction")) => {
                        MemoryPurpose::ProfileInstruction
                    }
                    (AiOperation::EmbeddingCreate, None) => MemoryPurpose::Embedding,
                    (AiOperation::RerankDocuments, None) => MemoryPurpose::Rerank,
                    _ => return Err(configuration_error("Unexpected native memory requirement.")),
                };
                let route = self
                    .offer
                    .provider_for(purpose)
                    .filter(|_| purpose != MemoryPurpose::Rerank || self.rerank)
                    .map(|disclosure| {
                        // Include consent, filter region and site in vector identity: the catalog publishes no independent embedding revision.
                        // 向量身份包含同意版本、过滤地区和站点，因为目录未发布独立的向量版本。
                        let vector_revision = (purpose == MemoryPurpose::Embedding).then(|| {
                            hex::encode(Sha256::digest(format!(
                                "{}\n{}\n{}\n{}",
                                self.region.api_origin(),
                                self.offer.id,
                                self.offer.consent_revision,
                                self.pii_region
                            )))
                        });
                        ExecutionRouteSnapshot {
                            provider_id: String::from("openvulcan"),
                            service_entry_id: String::from(CHANNEL),
                            connection_id: self.account.clone(),
                            model_id: disclosure.model.clone(),
                            protocol_binding_id: Some(String::from(CHANNEL)),
                            action_binding_id: Some(purpose.id().to_owned()),
                            secondary_route_id: vector_revision,
                            secondary_provider_name: Some(disclosure.provider.clone()),
                            secondary_endpoint_name: Some(disclosure.region.clone()),
                            capability_revision: self.key(),
                            catalog_revision: self.offer.consent_revision.clone(),
                            route_revision: self.offer.consent_revision.clone(),
                            usage_scope: InferenceUsageScope::InternalBackground,
                        }
                    });
                Ok(InferenceOperationCapability {
                    operation: requirement.operation,
                    purpose_id: requirement.purpose_id,
                    available: route.is_some(),
                    unavailable_reason: None,
                    route,
                    embedding: (purpose == MemoryPurpose::Embedding).then(|| {
                        EmbeddingCapabilities {
                            tasks: BTreeSet::from([EmbeddingTask::Document, EmbeddingTask::Query]),
                            input_resources: BTreeSet::new(),
                            output_kinds: BTreeSet::from([EmbeddingOutputKind::Dense]),
                            encodings: BTreeSet::from([EmbeddingEncoding::Float]),
                            dimensions: BTreeSet::from([self.offer.embedding_dimension]),
                            minimum_dimensions: None,
                            maximum_dimensions: None,
                            default_dimensions: Some(self.offer.embedding_dimension),
                            maximum_inputs: Some(MAX_EMBEDDING_INPUTS),
                            maximum_tokens_per_input: None,
                            normalization: EmbeddingNormalization::Unknown,
                            distance_metric: DistanceMetric::Cosine,
                            input_templates: Default::default(),
                        }
                    }),
                    embedding_batch: None,
                    rerank: None,
                })
            })
            .collect::<Result<Vec<_>, HostAiError>>()?;
        let snapshot = InferenceCapabilitySnapshot {
            contract_version: INFERENCE_CONTRACT_VERSION,
            consumer_profile_id: registration.consumer_profile_id.clone(),
            operations,
        };
        HostAiCapabilityHandle::new(
            CHANNEL,
            registration.owner_id,
            registration.consumer_profile_id,
            registration.caller,
            snapshot,
        )
    }
}

/// Filters nested business text, keeping known Core references intact for response reconciliation.
/// 过滤嵌套业务文本，并保留已知 Core 引用供响应对账。
fn scrub_business_value(
    scrubber: &PiiScrubber,
    value: &mut Value,
    field: Option<&str>,
    preserve_references: bool,
) -> Result<(), HostAiError> {
    match value {
        Value::String(text) if !preserve_references || !is_native_reference(field, text) => {
            *text = scrubber
                .scrub(text)
                .map_err(|_| configuration_error("Local PII filtering failed."))?
                .text;
        }
        Value::Array(items) => {
            for item in items {
                scrub_business_value(scrubber, item, None, preserve_references)?;
            }
        }
        Value::Object(fields) => {
            for (key, child) in fields {
                scrub_business_value(
                    scrubber,
                    child,
                    Some(key),
                    preserve_references && key != "raw_turn",
                )?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Recognizes only native database-generated references, never arbitrary text in ID fields.
/// 只识别原生数据库生成的引用，不豁免 ID 字段中的任意文本。
fn is_native_reference(field: Option<&str>, text: &str) -> bool {
    // Native IDs use numeric database identities; profile binding admits either known scope.
    // 原生 ID 使用数据库数字标识，画像绑定可指向两种已知作用域。
    let suffix = match field {
        Some("turn_id" | "source_turn_id" | "source_id") => text.strip_prefix("vmm-turn-"),
        Some("memory_id") => text.strip_prefix("vmm-memory-"),
        Some("id") => text.strip_prefix("vmm-profile-"),
        Some("bind_id") => text
            .strip_prefix("vmm-subject:")
            .or_else(|| text.strip_prefix("vmm-project:")),
        _ => None,
    };
    suffix.is_some_and(|suffix| {
        !suffix.is_empty()
            && suffix.bytes().all(|byte| byte.is_ascii_digit())
            && suffix.parse::<u64>().is_ok_and(|id| id > 0)
    })
}

/// Records one returned attempt in the normal ledger with the phase and actual upstream usage.
/// 按阶段和上游实际用量把一次返回的调用写入常规账本。
async fn record_usage(
    state: &SharedServiceBridgeState,
    route: &ExecutionRouteSnapshot,
    purpose: MemoryPurpose,
    context: &HostAiCallContext,
    start: time::OffsetDateTime,
    usage: Option<MemoryUsage>,
) {
    let controller = state
        .lock()
        .ok()
        .and_then(|guard| guard.controller_runtime.clone());
    let Some(controller) = controller else { return };
    let end = time::OffsetDateTime::now_utc();
    let id = uuid::Uuid::new_v4();
    let telemetry = ProviderRequestTelemetryEvent::detached(
        ProviderRequestTelemetryEventKind::Completed,
        id,
        json!({
            "execution_id": context.operation_id,
            "request_id": id,
            "attempt_index": context.attempt.saturating_sub(1),
            "owner_id": super::driver::NATIVE_VMM_CALLER_ID,
            "operation": format!("cloud_memory_{}", purpose.id()),
            "provider_key": route.provider_id,
            "service_entry_id": CHANNEL,
            "connection_id": route.connection_id,
            "protocol_profile_id": CHANNEL,
            "route_revision": route.route_revision,
            "transport": "http",
            "model_key": route.model_id,
            "started_at_ms": start.unix_timestamp() * 1000 + i64::from(start.millisecond()),
            "completed_at_ms": end.unix_timestamp() * 1000 + i64::from(end.millisecond()),
            "calendar_date": start.date().to_string(),
            "status": "succeeded",
            "dispatch_outcome": "dispatched",
            "semantic_output_observed": true,
            "http_status": 200,
        }),
        usage.map(completed_usage),
    );
    if let Err(error) = crate::provider_usage_runtime::persist_native_provider_usage_attempt(
        &controller,
        &telemetry,
    )
    .await
    {
        tracing::warn!("Failed to persist cloud memory usage: {error}");
    }
}

/// Converts reported counters without imputing missing usage.
/// 转换报告的计数，不为缺失用量补值。
fn completed_usage(usage: MemoryUsage) -> ProviderRequestCompletedUsage {
    let count = |value: Option<u64>| value.and_then(|count| usize::try_from(count).ok());
    let uncached = usage.input_tokens.and_then(|input| {
        input
            .checked_sub(usage.cached_input_tokens?)?
            .checked_sub(usage.cache_creation_input_tokens?)
    });
    ProviderRequestCompletedUsage {
        update_kind: Some(vulcan_ai_contract::UsageUpdateKind::Final),
        input_tokens: count(usage.input_tokens),
        uncached_input_tokens: count(uncached),
        output_tokens: count(usage.output_tokens),
        total_tokens: count(usage.total_tokens),
        cached_input_tokens: count(usage.cached_input_tokens),
        cache_creation_input_tokens: count(usage.cache_creation_input_tokens),
        ..Default::default()
    }
}

/// Returns a controlled error when the service is shutting down.
/// 服务正在关闭时返回受控错误。
fn service_closed() -> String {
    String::from("The cloud memory service is not ready.")
}

/// Binds the catalog shown by the client to the account and isolated site that supplied it.
/// 将客户端所展示的目录绑定到提供目录的账号及隔离站点。
fn account_context_id(account: &str, region: OpenVulcanRegion) -> String {
    hex::encode(Sha256::digest(format!(
        "{}\n{}",
        region.api_origin(),
        account
    )))
}

/// Converts local consent or privacy configuration failures into Host AI errors.
/// 将本机同意或隐私配置失败转换为宿主 AI 错误。
fn configuration_error(error: impl Into<String>) -> HostAiError {
    HostAiError::new(HostAiErrorCode::Configuration, error)
}

/// Maps only stable transport outcomes into diagnostics, excluding all request and upstream text.
/// 只将稳定传输结果映射为诊断，排除请求和上游文本。
fn transport_error(error: MemoryTransportError) -> HostAiError {
    use MemoryTransportError as Error;
    let (code, message) = match error {
        Error::SignIn => (
            HostAiErrorCode::Authentication,
            "Sign in with the OpenVulcan account that enabled cloud memory.",
        ),
        Error::SelectionChanged => (
            HostAiErrorCode::CapabilityUnavailable,
            "The cloud memory model or its disclosures changed. Choose and confirm a model in Models and Services.",
        ),
        Error::RateLimited => (
            HostAiErrorCode::RateLimited,
            "The cloud memory rate limit was reached.",
        ),
        Error::InputLimit => (
            HostAiErrorCode::LimitExceeded,
            "The cloud memory input exceeds the site limit.",
        ),
        Error::Unavailable => (
            HostAiErrorCode::ProviderUnavailable,
            "The cloud memory service is temporarily unavailable.",
        ),
    };
    HostAiError::new(code, message)
}

/// Resolves the installed cloud runtime without holding the service state lock across a request.
/// 解析已安装的云端运行时，避免请求期间持有服务状态锁。
pub(crate) fn cloud_runtime(
    state: &SharedServiceBridgeState,
) -> Result<std::sync::Arc<CloudMemoryRuntime>, String> {
    native_maintenance(state)?
        .cloud_memory()
        .ok_or_else(service_closed)
}

/// Resolves the native maintenance owner without carrying the shared state lock into callers.
/// 解析原生维护归属方，不将共享状态锁带入调用方。
fn native_maintenance(
    state: &SharedServiceBridgeState,
) -> Result<super::NativeVmmMaintenance, String> {
    state
        .lock()
        .map_err(|_| service_closed())?
        .native_vmm_maintenance
        .clone()
        .ok_or_else(service_closed)
}

/// Applies an explicit choice and publishes refreshed runtime readiness.
/// 应用明确选择，并发布刷新后的运行时就绪状态。
pub(crate) async fn save_cloud_choice(
    state: &SharedServiceBridgeState,
    choice: Option<CloudMemoryChoice>,
) -> Result<(), String> {
    let maintenance = native_maintenance(state)?;
    let cloud = maintenance.cloud_memory().ok_or_else(service_closed)?;
    if choice.is_none() && cloud.consent()?.is_some() {
        maintenance.validate_personal_ai()?;
    }
    cloud.save(choice).await?;
    if let Err(error) = maintenance.validate_host_ai().await {
        tracing::warn!("Native memory readiness after changing cloud selection: {error}");
    }
    if let Ok(snapshot) = crate::workbench_refresh::refresh_workbench_snapshot(state.clone()).await
    {
        crate::settings_runtime_projection::emit_workbench_event(&cloud.events, &snapshot);
    }
    Ok(())
}

/// A ranked candidate from the deployed API, using its original zero-based index.
/// 已部署 API 返回的排序候选，使用原始零基下标。
#[derive(Deserialize)]
pub(super) struct CloudRank {
    /// Original candidate index and provider-reported relevance score.
    /// 原始候选下标及供应商报告的相关度分数。
    pub(super) index: u32,
    pub(super) relevance_score: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Supplies a two-route synthetic offer and local privacy selection without credentials.
    /// 提供含两条路由的合成模型和本机隐私选择，不使用凭据。
    fn synthetic_consent() -> CloudConsent {
        let offer = serde_json::from_value(json!({
            "id": "fixture",
            "name": "Synthetic memory",
            "consent_revision": "a".repeat(64),
            "embedding_dimension": 1024,
            "providers": [
                {"capability": "inference", "model": "fixture-inference", "provider": "Test Inference",
                 "region": "Test", "data_policy": "Test disclosure", "privacy_url": "https://example.com/privacy"},
                {"capability": "embedding", "model": "fixture-embedding", "provider": "Test Embedding",
                 "region": "Test", "data_policy": "Test disclosure", "privacy_url": "https://example.com/privacy"}
            ]
        })).expect("synthetic offer");
        CloudConsent {
            offer,
            account: String::from("synthetic-account"),
            region: OpenVulcanRegion::Global,
            pii_region: String::from("en-US"),
            rerank: false,
        }
    }

    /// Ensures only the persisted consent and exact regional rules can survive restart.
    /// 确保重启后只恢复已持久化的同意和准确地区规则。
    #[test]
    fn cloud_memory_consent_restores_exact_scope_and_rejects_unknown_region() {
        let root = tempfile::tempdir().expect("temporary root");
        let store = NativeVmmResourceStore::new(root.path());
        assert!(
            CloudMemoryRuntime::load_consent(&store)
                .expect("missing choice")
                .is_none()
        );
        let mut choice = synthetic_consent();
        let key = choice.key();
        assert!(choice.matches_account("synthetic-account", Some(OpenVulcanRegion::Global)));
        assert!(!choice.matches_account("synthetic-account", Some(OpenVulcanRegion::China)));
        let mut file = ConsentFile {
            version: CONSENT_VERSION,
            consent: Some(choice.clone()),
        };
        store
            .write_cloud_consent(&serde_json::to_vec(&file).expect("serialize"))
            .expect("persist consent");
        let restored = CloudMemoryRuntime::load_consent(&store)
            .expect("load consent")
            .expect("saved choice");
        assert_eq!(restored.choice.key(), key);
        choice.pii_region = String::from("zz-ZZ");
        file.consent = Some(choice);
        store
            .write_cloud_consent(&serde_json::to_vec(&file).expect("serialize"))
            .expect("persist invalid consent fixture");
        assert!(CloudMemoryRuntime::load_consent(&store).is_err());
    }

    /// Redacts user and nested dialogue text while preserving identifiers from the typed renderer.
    /// 脱敏用户与嵌套对话文本，同时保留强类型渲染器的标识。
    #[test]
    fn cloud_memory_scrubs_nested_text_and_retains_core_references() {
        let consent = ActiveConsent::compile(synthetic_consent()).expect("filter");
        let mut input = json!({
            "current_user_input": "Email synthetic-memory@example.com",
            "target_turn": {
                "turn_id": "vmm-turn-7",
                "raw_turn": { "id": "synthetic-memory@example.com" }
            },
            "candidates": [
                {"memory_id": "vmm-memory-77", "details": "synthetic-memory@example.com"},
                {"memory_id": "synthetic-memory@example.com"}
            ]
        });
        scrub_business_value(&consent.scrubber, &mut input, None, true).expect("scrub");
        assert!(!input.to_string().contains("synthetic-memory@example.com"));
        assert_eq!(
            input
                .pointer("/target_turn/turn_id")
                .and_then(Value::as_str),
            Some("vmm-turn-7")
        );
        assert_eq!(
            input
                .pointer("/candidates/0/memory_id")
                .and_then(Value::as_str),
            Some("vmm-memory-77")
        );
    }

    /// Retains upstream totals and cache counts independently and preserves unknown components.
    /// 独立保留上游合计与缓存计数，并保留未知分项。
    #[test]
    fn cloud_memory_usage_preserves_actual_counts() {
        let usage = completed_usage(MemoryUsage {
            input_tokens: Some(512),
            output_tokens: None,
            total_tokens: Some(549),
            cached_input_tokens: Some(400),
            cache_creation_input_tokens: Some(50),
        });
        assert_eq!(usage.input_tokens, Some(512));
        assert_eq!(usage.uncached_input_tokens, Some(62));
        assert_eq!(usage.cached_input_tokens, Some(400));
        assert_eq!(usage.cache_creation_input_tokens, Some(50));
        assert_eq!(usage.output_tokens, None);
        assert_eq!(usage.total_tokens, Some(549));
    }
}
