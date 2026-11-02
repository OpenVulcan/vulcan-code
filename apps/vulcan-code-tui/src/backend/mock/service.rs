// Deterministic in-process service for the same RPC contracts as the real backend.
// 与真实后端共用 RPC 协议的确定性进程内服务。

use crate::service_api::{CallKind, ServiceCall, scrub_secrets};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[path = "service_plugins.rs"]
mod plugins;

#[path = "service_knowledge.rs"]
mod knowledge;

#[cfg(test)]
#[path = "service_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "service_knowledge_tests.rs"]
mod knowledge_tests;

/// Owns synthetic service DTOs; known mutations change the authority queried by later loaders.
/// 持有合成服务 DTO；已知变更修改后续加载器查询的权威状态。
pub struct FakeService {
    /// System-facing workbench fields, containing no real credentials.
    /// 面向系统页面的工作台字段，不包含真实凭据。
    pub workbench: Value,
    /// Registered host project identities, shared by project-scoped mock settings.
    /// 已登记的宿主项目身份，由项目作用域的模拟设置共用。
    project_ids: BTreeSet<String>,
    /// Raw loader fixtures keyed by exact RPC method.
    /// 按准确 RPC 方法索引的原始加载器夹具。
    loads: Value,
    /// Current-format synthetic content, history and lifecycle authority.
    /// 当前格式合成内容、历史及生命周期权威。
    knowledge: knowledge::KnowledgeService,
    /// Native VMM authorities keyed by exact action.
    /// 按准确动作索引的原生 VMM 权威数据。
    memory: Value,
    /// Project-owned records kept separately across project selections.
    /// 在项目切换间独立保存的项目记录。
    records: BTreeMap<i64, Value>,
    /// Immutable previews and retained batches for confirmation and restoration.
    /// 用于确认与恢复的不可变预检和保留批次。
    previews: BTreeMap<String, Value>,
    /// Monotonic synthetic identity sequence.
    /// 单调递增的合成身份序列。
    serial: u64,
    /// Native storage version, increased only by committed mutations.
    /// 原生存储版本，仅在已提交变更时增加。
    version: i64,
    /// Synthetic plugin with a settings mount: layout, configuration and credential state.
    /// 带设置挂载的合成插件：版面、配置与凭据状态。
    plugin: Value,
    /// Plugin invocation identities already accepted; each is accepted once.
    /// 已接受的插件调用身份；每个只接受一次。
    invocations: BTreeSet<String>,
    /// Base system prompt layers: built-in text, the user override and per-project overrides.
    /// 基础系统提示词层级：内置正文、用户覆盖及逐项目覆盖。
    system_prompts: Value,
}

/// Gets a required request value and reports malformed input explicitly.
/// 获取必需请求值，并明确报告格式错误的输入。
/// A small fixed project tree for the composer's `@` path list: the root, `src` and `src/app`.
/// 供输入框 `@` 路径列表使用的小型固定项目树：根目录、`src` 与 `src/app`。
fn demo_workspace_listing(params: &Value) -> Result<Value, String> {
    let root = string(params, "/workspacePath")?;
    let directory = params["directoryPath"].as_str().unwrap_or("");
    let names: &[(&str, bool)] = match directory {
        "" => &[("src", true), ("Cargo.toml", false), ("README.md", false)],
        "src" => &[("app", true), ("main.rs", false)],
        "src/app" => &[("mod.rs", false)],
        _ => &[],
    };
    let entries: Vec<Value> = names
        .iter()
        .map(|(name, is_directory)| {
            let relative = if directory.is_empty() {
                (*name).to_string()
            } else {
                format!("{directory}/{name}")
            };
            json!({"name":name,"path":format!("{root}/{relative}"),"relativePath":relative,
                "kind":if *is_directory {"directory"} else {"code"},"isDirectory":is_directory,
                "extension":null,"mimeType":null,"sizeBytes":null,"modifiedAt":null,
                "hidden":false,"hasChildren":is_directory})
        })
        .collect();
    Ok(
        json!({"workspacePath":root,"path":format!("{root}/{directory}"),"relativePath":directory,
        "parentPath":null,"entryCount":entries.len(),"entries":entries,"truncated":false,"notice":null,
        "filters":params["filters"].clone()}),
    )
}

fn required<'a>(value: &'a Value, pointer: &str) -> Result<&'a Value, String> {
    value
        .pointer(pointer)
        .ok_or_else(|| format!("Missing request field: {pointer}"))
}

/// Reads an exact string request field; identifiers are never guessed.
/// 读取准确的字符串请求字段，绝不推测标识。
fn string<'a>(value: &'a Value, pointer: &str) -> Result<&'a str, String> {
    required(value, pointer)?
        .as_str()
        .ok_or_else(|| format!("Expected string: {pointer}"))
}

/// Finds one row using a schema-owned identity field.
/// 使用结构持有的身份字段查找单个条目。
fn row_mut<'a>(catalog: &'a mut Value, field: &str, id: &Value) -> Result<&'a mut Value, String> {
    catalog
        .as_array_mut()
        .ok_or("Expected catalog array")?
        .iter_mut()
        .find(|row| row.get(field) == Some(id))
        .ok_or_else(|| format!("Unknown {field}"))
}

/// Counts the four synthetic record authorities; every demo memory has one vector row.
/// 统计四种合成记录权威数据；每条演示记忆对应一行向量。
fn content_counts(records: &Value) -> Result<Value, String> {
    // Counts.
    // 原生内容计数。
    let mut counts = json!({"contextEvidence":0,"compactBoundaries":0,"vectorRows":0});
    for field in ["sessions", "turns", "memories", "profileNodes"] {
        counts[field] = json!(records[field].as_array().ok_or("Record authority")?.len());
    }
    counts["vectorRows"] = counts["memories"].clone();
    Ok(counts)
}

/// Returns synthetic wall-clock milliseconds for preview expiry and recycle timestamps.
/// 返回预检过期与回收时间戳使用的合成场景时钟毫秒值。
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock follows epoch")
        .as_millis() as u64
}

/// Observes exact session/turn sources in `records` for the validated removal `query`.
/// 根据已验证的移除 `query`，观察 `records` 中准确的会话或回合来源。
/// Returns the declared VmmNativeRemovalImpact at `version` without changing records.
/// 返回 `version` 对应的协议 VmmNativeRemovalImpact，不修改记录。
fn removal_impact(
    records: &Value,
    query: &Value,
    version: i64,
    hot_window: usize,
) -> Result<Value, String> {
    // The closed family and distinct canonical IDs are required by VmmNativeRemovalTarget.
    // VmmNativeRemovalTarget 要求封闭的类别及互不重复的规范标识。
    let kind = string(query, "/selection/kind")?;
    // Ids.
    // 准确身份集合。
    let ids = required(query, "/selection/ids")?
        .as_array()
        .ok_or("Removal ids")?;
    // Seen.
    // 已观察身份去重集合。
    let mut seen = std::collections::BTreeSet::new();
    if !matches!(kind, "sessions" | "turns")
        || ids.is_empty()
        || !ids.iter().all(|id| {
            id.as_str()
                .is_some_and(|id| !id.is_empty() && seen.insert(id))
        })
    {
        return Err("Invalid removal selection".into());
    }
    // Native record IDs are numeric; removal selection IDs are their canonical strings.
    // 原生记录标识为数字；移除选择中的标识为其规范字符串。
    let turns = required(records, "/turns")?
        .as_array()
        .ok_or("Turn authority")?;
    // Selected turns.
    // 来源选择匹配的原生轮次。
    let selected_turns: Vec<_> = turns
        .iter()
        .filter(|turn| {
            if kind == "sessions" {
                ids.contains(&turn["sessionSourceId"])
            } else {
                ids.contains(&json!(turn["turnId"].to_string()))
            }
        })
        .collect();
    // Turn ids.
    // 被批准移除的轮次身份。
    let turn_ids: Vec<_> = selected_turns
        .iter()
        .map(|turn| json!(turn["turnId"].to_string()))
        .collect();
    // Sessions.
    // 所选项目的原生会话。
    let sessions = required(records, "/sessions")?
        .as_array()
        .ok_or("Session authority")?;
    // Session ids.
    // 来源选择匹配的会话身份。
    let session_ids: Vec<_> = sessions
        .iter()
        .filter(|session| {
            if kind == "sessions" {
                ids.contains(&session["sessionSourceId"])
            } else {
                selected_turns
                    .iter()
                    .any(|turn| turn["sessionSourceId"] == session["sessionSourceId"])
            }
        })
        .map(|session| session["sessionSourceId"].clone())
        .collect();
    if (kind == "sessions" && session_ids.len() != ids.len())
        || (kind == "turns" && turn_ids.len() != ids.len())
    {
        return Err("Removal source is missing".into());
    }
    // Session removal also includes direct memories owned by the selected session.
    // 会话移除还包含所选会话拥有的直接记忆。
    let memories = required(records, "/memories")?
        .as_array()
        .ok_or("Memory authority")?;
    // Memory ids.
    // 被批准移除的记忆身份。
    let memory_ids: Vec<_> = memories
        .iter()
        .filter(|memory| {
            (kind == "sessions" && ids.contains(&memory["sessionSourceId"]))
                || turn_ids.contains(&json!(memory["sourceTurnId"].to_string()))
        })
        .map(|memory| json!(memory["memoryId"].to_string()))
        .collect();
    // Pending.
    // 选中来源中的待提炼轮次。
    let pending: Vec<_> = selected_turns
        .iter()
        .filter(|turn| turn["status"] == "pending")
        .map(|turn| json!(turn["turnId"].to_string()))
        .collect();
    // Hot.
    // 选中来源中的热窗口轮次。
    let hot: Vec<_> = turns
        .iter()
        .rev()
        .take(hot_window)
        .map(|turn| json!(turn["turnId"].to_string()))
        .filter(|id| turn_ids.contains(id))
        .collect();
    Ok(
        json!({"query":query,"storageVersion":version,"hotWindowSize":hot_window,
        "sessionSourceIds":session_ids,"turnIds":turn_ids,"pendingTurnIds":pending,
        "hotWindowTurnIds":hot,"memoryIds":memory_ids,"vectorMemoryIds":memory_ids,
        "projectProfileNodeIds":[],"userProfileNodeIds":[],"contextEvidence":[]}),
    )
}

/// Builds collision renames and record summaries from the two exact native project authorities.
/// 从两个准确的原生项目权威数据构建冲突改名及记录摘要。
/// Returns the VmmNativeMigrationPreview at `version`, with deterministic demo identities.
/// 返回 `version` 对应的 VmmNativeMigrationPreview，使用确定性的演示身份。
fn migration_preview(source: &Value, target: &Value, version: i64) -> Result<Value, String> {
    // A project cannot migrate into itself, even when empty.
    // 项目即使为空，也不能迁移到自身。
    let source_id = required(source, "/projectId")?
        .as_i64()
        .ok_or("Source project")?;
    // Target id.
    // 准确目标项目身份。
    let target_id = required(target, "/projectId")?
        .as_i64()
        .ok_or("Target project")?;
    if source_id == target_id {
        return Err("Migration requires distinct projects".into());
    }
    // Source counts.
    // 来源项目的准确内容计数。
    let source_counts = content_counts(source)?;
    // Target counts.
    // 目标项目的准确内容计数。
    let target_counts = content_counts(target)?;
    // After.
    // 迁移后的预检内容计数。
    let mut after = target_counts.clone();
    for (field, count) in source_counts.as_object().ok_or("Counts")? {
        after[field] = json!(
            target_counts[field].as_u64().ok_or("Target count")?
                + count.as_u64().ok_or("Source count")?
        );
    }
    // Reserve names from both projects before allocating deterministic collision names.
    // 分配确定性冲突名称前，保留两个项目已使用的全部名称。
    let source_sessions = required(source, "/sessions")?
        .as_array()
        .ok_or("Source sessions")?;
    // Target sessions.
    // 目标项目的原生会话集合。
    let target_sessions = required(target, "/sessions")?
        .as_array()
        .ok_or("Target sessions")?;
    // Names.
    // 已经保留的名称集合。
    let mut names: std::collections::BTreeSet<String> = source_sessions
        .iter()
        .chain(target_sessions)
        .map(|session| string(session, "/sessionSourceId").map(str::to_string))
        .collect::<Result<_, _>>()?;
    // Renames.
    // 预检冻结的会话改名集合。
    let mut renames = Vec::new();
    for session in source_sessions {
        // From.
        // 预检中的原始会话名称。
        let from = string(session, "/sessionSourceId")?;
        if target_sessions
            .iter()
            .any(|session| session["sessionSourceId"] == from)
        {
            // Sequence.
            // 单调序号。
            let mut sequence = 1;
            loop {
                // To.
                // 预检中的明确迁移名称。
                let to = format!("{from}-migrated-{source_id}-{sequence}");
                if names.insert(to.clone()) {
                    renames.push(json!({"from":from,"to":to}));
                    break;
                }
                sequence += 1;
            }
        }
    }
    // Include every primary source record in the review rather than displaying counts alone.
    // 在审查中包含每条原始来源记录，而不只显示计数。
    let mut records = Vec::new();
    for (field, kind, identity, summary) in [
        ("sessions", "session", "sessionSourceId", "displayTitle"),
        ("turns", "turn", "turnId", "userPreview"),
        ("memories", "memory", "memoryId", "summary"),
        (
            "profileNodes",
            "project_profile",
            "profileNodeId",
            "content",
        ),
    ] {
        for row in required(source, &format!("/{field}"))?
            .as_array()
            .ok_or("Migration records")?
        {
            // Exact entity identity.
            // 准确实体身份。
            let id = required(row, &format!("/{identity}"))?;
            // Record id.
            // 审查记录的规范身份文本。
            let record_id = if kind == "session" {
                id.as_str().ok_or("Session identity")?.to_string()
            } else {
                id.as_i64().ok_or("Record identity")?.to_string()
            };
            records.push(json!({"recordKind":kind,"recordId":record_id,
                "sessionSourceId":row.get("sessionSourceId"),"summary":row.get(summary)}));
        }
    }
    Ok(
        json!({"storageVersion":version,"sourceProjectId":source_id,"targetProjectId":target_id,
        "source":source_counts,"targetBefore":target_counts,"targetAfter":after,
        "sessionRenames":renames,"records":records,
        "previewFingerprint":format!("demo-migration-{source_id}-{target_id}-v{version}")}),
    )
}

impl FakeService {
    /// Loads synthetic, repository-owned DTOs; input takes no machine state.
    /// 加载仓库持有的合成 DTO；输入不涉及本机状态。
    pub fn new() -> Self {
        // Fixture.
        // 仓库持有的合成协议夹具。
        let fixture: Value = serde_json::from_str(include_str!("service-fixtures.json"))
            .expect("valid demo DTO fixtures");
        // Loads.
        // 按准确方法索引的合成加载结果。
        let loads = fixture["loads"].clone();
        // Records.
        // 原生记录集合。
        let records = BTreeMap::from([
            (101, fixture["memory"]["native_memory_records"].clone()),
            (
                102,
                json!({"storageVersion":1,"projectId":102,"sessions":[],"turns":[],"memories":[],"profileNodes":[]}),
            ),
        ]);
        // Current fixture state has no source-directory or corpus compatibility.
        // 当前夹具状态不兼容来源目录或语料。
        let knowledge = knowledge::KnowledgeService::new(&fixture);
        let plugin = plugins::fixture();
        let mut workbench = fixture["workbench"].clone();
        workbench["plugins"]
            .as_array_mut()
            .expect("plugins fixture")
            .push(plugin["summary"].clone());
        Self {
            // Derive initial membership from the same project fixture owned by World.
            // 从 World 持有的同一项目夹具派生初始成员身份。
            project_ids: super::fixtures::projects()
                .into_iter()
                .map(|project| project.id)
                .collect(),
            workbench,
            loads,
            memory: fixture["memory"].clone(),
            records,
            knowledge,

            previews: BTreeMap::new(),
            serial: 1,
            version: 1,
            plugin,
            invocations: BTreeSet::new(),
            system_prompts: json!({
                "builtInText":"You are Vulcan Code, a demo coding assistant.",
                "userText":null,"projectTexts":{}
            }),
        }
    }

    /// Resolves an optional project identity the way the service does: blank means none, and a
    /// named project must be a registered host project.
    /// 按服务方式解析可选项目身份：空白视为无项目，具名项目必须是已登记的宿主项目。
    /// Returns the trimmed project identity, or `None` for the user layer only.
    /// 返回去除空白后的项目身份；仅用户层级时为 `None`。
    fn system_prompt_project(&self, project_id: &Value) -> Result<Option<String>, String> {
        let Some(id) = project_id
            .as_str()
            .map(str::trim)
            .filter(|id| !id.is_empty())
        else {
            return Ok(None);
        };
        if !self.project_ids.contains(id) {
            return Err(format!("unknown project: {id}"));
        }
        Ok(Some(id.to_string()))
    }

    /// Builds the `SystemPromptSettingsSnapshot` for the user layer and an optional project.
    /// 为用户层级与可选项目构建 `SystemPromptSettingsSnapshot`。
    fn system_prompt_snapshot(&self, project_id: &Value) -> Result<Value, String> {
        let project = self.system_prompt_project(project_id)?;
        let user_text = &self.system_prompts["userText"];
        let project_text = project
            .as_deref()
            .and_then(|id| self.system_prompts["projectTexts"].get(id))
            .cloned()
            .unwrap_or(Value::Null);
        // Precedence matches the runtime resolver: project, then user, then built-in.
        // 优先级与运行时解析器一致：项目、用户、内置。
        let effective = if !project_text.is_null() {
            "project"
        } else if !user_text.is_null() {
            "user"
        } else {
            "builtIn"
        };
        Ok(json!({
            "builtInText":self.system_prompts["builtInText"],
            "userPath":"demo/.vulcan/code/prompt/system-prompt.md",
            "userText":user_text,
            "projectId":project,
            "projectPath":project.as_deref().map(|id| format!("demo/{id}/.vulcan/code/prompt/system-prompt.md")),
            "projectText":project_text,
            "effectiveSource":effective,
        }))
    }

    /// Saves (`Some(text)`) or resets (`None`) one override and returns the refreshed snapshot.
    /// 保存（`Some(text)`）或重置（`None`）一个覆盖，并返回刷新后的快照。
    fn write_system_prompt(
        &mut self,
        request: &Value,
        text: Option<&str>,
    ) -> Result<Value, String> {
        let project_id = request.get("projectId").unwrap_or(&Value::Null);
        let text = match text.map(str::trim) {
            Some("") => {
                return Err(
                    "System prompt text is empty; remove the override to use the inherited prompt."
                        .into(),
                );
            }
            Some(text) => json!(text),
            None => Value::Null,
        };
        match string(request, "/scope")? {
            "user" => self.system_prompts["userText"] = text,
            "project" => {
                let id = self
                    .system_prompt_project(project_id)?
                    .ok_or("projectId is required for the project scope")?;
                let texts = self.system_prompts["projectTexts"]
                    .as_object_mut()
                    .ok_or("projectTexts fixture")?;
                if text.is_null() {
                    texts.remove(&id);
                } else {
                    texts.insert(id, text);
                }
            }
            scope => return Err(format!("Unknown system prompt scope: {scope}")),
        }
        self.system_prompt_snapshot(project_id)
    }

    /// Generates a stable synthetic id unrelated to any real service installation.
    /// 生成与任何真实服务安装无关的稳定合成标识。
    fn id(&mut self, prefix: &str) -> String {
        // Exact entity identity.
        // 准确实体身份。
        let id = format!("demo-{prefix}-{}", self.serial);
        self.serial += 1;
        id
    }

    /// Registers one newly added host project for project-scoped mock settings.
    /// 为项目作用域的模拟设置登记一个新增宿主项目。
    /// `id` is the exact project identity created by World; existing state is preserved.
    /// `id` 为 World 创建的准确项目身份；保留既有状态。
    pub fn register_project(&mut self, id: &str) {
        self.project_ids.insert(id.to_string());
    }

    /// Appends a sidebar archive operation to the same workbench catalog the page queries.
    /// 将侧栏归档操作追加到页面查询的同一工作台目录。
    pub fn archive(&mut self, id: &str, title: &str, project: &str) {
        self.workbench["archivedConversations"]
            .as_array_mut()
            .expect("archive fixture")
            .push(json!({"id":id,"title":title,"projectName":project}));
    }

    /// Supplies exact current host ownership before knowledge RPC execution.
    /// 在知识库 RPC 执行前提供精确当前宿主归属。
    /// Parameters: projects and conversations are typed World records. Returns after reconciling live ownership.
    /// 参数：projects 及 conversations 为强类型 World 记录。校正实时归属后返回。
    pub fn sync_knowledge_context(
        &mut self,
        projects: &[crate::model::Project],
        conversations: &[crate::model::ConversationSummary],
    ) {
        self.knowledge.context(projects, conversations);
    }

    /// Routes one call, applying known writes and allowing unmodeled mutations as the demo does.
    /// 分发单次调用，应用已知写入；演示中允许尚未建模的变更。
    pub fn answer(&mut self, call: &ServiceCall) -> Result<Value, String> {
        if call.method == "service_run_vmm_memory_action" {
            return self.vmm(call);
        }
        if call.method == "workbench.snapshot" {
            return Ok(self.workbench.clone());
        }
        if Self::is_plugin_method(call.method) {
            return self.plugin(call.method, &call.params);
        }
        if call.method.contains("knowledge") {
            return self.knowledge.answer(call);
        }
        if let CallKind::Load(_) = call.kind {
            match call.method {
                // The demo keeps one sign-in status and reports it for the requested server.
                // 演示仅保留一个登录状态，并按所请求服务器报告。
                "service_system_prompt_settings" => {
                    return self.system_prompt_snapshot(required(&call.params, "/projectId")?);
                }
                // The demo keeps one session document and reports it for the requested session.
                // 演示只保留一份会话文档，并按所请求的会话报告。
                "service_session_subagent_models" | "service_session_review_prompt" => {
                    let mut snapshot = self.loads[call.method].clone();
                    snapshot["sessionId"] = required(&call.params, "/sessionId")?.clone();
                    return Ok(snapshot);
                }
                "service_project_unattended_default" | "service_project_review_rules" => {
                    let mut policy = self.loads[call.method].clone();
                    policy["projectId"] = required(&call.params, "/projectId")?.clone();
                    return Ok(policy);
                }
                "service_project_permission_judge_policy" => {
                    let mut policy = self.loads[call.method].clone();
                    policy["projectId"] = required(&call.params, "/projectId")?.clone();
                    return Ok(policy);
                }
                // The demo keeps one request test and reports it for the requested endpoint.
                // 演示只保留一次请求测试，并按所请求的端点报告。
                "service_test_permission_judge_edge_request" => {
                    required(&call.params, "/request/input/kind")?;
                    let mut test = self.loads[call.method].clone();
                    test["endpointId"] = required(&call.params, "/request/endpointId")?.clone();
                    return Ok(test);
                }
                // The demo shows one memory tree and reports it for the requested host project.
                // 演示只有一棵记忆树，并按所请求的宿主项目报告。
                "service_load_markdown_memory" => {
                    let mut overview = self.loads[call.method].clone();
                    overview["hostProjectId"] = call.param("/request/projectId").clone();
                    return Ok(overview);
                }
                "workspace.files.list" => return demo_workspace_listing(&call.params),
                "service_mcp_oauth_status" => {
                    let mut status = self.loads["service_mcp_oauth_status"].clone();
                    status["serverId"] = required(&call.params, "/serverId")?.clone();
                    return Ok(status);
                }
                _ => {}
            }
            // Result.
            // 服务结果。
            let result = self
                .loads
                .get(call.method)
                .cloned()
                .ok_or_else(|| format!("No demo loader for {}", call.method))?;
            return Ok(result);
        }
        // Exact RPC parameters.
        // 准确 RPC 参数。
        let params = &call.params;
        match call.method {
            "service_inspect_local_plugin" => {
                return Ok(
                    json!({"pluginId":"demo-installed-plugin","name":"Demo Installed Plugin",
                "contentFingerprint":"demo-plugin-fingerprint","permissions":["filesystem_read"],"installSecretRequirements":[]}),
                );
            }

            "service_save_git_commit_settings" => {
                self.workbench["gitCommit"] = required(params, "/request")?.clone()
            }
            "service_save_provider_retry_settings" => {
                self.workbench["providerRetry"] = required(params, "/request")?.clone()
            }
            "service_save_provider_request_trace_settings" => {
                self.workbench["providerRequestTraceEnabled"] =
                    required(params, "/request/enabled")?.clone();
                // The optional field preserves the other preference for older clients.
                // 可选字段为旧客户端保留另一项偏好。
                if let Some(enabled) = params
                    .pointer("/request/errorsEnabled")
                    .and_then(Value::as_bool)
                {
                    self.workbench["providerErrorTraceEnabled"] = json!(enabled);
                }
            }
            "service_select_workbench_thinking_display_mode" => {
                self.workbench["thinkingDisplayMode"] = required(params, "/mode")?.clone()
            }
            "service_save_tool_collapse_default" => {
                self.workbench["toolCollapseDefaultEnabled"] = required(params, "/enabled")?.clone()
            }
            "service_save_session_title_generation" => {
                self.workbench["sessionTitleGenerationEnabled"] =
                    required(params, "/enabled")?.clone()
            }
            "service_save_tool_chain_settings" => {
                self.workbench["toolChainEnabled"] = required(params, "/enabled")?.clone();
                self.workbench["toolChainMaxSteps"] = required(params, "/maxSteps")?.clone();
            }
            "service_select_webui_locale" => {
                self.workbench["uiLocale"] = required(params, "/locale")?.clone()
            }
            "service_save_public_ai_context_compaction_window_nodes" => {
                self.loads["service_public_ai_settings_snapshot"]["contextCompactionWindowNodes"] =
                    required(params, "/request/nodes")?.clone()
            }
            "service_save_public_ai_new_session_context_window_default" => {
                self.loads["service_public_ai_settings_snapshot"]["newSessionContextCompactionWindowNodeId"] =
                    required(params, "/request/nodeId")?.clone()
            }
            "service_save_public_ai_usage_refresh_settings" => {
                self.loads["service_public_ai_settings_snapshot"]["usageRefreshIntervalSeconds"] =
                    required(params, "/request/intervalSeconds")?.clone()
            }
            // Both writes return the refreshed snapshot, as the service does.
            // 两种写入均与服务一致，返回刷新后的快照。
            "service_save_system_prompt" => {
                let request = required(params, "/request")?;
                let text = string(request, "/text")?.to_string();
                return self.write_system_prompt(request, Some(&text));
            }
            "service_reset_system_prompt" => {
                return self.write_system_prompt(required(params, "/request")?, None);
            }
            "service_save_prompt_output_style" => {
                self.loads["service_output_style_catalog"]["selectedId"] =
                    required(params, "/request/styleId")?.clone()
            }
            // The service returns the saved mode and projects it into the workbench snapshot.
            // 服务返回已保存的模式，并投影到工作台快照。
            "service_save_prompt_answer_mode" => {
                let mode = required(params, "/request/mode")?.clone();
                self.workbench["defaultAnswerMode"] = mode.clone();
                return Ok(mode);
            }
            "service_add_public_ai_connection" => {
                // Exact protocol request.
                // 协议请求文档。
                let request = required(params, "/request")?;
                // Exact entity identity.
                // 准确实体身份。
                let id = self.id("connection");
                // Retain only public metadata; even demo input secrets are consumed once.
                // 仅保留公开元数据；即使演示输入的机密也只消费一次。
                self.loads["service_public_ai_settings_snapshot"]["connections"].as_array_mut().ok_or("connections fixture")?.push(json!({
                    "id":id,"provider_id":required(request,"/providerId")?,"service_entry_id":required(request,"/serviceEntryId")?,
                    "credential_profile_id":required(request,"/credentialProfileId")?,"credential_mode":required(request,"/credentialMode")?,
                    "display_name":required(request,"/displayName")?,"enabled":true,"pending_validation":false,"has_management_key":false}));
            }
            "service_save_public_ai_connection_enabled" => {
                // Current row.
                // 当前条目。
                let row = row_mut(
                    &mut self.loads["service_public_ai_settings_snapshot"]["connections"],
                    "id",
                    required(params, "/request/connectionId")?,
                )?;
                row["enabled"] = required(params, "/request/enabled")?.clone();
            }
            "service_sync_public_ai_connection" => {
                // Current row.
                // 当前条目。
                let row = row_mut(
                    &mut self.loads["service_public_ai_settings_snapshot"]["connections"],
                    "id",
                    required(params, "/connectionId")?,
                )?;
                row["pending_validation"] = json!(false);
            }
            "service_remove_public_ai_connection" => {
                // Exact entity identity.
                // 准确实体身份。
                let id = required(params, "/request/connectionId")?;
                // Provider.
                // 提供方身份或元数据。
                let provider = self.loads["service_public_ai_settings_snapshot"]["connections"]
                    .as_array()
                    .ok_or("connections fixture")?
                    .iter()
                    .find(|connection| connection.get("id") == Some(id))
                    .ok_or("Unknown connection")?["provider_id"]
                    .as_str()
                    .ok_or("Connection provider")?
                    .to_string();
                // A replacement must be a distinct enabled connection of the same provider.
                // 替代默认连接必须为同一提供方下另一条已启用连接。
                if self.loads["service_public_ai_settings_snapshot"]["providerDefaults"]
                    .get(&provider)
                    == Some(id)
                {
                    // Alternatives.
                    // 符合服务准入规则的替代连接。
                    let alternatives: Vec<_> =
                        self.loads["service_public_ai_settings_snapshot"]["connections"]
                            .as_array()
                            .ok_or("Connection fixture")?
                            .iter()
                            .filter(|row| {
                                row["id"] != *id
                                    && row["provider_id"] == provider
                                    && row["enabled"] == true
                                    && row["pending_validation"] == false
                            })
                            .collect();
                    if !alternatives.is_empty()
                        && !params
                            .pointer("/request/replacementDefaultConnectionId")
                            .is_some_and(|replacement| {
                                alternatives.iter().any(|row| row["id"] == *replacement)
                            })
                    {
                        return Err("A valid replacement default is required".into());
                    }
                }
                if self.loads["service_public_ai_settings_snapshot"]["providerDefaults"]
                    .get(&provider)
                    != Some(id)
                    && params
                        .pointer("/request/replacementDefaultConnectionId")
                        .is_some()
                {
                    return Err("Only a current default accepts a replacement".into());
                }
                // Defaults.
                // 提供方默认连接集合。
                let defaults =
                    &mut self.loads["service_public_ai_settings_snapshot"]["providerDefaults"];
                if defaults.get(&provider) == Some(id) {
                    if let Some(replacement) =
                        params.pointer("/request/replacementDefaultConnectionId")
                    {
                        defaults[&provider] = replacement.clone();
                    } else {
                        defaults
                            .as_object_mut()
                            .ok_or("default fixture")?
                            .remove(&provider);
                    }
                }
                self.loads["service_public_ai_settings_snapshot"]["connections"]
                    .as_array_mut()
                    .ok_or("connections fixture")?
                    .retain(|row| row.get("id") != Some(id));
            }
            "service_save_public_ai_default_connection" => {
                self.loads["service_public_ai_settings_snapshot"]["providerDefaults"]
                    [string(params, "/request/providerId")?] =
                    required(params, "/request/connectionId")?.clone()
            }
            "service_save_public_ai_model_enabled" => {
                // Exact protocol request.
                // 协议请求文档。
                let request = required(params, "/request")?;
                // Current operation rows.
                // 当前操作的行集合。
                let rows = self.loads["service_public_ai_settings_snapshot"]["modelAccessRecords"]
                    .as_array_mut()
                    .ok_or("model fixture")?;
                // Models without a record use the default; the first save creates one.
                // 没有记录的模型使用默认值；首次保存时创建记录。
                let index = match rows.iter().position(|row| {
                    row.get("providerId") == request.get("providerId")
                        && row.get("modelId") == request.get("modelId")
                }) {
                    Some(index) => index,
                    None => {
                        rows.push(json!({"providerId":required(request,"/providerId")?,
                            "modelId":required(request,"/modelId")?,"defaultEnabled":true}));
                        rows.len() - 1
                    }
                };
                rows[index]["enabled"] = required(request, "/enabled")?.clone();
                rows[index]["customized"] = json!(true);
            }
            "service_save_public_ai_internal_model_enabled" => {
                // Exact protocol request.
                // 协议请求文档。
                let request = required(params, "/request")?;
                // Matching candidates share one model-level switch across operations.
                // 匹配的候选在各操作间共享同一个模型级开关。
                let mut matched = false;
                for candidate in
                    self.loads["service_public_ai_settings_snapshot"]["internalInferenceCandidates"]
                        .as_array_mut()
                        .ok_or("candidate fixture")?
                        .iter_mut()
                        .filter(|candidate| {
                            candidate.pointer("/route/provider_id") == request.get("providerId")
                                && candidate.pointer("/route/model_id") == request.get("modelId")
                        })
                {
                    candidate["enabled"] = required(request, "/enabled")?.clone();
                    matched = true;
                }
                if !matched {
                    return Err("unknown demo internal model".into());
                }
            }
            "service_save_public_ai_multimodal_understanding_candidate" => {
                // Exact protocol request.
                // 协议请求文档。
                let request = required(params, "/request")?;
                // Admitted provider/model pairs.
                // 已准入的供应商与模型对。
                let records = self.loads["service_public_ai_settings_snapshot"]
                    ["multimodalUnderstandingCandidateModels"]
                    .as_array_mut()
                    .ok_or("candidate fixture")?;
                records.retain(|record| {
                    record.get("providerId") != request.get("providerId")
                        || record.get("modelId") != request.get("modelId")
                });
                if required(request, "/enabled")? == true {
                    records.push(json!({"providerId":required(request,"/providerId")?,
                        "modelId":required(request,"/modelId")?}));
                }
            }
            "service_save_public_ai_provider_extension_target_set" => {
                self.loads["service_public_ai_settings_snapshot"]["providerExtensionTargetSets"]
                    [string(params, "/request/capability")?] =
                    required(params, "/request/selections")?.clone()
            }
            "service_save_public_ai_provider_extension_mode" => {
                self.loads["service_public_ai_settings_snapshot"]["providerExtensionModes"]
                    [string(params, "/request/capability")?] =
                    required(params, "/request/selection")?.clone()
            }
            "service_save_public_ai_provider_tool_selection" => {
                self.loads["service_public_ai_settings_snapshot"]["providerToolSelections"]
                    [string(params, "/request/capability")?] =
                    required(params, "/request/selection")?.clone()
            }
            "service_save_public_ai_provider_tool_capability_enabled" => {
                // Capability.
                // 模型声明的能力。
                let capability = required(params, "/request/capability")?;
                // Capabilities.
                // 已声明能力集合。
                let capabilities = self.loads["service_public_ai_settings_snapshot"]["providerToolEnabledCapabilities"].as_array_mut().ok_or("capabilities fixture")?;
                capabilities.retain(|value| value != capability);
                if required(params, "/request/enabled")? == true {
                    capabilities.push(capability.clone());
                }
            }
            "service_save_public_ai_inference_route" => {
                // Exact protocol request.
                // 协议请求文档。
                let request = required(params, "/request")?;
                // Profile.
                // 声明的消费者或凭据配置。
                let profile = &mut self.loads["service_public_ai_settings_snapshot"]["inferenceProfiles"]
                    [string(request, "/consumerProfileId")?];
                match string(request, "/operation")? {
                    "conversation_respond" => {
                        // Purpose.
                        // 准确背景推理用途。
                        let purpose = string(request, "/purposeId")?;
                        profile["llm_routes"][purpose] = required(request, "/route")?.clone();
                        profile["llm_reasoning_efforts"][purpose] = request
                            .get("reasoningEffort")
                            .cloned()
                            .unwrap_or(Value::Null);
                    }
                    "decision_evaluate" => {
                        profile["decision"] = required(request, "/route")?.clone()
                    }
                    "embedding_create" => {
                        profile["embedding"] = required(request, "/route")?.clone();
                        if let Some(dimension) = request.get("embeddingDimensions") {
                            profile["embedding_dimensions"] = dimension.clone();
                        }
                    }
                    "rerank_documents" => profile["rerank"] = required(request, "/route")?.clone(),
                    _ => return Err("Unknown inference operation".into()),
                }
            }
            "service_save_permission_judge_settings" => {
                // Exact protocol request.
                // 协议请求文档。
                let request = required(params, "/request")?;
                // Settings snapshot fixture.
                // 设置快照夹具。
                let snapshot = &mut self.loads["service_permission_judge_settings"];
                for field in [
                    "state",
                    "backend",
                    "device",
                    "llmMayAllowBlocked",
                    "recordDecisions",
                    "approvalTimeoutSeconds",
                ] {
                    snapshot[field] = required(request, &format!("/{field}"))?.clone();
                }
                snapshot["cuda"]["lockedDirectory"] =
                    required(request, "/cudaLibraryDirectory")?.clone();
            }
            "service_lock_permission_judge_edge_endpoint" => {
                // The demo mirrors the lock in the settings snapshot and the endpoint table, where
                // a locked endpoint is the one requests use.
                // 演示在设置快照与端点表中同步锁定，锁定的端点即请求使用的端点。
                let locked = required(params, "/request/endpointId")?.clone();
                self.loads["service_permission_judge_settings"]["edgeEndpointId"] = locked.clone();
                let table = &mut self.loads["service_permission_judge_edge_endpoints"];
                table["mode"] = json!(if locked.is_null() { "auto" } else { "locked" });
                table["selectedEndpointId"] = if locked.is_null() {
                    json!("us-mini")
                } else {
                    locked.clone()
                };
                table["lockedEndpointId"] = locked;
            }
            "service_save_memory_settings" => {
                // The demo keeps one settings document, also reported inside the overview.
                // 演示保留一份设置文档，并在总览中同样报告。
                let settings = &mut self.loads["service_get_memory_settings"];
                settings["mode"] = required(params, "/request/mode")?.clone();
                settings["budget"] = required(params, "/request/budget")?.clone();
                let settings = settings.clone();
                self.loads["service_load_markdown_memory"]["settings"] = settings.clone();
                return Ok(settings);
            }
            "service_save_unattended_system_default" => {
                // The demo keeps one system text; a project that follows it shows the new text.
                // 演示保留一份系统文本；跟随它的项目显示新文本。
                let text = required(params, "/request/denyPolicy")?.clone();
                self.loads["service_unattended_system_default"]["denyPolicy"] = text.clone();
                self.loads["service_project_unattended_default"]["systemDenyPolicy"] = text;
            }
            "service_save_review_rules_system_default" => {
                // The demo keeps one system rule set; a project that follows it shows the new rules.
                // 演示保留一组系统规则；跟随它的项目显示新规则。
                let rules = required(params, "/request/rules")?.clone();
                self.loads["service_review_rules_system_default"]["rules"] = rules.clone();
                self.loads["service_project_review_rules"]["systemRules"] = rules;
            }
            "service_save_subagent_models" => {
                // The demo keeps one system list; a session that follows it shows the new list.
                // 演示保留一份系统清单；跟随它的会话显示新清单。
                let models = required(params, "/request/models")?.clone();
                self.loads["service_subagent_models"]["models"] = models.clone();
                self.loads["service_session_subagent_models"]["systemModels"] = models;
            }
            "service_save_session_subagent_models" => {
                // The demo keeps one session list; null follows the system list.
                // 演示保留一份会话清单；null 表示跟随系统清单。
                self.loads["service_session_subagent_models"]["models"] =
                    required(params, "/request/models")?.clone();
            }
            "service_start_luaskills_debug_install" => {
                // The demo installs at once and reports the finished task in the status.
                // 演示立即完成安装，并在状态中报告已完成的任务。
                let status = &mut self.loads["service_luaskills_debug_status"];
                let package_root = format!(
                    "/home/demo/.vulcan/code/tools/luaskills-debug/{}.0",
                    status["series"].as_str().ok_or("series")?
                );
                let task = json!({
                    "taskId": "demo-luaskills-debug-install",
                    "state": "succeeded",
                    "phase": "completed",
                    "completedBytes": null,
                    "totalBytes": null,
                    "overallPercent": 100
                });
                status["version"] =
                    json!(format!("{}.0", status["series"].as_str().ok_or("series")?));
                status["compatible"] = json!(true);
                status["executable"] = json!(format!("{package_root}/bin/luaskills-debug"));
                status["runtimeRoot"] = json!(format!("{package_root}/runtime"));
                status["packageRoot"] = json!(package_root);
                status["task"] = task.clone();
                return Ok(task);
            }
            "service_save_rtk_settings" => {
                self.loads["service_rtk_settings"]["settings"] =
                    required(params, "/request/settings")?.clone();
                return Ok(self.loads["service_rtk_settings"].clone());
            }
            "service_clear_rtk_statistics" => {
                // The demo empties every series and keeps the snapshot shape.
                // 演示清空全部序列并保留快照结构。
                let statistics = &mut self.loads["service_rtk_statistics"];
                for series in ["daily", "weekly", "monthly", "byCommand"] {
                    statistics[series] = json!([]);
                }
                for total in [
                    "totalCommands",
                    "totalInput",
                    "totalOutput",
                    "totalSaved",
                    "oversizedExcluded",
                ] {
                    statistics["summary"][total] = json!(0);
                }
                statistics["summary"]["avgSavingsPct"] = json!(0.0);
                return Ok(statistics.clone());
            }
            "service_set_mcp_service_enabled" => {
                self.loads["service_mcp_service_snapshot"]["enabled"] =
                    required(params, "/enabled")?.clone();
            }
            "service_update_mcp_client" => {
                // The demo replaces the client's grants and keeps its token and usage.
                // 演示替换客户端的授权，保留其令牌与使用情况。
                let request = required(params, "/request")?;
                let client = self.loads["service_mcp_service_snapshot"]["clients"]
                    .as_array_mut()
                    .ok_or("MCP clients")?
                    .iter_mut()
                    .find(|client| client.get("id") == request.get("clientId"))
                    .ok_or("Unknown MCP client")?;
                for field in ["name", "enabled", "sessions", "mediaTargets"] {
                    client[field] = required(request, &format!("/{field}"))?.clone();
                }
            }
            "service_delete_mcp_client" => {
                self.loads["service_mcp_service_snapshot"]["clients"]
                    .as_array_mut()
                    .ok_or("MCP clients")?
                    .retain(|client| client.get("id") != params.get("clientId"));
            }
            "service_save_review_prompt_system_default" => {
                // The demo keeps one system prompt; a session that follows it shows the new text.
                // 演示保留一份系统提示词；跟随它的会话显示新文本。
                let prompt = required(params, "/request/prompt")?.clone();
                self.loads["service_review_prompt_system_default"]["prompt"] = prompt.clone();
                self.loads["service_session_review_prompt"]["systemPrompt"] = prompt;
            }
            "service_save_session_review_prompt" => {
                // The demo keeps one session prompt; null follows the system default.
                // 演示保留一份会话提示词；null 表示跟随系统默认值。
                self.loads["service_session_review_prompt"]["prompt"] =
                    required(params, "/request/prompt")?.clone();
            }
            "service_save_project_review_rules" => {
                // The demo keeps one project rule set; null follows the system rules.
                // 演示保留一组项目规则；null 表示跟随系统规则。
                self.loads["service_project_review_rules"]["rules"] =
                    required(params, "/request/rules")?.clone();
            }
            "service_save_project_unattended_default" => {
                // The demo keeps one project text; null follows the system default.
                // 演示保留一份项目文本；null 表示跟随系统默认值。
                self.loads["service_project_unattended_default"]["denyPolicy"] =
                    required(params, "/request/denyPolicy")?.clone();
            }
            "service_save_project_permission_judge_policy" => {
                // Saved choices; omitted categories follow the default.
                // 已保存的选择；未列出的类别跟随默认值。
                let chosen = required(params, "/request/options")?.clone();
                for option in self.loads["service_project_permission_judge_policy"]["options"]
                    .as_array_mut()
                    .ok_or("policy fixture")?
                {
                    option["chosenMode"] = chosen
                        .get(option["key"].as_str().ok_or("policy key")?)
                        .cloned()
                        .unwrap_or(Value::Null);
                }
            }
            "service_save_luaskills_runtime_settings" => {
                self.workbench["luaSkills"]["runtimeSettings"] =
                    required(params, "/settings")?.clone()
            }
            "service_set_luaskill_enabled" => {
                row_mut(
                    &mut self.workbench["luaSkills"]["skills"],
                    "skillId",
                    required(params, "/skillId")?,
                )?["enabled"] = required(params, "/enabled")?.clone()
            }
            "service_set_luaskill_tool_registration_state" => {
                // Skill.
                // 准确技能元数据。
                let skill = row_mut(
                    &mut self.workbench["luaSkills"]["skills"],
                    "skillId",
                    required(params, "/skillId")?,
                )?;
                row_mut(
                    &mut skill["tools"],
                    "localName",
                    required(params, "/localName")?,
                )?["registrationState"] = required(params, "/registrationState")?.clone();
            }
            "service_install_luaskill" => {
                // Exact entity identity.
                // 准确实体身份。
                let id = self.id("lua");
                self.workbench["luaSkills"]["skills"].as_array_mut().ok_or("lua fixture")?.push(json!({
                    "skillId":id,"displayName":required(params,"/source")?,"version":"1.0.0","enabled":true,"systemOwned":false,
                    "managed":true,"sourceType":required(params,"/sourceType")?,"tools":[]}));
            }
            "service_update_luaskill" => {
                row_mut(
                    &mut self.workbench["luaSkills"]["skills"],
                    "skillId",
                    required(params, "/skillId")?,
                )?["version"] = json!("1.0.1")
            }
            "service_uninstall_luaskill" => self.workbench["luaSkills"]["skills"]
                .as_array_mut()
                .ok_or("lua fixture")?
                .retain(|row| row.get("skillId") != params.get("skillId")),
            "service_set_agent_skill_enabled" => {
                row_mut(
                    &mut self.workbench["agentSkills"]["records"],
                    "name",
                    required(params, "/name")?,
                )?["enabled"] = required(params, "/enabled")?.clone()
            }
            "service_set_subagent_definition_enabled" => {
                row_mut(
                    &mut self.workbench["subagentCatalog"]["definitions"],
                    "agentId",
                    required(params, "/agentId")?,
                )?["enabled"] = required(params, "/enabled")?.clone()
            }
            "service_delete_agent_skill" => self.workbench["agentSkills"]["records"]
                .as_array_mut()
                .ok_or("skills fixture")?
                .retain(|row| row.get("name") != params.get("name")),
            "service_delete_subagent_definition" => {
                self.workbench["subagentCatalog"]["definitions"]
                    .as_array_mut()
                    .ok_or("agents fixture")?
                    .retain(|row| row.get("agentId") != params.get("agentId"))
            }
            "service_restore_agent_skill_default" => {
                row_mut(
                    &mut self.workbench["agentSkills"]["records"],
                    "name",
                    required(params, "/name")?,
                )?["enabled"] = json!(true)
            }
            "service_restore_subagent_definition_default" => {
                row_mut(
                    &mut self.workbench["subagentCatalog"]["definitions"],
                    "agentId",
                    required(params, "/agentId")?,
                )?["enabled"] = json!(true)
            }
            "service_set_vulcan_plugin_enabled" => {
                // Current row.
                // 当前条目。
                let row = row_mut(
                    &mut self.workbench["plugins"],
                    "id",
                    required(params, "/pluginId")?,
                )?;
                row["desiredEnabled"] = required(params, "/enabled")?.clone();
                row["enabled"] = required(params, "/enabled")?.clone();
            }
            "service_install_local_plugin" | "service_update_local_plugin" => {
                if string(params, "/contentFingerprint")? != "demo-plugin-fingerprint" {
                    return Err("Plugin preview changed".into());
                }
                // Plugins.
                // 插件元数据集合。
                let plugins = self.workbench["plugins"]
                    .as_array_mut()
                    .ok_or("plugins fixture")?;
                plugins.retain(|row| row["id"] != "demo-installed-plugin");
                plugins.push(json!({"id":"demo-installed-plugin","name":"Demo Installed Plugin","desiredEnabled":true,
                    "enabled":true,"runtimeState":"running","extension":{"origin":"userInstalled"}}));
            }
            "service_uninstall_user_plugin" => self.workbench["plugins"]
                .as_array_mut()
                .ok_or("plugins fixture")?
                .retain(|row| row.get("id") != params.get("pluginId")),
            "service_save_mcp_server_configuration" => {
                // Exact protocol request.
                // 协议请求文档。
                let mut request = required(params, "/request")?.clone();
                scrub_secrets(&mut request);
                // Exact entity identity.
                // 准确实体身份。
                let id = if request["serverId"].is_null() {
                    self.id("mcp")
                } else {
                    string(&request, "/serverId")?.to_string()
                };
                // Servers.
                // MCP 服务器集合。
                let servers = self.loads["service_mcp_runtime_snapshot"]
                    .as_array_mut()
                    .ok_or("mcp fixture")?;
                servers.retain(|server| server["state"]["id"] != id);
                servers.push(json!({"state":{"id":id,"name":request["name"],"status":"connected","source":"user"},"tools":[]}));
                // Configuration.
                // 准确 MCP 配置。
                let configuration = request.as_object_mut().ok_or("MCP request")?;
                configuration.remove("serverId");
                configuration.insert("id".into(), json!(id));
                // Configurations.
                // 工作台 MCP 配置集合。
                let configurations = self.workbench["mcpServers"]
                    .as_array_mut()
                    .ok_or("MCP configurations")?;
                configurations.retain(|server| server["id"] != id);
                configurations.push(request);
            }
            "service_cancel_provider_task" => {
                let task = self.loads["service_list_provider_tasks"]
                    .as_array_mut()
                    .ok_or("provider task fixture")?
                    .iter_mut()
                    .find(|task| task.get("executionId") == params.get("executionId"))
                    .ok_or("Unknown provider task")?;
                task["phase"] = json!("cancellation_requested");
            }
            "service_start_mcp_oauth" => {
                let server_id = string(params, "/serverId")?.to_string();
                self.loads["service_mcp_oauth_status"]["flow"] = json!({
                    "flowId": self.id("oauth"), "phase": "awaiting_callback", "callbackMode": "manual",
                    "authorizationUrl": "https://example.invalid/authorize",
                    "redirectUri": "https://example.invalid/callback", "error": null, "serverId": server_id,
                });
            }
            "service_finish_mcp_oauth" => {
                let status = &mut self.loads["service_mcp_oauth_status"];
                if status["flow"]["flowId"] != *required(params, "/request/flowId")? {
                    return Err("Unknown sign-in flow".into());
                }
                status["flow"]["phase"] = json!("succeeded");
                status["authStatus"] = json!("authenticated");
            }
            "service_cancel_mcp_oauth" => {
                self.loads["service_mcp_oauth_status"]["flow"] = Value::Null;
            }
            "service_remove_mcp_oauth" => {
                let status = &mut self.loads["service_mcp_oauth_status"];
                status["flow"] = Value::Null;
                status["authStatus"] = json!("not_authenticated");
            }
            "service_remove_mcp_server_configuration" => {
                self.loads["service_mcp_runtime_snapshot"]
                    .as_array_mut()
                    .ok_or("mcp fixture")?
                    .retain(|server| server["state"].get("id") != params.get("serverId"));
                self.workbench["mcpServers"]
                    .as_array_mut()
                    .ok_or("MCP configurations")?
                    .retain(|server| server.get("id") != params.get("serverId"));
            }
            "service_delete_archived_conversations" | "service_restore_archived_conversations" => {
                // Ids.
                // 准确身份集合。
                let ids = required(params, "/conversationIds")?
                    .as_array()
                    .ok_or("archive ids")?;
                self.workbench["archivedConversations"]
                    .as_array_mut()
                    .ok_or("archive fixture")?
                    .retain(|row| !ids.contains(&row["id"]));
            }
            _ => {}
        }
        Ok(json!({"ok":true}))
    }

    /// Wraps a native value in the exact VMM tagged result envelope.
    /// 将原生值包装为准确的 VMM 带标签结果信封。
    fn reply(action: &str, value: Value) -> Value {
        // These tag names are declared by VmmMemoryActionResponse, not action-name aliases.
        // 这些标签名由 VmmMemoryActionResponse 声明，不是动作名别名。
        let kind = match action {
            "native_project_clear_preview" => "native_clear_preview",
            "native_project_migration_preview" => "native_migration_preview",
            "native_project_clear" => "native_clear",
            "native_project_migrate" => "native_migration",
            "native_removal_recycle" => "native_removal",
            "native_resource_read" | "native_resource_write" | "native_resource_reset" => {
                "native_resource_file"
            }
            "native_recycle_restore" | "native_recycle_purge" => "native_recycle_batch",
            "native_runtime_policy_update"
            | "native_retrieval_policy_update"
            | "native_retention_settings_update"
            | "native_text_policy_update"
            | "native_profile_policy_update" => "native_runtime_policy",
            "native_resource_settings_update" => "native_resource_catalog",
            "native_project_create_and_bind"
            | "native_project_rename"
            | "native_project_delete" => "native_project_catalog",
            _ => action,
        };
        json!({"data":{"kind":kind,"value":value}})
    }

    /// Derives synthetic active/recycled counts and policy observations from the owned authority.
    /// 从持有的权威数据推导合成活动、回收计数及策略观察结果。
    /// Returns active and recycled memory counts, or an error for malformed synthetic records.
    /// 返回活动与回收记忆数量；合成记录格式错误时返回错误。
    fn refresh_native_observations(&mut self) -> Result<(u64, u64), String> {
        // Demo record authorities contain only active memories; removal moves them to retained batches.
        // 演示记录权威数据仅含活动记忆；移除会将其移动到保留批次。
        let active = self.records.values().try_fold(0_u64, |count, records| {
            records["memories"]
                .as_array()
                .map(|memories| count + memories.len() as u64)
                .ok_or("Memory authority")
        })?;
        // Batch metadata is published together with the retained source records.
        // 批次元数据与保留的来源记录同时发布。
        let batches = self.memory["native_recycle_bin"]["batches"]
            .as_array()
            .ok_or("Recycle authority")?;
        // Count only memory rows, excluding turn and profile counts from the same batch.
        // 仅统计记忆行，排除同一批次的轮次及画像计数。
        let recycled = batches.iter().try_fold(0_u64, |count, batch| {
            batch["memoryCount"]
                .as_u64()
                .map(|memories| count + memories)
                .ok_or("Recycle memory count")
        })?;
        // Retained batch count is independent from each batch's memory count.
        // 保留批次数量独立于每个批次的记忆数量。
        let batch_count = batches.len();
        // The synthetic vector fixture declares every retained and active vector compatible.
        // 合成向量夹具声明全部保留及活动向量均兼容当前代次。
        self.memory["native_vector_status"]["activeMemoryCount"] = json!(active);
        self.memory["native_vector_status"]["currentMemoryCount"] = json!(active);
        self.memory["native_vector_status"]["recycleBinMemoryCount"] = json!(recycled);
        self.memory["native_vector_status"]["currentRecycleBinMemoryCount"] = json!(recycled);
        // Demo publication maintains one lexical document per active memory; this is no integrity proof.
        // 演示发布为每条活动记忆维护一份词法文档；该计数不能证明索引完整性。
        self.memory["native_lexical_status"]["activeMemoryCount"] = json!(active);
        self.memory["native_lexical_status"]["documentCount"] = json!(active);
        self.memory["native_lexical_status"]["indexedRowCount"] = json!(active);
        self.memory["native_retention_status"]["recycleBatchCount"] = json!(batch_count);
        // The retention screen observes the exact policy saved through runtime-policy RPCs.
        // 保留页面观察通过运行时策略 RPC 保存的准确策略。
        let policy = self.memory["native_runtime_policy"]["retention"].clone();
        self.memory["native_retention_status"]["enabled"] = required(&policy, "/enabled")?.clone();
        self.memory["native_retention_status"]["intervalMinutes"] =
            required(&policy, "/scanIntervalMinutes")?.clone();
        self.memory["native_retention_status"]["profileExpiryIntervalSeconds"] =
            required(&policy, "/profileExpiryIntervalSeconds")?.clone();
        self.memory["native_retention_status"]["policy"] = policy;
        Ok((active, recycled))
    }

    /// Runs one native VMM action, retaining immutable preview parameters until confirmation.
    /// 执行单个原生 VMM 动作，将不可变预检参数保留到确认。
    /// `call` contains the exact native action envelope produced by the public request builder.
    /// `call` 包含公开请求构建器产出的准确原生动作信封。
    /// Returns the tagged synthetic response or an explicit contract/confirmation failure.
    /// 返回带标签的合成响应，或明确的协议及确认失败。
    fn vmm(&mut self, call: &ServiceCall) -> Result<Value, String> {
        // Action.
        // 已声明动作。
        let action = string(&call.params, "/request/action")?;
        // Payload.
        // 已声明载荷。
        let payload = required(&call.params, "/request/payload")?;
        // Refresh every observation from owned synthetic records before answering the request.
        // 响应请求前，从持有的合成记录刷新全部观察结果。
        let (active, recycled) = self.refresh_native_observations()?;
        if let CallKind::Load(_) = call.kind {
            // Value.
            // 当前协议值。
            let mut value = if action == "native_memory_records" {
                // Exact entity identity.
                // 准确实体身份。
                let id = required(payload, "/vmmProjectId")?
                    .as_i64()
                    .ok_or("VMM project id")?;
                self.records
                    .get(&id)
                    .cloned()
                    .ok_or("Unknown native project")?
            } else {
                self.memory
                    .get(action)
                    .cloned()
                    .ok_or("Unknown VMM loader")?
            };
            if action == "native_profile_records" {
                value["projectId"] = required(payload, "/vmmProjectId")?.clone();
                value["target"] = required(payload, "/target")?.clone();
                if value["target"] == "project" {
                    // Project profiles belong to the selected native record authority.
                    // 项目画像属于所选原生记录权威数据。
                    let id = value["projectId"].as_i64().ok_or("Profile project")?;
                    value["nodes"] =
                        self.records.get(&id).ok_or("Unknown profile project")?["profileNodes"]
                            .clone();
                    value["counts"] =
                        json!({"active":0,"inactive":0,"pending":0,"expired":0,"scheduled":0});
                    for node in value["nodes"].as_array().ok_or("Profile nodes")?.clone() {
                        // State.
                        // 当前生命周期状态。
                        let state = string(&node, "/effectiveState")?;
                        // Count.
                        // 当前计数。
                        let count = value["counts"]
                            .get(state)
                            .and_then(Value::as_u64)
                            .ok_or("Profile state")?;
                        value["counts"][state] = json!(count + 1);
                    }
                } else if value["target"] != "user" {
                    return Err("Unknown profile target".into());
                }
            }
            if matches!(
                action,
                "native_memory_records"
                    | "native_profile_records"
                    | "native_lexical_status"
                    | "native_recycle_bin"
            ) {
                value["storageVersion"] = json!(self.version);
            }
            if action == "native_resource_read" && required(payload, "/path")? != &value["path"] {
                return Err("Unknown resource path".into());
            }
            return Ok(Self::reply(action, value));
        }
        if let CallKind::Preview(_) = call.kind {
            // Value.
            // 当前协议值。
            let value = match action {
                "native_project_clear_preview" => {
                    // Exact entity identity.
                    // 准确实体身份。
                    let id = required(payload, "/vmmProjectId")?
                        .as_i64()
                        .ok_or("Native project id")?;
                    json!({"storageVersion":self.version,"projectId":id,
                        "removed":content_counts(self.records.get(&id).ok_or("Unknown native project")?)?})
                }
                "native_project_delete_preview" => {
                    // Project.
                    // 所选项目数据。
                    let project = self.memory["native_project_catalog"]["projects"]
                        .as_array()
                        .ok_or("project fixture")?
                        .iter()
                        .find(|project| project.get("projectId") == payload.get("vmmProjectId"))
                        .ok_or("Unknown native project")?;
                    // Exact entity identity.
                    // 准确实体身份。
                    let id = required(project, "/projectId")?
                        .as_i64()
                        .ok_or("Native project id")?;
                    json!({"storageVersion":self.version,"project":project,
                        "content":content_counts(self.records.get(&id).ok_or("Unknown native project")?)?})
                }
                "native_project_migration_preview" => {
                    // Source id.
                    // 准确来源项目身份。
                    let source_id = required(payload, "/sourceProjectId")?
                        .as_i64()
                        .ok_or("Source project")?;
                    // Target id.
                    // 准确目标项目身份。
                    let target_id = required(payload, "/targetProjectId")?
                        .as_i64()
                        .ok_or("Target project")?;
                    migration_preview(
                        self.records.get(&source_id).ok_or("Unknown source")?,
                        self.records.get(&target_id).ok_or("Unknown target")?,
                        self.version,
                    )?
                }
                "native_removal_preview" => {
                    // Inspect the selected authority and freeze its full source impact.
                    // 检查所选权威数据，并冻结其完整来源影响。
                    let id = required(payload, "/vmmProjectId")?
                        .as_i64()
                        .ok_or("Removal project")?;
                    // Hot window.
                    // 原生有效热窗口大小。
                    let hot_window =
                        self.memory["native_runtime_policy"]["retention"]["turnHotWindowSize"]
                            .as_u64()
                            .ok_or("Hot window fixture")? as usize;
                    // Impact.
                    // 预检冻结的来源影响。
                    let impact = removal_impact(
                        self.records.get(&id).ok_or("Unknown removal project")?,
                        payload,
                        self.version,
                        hot_window,
                    )?;
                    json!({"previewId":self.id("removal"),"impact":impact,
                        "trashRetentionMs":self.memory["native_runtime_policy"]["retention"]["trashRetentionMs"]})
                }
                "native_retention_preview" => {
                    // The no-op demo retention plan still carries a real five-minute expiry.
                    // 演示保留计划不清理数据，但仍携带真实的五分钟有效期。
                    let evaluated = now_ms();
                    json!({"previewId":self.id("retention"),"storageVersion":self.version,
                        "evaluatedAtUnixMs":evaluated,"expiresAtUnixMs":evaluated + 300000,"projects":[]})
                }
                "native_vector_rebuild_preview" => {
                    json!({"storageVersion":self.version,"targetVectorSpaceId":"demo-vector",
                    "embeddingDimension":384,"activeMemoryCount":active,"recycleBinMemoryCount":recycled,
                    "rebuildMemoryCount":0,"activeRebuildMemoryCount":0,"recycleBinRebuildMemoryCount":0,
                    "affectedProjectCount":0,"batchCount":0,"previewFingerprint":"demo-vector-fingerprint",
                    "adoptableMemoryCount":0})
                }
                _ => return Err("Unknown VMM preview".into()),
            };
            // Key.
            // 当前结构持有的键。
            let key = if let Some(id) = value.get("previewId").and_then(Value::as_str) {
                id.to_string()
            } else {
                string(&call.params, "/request/commandId")?.to_string()
            };
            self.previews.insert(key, value.clone());
            return Ok(Self::reply(action, value));
        }
        match action {
            "native_project_rename" => {
                row_mut(
                    &mut self.memory["native_project_catalog"]["projects"],
                    "projectId",
                    required(payload, "/vmmProjectId")?,
                )?["name"] = required(payload, "/name")?.clone()
            }
            "native_project_create_and_bind" => {
                // Exact entity identity.
                // 准确实体身份。
                let id = 1000 + self.serial as i64;
                self.serial += 1;
                self.memory["native_project_catalog"]["projects"].as_array_mut().ok_or("projects fixture")?.push(json!({
                    "projectId":id,"name":required(payload,"/projectName")?,"tag":format!("demo-{id}"),"protected":false,
                    "ownerVulcanProjectIds":[required(payload,"/vulcanProjectId")?],"linkedVulcanProjectIds":[]}));
                self.records.insert(id,json!({"projectId":id,"storageVersion":self.version,"sessions":[],"turns":[],"memories":[],"profileNodes":[]}));
                self.memory["native_project_catalog"]["bindings"].as_array_mut().ok_or("Binding fixture")?
                    .push(json!({"vulcanProjectId":required(payload,"/vulcanProjectId")?,"ownerProjectId":id,"linkedProjectIds":[]}));
            }
            "native_runtime_policy_update"
            | "native_retrieval_policy_update"
            | "native_retention_settings_update"
            | "native_text_policy_update"
            | "native_profile_policy_update" => {
                // Group.
                // 当前配置组。
                let group = match action {
                    "native_runtime_policy_update" => "recall",
                    "native_retrieval_policy_update" => "retrieval",
                    "native_retention_settings_update" => "retention",
                    "native_text_policy_update" => "text",
                    _ => "profiles",
                };
                self.memory["native_runtime_policy"][group] = payload.clone();
            }
            "native_resource_settings_update" => {
                self.memory["native_resource_catalog"]["settings"] = payload.clone()
            }
            "native_resource_write" => {
                if required(payload, "/confirmedPurpose")? != &json!(true)
                    || required(payload, "/confirmedModification")? != &json!(true)
                {
                    return Err("Resource write needs consent".into());
                }
                // File.
                // 当前文件数据。
                let file = &mut self.memory["native_resource_read"];
                if required(payload, "/path")? != &file["path"] {
                    return Err("Unknown resource path".into());
                }
                file["effectiveText"] = required(payload, "/content")?.clone();
                file["userOverrideText"] = file["effectiveText"].clone();
                file["hasUserOverride"] = json!(true);
                self.memory["native_resource_catalog"]["files"][0]["hasUserOverride"] = json!(true);
            }
            "native_resource_reset" => {
                // File.
                // 当前文件数据。
                let file = &mut self.memory["native_resource_read"];
                if required(payload, "/path")? != &file["path"] {
                    return Err("Unknown resource path".into());
                }
                file["effectiveText"] = file["builtInText"].clone();
                file["userOverrideText"] = Value::Null;
                file["hasUserOverride"] = json!(false);
                self.memory["native_resource_catalog"]["files"][0]["hasUserOverride"] =
                    json!(false);
            }
            "native_project_clear" | "native_project_delete" | "native_project_migrate" => {
                if required(payload, "/expectedStorageVersion")?.as_i64() != Some(self.version) {
                    return Err("VMM preview is stale".into());
                }
                // Command.
                // 准确原生命令身份。
                let command = string(&call.params, "/request/commandId")?;
                // A command can execute only the exact project targets in its stored preview.
                // 命令仅可执行其已保存预检中的准确项目目标。
                let reviewed = self.previews.get(command).ok_or("VMM preview is missing")?;
                if reviewed["storageVersion"].as_i64() != Some(self.version) {
                    return Err("VMM preview is stale".into());
                }
                if action == "native_project_migrate" {
                    if string(payload, "/expectedPreviewFingerprint")?
                        != string(reviewed, "/previewFingerprint")?
                    {
                        return Err("Migration targets changed".into());
                    }
                    // Source.
                    // 当前来源。
                    let source = required(payload, "/sourceProjectId")?
                        .as_i64()
                        .ok_or("source project")?;
                    // Target.
                    // 已声明目标。
                    let target = required(payload, "/targetProjectId")?
                        .as_i64()
                        .ok_or("target project")?;
                    if reviewed["sourceProjectId"] != source
                        || reviewed["targetProjectId"] != target
                    {
                        return Err("Migration targets changed".into());
                    }
                    // Replay exactly the collision renames shown before consent.
                    // 准确重放同意前展示的冲突改名。
                    let renames = required(reviewed, "/sessionRenames")?
                        .as_array()
                        .ok_or("Session renames")?;
                    if !renames.is_empty()
                        && required(payload, "/acceptSessionRenames")? != &json!(true)
                    {
                        return Err("Session renames need consent".into());
                    }
                    // Moved.
                    // 来源项目已批准移动的记录。
                    let mut moved = self.records.get(&source).cloned().ok_or("unknown source")?;
                    for field in ["sessions", "turns", "memories"] {
                        for row in moved[field].as_array_mut().ok_or("Migration authority")? {
                            if let Some(rename) = renames
                                .iter()
                                .find(|rename| rename["from"] == row["sessionSourceId"])
                            {
                                row["sessionSourceId"] = rename["to"].clone();
                            }
                        }
                    }
                    // Destination.
                    // 准确目标项目记录。
                    let destination = self.records.get_mut(&target).ok_or("unknown target")?;
                    for key in ["sessions", "turns", "memories", "profileNodes"] {
                        destination[key]
                            .as_array_mut()
                            .ok_or("record fixture")?
                            .extend(
                                moved[key]
                                    .as_array()
                                    .ok_or("record fixture")?
                                    .iter()
                                    .cloned(),
                            );
                    }
                    for key in ["sessions", "turns", "memories", "profileNodes"] {
                        self.records.get_mut(&source).ok_or("unknown source")?[key] = json!([]);
                    }
                } else {
                    // Exact entity identity.
                    // 准确实体身份。
                    let id = required(payload, "/vmmProjectId")?
                        .as_i64()
                        .ok_or("native project")?;
                    if action == "native_project_clear" && reviewed["projectId"] != id {
                        return Err("Clear target changed".into());
                    }
                    if action == "native_project_delete" {
                        // Project.
                        // 所选项目数据。
                        let project = &reviewed["project"];
                        if project["projectId"] != id || project["protected"] != false {
                            return Err("Delete target is protected or changed".into());
                        }
                        // Bound.
                        // 是否存在已观察绑定。
                        let bound = !required(project, "/ownerVulcanProjectIds")?
                            .as_array()
                            .ok_or("Owners")?
                            .is_empty()
                            || !required(project, "/linkedVulcanProjectIds")?
                                .as_array()
                                .ok_or("Links")?
                                .is_empty();
                        if bound && required(payload, "/confirmDetachBindings")? != &json!(true) {
                            return Err("Binding detachment needs consent".into());
                        }
                    }
                    if action == "native_project_clear" {
                        // Records.
                        // 原生记录集合。
                        let records = self.records.get_mut(&id).ok_or("unknown project")?;
                        for key in ["sessions", "turns", "memories", "profileNodes"] {
                            records[key] = json!([]);
                        }
                    } else {
                        self.memory["native_project_catalog"]["projects"]
                            .as_array_mut()
                            .ok_or("project fixture")?
                            .retain(|project| project["projectId"] != id);
                        self.records.remove(&id);
                        // Bindings.
                        // 权威绑定集合。
                        let bindings = self.memory["native_project_catalog"]["bindings"]
                            .as_array_mut()
                            .ok_or("Binding fixture")?;
                        bindings.retain(|binding| binding["ownerProjectId"] != id);
                        for binding in bindings {
                            binding["linkedProjectIds"]
                                .as_array_mut()
                                .ok_or("Linked projects")?
                                .retain(|linked| linked != &json!(id));
                        }
                    }
                }
                self.previews.remove(command);
            }
            "native_removal_recycle" => {
                if required(payload, "/confirmed")? != &json!(true) {
                    return Err("Removal needs consent".into());
                }
                // The immutable preview owns the selection; replacement targets are never accepted.
                // 不可变预检持有所选目标，绝不接受替换目标。
                let preview_id = string(payload, "/previewId")?;
                // Preview.
                // 只读预检。
                let preview = self
                    .previews
                    .get(preview_id)
                    .cloned()
                    .ok_or("Unknown removal preview")?;
                // Impact.
                // 预检冻结的来源影响。
                let impact = required(&preview, "/impact")?;
                if required(impact, "/storageVersion")?.as_i64() != Some(self.version) {
                    return Err("Removal preview is stale".into());
                }
                // Exact entity identity.
                // 准确实体身份。
                let id = required(impact, "/query/vmmProjectId")?
                    .as_i64()
                    .ok_or("Removal project")?;
                // Batch.
                // 回收批次身份。
                let batch = self.id("batch");
                // Records.
                // 原生记录集合。
                let records = self.records.get_mut(&id).ok_or("Unknown removal project")?;
                // Selection.
                // 已批准选择。
                let selection = required(impact, "/query/selection")?;
                // Retain only moved content, so restoring cannot roll back newer active records.
                // 仅保留已移动内容，防止恢复操作回滚较新的活动记录。
                let mut retained = json!({"projectId":id,"sessions":[],"turns":[],"memories":[],"profileNodes":[]});
                for (field, identity, approved) in [
                    ("sessions", "sessionSourceId", "sessionSourceIds"),
                    ("turns", "turnId", "turnIds"),
                    ("memories", "memoryId", "memoryIds"),
                ] {
                    if field == "sessions" && selection["kind"] != "sessions" {
                        continue;
                    }
                    // Ids.
                    // 准确身份集合。
                    let ids = required(impact, &format!("/{approved}"))?
                        .as_array()
                        .ok_or("Approved sources")?;
                    // Current operation rows.
                    // 当前操作的行集合。
                    let rows = records[field].as_array_mut().ok_or("Record authority")?;
                    // Selected.
                    // 当前选择。
                    let selected = |row: &Value| {
                        if field == "sessions" {
                            ids.contains(&row[identity])
                        } else {
                            ids.contains(&json!(row[identity].to_string()))
                        }
                    };
                    retained[field] =
                        Value::Array(rows.iter().filter(|row| selected(row)).cloned().collect());
                    rows.retain(|row| !selected(row));
                }
                // Recycled.
                // 实际回收时间。
                let recycled = now_ms();
                self.memory["native_recycle_bin"]["batches"].as_array_mut().ok_or("Recycle fixture")?.push(json!({
                    "batchId":batch,"projectId":id,"reason":"manual","manualSelection":selection,
                    "recycledAtUnixMs":recycled,"expiresAtUnixMs":recycled + required(&preview,"/trashRetentionMs")?.as_u64().ok_or("Trash retention")?,
                    "turnCount":retained["turns"].as_array().ok_or("Retained turns")?.len(),
                    "memoryCount":retained["memories"].as_array().ok_or("Retained memories")?.len(),
                    "profileNodeCount":0,"contextEvidenceCount":0}));
                self.previews.remove(preview_id);
                self.previews.insert(batch, retained);
            }
            "native_recycle_restore" | "native_recycle_purge" => {
                if required(payload, "/storageVersion")?.as_i64() != Some(self.version) {
                    return Err("Recycle bin changed".into());
                }
                // Batch.
                // 回收批次身份。
                let batch = string(payload, "/batchId")?;
                // Retained.
                // 已移动且可恢复的原始记录。
                let retained = self
                    .previews
                    .get(batch)
                    .cloned()
                    .ok_or("Unknown recycle batch")?;
                if action == "native_recycle_restore" {
                    // Exact entity identity.
                    // 准确实体身份。
                    let id = required(&retained, "/projectId")?
                        .as_i64()
                        .ok_or("Batch project")?;
                    // Records.
                    // 原生记录集合。
                    let records = self.records.get_mut(&id).ok_or("Unknown restore project")?;
                    // Check all identities before merging, keeping restoration atomic on a collision.
                    // 合并前检查全部身份，发生冲突时保持恢复原子性。
                    for (field, identity) in [
                        ("sessions", "sessionSourceId"),
                        ("turns", "turnId"),
                        ("memories", "memoryId"),
                    ] {
                        // Current operation rows.
                        // 当前操作的行集合。
                        let rows = records[field].as_array().ok_or("Record authority")?;
                        if retained[field]
                            .as_array()
                            .ok_or("Retained authority")?
                            .iter()
                            .any(|row| rows.iter().any(|active| active[identity] == row[identity]))
                        {
                            return Err("Restore identity collision".into());
                        }
                    }
                    for field in ["sessions", "turns", "memories"] {
                        records[field]
                            .as_array_mut()
                            .ok_or("Record authority")?
                            .extend(
                                retained[field]
                                    .as_array()
                                    .ok_or("Retained authority")?
                                    .iter()
                                    .cloned(),
                            );
                    }
                }
                self.previews.remove(batch);
                self.memory["native_recycle_bin"]["batches"]
                    .as_array_mut()
                    .ok_or("Recycle fixture")?
                    .retain(|row| row["batchId"] != batch);
            }
            "native_vector_rebuild" => {
                if required(payload, "/confirmed")? != &json!(true) {
                    return Err("Vector rebuild needs consent".into());
                }
                if required(payload, "/expectedStorageVersion")?.as_i64() != Some(self.version)
                    || string(payload, "/expectedPreviewFingerprint")? != "demo-vector-fingerprint"
                {
                    return Err("Vector preview is stale".into());
                }
                self.memory["native_vector_status"]["rebuildRequiredMemoryCount"] = json!(0);
            }
            "native_vector_adoption" => {
                if required(payload, "/confirmed")? != &json!(true) {
                    return Err("Vector adoption needs consent".into());
                }
                if required(payload, "/expectedStorageVersion")?.as_i64() != Some(self.version)
                    || string(payload, "/expectedPreviewFingerprint")? != "demo-vector-fingerprint"
                {
                    return Err("Vector preview is stale".into());
                }
                self.memory["native_vector_status"]["rebuildRequiredMemoryCount"] = json!(0);
            }
            "native_lexical_rebuild" => {
                if required(payload, "/confirmed")? != &json!(true) {
                    return Err("Lexical rebuild needs consent".into());
                }
                if required(payload, "/expectedStorageVersion")?.as_i64() != Some(self.version) {
                    return Err("Lexical snapshot is stale".into());
                }
                self.memory["native_lexical_status"]["indexedRowCount"] =
                    self.memory["native_lexical_status"]["activeMemoryCount"].clone();
            }
            "native_retention_run" => {
                if required(payload, "/confirmed")? != &json!(true) {
                    return Err("Retention needs consent".into());
                }
                // Exact entity identity.
                // 准确实体身份。
                let id = string(payload, "/previewId")?;
                // Plan.
                // 已审查操作计划。
                let plan = self.previews.get(id).ok_or("Unknown retention preview")?;
                if required(plan, "/storageVersion")?.as_i64() != Some(self.version)
                    || required(plan, "/expiresAtUnixMs")?
                        .as_u64()
                        .ok_or("Retention expiry")?
                        < now_ms()
                {
                    return Err("Retention preview is stale".into());
                }
                self.previews.remove(id);
                self.memory["native_retention_status"]["lastResult"] =
                    json!({"recycledMemories":0,"recycledTurns":0,"projectFailures":[]})
            }
            _ => {}
        }
        if matches!(
            action,
            "native_project_create_and_bind"
                | "native_project_rename"
                | "native_project_clear"
                | "native_project_delete"
                | "native_project_migrate"
                | "native_removal_recycle"
                | "native_recycle_restore"
                | "native_recycle_purge"
                | "native_retention_run"
        ) {
            self.version += 1;
        }
        Ok(Self::reply(action, json!({"storageVersion":self.version})))
    }
}
