//! Read-only replay uses the production prompt planner without upload or history mutation authority.
//! 只读重放使用生产提示词规划器，不具备上传或历史修改权限。

use super::*;

/// Creates exact historical inline proof for `entry` on `context`, independent of current soft upload preference.
/// 为`context`上的`entry`创建精确历史内联证明，独立于当前软上传偏好。
fn inline_origin(
    entry: &ResourceCatalogEntry,
    context: &ProviderRequestMediaContext,
) -> UserPromptMediaOrigin {
    UserPromptMediaOrigin {
        route: original_route(context),
        resources: vec![UserPromptResourceDelivery {
            resource_id: entry.resource_id,
            original_route: original_route(context),
            delivery: UserPromptResourceDeliveryKind::Inline,
        }],
    }
}

/// Compaction restores exact uploaded and inline origins but never prepares a genuinely new resource.
/// 压缩恢复精确上传及内联原身份，但绝不准备真正的新资源。
#[tokio::test]
async fn readonly_prompt_media_restores_originals_without_new_uploads()
-> Result<(), Box<dyn std::error::Error>> {
    // All source and receipt data is isolated from the user's workspace and providers.
    // 全部来源及回执数据与用户工作区、供应商隔离。
    let project = tempfile::tempdir()?;
    // The production registry supplies append-only receipt lookup.
    // 生产注册表提供仅追加回执查询。
    let registry = Arc::new(ProjectResourceCatalogRegistry::default());
    // One immutable source appears under three independent prompt intentions.
    // 一个不可变来源出现在三个独立提示词意图之下。
    let entry = register(project.path(), &registry, b"abc")?;
    // A offers a real upload; the fixture seeds only a past receipt, never uploads.
    // A提供真实上传；夹具仅写入过去回执，绝不上传。
    let (first, first_secrets) = context(false, true).await?;
    // The original fingerprint must survive a later receipt for the same bytes.
    // 相同字节的后来回执不能替换原指纹。
    let uploaded = seed_receipt(
        project.path(),
        &registry,
        &entry,
        &first,
        "file-readonly-original",
        false,
    )?;
    seed_receipt(
        project.path(),
        &registry,
        &entry,
        &first,
        "file-readonly-later",
        false,
    )?;
    // The new prompt has real first-API eligibility, which read-only mode must not consume.
    // 新提示词具有真实首次API资格，只读模式不得消费。
    let input = snapshot(
        project.path(),
        entry.resource_id,
        vec![
            frozen(uploaded),
            frozen(inline_origin(&entry, &first)),
            UserPromptParameters::for_new_resources(&[entry.resource_id]),
        ],
    )?;
    // Map the actual planner identities explicitly, without parsing or guessing message names.
    // 显式映射真实规划器身份，不解析或猜测消息名称。
    let ids = vec![
        String::from("original-upload"),
        String::from("original-inline"),
        String::from("new-prompt"),
    ];
    // The frozen source owner must not be replaced by a newly loaded session or latest model.
    // 冻结来源所有者不能被新加载会话或最近模型替换。
    let projection = frozen_prompt_media_projection(
        input,
        first.session_scope_id.clone(),
        ids.iter()
            .enumerate()
            .map(|(index, id)| (id.clone(), index))
            .collect(),
        Arc::clone(&registry),
    )?;
    // Record credential reads after route resolution; media replay must add none.
    // 在路线解析后记录凭据读取次数；媒体重放不得新增读取。
    let first_reads = first_secrets.reads.load(Ordering::SeqCst);
    // A restores its exact upload and original inline mode; the new prompt stays address-only.
    // A恢复精确上传及原内联方式；新提示词保持仅地址。
    let on_first = projection.project_readonly(&ids, first.clone()).await?;
    assert!(
        matches!(&on_first[&ids[0]].content[1], ContentItem::Resource { resource } if resource.id == "file-readonly-original")
    );
    assert!(
        matches!(&on_first[&ids[1]].content[1], ContentItem::Resource { resource } if resource.source_kind == ResourceSourceKind::InlineData)
    );
    assert!(
        matches!(&on_first[&ids[2]].content[1], ContentItem::Text { text } if text.contains("original delivery") && text.contains("unknown"))
    );
    // Exact capture restores the same historical subset, without including the separate new-upload prompt.
    // 精确捕获恢复相同历史子集，不包含独立的新上传提示词。
    let captured_history = projection.project_capture(&ids[..2], first.clone()).await?;
    assert_eq!(captured_history[&ids[0]], on_first[&ids[0]]);
    assert_eq!(captured_history[&ids[1]], on_first[&ids[1]]);
    // B may support upload, but that cannot transfer original uploaded ownership.
    // B可能支持上传，但不能因此转移原上传所有权。
    let (second, second_secrets) = context(true, true).await?;
    // The independently resolved route is frozen before this pure replay check.
    // 本纯重放检查之前独立解析路线已经冻结。
    let second_reads = second_secrets.reads.load(Ordering::SeqCst);
    let on_second = projection.project_readonly(&ids, second).await?;
    assert!(
        matches!(&on_second[&ids[0]].content[1], ContentItem::Text { text } if text.contains("another model"))
    );
    assert!(
        matches!(&on_second[&ids[1]].content[1], ContentItem::Resource { resource } if resource.source_kind == ResourceSourceKind::InlineData)
    );
    assert!(matches!(
        &on_second[&ids[2]].content[1],
        ContentItem::Text { .. }
    ));
    assert_eq!(first_secrets.reads.load(Ordering::SeqCst), first_reads);
    assert_eq!(second_secrets.reads.load(Ordering::SeqCst), second_reads);
    assert_eq!(
        load_project_resource_catalog(project.path())?.entries[0]
            .provider_bindings
            .len(),
        2
    );
    Ok(())
}

/// Exact capture may prepare new inline bytes without publishing origins or modifying the source frontier.
/// 精确捕获可准备新内联字节，但不发布原身份或修改来源前沿。
#[tokio::test]
async fn capture_prompt_media_prepares_inline_without_freezing_origins()
-> Result<(), Box<dyn std::error::Error>> {
    // The isolated project owns one immutable source.
    // 隔离项目持有一个不可变来源。
    let project = tempfile::tempdir()?;
    // Capture reuses the existing registry without installing another owner.
    // 捕获复用既有注册表，不安装另一个所有者。
    let registry = Arc::new(ProjectResourceCatalogRegistry::default());
    // Three raw bytes fit the frozen inline threshold.
    // 三个原始字节符合冻结内联阈值。
    let entry = register(project.path(), &registry, b"abc")?;
    // This route permits inline input without an upload channel.
    // 此路线允许内联输入且没有上传通道。
    let (context, secrets) = context(false, false).await?;
    // Only this explicit pending prompt owns first-API preparation eligibility.
    // 只有此显式待处理提示词拥有首次API准备资格。
    let input = snapshot(
        project.path(),
        entry.resource_id,
        vec![UserPromptParameters::for_new_resources(
            &[entry.resource_id],
        )],
    )?;
    // Direct engine output proves capture cannot produce prompt-publication bindings.
    // 直接引擎输出证明捕获不能生成提示词发布绑定。
    let captured = prepare_user_media_with_mode(
        &input,
        Some(&context),
        Arc::clone(&registry),
        PromptMediaPreparationMode::Capture,
    )
    .await?;
    assert!(captured.bindings.is_empty());
    assert_eq!(
        native(&captured, 0).source_kind,
        ResourceSourceKind::InlineData
    );
    // The public read-only port must preserve the same mode boundary.
    // 公共只读端口必须保留相同模式边界。
    let projection = frozen_prompt_media_projection(
        input,
        context.session_scope_id.clone(),
        vec![("pending".into(), 0)],
        Arc::clone(&registry),
    )?;
    // No credential is read during either capture or historical projection.
    // 捕获或历史投影都不读取凭据。
    let reads = secrets.reads.load(Ordering::SeqCst);
    // New inline capture can be repeated without consuming first-send eligibility.
    // 新内联捕获可以重复且不消费首次发送资格。
    let first = projection
        .project_capture(&["pending".into()], context.clone())
        .await?;
    assert!(
        matches!(&first["pending"].content[1], ContentItem::Resource { resource } if resource.source_kind == ResourceSourceKind::InlineData)
    );
    assert_eq!(
        projection
            .project_capture(&["pending".into()], context.clone())
            .await?,
        first
    );
    // Historical projection still sees the original pending state, not a capture-created inline origin.
    // 历史投影仍看到原待处理状态，而非捕获创建的内联原身份。
    let historical = projection
        .project_readonly(&["pending".into()], context)
        .await?;
    assert!(
        matches!(&historical["pending"].content[1], ContentItem::Text { text } if text.contains("unknown"))
    );
    assert_eq!(secrets.reads.load(Ordering::SeqCst), reads);
    assert!(
        load_project_resource_catalog(project.path())?.entries[0]
            .provider_bindings
            .is_empty()
    );
    Ok(())
}

/// An eligible fresh upload is rejected for capture before network credentials or receipt publication.
/// 合格新上传在捕获时于网络凭据读取或回执发布之前被拒绝。
#[tokio::test]
async fn capture_prompt_media_refuses_fresh_upload_without_side_effects()
-> Result<(), Box<dyn std::error::Error>> {
    // All data is confined to a disposable source catalog.
    // 全部数据限定于一次性来源目录。
    let project = tempfile::tempdir()?;
    // Production catalog receipt behavior remains observable without provider traffic.
    // 无需供应商流量即可观察生产目录回执行为。
    let registry = Arc::new(ProjectResourceCatalogRegistry::default());
    // A real source is admitted by metadata planning.
    // 真实来源由元数据规划准入。
    let entry = register(project.path(), &registry, b"abc")?;
    // The frozen upload preference requires upload even for this small source.
    // 冻结上传偏好要求此小来源仍走上传。
    let (context, secrets) = context(false, true).await?;
    // Pending origin must never be converted into a fake captured upload receipt.
    // 待处理原身份绝不能转换为伪造的捕获上传回执。
    let input = snapshot(
        project.path(),
        entry.resource_id,
        vec![UserPromptParameters::for_new_resources(
            &[entry.resource_id],
        )],
    )?;
    // The exact source identity is supplied by the host, not inferred from resource ID.
    // 精确来源身份由宿主提供，不从资源ID推导。
    let projection = frozen_prompt_media_projection(
        input,
        context.session_scope_id.clone(),
        vec![("pending".into(), 0)],
        Arc::clone(&registry),
    )?;
    // Credential reads made while resolving the route are not media-upload reads.
    // 解析路线期间的凭据读取不属于媒体上传读取。
    let reads = secrets.reads.load(Ordering::SeqCst);
    for _ in 0..2 {
        assert!(
            projection
                .project_capture(&["pending".into()], context.clone())
                .await
                .unwrap_err()
                .contains("requires a new upload")
        );
    }
    assert_eq!(secrets.reads.load(Ordering::SeqCst), reads);
    assert!(
        load_project_resource_catalog(project.path())?.entries[0]
            .provider_bindings
            .is_empty()
    );
    Ok(())
}

/// Chronological selected-window budgeting and per-message counts use the same production planner as dispatch.
/// 时序所选窗口预算及逐消息数量使用与分派相同的生产规划器。
#[tokio::test]
async fn readonly_prompt_media_budgets_only_the_selected_window_in_original_order()
-> Result<(), Box<dyn std::error::Error>> {
    // The temporary source expands from three bytes to exactly four Base64 bytes.
    // 临时来源由三个字节膨胀为精确四个Base64字节。
    let project = tempfile::tempdir()?;
    // Reuse one registry for all read-only projections.
    // 全部只读投影复用一个注册表。
    let registry = Arc::new(ProjectResourceCatalogRegistry::default());
    // Duplicate appearances must not collapse into one budget entry.
    // 重复出现不能折叠成一个预算条目。
    let entry = register(project.path(), &registry, b"abc")?;
    // Freeze a small budget and one native resource per message, not per request.
    // 冻结小预算及每条消息一个原生资源限制，而非逐请求限制。
    let (mut context, _) = context(false, false).await?;
    // This test owns the only execution reference before the read-only call.
    // 只读调用之前本测试持有唯一执行引用。
    let execution = Arc::get_mut(&mut context.execution).ok_or("exclusive execution")?;
    execution
        .media_delivery_policy
        .conversation
        .max_total_encoded_bytes = 4;
    execution.route.protocol_route.input_resource_constraints =
        vec![vulcan_provider_adapters::InputResourceConstraint {
            resource_kind: ResourceKind::Image,
            accepted_mime_types: vec!["image/png".into()],
            max_bytes: None,
            max_count: Some(1),
            reference_policy: vulcan_provider_adapters::ResourceReferencePolicy::Any,
            upload_channel_refs: Vec::new(),
        }];
    // Lexicographic identity order intentionally differs from source chronology.
    // 身份字典序故意与来源时序不同。
    let ids = vec![String::from("z-first"), String::from("a-second")];
    // Each message owns its own original inline proof even when resource IDs repeat.
    // 即使资源ID重复，每条消息仍持有自己的原内联证明。
    let origin = inline_origin(&entry, &context);
    let input = snapshot(
        project.path(),
        entry.resource_id,
        vec![frozen(origin.clone()), frozen(origin.clone())],
    )?;
    let projection = frozen_prompt_media_projection(
        input,
        context.session_scope_id.clone(),
        ids.iter()
            .enumerate()
            .map(|(index, id)| (id.clone(), index))
            .collect(),
        Arc::clone(&registry),
    )?;
    let both = projection.project_readonly(&ids, context.clone()).await?;
    assert!(matches!(
        &both[&ids[0]].content[1],
        ContentItem::Resource { .. }
    ));
    assert!(
        matches!(&both[&ids[1]].content[1], ContentItem::Text { text } if text.contains("combined encoded"))
    );
    // An omitted earlier prompt must not consume the selected suffix's budget.
    // 省略的较早提示词不得消费所选后缀预算。
    let suffix = projection
        .project_readonly(&ids[1..], context.clone())
        .await?;
    assert!(matches!(
        &suffix[&ids[1]].content[1],
        ContentItem::Resource { .. }
    ));
    Arc::get_mut(&mut context.execution)
        .ok_or("exclusive execution")?
        .media_delivery_policy
        .conversation
        .max_total_encoded_bytes = 8;
    let both = projection.project_readonly(&ids, context.clone()).await?;
    assert!(
        both.values()
            .all(|message| matches!(&message.content[1], ContentItem::Resource { .. }))
    );
    // Two resources inside one message still obey that message's single-resource hard limit.
    // 一条消息内的两个资源仍遵循该消息的单资源硬限制。
    let mut doubled = origin.clone();
    doubled.resources.push(origin.resources[0].clone());
    let mut single = snapshot(project.path(), entry.resource_id, vec![frozen(doubled)])?;
    single.sourced_items[0].item = ProjectedItem::UserInput {
        text: "two occurrences".into(),
        resource_ids: vec![entry.resource_id, entry.resource_id],
    };
    single.items[0] = single.sourced_items[0].item.clone();
    let one = frozen_prompt_media_projection(
        single,
        context.session_scope_id.clone(),
        vec![("one".into(), 0)],
        registry,
    )?
    .project_readonly(&["one".into()], context)
    .await?;
    assert!(matches!(
        &one["one"].content[1],
        ContentItem::Resource { .. }
    ));
    assert!(matches!(&one["one"].content[2], ContentItem::Text { .. }));
    Ok(())
}

/// Invalid selection identities and session claims fail before media or credential lookup.
/// 无效选择身份及会话声明在媒体或凭据查询之前失败。
#[tokio::test]
async fn readonly_prompt_media_rejects_ambiguous_source_selection()
-> Result<(), Box<dyn std::error::Error>> {
    // Metadata fixtures use the production snapshot shape but no provider operations.
    // 元数据夹具使用生产快照形态，但不执行供应商操作。
    let project = tempfile::tempdir()?;
    let registry = Arc::new(ProjectResourceCatalogRegistry::default());
    let entry = register(project.path(), &registry, b"abc")?;
    let (context, secrets) = context(false, false).await?;
    let input = snapshot(
        project.path(),
        entry.resource_id,
        vec![
            frozen(inline_origin(&entry, &context)),
            frozen(inline_origin(&entry, &context)),
        ],
    )?;
    let reads = secrets.reads.load(Ordering::SeqCst);
    for invalid in [
        vec![("same".into(), 0), ("same".into(), 1)],
        vec![("later".into(), 1), ("earlier".into(), 0)],
        vec![("unknown".into(), 2)],
    ] {
        assert!(
            frozen_prompt_media_projection(
                input.clone(),
                context.session_scope_id.clone(),
                invalid,
                Arc::clone(&registry)
            )
            .is_err()
        );
    }
    // A mismatched semantic/source snapshot must fail before the factory can freeze any mapping.
    // 语义与来源矛盾的快照必须在工厂冻结任何映射之前失败。
    let mut contradictory = input.clone();
    contradictory.items[0] = ProjectedItem::UserInput {
        text: "different prompt".into(),
        resource_ids: vec![entry.resource_id],
    };
    assert!(
        frozen_prompt_media_projection(
            contradictory,
            context.session_scope_id.clone(),
            vec![("first".into(), 0)],
            Arc::clone(&registry)
        )
        .is_err()
    );
    let projection = frozen_prompt_media_projection(
        input,
        context.session_scope_id.clone(),
        vec![("first".into(), 0), ("second".into(), 1)],
        registry,
    )?;
    for invalid in [
        vec!["absent".into()],
        vec!["first".into(), "first".into()],
        vec!["second".into(), "first".into()],
    ] {
        assert!(
            projection
                .project_readonly(&invalid, context.clone())
                .await
                .is_err()
        );
    }
    let mut other = context.clone();
    other.session_scope_id = "another-session".into();
    assert!(
        projection
            .project_readonly(&["first".into()], other)
            .await
            .unwrap_err()
            .contains("another session")
    );
    assert_eq!(secrets.reads.load(Ordering::SeqCst), reads);
    Ok(())
}
