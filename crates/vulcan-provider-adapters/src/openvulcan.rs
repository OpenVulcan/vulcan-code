//! OpenVulcan native protocol boundaries shared by authorization and execution.
//! 授权和执行共享的 OpenVulcan 原生协议边界。

use crate::OpenVulcanRegion;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use vulcan_ai_contract::{
    AiOperation, DistanceMetric, EmbeddingCapabilities, EmbeddingEncoding, EmbeddingNormalization,
    EmbeddingOutputKind, EmbeddingTask, ModelCapabilities,
};

/// Versioned account-visible native model directory.
/// 版本化的账户可见原生模型目录。
#[derive(Deserialize)]
struct Catalog {
    /// Exact server protocol version.
    /// 准确服务端协议版本。
    contract_version: String,
    /// Public descriptors decoded after version admission, with their array positions retained for diagnostics.
    /// 版本准入后解码的公开描述符，保留数组位置用于诊断。
    models: Vec<serde_json::Value>,
}

/// Public model descriptor; only documented native kinds acquire executable capabilities.
/// 公开模型描述符；仅已记录的原生类型获得可执行能力。
#[derive(Deserialize)]
struct PublicModel {
    /// Stable public model identifier.
    /// 稳定公开模型标识。
    model_id: String,
    /// Provider-owned display label.
    /// 供应商持有的显示标签。
    display_name: String,
    /// Closed native operation and its corresponding admission contract.
    /// 封闭原生操作及对应准入契约。
    #[serde(flatten)]
    operation: PublicOperation,
}

/// Native operation-specific capability declarations.
/// 原生操作专属能力声明。
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum PublicOperation {
    /// Text embedding capability set.
    /// 文本向量能力集合。
    Embedding {
        /// Exact embedding limits.
        /// 准确向量限制。
        capabilities: PublicEmbedding,
    },
    /// Closed-question decision capability set.
    /// 封闭问题判定能力集合。
    Decision {
        /// Exact decision limits.
        /// 准确判定限制。
        decision_capabilities: PublicDecision,
    },
}

/// Embedding limits advertised by the user-center catalog.
/// 用户中心目录公布的向量限制。
#[derive(Deserialize)]
struct PublicEmbedding {
    /// Per-item token and batch limits.
    /// 单条 Token 和批量限制。
    max_input_tokens_per_item: u64,
    /// Maximum input items in one billable request.
    /// 单次计费请求允许的最大输入条数。
    max_batch_size: u32,
    /// Default and admitted discrete dimensions.
    /// 默认维度及获准离散维度。
    default_dimensions: u32,
    /// Closed set of vector widths accepted by the server.
    /// 服务端接受的封闭向量宽度集合。
    allowed_dimensions: BTreeSet<u32>,
    /// Exact server task identifiers; they are not the host's semantic enum labels.
    /// 准确的服务端任务标识；它们并非宿主语义枚举标签。
    allowed_tasks: BTreeSet<String>,
    /// Server output representations, intersected with host-supported shapes.
    /// 服务端输出表示，与宿主支持的形态求交集。
    allowed_output_kinds: BTreeSet<String>,
    /// Server-declared numeric encodings.
    /// 服务端声明的数值编码。
    allowed_encodings: BTreeSet<EmbeddingEncoding>,
}

/// Decision context bound; remaining admission rules are enforced by the authority.
/// 判定上下文边界；其余准入规则由授权服务强制执行。
#[derive(Deserialize)]
struct PublicDecision {
    /// Total state and question input limit.
    /// 状态与问题的总输入上限。
    max_total_input_tokens: u64,
}

/// Parses body for connection_id/generation, preserving the catalog's exact native capabilities.
/// 为 connection_id/generation 解析 body，并保留目录的准确原生能力。
pub(crate) fn parse_catalog(
    connection_id: &str,
    generation: u64,
    body: &str,
) -> Result<crate::ConnectionModelCatalog, String> {
    // catalog must identify the protocol before any model becomes executable.
    // catalog 必须先标识协议，任何模型才能执行。
    let catalog: Catalog = serde_json::from_str(body)
        .map_err(|error| format!("Invalid OpenVulcan model catalog: {error}"))?;
    if catalog.contract_version != CONTRACT_VERSION {
        return Err(String::from(
            "Unsupported OpenVulcan model catalog version.",
        ));
    }
    // models contains only executable operations backed by published admission facts.
    // models 仅包含有已发布准入事实支持的可执行操作。
    let mut models = Vec::with_capacity(catalog.models.len());
    for (model_index, descriptor) in catalog.models.into_iter().enumerate() {
        // descriptor is decoded independently so a schema failure identifies its exact model entry.
        // descriptor 独立解码，使结构错误能够标识准确的模型条目。
        let descriptor: PublicModel = serde_json::from_value(descriptor).map_err(|error| {
            format!("Invalid OpenVulcan model catalog at models[{model_index}]: {error}")
        })?;
        // embedding_task_values preserves discovered wire names across catalog activation and selection.
        // embedding_task_values 在目录激活与选择过程中保留发现得到的线协议名称。
        let mut embedding_task_values = BTreeMap::new();
        // capabilities never acquire conversation operations from a model-name heuristic.
        // capabilities 绝不从模型名称启发式规则获得对话操作。
        let mut capabilities = ModelCapabilities {
            evidence_revision: format!("openvulcan.{CONTRACT_VERSION}"),
            ..ModelCapabilities::default()
        };
        match descriptor.operation {
            PublicOperation::Embedding {
                capabilities: published,
            } => {
                if published.max_input_tokens_per_item == 0
                    || published.max_batch_size == 0
                    || published.default_dimensions == 0
                    || !published
                        .allowed_dimensions
                        .contains(&published.default_dimensions)
                    || published.allowed_tasks.is_empty()
                    || published.allowed_encodings != BTreeSet::from([EmbeddingEncoding::Float])
                {
                    return Err(String::from(
                        "OpenVulcan published invalid embedding capabilities.",
                    ));
                }
                // output_kinds intersects the remote contract with host-supported independent vector shapes.
                // output_kinds 将远端契约与宿主支持的独立向量形态求交集。
                let output_kinds = published
                    .allowed_output_kinds
                    .iter()
                    .filter_map(|kind| match kind.as_str() {
                        "dense" => Some(EmbeddingOutputKind::Dense),
                        "sparse" => Some(EmbeddingOutputKind::Sparse),
                        _ => None,
                    })
                    .collect::<BTreeSet<_>>();
                embedding_task_values =
                    project_embedding_tasks(model_index, published.allowed_tasks)?;
                if output_kinds.is_empty() || embedding_task_values.is_empty() {
                    continue;
                }
                capabilities.operations = vec![AiOperation::EmbeddingCreate];
                capabilities.context_limit = Some(published.max_input_tokens_per_item);
                capabilities.embedding = Some(EmbeddingCapabilities {
                    tasks: embedding_task_values.keys().copied().collect(),
                    input_resources: BTreeSet::new(),
                    output_kinds,
                    encodings: published.allowed_encodings,
                    dimensions: published.allowed_dimensions,
                    minimum_dimensions: None,
                    maximum_dimensions: None,
                    default_dimensions: Some(published.default_dimensions),
                    maximum_inputs: Some(published.max_batch_size),
                    maximum_tokens_per_input: Some(published.max_input_tokens_per_item),
                    normalization: EmbeddingNormalization::Unknown,
                    distance_metric: DistanceMetric::Unknown,
                });
            }
            PublicOperation::Decision {
                decision_capabilities,
            } => {
                if decision_capabilities.max_total_input_tokens == 0 {
                    return Err(String::from(
                        "OpenVulcan published invalid decision capabilities.",
                    ));
                }
                capabilities.operations = vec![AiOperation::DecisionEvaluate];
                capabilities.context_limit = Some(decision_capabilities.max_total_input_tokens);
            }
        }
        models.push(crate::ConnectionModel {
            id: descriptor.model_id,
            display_name: descriptor.display_name,
            capabilities,
            capability_evidence: crate::ModelCapabilityEvidence {
                embedding_task_values,
                context_limit_declared: true,
                ..Default::default()
            },
            protocol_variants: BTreeMap::new(),
            resolved_protocol_binding: None,
        });
    }
    Ok(crate::ConnectionModelCatalog {
        connection_id: connection_id.to_string(),
        models,
        generation,
        page_count: 1,
        discovery_schema_revision: format!("openvulcan.models.{CONTRACT_VERSION}"),
    })
}

/// Projects `declared_tasks` into host semantics while retaining each exact wire value.
/// 将 `declared_tasks` 投影到宿主语义，同时保留每个准确线协议值。
/// `model_index` identifies the source entry in errors; returns bindings or rejects ambiguous semantics.
/// `model_index` 在错误中标识源条目；返回绑定，或拒绝含糊的语义声明。
fn project_embedding_tasks(
    model_index: usize,
    declared_tasks: BTreeSet<String>,
) -> Result<BTreeMap<EmbeddingTask, String>, String> {
    // bindings admits the task families published by the user-center presets and seed catalogs.
    // bindings 接纳用户中心预设与种子目录发布的任务命名族。
    let mut bindings = BTreeMap::new();
    for wire_task in declared_tasks {
        // task is mapped only from an explicit task label, never from a model name or deployment region.
        // task 仅从明确的任务标签映射，绝不从模型名或部署区域推断。
        let task = match wire_task.as_str() {
            "query" | "retrieval_query" | "search_query" => EmbeddingTask::Query,
            "document" | "retrieval_document" | "search_document" => EmbeddingTask::Document,
            "provider_default" => EmbeddingTask::ProviderDefault,
            "semantic_similarity" => EmbeddingTask::SemanticSimilarity,
            "classification" => EmbeddingTask::Classification,
            "clustering" => EmbeddingTask::Clustering,
            "code_retrieval" => EmbeddingTask::CodeRetrieval,
            // The server permits arbitrary task identifiers; unrecognized semantics are not executable.
            // 服务端允许任意任务标识；无法识别的语义不能执行。
            _ => continue,
        };
        if bindings.insert(task, wire_task).is_some() {
            return Err(format!(
                "Invalid OpenVulcan model catalog at models[{model_index}].capabilities.allowed_tasks: multiple wire values declare {task:?}."
            ));
        }
    }
    Ok(bindings)
}

/// Validates that secret may reach url and returns its exact native authorization header.
/// 校验 secret 可发送至 url，并返回其准确原生授权头。
pub(crate) fn authorization_header(
    secret: &crate::CredentialExecutionSecret,
    url: &str,
) -> Result<String, String> {
    // region is fixed by the secure credential, not the caller's connection fields.
    // region 由安全凭据固定，不来自调用方连接字段。
    let region = secret
        .openvulcan_region()
        .ok_or_else(|| String::from("OpenVulcan requires a native account credential."))?;
    let parsed =
        reqwest::Url::parse(url).map_err(|_| String::from("Invalid OpenVulcan request URL."))?;
    if ![region.auth_origin(), region.api_origin()]
        .contains(&parsed.origin().ascii_serialization().as_str())
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.fragment().is_some()
    {
        return Err(String::from(
            "OpenVulcan credentials cannot leave their authorized deployment.",
        ));
    }
    Ok(format!("Auth {}", secret.expose_value()))
}

/// Refresh one day before the server-owned expiry to survive ordinary idle intervals.
/// 在服务端持有的到期时间前一天刷新，以覆盖普通空闲间隔。
pub(crate) const REFRESH_AHEAD_MS: u64 = 24 * 60 * 60 * 1_000;
/// Version of the user-center model catalog and embedding request contract.
/// 用户中心模型目录与向量请求契约版本。
pub(crate) const CONTRACT_VERSION: &str = "2026-08-30";
/// Version of the user-center decision request and response contract, versioned apart from embeddings
/// (`DecisionContractVersion` in openvulcan-web `vectorapi/decisions.go`).
/// 用户中心判定请求与响应契约版本，与向量契约分别版本化（openvulcan-web `vectorapi/decisions.go` 的
/// `DecisionContractVersion`）。
pub(crate) const DECISION_CONTRACT_VERSION: &str = "2026-10-11";

impl OpenVulcanRegion {
    /// Returns the reviewed authorization origin for this fixed region without discovery.
    /// 返回此固定区域已审核的授权来源，不进行探测。
    pub fn auth_origin(self) -> &'static str {
        match self {
            Self::China => "https://auth.openvulcan.cn",
            Self::Global => "https://auth.openvulcan.com",
        }
    }

    /// Returns the inference origin belonging to the same isolated deployment.
    /// 返回属于同一隔离部署的推理来源。
    pub fn api_origin(self) -> &'static str {
        match self {
            Self::China => "https://api.openvulcan.cn",
            Self::Global => "https://api.openvulcan.com",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a native descriptor from `tasks` and returns an envelope for projection and schema tests.
    /// 从 `tasks` 构建原生描述符，返回用于投影和结构测试的封装。
    fn task_catalog(tasks: &[&str]) -> serde_json::Value {
        serde_json::json!({"contract_version": CONTRACT_VERSION, "models": [
            {"model_id":"task-vector", "display_name":"Task Vector", "kind":"embedding", "capabilities":{
                "max_input_tokens_per_item":128, "max_batch_size":2,
                "default_dimensions":4, "allowed_dimensions":[4],
                "allowed_tasks":tasks, "allowed_output_kinds":["dense"], "allowed_encodings":["float"]
            }},
            {"model_id":"task-decision", "display_name":"Task Decision", "kind":"decision",
             "decision_capabilities":{"max_total_input_tokens":256}}
        ]})
    }

    /// With no inputs, verifies unsupported task semantics cannot grant capabilities or hide other models.
    /// 无需参数，验证不受支持的任务语义不能授予能力或隐藏其他模型，成功时正常返回。
    #[test]
    fn openvulcan_catalog_filters_unsupported_tasks_without_guessing() {
        // mixed includes one supported declared task alongside an arbitrary server-owned task identifier.
        // mixed 在一个受支持声明任务之外包含任意服务端任务标识。
        let mixed = parse_catalog(
            "fixture",
            1,
            &task_catalog(&["retrieval_query", "opaque_task"]).to_string(),
        )
        .expect("supported task projection");
        // vector is selected by identity so catalog additions cannot change this assertion's owner.
        // vector 按身份选择，避免目录增加条目改变断言归属。
        let vector = mixed
            .models
            .iter()
            .find(|model| model.id == "task-vector")
            .expect("vector model");
        assert_eq!(
            vector
                .capabilities
                .embedding
                .as_ref()
                .expect("embedding")
                .tasks,
            BTreeSet::from([EmbeddingTask::Query])
        );
        assert_eq!(
            vector.capability_evidence.embedding_task_values,
            BTreeMap::from([(EmbeddingTask::Query, String::from("retrieval_query"))])
        );
        // unsupported has no executable embedding semantics, while the independent decision model remains valid.
        // unsupported 没有可执行的向量语义，但独立判定模型仍有效。
        let unsupported = parse_catalog("fixture", 1, &task_catalog(&["opaque_task"]).to_string())
            .expect("valid catalog with unsupported task");
        assert!(
            !unsupported
                .models
                .iter()
                .any(|model| model.id == "task-vector")
        );
        assert!(
            unsupported
                .models
                .iter()
                .any(|model| model.id == "task-decision")
        );
    }

    /// With no inputs, verifies conflicting task spellings are rejected instead of selecting an arbitrary wire value.
    /// 无需参数，验证冲突的任务拼写被拒绝，不任意选择线协议值，成功时正常返回。
    #[test]
    fn openvulcan_catalog_rejects_ambiguous_task_bindings() {
        // body declares two wire values for the same query semantics without a server preference.
        // body 为同一查询语义声明两个线协议值，且没有服务端偏好。
        let body = task_catalog(&["retrieval_query", "search_query"]);
        // failure must identify the wire field without relying on the exact human-readable wording.
        // failure 必须标识线协议字段，不依赖人类可读文案的准确措辞。
        let failure =
            parse_catalog("fixture", 1, &body.to_string()).expect_err("ambiguous task binding");
        assert!(failure.contains("capabilities.allowed_tasks"));
    }

    /// With no inputs, verifies malformed descriptors expose their model location and missing schema field.
    /// 无需参数，验证损坏描述符暴露模型位置与缺失结构字段，成功时正常返回。
    #[test]
    fn openvulcan_catalog_schema_error_identifies_model_and_field() {
        // body omits one required field on the descriptor selected by its stable model id.
        // body 在按稳定模型标识选出的描述符上省略一个必填字段。
        let mut body = task_catalog(&["search_document"]);
        // models and model_index derive the error location from actual content rather than a fixed list position.
        // models 与 model_index 从实际内容派生错误位置，不使用固定列表位置。
        let models = body["models"].as_array_mut().expect("catalog array");
        let model_index = models
            .iter()
            .position(|model| model["model_id"] == "task-vector")
            .expect("fixture model");
        models[model_index]["capabilities"]
            .as_object_mut()
            .expect("embedding capabilities")
            .remove("max_batch_size");
        // failure retains both the concrete schema field and its containing source model entry.
        // failure 同时保留具体结构字段及包含它的源模型条目。
        let failure =
            parse_catalog("fixture", 1, &body.to_string()).expect_err("missing required field");
        assert!(failure.contains(&format!("models[{model_index}]")));
        assert!(failure.contains("max_batch_size"));
    }

    /// Returns a synthetic credential bound to region, with its exact server-owned expiration.
    /// 返回绑定到 region 并携带准确服务端到期时间的合成凭据。
    fn credential(region: OpenVulcanRegion) -> crate::CredentialSecretEnvelope {
        crate::CredentialSecretEnvelope::new(crate::CredentialSecretMaterial::OpenVulcanAuth {
            access_token: "a".repeat(43),
            authorization_id: uuid::Uuid::new_v4().to_string(),
            region,
            expires_at_unix_ms: 2_000_000_000_000,
            rotation_pending: false,
        })
        .expect("native credential")
    }

    /// Regional isolation rejects alternate deployments, redirects, ports and credential-bearing URL syntax.
    /// 区域隔离拒绝其他部署、重定向端点、端口以及携带凭据的 URL 语法。
    #[test]
    fn openvulcan_authorization_stays_in_its_deployment() {
        let secret = credential(OpenVulcanRegion::China)
            .execution_secret()
            .expect("execution projection");
        for url in [
            "https://auth.openvulcan.cn/api/v1/vulcancode/account",
            "https://api.openvulcan.cn/api/v1/vulcancode/embeddings",
        ] {
            assert!(
                authorization_header(&secret, url)
                    .unwrap()
                    .starts_with("Auth ")
            );
        }
        for url in [
            "https://auth.openvulcan.com",
            "https://api.openvulcan.com",
            "https://auth.openvulcan.cn.evil.example",
            "https://user@auth.openvulcan.cn",
            "http://auth.openvulcan.cn",
            "https://auth.openvulcan.cn:444",
            "https://auth.openvulcan.cn/#fragment",
        ] {
            assert!(
                authorization_header(&secret, url).is_err(),
                "must reject {url}"
            );
        }
    }

    /// Refresh eligibility derives from expiry and never treats a pending single-use rotation as usable.
    /// 刷新资格从到期时间派生，绝不将待处理的单次轮换视为可用。
    #[test]
    fn openvulcan_refresh_boundary_uses_server_expiry() {
        let mut secret = credential(OpenVulcanRegion::Global);
        let expiry = 2_000_000_000_000;
        assert!(!secret.requires_refresh(expiry - REFRESH_AHEAD_MS - 1, 0));
        assert!(secret.requires_refresh(expiry - REFRESH_AHEAD_MS, 0));
        if let crate::CredentialSecretMaterial::OpenVulcanAuth {
            rotation_pending, ..
        } = &mut secret.material
        {
            *rotation_pending = true;
        }
        assert!(secret.requires_refresh(1, 0));
        assert!(secret.execution_secret().is_err());
    }

    /// Dynamic catalog parsing preserves native capabilities and cannot invent chat or dimensions.
    /// 动态目录解析保留原生能力，不能虚构聊天或维度。
    #[test]
    fn openvulcan_catalog_preserves_native_operations_and_rejects_bad_bounds() {
        let mut body = serde_json::json!({"contract_version": CONTRACT_VERSION, "models": [
            {"model_id":"synthetic-vector", "display_name":"Vector", "kind":"embedding", "capabilities":{"max_input_tokens_per_item":8000,"max_batch_size":8,"default_dimensions":3,"allowed_dimensions":[3,6],"allowed_tasks":["query","document"],"allowed_output_kinds":["dense","sparse","dense_and_sparse"],"allowed_encodings":["float"]}},
            {"model_id":"synthetic-decision", "display_name":"Decision", "kind":"decision", "decision_capabilities":{"max_total_input_tokens":16384}}
        ]});
        let parsed = parse_catalog("fixture", 1, &body.to_string()).expect("native catalog");
        let vector = parsed
            .models
            .iter()
            .find(|model| model.id == "synthetic-vector")
            .expect("vector model");
        assert_eq!(
            vector.capabilities.operations,
            [AiOperation::EmbeddingCreate]
        );
        assert_eq!(
            vector
                .capabilities
                .embedding
                .as_ref()
                .unwrap()
                .default_dimensions,
            Some(3)
        );
        assert_eq!(
            vector.capabilities.embedding.as_ref().unwrap().tasks,
            BTreeSet::from([EmbeddingTask::Query, EmbeddingTask::Document])
        );
        let decision = parsed
            .models
            .iter()
            .find(|model| model.id == "synthetic-decision")
            .expect("decision model");
        assert_eq!(
            decision.capabilities.operations,
            [AiOperation::DecisionEvaluate]
        );
        body["models"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|model| model["kind"] == "embedding")
            .unwrap()["capabilities"]["default_dimensions"] = serde_json::json!(7);
        assert!(parse_catalog("fixture", 1, &body.to_string()).is_err());
        body["contract_version"] = serde_json::json!("unknown");
        assert!(parse_catalog("fixture", 1, &body.to_string()).is_err());
    }
}
