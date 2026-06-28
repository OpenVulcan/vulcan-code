//! Shared host-side compatibility adapters for configured image, video, speech, and music capability vendors.
//! 为已配置图片、视频、语音与音乐能力供应商提供共享宿主兼容适配器。

mod assets;
mod codecs;
mod dispatch;
mod endpoints;
mod http;
mod invocation;
mod minimax_requests;
mod schema;
mod targets;
mod tasks;
mod types;

pub use dispatch::{
    execute_configured_image_generation, execute_configured_music_cover,
    execute_configured_music_generation, execute_configured_speech_synthesis,
    execute_configured_video_generation,
};
pub use schema::{
    configured_image_generation_parameters_schema, configured_music_cover_parameters_schema,
    configured_music_generation_parameters_schema, configured_speech_synthesis_parameters_schema,
    configured_video_generation_parameters_schema,
};

#[cfg(test)]
mod tests {
    use super::{
        assets::{
            generated_asset_to_model_output_json, resolve_attachment_from_local_path,
            resolve_generated_output_directory, stage_generated_asset_bytes,
            with_optional_output_sequence,
        },
        invocation::parse_image_generation_invocation,
        types::{GeneratedAssetOutputOptions, StagedGeneratedAsset},
    };
    use crate::attachment_assets::encode_rgba_image_to_png_bytes;
    use crate::workspace_path::normalized_workspace_path_text_from_path;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};
    use vulcan_core::types::{ConversationAttachment, ConversationAttachmentKind};

    /// Builds one unique temporary workspace directory path so media-generation tests stay isolated.
    /// 构建一条唯一的临时工作区目录路径，使媒体生成测试彼此隔离。
    fn unique_workspace_directory() -> std::path::PathBuf {
        let unique_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("vulcan-media-generation-test-{unique_id}"));
        std::fs::create_dir_all(&directory).expect("temporary workspace directory should create");
        directory
    }

    /// Removes one temporary workspace directory after a media-generation test has finished.
    /// 在媒体生成测试结束后移除一条临时工作区目录。
    fn cleanup_workspace_directory(directory: &std::path::Path) {
        let _ = std::fs::remove_dir_all(directory);
    }

    /// Verifies caller-selected output stems are numbered only when one generation request returns multiple files.
    /// 验证只有在一次生成请求返回多个文件时，才会对调用方选定的输出名称主体追加编号。
    #[test]
    fn optional_output_stems_are_numbered_for_multi_output_requests() {
        let options = GeneratedAssetOutputOptions {
            output_dir: Some(String::from("exports")),
            output_file_stem: Some(String::from("cover")),
        };

        let single_output = with_optional_output_sequence(&options, 1, 0);
        assert_eq!(single_output.output_file_stem.as_deref(), Some("cover"));

        let multi_output = with_optional_output_sequence(&options, 3, 1);
        assert_eq!(multi_output.output_file_stem.as_deref(), Some("cover-002"));
    }

    /// Verifies shared generated-output options are parsed from the semantic image-generation invocation payload.
    /// 验证可以从语义图片生成调用载荷中解析出共享的生成输出选项。
    #[test]
    fn image_generation_invocation_parses_generated_output_options() {
        let invocation = parse_image_generation_invocation(serde_json::json!({
            "prompt": "render one skyline",
            "output_dir": "exports/images",
            "output_file_stem": "skyline.png"
        }))
        .expect("image generation invocation should parse");

        assert_eq!(
            invocation.output.output_dir.as_deref(),
            Some("exports/images")
        );
        assert_eq!(
            invocation.output.output_file_stem.as_deref(),
            Some("skyline.png")
        );
    }

    /// Verifies generated assets can be exported to a caller-selected directory while still keeping one managed attachment copy for replay.
    /// 验证可以把生成资产导出到调用方选定目录，同时仍保留一份托管附件副本用于回放。
    #[test]
    fn generated_assets_can_export_copy_and_keep_managed_attachment() {
        let workspace_directory = unique_workspace_directory();
        let workspace_path = workspace_directory.to_string_lossy().into_owned();
        let png_bytes = encode_rgba_image_to_png_bytes(1, 1, &[255_u8, 0, 0, 255])
            .expect("png bytes should encode");
        let output = stage_generated_asset_bytes(
            &workspace_path,
            "image/png",
            "provider-output.jpg",
            &png_bytes,
            &GeneratedAssetOutputOptions {
                output_dir: Some(String::from("exports")),
                output_file_stem: Some(String::from("poster.jpeg")),
            },
        )
        .expect("generated asset should stage");

        assert!(Path::new(&output.model_local_path).is_file());
        assert!(Path::new(&output.attachment.local_path).is_file());
        assert_ne!(
            output.model_local_path,
            output.attachment.local_path.replace('\\', "/")
        );
        assert_eq!(output.attachment.file_name, "poster.png");
        let expected_source_path = output.model_local_path.replace('\\', "/");
        assert_eq!(
            output
                .attachment
                .source_path
                .as_deref()
                .map(|value| value.replace('\\', "/"))
                .as_deref(),
            Some(expected_source_path.as_str())
        );

        cleanup_workspace_directory(&workspace_directory);
    }

    /// Verifies model-visible generated-asset payloads now expose the preferred local path required for immediate follow-up tool calls.
    /// 验证模型可见的生成资产载荷现在会暴露首选本地路径，以支持立即发起后续工具调用。
    #[test]
    fn generated_asset_model_output_exposes_preferred_local_path() {
        let output = StagedGeneratedAsset {
            attachment: ConversationAttachment {
                kind: ConversationAttachmentKind::Image,
                file_name: String::from("diagram.png"),
                local_path: String::from(r"D:\managed\diagram.png"),
                source_path: None,
                asset_id: Some(String::from("asset-1")),
                byte_len: Some(42),
                preview_local_path: None,
                mime_type: String::from("image/png"),
            },
            model_local_path: String::from(r"D:\exports\diagram.png"),
        };

        let value = generated_asset_to_model_output_json(&output);
        assert_eq!(
            value.get("local_path").and_then(serde_json::Value::as_str),
            Some(r"D:\exports\diagram.png")
        );
    }

    /// Verifies workspace-relative output directories resolve under the active workspace root.
    /// 验证工作区相对输出目录会被解析到当前工作区根目录下。
    #[test]
    fn relative_generated_output_directories_resolve_from_workspace_root() {
        let workspace_directory = unique_workspace_directory();
        let workspace_path = workspace_directory.to_string_lossy().into_owned();
        let resolved_directory =
            resolve_generated_output_directory(&workspace_path, "exports/videos")
                .expect("workspace-relative output directory should resolve");
        let expected_directory = workspace_directory.join("exports").join("videos");

        assert_eq!(
            normalized_workspace_path_text_from_path(&resolved_directory),
            normalized_workspace_path_text_from_path(&expected_directory)
        );

        cleanup_workspace_directory(&workspace_directory);
    }

    /// Verifies relative generated-output directories fail when no workspace anchor exists.
    /// 验证缺少工作区锚点时，相对生成输出目录会显式失败。
    #[test]
    fn relative_generated_output_directories_require_workspace_anchor() {
        let error = resolve_generated_output_directory("", "exports/videos")
            .expect_err("relative output directory should require workspace path");

        assert!(
            error.contains("non-empty workspace path"),
            "unexpected error text: {error}"
        );
    }

    /// Verifies relative direct local selectors do not resolve without a workspace anchor.
    /// 验证缺少工作区锚点时，相对直接本地选择器不会被解析。
    #[test]
    fn relative_direct_local_selectors_require_workspace_anchor() {
        let attachment = resolve_attachment_from_local_path(
            ConversationAttachmentKind::Image,
            "exports/shot.png",
            "",
        );

        assert!(attachment.is_none());
    }

    /// Verifies follow-up media selectors can resolve one direct local file path even when that path was not part of the original attachment snapshot.
    /// 验证即使某条本地文件路径不在原始附件快照中，后续媒体选择器仍然可以直接解析它。
    #[test]
    fn direct_local_media_selectors_resolve_without_attachment_snapshot() {
        let workspace_directory = unique_workspace_directory();
        let workspace_path = workspace_directory.to_string_lossy().into_owned();
        let export_directory = workspace_directory.join("exports");
        std::fs::create_dir_all(&export_directory).expect("export directory should create");
        let export_path = export_directory.join("shot.png");
        let png_bytes = encode_rgba_image_to_png_bytes(1, 1, &[0_u8, 255, 0, 255])
            .expect("png bytes should encode");
        std::fs::write(&export_path, png_bytes).expect("export file should write");

        let attachment = resolve_attachment_from_local_path(
            ConversationAttachmentKind::Image,
            "exports/shot.png",
            &workspace_path,
        )
        .expect("direct local selector should resolve");

        assert_eq!(attachment.file_name, "shot.png");
        assert!(Path::new(&attachment.local_path).is_file());
        assert_eq!(attachment.mime_type, "image/png");

        cleanup_workspace_directory(&workspace_directory);
    }
}
