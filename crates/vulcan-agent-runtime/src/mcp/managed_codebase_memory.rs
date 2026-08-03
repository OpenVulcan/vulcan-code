//! Host-owned authorization and protocol helpers for the managed Codebase Memory MCP.
//! 宿主管理 Codebase Memory MCP 使用的宿主授权与协议辅助能力。

use crate::model::SessionId;
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

/// Private protocol contract required from the Vulcan Codebase Memory sidecar.
/// Vulcan Codebase Memory Sidecar 必须支持的私有协议契约。
pub const MANAGED_CODEBASE_MEMORY_CONTRACT: &str = "vulcan.codebase-memory/2";
/// Exact initialize product name required from the packaged Codebase Memory sidecar.
/// 打包 Codebase Memory Sidecar 在 initialize 中必须返回的精确产品名称。
pub const MANAGED_CODEBASE_MEMORY_PRODUCT: &str = "vulcan-codebase-memory-mcp";
/// Stable server identifier reserved for the packaged Codebase Memory sidecar.
/// 为打包的 Codebase Memory Sidecar 保留的稳定服务器标识。
pub const MANAGED_CODEBASE_MEMORY_SERVER_ID: &str = "vulcan-codebase-memory";

/// Host-reserved argument names that a model may never provide.
/// 模型绝不能提供的宿主保留参数名称。
pub const MANAGED_CODEBASE_MEMORY_RESERVED_ARGUMENTS: [&str; 3] =
    ["_vulcan", "project", "repo_path"];

/// Initial read-only Codebase Memory tools that Vulcan may expose to an AI.
/// Vulcan 首期可向 AI 暴露的只读 Codebase Memory 工具。
pub const MANAGED_CODEBASE_MEMORY_READ_ONLY_TOOLS: [&str; 10] = [
    "get_graph_schema",
    "search_graph",
    "query_graph",
    "index_status",
    "check_index_coverage",
    "trace_path",
    "get_architecture",
    "get_code_snippet",
    "search_code",
    "detect_changes",
];

/// Stable public namespace for host-managed Codebase Memory tools.
/// 宿主托管 Codebase Memory 工具使用的稳定公开命名空间。
const MANAGED_CODEBASE_MEMORY_PUBLIC_TOOL_PREFIX: &str = "vulcan_codebase_memory";

/// Exact enabled-project route injected into one managed MCP call.
/// 注入一次托管 MCP 调用的精确启用项目路由。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedCodebaseMemoryProjectRoute {
    /// Stable Vulcan project identifier.
    /// 稳定的 Vulcan 项目标识。
    pub project_id: String,
    /// Canonical absolute project root owned by the project entity.
    /// 项目实体持有的规范绝对项目根目录。
    pub pwd: String,
}

/// Mutable authorization table shared by execution preparation and in-flight tool bindings.
/// 由执行准备与执行中工具绑定共享的可变授权表。
#[derive(Debug, Clone, Default)]
pub struct ManagedCodebaseMemoryAuthorization {
    /// Atomically replaceable enabled-project and session-ownership state.
    /// 可原子替换的启用项目与会话归属状态。
    state: Arc<RwLock<ManagedCodebaseMemoryAuthorizationState>>,
}

/// Authoritative enabled-project and root-session ownership maps.
/// 权威启用项目与根会话归属映射。
#[derive(Debug, Clone, Default)]
struct ManagedCodebaseMemoryAuthorizationState {
    /// Enabled project routes keyed by stable project identifier.
    /// 按稳定项目标识索引的启用项目路由。
    enabled_projects: BTreeMap<String, ManagedCodebaseMemoryProjectRoute>,
    /// Root-session ownership keyed by stable session identifier.
    /// 按稳定会话标识索引的根会话归属。
    session_projects: BTreeMap<SessionId, String>,
}

/// Request-scoped handle used to project managed tools while retaining live reauthorization.
/// 用于投影托管工具并保留实时重新授权能力的请求级句柄。
#[derive(Debug, Clone)]
pub struct ManagedCodebaseMemoryExecutionContext {
    /// Real session identifier whose project controls tool visibility.
    /// 由所属项目控制工具可见性的真实会话标识。
    pub session_id: SessionId,
    /// Stable project identity resolved strictly from the root session at preparation time.
    /// 在准备阶段从根会话严格解析出的稳定项目身份。
    pub project_id: String,
    /// Shared authorization table checked both before registration and before invocation.
    /// 在注册前和调用前均会检查的共享授权表。
    pub authorization: ManagedCodebaseMemoryAuthorization,
}

impl ManagedCodebaseMemoryAuthorization {
    /// Replaces every authorized session route from one authoritative workspace snapshot.
    /// 使用一份权威工作区快照替换全部已授权会话路由。
    ///
    /// # Parameters
    /// 参数
    /// `routes` contains only sessions owned by currently existing and enabled projects.
    /// `routes` 仅包含当前仍存在且已启用项目所拥有的会话。
    pub fn replace_routes(
        &self,
        routes: impl IntoIterator<Item = (SessionId, ManagedCodebaseMemoryProjectRoute)>,
    ) {
        let routes = routes.into_iter().collect::<Vec<_>>();
        let enabled_projects = routes
            .iter()
            .map(|(_, route)| (route.project_id.clone(), route.clone()))
            .collect();
        let session_projects = routes
            .into_iter()
            .map(|(session_id, route)| (session_id, route.project_id))
            .collect();
        *self
            .state
            .write()
            .expect("managed Codebase Memory authorization table poisoned") =
            ManagedCodebaseMemoryAuthorizationState {
                enabled_projects,
                session_projects,
            };
    }

    /// Replaces enabled projects and root-session ownership from one authoritative workspace.
    /// 使用一份权威工作区替换启用项目与根会话归属。
    ///
    /// # Parameters
    /// 参数
    /// `projects` contains every currently enabled project route.
    /// `projects` 包含当前全部启用项目路由。
    /// `sessions` maps only root sessions owned by those enabled projects.
    /// `sessions` 仅映射这些启用项目持有的根会话。
    pub fn replace_workspace(
        &self,
        projects: impl IntoIterator<Item = ManagedCodebaseMemoryProjectRoute>,
        sessions: impl IntoIterator<Item = (SessionId, String)>,
    ) {
        let enabled_projects = projects
            .into_iter()
            .map(|route| (route.project_id.clone(), route))
            .collect::<BTreeMap<_, _>>();
        let session_projects = sessions
            .into_iter()
            .filter(|(_, project_id)| enabled_projects.contains_key(project_id))
            .collect();
        *self
            .state
            .write()
            .expect("managed Codebase Memory authorization table poisoned") =
            ManagedCodebaseMemoryAuthorizationState {
                enabled_projects,
                session_projects,
            };
    }

    /// Resolves the exact currently authorized project route for one session.
    /// 解析单个会话当前获得授权的精确项目路由。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` is the real host execution session identifier.
    /// `session_id` 是真实的宿主执行会话标识。
    ///
    /// # Returns
    /// 返回值
    /// Returns the current enabled route, or an English tool error when the session is not authorized.
    /// 返回当前启用路由；会话未获授权时返回英文工具错误。
    pub fn resolve(
        &self,
        session_id: SessionId,
    ) -> Result<ManagedCodebaseMemoryProjectRoute, String> {
        let state = self
            .state
            .read()
            .expect("managed Codebase Memory authorization table poisoned");
        let project_id = state
            .session_projects
            .get(&session_id)
            .ok_or_else(|| {
                String::from(
                    "Codebase Memory is unavailable because this session has no currently enabled project route.",
                )
            })?;
        state
            .enabled_projects
            .get(project_id)
            .cloned()
            .ok_or_else(|| {
                String::from(
                    "Codebase Memory is unavailable because this session has no currently enabled project route.",
                )
            })
    }

    /// Resolves one enabled project route by the exact stable project identity.
    /// 根据精确稳定项目身份解析一个启用项目路由。
    ///
    /// # Parameters
    /// 参数
    /// `project_id` is the project captured from strict root-session ownership.
    /// `project_id` 是从严格根会话归属捕获的项目。
    ///
    /// # Returns
    /// 返回值
    /// Returns the current route, or an English tool error after disablement/removal.
    /// 返回当前路由；项目停用或移除后返回英文工具错误。
    pub fn resolve_project(
        &self,
        project_id: &str,
    ) -> Result<ManagedCodebaseMemoryProjectRoute, String> {
        self.state
            .read()
            .expect("managed Codebase Memory authorization table poisoned")
            .enabled_projects
            .get(project_id)
            .cloned()
            .ok_or_else(|| {
                String::from(
                    "Codebase Memory is unavailable because the owning project is no longer enabled.",
                )
            })
    }
}

/// Returns whether one discovered tool is part of the host-approved read-only surface.
/// 返回某个发现工具是否属于宿主批准的只读工具面。
///
/// # Parameters
/// 参数
/// `tool_name` is the original unprefixed MCP tool name.
/// `tool_name` 是原始未加前缀的 MCP 工具名称。
///
/// # Returns
/// 返回值
/// Returns `true` only for the fixed initial read-only allowlist.
/// 仅对固定的首期只读白名单返回 `true`。
pub fn is_managed_codebase_memory_read_only_tool(tool_name: &str) -> bool {
    MANAGED_CODEBASE_MEMORY_READ_ONLY_TOOLS.contains(&tool_name)
}

/// Builds the model-visible Vulcan tool identifier for an allowed managed Codebase Memory tool.
/// 为允许的托管 Codebase Memory 工具构建模型可见的 Vulcan 工具标识。
///
/// # Parameters
/// 参数
/// `tool_name` is the original unprefixed MCP tool name.
/// `tool_name` 是原始未加前缀的 MCP 工具名称。
///
/// # Returns
/// 返回值
/// Returns the stable Vulcan public identifier only when the tool belongs to the managed allowlist.
/// 仅当工具属于托管白名单时返回稳定的 Vulcan 公开标识。
pub fn managed_codebase_memory_public_tool_id(tool_name: &str) -> Option<String> {
    is_managed_codebase_memory_read_only_tool(tool_name)
        .then(|| format!("{MANAGED_CODEBASE_MEMORY_PUBLIC_TOOL_PREFIX}_{tool_name}"))
}

/// Removes host-owned routing fields from one model-visible MCP argument schema.
/// 从模型可见的 MCP 参数 Schema 中移除宿主持有的路由字段。
///
/// # Parameters
/// 参数
/// `schema` is the original upstream JSON Schema discovered through MCP.
/// `schema` 是通过 MCP 发现的原始上游 JSON Schema。
///
/// # Returns
/// 返回值
/// Returns the schema with reserved properties and matching required entries removed.
/// 返回已移除保留属性及对应必填项的 Schema。
pub fn model_visible_managed_codebase_memory_schema(mut schema: Value) -> Value {
    strip_reserved_fields_from_same_instance_schema(&mut schema);
    schema
}

/// Removes reserved fields from schema branches that validate the same top-level argument object.
/// 从校验同一个顶层参数对象的 Schema 分支中移除保留字段。
fn strip_reserved_fields_from_same_instance_schema(schema: &mut Value) {
    let Some(object) = schema.as_object_mut() else {
        return;
    };
    if let Some(properties) = object.get_mut("properties").and_then(Value::as_object_mut) {
        for reserved in MANAGED_CODEBASE_MEMORY_RESERVED_ARGUMENTS {
            properties.remove(reserved);
        }
    }
    let remove_required =
        if let Some(required) = object.get_mut("required").and_then(Value::as_array_mut) {
            required.retain(|entry| {
                entry
                    .as_str()
                    .is_none_or(|name| !MANAGED_CODEBASE_MEMORY_RESERVED_ARGUMENTS.contains(&name))
            });
            required.is_empty()
        } else {
            false
        };
    if remove_required {
        object.remove("required");
    }
    for keyword in ["allOf", "anyOf", "oneOf"] {
        if let Some(branches) = object.get_mut(keyword).and_then(Value::as_array_mut) {
            for branch in branches {
                strip_reserved_fields_from_same_instance_schema(branch);
            }
        }
    }
    for keyword in ["if", "then", "else", "not"] {
        if let Some(branch) = object.get_mut(keyword) {
            strip_reserved_fields_from_same_instance_schema(branch);
        }
    }
}

/// Validates model arguments and injects the current host-owned routing envelope.
/// 校验模型参数并注入当前宿主持有的路由信封。
///
/// # Parameters
/// 参数
/// `args` is the untrusted model-produced tool argument value.
/// `args` 是不可信的模型生成工具参数值。
/// `route` is the route resolved again from live authorization state.
/// `route` 是从实时授权状态重新解析得到的路由。
/// `session_id` is the real host execution session identifier.
/// `session_id` 是真实的宿主执行会话标识。
///
/// # Returns
/// 返回值
/// Returns an object containing the private `_vulcan` envelope, or an English tool error.
/// 返回包含私有 `_vulcan` 信封的对象，或英文工具错误。
pub fn inject_managed_codebase_memory_context(
    args: Value,
    route: &ManagedCodebaseMemoryProjectRoute,
    session_id: SessionId,
) -> Result<Value, String> {
    let mut object = match args {
        Value::Object(object) => object,
        _ => {
            return Err(String::from(
                "Codebase Memory tool arguments must be a JSON object.",
            ));
        }
    };
    reject_reserved_arguments(&object)?;
    object.insert(
        String::from("_vulcan"),
        json!({
            "contract": MANAGED_CODEBASE_MEMORY_CONTRACT,
            "project_id": route.project_id,
            "pwd": route.pwd,
            "session_id": session_id.to_string(),
        }),
    );
    Ok(Value::Object(object))
}

/// Rejects any attempt to provide a host-reserved routing field.
/// 拒绝任何提供宿主保留路由字段的尝试。
///
/// # Parameters
/// 参数
/// `object` is the untrusted model-produced argument object.
/// `object` 是不可信的模型生成参数对象。
///
/// # Returns
/// 返回值
/// Returns success only when no reserved field is present.
/// 仅在不存在任何保留字段时返回成功。
fn reject_reserved_arguments(object: &Map<String, Value>) -> Result<(), String> {
    if let Some(name) = MANAGED_CODEBASE_MEMORY_RESERVED_ARGUMENTS
        .iter()
        .find(|name| object.contains_key(**name))
    {
        return Err(format!(
            "Codebase Memory argument `{name}` is host-managed and must not be provided by the model."
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    /// Verifies managed public identifiers are available only for the fixed read-only contract.
    /// 验证托管公开标识仅对固定只读契约可用。
    #[test]
    fn managed_public_tool_id_requires_read_only_contract_membership() {
        assert_eq!(
            managed_codebase_memory_public_tool_id("search_graph").as_deref(),
            Some("vulcan_codebase_memory_search_graph")
        );
        assert_eq!(managed_codebase_memory_public_tool_id("file_edit"), None);
    }

    /// Verifies that model-visible schemas cannot advertise host routing fields.
    /// 验证模型可见 Schema 不会公开宿主路由字段。
    #[test]
    fn managed_schema_removes_reserved_properties_and_required_entries() {
        let schema = json!({
            "type": "object",
            "properties": {
                "_vulcan": {"type": "object"},
                "project": {"type": "string"},
                "repo_path": {"type": "string"},
                "query": {"type": "string"}
            },
            "required": ["_vulcan", "project", "repo_path", "query"]
        });

        assert_eq!(
            model_visible_managed_codebase_memory_schema(schema),
            json!({
                "type": "object",
                "properties": {"query": {"type": "string"}},
                "required": ["query"]
            })
        );
    }

    /// Verifies composition branches are sanitized without deleting same-named fields from nested business objects.
    /// 验证组合分支会被清理，同时不会删除嵌套业务对象中的同名字段。
    #[test]
    fn managed_schema_sanitizes_compositions_but_preserves_nested_fields() {
        let schema = json!({
            "type": "object",
            "properties": {
                "filter": {
                    "type": "object",
                    "properties": {
                        "project": {"type": "string"}
                    }
                }
            },
            "allOf": [{
                "properties": {
                    "_vulcan": {"type": "object"},
                    "query": {"type": "string"}
                },
                "required": ["_vulcan", "query"]
            }]
        });

        let visible = model_visible_managed_codebase_memory_schema(schema);

        assert!(visible["properties"]["filter"]["properties"]["project"].is_object());
        assert!(visible["allOf"][0]["properties"]["_vulcan"].is_null());
        assert_eq!(visible["allOf"][0]["required"], json!(["query"]));
    }

    /// Verifies that injection uses live authorization and rejects forged host fields.
    /// 验证注入使用实时授权，并拒绝伪造的宿主字段。
    #[test]
    fn managed_context_injection_rejects_forged_routing() {
        let session_id = Uuid::new_v4();
        let route = ManagedCodebaseMemoryProjectRoute {
            project_id: String::from("project-a"),
            pwd: String::from(r"D:\projects\a"),
        };
        let error = inject_managed_codebase_memory_context(
            json!({"project": "project-b"}),
            &route,
            session_id,
        )
        .expect_err("forged project routing must be rejected");
        assert!(error.contains("host-managed"));

        let injected =
            inject_managed_codebase_memory_context(json!({"query": "symbol"}), &route, session_id)
                .expect("host routing should be injected");
        assert_eq!(injected["_vulcan"]["project_id"], "project-a");
        assert_eq!(injected["_vulcan"]["pwd"], r"D:\projects\a");
        assert_eq!(injected["_vulcan"]["session_id"], session_id.to_string());
    }

    /// Verifies that replacing authorization invalidates already built tool closures.
    /// 验证替换授权会使已经构建的工具闭包失去权限。
    #[test]
    fn replacing_authorization_invalidates_old_session_routes() {
        let authorization = ManagedCodebaseMemoryAuthorization::default();
        let session_id = Uuid::new_v4();
        authorization.replace_routes([(
            session_id,
            ManagedCodebaseMemoryProjectRoute {
                project_id: String::from("project-a"),
                pwd: String::from(r"D:\projects\a"),
            },
        )]);
        assert!(authorization.resolve(session_id).is_ok());

        authorization.replace_routes([]);
        assert!(authorization.resolve(session_id).is_err());
    }

    /// Verifies task sessions inherit the root project while the injected session id remains the real child id.
    /// 验证任务会话继承根项目，同时注入的会话标识仍为真实子会话标识。
    #[test]
    fn child_invocation_uses_root_project_and_real_child_session() {
        let authorization = ManagedCodebaseMemoryAuthorization::default();
        let root_session_id = Uuid::new_v4();
        let child_session_id = Uuid::new_v4();
        let route = ManagedCodebaseMemoryProjectRoute {
            project_id: String::from("project-a"),
            pwd: String::from(r"D:\projects\a"),
        };
        authorization.replace_workspace(
            [route.clone()],
            [(root_session_id, route.project_id.clone())],
        );

        let root_route = authorization
            .resolve(root_session_id)
            .expect("root session should remain authorized");
        let project_route = authorization
            .resolve_project(&root_route.project_id)
            .expect("child invocation should resolve the enabled root project");
        let injected = inject_managed_codebase_memory_context(
            json!({"query": "symbol"}),
            &project_route,
            child_session_id,
        )
        .expect("child invocation should receive host routing");

        assert_eq!(injected["_vulcan"]["project_id"], "project-a");
        assert_eq!(
            injected["_vulcan"]["session_id"],
            child_session_id.to_string()
        );

        authorization.replace_workspace([route], []);
        assert!(authorization.resolve(root_session_id).is_err());
    }
}
