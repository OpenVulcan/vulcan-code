//! Actual-prefix media admission after source and upload failures.
//! 来源及上传失败后按实际前缀进行媒体准入。

use super::*;

/// Builds frozen fixture execution with `uploaded` capability, exact encoded `budget`, and per-prompt `max_count`.
/// 构建具有`uploaded`能力、精确编码`budget`及逐提示词`max_count`的冻结夹具执行。
/// Returns the context and credential recorder; only fixture declarations are changed, never user configuration.
/// 返回上下文及凭据记录器；只改变夹具声明，绝不改变用户配置。
async fn limited_context(
    uploaded: bool,
    budget: u64,
    max_count: Option<u64>,
) -> Result<(ProviderRequestMediaContext, Arc<OfflineSecrets>), String> {
    // Reuse the exact offline source catalog and selected route from the existing V1 tests.
    // 复用既有V1测试的精确离线来源目录及选中路线。
    let (mut context, secrets) = context(false, uploaded).await?;
    // The fixture is uniquely owned before the immutable context is handed to preparation.
    // 在把不可变上下文交给准备前，夹具拥有唯一所有权。
    let execution = Arc::get_mut(&mut context.execution).expect("unique fixture execution");
    execution
        .media_delivery_policy
        .conversation
        .max_total_encoded_bytes = budget;
    execution.route.protocol_route.input_resource_constraints =
        vec![vulcan_provider_adapters::InputResourceConstraint {
            resource_kind: ResourceKind::Image,
            accepted_mime_types: vec!["image/png".into()],
            max_bytes: Some(1_048_576),
            max_count,
            reference_policy: ResourceReferencePolicy::Any,
            upload_channel_refs: if uploaded {
                vec!["offline-files".into()]
            } else {
                Vec::new()
            },
        }];
    Ok((context, secrets))
}

/// Builds exact chronological `rows` with corresponding original `parameters` inside isolated `project`.
/// 在隔离`project`中按精确时序`rows`及对应原`parameters`构建快照。
/// Returns a consistent semantic/source projection, retaining every duplicate resource occurrence.
/// 返回一致的语义/来源投影，保留每个重复资源出现。
fn resource_rows(
    project: &std::path::Path,
    rows: &[Vec<ResourceId>],
    parameters: Vec<Option<UserPromptParameters>>,
) -> Result<SessionProviderProjectionSnapshot, String> {
    assert_eq!(rows.len(), parameters.len());
    // Every test row is nonempty and the first actual ID seeds the existing snapshot helper.
    // 每个测试行均非空，首个实际ID用于初始化既有快照辅助方法。
    let mut input = snapshot(project, rows[0][0], parameters)?;
    for (sourced, resources) in input.sourced_items.iter_mut().zip(rows) {
        // The shared helper constructs only exact user-input rows; other roles are a fixture error.
        // 共享辅助方法仅构造精确用户输入行；其他角色属于夹具错误。
        let ProjectedItem::UserInput { resource_ids, .. } = &mut sourced.item else {
            unreachable!("user input fixture")
        };
        resource_ids.clone_from(resources);
    }
    input.items = input
        .sourced_items
        .iter()
        .map(|sourced| sourced.item.clone())
        .collect();
    Ok(input)
}

/// Returns proven inline history for ordered `resources` on the exact original `context`.
/// 为精确原`context`上的有序`resources`返回已证明内联历史。
fn inline_history(
    context: &ProviderRequestMediaContext,
    resources: &[ResourceId],
) -> Option<UserPromptParameters> {
    frozen(UserPromptMediaOrigin {
        route: original_route(context),
        resources: resources
            .iter()
            .map(|id| UserPromptResourceDelivery {
                resource_id: *id,
                original_route: original_route(context),
                delivery: UserPromptResourceDeliveryKind::Inline,
            })
            .collect(),
    })
}

/// Corrupt first content releases encoded budget for later prompts without reversing priority or omitting attachments.
/// 首项内容损坏为后续提示词释放编码预算，不反转优先级或遗漏附件。
#[tokio::test]
async fn v1_failed_inline_refunds_encoded_budget_in_prompt_order()
-> Result<(), Box<dyn std::error::Error>> {
    // The isolated catalog owns both corrupted and valid original entities.
    // 隔离目录同时持有损坏及有效原实体。
    let project = tempfile::tempdir()?;
    // All preparation and receipt access share the real project registry.
    // 全部准备和回执访问共享真实项目注册表。
    let registry = Arc::new(ProjectResourceCatalogRegistry::default());
    // This source remains the same size but no longer matches its admitted hash.
    // 此来源大小不变，但不再匹配准入哈希。
    let bad = register(project.path(), &registry, b"bad")?;
    // Later sources have equal cost so only chronological admission decides which fit.
    // 后续来源成本相同，因此仅由时序准入决定哪些可容纳。
    let first = register(project.path(), &registry, b"one")?;
    // This file must become inline only after the first failure releases its reservation.
    // 此文件必须仅在首次失败释放预留后才能内联。
    let second = register(project.path(), &registry, b"two")?;
    // The final file remains over budget after the two earlier successes.
    // 两个更早项成功后，最后文件仍超出预算。
    let last = register(project.path(), &registry, b"end")?;
    std::fs::write(project.path().join(&bad.entity_relative_path), b"BAD")?;
    // Eight encoded bytes fit exactly two three-byte raw sources, across both prompts together.
    // 八个编码字节精确容纳两个三字节原来源，两个提示词共同计入。
    let (context, secrets) = limited_context(false, 8, None).await?;
    // Preserve the initial credential-resolution baseline to detect any unexpected upload attempt.
    // 保留初始凭据解析基线，以发现任何意外上传尝试。
    let reads = secrets.reads.load(Ordering::SeqCst);
    // Each row has its own source event and ordered resource list.
    // 每行具有自己的来源事件及有序资源列表。
    let rows = vec![
        vec![bad.resource_id, first.resource_id],
        vec![second.resource_id, last.resource_id],
    ];
    // New prompt parameters remain owned by each source row.
    // 新提示词参数仍归每个来源行所有。
    let input = resource_rows(
        project.path(),
        &rows,
        rows.iter()
            .map(|ids| UserPromptParameters::for_new_resources(ids))
            .collect(),
    )?;
    // All actual source reads and final content assembly run through the production consumer.
    // 全部实际来源读取及最终内容装配均通过生产消费者。
    let prepared = prepare_v1_user_media(&input, Some(&context), Arc::clone(&registry)).await?;
    assert!(
        matches!(&prepared.prompts[&0].content[1], ContentItem::Text { text } if text.contains("could not be verified"))
    );
    assert!(
        matches!(&prepared.prompts[&0].content[2], ContentItem::Resource { resource } if resource.resource_path.as_deref() == Some(format!("project-resource://{}", first.resource_id.value()).as_str()))
    );
    assert!(
        matches!(&prepared.prompts[&1].content[1], ContentItem::Resource { resource } if resource.resource_path.as_deref() == Some(format!("project-resource://{}", second.resource_id.value()).as_str()))
    );
    assert!(
        matches!(&prepared.prompts[&1].content[2], ContentItem::Text { text } if text.contains("combined encoded"))
    );
    assert_eq!(prepared.prompts[&0].attachments.len(), 2);
    assert_eq!(prepared.prompts[&1].attachments.len(), 2);
    assert_eq!(
        prepared.bindings[0].origin.resources[0].delivery,
        UserPromptResourceDeliveryKind::Attachment
    );
    assert_eq!(
        prepared.bindings[1].origin.resources[0].delivery,
        UserPromptResourceDeliveryKind::Inline
    );
    assert_eq!(
        prepared.bindings[1].origin.resources[1].delivery,
        UserPromptResourceDeliveryKind::Attachment
    );
    assert_eq!(secrets.reads.load(Ordering::SeqCst), reads);
    Ok(())
}

/// Actual failed history consumes neither count nor budget, but its original inline proof is never rewritten.
/// 实际失败历史既不消耗数量也不消耗预算，但其原内联证明绝不被改写。
#[tokio::test]
async fn v1_failed_history_releases_prompt_count_without_rewriting_origins()
-> Result<(), Box<dyn std::error::Error>> {
    // Every source and failure is isolated from user project data.
    // 每个来源及失败均与用户项目数据隔离。
    let project = tempfile::tempdir()?;
    // The same registry preserves original catalog and receipt ownership.
    // 同一注册表保留原目录及回执归属。
    let registry = Arc::new(ProjectResourceCatalogRegistry::default());
    // This historical inline occurrence fails despite retaining an immutable Inline original mode.
    // 此历史内联出现虽然保留不可变Inline原方式，仍会失败。
    let bad = register(project.path(), &registry, b"bad")?;
    // Two valid resources exercise the exact per-message count limit.
    // 两个有效资源覆盖精确逐消息数量限制。
    let first = register(project.path(), &registry, b"one")?;
    // Repeated use in the next prompt must not be counted against the preceding prompt.
    // 下一提示词中的重复使用不得计入前一提示词。
    let second = register(project.path(), &registry, b"two")?;
    std::fs::write(project.path().join(&bad.entity_relative_path), b"BAD")?;
    // There is enough encoded budget; only a single native image per prompt is allowed.
    // 编码预算充足；每个提示词仅允许一个原生图片。
    let (context, _) = limited_context(false, 64, Some(1)).await?;
    // Source rows preserve the same valid resource's two distinct chronological occurrences.
    // 来源行保留同一有效资源的两个独立时序出现。
    let rows = vec![
        vec![bad.resource_id, first.resource_id, second.resource_id],
        vec![second.resource_id],
    ];
    // Keep a copy of the actual historical parameters for the non-mutation assertion.
    // 保留实际历史参数副本，用于不修改断言。
    let parameters: Vec<_> = rows
        .iter()
        .map(|ids| inline_history(&context, ids))
        .collect();
    // Both history rows are frozen and have no first-delivery eligibility.
    // 两个历史行均已冻结，没有首次投递资格。
    let input = resource_rows(project.path(), &rows, parameters.clone())?;
    // Run actual hash verification and source materialization.
    // 运行实际哈希校验及来源物化。
    let prepared = prepare_v1_user_media(&input, Some(&context), Arc::clone(&registry)).await?;
    assert!(matches!(
        &prepared.prompts[&0].content[1],
        ContentItem::Text { .. }
    ));
    assert!(matches!(
        &prepared.prompts[&0].content[2],
        ContentItem::Resource { .. }
    ));
    assert!(
        matches!(&prepared.prompts[&0].content[3], ContentItem::Text { text } if text.contains("target does not support"))
    );
    assert!(matches!(
        &prepared.prompts[&1].content[1],
        ContentItem::Resource { .. }
    ));
    assert!(prepared.bindings.is_empty());
    assert_eq!(
        input
            .sourced_items
            .iter()
            .map(|item| item.user_prompt_parameters.clone())
            .collect::<Vec<_>>(),
        parameters
    );
    // Real read-only compaction projection must use the same actual-prefix accounting for these original source rows.
    // 对这些原来源行，真实只读压缩投影必须使用相同实际前缀核算。
    let projection = frozen_prompt_media_projection(
        input.clone(),
        context.session_scope_id.clone(),
        vec![("earlier".into(), 0), ("later".into(), 1)],
        registry,
    )?;
    // The selection order is exact, not lexical order or newest-first traversal.
    // 选择顺序精确，不使用字典序或从新到旧遍历。
    let readonly = projection
        .project_readonly(&["earlier".into(), "later".into()], context)
        .await?;
    assert_eq!(readonly["earlier"].content, prepared.prompts[&0].content);
    assert_eq!(readonly["later"].content, prepared.prompts[&1].content);
    Ok(())
}

/// Refunded encoded budget keeps the next new intent inline rather than uploading it under a stale preflight reservation.
/// 返还编码预算使下一新意图保持内联，而非依据陈旧预检预留上传它。
#[tokio::test]
async fn v1_refunded_budget_avoids_a_stale_new_upload_decision()
-> Result<(), Box<dyn std::error::Error>> {
    // Both files belong to this one real isolated project catalog.
    // 两个文件属于此真实隔离项目目录。
    let project = tempfile::tempdir()?;
    // Reuse the production project registry throughout preparation.
    // 在整个准备期间复用生产项目注册表。
    let registry = Arc::new(ProjectResourceCatalogRegistry::default());
    // The first planned inline source consumes a reservation until its hash fails.
    // 首个计划内联来源在哈希失败之前占用预留。
    let bad = register(project.path(), &registry, b"bad")?;
    // The second source must consume the refunded reservation without touching credentials.
    // 第二来源必须使用返还预留，不触碰凭据。
    let good = register(project.path(), &registry, b"one")?;
    std::fs::write(project.path().join(&bad.entity_relative_path), b"BAD")?;
    // A real upload channel exists, but the frozen preference permits these three-byte sources inline.
    // 存在真实上传通道，但冻结偏好允许这些三字节来源内联。
    let (mut context, secrets) = limited_context(true, 4, Some(4)).await?;
    Arc::get_mut(&mut context.execution)
        .expect("unique fixture")
        .media_delivery_policy
        .conversation
        .image
        .with_upload_max_bytes = 3;
    secrets.reject_reads.store(true, Ordering::SeqCst);
    // Failed credential reads are an offline tripwire, never a real network attempt.
    // 失败凭据读取是离线哨兵，绝不是真实网络尝试。
    let reads = secrets.reads.load(Ordering::SeqCst);
    // One prompt keeps both explicit new intents in source order.
    // 一个提示词按来源顺序保留两个显式新意图。
    let rows = vec![vec![bad.resource_id, good.resource_id]];
    // The input is unchanged between initial preflight and the real production preparation.
    // 输入在首次预检与真实生产准备之间保持不变。
    let input = resource_rows(
        project.path(),
        &rows,
        vec![UserPromptParameters::for_new_resources(&rows[0])],
    )?;
    // Confirm the regression setup really creates an upload decision under the tentative prefix budget.
    // 确认回归设置确实在暂定前缀预算下产生上传决定。
    let plans = plan_prompts(
        &input,
        &context,
        PromptMediaPreparationMode::Dispatch,
        &MediaUploadCancellation::new(),
    )?;
    assert_eq!(
        plans[0].resources[1].decision,
        MediaDeliveryDecision::Upload
    );
    // Capture must not reject this stale upload choice once actual earlier failure makes the next file inline again.
    // 实际更早失败使下一文件重新可内联后，捕获不能因陈旧上传选择而拒绝。
    let captured = prepare_user_media_with_mode(
        &input,
        Some(&context),
        Arc::clone(&registry),
        PromptMediaPreparationMode::Capture,
    )
    .await?;
    assert!(captured.bindings.is_empty());
    // The final decision uses actual earlier results while retaining the exact same declared upload option.
    // 最终决定使用实际更早结果，同时保留精确相同声明上传选项。
    let prepared = prepare_v1_user_media(&input, Some(&context), registry).await?;
    assert_eq!(captured.prompts[&0].content, prepared.prompts[&0].content);
    assert!(
        matches!(&prepared.prompts[&0].content[2], ContentItem::Resource { resource } if resource.source_kind == ResourceSourceKind::InlineData)
    );
    assert_eq!(
        prepared.bindings[0].origin.resources[1].delivery,
        UserPromptResourceDeliveryKind::Inline
    );
    assert_eq!(secrets.reads.load(Ordering::SeqCst), reads);
    assert!(
        load_project_resource_catalog(project.path())?
            .entries
            .iter()
            .all(|entry| entry.provider_bindings.is_empty())
    );
    Ok(())
}

/// A failed upload releases the message count for the next distinct intent, without retrying the failed file inline.
/// 上传失败为下一个独立意图释放消息数量，不把失败文件改为内联重试。
#[tokio::test]
async fn v1_failed_upload_releases_count_without_inline_fallback()
-> Result<(), Box<dyn std::error::Error>> {
    // The isolated project contains two different new upload intents.
    // 隔离项目包含两个不同新上传意图。
    let project = tempfile::tempdir()?;
    // All source lookups and receipt publications use the existing owner.
    // 全部来源查询及回执发布使用既有所有者。
    let registry = Arc::new(ProjectResourceCatalogRegistry::default());
    // The first source will fail at credential preparation before network access.
    // 第一来源将在网络访问前的凭据准备处失败。
    let first = register(project.path(), &registry, b"one")?;
    // The second source must be reconsidered only after the first yields no native content.
    // 第二来源必须仅在第一项没有产出原生内容后被重新判断。
    let second = register(project.path(), &registry, b"two")?;
    // Upload preference remains zero, and the target allows only one native resource per prompt.
    // 上传路线的内联偏好保持零，目标每个提示词仅允许一个原生资源。
    let (context, secrets) = limited_context(true, 8, Some(1)).await?;
    secrets.reject_reads.store(true, Ordering::SeqCst);
    // Exactly one credential attempt per distinct source is expected; no same-source fallback is authorized.
    // 每个独立来源精确预期一次凭据尝试；不授权同一来源回退。
    let reads = secrets.reads.load(Ordering::SeqCst);
    // Both new occurrences have no historical or draft upload receipt.
    // 两个新出现都没有历史或草稿上传回执。
    let rows = vec![vec![first.resource_id, second.resource_id]];
    // Ordered source identity is the sole input to retry-free preparation.
    // 有序来源身份是无重试准备的唯一输入。
    let input = resource_rows(
        project.path(),
        &rows,
        vec![UserPromptParameters::for_new_resources(&rows[0])],
    )?;
    // Both failures must remain explicit metadata, not success through Base64 or another channel.
    // 两次失败都必须保留明确元数据，不能通过Base64或其他通道转成成功。
    let prepared = prepare_v1_user_media(&input, Some(&context), registry).await?;
    for item in &prepared.prompts[&0].content[1..] {
        assert!(
            matches!(item, ContentItem::Text { text } if text.contains("selected upload could not be prepared"))
        );
    }
    assert_eq!(secrets.reads.load(Ordering::SeqCst), reads + 2);
    assert!(
        prepared.bindings[0]
            .origin
            .resources
            .iter()
            .all(|origin| matches!(
                origin.delivery,
                UserPromptResourceDeliveryKind::UploadPending { .. }
            ))
    );
    assert!(
        load_project_resource_catalog(project.path())?
            .entries
            .iter()
            .all(|entry| entry.provider_bindings.is_empty())
    );
    Ok(())
}

/// A count refund cannot let exact capture cross its no-new-upload authority boundary.
/// 数量返还不能让精确捕获跨越禁止新上传的权限边界。
#[tokio::test]
async fn capture_refunded_count_cannot_authorize_a_new_upload()
-> Result<(), Box<dyn std::error::Error>> {
    // This catalog owns one earlier inline proof and one new pending intent.
    // 此目录持有一个更早内联证明及一个新待准备意图。
    let project = tempfile::tempdir()?;
    // Keep mutations confined to the isolated existing registry.
    // 将修改限制在隔离既有注册表内。
    let registry = Arc::new(ProjectResourceCatalogRegistry::default());
    // The proven inline source is deliberately damaged without changing its size.
    // 故意损坏已证明内联来源，但不改变大小。
    let bad = register(project.path(), &registry, b"bad")?;
    // New media would require upload after the failed inline history releases the count.
    // 失败内联历史释放数量后，新媒体将要求上传。
    let good = register(project.path(), &registry, b"one")?;
    std::fs::write(project.path().join(&bad.entity_relative_path), b"BAD")?;
    // The upload preference stays zero; proven original inline history still remains inline-eligible.
    // 上传路线内联偏好保持零；已证明原内联历史仍具有内联资格。
    let (context, secrets) = limited_context(true, 8, Some(1)).await?;
    secrets.reject_reads.store(true, Ordering::SeqCst);
    // Any attempted upload would change this count even though credentials are offline.
    // 即使凭据离线，任何上传尝试也会改变此计数。
    let reads = secrets.reads.load(Ordering::SeqCst);
    // The existing prompt parameter contract explicitly supports prepared and new occurrences together.
    // 既有提示词参数契约明确支持已准备和新出现混合。
    let parameters = UserPromptParameters {
        version: 1,
        unproven_resource_positions: Vec::new(),
        prepared_resources: vec![
            Some(UserPromptResourceDelivery {
                resource_id: bad.resource_id,
                original_route: original_route(&context),
                delivery: UserPromptResourceDeliveryKind::Inline,
            }),
            None,
        ],
        media_delivery: UserPromptMediaDelivery::Pending,
    };
    // No upload appears in tentative preflight because the first native count temporarily excludes the second.
    // 暂定预检没有上传，因为第一项原生数量暂时排除第二项。
    let input = resource_rows(
        project.path(),
        &[vec![bad.resource_id, good.resource_id]],
        vec![Some(parameters)],
    )?;
    // Verify this exercises the final permission gate rather than only the initial capture preflight gate.
    // 验证此用例覆盖最终权限门，而非仅首次捕获预检门。
    let plans = plan_prompts(
        &input,
        &context,
        PromptMediaPreparationMode::Capture,
        &MediaUploadCancellation::new(),
    )?;
    assert!(
        plans[0]
            .resources
            .iter()
            .all(|resource| resource.decision != MediaDeliveryDecision::Upload)
    );
    // Actual capture must reject the newly eligible upload before credentials or receipts are touched.
    // 实际捕获必须在触碰凭据或回执之前拒绝新取得资格的上传。
    let error = prepare_user_media_with_mode(
        &input,
        Some(&context),
        registry,
        PromptMediaPreparationMode::Capture,
    )
    .await
    .err()
    .expect("capture cannot upload");
    assert!(
        error
            .to_string()
            .contains("requires a new upload at final chronological admission")
    );
    assert_eq!(secrets.reads.load(Ordering::SeqCst), reads);
    assert!(
        load_project_resource_catalog(project.path())?
            .entries
            .iter()
            .all(|entry| entry.provider_bindings.is_empty())
    );
    Ok(())
}

/// Free budget does not grant native eligibility to foreign uploads, unknown history or original address-only intents.
/// 空闲预算不会为其他模型上传、未知历史或原仅地址意图授予原生资格。
#[tokio::test]
async fn v1_refunded_budget_does_not_promote_address_only_ownership()
-> Result<(), Box<dyn std::error::Error>> {
    // One isolated catalog contains all canonical sources and the original A receipt.
    // 一个隔离目录包含全部规范来源及原A回执。
    let project = tempfile::tempdir()?;
    // This owner is shared by both model fixtures without sharing their per-occurrence identity.
    // 此所有者由两个模型夹具共享，但不共享其逐出现身份。
    let registry = Arc::new(ProjectResourceCatalogRegistry::default());
    // A failed new inline occurrence frees its complete encoded reservation.
    // 一个失败的新内联出现释放其完整编码预留。
    let bad = register(project.path(), &registry, b"bad")?;
    // This separate valid source appears in three different historical ownership states.
    // 此独立有效来源以三种不同历史归属状态出现。
    let entry = register(project.path(), &registry, b"one")?;
    // The original upload belongs to exact model A.
    // 原上传归精确模型A所有。
    let (a, _) = context(false, true).await?;
    // Keep the real scoped receipt fingerprint, not a file-global latest-upload guess.
    // 保留真实作用域回执指纹，而非文件全局最近上传猜测。
    let uploaded = seed_receipt(
        project.path(),
        &registry,
        &entry,
        &a,
        "file-a-budget",
        false,
    )?;
    // B supports both images and uploads, so only original ownership can keep A's history as an address.
    // B支持图片和上传，因此只有原归属能让A历史保持为地址。
    let (mut b, secrets) = context(true, true).await?;
    // The new bad file may initially be inline, freeing exactly four encoded bytes when it fails.
    // 新损坏文件最初可内联，失败时精确释放四个编码字节。
    let execution = Arc::get_mut(&mut b.execution).expect("unique fixture execution");
    execution
        .media_delivery_policy
        .conversation
        .max_total_encoded_bytes = 4;
    execution
        .media_delivery_policy
        .conversation
        .image
        .with_upload_max_bytes = 3;
    secrets.reject_reads.store(true, Ordering::SeqCst);
    // Credential resolution cannot occur for any address-only historical row.
    // 任何仅地址历史行都不能触发凭据解析。
    let reads = secrets.reads.load(Ordering::SeqCst);
    std::fs::write(project.path().join(&bad.entity_relative_path), b"BAD")?;
    // Original address-only delivery is distinct from history whose origin is missing.
    // 原仅地址投递与原来源缺失的历史不同。
    let address_only = UserPromptMediaOrigin {
        route: original_route(&b),
        resources: vec![UserPromptResourceDelivery {
            resource_id: entry.resource_id,
            original_route: original_route(&b),
            delivery: UserPromptResourceDeliveryKind::Attachment,
        }],
    };
    // All rows preserve their original sequence and occurrence-specific proof.
    // 全部行保留原顺序及逐出现证明。
    let rows = vec![
        vec![bad.resource_id],
        vec![entry.resource_id],
        vec![entry.resource_id],
        vec![entry.resource_id],
    ];
    // Only the first row is a new intent; the other rows cannot acquire first-delivery authority.
    // 只有第一行是新意图；其他行不能取得首次投递权限。
    let input = resource_rows(
        project.path(),
        &rows,
        vec![
            UserPromptParameters::for_new_resources(&rows[0]),
            frozen(uploaded),
            None,
            frozen(address_only),
        ],
    )?;
    // The actual preparation has available budget after the first failure but must retain all three ownership barriers.
    // 实际准备在首次失败后具有空闲预算，但必须保留三个归属屏障。
    let prepared = prepare_v1_user_media(&input, Some(&b), registry).await?;
    assert!(metadata(&prepared, 1).contains("belongs to another model or route"));
    assert!(
        metadata(&prepared, 2)
            .contains("original delivery of this historical attachment is unknown")
    );
    assert!(
        metadata(&prepared, 3)
            .contains("original intent retained this resource as an address-only attachment")
    );
    assert_eq!(prepared.bindings.len(), 1);
    assert_eq!(secrets.reads.load(Ordering::SeqCst), reads);
    Ok(())
}

/// An expired captured receipt cannot restore merely because its earlier preflight cached a usable flag.
/// 已过期捕获回执不能仅因更早预检缓存了可用标志而恢复。
#[tokio::test]
async fn final_admission_rechecks_exact_original_receipt_validity()
-> Result<(), Box<dyn std::error::Error>> {
    // This isolated catalog owns a valid original upload and its typed immutable prompt proof.
    // 此隔离目录持有有效原上传及其强类型不可变提示词证明。
    let project = tempfile::tempdir()?;
    // Keep the normal production receipt registry and scope checks.
    // 保留正常生产回执注册表及作用域检查。
    let registry = ProjectResourceCatalogRegistry::default();
    // Source identity is stable throughout this receipt-validity fault injection.
    // 在此次回执有效性故障注入期间，来源身份保持稳定。
    let entry = register(project.path(), &registry, b"one")?;
    // The current target is exactly the original upload model.
    // 当前目标精确为原上传模型。
    let (context, secrets) = context(false, true).await?;
    // Seed an actually valid scoped receipt, then capture its normal plan.
    // 植入实际有效作用域回执，再捕获其正常规划。
    let origin = seed_receipt(
        project.path(),
        &registry,
        &entry,
        &context,
        "file-original-expiry",
        false,
    )?;
    // Frozen history never receives permission to perform a fresh upload.
    // 冻结历史绝不取得执行新上传的权限。
    let input = snapshot(project.path(), entry.resource_id, vec![frozen(origin)])?;
    // Initial preflight genuinely permits restoration rather than already being address-only.
    // 首次预检确实允许恢复，而非已经仅为地址。
    let mut plans = plan_prompts(
        &input,
        &context,
        PromptMediaPreparationMode::Dispatch,
        &MediaUploadCancellation::new(),
    )?;
    // Fault injection changes only the captured private receipt's validity, never the original prompt proof or the persisted catalog.
    // 故障注入仅改变捕获私有回执的有效性，绝不改变原提示词证明或持久目录。
    let resource = &mut plans[0].resources[0];
    assert_eq!(
        resource.decision,
        MediaDeliveryDecision::RestoreOriginalUpload
    );
    assert!(matches!(
        resource.admission_origin,
        Some(MediaDeliveryOrigin::HistoricalUpload {
            reference_usable: true,
            ..
        })
    ));
    resource
        .original_receipt
        .as_mut()
        .expect("captured receipt")
        .upload_receipt
        .as_mut()
        .expect("typed receipt")
        .expires_at_unix_millis = 2;
    // Final admission may inspect this exact receipt, but cannot refresh it through credentials or another binding.
    // 最终准入可检查此精确回执，但不能通过凭据或另一个绑定刷新它。
    let reads = secrets.reads.load(Ordering::SeqCst);
    // Empty request budget ensures expiry, not competition with earlier files, causes the rejection.
    // 空请求预算确保由过期而非更早文件竞争导致拒绝。
    let mut planner = MediaDeliveryPlanner::new(
        context.execution.media_delivery_policy,
        MediaDeliveryPurpose::Conversation,
    )?;
    replan_resource_for_materialization(resource, &context, &mut planner, 1)?;
    assert_eq!(
        resource.decision,
        MediaDeliveryDecision::Attachment {
            reason: MediaDeliveryRejection::OriginalReferenceUnavailable
        }
    );
    assert!(matches!(
        resource.original.as_ref().expect("original proof").delivery,
        UserPromptResourceDeliveryKind::Uploaded { .. }
    ));
    assert_eq!(planner.encoded_bytes_reserved(), 0);
    assert_eq!(secrets.reads.load(Ordering::SeqCst), reads);
    Ok(())
}
