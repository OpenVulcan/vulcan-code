//! Model-specific conversion before composer delivery intentions are frozen.
//! 在冻结输入区交付意图之前按模型转换图片。

use super::*;

/// Converts new images in `attachments` for frozen `execution`, using the exact `workspace_path` and live `draft_scope`.
/// 使用精确`workspace_path`及活动`draft_scope`，为冻结`execution`转换`attachments`中的新图片。
/// Returns replacement/error rows at caller `generation`, or none when ordinary planning can proceed unchanged.
/// 返回调用方`generation`的替换或错误记录；可以保持原样继续普通规划时返回空。
pub(super) async fn prepare_image_conversions(
    workspace_path: &str,
    draft_scope: &str,
    generation: u64,
    execution: &vulcan_provider_adapters::ResolvedPublicAiExecution,
    attachments: &[ComposerAttachmentSummary],
) -> Result<Option<Vec<ComposerAttachmentPreparationSummary>>, String> {
    if !execution
        .model
        .capabilities
        .input_resources
        .contains(&vulcan_ai_contract::ResourceKind::Image)
    {
        return Ok(None);
    }
    // Capability checks use the final provider/model constraint precedence already resolved by the runtime.
    // 能力检查使用运行时已解析的最终供应商和模型约束优先级。
    let accepts = |mime: &str| {
        vulcan_agent_runtime::image_conversion::native_image_mime_supported(
            &execution.route.protocol_route.protocol_profile_id,
            &execution.model.capabilities.input_resources,
            &execution.route.protocol_route.input_resource_constraints,
            mime,
        )
    };
    // Build a complete response without publishing any upload plan that a replacement would invalidate.
    // 构建完整响应，不发布会被替换失效的任何上传规划。
    let mut rows = Vec::with_capacity(attachments.len());
    // Only an actual conversion attempt changes the normal planning path.
    // 只有实际转换尝试才改变普通规划路径。
    let mut attempted = false;
    for (position, summary) in attachments.iter().enumerate() {
        // Project references preserve historical delivery identity and are outside new paste staging.
        // 项目引用保留历史交付身份，不属于新粘贴暂存。
        let attachment_id = match summary.project_resource_id {
            Some(id) => format!("project-resource:{id}:{position}"),
            None => summary
                .asset_id
                .clone()
                .ok_or("Composer image has no stable asset identity.")?,
        };
        // Each row stays bound to its original occurrence until the browser accepts the new asset.
        // 浏览器接受新资产前，每条记录始终绑定原始出现。
        let mut row = ComposerAttachmentPreparationSummary {
            converted_attachment: None,
            attachment_id,
            generation,
            transfer_id: format!("draft-image-conversion:{}", uuid::Uuid::new_v4()),
            physical_transfer_id: None,
            delivery_plan: ComposerAttachmentDeliveryPlan::MetadataOnly,
            phase: ComposerAttachmentPreparationPhase::LocalReady,
            blocks_submission: true,
            route_revision: Some(execution.route.route_revision.clone()),
            reason: "image_conversion_batch_replan_required".into(),
            error: None,
        };
        if summary.project_resource_id.is_none() {
            // Resolve authoritative metadata before reading source bytes; browser MIME/path fields are not authority.
            // 在读取源字节前解析权威元数据；浏览器MIME和路径字段不构成权威。
            let attachment = composer_attachment_summary_to_core(
                resolve_composer_attachment_summary_for_workspace(workspace_path, summary.clone())?,
            )?;
            // Live draft ownership is required even if this image needs no conversion.
            // 即使此图片无需转换，也要求活动草稿所有权。
            let original =
                vulcan_agent_runtime::attachment_asset_index::load_attachment_draft_media(
                    &attachment,
                    draft_scope,
                )?;
            if original.is_none()
                && attachment.kind == ConversationAttachmentKind::Image
                && !accepts(&attachment.mime_type)
            {
                attempted = true;
                match stage_converted_image(workspace_path, &attachment, accepts("image/png")).await
                {
                    Ok(converted) => row.converted_attachment = Some(converted),
                    Err(error) => {
                        row.phase = ComposerAttachmentPreparationPhase::Failed;
                        row.reason = "image_conversion_failed".into();
                        row.error = Some(error);
                    }
                }
            }
        }
        rows.push(row);
    }
    Ok(attempted.then_some(rows))
}

/// Converts rejected `attachment` for a destination that may `accepts_png`, then stages its canonical derivative.
/// 为可能`accepts_png`的目标转换已拒绝的`attachment`，再暂存规范派生产物。
/// `workspace_path` owns the new asset; returns its summary or the concrete conversion/staging failure.
/// `workspace_path`持有新资产；返回其摘要或具体转换及暂存失败。
async fn stage_converted_image(
    workspace_path: &str,
    attachment: &ConversationAttachment,
    accepts_png: bool,
) -> Result<ComposerAttachmentSummary, String> {
    // Temporary ownership protects the derivative until the canonical store has copied it.
    // 临时所有权保护派生文件，直到规范存储复制完成。
    let converted = vulcan_agent_runtime::image_conversion::convert_attachment_if_needed(
        attachment,
        false,
        accepts_png,
    )
    .await?
    .ok_or("Rejected image did not produce a PNG conversion.")?;
    // The converter has already bounded and closed this private output file.
    // 转换器已经限制并完成此私有输出文件。
    let bytes = tokio::fs::read(&converted.attachment.local_path)
        .await
        .map_err(|error| error.to_string())?;
    // Canonical PNG descriptor is shared with ordinary pasted PNG attachments.
    // 规范PNG描述符与普通粘贴PNG附件共享。
    let descriptor =
        vulcan_agent_runtime::attachment_assets::attachment_descriptor_from_mime_type("image/png")
            .expect("registered PNG descriptor");
    // Publish a distinct content-addressed asset while preserving the user's readable filename.
    // 发布独立的内容寻址资产，同时保留用户可读文件名。
    let staged = vulcan_agent_runtime::attachment_assets::stage_project_attachment_bytes(
        workspace_path,
        &descriptor,
        &converted.attachment.file_name,
        None,
        &bytes,
    )?;
    Ok(composer_attachment_summary_from_core(&staged))
}

/// Exercises canonical paste staging, conversion, lease replacement and replan against exact offline `execution`.
/// 使用精确离线`execution`验证规范粘贴暂存、转换、租约替换及重新规划。
#[cfg(test)]
pub(crate) async fn exercise_composer_image_conversion(
    execution: &vulcan_provider_adapters::ResolvedPublicAiExecution,
) -> Result<(), String> {
    // A real TIFF enters the exact byte-staging entry used by browser paste and file upload.
    // 真实TIFF进入浏览器粘贴及文件上传使用的精确字节暂存入口。
    let workspace = tempfile::tempdir().map_err(|error| error.to_string())?;
    // UTF-8 workspace identity is required by the real managed asset store.
    // 真实托管资产存储要求UTF-8工作区身份。
    let workspace_path = workspace.path().to_str().ok_or("Invalid fixture path")?;
    // Real TIFF bytes exercise decoding, not extension relabeling.
    // 真实TIFF字节验证解码，而非扩展名改标。
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(2, 1, image::Rgb([15, 30, 90])))
        .write_to(&mut bytes, image::ImageFormat::Tiff)
        .map_err(|error| error.to_string())?;
    // The staging descriptor comes from the same registry used by browser uploads.
    // 暂存描述符来自浏览器上传使用的同一注册表。
    let descriptor =
        vulcan_agent_runtime::attachment_assets::attachment_descriptor_from_mime_type("image/tiff")
            .unwrap();
    // Preserve the canonical original so replacement cannot mutate its identity.
    // 保留规范原始附件，确保替换不能改变其身份。
    let original = vulcan_agent_runtime::attachment_assets::stage_project_attachment_bytes(
        workspace_path,
        &descriptor,
        "pasted.tiff",
        None,
        bytes.get_ref(),
    )?;
    // One live draft lease owns both the original and subsequent replacement transactions.
    // 一个活动草稿租约持有原始及后续替换事务。
    let scope = crate::attachments::composer_attachment_draft_scope("new:image-conversion")?;
    // Borrow the canonical service-produced scope rather than reproducing its namespace in the fixture.
    // 借用服务产出的规范作用域，不在夹具中复刻其命名空间。
    let scope = scope.as_str();
    vulcan_agent_runtime::attachment_asset_index::reconcile_staged_attachment_ref_scope(
        workspace_path,
        scope,
        std::slice::from_ref(&original),
    )?;
    // Send the same canonical summary shape that the browser planning RPC uses.
    // 传入浏览器规划RPC使用的同一规范摘要形状。
    let summaries = vec![composer_attachment_summary_from_core(&original)];
    // The first plan must request replacement before publishing any upload intention.
    // 首次规划必须在发布任何上传意图前请求替换。
    let rows = prepare_image_conversions(workspace_path, scope, 7, execution, &summaries)
        .await?
        .expect("TIFF must be converted for default native image formats");
    // This fixture contains exactly one requested occurrence.
    // 此夹具恰好包含一次请求出现。
    let row = rows.first().unwrap();
    assert!(row.blocks_submission);
    assert_eq!(
        row.attachment_id,
        original.asset_id.as_ref().unwrap().as_str()
    );
    // Inspect the service-issued derivative, not a browser-created metadata clone.
    // 检查服务签发的派生产物，而非浏览器创建的元数据克隆。
    let converted = row.converted_attachment.as_ref().unwrap();
    assert_eq!(converted.mime_type, "image/png");
    assert_eq!(converted.file_name, "pasted.png");
    assert_ne!(converted.asset_id, original.asset_id);
    // Verify conversion did not alter the original before the draft legitimately releases its old lease.
    // 在草稿合法释放旧租约前，验证转换没有改变原始内容。
    assert_eq!(
        std::fs::read(&original.local_path).map_err(|error| error.to_string())?,
        *bytes.get_ref()
    );
    // The new source acquires its lease before ordinary delivery replanning.
    // 新来源在普通交付重新规划前取得租约。
    let staged = composer_attachment_summary_to_core(converted.clone())?;
    vulcan_agent_runtime::attachment_asset_index::reconcile_staged_attachment_ref_scope(
        workspace_path,
        scope,
        std::slice::from_ref(&staged),
    )?;
    assert!(
        prepare_image_conversions(
            workspace_path,
            scope,
            8,
            execution,
            std::slice::from_ref(converted)
        )
        .await?
        .is_none()
    );
    // Invalid image bytes yield the concrete decoding error and block submission.
    // 无效图片字节返回具体解码错误，并阻止提交。
    let corrupt = vulcan_agent_runtime::attachment_assets::stage_project_attachment_bytes(
        workspace_path,
        &descriptor,
        "broken.tiff",
        None,
        b"invalid TIFF",
    )?;
    vulcan_agent_runtime::attachment_asset_index::reconcile_staged_attachment_ref_scope(
        workspace_path,
        scope,
        std::slice::from_ref(&corrupt),
    )?;
    // A failed decoder returns a visible row bound to the same draft generation.
    // 解码失败返回绑定同一草稿代际的可见记录。
    let failed = prepare_image_conversions(
        workspace_path,
        scope,
        9,
        execution,
        &[composer_attachment_summary_from_core(&corrupt)],
    )
    .await?
    .unwrap();
    // The exact error remains available to the retry UI.
    // 精确错误保留给重试界面。
    let failure = failed.first().unwrap();
    assert_eq!(failure.reason, "image_conversion_failed");
    assert!(failure.blocks_submission);
    assert!(
        failure
            .error
            .as_deref()
            .unwrap()
            .contains("Image conversion from image/tiff to image/png failed")
    );
    assert!(failure.converted_attachment.is_none());
    Ok(())
}
