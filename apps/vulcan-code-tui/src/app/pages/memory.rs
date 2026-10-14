// Native VMM pages read the exact data.value DTOs produced by runVmmMemoryAction.
// 原生 VMM 页面读取 runVmmMemoryAction 产出的准确 data.value DTO。

use super::*;

/// Maximum milliseconds admitted by the retention duration-to-nanoseconds contract.
/// 保留时长转纳秒协议准入的最大毫秒数。
const MAX_RETENTION_DURATION_MS: i64 = 9223372036854;
/// Maximum profile lifetime of 3650 days, shared by all lifetime levels.
/// 所有有效期级别共用的 3650 天画像有效期上限。
const MAX_PROFILE_DURATION_MS: i64 = 315360000000;
use crate::service_api::memory_actions as memory;

/// Returns the native value of a loaded VMM result, without remapping its schema.
/// 返回已加载 VMM 结果的原生值，不重新映射结构。
fn value(app: &App, slot: Slot) -> &Value {
    memory::value_of(app.slots.value(slot))
}

/// The shared project selector uses numeric VMM identities, independently of host project ids.
/// 共用项目选择器使用数值型 VMM 身份，与宿主项目标识独立。
fn project_selector(app: &App) -> Row {
    choice(
        tr!("tui.field.memoryProject"),
        a(value(app, Slot::VmmProjects), "/projects")
            .iter()
            .map(|project| (s(project, "/projectId"), s(project, "/name")))
            .collect(),
        app.vmm_project.map(|id| id.to_string()).unwrap_or_default(),
        Edit::Local(LocalSetting::VmmProject),
    )
}

/// Displays the native catalog and host bindings as authoritative service facts.
/// 将原生目录与宿主绑定显示为权威服务事实。
pub(super) fn overview(app: &App) -> Vec<Row> {
    vec![
        stats(vec![(
            tr!("tui.field.projects"),
            a(value(app, Slot::VmmProjects), "/projects")
                .len()
                .to_string(),
            Tone::Normal,
        )]),
        info(
            tr!("tui.field.projects"),
            a(value(app, Slot::VmmProjects), "/projects")
                .len()
                .to_string(),
            Tone::Normal,
        ),
        item("vmm-bindings", tr!("tui.field.bindings"), "")
            .lines(details(v(value(app, Slot::VmmProjects), "/bindings"))),
        button(
            tr!("tui.field.records"),
            UiAction::GoPage(PageId::MemoryContent),
            Tone::Normal,
        ),
        button(
            tr!("tui.field.maintenance"),
            UiAction::GoPage(PageId::MemoryMaintenance),
            Tone::Normal,
        ),
    ]
}

/// Lists provenance with session/turn removal previews; individual memory deletion is absent.
/// 列出来源并提供会话或轮次删除预检，不提供逐条记忆删除。
pub(super) fn content(app: &App) -> Vec<Row> {
    // Current operation rows.
    // 当前操作的行集合。
    let mut rows = vec![project_selector(app)];
    // Project id.
    // 准确宿主项目身份。
    let Some(project_id) = app.vmm_project else {
        return rows;
    };
    // Records.
    // 原生记录集合。
    let records = value(app, Slot::VmmRecords);
    // Query.
    // 准确搜索词。
    let query = app.memory_query.to_lowercase();
    rows.push(section(tr!("tui.field.sessions")));
    for session in a(records, "/sessions") {
        // Exact entity identity.
        // 准确实体身份。
        let id = s(session, "/sessionSourceId");
        rows.push(
            item(
                &format!("session-{id}"),
                s(session, "/displayTitle"),
                s(session, "/status"),
            )
            .lines(details(session))
            .actions(vec![action(
                tr!("tui.field.recycle"),
                call(memory::native_removal_preview(
                    project_id,
                    "sessions",
                    vec![json!(id)],
                )),
                Tone::Warn,
            )]),
        );
    }
    rows.push(section(tr!("tui.field.turns")));
    for turn in a(records, "/turns") {
        // Exact entity identity.
        // 准确实体身份。
        let id = s(turn, "/turnId");
        rows.push(
            item(
                &format!("turn-{id}"),
                s(turn, "/userPreview"),
                s(turn, "/status"),
            )
            .lines(details(turn))
            .actions(vec![action(
                tr!("tui.field.recycle"),
                call(memory::native_removal_preview(
                    project_id,
                    "turns",
                    vec![json!(id)],
                )),
                Tone::Warn,
            )]),
        );
    }
    rows.push(section(tr!("tui.field.memories")));
    for entry in a(records, "/memories").iter().filter(|entry| {
        query.is_empty()
            || s(entry, "/summary").to_lowercase().contains(&query)
            || s(entry, "/content").to_lowercase().contains(&query)
    }) {
        rows.push(
            item(
                &format!("memory-{}", s(entry, "/memoryId")),
                s(entry, "/summary"),
                s(entry, "/state"),
            )
            .lines(details(entry)),
        );
    }
    rows
}

/// Presents all native profile states and their source turns for the chosen target.
/// 显示所选目标全部原生画像状态及来源轮次。
pub(super) fn profile(app: &App) -> Vec<Row> {
    // Current operation rows.
    // 当前操作的行集合。
    let mut rows = vec![
        project_selector(app),
        choice(
            tr!("tui.field.profileTarget"),
            vec![
                ("user".into(), tr!("tui.field.userProfile")),
                ("project".into(), tr!("tui.field.projectProfile")),
            ],
            app.profile_target.clone(),
            Edit::Local(LocalSetting::ProfileTarget),
        ),
    ];
    // Profile.
    // 声明的消费者或凭据配置。
    let profile = value(app, Slot::VmmProfile);
    rows.push(
        item("profile-counts", tr!("tui.field.status"), "").lines(details(v(profile, "/counts"))),
    );
    for node in a(profile, "/nodes") {
        rows.push(
            item(
                &format!("profile-{}", s(node, "/profileNodeId")),
                s(node, "/content"),
                s(node, "/effectiveState"),
            )
            .lines(details(node)),
        );
    }
    rows
}

/// Creates and binds a native project using the exact host project identifier captured by its row.
/// 使用条目捕获的准确宿主项目标识创建并绑定原生项目。
fn create_project(request: Value) -> ServiceCall {
    memory::native_project_create_and_bind(
        &s(&request, "/vulcanProjectId"),
        &s(&request, "/projectName"),
    )
}

/// Builds catalog-owned migration targets, retaining explicit source and target ids.
/// 构建目录持有的迁移目标，保留明确来源与目标标识。
fn migrate_project(context: &Value, value: &SettingValue) -> Option<ServiceCall> {
    Some(memory::native_project_migration_preview(
        context.as_i64()?,
        value.as_str().parse().ok()?,
    ))
}

/// Offers rename/create and read-only previews before clear/delete/migration.
/// 提供重命名与创建，并在清空、删除或迁移前执行只读预检。
pub(super) fn projects(app: &App) -> Vec<Row> {
    // Current operation rows.
    // 当前操作的行集合。
    let mut rows = Vec::new();
    for host in &app.projects {
        rows.push(button(
            format!("{} · {}", tr!("tui.field.createAndBind"), host.name),
            form(
                json!({"vulcanProjectId":host.id,"projectName":host.name}),
                vec![field(
                    tr!("tui.field.displayName"),
                    "/projectName",
                    false,
                    false,
                )],
                create_project,
            ),
            Tone::Accent,
        ));
    }
    // Projects.
    // 项目目录。
    let projects = a(value(app, Slot::VmmProjects), "/projects");
    for project in projects {
        // Exact entity identity.
        // 准确实体身份。
        let Some(id) = v(project, "/projectId").as_i64() else {
            continue;
        };
        rows.push(text(
            s(project, "/name"),
            s(project, "/name"),
            dto(project.clone(), "/name", |request| {
                memory::native_project_rename(n(&request, "/projectId"), &s(&request, "/name"))
            }),
        ));
        // Actions.
        // 声明的操作集合。
        let mut actions = vec![action(
            tr!("tui.field.clear"),
            call(memory::native_project_clear_preview(id)),
            Tone::Warn,
        )];
        if !b(project, "/protected") {
            actions.push(action(
                tr!("tui.field.remove"),
                call(memory::native_project_delete_preview(id)),
                Tone::Bad,
            ));
        }
        rows.push(
            item(
                &format!("project-{id}"),
                s(project, "/name"),
                s(project, "/tag"),
            )
            .lines(details(project))
            .actions(actions),
        );
        rows.push(choice(
            tr!("tui.field.migrate"),
            projects
                .iter()
                .filter(|target| v(target, "/projectId").as_i64() != Some(id))
                .map(|target| (s(target, "/projectId"), s(target, "/name")))
                .collect(),
            String::new(),
            Edit::Call(Setter::Build {
                ctx: json!(id),
                build: migrate_project,
            }),
        ));
    }
    rows
}

/// Shows vector/lexical/retention status and version-bound recycle batches.
/// 显示向量、全文与保留状态，并展示带版本的回收批次。
pub(super) fn maintenance(app: &App) -> Vec<Row> {
    // Current operation rows.
    // 当前操作的行集合。
    let mut rows = Vec::new();
    for (slot, label) in [
        (Slot::VmmVector, tr!("tui.field.vectorStatus")),
        (Slot::VmmLexical, tr!("tui.field.lexicalStatus")),
        (Slot::VmmRetention, tr!("tui.field.retentionStatus")),
    ] {
        rows.push(item(&format!("{slot:?}"), label, "").lines(details(value(app, slot))));
    }
    rows.push(button(
        tr!("tui.field.vectorRebuild"),
        call(memory::native_vector_rebuild_preview()),
        Tone::Warn,
    ));
    rows.push(button(
        tr!("memory.maintenance.keepVectors"),
        call(memory::native_vector_adoption_preview()),
        Tone::Warn,
    ));
    if let Some(version) = v(value(app, Slot::VmmLexical), "/storageVersion").as_i64() {
        rows.push(button(
            tr!("tui.field.lexicalRebuild"),
            confirm(
                tr!("tui.field.lexicalRebuild"),
                format!("{}: {version}", tr!("tui.field.storageVersion")),
                tr!("tui.field.confirm"),
                call(memory::native_lexical_rebuild(version)),
            ),
            Tone::Warn,
        ));
    }
    rows.push(button(
        tr!("tui.field.retentionRun"),
        call(memory::native_retention_preview()),
        Tone::Warn,
    ));
    // Recycle.
    // 回收站权威数据。
    let recycle = value(app, Slot::VmmRecycleBin);
    if let Some(version) = v(recycle, "/storageVersion").as_i64() {
        for batch in a(recycle, "/batches") {
            // Exact entity identity.
            // 准确实体身份。
            let id = s(batch, "/batchId");
            rows.push(
                item(&id, id.clone(), s(batch, "/reason"))
                    .lines(details(batch))
                    .actions(vec![
                        action(
                            tr!("tui.field.restore"),
                            confirm(
                                tr!("tui.field.restore"),
                                id.clone(),
                                tr!("tui.field.confirm"),
                                call(memory::native_recycle_restore(&id, version)),
                            ),
                            Tone::Good,
                        ),
                        action(
                            tr!("tui.field.purge"),
                            confirm(
                                tr!("tui.field.purge"),
                                id.clone(),
                                tr!("tui.field.confirm"),
                                call(memory::native_recycle_purge(&id, version)),
                            ),
                            Tone::Bad,
                        ),
                    ]),
            );
        }
    }
    rows
}

/// Edits the native recall/extraction group with the same closed ranges as the WebUI.
/// 使用与 WebUI 相同的封闭范围编辑原生召回及提炼配置组。
pub(super) fn rules(app: &App) -> Vec<Row> {
    // Policy.
    // 原生策略文档。
    let policy = v(value(app, Slot::VmmRuntimePolicy), "/recall");
    // Current operation rows.
    // 当前操作的行集合。
    let mut rows = Vec::new();
    if !policy.is_object() {
        return rows;
    }
    for (pointer, label, min, max) in [
        (
            "/precheckHistoryTurnLimit",
            tr!("memory.policy.precheckHistory"),
            1,
            64,
        ),
        (
            "/precheckMaxInputTokens",
            tr!("memory.policy.precheckBudget"),
            1,
            131072,
        ),
        (
            "/precheckRecallTopK",
            tr!("memory.policy.recallTopK"),
            1,
            32,
        ),
        (
            "/precheckReviewCandidateLimit",
            tr!("memory.policy.reviewLimit"),
            1,
            256,
        ),
        ("/maxSearchQueries", tr!("memory.policy.queryLimit"), 1, 64),
        (
            "/postactionHistoryTurnLimit",
            tr!("memory.policy.postactionHistory"),
            1,
            64,
        ),
        (
            "/postactionMaxInputTokens",
            tr!("memory.policy.postactionBudget"),
            1,
            131072,
        ),
        (
            "/postactionMemoryPriority",
            tr!("memory.policy.extractedPriority"),
            0,
            100,
        ),
        (
            "/postactionMemoryRefreshWeight",
            tr!("memory.policy.extractedWeight"),
            1,
            1000000,
        ),
        (
            "/postactionMemoryExpiryMs",
            tr!("memory.policy.extractedExpiry"),
            1,
            315360000000,
        ),
    ] {
        rows.push(number(
            label,
            n(policy, pointer),
            (min, max, 1),
            dto(
                policy.clone(),
                pointer,
                memory::native_runtime_policy_update,
            ),
        ));
    }
    rows.push(decimal(
        tr!("memory.policy.reviewScore"),
        policy,
        "/precheckMinReviewScore",
        0.,
        1.,
        memory::native_runtime_policy_update,
    ));
    rows.push(choice(
        tr!("memory.policy.extractedStability"),
        vec![
            ("working".into(), tr!("memory.policy.working")),
            ("stable".into(), tr!("memory.policy.stable")),
            ("archival".into(), tr!("memory.policy.archival")),
        ],
        s(policy, "/postactionMemoryStability"),
        dto(
            policy.clone(),
            "/postactionMemoryStability",
            memory::native_runtime_policy_update,
        ),
    ));
    rows.push(choice(
        tr!("memory.policy.extractedLevel"),
        vec![
            ("session".into(), tr!("memory.nativeRecords.levelSession")),
            ("phase".into(), tr!("memory.nativeRecords.levelPhase")),
            ("stable".into(), tr!("memory.nativeRecords.levelStable")),
            (
                "persistent".into(),
                tr!("memory.nativeRecords.levelPersistent"),
            ),
        ],
        s(policy, "/postactionMemoryLevel"),
        dto(
            policy.clone(),
            "/postactionMemoryLevel",
            memory::native_runtime_policy_update,
        ),
    ));
    rows
}

/// Uses the shared catalog-backed VMM inference controls.
/// 使用共用的基于目录的 VMM 推理控件。
pub(super) fn inference(app: &App) -> Vec<Row> {
    super::inference::rows(app, "vmm")
}

/// Displays native resource composition and guarded effective-file editing.
/// 显示原生资源组合与受保护的生效文件编辑。
pub(super) fn resources(app: &App) -> Vec<Row> {
    // Catalog.
    // 服务发布的目录。
    let catalog = value(app, Slot::VmmResources);
    // Settings.
    // 权威设置文档。
    let settings = v(catalog, "/settings");
    // Current operation rows.
    // 当前操作的行集合。
    let mut rows = vec![info(
        tr!("tui.field.overrideDirectory"),
        s(catalog, "/overrideRoot"),
        Tone::Muted,
    )];
    if settings.is_object() {
        for (pointer, options, label) in [
            (
                "/promptBundle",
                "/promptBundles",
                tr!("tui.field.promptBundle"),
            ),
            (
                "/piiLanguage",
                "/piiLanguages",
                tr!("tui.field.piiLanguage"),
            ),
            (
                "/noiseLanguage",
                "/noiseLanguages",
                tr!("tui.field.noiseLanguage"),
            ),
        ] {
            rows.push(choice(
                label,
                a(catalog, options)
                    .iter()
                    .filter_map(Value::as_str)
                    .map(|value| (value.into(), value.into()))
                    .collect(),
                s(settings, pointer),
                dto(
                    settings.clone(),
                    pointer,
                    memory::native_resource_settings_update,
                ),
            ));
        }
        for (pointer, label) in [
            ("/noiseEnabled", tr!("tui.field.noiseEnabled")),
            ("/noiseSemanticEnabled", tr!("tui.field.semanticNoise")),
        ] {
            rows.push(toggle(
                label,
                b(settings, pointer),
                dto(
                    settings.clone(),
                    pointer,
                    memory::native_resource_settings_update,
                ),
            ));
        }
        rows.push(decimal(
            tr!("tui.field.noiseThreshold"),
            settings,
            "/noiseSemanticThreshold",
            0.,
            1.,
            memory::native_resource_settings_update,
        ));
    }
    for file in a(catalog, "/files") {
        // Exact path.
        // 准确路径。
        let path = s(file, "/path");
        rows.push(
            item(&path, s(file, "/fileName"), s(file, "/group"))
                .lines(details(file))
                .actions(vec![action(
                    tr!("tui.field.view"),
                    call(memory::native_resource_read(&path)),
                    Tone::Normal,
                )]),
        );
    }
    // File.
    // 当前文件数据。
    let file = value(app, Slot::VmmResourceFile);
    if let Some(path) = v(file, "/path").as_str() {
        rows.push(note(s(file, "/effectiveText"), Tone::Normal));
        rows.push(button(
            tr!("tui.field.editResource"),
            confirm(
                tr!("tui.field.editResource"),
                tr!("tui.field.resourcePurpose"),
                tr!("tui.field.confirm"),
                UiAction::Prompt(Box::new(PromptDialog::new(
                    tr!("tui.field.content"),
                    path.into(),
                    s(file, "/effectiveText"),
                    PromptThen::ResourceWrite(path.into()),
                    false,
                ))),
            ),
            Tone::Warn,
        ));
        if b(file, "/hasUserOverride") {
            rows.push(button(
                tr!("tui.field.restoreBuiltIn"),
                confirm(
                    tr!("tui.field.restoreBuiltIn"),
                    path.into(),
                    tr!("tui.field.confirm"),
                    call(memory::native_resource_reset(path)),
                ),
                Tone::Warn,
            ));
        }
    }
    rows
}

/// Edits native ranking, retention, text reduction and profile lifetimes as complete groups.
/// 以完整配置组编辑原生排序、保留、文本缩减与画像有效期。
pub(super) fn advanced(app: &App) -> Vec<Row> {
    // Policy.
    // 原生策略文档。
    let policy = value(app, Slot::VmmRuntimePolicy);
    // Current operation rows.
    // 当前操作的行集合。
    let mut rows = Vec::new();
    // Retrieval.
    // 原生排序策略组。
    let retrieval = v(policy, "/retrieval");
    if retrieval.is_object() {
        rows.push(section(tr!("tui.field.retrieval")));
        for (pointer, label) in [
            ("/hybridEnabled", tr!("tui.field.hybrid")),
            ("/mmrEnabled", tr!("tui.field.mmr")),
            ("/weibullEnabled", tr!("tui.field.decay")),
        ] {
            rows.push(toggle(
                label,
                b(retrieval, pointer),
                dto(
                    retrieval.clone(),
                    pointer,
                    memory::native_retrieval_policy_update,
                ),
            ));
        }
        for (pointer, label, min) in [
            ("/lexicalCandidateFloor", tr!("tui.field.lexicalFloor"), 1),
            ("/hardDedupePoolTopK", tr!("tui.field.dedupePool"), 1),
            ("/replacementTopK", tr!("tui.field.replacementPool"), 1),
            ("/rerankTopN", tr!("tui.field.rerankPool"), 0),
        ] {
            rows.push(number(
                label,
                n(retrieval, pointer),
                (min, 32, 1),
                dto(
                    retrieval.clone(),
                    pointer,
                    memory::native_retrieval_policy_update,
                ),
            ));
        }
        rows.push(text(
            tr!("tui.field.rrfK"),
            s(retrieval, "/rrfK"),
            Edit::Call(Setter::Build {
                ctx: retrieval.clone(),
                build: |context, value| {
                    if value.as_str().starts_with('0')
                        || !value.as_str().bytes().all(|byte| byte.is_ascii_digit())
                    {
                        return None;
                    }
                    // Parsed.
                    // 通过解析的输入值。
                    let parsed: i64 = value.as_str().parse().ok()?;
                    if parsed <= 0 {
                        return None;
                    }
                    // Complete document preserving sibling fields.
                    // 保留同级字段的完整文档。
                    let mut document = context.clone();
                    document["rrfK"] = json!(parsed.to_string());
                    Some(memory::native_retrieval_policy_update(document))
                },
            }),
        ));
        for (pointer, label, min, max) in [
            (
                "/replaceMinSimilarity",
                tr!("tui.field.replaceSimilarity"),
                0.,
                1.,
            ),
            (
                "/hardDedupeCosineThreshold",
                tr!("tui.field.dedupeCosine"),
                0.,
                1.,
            ),
            ("/mmrLambda", tr!("tui.field.mmrLambda"), 0., 1.),
            (
                "/weibullShape",
                tr!("tui.field.decayShape"),
                f64::from_bits(1),
                f64::MAX,
            ),
            (
                "/weibullScaleHours",
                tr!("tui.field.decayScale"),
                f64::from_bits(1),
                f64::MAX,
            ),
            ("/weibullMinMultiplier", tr!("tui.field.decayMin"), 0., 1.),
            (
                "/weibullReinforceWeight",
                tr!("tui.field.reinforceWeight"),
                0.,
                f64::MAX,
            ),
            (
                "/weibullCrossSessionBoost",
                tr!("tui.field.crossSessionBoost"),
                0.,
                f64::MAX,
            ),
        ] {
            rows.push(decimal(
                label,
                retrieval,
                pointer,
                min,
                max,
                memory::native_retrieval_policy_update,
            ));
        }
    }
    // Retention.
    // 保留策略组。
    let retention = v(policy, "/retention");
    if retention.is_object() {
        rows.push(section(tr!("tui.field.retention")));
        for (pointer, label) in [
            ("/enabled", tr!("tui.field.enabled")),
            (
                "/skipProtectedSharedMemories",
                tr!("tui.field.protectShared"),
            ),
        ] {
            rows.push(toggle(
                label,
                b(retention, pointer),
                dto(
                    retention.clone(),
                    pointer,
                    memory::native_retention_settings_update,
                ),
            ));
        }
        for (pointer, label, min, max) in [
            (
                "/scanIntervalMinutes",
                tr!("tui.field.scanInterval"),
                1,
                u32::MAX.into(),
            ),
            (
                "/profileExpiryIntervalSeconds",
                tr!("tui.field.profileScanInterval"),
                1,
                u32::MAX.into(),
            ),
            (
                "/turnHotWindowSize",
                tr!("tui.field.hotWindow"),
                1,
                u32::MAX.into(),
            ),
            (
                "/sessionIdleMs",
                tr!("tui.field.sessionIdleMs"),
                1,
                MAX_RETENTION_DURATION_MS,
            ),
            (
                "/trashRetentionMs",
                tr!("tui.field.trashRetentionMs"),
                1,
                MAX_RETENTION_DURATION_MS,
            ),
            (
                "/protectedPriorityFloor",
                tr!("tui.field.priorityFloor"),
                0,
                100,
            ),
            (
                "/protectedMemoryLevelFloor",
                tr!("tui.field.levelFloor"),
                1,
                4,
            ),
        ] {
            rows.push(number(
                label,
                n(retention, pointer),
                (min, max, 1),
                dto(
                    retention.clone(),
                    pointer,
                    memory::native_retention_settings_update,
                ),
            ));
        }
    }
    // Text policy.
    // 原生文本清洗策略组。
    let text_policy = v(policy, "/text");
    if text_policy.is_object() {
        rows.push(section(tr!("tui.field.textReduction")));
        rows.push(toggle(
            tr!("tui.field.foldMachine"),
            b(text_policy, "/foldMachineText"),
            dto(
                text_policy.clone(),
                "/foldMachineText",
                memory::native_text_policy_update,
            ),
        ));
        rows.push(number(
            tr!("tui.field.fragmentTokens"),
            n(text_policy, "/maxFragmentTokens"),
            (256, 131072, 1),
            dto(
                text_policy.clone(),
                "/maxFragmentTokens",
                memory::native_text_policy_update,
            ),
        ));
    }
    // Profiles.
    // 原生画像有效期配置组。
    let profiles = v(policy, "/profiles");
    if profiles.is_object() {
        rows.push(section(tr!("tui.field.profileLifetimes")));
        for (target, label) in [
            ("user", tr!("tui.field.userProfile")),
            ("project", tr!("tui.field.projectProfile")),
        ] {
            rows.push(section(label));
            for (level, label) in [
                ("transient", tr!("tui.field.transient")),
                ("situational", tr!("tui.field.situational")),
                ("stable", tr!("tui.field.stable")),
            ] {
                for (key, part) in [
                    ("baseMs", tr!("tui.field.baseMs")),
                    ("stepMs", tr!("tui.field.stepMs")),
                    ("maxMs", tr!("tui.field.maxMs")),
                ] {
                    // Pointer.
                    // 已确认 JSON 指针。
                    let pointer = format!("/{target}/{level}/{key}");
                    rows.push(number(
                        format!("{label} · {part}"),
                        n(profiles, &pointer),
                        (if key == "stepMs" {0} else {1},MAX_PROFILE_DURATION_MS,1),
                        Edit::Call(Setter::Build{
                            ctx:json!({"profiles":profiles,"pointer":pointer,"rule":format!("/{target}/{level}")}),
                            build:|context,value| {
                                // Complete document preserving sibling fields.
                                // 保留同级字段的完整文档。
                                let mut document = context.get("profiles")?.clone();
                                *document.pointer_mut(context.get("pointer")?.as_str()?)? = json!(value.as_str().parse::<i64>().ok()?);
                                // Rule.
                                // 当前画像有效期规则。
                                let rule = document.pointer(context.get("rule")?.as_str()?)?;
                                if rule.get("baseMs")?.as_i64()? > rule.get("maxMs")?.as_i64()? {return None;}
                                Some(memory::native_profile_policy_update(document))
                            }
                        }),
                    ));
                }
            }
        }
    }
    rows
}
