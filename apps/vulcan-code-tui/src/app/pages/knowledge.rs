// Knowledge management uses host-owned library, source, job and search DTOs.
// 知识库管理使用宿主持有的知识库、来源、任务与检索 DTO。

use super::*;
use crate::app::action::LocalSetting;
use api::knowledge_base as knowledge;
/// Maps the declared local view key to its exact knowledge page.
/// 将已声明本地视图键映射到准确知识库页面。
/// `key` is produced by view_options; returns None for an unknown destination.
/// `key` 由 view_options 产生；未知目标返回 None。
pub fn knowledge_view(key: &str) -> Option<PageId> {
    match key {
        "overview" => Some(PageId::KnowledgeLibrary),
        "documents" => Some(PageId::KnowledgeDocuments),
        "search" => Some(PageId::KnowledgeSearch),
        "settings" => Some(PageId::KnowledgeSettings),
        "sources" => Some(PageId::KnowledgeSources),
        "bindings" => Some(PageId::KnowledgeBindings),
        "maintenance" => Some(PageId::KnowledgeMaintenance),
        _ => None,
    }
}

/// Produces the ordered library views with translated labels.
/// 产生带翻译标签的有序知识库视图。
/// Returns selector keys shared with knowledge_view.
/// 返回与 knowledge_view 共用的选择器键。
fn view_options() -> Vec<(String, String)> {
    [
        "overview",
        "documents",
        "search",
        "sources",
        "bindings",
        "settings",
        "maintenance",
    ]
    .into_iter()
    .map(|key| {
        (
            key.into(),
            knowledge_view(key)
                .expect("declared knowledge view")
                .label(),
        )
    })
    .collect()
}

/// Builds the common library name and view selector, or an actionable empty-library message.
/// 构建共用知识库名称与视图选择器，或可操作的空知识库提示。
/// `app` supplies the selected library; `page` chooses the current view; returns declarative rows.
/// `app` 提供所选知识库；`page` 选择当前视图；返回声明式行。
fn header(app: &App, page: PageId) -> Vec<Row> {
    let Some(library) = selected(app) else {
        return vec![
            note(tr!("tui.knowledge.selectLibrary"), Tone::Muted),
            button(
                tr!("tui.knowledge.catalog"),
                UiAction::GoPage(PageId::KnowledgeBases),
                Tone::Accent,
            ),
        ];
    };
    // A document reader belongs to the documents view, while preserving its independent reader page.
    // 文档阅读属于文档视图，同时保留独立阅读页面。
    let active = if page == PageId::KnowledgeDocument {
        PageId::KnowledgeDocuments
    } else {
        page
    };
    // Exact local view key, never a service-owned category or library index.
    // 准确本地视图键，不使用服务分类或知识库下标。
    let key = view_options()
        .into_iter()
        .find(|(key, _)| knowledge_view(key) == Some(active))
        .map(|(key, _)| key)
        .expect("knowledge detail view");
    vec![
        section(s(library, "/name")),
        note(s(library, "/description"), Tone::Muted),
        choice(
            tr!("tui.knowledge.view"),
            view_options(),
            key,
            Edit::Local(LocalSetting::KnowledgeView),
        ),
    ]
}

/// Finds the library with the exact selected id; never substitutes another library.
/// 按精确的选中标识查找知识库，绝不替换为其他知识库。
fn selected(app: &App) -> Option<&Value> {
    // Exact entity identity.
    // 准确实体身份。
    let id = app.library.as_deref()?;
    a(app.slots.value(Slot::KnowledgeLibraries), "")
        .iter()
        .find(|library| s(library, "/id") == id)
}

/// Converts one immutable catalog route to the knowledge service's credential-free reference.
/// 将不可变目录路由转换为知识库服务不含凭据的引用。
fn knowledge_route(candidate: &Value) -> Option<Value> {
    // Route.
    // 目录中的推理路由。
    let route = candidate.get("route")?;
    // Binding.
    // 已声明绑定。
    let binding = route.get("binding")?;
    // Protocol.
    // 已声明协议绑定。
    let protocol = match binding.get("kind")?.as_str()? {
        "protocol" => binding.get("protocol_profile_id")?.as_str()?,
        "action" => binding.get("action_binding_id")?.as_str()?,
        _ => return None,
    };
    Some(
        json!({"consumerProfileId":"knowledge_base","providerId":route.get("provider_id")?.as_str()?,
        "serviceEntryId":route.get("service_entry_id")?.as_str()?,"connectionId":route.get("connection_id")?.as_str()?,
        "modelId":route.get("model_id")?.as_str()?,"bindingKind":binding.get("kind")?,
        "protocol":protocol,"usageScope":"internal_background"}),
    )
}

/// Adds verified inference choices and operation-specific scalar controls to one library.
/// 为单个知识库追加已核实的推理选项与操作专属标量控件。
fn inference_rows(app: &App, library: &Value) -> Vec<Row> {
    // Exact entity identity.
    // 准确实体身份。
    let id = s(library, "/id");
    // Public AI settings.
    // 公开 AI 设置。
    let ai = app.slots.value(Slot::PublicAiSettings);
    // Current operation rows.
    // 当前操作的行集合。
    let mut rows = Vec::new();
    for (group, operation, label) in [
        (
            "curation",
            "conversation_respond",
            tr!("tui.field.curationRoute"),
        ),
        (
            "embedding",
            "embedding_create",
            tr!("tui.field.embeddingRoute"),
        ),
    ] {
        // Current.
        // 当前选中或已读取值。
        let current = v(library, &format!("/settings/{group}/route"));
        // Candidates.
        // 目录发布的候选集合。
        let candidates: Vec<_> = a(ai, "/internalInferenceCandidates")
            .iter()
            .filter(|candidate| {
                b(candidate, "/enabled")
                    && s(candidate, "/route/usage_scope") == "internal_background"
                    && s(candidate, "/route/operation") == operation
                    && (group == "curation"
                        || super::inference::dimension(v(candidate, "/capabilities/embedding"))
                            .is_some())
            })
            .filter_map(|candidate| {
                knowledge_route(candidate).map(|reference| (candidate, reference))
            })
            .collect();
        // Options.
        // 允许的选择项。
        let mut options = vec![("null".into(), tr!("tui.field.unconfigured"))];
        for (candidate, reference) in &candidates {
            options.push((
                reference.to_string(),
                format!(
                    "{} / {} / {}",
                    s(candidate, "/providerDisplayName"),
                    s(candidate, "/connectionDisplayName"),
                    s(candidate, "/modelDisplayName")
                ),
            ));
        }
        rows.push(choice(label,options,current.to_string(),Edit::Call(Setter::Build{
            ctx:json!({"id":id,"group":group,"candidates":candidates.iter().map(|(candidate,reference)|json!({"candidate":candidate,"reference":reference})).collect::<Vec<_>>()}),
            build:|context,value| {
                // Reference.
                // 准确知识推理路由引用。
                let reference: Value = serde_json::from_str(value.as_str()).ok()?;
                // Group.
                // 当前配置组。
                let group = context.get("group")?.as_str()?;
                // Settings.
                // 权威设置文档。
                let mut settings = json!({});
                settings[group] = json!({"route":reference});
                if !reference.is_null() {
                    // Candidate.
                    // 服务公开的候选项。
                    let candidate = context.get("candidates")?.as_array()?.iter()
                        .find(|entry|entry.get("reference") == Some(&reference))?.get("candidate")?;
                    if group == "embedding" {
                        // Capability.
                        // 模型声明的能力。
                        let capability = candidate.pointer("/capabilities/embedding")?;
                        settings[group]["configuredDimension"] = json!(super::inference::dimension(capability)?);
                        settings[group]["modelMaxBatchSize"] = capability.get("maximum_inputs").cloned().unwrap_or(Value::Null);
                    } else {
                        settings[group]["promptVersion"] = json!("knowledge-display-v1");
                        settings[group]["reasoningEffort"] = candidate.pointer("/capabilities/reasoning_efforts")?.as_array()?
                            .first().cloned().unwrap_or(Value::Null);
                    }
                } else if group == "embedding" {settings[group]["configuredDimension"] = Value::Null;}
                Some(knowledge::update_knowledge_library_settings(context.get("id")?.as_str()?,settings))
            }
        })));
        if let Some((candidate, _)) = candidates
            .iter()
            .find(|(_, reference)| reference == current)
        {
            if group == "embedding" {
                // Capability.
                // 模型声明的能力。
                let capability = v(candidate, "/capabilities/embedding");
                rows.push(text(
                    tr!("tui.field.embeddingDimension"),
                    v(library, "/settings/embedding/configuredDimension").to_string(),
                    Edit::Call(Setter::Build {
                        ctx: json!({"id":id,"capability":capability}),
                        build: |context, value| {
                            // Dimension.
                            // 明确的向量维度。
                            let dimension: u32 = value.as_str().parse().ok()?;
                            if !super::inference::dimension_valid(
                                context.get("capability")?,
                                dimension,
                            ) {
                                return None;
                            }
                            Some(knowledge::update_knowledge_library_settings(
                                context.get("id")?.as_str()?,
                                json!({"embedding":{"configuredDimension":dimension}}),
                            ))
                        },
                    }),
                ));
                // Maximum.
                // 协议允许的最大值。
                let maximum = v(capability, "/maximum_inputs")
                    .as_u64()
                    .map(|value| value.min(u64::from(u32::MAX)))
                    .unwrap_or(u64::from(u32::MAX));
                rows.push(number(tr!("tui.field.embeddingBatch"),n(library,"/settings/embedding/embeddingBatchSize"),(1,maximum as i64,1),
                    Edit::Call(Setter::Build{ctx:json!({"id":id}),build:|context,value|Some(knowledge::update_knowledge_library_settings(
                        context.get("id")?.as_str()?,json!({"embedding":{"requestedBatchSize":value.as_str().parse::<u32>().ok()?}})))})));
            } else {
                // Options.
                // 允许的选择项。
                let options = a(candidate, "/capabilities/reasoning_efforts")
                    .iter()
                    .filter_map(Value::as_str)
                    .map(|effort| (effort.to_string(), effort.to_string()))
                    .collect::<Vec<_>>();
                if !options.is_empty() {
                    rows.push(choice(
                        tr!("tui.field.reasoningEffort"),
                        options,
                        s(library, "/settings/curation/reasoningEffort"),
                        Edit::Call(Setter::Build {
                            ctx: json!({"id":id}),
                            build: |context, value| {
                                Some(knowledge::update_knowledge_library_settings(
                                    context.get("id")?.as_str()?,
                                    json!({"curation":{"reasoningEffort":value.as_str()}}),
                                ))
                            },
                        }),
                    ));
                }
            }
        }
    }
    rows
}

/// Lists libraries with create, open, rename and confirmed delete actions.
/// 列出知识库，提供创建、打开、重命名与确认删除动作。
pub(super) fn libraries(app: &App) -> Vec<Row> {
    // Current operation rows.
    // 当前操作的行集合。
    let mut rows = vec![button(
        tr!("tui.field.createLibrary"),
        form(
            json!({"name":"","description":""}),
            vec![
                field(tr!("tui.field.displayName"), "/name", false, false),
                field(tr!("tui.field.description"), "/description", false, true),
            ],
            |request| {
                knowledge::create_knowledge_library(
                    &s(&request, "/name"),
                    &s(&request, "/description"),
                    &[],
                    json!({}),
                )
            },
        ),
        Tone::Accent,
    )];
    for library in a(app.slots.value(Slot::KnowledgeLibraries), "") {
        // Exact entity identity.
        // 准确实体身份。
        let id = s(library, "/id");
        rows.push(
            item(&id, s(library, "/name"), s(library, "/description"))
                .badge(
                    format!(
                        "{} / {}",
                        n(library, "/documentCount"),
                        n(library, "/chunkCount")
                    ),
                    Tone::Muted,
                )
                .lines(details(library))
                .actions(vec![
                    action(
                        tr!("tui.field.view"),
                        UiAction::OpenLibrary(id.clone()),
                        Tone::Accent,
                    ),
                    action(
                        tr!("tui.field.remove"),
                        confirm(
                            tr!("tui.field.remove"),
                            s(library, "/name"),
                            tr!("tui.field.remove"),
                            call(knowledge::delete_knowledge_library(&id)),
                        ),
                        Tone::Bad,
                    ),
                ]),
        );
    }
    rows
}

/// Builds the selected library overview with readable content counts, health and task progress.
/// 构建所选知识库概览，提供可读内容计数、健康状态及任务进度。
/// `app` supplies the exact library and scoped observations; returns overview rows.
/// `app` 提供准确知识库及限定范围观察；返回概览行。
pub(super) fn library(app: &App) -> Vec<Row> {
    // Overview keeps content and health above operations instead of burying them beneath settings.
    // 概览将内容及健康状态置于操作上方，不隐藏在设置后面。
    let mut rows = header(app, PageId::KnowledgeLibrary);
    let Some(library) = selected(app) else {
        return rows;
    };
    // Catalog library identity owns every operation in this header.
    // 目录知识库身份持有此标题内的全部操作。
    let id = s(library, "/id");
    rows.push(stats(vec![
        (
            tr!("tui.knowledge.documents"),
            n(library, "/documentCount").to_string(),
            Tone::Normal,
        ),
        (
            tr!("tui.knowledge.chunks"),
            n(library, "/chunkCount").to_string(),
            Tone::Normal,
        ),
        (
            tr!("tui.knowledge.indexStatus"),
            s(library, "/indexStatus"),
            Tone::Accent,
        ),
    ]));
    rows.push(
        item("library-actions", tr!("tui.knowledge.actions"), "").actions(vec![
            action(
                tr!("tui.knowledge.documents"),
                UiAction::GoPage(PageId::KnowledgeDocuments),
                Tone::Accent,
            ),
            action(
                tr!("knowledge.search"),
                UiAction::GoPage(PageId::KnowledgeSearch),
                Tone::Normal,
            ),
            action(
                tr!("tui.field.importDirectory"),
                UiAction::AddKnowledgeSource(id.clone()),
                Tone::Normal,
            ),
            action(
                tr!("tui.knowledge.settings"),
                UiAction::GoPage(PageId::KnowledgeSettings),
                Tone::Normal,
            ),
            action(
                tr!("tui.field.remove"),
                confirm(
                    tr!("tui.field.remove"),
                    s(library, "/name"),
                    tr!("tui.field.remove"),
                    call(knowledge::delete_knowledge_library(&id)),
                ),
                Tone::Bad,
            ),
        ]),
    );
    rows.push(info(
        tr!("tui.field.retrievalMode"),
        retrieval_label(&s(library, "/settings/retrievalMode")),
        Tone::Normal,
    ));
    rows.extend(health_rows(app));
    rows.extend(job_rows(app));
    rows
}

/// Searches the selected library through its service, retaining citation and failure details.
/// 通过服务检索选中知识库，保留引用与失败详情。
pub(super) fn search(app: &App) -> Vec<Row> {
    // Search belongs to the selected library identity and retains a visible query.
    // 搜索属于所选知识库身份，并保留可见查询。
    let mut rows = header(app, PageId::KnowledgeSearch);
    if selected(app).is_none() {
        return rows;
    }
    rows.push(button(
        tr!("knowledge.search"),
        UiAction::Prompt(Box::new(PromptDialog::new(
            tr!("knowledge.search"),
            String::new(),
            app.knowledge_query.clone(),
            PromptThen::KnowledgeSearch,
            false,
        ))),
        Tone::Accent,
    ));
    rows.push(info(
        tr!("tui.catalog.query"),
        app.knowledge_query.clone(),
        Tone::Normal,
    ));
    if app.knowledge_query.is_empty() {
        rows.push(note(tr!("tui.knowledge.searchHint"), Tone::Muted));
        return rows;
    }
    if !app.slots.loaded(Slot::KnowledgeSearch) {
        if !app.slot_errors.contains_key(&Slot::KnowledgeSearch) {
            rows.push(note(tr!("tui.page.loading"), Tone::Muted));
        }
        return rows;
    }
    // Authoritative result includes exact document identities and source citations.
    // 权威结果包含准确文档身份及来源引用。
    let result = app.slots.value(Slot::KnowledgeSearch);
    if a(result, "/hits").is_empty() {
        rows.push(note(tr!("tui.knowledge.noResults"), Tone::Muted));
    }
    for hit in a(result, "/hits") {
        rows.push(
            item(
                &s(hit, "/chunkId"),
                s(hit, "/title"),
                format!(
                    "{}:{}–{} · {}",
                    s(hit, "/relativePath"),
                    n(hit, "/startLine"),
                    n(hit, "/endLine"),
                    s(hit, "/headingPath")
                ),
            )
            .lines(s(hit, "/content").lines().map(str::to_string).collect())
            .actions(vec![action(
                tr!("tui.knowledge.readDocument"),
                UiAction::OpenKnowledgeDocument(s(hit, "/documentId")),
                Tone::Accent,
            )]),
        );
    }
    for failure in a(result, "/failures") {
        rows.push(note(s(failure, "/error"), Tone::Bad));
    }
    for warning in a(result, "/warnings") {
        rows.push(note(s(warning, ""), Tone::Warn));
    }
    rows
}
/// Builds editable retrieval and model policy separately from content browsing.
/// 将可编辑检索及模型策略与内容浏览分开构建。
/// `app` supplies the exact selected library and inference catalog; returns declarative settings.
/// `app` 提供准确所选知识库及推理目录；返回声明式设置。
pub(super) fn settings(app: &App) -> Vec<Row> {
    // Shared header keeps the selected library and view visible.
    // 共用标题保持所选知识库及视图可见。
    let mut rows = header(app, PageId::KnowledgeSettings);
    let Some(library) = selected(app) else {
        return rows;
    };
    // Immutable library identity for partial settings requests.
    // 部分设置请求使用的不可变知识库身份。
    let id = s(library, "/id");
    rows.push(text(
        tr!("tui.field.displayName"),
        s(library, "/name"),
        dto(library.clone(), "/name", |request| {
            knowledge::rename_knowledge_library(
                &s(&request, "/id"),
                &s(&request, "/name"),
                &s(&request, "/description"),
            )
        }),
    ));
    rows.push(text(
        tr!("tui.field.description"),
        s(library, "/description"),
        dto(library.clone(), "/description", |request| {
            knowledge::rename_knowledge_library(
                &s(&request, "/id"),
                &s(&request, "/name"),
                &s(&request, "/description"),
            )
        }),
    ));
    for (pointer, label, options) in [
        (
            "/retrievalMode",
            tr!("tui.field.retrievalMode"),
            vec!["raw_bm25", "display_bm25", "display_hybrid"],
        ),
        (
            "/fallbackPolicy",
            tr!("tui.field.fallbackPolicy"),
            vec!["explicit_only", "raw_only_when_display_missing"],
        ),
    ] {
        // Key.
        // 当前结构持有的键。
        let key = pointer.trim_start_matches('/');
        rows.push(choice(
            label,
            options
                .into_iter()
                .map(|value| (value.to_string(), policy_label(value)))
                .collect(),
            s(library, &format!("/settings{pointer}")),
            Edit::Call(Setter::Build {
                ctx: json!({"id":id,"key":key}),
                build: |context, value| {
                    // Setting.
                    // 当前设置值。
                    let mut setting = serde_json::Map::new();
                    setting.insert(s(context, "/key"), json!(value.as_str()));
                    Some(knowledge::update_knowledge_library_settings(
                        &s(context, "/id"),
                        Value::Object(setting),
                    ))
                },
            }),
        ));
    }
    for (group, label) in [
        ("curation", tr!("tui.field.curation")),
        ("embedding", tr!("tui.field.embedding")),
    ] {
        rows.push(toggle(
            label,
            b(library, &format!("/settings/{group}/enabled")),
            Edit::Call(Setter::Build {
                ctx: json!({"id":id,"group":group}),
                build: |context, value| {
                    // Setting.
                    // 当前设置值。
                    let mut setting = serde_json::Map::new();
                    setting.insert(s(context, "/group"), json!({"enabled":value.as_bool()}));
                    Some(knowledge::update_knowledge_library_settings(
                        &s(context, "/id"),
                        Value::Object(setting),
                    ))
                },
            }),
        ));
    }
    rows.extend(inference_rows(app, library));

    rows
}

/// Builds project authorization using presence of an authoritative binding as its grant.
/// 根据权威绑定是否存在构建项目授权。
/// `app` supplies host projects and the current library bindings; returns authorization rows.
/// `app` 提供宿主项目及当前知识库绑定；返回授权行。
pub(super) fn bindings(app: &App) -> Vec<Row> {
    // Every binding is scoped to the selected catalog library.
    // 每个绑定均属于所选目录知识库。
    let mut rows = header(app, PageId::KnowledgeBindings);
    let Some(library) = selected(app) else {
        return rows;
    };
    // Exact library identity retained when default inclusion changes.
    // 默认使用状态改变时保留的准确知识库身份。
    let id = s(library, "/id");
    // Bindings.
    // 权威绑定集合。
    let bindings = a(app.slots.value(Slot::KnowledgeBindings), "");
    for project in &app.projects {
        // Existence is authorization; the DTO stores only authorized bindings.
        // 绑定存在即已授权；DTO 只保存已授权绑定。
        let binding = bindings
            .iter()
            .find(|binding| s(binding, "/projectId") == project.id);
        // Authorized.
        // 当前项目授权状态。
        let authorized = binding.is_some();
        // Default enabled.
        // 授权库的默认启用状态。
        let default_enabled = binding.is_some_and(|binding| b(binding, "/defaultEnabled"));
        rows.push(toggle(
            project.name.clone(),
            authorized,
            Edit::Call(Setter::Build {
                ctx: json!({"library":id,"project":project.id,"default":default_enabled}),
                build: |context, value| {
                    Some(knowledge::set_knowledge_project_binding(
                        &s(context, "/library"),
                        &s(context, "/project"),
                        value.as_bool(),
                        b(context, "/default"),
                    ))
                },
            }),
        ));
        if authorized {
            rows.push(toggle(
                tr!("tui.field.defaultEnabled"),
                default_enabled,
                Edit::Call(Setter::Build {
                    ctx: json!({"library":id,"project":project.id}),
                    build: |context, value| {
                        Some(knowledge::set_knowledge_project_binding(
                            &s(context, "/library"),
                            &s(context, "/project"),
                            true,
                            value.as_bool(),
                        ))
                    },
                }),
            ));
        }
    }

    if app.projects.is_empty() {
        rows.push(note(tr!("tui.knowledge.noProjects"), Tone::Muted));
    }
    rows
}

/// Builds source import, rescan and remap controls while preserving immutable source identities.
/// 构建来源导入、重扫描及重映射控件，并保留不可变来源身份。
/// `app` supplies source summaries for the selected library; returns declarative source rows.
/// `app` 提供所选知识库来源摘要；返回声明式来源行。
pub(super) fn sources(app: &App) -> Vec<Row> {
    // Source operations stay separate from reader and model settings.
    // 来源操作与阅读及模型设置分开。
    let mut rows = header(app, PageId::KnowledgeSources);
    let Some(library) = selected(app) else {
        return rows;
    };
    // Catalog-owned library identity.
    // 目录持有的知识库身份。
    let id = s(library, "/id");
    rows.push(button(
        tr!("tui.field.importDirectory"),
        UiAction::AddKnowledgeSource(id.clone()),
        Tone::Accent,
    ));
    if app.slots.loaded(Slot::KnowledgeSources)
        && a(app.slots.value(Slot::KnowledgeSources), "").is_empty()
    {
        rows.push(note(tr!("tui.knowledge.noSources"), Tone::Muted));
    }
    for source in a(app.slots.value(Slot::KnowledgeSources), "") {
        // The service explicitly preserves sourceId when its canonical directory changes.
        // 服务明确规定规范目录变化时保留 sourceId。
        let source_id = s(source, "/id");
        rows.push(item(&source_id, s(source, "/displayName"), s(source, "/canonicalRoot"))
            .badge(format!("{} / {}", n(source, "/documentCount"), n(source, "/chunkCount")), Tone::Muted)
            .actions(vec![
                action(tr!("tui.field.sync"), call(knowledge::preview_knowledge_import(&id, &s(source, "/canonicalRoot"), Some(&source_id))), Tone::Normal),
                action(tr!("tui.knowledge.remapSource"), form(json!({"libraryId":id,"sourceId":source_id,"sourceRoot":s(source, "/canonicalRoot")}),
                    vec![field(tr!("tui.field.sourceRoot"), "/sourceRoot", false, false)], |request| {
                        knowledge::preview_knowledge_import(&s(&request, "/libraryId"), &s(&request, "/sourceRoot"), Some(&s(&request, "/sourceId")))
                    }), Tone::Normal),
            ]));
    }
    rows.extend(job_rows(app));
    rows
}

/// Builds a bounded document page with an exact relative-folder filter and sentinel pagination.
/// 构建有界文档页，包含准确相对目录筛选及哨兵分页。
/// `app` supplies the current folder, offset and document list; returns browse actions.
/// `app` 提供当前目录、偏移及文档列表；返回浏览操作。
pub(super) fn documents(app: &App) -> Vec<Row> {
    // Reader navigation preserves the current folder and offset.
    // 阅读导航保留当前目录及偏移。
    let mut rows = header(app, PageId::KnowledgeDocuments);
    if selected(app).is_none() {
        return rows;
    }
    // JSON selector values distinguish all folders from the service's empty root-folder path.
    // JSON 选择器值区分全部目录与服务的空根目录路径。
    let mut folders = vec![("null".into(), tr!("tui.knowledge.allFolders"))];
    for folder in a(app.slots.value(Slot::KnowledgeFolders), "")
        .iter()
        .filter_map(Value::as_str)
    {
        folders.push((
            json!(folder).to_string(),
            if folder.is_empty() {
                tr!("tui.knowledge.rootFolder")
            } else {
                folder.to_string()
            },
        ));
    }
    rows.push(choice(
        tr!("tui.knowledge.folder"),
        folders,
        json!(app.knowledge_folder).to_string(),
        Edit::Local(LocalSetting::KnowledgeFolder),
    ));
    // An extra record proves another page without inferring filtered counts from a global total.
    // 额外记录证明存在下一页，不从全局总数推测筛选数量。
    let documents = a(app.slots.value(Slot::KnowledgeDocuments), "");
    rows.push(info(
        tr!("tui.knowledge.page"),
        (app.knowledge_offset / knowledge::DOCUMENT_PAGE_SIZE + 1).to_string(),
        Tone::Normal,
    ));
    // Pagination remains available after a content deletion empties a later page.
    // 内容删除使后续页变空时，分页仍可用。
    let mut paging = Vec::new();
    if app.knowledge_offset > 0 {
        paging.push(action(
            tr!("tui.knowledge.previousPage"),
            UiAction::KnowledgeOffset(
                app.knowledge_offset
                    .saturating_sub(knowledge::DOCUMENT_PAGE_SIZE),
            ),
            Tone::Normal,
        ));
    }
    if documents.len() > knowledge::DOCUMENT_PAGE_SIZE {
        paging.push(action(
            tr!("tui.knowledge.nextPage"),
            UiAction::KnowledgeOffset(app.knowledge_offset + knowledge::DOCUMENT_PAGE_SIZE),
            Tone::Normal,
        ));
    }
    if !paging.is_empty() {
        rows.push(item("document-paging", tr!("tui.knowledge.page"), "").actions(paging));
    }
    if !app.slots.loaded(Slot::KnowledgeDocuments) {
        if !app.slot_errors.contains_key(&Slot::KnowledgeDocuments) {
            rows.push(note(tr!("tui.page.loading"), Tone::Muted));
        }
    } else if documents.is_empty() {
        rows.push(note(tr!("tui.knowledge.noDocuments"), Tone::Muted));
    }
    for document in documents.iter().take(knowledge::DOCUMENT_PAGE_SIZE) {
        rows.push(
            item(
                &s(document, "/id"),
                s(document, "/title"),
                s(document, "/relativePath"),
            )
            .badge(
                format!(
                    "{} {}",
                    n(document, "/lineCount"),
                    tr!("tui.knowledge.lines")
                ),
                Tone::Muted,
            )
            .actions(vec![action(
                tr!("tui.knowledge.readDocument"),
                UiAction::OpenKnowledgeDocument(s(document, "/id")),
                Tone::Accent,
            )]),
        );
    }
    rows
}

/// Reads one exact document as visible Markdown lines, with explicit raw/display corpus selection.
/// 按可见 Markdown 行阅读单个准确文档，并明确选择原始或展示语料。
/// `app` supplies the selected document and reader response; returns content and navigation rows.
/// `app` 提供所选文档及阅读响应；返回正文及导航行。
pub(super) fn document(app: &App) -> Vec<Row> {
    // The reader header retains the parent library identity.
    // 阅读标题保留父知识库身份。
    let mut rows = header(app, PageId::KnowledgeDocument);
    if selected(app).is_none() {
        return rows;
    }
    rows.push(button(
        tr!("tui.knowledge.backDocuments"),
        UiAction::GoPage(PageId::KnowledgeDocuments),
        Tone::Normal,
    ));
    rows.push(choice(
        tr!("tui.knowledge.corpus"),
        vec![
            ("raw".into(), tr!("tui.knowledge.rawCorpus")),
            ("display".into(), tr!("tui.knowledge.displayCorpus")),
        ],
        app.knowledge_corpus.clone(),
        Edit::Local(LocalSetting::KnowledgeCorpus),
    ));
    if !app.slots.loaded(Slot::KnowledgeDocument) {
        if !app.slot_errors.contains_key(&Slot::KnowledgeDocument) {
            rows.push(note(tr!("tui.page.loading"), Tone::Muted));
        }
        return rows;
    }
    // The read contract owns document metadata and unescaped normalized content.
    // 阅读契约持有文档元数据及未经转义的规范化正文。
    let document = app.slots.value(Slot::KnowledgeDocument);
    rows.push(section(s(document, "/document/title")));
    rows.push(note(s(document, "/document/relativePath"), Tone::Muted));
    rows.push(note(tr!("tui.knowledge.readHint"), Tone::Muted));
    rows.push(Row::new(
        RowKind::Document {
            content: s(document, "/content"),
        },
        tr!("tui.knowledge.document"),
    ));
    rows
}

/// Builds index health and gated maintenance actions independently from ordinary library settings.
/// 独立于普通知识库设置构建索引健康状态及受条件约束的维护操作。
/// `app` supplies the library configuration and retained job; returns maintenance rows.
/// `app` 提供知识库配置及保留任务；返回维护行。
pub(super) fn maintenance(app: &App) -> Vec<Row> {
    // Health precedes repair/build actions so decisions use visible evidence.
    // 健康状态位于修复、构建操作前，让决定基于可见证据。
    let mut rows = header(app, PageId::KnowledgeMaintenance);
    let Some(library) = selected(app) else {
        return rows;
    };
    // Exact catalog identity for all maintenance requests.
    // 全部维护请求的准确目录身份。
    let id = s(library, "/id");
    rows.extend(health_rows(app));
    if !matches!(
        s(app.slots.value(Slot::KnowledgeJob), "/state").as_str(),
        "queued" | "running"
    ) {
        rows.push(
            item("index-actions", tr!("tui.knowledge.actions"), "").actions(vec![
                action(
                    tr!("tui.field.repair"),
                    call(knowledge::repair_knowledge_index(&id)),
                    Tone::Normal,
                ),
                action(
                    tr!("tui.field.rebuild"),
                    confirm(
                        tr!("tui.field.rebuild"),
                        s(library, "/name"),
                        tr!("tui.field.rebuild"),
                        call(knowledge::rebuild_knowledge_index(&id)),
                    ),
                    Tone::Warn,
                ),
            ]),
        );
        if b(library, "/settings/curation/enabled") {
            rows.push(button(
                tr!("tui.knowledge.buildDisplay"),
                call(knowledge::preview_knowledge_display(&id)),
                Tone::Accent,
            ));
        }
        if b(library, "/settings/embedding/enabled")
            && s(library, "/settings/displayIndex/status") == "ready"
        {
            rows.push(button(
                tr!("tui.knowledge.buildVector"),
                confirm(
                    tr!("tui.knowledge.buildVector"),
                    s(library, "/name"),
                    tr!("tui.field.confirm"),
                    call(knowledge::build_knowledge_vector_index(&id)),
                ),
                Tone::Accent,
            ));
        }
    }
    rows.push(info(
        tr!("tui.knowledge.displayIndex"),
        s(library, "/settings/displayIndex/status"),
        Tone::Normal,
    ));
    rows.push(info(
        tr!("tui.knowledge.vectorIndex"),
        s(library, "/settings/vectorIndex/status"),
        Tone::Normal,
    ));
    rows.extend(job_rows(app));
    rows
}

/// Projects physical index counts and revision gates into human-readable health rows.
/// 将物理索引计数及版本门控投影为可读健康状态行。
/// `app` supplies KnowledgeIndexIntegritySummary; returns no rows while the slot is pending.
/// `app` 提供 KnowledgeIndexIntegritySummary；槽位未加载时返回空行集合。
fn health_rows(app: &App) -> Vec<Row> {
    if !app.slots.loaded(Slot::KnowledgeIntegrity) {
        return Vec::new();
    }
    // Integrity is scoped by the reducer to the selected library.
    // 完整性由 reducer 限定为所选知识库。
    let integrity = app.slots.value(Slot::KnowledgeIntegrity);
    vec![
        section(tr!("tui.field.indexIntegrity")),
        info(
            tr!("tui.knowledge.searchable"),
            if b(integrity, "/searchable") {
                tr!("tui.page.on")
            } else {
                tr!("tui.page.off")
            },
            if b(integrity, "/searchable") {
                Tone::Good
            } else {
                Tone::Warn
            },
        ),
        stats(vec![
            (
                tr!("tui.knowledge.authoritativeChunks"),
                n(integrity, "/authoritativeChunkCount").to_string(),
                Tone::Normal,
            ),
            (
                tr!("tui.knowledge.indexedChunks"),
                n(integrity, "/indexedChunkCount").to_string(),
                Tone::Normal,
            ),
            (
                tr!("tui.knowledge.missingChunks"),
                n(integrity, "/missingOrMismatchedCount").to_string(),
                Tone::Warn,
            ),
            (
                tr!("tui.knowledge.orphanChunks"),
                n(integrity, "/orphanOrMismatchedCount").to_string(),
                Tone::Warn,
            ),
        ]),
        info(
            tr!("tui.knowledge.contentRevision"),
            format!(
                "{} / {}",
                n(integrity, "/indexedContentRevision"),
                n(integrity, "/contentRevision")
            ),
            Tone::Normal,
        ),
        info(
            tr!("tui.knowledge.dictionaryRevision"),
            format!(
                "{} / {}",
                n(integrity, "/indexedDictionaryRevision"),
                n(integrity, "/dictionaryRevision")
            ),
            Tone::Normal,
        ),
    ]
}

/// Projects the retained job into readable progress and permits cancellation only while active.
/// 将保留任务投影为可读进度，只允许取消活动任务。
/// `app` supplies the scoped job and library identity; returns no rows for a null job.
/// `app` 提供限定范围的任务及知识库身份；空任务返回空行集合。
fn job_rows(app: &App) -> Vec<Row> {
    // Null is the documented no-job response.
    // 空值为文档定义的无任务响应。
    let job = app.slots.value(Slot::KnowledgeJob);
    if job.is_null() {
        return Vec::new();
    }
    // Exact selected library identity; jobs cannot be cancelled across libraries.
    // 准确所选知识库身份；不能跨知识库取消任务。
    let Some(library) = app.library.as_deref() else {
        return Vec::new();
    };
    // Job state and phase are durable service facts.
    // 任务状态及阶段为持久服务事实。
    let mut rows = vec![
        section(tr!("tui.field.job")),
        info(tr!("tui.catalog.status"), s(job, "/state"), Tone::Accent),
        info(
            tr!("tui.knowledge.operation"),
            s(job, "/operationKind"),
            Tone::Normal,
        ),
        info(tr!("tui.knowledge.phase"), s(job, "/phase"), Tone::Normal),
    ];
    for (done, total, label) in [
        (
            "/processedFiles",
            "/totalFiles",
            tr!("tui.knowledge.fileProgress"),
        ),
        (
            "/processedIndexItems",
            "/totalIndexItems",
            tr!("tui.knowledge.indexProgress"),
        ),
    ] {
        if let (Some(done), Some(total)) = (v(job, done).as_u64(), v(job, total).as_u64())
            && total > 0
        {
            rows.push(meter(
                label,
                done as f64 / total as f64,
                format!("{done}/{total}"),
                Tone::Accent,
            ));
        }
    }
    if let Some(file) = v(job, "/currentFile").as_str() {
        rows.push(note(file.to_string(), Tone::Muted));
    }
    if let Some(error) = v(job, "/errorMessage").as_str() {
        rows.push(note(error.to_string(), Tone::Bad));
    }
    for warning in a(job, "/warnings") {
        rows.push(note(s(warning, ""), Tone::Warn));
    }
    if matches!(s(job, "/state").as_str(), "queued" | "running") {
        rows.push(button(
            tr!("tui.field.cancelJob"),
            call(knowledge::cancel_knowledge_import(library, &s(job, "/id"))),
            Tone::Warn,
        ));
    }
    rows
}

/// Translates retrieval-mode keys while preserving exact protocol values in requests.
/// 翻译检索模式键，在请求中保留准确协议值。
/// `mode` is the declared retrieval mode; returns its user-facing label.
/// `mode` 为已声明检索模式；返回面向用户标签。
fn retrieval_label(mode: &str) -> String {
    match mode {
        "raw_bm25" => tr!("tui.knowledge.rawSearch"),
        "display_bm25" => tr!("tui.knowledge.displaySearch"),
        "display_hybrid" => tr!("tui.knowledge.hybridSearch"),
        value => value.to_string(),
    }
}

/// Translates the declared retrieval and fallback policy choices.
/// 翻译已声明检索及回退策略选项。
/// `policy` is a protocol choice; returns its translated description.
/// `policy` 为协议选项；返回其翻译描述。
fn policy_label(policy: &str) -> String {
    match policy {
        "explicit_only" => tr!("tui.knowledge.explicitFallback"),
        "raw_only_when_display_missing" => tr!("tui.knowledge.rawFallback"),
        value => retrieval_label(value),
    }
}
