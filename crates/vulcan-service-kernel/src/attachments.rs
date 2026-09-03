//! WebUI attachment cleanup and composer attachment helpers.
//! WebUI 附件清理与输入区附件辅助逻辑。

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use serde::{Deserialize, Serialize};
use vulcan_agent_runtime::{
    attachment_asset_index::{
        AttachmentCleanupPreview, AttachmentCleanupReport, AttachmentCleanupSelection,
        execute_attachment_cleanup, preview_attachment_cleanup,
        reconcile_staged_attachment_ref_scope, resolve_indexed_attachment_resource,
    },
    attachment_assets::{
        attachment_descriptor_from_mime_type, attachment_descriptor_from_path,
        resolve_attachment_image_preview_sidecar, stage_project_attachment_bytes,
        stage_project_attachment_file, stage_project_generic_attachment_bytes,
        write_attachment_image_preview_sidecar,
    },
};
use vulcan_core::types::{ConversationAttachment, ConversationAttachmentKind};
use vulcan_service_api::{ComposerAttachmentCapabilitySummary, ComposerAttachmentSummary};

use crate::{
    SharedServiceBridgeState, normalized_workspace_path_text_from_path, parse_session_id,
    spawn_service_blocking_task, vmm_project_scope_path, workspace_root,
};
mod capabilities;
mod cleanup;
mod conversion;
mod staging;

pub(super) use capabilities::*;
pub(super) use cleanup::*;
pub(super) use conversion::*;
pub(super) use staging::*;
