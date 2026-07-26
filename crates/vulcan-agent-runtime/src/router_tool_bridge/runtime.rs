use super::schema::router_parameter_schema_to_json_schema;
use base64::Engine;
use reqwest::{Client, Method, StatusCode};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;
use uuid::Uuid;
use vulcan_provider_config::config::{
    PersistedRouterToolBinding, PersistedRouterToolConfiguration, RouterCapabilityContract,
    RouterClientToolContract, RouterParameterSchema, RouterToolParameterOwner,
    RouterToolParameterTargetKind, RouterToolResourceInput,
};
use vulcan_tools_manager::{
    VulcanPlanToolPolicy, VulcanToolCandidate, VulcanToolDescriptor, VulcanToolInvocationResult,
    VulcanToolSourceKind,
};

/// Maximum time spent following one already-created Router execution.
/// 跟随一个已创建 Router 执行的最长时间。
const EXECUTION_FOLLOW_TIMEOUT: Duration = Duration::from_secs(300);

/// Delay between reads of the same durable Router execution.
/// 同一持久化 Router 执行两次读取之间的延迟。
const EXECUTION_POLL_INTERVAL: Duration = Duration::from_millis(500);

/// Exact model-or-service selector used to discover Router contracts.
/// 用于发现 Router 合同的精确模型或服务选择器。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouterContractSelector {
    /// Immutable provider instance identifier.
    /// 不可变供应商实例标识。
    pub provider_instance_id: String,
    /// Exact model identifier when discovering model contracts.
    /// 发现模型合同时使用的精确模型标识。
    pub provider_model_id: Option<String>,
    /// Exact service identifier when discovering service contracts.
    /// 发现服务合同时使用的精确服务标识。
    pub provider_service_id: Option<String>,
    /// Optional exact execution profile identifier.
    /// 可选精确执行规格标识。
    pub execution_profile_id: Option<String>,
}

impl RouterContractSelector {
    /// Validates that exactly one provider-owned target is selected.
    /// 校验是否精确选择了一个供应商所属目标。
    fn validate(&self) -> Result<(), String> {
        if self.provider_instance_id.trim().is_empty() {
            return Err(String::from(
                "Router contract discovery requires a provider instance id.",
            ));
        }
        let model = self
            .provider_model_id
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty());
        let service = self
            .provider_service_id
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty());
        if model == service {
            return Err(String::from(
                "Router contract discovery requires exactly one model or service selector.",
            ));
        }
        Ok(())
    }
}

/// Discovers complete immutable contracts for one exact Router model or service target.
/// 为一个精确 Router 模型或服务目标发现完整不可变合同。
pub async fn discover_router_contracts(
    configuration: &PersistedRouterToolConfiguration,
    selector: &RouterContractSelector,
) -> Result<Vec<RouterCapabilityContract>, String> {
    selector.validate()?;
    let connection = RouterConnection::from_configuration(configuration)?;
    let request = RouterContractDiscoveryRequest {
        get: "contracts",
        provider_instance_id: selector.provider_instance_id.trim().to_string(),
        provider_model_id: selector
            .provider_model_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        provider_service_id: selector
            .provider_service_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        execution_profile_id: selector
            .execution_profile_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
    };
    let response: RouterContractDiscoveryResponse = connection
        .send_json(Method::POST, "/vulcan/v1/info", &request)
        .await?;
    if response.get != "contracts" {
        return Err(String::from(
            "Router contract discovery returned the wrong information branch.",
        ));
    }
    for contract in &response.contracts {
        if contract.client_tool().is_some() {
            contract.validate_tool_contract()?;
        }
    }
    Ok(response.contracts)
}

/// Appends deterministic Router-derived tool candidates with one frozen credential each.
/// 追加每项冻结一个凭据的确定性 Router 派生工具候选。
pub fn append_router_contract_tool_candidates(
    candidates: &mut Vec<VulcanToolCandidate>,
    configuration: &PersistedRouterToolConfiguration,
    workspace_root: Option<PathBuf>,
) {
    if !configuration.enabled {
        return;
    }
    let Ok(connection) = RouterConnection::from_configuration(configuration) else {
        return;
    };
    let mut names = BTreeMap::<String, usize>::new();
    let mut identifiers = BTreeMap::<String, usize>::new();
    for binding in configuration
        .bindings
        .iter()
        .filter(|binding| binding.enabled)
    {
        if binding.validate().is_ok()
            && let Some(tool) = binding.contract.client_tool()
        {
            *names.entry(tool.name.clone()).or_default() += 1;
            *identifiers.entry(binding.id.clone()).or_default() += 1;
        }
    }
    for binding in configuration
        .bindings
        .iter()
        .filter(|binding| binding.enabled)
    {
        let Some(tool) = binding.contract.client_tool() else {
            continue;
        };
        if binding.validate().is_err()
            || names.get(&tool.name) != Some(&1)
            || identifiers.get(&binding.id) != Some(&1)
        {
            continue;
        }
        let requires_workspace = !tool.resource_inputs.is_empty() || tool.output.auto_download;
        if requires_workspace && workspace_root.is_none() {
            continue;
        }
        let Ok(parameters_schema) = router_parameter_schema_to_json_schema(&tool.parameters) else {
            continue;
        };
        if jsonschema::draft202012::meta::validate(&parameters_schema).is_err()
            || jsonschema::draft202012::new(&parameters_schema).is_err()
        {
            continue;
        }
        let runtime = RouterToolRuntime {
            connection: connection.clone(),
            binding: binding.clone(),
            parameters_schema: parameters_schema.clone(),
            workspace_root: workspace_root.clone(),
        };
        let descriptor = VulcanToolDescriptor {
            id: format!("vulcan-router::{}", binding.id),
            name: tool.name.clone(),
            description: tool.description.clone(),
            help_text: None,
            source: VulcanToolSourceKind::Native,
        };
        let tool_name = tool.name.clone();
        candidates.push(VulcanToolCandidate {
            descriptor,
            parameters_schema,
            ai_visible: true,
            plan_policy: VulcanPlanToolPolicy::HeuristicText(tool_name),
            request_streaming: None,
            invoke: Arc::new(move |args, _session_id| {
                let runtime = runtime.clone();
                Box::pin(async move {
                    runtime
                        .invoke(args)
                        .await
                        .map(VulcanToolInvocationResult::from_model_output)
                })
            }),
        });
    }
}

/// Projects arguments through the first configured Router binding for focused contract tests.
/// 通过首条已配置 Router 绑定投影参数，以支持聚焦合同测试。
#[cfg(test)]
pub(super) async fn project_router_arguments_for_test(
    configuration: &PersistedRouterToolConfiguration,
    arguments: Value,
) -> Result<Value, String> {
    let binding = configuration
        .bindings
        .first()
        .cloned()
        .ok_or_else(|| String::from("Router test configuration requires one binding."))?;
    let tool = binding
        .contract
        .client_tool()
        .ok_or_else(|| String::from("Router test binding requires one client tool contract."))?
        .clone();
    let runtime = RouterToolRuntime {
        connection: RouterConnection::from_configuration(configuration)?,
        binding,
        parameters_schema: router_parameter_schema_to_json_schema(&tool.parameters)?,
        workspace_root: None,
    };
    runtime
        .project_arguments(&tool, &arguments)
        .await
        .map(|projected| projected.payload)
}

/// Immutable authenticated Router HTTP connection.
/// 不可变已认证 Router HTTP 连接。
#[derive(Clone)]
struct RouterConnection {
    /// Shared asynchronous HTTP client.
    /// 共享异步 HTTP 客户端。
    client: Client,
    /// Normalized Router origin.
    /// 规范化 Router 源地址。
    base_url: String,
    /// Call-plane secret retained only by the host runtime.
    /// 仅由宿主运行时持有的调用面秘密。
    api_key: Arc<str>,
}

impl RouterConnection {
    /// Resolves and validates one persisted Router connection.
    /// 解析并校验一条持久化 Router 连接。
    fn from_configuration(
        configuration: &PersistedRouterToolConfiguration,
    ) -> Result<Self, String> {
        let base_url = configuration.base_url.trim().trim_end_matches('/');
        if base_url.is_empty() {
            return Err(String::from("Vulcan Router base URL is not configured."));
        }
        reqwest::Url::parse(base_url)
            .map_err(|error| format!("Vulcan Router base URL is invalid: {error}"))?;
        let api_key = configuration
            .resolved_api_key()?
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| String::from("Vulcan Router API key is not configured."))?;
        Ok(Self {
            client: Client::new(),
            base_url: base_url.to_string(),
            api_key: Arc::from(api_key),
        })
    }

    /// Sends one authenticated JSON request and decodes one exact response type.
    /// 发送一个已认证 JSON 请求并解码一种精确响应类型。
    async fn send_json<Request, Response>(
        &self,
        method: Method,
        route: &str,
        payload: &Request,
    ) -> Result<Response, String>
    where
        Request: Serialize + ?Sized,
        Response: DeserializeOwned,
    {
        let response = self
            .client
            .request(method, format!("{}{}", self.base_url, route))
            .bearer_auth(self.api_key.as_ref())
            .json(payload)
            .send()
            .await
            .map_err(|error| format!("Vulcan Router request failed: {error}"))?;
        decode_router_response(response).await
    }

    /// Reads one authenticated JSON resource by route.
    /// 按路由读取一个已认证 JSON 资源。
    async fn get_json<Response>(&self, route: &str) -> Result<Response, String>
    where
        Response: DeserializeOwned,
    {
        let response = self
            .client
            .get(format!("{}{}", self.base_url, route))
            .bearer_auth(self.api_key.as_ref())
            .send()
            .await
            .map_err(|error| format!("Vulcan Router request failed: {error}"))?;
        decode_router_response(response).await
    }

    /// Downloads one authenticated Router resource body.
    /// 下载一个已认证 Router 资源正文。
    async fn download_bytes(&self, route: &str) -> Result<Vec<u8>, String> {
        let response = self
            .client
            .get(format!("{}{}", self.base_url, route))
            .bearer_auth(self.api_key.as_ref())
            .send()
            .await
            .map_err(|error| format!("Vulcan Router resource download failed: {error}"))?;
        let status = response.status();
        if !status.is_success() {
            return Err(router_http_error(status, response).await);
        }
        response
            .bytes()
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(|error| format!("Vulcan Router resource body could not be read: {error}"))
    }
}

/// One immutable configured Router tool runtime.
/// 一个不可变已配置 Router 工具运行时。
#[derive(Clone)]
struct RouterToolRuntime {
    /// Authenticated Router connection.
    /// 已认证 Router 连接。
    connection: RouterConnection,
    /// Exact contract and credential binding.
    /// 精确合同与凭据绑定。
    binding: PersistedRouterToolBinding,
    /// AI-facing JSON Schema derived from the Router contract.
    /// 从 Router 合同派生的 AI 可见 JSON Schema。
    parameters_schema: Value,
    /// Workspace boundary used for input and output paths.
    /// 输入与输出路径使用的工作区边界。
    workspace_root: Option<PathBuf>,
}

impl RouterToolRuntime {
    /// Executes one AI invocation as one create-once Router workflow.
    /// 将一次 AI 调用作为一个仅创建一次的 Router 工作流执行。
    async fn invoke(&self, arguments: Value) -> Result<Value, String> {
        self.binding.validate()?;
        let validator = jsonschema::draft202012::new(&self.parameters_schema)
            .map_err(|error| format!("Router tool JSON Schema could not be compiled: {error}"))?;
        if let Err(error) = validator.validate(&arguments) {
            return Err(format!(
                "Router tool arguments do not satisfy the published contract: {error}"
            ));
        }
        let tool = self.binding.contract.client_tool().ok_or_else(|| {
            String::from("Router capability contract no longer contains a client tool contract.")
        })?;
        let projected = self.project_arguments(tool, &arguments).await?;
        let request_id = format!("vcode_{}", Uuid::new_v4().simple());
        let request = RouterExecutionRequest {
            protocol_version: "1.0",
            request_id: request_id.clone(),
            idempotency_key: request_id,
            target: execution_target(&self.binding)?,
            operation: self.binding.contract.operation.clone(),
            capability_contract_id: self.binding.contract.reference.id.clone(),
            capability_contract_revision: self.binding.contract.reference.catalog_revision,
            capability_contract_hash: self.binding.contract.reference.content_hash.clone(),
            parameters: projected.parameters,
            stream: false,
            dispatch_mode: "deferred",
            payload: projected.payload,
            projection_policy: RouterProjectionPolicy::default(),
            budget: RouterOperationBudget {
                max_provider_tasks: 1,
            },
            retry_policy: RouterRetryPolicy { max_attempts: 1 },
        };
        let created: RouterExecutionCreateResponse = self
            .connection
            .send_json(Method::POST, "/vulcan/v1/executions", &request)
            .await?;
        let execution = self.follow_execution(created.execution).await?;
        self.render_result(tool, execution).await
    }

    /// Projects AI arguments into a typed VCP payload and capability parameter object.
    /// 将 AI 参数投影为类型化 VCP Payload 与能力参数对象。
    async fn project_arguments(
        &self,
        tool: &RouterClientToolContract,
        arguments: &Value,
    ) -> Result<ProjectedInvocation, String> {
        let mut payload = Value::Object(Map::new());
        let mut parameters = Value::Object(Map::new());
        for mapping in &tool.mappings {
            if mapping.owner != RouterToolParameterOwner::Ai {
                return Err(format!(
                    "Router tool mapping `{}` is not AI-owned.",
                    mapping.source_path
                ));
            }
            let Some(source) = value_at_path(arguments, &mapping.source_path) else {
                if parameter_path_is_required(&tool.parameters, &mapping.source_path) {
                    return Err(format!(
                        "Router tool mapping source `{}` is missing.",
                        mapping.source_path
                    ));
                }
                continue;
            };
            match mapping.target_kind {
                RouterToolParameterTargetKind::Operation => {
                    set_value_at_path(&mut payload, &mapping.target_path, source.clone())?;
                }
                RouterToolParameterTargetKind::ExecutionParameter => {
                    set_value_at_path(&mut parameters, &mapping.target_path, source.clone())?;
                }
                RouterToolParameterTargetKind::ResourceImport => {
                    let input = tool
                        .resource_inputs
                        .iter()
                        .find(|input| input.source_path == mapping.source_path)
                        .ok_or_else(|| {
                            format!(
                                "Router tool resource mapping `{}` has no resource declaration.",
                                mapping.source_path
                            )
                        })?;
                    let resources = self.import_resource_values(input, source).await?;
                    set_value_at_path(&mut payload, &mapping.target_path, resources)?;
                }
            }
        }
        let parameters = if parameters
            .as_object()
            .is_some_and(|value| !value.is_empty())
        {
            Some(parameters)
        } else {
            None
        };
        Ok(ProjectedInvocation {
            payload,
            parameters,
        })
    }

    /// Imports all local paths from one resource parameter and returns typed VCP media inputs.
    /// 导入一个资源参数中的全部本地路径并返回类型化 VCP 媒体输入。
    async fn import_resource_values(
        &self,
        input: &RouterToolResourceInput,
        source: &Value,
    ) -> Result<Value, String> {
        let paths = local_path_values(source)?;
        if paths.len() < input.minimum_items
            || input.maximum_items > 0 && paths.len() > input.maximum_items
        {
            return Err(format!(
                "Router tool resource `{}` requires {}..={} local paths.",
                input.source_path,
                input.minimum_items,
                if input.maximum_items == 0 {
                    usize::MAX
                } else {
                    input.maximum_items
                }
            ));
        }
        let mut media_inputs = Vec::with_capacity(paths.len());
        for (index, path) in paths.iter().enumerate() {
            let resource = self.import_local_file(input, path).await?;
            media_inputs.push(json!({
                "id": format!("{}_{}", input.source_path.replace('.', "_"), index + 1),
                "kind": input.kind,
                "role": input.role,
                "resource": {"resource_id": resource.id}
            }));
        }
        Ok(Value::Array(media_inputs))
    }

    /// Imports one workspace-local file into Router without exposing its bytes to the AI.
    /// 将一个工作区本地文件导入 Router 且不向 AI 暴露其字节。
    async fn import_local_file(
        &self,
        input: &RouterToolResourceInput,
        local_path: &str,
    ) -> Result<RouterResource, String> {
        let resolved = resolve_workspace_file(self.workspace_root.as_deref(), local_path)?;
        let metadata = tokio::fs::metadata(&resolved)
            .await
            .map_err(|error| format!("Local Router input metadata could not be read: {error}"))?;
        if !metadata.is_file() {
            return Err(format!(
                "Router tool input `{}` is not a regular file.",
                resolved.display()
            ));
        }
        if input.maximum_item_bytes > 0 && metadata.len() >= input.maximum_item_bytes as u64 {
            return Err(format!(
                "Router tool input `{}` exceeds the published exclusive byte ceiling.",
                resolved.display()
            ));
        }
        let bytes = tokio::fs::read(&resolved)
            .await
            .map_err(|error| format!("Local Router input could not be read: {error}"))?;
        let request = RouterResourceImportRequest {
            kind: input.kind.clone(),
            retention: "ephemeral",
            base64: RouterBase64Import {
                encoding: "standard",
                data: base64::engine::general_purpose::STANDARD.encode(bytes),
            },
        };
        let resource: RouterResource = self
            .connection
            .send_json(Method::POST, "/vulcan/v1/resources/import", &request)
            .await?;
        validate_imported_resource(input, &resource)?;
        Ok(resource)
    }

    /// Follows one already-created durable execution without ever recreating the provider task.
    /// 跟随一个已创建的持久执行且绝不重新创建供应商任务。
    async fn follow_execution(
        &self,
        mut execution: RouterExecutionRecord,
    ) -> Result<RouterExecutionRecord, String> {
        let deadline = Instant::now() + EXECUTION_FOLLOW_TIMEOUT;
        loop {
            match execution.status.as_str() {
                "succeeded" | "partially_succeeded" => return Ok(execution),
                "failed" | "cancelled" | "expired" => {
                    return Err(render_execution_failure(&execution));
                }
                "accepted" | "preparing_inputs" | "queued" | "running" | "waiting_retry" => {}
                status => {
                    return Err(format!(
                        "Router execution `{}` returned unknown status `{status}`.",
                        execution.id
                    ));
                }
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "Router execution `{}` is still in progress after the local wait timeout. The paid task was created once and was not restarted; query this execution id to resume.",
                    execution.id
                ));
            }
            tokio::time::sleep(EXECUTION_POLL_INTERVAL).await;
            execution = self
                .connection
                .get_json(&format!("/vulcan/v1/executions/{}", execution.id))
                .await?;
        }
    }

    /// Validates and renders one terminal Router result for the AI.
    /// 校验并为 AI 渲染一个终态 Router 结果。
    async fn render_result(
        &self,
        tool: &RouterClientToolContract,
        execution: RouterExecutionRecord,
    ) -> Result<Value, String> {
        let result = execution.result.ok_or_else(|| {
            format!(
                "Router execution `{}` succeeded without a result.",
                execution.id
            )
        })?;
        if result.resources.len() < tool.output.minimum_items
            || result.resources.len() > tool.output.maximum_items
        {
            return Err(format!(
                "Router execution `{}` returned {} resources outside the published {}..={} range.",
                execution.id,
                result.resources.len(),
                tool.output.minimum_items,
                tool.output.maximum_items
            ));
        }
        let mut values = Vec::with_capacity(result.resources.len());
        for resource in result.resources {
            validate_output_resource(tool, &resource)?;
            if tool.output.auto_download {
                values.push(Value::String(
                    self.download_output_resource(&execution.id, &resource)
                        .await?
                        .to_string_lossy()
                        .to_string(),
                ));
            } else {
                values.push(Value::String(resource.id));
            }
        }
        let mut output = Value::Object(Map::new());
        set_value_at_path(
            &mut output,
            &tool.output.ai_result_path,
            Value::Array(values),
        )?;
        Ok(output)
    }

    /// Downloads and verifies one Router-owned output resource inside the workspace.
    /// 在工作区内下载并校验一个 Router 所有输出资源。
    async fn download_output_resource(
        &self,
        execution_id: &str,
        resource: &RouterResource,
    ) -> Result<PathBuf, String> {
        let workspace = canonical_workspace_root(self.workspace_root.as_deref())?;
        let output_directory = workspace
            .join(".vulcan")
            .join("router-output")
            .join(safe_filename_segment(execution_id));
        tokio::fs::create_dir_all(&output_directory)
            .await
            .map_err(|error| format!("Router output directory could not be created: {error}"))?;
        let extension = extension_for_mime_type(&resource.mime_type);
        let output_path = output_directory.join(format!(
            "{}-{}.{}",
            safe_filename_segment(&resource.id),
            &resource.sha256[..12],
            extension
        ));
        let bytes = self
            .connection
            .download_bytes(&format!("/vulcan/v1/resources/{}/content", resource.id))
            .await?;
        if bytes.len() as i64 != resource.size_bytes {
            return Err(format!(
                "Router resource `{}` byte length differs from its authoritative metadata.",
                resource.id
            ));
        }
        let digest = hex_lowercase(&Sha256::digest(&bytes));
        if digest != resource.sha256 {
            return Err(format!(
                "Router resource `{}` SHA-256 digest verification failed.",
                resource.id
            ));
        }
        tokio::fs::write(&output_path, bytes)
            .await
            .map_err(|error| format!("Router output file could not be written: {error}"))?;
        Ok(output_path)
    }
}

/// Exact Router information request for immutable capability contracts.
/// 用于不可变能力合同的精确 Router 信息请求。
#[derive(Serialize)]
struct RouterContractDiscoveryRequest {
    /// Selected information branch.
    /// 选定信息分支。
    get: &'static str,
    /// Immutable provider instance identifier.
    /// 不可变供应商实例标识。
    provider_instance_id: String,
    /// Optional exact model identifier.
    /// 可选精确模型标识。
    #[serde(skip_serializing_if = "Option::is_none")]
    provider_model_id: Option<String>,
    /// Optional exact service identifier.
    /// 可选精确服务标识。
    #[serde(skip_serializing_if = "Option::is_none")]
    provider_service_id: Option<String>,
    /// Optional exact execution profile identifier.
    /// 可选精确执行规格标识。
    #[serde(skip_serializing_if = "Option::is_none")]
    execution_profile_id: Option<String>,
}

/// Exact Router information response for immutable capability contracts.
/// 用于不可变能力合同的精确 Router 信息响应。
#[derive(Deserialize)]
struct RouterContractDiscoveryResponse {
    /// Echoed information branch.
    /// 回显信息分支。
    get: String,
    /// Complete exact-target contracts.
    /// 完整精确目标合同。
    contracts: Vec<RouterCapabilityContract>,
}

/// Projected invocation pieces placed into one VCP request.
/// 放入一个 VCP 请求的投影调用片段。
struct ProjectedInvocation {
    /// Closed operation payload union.
    /// 封闭操作 Payload 联合。
    payload: Value,
    /// Capability-scoped parameter object when non-empty.
    /// 非空时的能力作用域参数对象。
    parameters: Option<Value>,
}

/// Typed create-once VCP execution request.
/// 类型化的仅创建一次 VCP 执行请求。
#[derive(Serialize)]
struct RouterExecutionRequest {
    /// Exact VCP protocol version.
    /// 精确 VCP 协议版本。
    protocol_version: &'static str,
    /// Stable Router-visible request identifier.
    /// 稳定 Router 可见请求标识。
    request_id: String,
    /// Stable idempotency key reused only for this logical invocation.
    /// 仅为本次逻辑调用复用的稳定幂等键。
    idempotency_key: String,
    /// Exact model or service plus credential target.
    /// 精确模型或服务及凭据目标。
    target: RouterExecutionTarget,
    /// Sole closed operation.
    /// 唯一封闭操作。
    operation: String,
    /// Immutable capability contract identifier.
    /// 不可变能力合同标识。
    capability_contract_id: String,
    /// Immutable capability contract revision.
    /// 不可变能力合同修订。
    capability_contract_revision: u64,
    /// Immutable capability contract semantic hash.
    /// 不可变能力合同语义哈希。
    capability_contract_hash: String,
    /// Provider-specific validated execution parameters.
    /// 供应商专属已校验执行参数。
    #[serde(skip_serializing_if = "Option::is_none")]
    parameters: Option<Value>,
    /// Whether semantic streaming is requested.
    /// 是否请求语义流式输出。
    stream: bool,
    /// Durable deferred execution mode.
    /// 持久延迟执行模式。
    dispatch_mode: &'static str,
    /// Exact-one operation payload union.
    /// 精确单项操作 Payload 联合。
    payload: Value,
    /// Explicitly denied lossy media projections.
    /// 显式拒绝的有损媒体投影。
    projection_policy: RouterProjectionPolicy,
    /// Hard caller-owned provider task ceiling.
    /// 调用方拥有的供应商任务硬上限。
    budget: RouterOperationBudget,
    /// Hard single-attempt retry policy.
    /// 硬性单次尝试重试策略。
    retry_policy: RouterRetryPolicy,
}

/// Exact-one model or service VCP target.
/// 精确单项模型或服务 VCP 目标。
#[derive(Serialize)]
struct RouterExecutionTarget {
    /// Exact model target.
    /// 精确模型目标。
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<RouterModelSelection>,
    /// Exact service target.
    /// 精确服务目标。
    #[serde(skip_serializing_if = "Option::is_none")]
    service: Option<RouterServiceSelection>,
}

/// Exact model and credential selection.
/// 精确模型与凭据选择。
#[derive(Serialize)]
struct RouterModelSelection {
    /// Sole exact resolution mode.
    /// 唯一精确解析模式。
    target: &'static str,
    /// Immutable provider instance.
    /// 不可变供应商实例。
    provider_instance_id: String,
    /// Exact provider model.
    /// 精确供应商模型。
    provider_model_id: String,
    /// Exact execution profile.
    /// 精确执行规格。
    execution_profile_id: String,
    /// Exact configured account.
    /// 精确已配置账号。
    credential_id: String,
}

/// Exact service offering and credential selection.
/// 精确服务产品与凭据选择。
#[derive(Serialize)]
struct RouterServiceSelection {
    /// Immutable provider instance.
    /// 不可变供应商实例。
    provider_instance_id: String,
    /// Exact provider service.
    /// 精确供应商服务。
    provider_service_id: String,
    /// Exact service offering.
    /// 精确服务产品。
    service_offering_id: String,
    /// Exact execution profile.
    /// 精确执行规格。
    execution_profile_id: String,
    /// Exact configured account.
    /// 精确已配置账号。
    credential_id: String,
}

/// Lossy media projection policy with every permission denied.
/// 全部许可均拒绝的有损媒体投影策略。
#[derive(Default, Serialize)]
struct RouterProjectionPolicy {
    /// Whether video-to-frame projection is allowed.
    /// 是否允许视频转帧投影。
    allow_frame_sequence: bool,
    /// Whether embedded audio extraction is allowed.
    /// 是否允许内嵌音轨提取。
    allow_audio_track: bool,
    /// Whether media transcoding is allowed.
    /// 是否允许媒体转码。
    allow_transcode: bool,
}

/// Hard operation ceilings owned by Vulcan Code.
/// Vulcan Code 拥有的操作硬上限。
#[derive(Serialize)]
struct RouterOperationBudget {
    /// Maximum provider tasks created by this invocation.
    /// 本次调用可创建的最大供应商任务数。
    max_provider_tasks: usize,
}

/// Hard retry policy that forbids a second provider dispatch.
/// 禁止第二次供应商分派的硬重试策略。
#[derive(Serialize)]
struct RouterRetryPolicy {
    /// Maximum provider dispatch attempts.
    /// 最大供应商分派尝试次数。
    max_attempts: usize,
}

/// One create execution response.
/// 一个创建执行响应。
#[derive(Deserialize)]
struct RouterExecutionCreateResponse {
    /// Durable execution record.
    /// 持久执行记录。
    execution: RouterExecutionRecord,
}

/// Client-safe durable Router execution projection.
/// 客户端安全的持久 Router 执行投影。
#[derive(Deserialize)]
struct RouterExecutionRecord {
    /// Opaque Router execution identifier.
    /// 不透明 Router 执行标识。
    id: String,
    /// Closed durable lifecycle status.
    /// 封闭持久生命周期状态。
    status: String,
    /// Terminal or provider-confirmed partial result.
    /// 终态或供应商确认的部分结果。
    #[serde(default)]
    result: Option<RouterExecutionResult>,
    /// Client-safe terminal failure.
    /// 客户端安全终态失败。
    #[serde(default)]
    failure: Option<RouterExecutionFailure>,
}

/// Generated-resource subset of one Router execution result.
/// 一个 Router 执行结果的生成资源子集。
#[derive(Deserialize)]
struct RouterExecutionResult {
    /// Completed Router-owned generated resources.
    /// 已完成的 Router 所有生成资源。
    #[serde(default)]
    resources: Vec<RouterResource>,
}

/// Safe Router execution failure subset.
/// 安全 Router 执行失败子集。
#[derive(Deserialize)]
struct RouterExecutionFailure {
    /// Stable machine-readable failure code.
    /// 稳定机器可读失败码。
    code: String,
    /// Whether an explicit caller retry may be safe.
    /// 显式调用方重试是否可能安全。
    retryable: bool,
    /// Completed provider attempt count.
    /// 已完成供应商尝试次数。
    attempt: u32,
    /// Stable Router request correlation identifier.
    /// 稳定 Router 请求关联标识。
    router_request_id: String,
}

/// Local-byte resource import request.
/// 本地字节资源导入请求。
#[derive(Serialize)]
struct RouterResourceImportRequest {
    /// Exact Router media kind.
    /// 精确 Router 媒体类型。
    kind: String,
    /// Closed short-lived retention policy.
    /// 封闭短生命周期保留策略。
    retention: &'static str,
    /// Standard Base64 source.
    /// 标准 Base64 来源。
    base64: RouterBase64Import,
}

/// Standard Base64 resource import source.
/// 标准 Base64 资源导入来源。
#[derive(Serialize)]
struct RouterBase64Import {
    /// Closed Base64 alphabet.
    /// 封闭 Base64 字母表。
    encoding: &'static str,
    /// Encoded local bytes.
    /// 编码后的本地字节。
    data: String,
}

/// Public Router resource metadata required by the Vulcan Code bridge.
/// Vulcan Code 桥接所需的公开 Router 资源元数据。
#[derive(Deserialize)]
struct RouterResource {
    /// Opaque Router resource identifier.
    /// 不透明 Router 资源标识。
    id: String,
    /// Exact resource family.
    /// 精确资源类别。
    kind: String,
    /// Authoritative MIME type after magic inspection.
    /// 魔数检查后的权威 MIME 类型。
    mime_type: String,
    /// Exact byte length.
    /// 精确字节长度。
    size_bytes: i64,
    /// Lowercase SHA-256 digest.
    /// 小写 SHA-256 摘要。
    sha256: String,
    /// Durable resource lifecycle state.
    /// 持久资源生命周期状态。
    state: String,
}

/// Builds one exact VCP target from a frozen Router tool binding.
/// 从一个冻结 Router 工具绑定构建精确 VCP 目标。
fn execution_target(binding: &PersistedRouterToolBinding) -> Result<RouterExecutionTarget, String> {
    let contract = &binding.contract;
    if contract.kind == "model" {
        return Ok(RouterExecutionTarget {
            model: Some(RouterModelSelection {
                target: "exact",
                provider_instance_id: contract.provider_instance_id.clone(),
                provider_model_id: contract.provider_model_id.clone(),
                execution_profile_id: contract.execution_profile_id.clone(),
                credential_id: binding.credential_id.clone(),
            }),
            service: None,
        });
    }
    if contract.kind == "service" {
        return Ok(RouterExecutionTarget {
            model: None,
            service: Some(RouterServiceSelection {
                provider_instance_id: contract.provider_instance_id.clone(),
                provider_service_id: contract.provider_service_id.clone(),
                service_offering_id: contract.offering_id.clone(),
                execution_profile_id: contract.execution_profile_id.clone(),
                credential_id: binding.credential_id.clone(),
            }),
        });
    }
    Err(String::from(
        "Router tool binding contains an unsupported contract target kind.",
    ))
}

/// Decodes one successful Router JSON response or returns one bounded safe error.
/// 解码一个成功 Router JSON 响应或返回一个受限安全错误。
async fn decode_router_response<Response>(response: reqwest::Response) -> Result<Response, String>
where
    Response: DeserializeOwned,
{
    let status = response.status();
    if !status.is_success() {
        return Err(router_http_error(status, response).await);
    }
    response
        .json::<Response>()
        .await
        .map_err(|error| format!("Vulcan Router response could not be decoded: {error}"))
}

/// Renders one bounded Router HTTP failure without exposing request secrets.
/// 渲染一个受限 Router HTTP 失败且不暴露请求秘密。
async fn router_http_error(status: StatusCode, response: reqwest::Response) -> String {
    let message = response
        .text()
        .await
        .ok()
        .and_then(|body| serde_json::from_str::<RouterErrorResponse>(&body).ok())
        .map(|value| value.error)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| String::from("request rejected"));
    format!("Vulcan Router returned HTTP {}: {message}", status.as_u16())
}

/// Safe Router error response.
/// 安全 Router 错误响应。
#[derive(Deserialize)]
struct RouterErrorResponse {
    /// Client-safe error message.
    /// 客户端安全错误消息。
    error: String,
}

/// Returns one source value by exact dotted object path.
/// 按精确点分对象路径返回一个源值。
fn value_at_path<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = root;
    for segment in path.split('.') {
        current = current.as_object()?.get(segment)?;
    }
    Some(current)
}

/// Reports whether every object-property segment on one mapped path is required by the published schema.
/// 报告一个映射路径上的每个对象属性段是否都被已发布 Schema 标记为必填。
fn parameter_path_is_required(schema: &RouterParameterSchema, path: &str) -> bool {
    let mut current = schema;
    for segment in path.split('.') {
        let Some(property) = current
            .properties
            .iter()
            .find(|property| property.name == segment)
        else {
            return false;
        };
        if !property.required {
            return false;
        }
        current = &property.schema;
    }
    true
}

/// Assigns one value to an exact dotted object path and rejects conflicting parents or writes.
/// 将一个值赋给精确点分对象路径并拒绝冲突父级或重复写入。
fn set_value_at_path(root: &mut Value, path: &str, value: Value) -> Result<(), String> {
    let segments = path.split('.').collect::<Vec<_>>();
    if segments.is_empty()
        || segments
            .iter()
            .any(|segment| segment.is_empty() || segment.trim() != *segment)
    {
        return Err(format!("Router mapping target path `{path}` is invalid."));
    }
    let mut current = root;
    for segment in &segments[..segments.len() - 1] {
        let object = current
            .as_object_mut()
            .ok_or_else(|| format!("Router mapping parent for `{path}` is not an object."))?;
        current = object
            .entry((*segment).to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if !current.is_object() {
            return Err(format!(
                "Router mapping parent `{segment}` conflicts with an existing scalar."
            ));
        }
    }
    let leaf = segments[segments.len() - 1];
    let object = current
        .as_object_mut()
        .ok_or_else(|| format!("Router mapping target parent for `{path}` is not an object."))?;
    if object.insert(leaf.to_string(), value).is_some() {
        return Err(format!(
            "Router mapping target `{path}` was assigned twice."
        ));
    }
    Ok(())
}

/// Extracts one string or string-array local path parameter.
/// 提取一个字符串或字符串数组本地路径参数。
fn local_path_values(source: &Value) -> Result<Vec<String>, String> {
    if let Some(path) = source.as_str() {
        return Ok(vec![path.to_string()]);
    }
    let values = source
        .as_array()
        .ok_or_else(|| String::from("Router resource parameter must be a local path array."))?;
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|path| !path.trim().is_empty())
                .map(str::to_string)
                .ok_or_else(|| {
                    String::from("Router resource path array contains a non-string or empty value.")
                })
        })
        .collect()
}

/// Resolves one local file and proves it remains inside the selected workspace.
/// 解析一个本地文件并证明其仍位于选定工作区内部。
fn resolve_workspace_file(
    workspace_root: Option<&Path>,
    local_path: &str,
) -> Result<PathBuf, String> {
    if local_path.trim().is_empty() {
        return Err(String::from("Router tool local path is empty."));
    }
    let workspace = canonical_workspace_root(workspace_root)?;
    let candidate = PathBuf::from(local_path);
    let candidate = if candidate.is_absolute() {
        candidate
    } else {
        workspace.join(candidate)
    };
    let resolved = candidate.canonicalize().map_err(|error| {
        format!(
            "Router tool local path `{}` could not be resolved: {error}",
            candidate.display()
        )
    })?;
    if !resolved.starts_with(&workspace) {
        return Err(format!(
            "Router tool local path `{}` is outside the selected workspace.",
            resolved.display()
        ));
    }
    Ok(resolved)
}

/// Canonicalizes the required workspace root.
/// 规范化必需的工作区根目录。
fn canonical_workspace_root(workspace_root: Option<&Path>) -> Result<PathBuf, String> {
    workspace_root
        .ok_or_else(|| String::from("Router tool requires one selected workspace."))
        .and_then(|path| {
            path.canonicalize()
                .map_err(|error| format!("Router tool workspace could not be resolved: {error}"))
        })
}

/// Validates authoritative Router metadata for one imported local resource.
/// 校验一个已导入本地资源的权威 Router 元数据。
fn validate_imported_resource(
    input: &RouterToolResourceInput,
    resource: &RouterResource,
) -> Result<(), String> {
    if resource.state != "ready" || resource.kind != input.kind {
        return Err(format!(
            "Router imported resource `{}` is not ready with the published kind.",
            resource.id
        ));
    }
    if !input.mime_types.is_empty() && !input.mime_types.contains(&resource.mime_type) {
        return Err(format!(
            "Router imported resource `{}` has unsupported MIME type `{}`.",
            resource.id, resource.mime_type
        ));
    }
    if input.maximum_item_bytes > 0 && resource.size_bytes >= input.maximum_item_bytes {
        return Err(format!(
            "Router imported resource `{}` exceeds the published exclusive byte ceiling.",
            resource.id
        ));
    }
    Ok(())
}

/// Validates one generated resource against the frozen output contract.
/// 根据冻结输出合同校验一个生成资源。
fn validate_output_resource(
    tool: &RouterClientToolContract,
    resource: &RouterResource,
) -> Result<(), String> {
    if resource.state != "ready"
        || resource.kind != tool.output.kind
        || resource.size_bytes < 0
        || resource.sha256.len() != 64
        || !resource
            .sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(format!(
            "Router output resource `{}` has invalid authoritative metadata.",
            resource.id
        ));
    }
    if !tool.output.mime_types.is_empty() && !tool.output.mime_types.contains(&resource.mime_type) {
        return Err(format!(
            "Router output resource `{}` has MIME type `{}` outside the frozen contract.",
            resource.id, resource.mime_type
        ));
    }
    Ok(())
}

/// Renders one terminal Router failure with stable safe fields only.
/// 仅使用稳定安全字段渲染一个终态 Router 失败。
fn render_execution_failure(execution: &RouterExecutionRecord) -> String {
    let Some(failure) = execution.failure.as_ref() else {
        return format!(
            "Router execution `{}` ended with status `{}` without a failure record.",
            execution.id, execution.status
        );
    };
    format!(
        "Router execution `{}` failed with code `{}` after {} attempt(s); retryable={}, request_id={}.",
        execution.id, failure.code, failure.attempt, failure.retryable, failure.router_request_id
    )
}

/// Returns one stable extension for an authoritative MIME type.
/// 为一个权威 MIME 类型返回稳定扩展名。
fn extension_for_mime_type(mime_type: &str) -> &'static str {
    match mime_type {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/webp" => "webp",
        "image/gif" => "gif",
        "audio/mpeg" => "mp3",
        "audio/wav" | "audio/x-wav" => "wav",
        "audio/flac" => "flac",
        "audio/ogg" => "ogg",
        "video/mp4" => "mp4",
        "video/webm" => "webm",
        "application/pdf" => "pdf",
        "application/json" => "json",
        "text/plain" => "txt",
        _ => "bin",
    }
}

/// Converts one SHA-256 digest into lowercase hexadecimal.
/// 将一个 SHA-256 摘要转换为小写十六进制。
fn hex_lowercase(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

/// Converts an opaque Router identifier into one traversal-safe filename segment.
/// 将一个不透明 Router 标识转换为防路径穿越的文件名片段。
fn safe_filename_segment(value: &str) -> String {
    let filtered = value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
        .collect::<String>();
    if filtered.is_empty() {
        String::from("resource")
    } else {
        filtered
    }
}
