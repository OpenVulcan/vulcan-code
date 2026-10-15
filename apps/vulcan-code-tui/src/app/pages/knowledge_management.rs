//! Human-only metadata, indexing, grants, dictionary and portable package management.
//! 人工专属元信息、索引、授权、词典及可移植套件管理。

use super::*;

/// Renders an explicit unsaved policy, selecting no mode or model for the human.
/// 渲染明确未保存策略，不代替人工选择模式或模型。
/// Parameters: app owns the local draft and current official model candidates. Returns local fields and one explicit save action.
/// 参数：app 拥有本地草稿及当前官方模型候选。返回本地字段及一个明确保存动作。
pub(crate) fn policy(app: &App) -> Vec<Row> {
    // A draft is created only by the corresponding explicit action.
    // 草稿仅由对应明确动作创建。
    let Some(draft) = app.knowledge.draft.as_ref() else {
        return vec![button(
            tr!("knowledge.policy.createTitle"),
            UiAction::Knowledge(KnowledgeAction::Create),
            Tone::Accent,
        )];
    };
    // New and existing owners have distinct metadata edit boundaries.
    // 新建及既有所有者具有不同元信息编辑边界。
    let mut rows = vec![
        section(if draft.library_id.is_none() {
            tr!("knowledge.policy.createTitle")
        } else {
            tr!("knowledge.policy.modeTitle")
        }),
        note(tr!("knowledge.policy.description"), Tone::Muted),
    ];
    if draft.library_id.is_none() {
        rows.push(text(
            tr!("knowledge.name"),
            draft.name.clone(),
            draft_edit(KnowledgeDraftField::Name),
        ));
        rows.push(text(
            tr!("knowledge.purpose"),
            draft.purpose.clone(),
            draft_edit(KnowledgeDraftField::Purpose),
        ));
    } else {
        rows.push(section(draft.name.clone()));
    }
    // The options mirror the closed shared indexing enum.
    // 选项镜像封闭共享索引枚举。
    let modes = [
        KnowledgeIndexModeRequest::Browse,
        KnowledgeIndexModeRequest::Bm25,
        KnowledgeIndexModeRequest::Vector,
        KnowledgeIndexModeRequest::Hybrid,
    ]
    .into_iter()
    .map(|mode| {
        // Serialization is the enum's declared wire identity.
        // 序列化为枚举声明线协议身份。
        let key = serde_json::to_value(mode)
            .expect("index mode")
            .as_str()
            .expect("index mode string")
            .to_string();
        (key.clone(), mode_label(&key))
    })
    .collect();
    // An empty current choice preserves conscious selection for a new library.
    // 空当前选择保留新知识库的主动选择。
    let current = draft
        .mode
        .map(|mode| {
            serde_json::to_value(mode)
                .expect("index mode")
                .as_str()
                .expect("index mode string")
                .to_string()
        })
        .unwrap_or_default();
    rows.push(choice(
        tr!("knowledge.indexMode"),
        modes,
        current,
        draft_edit(KnowledgeDraftField::Mode),
    ));
    if matches!(
        draft.mode,
        Some(KnowledgeIndexModeRequest::Vector | KnowledgeIndexModeRequest::Hybrid)
    ) {
        rows.push(note(tr!("knowledge.mode.vector.description"), Tone::Muted));
        // Candidate membership must pass the same official admission as the WebUI.
        // 候选成员关系必须通过与 WebUI 相同的官方准入。
        let models = a(
            app.slots.value(Slot::PublicAiSettings),
            "/internalInferenceCandidates",
        )
        .iter()
        .filter(|candidate| embedding_candidate(candidate))
        .filter_map(|candidate| {
            Some((
                candidate_key(candidate)?,
                format!(
                    "{} · {} · {}",
                    s(candidate, "/modelDisplayName"),
                    s(candidate, "/providerDisplayName"),
                    s(candidate, "/connectionDisplayName")
                ),
            ))
        })
        .collect::<Vec<_>>();
        if models.is_empty() {
            rows.push(note(tr!("knowledge.policy.noModels"), Tone::Muted));
        }
        rows.push(choice(
            tr!("knowledge.policy.embedding"),
            models,
            draft.route_key.clone(),
            draft_edit(KnowledgeDraftField::Route),
        ));
        rows.push(text(
            tr!("knowledge.policy.dimension"),
            draft.dimensions.clone(),
            draft_edit(KnowledgeDraftField::Dimensions),
        ));
        rows.push(text(
            tr!("knowledge.policy.batch"),
            draft.batch.clone(),
            draft_edit(KnowledgeDraftField::Batch),
        ));
        rows.push(note(tr!("knowledge.policy.serverBatch"), Tone::Muted));
    }
    if draft
        .settings(app.slots.value(Slot::PublicAiSettings))
        .is_some()
        && (draft.library_id.is_some() || !draft.name.trim().is_empty())
    {
        rows.push(button(
            tr!("memory.common.save"),
            UiAction::Knowledge(KnowledgeAction::SaveDraft),
            Tone::Accent,
        ));
    } else {
        rows.push(note(tr!("knowledge.policy.dimensionRequired"), Tone::Muted));
    }
    rows.push(button(
        tr!("conversation.action.cancel"),
        UiAction::Knowledge(KnowledgeAction::CancelDraft),
        Tone::Normal,
    ));
    rows
}

/// Edits only human name and purpose; index mode changes use the complete explicit draft.
/// 仅编辑人工名称及用途；索引模式变更使用完整明确草稿。
/// Parameters: app supplies the current exact library. Returns metadata form and configuration entry, with no content editor.
/// 参数：app 提供当前精确知识库。返回元信息表单及配置入口，不含内容编辑器。
pub(crate) fn settings(app: &App) -> Vec<Row> {
    // The format gate prevents writes to unreadable old libraries.
    // 格式门禁防止写入不可读旧知识库。
    let mut rows = header(app, PageId::KnowledgeSettings);
    let Some(library) = active_library(app) else {
        return rows;
    };
    rows.push(note(tr!("knowledge.metadataDescription"), Tone::Muted));
    rows.push(button(tr!("knowledge.metadata"), form(json!({"libraryId":s(library,"/id"),"name":s(library,"/name"),"description":s(library,"/description")}), vec![field(tr!("knowledge.name"), "/name", false, false), field(tr!("knowledge.purpose"), "/description", false, true)], |request| knowledge::rename_knowledge_library(serde_json::from_value(request).expect("validated metadata form"))), Tone::Normal));
    rows.push(info(
        tr!("knowledge.indexMode"),
        mode_label(&s(library, "/settings/indexMode")),
        Tone::Normal,
    ));
    rows.push(button(
        tr!("knowledge.policy.modeTitle"),
        UiAction::Knowledge(KnowledgeAction::ConfigureMode),
        Tone::Accent,
    ));
    rows.push(button(
        tr!("knowledge.delete"),
        confirm(
            tr!("knowledge.delete"),
            tr!("knowledge.deleteConfirm", name = s(library, "/name")),
            tr!("knowledge.delete"),
            call(knowledge::delete_knowledge_library(&s(library, "/id"))),
        ),
        Tone::Bad,
    ));
    rows
}

/// Edits exact project grants, keeping authorization separate from inherited default selection.
/// 编辑精确项目授权，保持授权与继承默认选择分离。
/// Parameters: app supplies workbench projects and this library's binding DTOs. Returns atomic grant toggles.
/// 参数：app 提供工作台项目及此库绑定 DTO。返回原子授权开关。
pub(crate) fn bindings(app: &App) -> Vec<Row> {
    // Only the active exact library can own a grant mutation.
    // 仅精确活动知识库可拥有授权变更。
    let mut rows = header(app, PageId::KnowledgeBindings);
    let Some(library) = active_library(app) else {
        return rows;
    };
    rows.push(note(tr!("knowledge.bindings.description"), Tone::Muted));
    if !ready(app, Slot::KnowledgeBindings, &mut rows) {
        return rows;
    }
    // This is the current library's sole authoritative grant collection.
    // 此集合为当前知识库唯一权威授权集合。
    let grants = a(app.slots.value(Slot::KnowledgeBindings), "");
    for project in a(app.slots.value(Slot::Workbench), "/projects") {
        // The exact host project ID comes from the workbench, never typed arbitrary text.
        // 精确宿主项目身份来自工作台，绝非输入任意文本。
        let project_id = s(project, "/id");
        // Presence means authorization; revocation deletes the binding row.
        // 存在表示授权；撤销会删除绑定行。
        let grant = grants.iter().find(|grant| {
            grant.get("projectId").and_then(Value::as_str) == Some(project_id.as_str())
        });
        // Missing grants explicitly have no inherited default.
        // 缺少授权明确表示没有继承默认。
        let default_enabled = grant.is_some_and(|grant| b(grant, "/defaultEnabled"));
        rows.push(item(&project_id, s(project, "/name"), "").actions(vec![
            switch(
                tr!("knowledge.bindings.authorized"),
                grant.is_some(),
                Some(call(knowledge::set_knowledge_project_binding(
                    KnowledgeProjectBindingRequest {
                        library_id: s(library, "/id"),
                        project_id: project_id.clone(),
                        authorized: grant.is_none(),
                        default_enabled: false,
                    },
                ))),
            ),
            switch(
                tr!("knowledge.bindings.default"),
                default_enabled,
                grant.map(|_| {
                    call(knowledge::set_knowledge_project_binding(
                        KnowledgeProjectBindingRequest {
                            library_id: s(library, "/id"),
                            project_id: project_id.clone(),
                            authorized: true,
                            default_enabled: !default_enabled,
                        },
                    ))
                }),
            ),
        ]));
    }
    rows
}

/// Atomically updates dictionary configuration, without exposing document content changes.
/// 原子更新词典配置，不暴露文档内容变更。
/// Parameters: app owns exact library and current dictionary. Returns a validated term form and exact-key removal.
/// 参数：app 拥有精确知识库及当前词典。返回已验证词语表单及精确键移除。
pub(crate) fn dictionary(app: &App) -> Vec<Row> {
    // Dictionary state must be read successfully before writable controls exist.
    // 词典状态必须成功读取后才存在可写控件。
    let mut rows = header(app, PageId::KnowledgeDictionary);
    let Some(library) = active_library(app) else {
        return rows;
    };
    rows.push(note(tr!("knowledge.dictionary.description"), Tone::Muted));
    if !ready(app, Slot::KnowledgeWords, &mut rows) {
        return rows;
    }
    rows.push(button(
        tr!("memory.common.save"),
        form(
            json!({"libraryId":s(library,"/id"),"word":"","weight":1}),
            vec![
                field(tr!("knowledge.dictionary.word"), "/word", false, false),
                parsed_field(
                    tr!("knowledge.dictionary.weight"),
                    "/weight",
                    false,
                    |value| {
                        value
                            .trim()
                            .parse::<u32>()
                            .ok()
                            .filter(|weight| *weight > 0)
                            .map(Value::from)
                    },
                ),
            ],
            |request| {
                knowledge::upsert_knowledge_custom_word(
                    serde_json::from_value(request).expect("validated dictionary form"),
                )
            },
        ),
        Tone::Accent,
    ));
    for word in a(app.slots.value(Slot::KnowledgeWords), "") {
        // Exact term text is the atomic key; removal never replaces the whole dictionary.
        // 精确词语文本为原子键；移除绝不替换整个词典。
        let term = s(word, "/word");
        rows.push(
            item(&term, term.clone(), n(word, "/weight").to_string()).actions(vec![action(
                tr!("knowledge.dictionary.remove"),
                call(knowledge::remove_knowledge_custom_word(
                    &s(library, "/id"),
                    &term,
                )),
                Tone::Bad,
            )]),
        );
    }
    rows
}

/// Localizes declared durable operation states with the shared catalog.
/// 使用共享目录本地化声明持久操作状态。
/// Parameters: state is the exact operation code. Returns its localized status.
/// 参数：state 为精确操作代码。返回本地化状态。
fn operation_label(state: &str) -> String {
    match state {
        "queued" => tr!("knowledge.operation.queued"),
        "running" => tr!("knowledge.operation.running"),
        "completed" => tr!("knowledge.operation.completed"),
        "cancelled" => tr!("knowledge.operation.cancelled"),
        "failed" => tr!("knowledge.operation.failed"),
        _ => tr!("knowledge.operation.unknown"),
    }
}

/// Displays independent live jobs, full-text integrity and vector generation backlog.
/// 显示独立实时任务、全文完整性及向量生成积压。
/// Parameters: app owns current exact status slots. Returns visible failures and counts without inventing success.
/// 参数：app 拥有当前精确状态槽位。返回可见失败及数量，不虚构成功。
pub(super) fn status_rows(app: &App) -> Vec<Row> {
    // Each status endpoint retains its own error and read lifetime.
    // 每个状态端点保留自身错误及读取寿命。
    let mut rows = Vec::new();
    if app.slots.loaded(Slot::KnowledgeJob) && app.slots.value(Slot::KnowledgeJob).is_object() {
        // The latest durable job is an observation, not a guessed live process.
        // 最新持久任务为观察，绝非推测实时进程。
        let job = app.slots.value(Slot::KnowledgeJob);
        rows.push(info(
            tr!("knowledge.index.latest"),
            operation_label(&s(job, "/state")),
            Tone::Accent,
        ));
        rows.push(note(
            tr!(
                "knowledge.index.operationProgress",
                done = n(job, "/processedItems"),
                total = n(job, "/totalItems")
            ),
            Tone::Normal,
        ));
        if n(job, "/totalItems") > 0 {
            rows.push(meter(
                tr!("knowledge.index.latest"),
                n(job, "/processedItems") as f64 / n(job, "/totalItems") as f64,
                tr!(
                    "knowledge.index.operationProgress",
                    done = n(job, "/processedItems"),
                    total = n(job, "/totalItems")
                ),
                Tone::Accent,
            ));
        }
        if let Some(error) = job.get("errorMessage").and_then(Value::as_str) {
            rows.push(note(error.into(), Tone::Bad));
        }
        for warning in a(job, "/warnings") {
            if let Some(warning) = warning.as_str() {
                rows.push(note(warning.into(), Tone::Muted));
            }
        }
    }
    if app.slots.loaded(Slot::KnowledgeIntegrity) {
        // These counts come from a fresh physical comparison, not stale registry labels.
        // 这些数量来自新鲜物理比较，而非过期注册表标签。
        let integrity = app.slots.value(Slot::KnowledgeIntegrity);
        rows.push(note(
            if b(integrity, "/healthy") {
                tr!("knowledge.index.healthy")
            } else {
                tr!("knowledge.index.needsRepair")
            },
            Tone::Normal,
        ));
        rows.push(note(
            tr!(
                "knowledge.index.fts",
                rows = n(integrity, "/indexedChunkCount"),
                mismatches = n(integrity, "/mismatchedRows")
            ),
            Tone::Muted,
        ));
    }
    if app.slots.loaded(Slot::KnowledgeEmbeddings) {
        // Active and desired spaces remain distinct until a replacement is fully generated.
        // 替代空间完整生成前，活动及期望空间保持不同。
        let vectors = app.slots.value(Slot::KnowledgeEmbeddings);
        if v(vectors, "/desiredSpaceId").is_null() {
            rows.push(note(tr!("knowledge.index.vectorsDisabled"), Tone::Muted));
        } else {
            rows.push(note(
                tr!(
                    "knowledge.index.vectorCounts",
                    ready = n(vectors, "/ready"),
                    pending = n(vectors, "/pending"),
                    failed = n(vectors, "/failed")
                ),
                Tone::Normal,
            ));
            if b(vectors, "/paused") {
                rows.push(note(tr!("knowledge.index.paused"), Tone::Muted));
            }
            if v(vectors, "/activeSpaceId") != v(vectors, "/desiredSpaceId") {
                rows.push(note(tr!("knowledge.index.switching"), Tone::Muted));
            }
        }
    }
    rows
}

/// Exposes explicit rebuild, repair, stable-ID cancellation and current-vector retry actions.
/// 暴露明确重建、修复、稳定身份取消及当前向量重试动作。
/// Parameters: app owns current exact status. Returns lifecycle controls only after required reads succeed.
/// 参数：app 拥有当前精确状态。仅在必要读取成功后返回生命周期控件。
pub(crate) fn maintenance(app: &App) -> Vec<Row> {
    // Unreadable libraries cannot schedule derived work.
    // 不可读知识库不能调度派生工作。
    let mut rows = header(app, PageId::KnowledgeMaintenance);
    let Some(library) = active_library(app) else {
        return rows;
    };
    rows.extend(status_rows(app));
    // Failed job reads cannot prove a new job is safe to start.
    // 失败任务读取不能证明可安全开始新任务。
    if !app.slots.loaded(Slot::KnowledgeJob) || app.slot_errors.contains_key(&Slot::KnowledgeJob) {
        return rows;
    }
    // Only explicit absence or a recognized state can authorize any lifecycle operation.
    // 仅明确不存在或已识别状态可授权任何生命周期操作。
    let job = app.slots.value(Slot::KnowledgeJob);
    if !job.is_null()
        && !matches!(
            job.get("state").and_then(Value::as_str),
            Some("queued" | "running" | "completed" | "cancelled" | "failed")
        )
    {
        return rows;
    }
    // All operation requests retain the actual physical library identity.
    // 全部操作请求保留实际物理知识库身份。
    let id = s(library, "/id");
    // Only declared queued/running states are currently cancellable.
    // 仅声明排队及运行状态当前可取消。
    let running = matches!(
        app.slots.at(Slot::KnowledgeJob, "/state").as_str(),
        Some("queued" | "running")
    );
    if running {
        if let Some(operation_id) = app.slots.at(Slot::KnowledgeJob, "/id").as_str() {
            rows.push(button(
                tr!("knowledge.index.cancel"),
                call(knowledge::cancel_knowledge_operation(
                    KnowledgeOperationCancelRequest {
                        library_id: id,
                        operation_id: operation_id.into(),
                    },
                )),
                Tone::Bad,
            ));
        }
        return rows;
    }
    rows.push(button(
        tr!("knowledge.rebuildIndex"),
        call(knowledge::rebuild_knowledge_index(&id)),
        Tone::Normal,
    ));
    if app.slots.loaded(Slot::KnowledgeIntegrity)
        && !b(app.slots.value(Slot::KnowledgeIntegrity), "/healthy")
    {
        rows.push(button(
            tr!("knowledge.repairIndex"),
            call(knowledge::repair_knowledge_index(&id)),
            Tone::Normal,
        ));
    }
    if !app.slots.loaded(Slot::KnowledgeEmbeddings) {
        return rows;
    }
    // Retry targets are current document identities, not obsolete historical chunks.
    // 重试目标为当前文档身份，而非过期历史切块。
    let vectors = app.slots.value(Slot::KnowledgeEmbeddings);
    if n(vectors, "/failed") > 0 || (n(vectors, "/pending") > 0 && b(vectors, "/paused")) {
        rows.push(button(
            tr!("knowledge.index.retryAll"),
            call(knowledge::retry_knowledge_embeddings(
                KnowledgeEmbeddingRetryRequest {
                    library_id: id.clone(),
                    document_ids: Vec::new(),
                },
            )),
            Tone::Accent,
        ));
    }
    for failure in a(vectors, "/failures") {
        rows.push(
            item(
                &s(failure, "/chunkId"),
                s(failure, "/path"),
                s(failure, "/lastError"),
            )
            .actions(vec![action(
                tr!("knowledge.index.retryDocument"),
                call(knowledge::retry_knowledge_embeddings(
                    KnowledgeEmbeddingRetryRequest {
                        library_id: id.clone(),
                        document_ids: vec![s(failure, "/documentId")],
                    },
                )),
                Tone::Accent,
            )]),
        );
    }
    if app.knowledge.failure_offsets.len() > 1 {
        rows.push(button(
            tr!("knowledge.page.previous"),
            UiAction::Knowledge(KnowledgeAction::FailuresBack),
            Tone::Normal,
        ));
    }
    if let Some(offset) = vectors.get("nextOffset").and_then(Value::as_u64) {
        rows.push(button(
            tr!("knowledge.page.next"),
            UiAction::Knowledge(KnowledgeAction::FailuresNext(offset)),
            Tone::Normal,
        ));
    }
    rows
}

/// Browses host-native packages and freezes inspected bytes before independent import.
/// 浏览宿主原生套件，并在独立导入前冻结已检查字节。
/// Parameters: app owns the directory and optional export library. Returns read navigation, inspection and explicit export form.
/// 参数：app 拥有目录及可选导出知识库。返回读取导航、检查及明确导出表单。
pub(crate) fn packages(app: &App) -> Vec<Row> {
    // Package browsing is available even when no library exists.
    // 即使知识库不存在，套件浏览仍可使用。
    let mut rows = vec![
        section(tr!("knowledge.package.import")),
        note(tr!("knowledge.package.independent"), Tone::Muted),
    ];
    rows.push(button(
        tr!("knowledge.package.inspect"),
        form(
            json!({"archivePath":""}),
            vec![field(
                tr!("knowledge.package.importPickerTitle"),
                "/archivePath",
                false,
                false,
            )],
            |request| {
                knowledge::inspect_knowledge_package(
                    serde_json::from_value(request).expect("validated archive path form"),
                )
            },
        ),
        Tone::Normal,
    ));
    if let Some(library) = active_library(app) {
        rows.push(button(
            tr!("knowledge.package.export"),
            form(
                json!({"libraryId":s(library,"/id"),"destinationDirectory":""}),
                vec![field(
                    tr!("knowledge.package.exportPickerTitle"),
                    "/destinationDirectory",
                    false,
                    false,
                )],
                |request| {
                    knowledge::export_knowledge_package(
                        serde_json::from_value(request).expect("validated export path form"),
                    )
                },
            ),
            Tone::Normal,
        ));
    }
    if !ready(app, Slot::KnowledgePackages, &mut rows) {
        return rows;
    }
    // Native paths and roots come only from the host directory response.
    // 原生路径及根仅来自宿主目录响应。
    let listing = app.slots.value(Slot::KnowledgePackages);
    rows.push(info(
        tr!("projectFolderPicker.pathAria"),
        s(listing, "/directory/path"),
        Tone::Normal,
    ));
    if let Some(parent) = listing
        .pointer("/directory/parentPath")
        .and_then(Value::as_str)
    {
        rows.push(button(
            tr!("projectFolderPicker.parent"),
            UiAction::Knowledge(KnowledgeAction::PackageDirectory(Some(parent.into()))),
            Tone::Normal,
        ));
    }
    for entry in a(listing, "/directory/roots") {
        rows.push(button(
            s(entry, "/label"),
            UiAction::Knowledge(KnowledgeAction::PackageDirectory(Some(s(entry, "/path")))),
            Tone::Normal,
        ));
    }
    for entry in a(listing, "/directory/entries") {
        rows.push(button(
            s(entry, "/name"),
            UiAction::Knowledge(KnowledgeAction::PackageDirectory(Some(s(entry, "/path")))),
            Tone::Normal,
        ));
    }
    for package in a(listing, "/packages") {
        rows.push(
            item(
                &s(package, "/path"),
                s(package, "/name"),
                s(package, "/path"),
            )
            .actions(vec![action(
                tr!("knowledge.package.inspect"),
                call(knowledge::inspect_knowledge_package(
                    KnowledgePackageInspectRequest {
                        archive_path: s(package, "/path"),
                    },
                )),
                Tone::Accent,
            )]),
        );
    }
    rows
}
