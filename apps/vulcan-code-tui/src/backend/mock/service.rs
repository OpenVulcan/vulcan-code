// Deterministic in-process service for the same RPC contracts as the real backend.
// 与真实后端共用 RPC 协议的确定性进程内服务。

use crate::service_api::{CallKind, ServiceCall, scrub_secrets};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[cfg(test)]
#[path = "service_tests.rs"]
mod tests;

/// Owns synthetic service DTOs; known mutations change the authority queried by later loaders.
/// 持有合成服务 DTO；已知变更修改后续加载器查询的权威状态。
pub struct FakeService {
    /// System-facing workbench fields, containing no real credentials.
    /// 面向系统页面的工作台字段，不包含真实凭据。
    pub workbench: Value,
    /// Raw loader fixtures keyed by exact RPC method.
    /// 按准确 RPC 方法索引的原始加载器夹具。
    loads: Value,
    /// Independent library defaults survive deletion of every catalog row.
    /// 独立知识库默认值在全部目录条目删除后仍然保留。
    library_defaults: Value,
    /// Sources, jobs and search rows owned by exact library identities.
    /// 由准确知识库标识持有的来源、任务及搜索条目。
    knowledge: BTreeMap<String, Value>,
    /// Settings, jobs and plans owned by exact host project identities.
    /// 由准确宿主项目标识持有的设置、任务及计划。
    vce_projects: BTreeMap<String, Value>,
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
}

/// Exact synthetic import key exposed by the demo preflight.
/// 演示预检公开的准确合成导入键。
const DEMO_IMPORT_FILE_KEY: &str = "demo/readme.md";
/// Relative managed path belongs to a separate documented preview field.
/// 相对托管路径属于独立的文档定义预检字段。
const DEMO_IMPORT_RELATIVE_PATH: &str = "readme.md";

/// Builds the mock's stable root-identity/path key from an exact managed document summary.
/// 从准确托管文档摘要构建模拟的稳定根身份、路径键。
/// `entry` owns document.sourceId and relativePath; returns the distinct confirmation key.
/// `entry` 持有 document.sourceId 及 relativePath；返回唯一确认键。
fn display_file_key(entry: &Value) -> Result<String, String> {
    Ok(format!(
        "{}:{}",
        string(entry, "/document/sourceId")?,
        string(entry, "/document/relativePath")?
    ))
}

/// Gets a required request value and reports malformed input explicitly.
/// 获取必需请求值，并明确报告格式错误的输入。
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

/// Recursively merges a protocol-defined partial settings document.
/// 递归合并协议定义的部分设置文档。
fn merge(destination: &mut Value, update: &Value) {
    if let (Some(destination), Some(update)) = (destination.as_object_mut(), update.as_object()) {
        for (key, value) in update {
            match destination.get_mut(key) {
                Some(existing) if existing.is_object() && value.is_object() => {
                    merge(existing, value)
                }
                _ => {
                    destination.insert(key.clone(), value.clone());
                }
            }
        }
    }
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
        // Knowledge.
        // 逐知识库独立的合成状态。
        let knowledge = BTreeMap::from([(
            "demo-library".into(),
            json!({
                "sources":loads["service_list_knowledge_sources"],"job":loads["service_get_knowledge_import_job"],
                "integrity":loads["service_get_knowledge_index_integrity"],"search":loads["service_search_knowledge_library"],
                "documents":fixture["knowledgeDocuments"],
            }),
        )]);
        // Vce projects.
        // 逐宿主项目独立的合成索引状态。
        let mut vce_projects = BTreeMap::new();
        for project in ["p-tui", "p-core", "p-lua"] {
            // Overview.
            // 所选项目权威概览。
            let mut overview = loads["service_get_vce_project_overview"].clone();
            overview["status"]["projectId"] = json!(project);
            vce_projects.insert(
                project.into(),
                json!({"overview":overview,"job":null,"plan":null,"jobs":{},"commands":{}}),
            );
        }
        Self {
            workbench: fixture["workbench"].clone(),
            library_defaults: loads["service_list_knowledge_libraries"][0].clone(),
            loads,
            memory: fixture["memory"].clone(),
            records,
            knowledge,
            vce_projects,
            previews: BTreeMap::new(),
            serial: 1,
            version: 1,
        }
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

    /// Registers one newly added host project with independent uninitialized VCE state.
    /// 为一个新增宿主项目登记独立、尚未初始化的 VCE 状态。
    /// `id` is the exact project identity created by World; existing state is preserved.
    /// `id` 为 World 创建的准确项目身份；保留既有状态。
    pub fn register_project(&mut self, id: &str) {
        if self.vce_projects.contains_key(id) {
            return;
        }
        // Defaults come from the declared fixture, while all index-owned counts start empty.
        // 默认设置来自声明的夹具，所有索引持有的计数从空值开始。
        let mut overview = self.loads["service_get_vce_project_overview"].clone();
        overview["status"]["projectId"] = json!(id);
        overview["status"]["lifecycle"] = json!("uninitialized");
        overview["status"]["checkpointVersion"] = Value::Null;
        overview["status"]["presentFiles"] = json!(0);
        overview["status"]["checkpointBlobs"] = json!(0);
        overview["counts"] = json!({"files":0,"chunks":0});
        self.vce_projects.insert(
            id.to_string(),
            json!({"overview":overview,"job":null,"plan":null,"jobs":{},"commands":{}}),
        );
    }

    /// Appends a sidebar archive operation to the same workbench catalog the page queries.
    /// 将侧栏归档操作追加到页面查询的同一工作台目录。
    pub fn archive(&mut self, id: &str, title: &str, project: &str) {
        self.workbench["archivedConversations"]
            .as_array_mut()
            .expect("archive fixture")
            .push(json!({"id":id,"title":title,"projectName":project}));
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
        if let CallKind::Load(_) = call.kind {
            match call.method {
                "service_list_knowledge_sources"
                | "service_get_knowledge_import_job"
                | "service_get_knowledge_index_integrity"
                | "service_search_knowledge_library" => {
                    // Exact entity identity.
                    // 准确实体身份。
                    let id = if call.method == "service_search_knowledge_library" {
                        string(&call.params, "/request/libraryId")?
                    } else {
                        string(&call.params, "/libraryId")?
                    };
                    // Library.
                    // 所选知识库。
                    let library = self.knowledge.get(id).ok_or("Unknown demo library")?;
                    // Field.
                    // 准确字段定义。
                    let field = match call.method {
                        "service_list_knowledge_sources" => "sources",
                        "service_get_knowledge_import_job" => "job",
                        "service_get_knowledge_index_integrity" => "integrity",
                        _ => "search",
                    };
                    // Value.
                    // 当前协议值。
                    let mut value = library[field].clone();
                    if field == "search" {
                        value["query"] = required(&call.params, "/request/query")?.clone();
                    }
                    return Ok(value);
                }
                "service_list_knowledge_library_documents" => {
                    // Managed document summaries are owned by the requested library.
                    // 托管文档摘要属于请求的知识库。
                    let library = self
                        .knowledge
                        .get(string(&call.params, "/request/libraryId")?)
                        .ok_or("Unknown demo library")?;
                    // None includes all folders; the empty string is an exact root-folder filter.
                    // None 包含全部目录；空字符串为准确根目录筛选。
                    let folder = required(&call.params, "/request/folderPath")?;
                    // Pagination is bounded by the exact request values.
                    // 分页由准确请求值限定。
                    let offset = required(&call.params, "/request/offset")?
                        .as_u64()
                        .ok_or("Document offset")? as usize;
                    // Requested document limit, including the caller's next-page sentinel.
                    // 请求文档数量限制，包含调用方的下一页哨兵。
                    let limit = required(&call.params, "/request/limit")?
                        .as_u64()
                        .ok_or("Document limit")? as usize;
                    return Ok(Value::Array(
                        library["documents"]
                            .as_array()
                            .ok_or("Documents fixture")?
                            .iter()
                            .filter(|entry| {
                                folder.is_null() || entry["document"]["folderPath"] == *folder
                            })
                            .skip(offset)
                            .take(limit)
                            .map(|entry| entry["document"].clone())
                            .collect(),
                    ));
                }
                "service_list_knowledge_library_folders" => {
                    // Derive exact distinct managed folders, never native directories.
                    // 派生准确唯一托管目录，不使用原生目录。
                    let library = self
                        .knowledge
                        .get(string(&call.params, "/libraryId")?)
                        .ok_or("Unknown demo library")?;
                    // A sorted set gives deterministic fixture navigation.
                    // 有序集合提供确定性夹具导航。
                    let folders: std::collections::BTreeSet<&str> = library["documents"]
                        .as_array()
                        .ok_or("Documents fixture")?
                        .iter()
                        .map(|entry| string(entry, "/document/folderPath"))
                        .collect::<Result<_, _>>()?;
                    return Ok(json!(folders));
                }
                "service_read_knowledge_library_document" => {
                    // The reader resolves one exact managed document identity.
                    // 阅读器解析单个准确托管文档身份。
                    let library = self
                        .knowledge
                        .get(string(&call.params, "/libraryId")?)
                        .ok_or("Unknown demo library")?;
                    // Reject unknown documents rather than returning another library's fixture.
                    // 拒绝未知文档，不返回其他知识库夹具。
                    let document = library["documents"]
                        .as_array()
                        .ok_or("Documents fixture")?
                        .iter()
                        .find(|entry| entry["document"]["id"] == call.params["documentId"])
                        .ok_or("Unknown demo document")?;
                    // Both declared corpus variants have independent content availability.
                    // 两种已声明语料变体拥有独立正文可用状态。
                    let corpus = string(&call.params, "/corpus")?;
                    if !matches!(corpus, "raw" | "display") {
                        return Err("Unknown document corpus".into());
                    }
                    return Ok(
                        json!({"document":document["document"],"content":document[corpus].as_str().ok_or("Document corpus is not built")?}),
                    );
                }
                "service_list_knowledge_project_bindings" => {
                    // Exact entity identity.
                    // 准确实体身份。
                    let id = required(&call.params, "/libraryId")?;
                    return Ok(Value::Array(
                        self.loads[call.method]
                            .as_array()
                            .ok_or("bindings fixture")?
                            .iter()
                            .filter(|binding| binding.get("libraryId") == Some(id))
                            .cloned()
                            .collect(),
                    ));
                }
                "service_get_vce_project_overview" | "service_get_vce_job" => {
                    // Exact entity identity.
                    // 准确实体身份。
                    let id = string(&call.params, "/request/projectId")?;
                    // Project.
                    // 所选项目数据。
                    let project = self.vce_projects.get(id).ok_or("Unknown demo project")?;
                    if call.method == "service_get_vce_project_overview" {
                        return Ok(project["overview"].clone());
                    }
                    // Exact history requests cannot accidentally return another job.
                    // 准确历史请求不能意外返回另一个任务。
                    let job_id = required(&call.params, "/request/jobId")?;
                    if job_id.is_null() {
                        return Ok(project["job"].clone());
                    }
                    return Ok(project["jobs"]
                        .get(job_id.as_str().ok_or("Job id")?)
                        .cloned()
                        .unwrap_or(Value::Null));
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
            "service_preview_knowledge_import" => {
                return Ok(json!({"fingerprint":"demo-import-fingerprint",
                "entries":[{"key":DEMO_IMPORT_FILE_KEY,"relativePath":DEMO_IMPORT_RELATIVE_PATH,"disposition":"ready","sourceRoot":required(params,"/sourceRoot")?}]}));
            }
            "service_preview_knowledge_display" => {
                // Preflight returns only the selected library's managed raw documents.
                // 预检只返回所选知识库的托管原始文档。
                let library = self
                    .knowledge
                    .get(string(params, "/libraryId")?)
                    .ok_or("Unknown demo library")?;
                return Ok(
                    json!({"fingerprint":"demo-display-fingerprint","entries":library["documents"].as_array().ok_or("Documents fixture")?.iter()
                    .map(|entry| Ok(json!({"key":display_file_key(entry)?,"relativePath":entry["document"]["relativePath"],"disposition":"ready"}))).collect::<Result<Vec<Value>,String>>()?}),
                );
            }
            "service_build_knowledge_display_index" => {
                if string(params, "/confirmation/previewFingerprint")? != "demo-display-fingerprint"
                {
                    return Err("Display preview changed".into());
                }
                // The frozen keys select exact managed documents in this library.
                // 冻结键选择此知识库内准确托管文档。
                let selected = required(params, "/confirmation/selectedFileKeys")?
                    .as_array()
                    .ok_or("Display selection")?;
                // Scoped mutable raw/display content store.
                // 限定范围的可变原始、展示正文存储。
                let library = self
                    .knowledge
                    .get_mut(string(params, "/libraryId")?)
                    .ok_or("Unknown demo library")?;
                // Root/path keys are opaque confirmations and cannot be replaced with bare relative paths.
                // 根身份、路径键属于不透明确认，不能替换成裸相对路径。
                let ready = library["documents"]
                    .as_array()
                    .ok_or("Documents fixture")?
                    .iter()
                    .map(display_file_key)
                    .collect::<Result<std::collections::BTreeSet<_>, _>>()?;
                // Reject duplicate or unreviewed targets before changing any corpus content.
                // 改变任何语料正文前拒绝重复或未审查目标。
                let mut seen = std::collections::BTreeSet::new();
                if !selected.iter().all(|key| {
                    key.as_str()
                        .is_some_and(|key| ready.contains(key) && seen.insert(key))
                }) {
                    return Err("Unreviewed or duplicate display key".into());
                }
                for document in library["documents"]
                    .as_array_mut()
                    .ok_or("Documents fixture")?
                {
                    if selected.contains(&json!(display_file_key(document)?)) {
                        document["display"] = document["raw"].clone();
                    }
                }
                library["job"] = json!({"id":"demo-display-job","libraryId":params["libraryId"],"operationKind":"display_build","state":"succeeded","phase":"completed","warnings":[]});
                row_mut(
                    &mut self.loads["service_list_knowledge_libraries"],
                    "id",
                    required(params, "/libraryId")?,
                )?["settings"]["displayIndex"]["status"] = json!("ready");
            }
            "service_build_knowledge_vector_index" => {
                // A synthetic completed job models the same retained operation contract.
                // 合成完成任务模拟相同保留操作契约。
                let library = self
                    .knowledge
                    .get_mut(string(params, "/libraryId")?)
                    .ok_or("Unknown demo library")?;
                library["job"] = json!({"id":"demo-vector-job","libraryId":params["libraryId"],"operationKind":"vector_build","state":"succeeded","phase":"completed","warnings":[]});
                row_mut(
                    &mut self.loads["service_list_knowledge_libraries"],
                    "id",
                    required(params, "/libraryId")?,
                )?["settings"]["vectorIndex"]["status"] = json!("ready");
            }
            "service_inspect_local_plugin" => {
                return Ok(
                    json!({"pluginId":"demo-installed-plugin","name":"Demo Installed Plugin",
                "contentFingerprint":"demo-plugin-fingerprint","permissions":["filesystem_read"],"installSecretRequirements":[]}),
                );
            }
            "service_plan_vce_maintenance" => {
                // Project.
                // 所选项目数据。
                let project = self
                    .vce_projects
                    .get_mut(string(params, "/request/projectId")?)
                    .ok_or("Unknown demo project")?;
                // Plan.
                // 已审查操作计划。
                let plan = json!({"checkpointVersion":project["overview"]["status"]["checkpointVersion"],
                    "mode":required(params,"/request/mode")?,"embeddingDimension":384,
                    "embeddingFingerprint":"demo-embedding-fingerprint","actions":[{"actionId":"demo-maintenance-action","description":"Demo index maintenance","destructive":false,"blobNames":[]}],
                    "affectedBlobCount":0,"physicalTables":[]});
                project["plan"] = plan.clone();
                return Ok(plan);
            }
            "service_preflight_vce_embedding_route" => {
                // Route.
                // 目录中的推理路由。
                let route = required(params, "/request/route")?;
                return Ok(json!({"candidate":if route.is_null() {Value::Null} else {
                    json!({"route":route,"dimension":required(params,"/request/dimension")?,"physicalFingerprint":"demo-vector"})},
                    "projects":[],"revision":"demo-preflight-revision"}));
            }
            "service_save_git_commit_settings" => {
                self.workbench["gitCommit"] = required(params, "/request")?.clone()
            }
            "service_save_provider_retry_settings" => {
                self.workbench["providerRetry"] = required(params, "/request")?.clone()
            }
            "service_save_provider_request_trace_settings" => {
                self.workbench["providerRequestTraceEnabled"] =
                    required(params, "/request/enabled")?.clone()
            }
            "service_select_workbench_thinking_display_mode" => {
                self.workbench["thinkingDisplayMode"] = required(params, "/mode")?.clone()
            }
            "service_save_tool_collapse_default" => {
                self.workbench["toolCollapseDefaultEnabled"] = required(params, "/enabled")?.clone()
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
            "service_save_prompt_output_style" => {
                self.loads["service_output_style_catalog"]["selectedId"] =
                    required(params, "/request/styleId")?.clone()
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
            "service_save_public_ai_model_enabled"
            | "service_save_public_ai_internal_model_enabled" => {
                // Exact protocol request.
                // 协议请求文档。
                let request = required(params, "/request")?;
                // Current operation rows.
                // 当前操作的行集合。
                let rows = self.loads["service_public_ai_settings_snapshot"]["modelAccessRecords"]
                    .as_array_mut()
                    .ok_or("model fixture")?;
                // Current row.
                // 当前条目。
                let row = rows
                    .iter_mut()
                    .find(|row| {
                        row.get("providerId") == request.get("providerId")
                            && row.get("modelId") == request.get("modelId")
                    })
                    .ok_or("unknown demo model")?;
                row["enabled"] = required(request, "/enabled")?.clone();
                row["customized"] = json!(true);
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
                        profile["llm_routes"][string(request, "/purposeId")?] =
                            required(request, "/route")?.clone()
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
            "service_create_knowledge_library" => {
                // Exact protocol request.
                // 协议请求文档。
                let request = required(params, "/request")?;
                // Library.
                // 所选知识库。
                let mut library = self.library_defaults.clone();
                // Exact entity identity.
                // 准确实体身份。
                let id = self.id("library");
                library["id"] = json!(id);
                library["name"] = required(request, "/name")?.clone();
                library["description"] = required(request, "/description")?.clone();
                library["documentCount"] = json!(0);
                library["chunkCount"] = json!(0);
                merge(&mut library["settings"], required(request, "/settings")?);
                self.knowledge.insert(
                    id.clone(),
                    json!({"sources":[],"job":null,"documents":[],
                    "integrity":{"libraryId":id,"state":"ok"},
                    "search":{"libraryId":id,"query":"","hits":[]}}),
                );
                self.loads["service_list_knowledge_libraries"]
                    .as_array_mut()
                    .ok_or("library fixture")?
                    .push(library);
            }
            "service_rename_knowledge_library" => {
                // Current row.
                // 当前条目。
                let row = row_mut(
                    &mut self.loads["service_list_knowledge_libraries"],
                    "id",
                    required(params, "/request/libraryId")?,
                )?;
                row["name"] = required(params, "/request/name")?.clone();
                row["description"] = required(params, "/request/description")?.clone();
            }
            "service_delete_knowledge_library" => {
                // Exact entity identity.
                // 准确实体身份。
                let id = string(params, "/libraryId")?;
                self.knowledge.remove(id);
                self.loads["service_list_knowledge_libraries"]
                    .as_array_mut()
                    .ok_or("library fixture")?
                    .retain(|library| library["id"] != id);
                self.loads["service_list_knowledge_project_bindings"]
                    .as_array_mut()
                    .ok_or("bindings fixture")?
                    .retain(|binding| binding["libraryId"] != id);
            }
            "service_set_knowledge_library_settings" => {
                // Current row.
                // 当前条目。
                let row = row_mut(
                    &mut self.loads["service_list_knowledge_libraries"],
                    "id",
                    required(params, "/request/libraryId")?,
                )?;
                merge(&mut row["settings"], required(params, "/request/settings")?);
            }
            "service_set_knowledge_project_binding" => {
                // Exact protocol request.
                // 协议请求文档。
                let request = required(params, "/request")?;
                // Bindings.
                // 权威绑定集合。
                let bindings = self.loads["service_list_knowledge_project_bindings"]
                    .as_array_mut()
                    .ok_or("bindings fixture")?;
                bindings.retain(|binding| {
                    !(binding.get("libraryId") == request.get("libraryId")
                        && binding.get("projectId") == request.get("projectId"))
                });
                if request["authorized"] == true {
                    bindings.push(json!({"libraryId":request["libraryId"],"projectId":request["projectId"],"defaultEnabled":request["defaultEnabled"]}));
                }
            }
            "service_import_knowledge_directory" => {
                if string(params, "/request/previewFingerprint")? != "demo-import-fingerprint" {
                    return Err("Import preview changed".into());
                }
                // The demo preview publishes a single exact key/path mapping.
                // 演示预检发布单个准确的键、路径映射。
                let mut seen = std::collections::BTreeSet::new();
                if !required(params, "/request/selectedFileKeys")?
                    .as_array()
                    .ok_or("Selected files")?
                    .iter()
                    .all(|key| {
                        key.as_str() == Some(DEMO_IMPORT_FILE_KEY) && seen.insert(key.as_str())
                    })
                {
                    return Err("Unreviewed or duplicate import key".into());
                }
                // Exact reviewed file keys and durable source identity.
                // 准确已审查文件键及持久来源身份。
                let selected = required(params, "/request/selectedFileKeys")?
                    .as_array()
                    .ok_or("Selected files")?;
                // Existing source remaps keep their identity; new imports allocate a distinct source.
                // 已有来源重映射保留身份；新导入分配独立来源。
                let source_id = match required(params, "/request/sourceId")? {
                    Value::Null => self.id("source"),
                    Value::String(id) => id.clone(),
                    _ => return Err("Source id".into()),
                };
                // Target library content store.
                // 目标知识库正文存储。
                let state = self
                    .knowledge
                    .get_mut(string(params, "/request/libraryId")?)
                    .ok_or("Unknown demo library")?;
                // Update the exact existing source or append the newly allocated source.
                // 更新准确已有来源或追加新分配来源。
                let sources = state["sources"].as_array_mut().ok_or("Sources fixture")?;
                if let Some(source) = sources.iter_mut().find(|source| source["id"] == source_id) {
                    source["canonicalRoot"] = required(params, "/request/sourceRoot")?.clone();
                } else if !required(params, "/request/sourceId")?.is_null() {
                    return Err("Unknown demo source".into());
                } else {
                    sources.push(json!({"id":source_id,"canonicalRoot":required(params,"/request/sourceRoot")?,
                        "displayName":"Imported demo source","documentCount":selected.len(),"chunkCount":selected.len()}));
                }
                for _ in selected {
                    // Slash-separated managed paths define the folder without native path guessing.
                    // 斜杠分隔的托管路径定义目录，不猜测原生路径。
                    let relative_path = DEMO_IMPORT_RELATIVE_PATH;
                    // The published preview path, rather than its independently defined file key, owns folder structure.
                    // 已发布预检路径而非独立定义的文件键持有目录结构。
                    let folder = relative_path
                        .rsplit_once('/')
                        .map_or("", |(folder, _)| folder);
                    // Stable document identity is derived solely within the mock's declared source fixture.
                    // 稳定文档身份仅在模拟声明的来源夹具内派生。
                    let document_id = state["documents"]
                        .as_array()
                        .ok_or("Documents fixture")?
                        .iter()
                        .find(|entry| {
                            entry["document"]["sourceId"] == source_id
                                && entry["document"]["relativePath"] == relative_path
                        })
                        .map(|entry| string(entry, "/document/id").map(str::to_string))
                        .transpose()?
                        .unwrap_or_else(|| format!("{source_id}:{relative_path}"));
                    // Replace the same logical document on rescan instead of duplicating it.
                    // 重新扫描替换同一逻辑文档，不重复追加。
                    let documents = state["documents"]
                        .as_array_mut()
                        .ok_or("Documents fixture")?;
                    documents.retain(|entry| entry["document"]["id"] != document_id);
                    documents.push(json!({"document":{"id":document_id,"sourceId":source_id,"relativePath":relative_path,"folderPath":folder,
                        "title":relative_path,"byteSize":22,"lineCount":1,"chunkCount":1,"updatedAtMs":0},"raw":"Synthetic imported text","display":null}));
                }
                // Reflect actual stored content counts after imports and rescans.
                // 导入及重新扫描后反映真实已存正文计数。
                let count = state["documents"]
                    .as_array()
                    .ok_or("Documents fixture")?
                    .len();
                // Each source count reflects the same authoritative managed document store.
                // 每个来源计数反映相同权威托管文档存储。
                let documents = state["documents"]
                    .as_array()
                    .ok_or("Documents fixture")?
                    .clone();
                for source in state["sources"].as_array_mut().ok_or("Sources fixture")? {
                    // Exact source-owned document count after rescan/remap.
                    // 重新扫描及重映射后准确的来源文档数。
                    let owned = documents
                        .iter()
                        .filter(|entry| entry["document"]["sourceId"] == source["id"])
                        .count();
                    source["documentCount"] = json!(owned);
                    source["chunkCount"] = json!(owned);
                }
                state["job"] = json!({"id":"demo-import-job","libraryId":required(params,"/request/libraryId")?,
                    "operationKind":"import","state":"succeeded","phase":"completed","processedFiles":selected.len(),"totalFiles":selected.len(),"warnings":[]});
                // The catalog reflects the same library-owned document store.
                // 目录反映同一知识库持有的文档存储。
                let library = row_mut(
                    &mut self.loads["service_list_knowledge_libraries"],
                    "id",
                    required(params, "/request/libraryId")?,
                )?;
                library["documentCount"] = json!(count);
                library["chunkCount"] = json!(count);
            }
            "service_rebuild_knowledge_index" | "service_repair_knowledge_index" => {
                self.knowledge
                    .get_mut(string(params, "/libraryId")?)
                    .ok_or("Unknown demo library")?["job"] = json!({
                "id":"demo-index-job","libraryId":required(params,"/libraryId")?,"operationKind":if call.method.contains("repair") {"repair"} else {"rebuild"},"state":"succeeded"})
            }
            "service_cancel_knowledge_import" => {
                self.knowledge
                    .get_mut(string(params, "/libraryId")?)
                    .ok_or("Unknown demo library")?["job"]["state"] = json!("cancelled")
            }
            "service_update_vce_project_settings" => {
                // Overview.
                // 所选项目权威概览。
                let overview = &mut self
                    .vce_projects
                    .get_mut(string(params, "/request/projectId")?)
                    .ok_or("Unknown demo project")?["overview"];
                if required(params, "/request/expectedRevision")?
                    != &overview["settings"]["revision"]
                {
                    return Err("VCE settings changed".into());
                }
                // Revision.
                // 权威修订号。
                let revision = overview["settings"]["revision"]
                    .as_i64()
                    .ok_or("revision fixture")?
                    + 1;
                merge(
                    &mut overview["settings"],
                    required(params, "/request/settings")?,
                );
                overview["settings"]["revision"] = json!(revision);
                overview["status"]["settingsRevision"] = json!(revision);
                overview["status"]["enabled"] = overview["settings"]["enabled"].clone();
            }
            "service_submit_vce_job" | "service_execute_vce_maintenance" => {
                // Compare frozen settings/checkpoint guards before creating a durable demo receipt.
                // 创建持久演示回执前，比较冻结的设置与检查点保护值。
                let project_id = string(params, "/request/projectId")?;
                // Exact protocol request.
                // 协议请求文档。
                let request = required(params, "/request")?;
                // Key.
                // 当前结构持有的键。
                let key = string(request, "/idempotencyKey")?;
                // Project.
                // 所选项目数据。
                let project = self
                    .vce_projects
                    .get(project_id)
                    .ok_or("Unknown demo project")?;
                if let Some(replay) = project["commands"].get(key) {
                    if replay["request"] != *request {
                        return Err("Idempotency key changed intent".into());
                    }
                    return Ok(replay["job"].clone());
                }
                if required(request, "/expectedSettingsRevision")?
                    != &project["overview"]["settings"]["revision"]
                    || required(request, "/expectedCheckpointVersion")?
                        != &project["overview"]["status"]["checkpointVersion"]
                {
                    return Err("VCE plan is stale".into());
                }
                // Kind.
                // 已声明的操作类别。
                let kind = if call.method == "service_execute_vce_maintenance" {
                    // Plan.
                    // 已审查操作计划。
                    let plan = &project["plan"];
                    // Actions.
                    // 声明的操作集合。
                    let actions: Vec<_> = required(plan, "/actions")?
                        .as_array()
                        .ok_or("Planned actions")?
                        .iter()
                        .map(|action| action["actionId"].clone())
                        .collect();
                    if required(request, "/mode")? != &plan["mode"]
                        || required(request, "/expectedEmbeddingFingerprint")?
                            != &plan["embeddingFingerprint"]
                        || required(request, "/actionIds")? != &json!(actions)
                    {
                        return Err("VCE maintenance targets changed".into());
                    }
                    if plan["mode"] == "compact" {
                        "compact"
                    } else {
                        "rebuild"
                    }
                } else {
                    match string(request, "/action")? {
                        "initialize" => "initialize",
                        "synchronize" => "sync",
                        _ => return Err("Unknown job action".into()),
                    }
                };
                // Exact entity identity.
                // 准确实体身份。
                let id = self.id("vce-job");
                // Created.
                // 合成任务创建时间。
                let created = now_ms();
                // Job.
                // 持久任务回执。
                let job = json!({"jobId":id,"projectId":project_id,"state":"queued","kind":kind,
                    "sequence":1,"createdAtUnixMs":created,"updatedAtUnixMs":created});
                // Project.
                // 所选项目数据。
                let project = self
                    .vce_projects
                    .get_mut(project_id)
                    .ok_or("Unknown demo project")?;
                project["job"] = job.clone();
                project["jobs"][&id] = job.clone();
                project["commands"][key] = json!({"request":request,"job":job});
                return Ok(job);
            }
            "service_cancel_vce_job" => {
                // Job.
                // 持久任务回执。
                let job = &mut self
                    .vce_projects
                    .get_mut(string(params, "/request/projectId")?)
                    .ok_or("Unknown demo project")?["job"];
                if required(params, "/request/jobId")? != &job["jobId"] {
                    return Err("Unknown demo job".into());
                }
                job["state"] = json!("cancelled");
                job["sequence"] = json!(job["sequence"].as_u64().ok_or("job sequence")? + 1);
                // Receipt.
                // 持久任务回执。
                let receipt = job.clone();
                // Exact entity identity.
                // 准确实体身份。
                let id = string(params, "/request/jobId")?;
                self.vce_projects
                    .get_mut(string(params, "/request/projectId")?)
                    .ok_or("Demo project")?["jobs"][id] = receipt;
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
                    "affectedProjectCount":0,"batchCount":0,"previewFingerprint":"demo-vector-fingerprint"})
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
