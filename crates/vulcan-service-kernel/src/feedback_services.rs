//! Feedback drafts and submission, the user's feedback list, and the message center (announcements
//! and personal messages) backed by the OpenVulcan user center.
//! 反馈草稿与提交、用户的反馈列表，以及由 OpenVulcan 用户中心支撑的消息中心（公告与个人消息）。
//!
//! Nothing here sends a report on its own: [`service_prepare_feedback`] filters and encodes a draft
//! on this machine and keeps the exact bytes, and only [`service_submit_feedback`], called from the
//! user's own send action with the digest they previewed, transmits them. Messages never enter any
//! model context. Contract: `docs/contracts/feedback.md`.
//! 这里不会自行发送任何报告：[`service_prepare_feedback`] 在本机过滤并编码草稿、保留精确字节，只有用户
//! 亲自点击发送并带上其预览过的摘要时，[`service_submit_feedback`] 才会发送这些字节。消息永不进入任何
//! 模型上下文。契约见 `docs/contracts/feedback.md`。

use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use vulcan_agent_runtime::tooling::native::{
    ENVIRONMENT_SECRET_REDACTION_MIN_CHARS, EnvironmentVariableKind,
};
use vulcan_feedback::limits::PREPARED_DRAFTS_KEPT;
use vulcan_feedback::{
    FeedbackClientInfo, FeedbackDraft, FeedbackErrorAttachment, FeedbackErrorCode, FeedbackKind,
    FeedbackSource, KnownSecret, PreparedFeedback, Redactor, RedactorInputs,
};
use vulcan_provider_adapters::{
    CredentialExecutionSecret, OpenVulcanAnnouncementDocument, OpenVulcanAnnouncementLink,
    OpenVulcanAnnouncementsFetch, OpenVulcanFeedbackDetail, OpenVulcanFeedbackLinks,
    OpenVulcanFeedbackPage, OpenVulcanInboxError, OpenVulcanMarkRead, OpenVulcanMarkReadResult,
    OpenVulcanMessagePage, OpenVulcanRegion,
};
use vulcan_service_api::{WebuiLocalizedMessage, WebuiServiceRpcError};
use vulcan_workspace_store::workspace_store::{
    AnnouncementMark, list_environment_variables, load_announcement_states, mark_announcements,
    prune_announcement_states,
};

use crate::SharedServiceBridgeState;
use crate::public_ai_settings::openvulcan_account::signed_in_credential;
use crate::service_state_access::{
    mutate_provider_configuration, provider_configuration_from_state,
};

/// Result of a feedback or message-center service call.
/// 反馈或消息中心服务调用的结果。
type InboxResult<T> = Result<T, WebuiServiceRpcError>;

/// Failure facts of an error card, as the interface parsed them from the transcript.
/// 界面从转录中解析出的错误卡片失败事实。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct FeedbackErrorInput {
    category: String,
    #[serde(default)]
    subcategory: Option<String>,
    #[serde(default)]
    stage: Option<String>,
    #[serde(default)]
    http_status: Option<u16>,
    #[serde(default)]
    provider_id: Option<String>,
    #[serde(default)]
    model_id: Option<String>,
    #[serde(default)]
    upstream_code: Option<String>,
    #[serde(default)]
    request_id: Option<String>,
    #[serde(default)]
    retryable: Option<bool>,
    #[serde(default)]
    message: Option<String>,
}

/// One draft as the feedback window or the AI draft card holds it.
/// 反馈窗口或 AI 草稿卡持有的一份草稿。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct FeedbackDraftInput {
    /// Draft id of an earlier preparation, reused as the idempotency key of every retry.
    /// 先前准备返回的草稿编号，作为每次重试的幂等键复用。
    #[serde(default)]
    draft_id: Option<String>,
    kind: FeedbackKind,
    source: FeedbackSource,
    title: String,
    body: String,
    allow_contact: bool,
    include_client_info: bool,
    include_log: bool,
    #[serde(default)]
    error: Option<FeedbackErrorInput>,
}

/// Exact preview of what a send would transmit.
/// 发送时将传输内容的精确预览。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FeedbackPreview {
    /// Draft id to pass back when sending or preparing again.
    /// 发送或再次准备时传回的草稿编号。
    draft_id: String,
    /// SHA-256 of the exact request bytes.
    /// 精确请求字节的 SHA-256。
    digest: String,
    /// The request, pretty-printed; the same content as the bytes sent.
    /// 美化排版的请求；与发送的字节内容相同。
    payload: String,
    /// Size of the exact request bytes.
    /// 精确请求字节的大小。
    byte_length: usize,
    /// Replacements per category.
    /// 各类别的替换次数。
    redaction_hits: BTreeMap<String, u32>,
    /// Log lines included after filtering and trimming.
    /// 过滤与裁剪后附带的日志行数。
    log_line_count: usize,
    /// Whether an OpenVulcan account is signed in, so the draft can be sent now.
    /// 是否已登录 OpenVulcan 账户，从而可以立即发送。
    signed_in: bool,
}

/// One prepared draft awaiting the user's send.
/// 一份等待用户发送的已准备草稿。
struct StoredDraft {
    draft_id: String,
    prepared: PreparedFeedback,
}

/// Prepared drafts of this process, newest last.
/// 本进程的已准备草稿，最新的在末尾。
fn prepared_drafts() -> &'static Mutex<VecDeque<StoredDraft>> {
    static DRAFTS: OnceLock<Mutex<VecDeque<StoredDraft>>> = OnceLock::new();
    DRAFTS.get_or_init(|| Mutex::new(VecDeque::new()))
}

/// Maps a preparation failure to its catalog message.
/// 将准备失败映射为目录文案。
fn draft_error(error: vulcan_feedback::FeedbackError) -> WebuiServiceRpcError {
    let message = match error.code() {
        FeedbackErrorCode::TitleRequired => {
            WebuiLocalizedMessage::new("serviceError.feedback.titleRequired")
        }
        FeedbackErrorCode::TitleTooLong => {
            WebuiLocalizedMessage::new("serviceError.feedback.titleTooLong")
        }
        FeedbackErrorCode::BodyRequired => {
            WebuiLocalizedMessage::new("serviceError.feedback.bodyRequired")
        }
        FeedbackErrorCode::BodyTooLong => {
            WebuiLocalizedMessage::new("serviceError.feedback.bodyTooLong")
        }
        FeedbackErrorCode::InvalidField => {
            WebuiLocalizedMessage::new("serviceError.feedback.invalidField")
                .with("field", error.detail())
        }
        FeedbackErrorCode::RequestTooLarge => {
            WebuiLocalizedMessage::new("serviceError.feedback.requestTooLarge")
        }
        FeedbackErrorCode::FilterFailed => {
            tracing::warn!("[feedback] filtering failed: {}", error.detail());
            WebuiLocalizedMessage::new("serviceError.feedback.filterFailed")
        }
    };
    message.into()
}

/// Maps a user-center failure to its catalog message.
/// 将用户中心失败映射为目录文案。
fn inbox_error(error: OpenVulcanInboxError) -> WebuiServiceRpcError {
    match error {
        OpenVulcanInboxError::InvalidArgument(detail) => {
            WebuiServiceRpcError::InvalidParams(detail.to_owned())
        }
        OpenVulcanInboxError::Transport => {
            WebuiLocalizedMessage::new("serviceError.inbox.unreachable").into()
        }
        OpenVulcanInboxError::InvalidAnswer => {
            WebuiLocalizedMessage::new("serviceError.inbox.invalidAnswer").into()
        }
        OpenVulcanInboxError::Rejected {
            status,
            code,
            retry_after_secs,
        } => match (status, code.as_str()) {
            (401, _) => WebuiLocalizedMessage::new("serviceError.inbox.signInExpired").into(),
            (_, "rate_limited") => {
                let minutes = retry_after_secs.unwrap_or(60).div_ceil(60).max(1);
                WebuiLocalizedMessage::new("serviceError.inbox.rateLimited")
                    .with("minutes", minutes.to_string())
                    .into()
            }
            (_, "feedback_not_found") => {
                WebuiLocalizedMessage::new("serviceError.inbox.feedbackNotFound").into()
            }
            (_, "message_not_found") => {
                WebuiLocalizedMessage::new("serviceError.inbox.messageNotFound").into()
            }
            (_, "invalid_link") => {
                WebuiLocalizedMessage::new("serviceError.inbox.invalidLink").into()
            }
            (_, "payload_too_large") => {
                WebuiLocalizedMessage::new("serviceError.feedback.requestTooLarge").into()
            }
            (status, _) if status >= 500 => {
                WebuiLocalizedMessage::new("serviceError.inbox.unavailable").into()
            }
            (status, code) => {
                let reason = if code.is_empty() {
                    format!("HTTP {status}")
                } else {
                    code.to_owned()
                };
                WebuiLocalizedMessage::new("serviceError.inbox.refused")
                    .with("reason", reason)
                    .into()
            }
        },
    }
}

/// Builds the transport used for every user-center call; redirects are never followed.
/// 构建所有用户中心调用使用的传输；从不跟随重定向。
fn inbox_http() -> InboxResult<reqwest::Client> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| {
            WebuiServiceRpcError::Service(String::from("Failed to create inbox transport."))
        })
}

/// Returns the signed-in account's credential or the catalog message asking to sign in.
/// 返回已登录账户的凭据，或要求登录的目录文案。
async fn required_credential(
    state: &SharedServiceBridgeState,
) -> InboxResult<CredentialExecutionSecret> {
    signed_in_credential(state)
        .await
        .map_err(WebuiServiceRpcError::Service)?
        .map(|(_, secret)| secret)
        .ok_or_else(|| WebuiLocalizedMessage::new("serviceError.inbox.signInRequired").into())
}

/// Interface locale of this service, or the operating system's best supported language.
/// 本服务的界面语言；未设置时取操作系统最匹配的受支持语言。
fn interface_locale(state: &SharedServiceBridgeState) -> String {
    crate::service_ui_locale_from_state(state)
        .and_then(|locale| vulcan_i18n::normalize(&locale).map(str::to_owned))
        .unwrap_or_else(|| vulcan_i18n::system_locale().to_owned())
}

/// Collects every secret value this process knows: the values masked in tool output plus every
/// stored secret variable of any scope that is long enough to mask.
/// 收集本进程已知的全部秘密值：工具输出中遮蔽的值，以及任一作用域中长度足以遮蔽的已存储密钥变量。
async fn known_secrets(state: &SharedServiceBridgeState) -> InboxResult<Vec<KnownSecret>> {
    let (database, task_runtime) = {
        let guard = state
            .lock()
            .map_err(|error| format!("Failed to lock service state: {error}"))?;
        (
            guard.controller_runtime.clone(),
            guard
                .agent_runtime
                .as_ref()
                .map(|runtime| Arc::clone(&runtime.task_runtime)),
        )
    };
    let mut secrets = Vec::new();
    if let Some(task_runtime) = task_runtime {
        for (label, value) in task_runtime.sensitive_values_with_labels()? {
            secrets.push(KnownSecret { label, value });
        }
    }
    if let Some(database) = database {
        for record in list_environment_variables(&database).await? {
            if record.kind == EnvironmentVariableKind::Secret
                && record.value.chars().count() >= ENVIRONMENT_SECRET_REDACTION_MIN_CHARS
            {
                secrets.push(KnownSecret {
                    label: record.name,
                    value: record.value,
                });
            }
        }
    }
    Ok(secrets)
}

/// Paths of every open and archived project.
/// 所有打开与已归档项目的路径。
fn project_roots(state: &SharedServiceBridgeState) -> InboxResult<Vec<PathBuf>> {
    let guard = state
        .lock()
        .map_err(|error| format!("Failed to lock service state: {error}"))?;
    Ok(guard
        .workspace_snapshot
        .as_ref()
        .map(|workspace| {
            workspace
                .task_projects
                .iter()
                .chain(workspace.archived_task_projects.iter())
                .filter(|project| !project.path.trim().is_empty())
                .map(|project| PathBuf::from(&project.path))
                .collect()
        })
        .unwrap_or_default())
}

/// Facts about this installation sent when the user keeps "include environment" checked.
/// 用户保留勾选"附带环境信息"时发送的本安装事实。
fn client_info(locale: &str) -> InboxResult<FeedbackClientInfo> {
    let product = vulcan_product_info::product().map_err(|error| error.to_owned())?;
    Ok(FeedbackClientInfo {
        version: product.version.clone(),
        install_epoch: product.install_epoch,
        channel: String::from(if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }),
        os: String::from(std::env::consts::OS),
        arch: String::from(std::env::consts::ARCH),
        locale: locale.to_owned(),
    })
}

/// Filters and encodes one draft on this machine and keeps the exact bytes for the user's send.
/// 在本机过滤并编码一份草稿，并保留精确字节等待用户发送。
///
/// # Parameters
/// 参数
/// `draft` is the user's draft; `draft.draft_id` of an earlier preview keeps the idempotency key.
/// `draft` 是用户草稿；先前预览的 `draft.draft_id` 用于保持幂等键。
///
/// # Returns
/// 返回值
/// The exact preview, or a catalog message naming what to fix.
/// 精确预览，或指出需要修改之处的目录文案。
pub(crate) async fn service_prepare_feedback(
    state: SharedServiceBridgeState,
    draft: FeedbackDraftInput,
) -> InboxResult<FeedbackPreview> {
    let draft_id = match draft.draft_id.as_deref().map(str::trim) {
        Some(id) if uuid::Uuid::parse_str(id).is_ok() => id.to_ascii_lowercase(),
        Some(_) => {
            return Err(WebuiServiceRpcError::InvalidParams(String::from(
                "draftId is not a UUID",
            )));
        }
        None => uuid::Uuid::new_v4().hyphenated().to_string(),
    };
    let locale = interface_locale(&state);
    let client = draft
        .include_client_info
        .then(|| client_info(&locale))
        .transpose()?;
    let error = draft.error.map(|error| FeedbackErrorAttachment {
        category: error.category,
        subcategory: error.subcategory,
        stage: error.stage,
        http_status: error.http_status,
        provider_id: error.provider_id,
        model_id: error.model_id,
        upstream_code: error.upstream_code,
        request_id: error.request_id,
        retryable: error.retryable,
        message: error.message,
    });
    let related_call = error.as_ref().and_then(|error| error.request_id.clone());
    let data_root = vulcan_runtime_assets::resolve_user_vulcan_code_root();
    let inputs = RedactorInputs {
        secrets: known_secrets(&state).await?,
        workspace_roots: project_roots(&state)?,
        data_root: data_root.clone(),
        home_dir: vulcan_runtime_assets::user_home_dir(),
        user_name: std::env::var("USERNAME")
            .or_else(|_| std::env::var("USER"))
            .ok(),
        locale: locale.clone(),
    };
    let include_log = draft.include_log;
    let feedback = FeedbackDraft {
        client_request_id: draft_id.clone(),
        kind: draft.kind,
        source: draft.source,
        title: draft.title,
        body: draft.body,
        allow_contact: draft.allow_contact,
        client,
        error,
        log_lines: None,
    };
    let prepared = tokio::task::spawn_blocking(move || {
        let mut feedback = feedback;
        if include_log && let Some(root) = &data_root {
            let directory = vulcan_runtime_assets::service_log_directory(root);
            match vulcan_feedback::read_recent_log_text(&directory) {
                Ok(text) => {
                    feedback.log_lines = Some(vulcan_feedback::select_log_lines(
                        &text,
                        related_call.as_deref(),
                    ));
                }
                Err(error) => tracing::warn!("[feedback] service log unreadable: {error}"),
            }
        }
        let redactor = Redactor::new(inputs)?;
        vulcan_feedback::prepare_feedback(feedback, &redactor)
    })
    .await
    .map_err(|error| format!("Feedback preparation stopped: {error}"))?
    .map_err(draft_error)?;
    let payload = serde_json::to_string_pretty(&prepared.request)
        .map_err(|error| format!("Failed to render the feedback preview: {error}"))?;
    let preview = FeedbackPreview {
        draft_id: draft_id.clone(),
        digest: prepared.digest.clone(),
        payload,
        byte_length: prepared.encoded.len(),
        redaction_hits: prepared.request.redaction.hits.clone(),
        log_line_count: prepared
            .request
            .attachments
            .log_excerpt
            .as_deref()
            .map_or(0, |excerpt| excerpt.lines().count()),
        signed_in: signed_in_credential(&state)
            .await
            .map_err(WebuiServiceRpcError::Service)?
            .is_some(),
    };
    let mut drafts = prepared_drafts()
        .lock()
        .map_err(|_| String::from("Prepared feedback drafts are unavailable."))?;
    drafts.retain(|stored| stored.draft_id != draft_id);
    drafts.push_back(StoredDraft { draft_id, prepared });
    while drafts.len() > PREPARED_DRAFTS_KEPT {
        drafts.pop_front();
    }
    Ok(preview)
}

/// Receipt of a sent report, as the interface shows it.
/// 界面显示的已发送报告回执。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FeedbackSubmitted {
    feedback_id: String,
    status: String,
    created_at: String,
}

/// Sends the exact bytes of a prepared draft whose digest the user previewed.
/// 发送用户预览过摘要的已准备草稿的精确字节。
///
/// # Parameters
/// 参数
/// `draft_id` names the prepared draft; `digest` must equal the digest of its preview.
/// `draft_id` 指定已准备草稿；`digest` 必须等于其预览的摘要。
///
/// # Returns
/// 返回值
/// The receipt; the draft is kept on failure so a retry reuses the same idempotency key.
/// 回执；失败时保留草稿，以便重试复用同一幂等键。
pub(crate) async fn service_submit_feedback(
    state: SharedServiceBridgeState,
    draft_id: String,
    digest: String,
) -> InboxResult<FeedbackSubmitted> {
    let encoded = {
        let drafts = prepared_drafts()
            .lock()
            .map_err(|_| String::from("Prepared feedback drafts are unavailable."))?;
        drafts
            .iter()
            .find(|stored| stored.draft_id == draft_id && stored.prepared.digest == digest)
            .map(|stored| stored.prepared.encoded.clone())
            .ok_or_else(|| {
                WebuiServiceRpcError::from(WebuiLocalizedMessage::new(
                    "serviceError.feedback.previewChanged",
                ))
            })?
    };
    let secret = required_credential(&state).await?;
    let receipt =
        vulcan_provider_adapters::create_openvulcan_feedback(&inbox_http()?, &secret, encoded)
            .await
            .map_err(inbox_error)?;
    if let Ok(mut drafts) = prepared_drafts().lock() {
        drafts.retain(|stored| stored.draft_id != draft_id);
    }
    Ok(FeedbackSubmitted {
        feedback_id: receipt.id,
        status: receipt.status,
        created_at: receipt.created_at,
    })
}

/// Lists the signed-in user's feedback, newest first.
/// 列出已登录用户的反馈，按从新到旧排列。
pub(crate) async fn service_list_feedback(
    state: SharedServiceBridgeState,
    cursor: Option<String>,
) -> InboxResult<OpenVulcanFeedbackPage> {
    let secret = required_credential(&state).await?;
    vulcan_provider_adapters::list_openvulcan_feedback(&inbox_http()?, &secret, cursor.as_deref())
        .await
        .map_err(inbox_error)
}

/// Reads one of the signed-in user's feedback items with its public replies and links.
/// 读取已登录用户的一条反馈及其公开回复与链接。
pub(crate) async fn service_feedback_detail(
    state: SharedServiceBridgeState,
    feedback_id: String,
) -> InboxResult<OpenVulcanFeedbackDetail> {
    let secret = required_credential(&state).await?;
    vulcan_provider_adapters::read_openvulcan_feedback(&inbox_http()?, &secret, &feedback_id)
        .await
        .map_err(inbox_error)
}

/// Links a Vulcan Code pull request to one of the signed-in user's feedback items.
/// 将 Vulcan Code 拉取请求关联到已登录用户的一条反馈。
pub(crate) async fn service_link_feedback_pr(
    state: SharedServiceBridgeState,
    feedback_id: String,
    url: String,
) -> InboxResult<OpenVulcanFeedbackLinks> {
    let url = url.trim().to_owned();
    if !vulcan_provider_adapters::is_openvulcan_feedback_pr_url(&url) {
        return Err(WebuiLocalizedMessage::new("serviceError.inbox.invalidLink").into());
    }
    let secret = required_credential(&state).await?;
    vulcan_provider_adapters::link_openvulcan_feedback_pr(
        &inbox_http()?,
        &secret,
        &feedback_id,
        &url,
    )
    .await
    .map_err(inbox_error)
}

/// Permanently deletes one of the signed-in user's feedback items.
/// 永久删除已登录用户的一条反馈。
pub(crate) async fn service_delete_feedback(
    state: SharedServiceBridgeState,
    feedback_id: String,
) -> InboxResult<()> {
    let secret = required_credential(&state).await?;
    vulcan_provider_adapters::delete_openvulcan_feedback(&inbox_http()?, &secret, &feedback_id)
        .await
        .map_err(inbox_error)
}

/// Message-center settings.
/// 消息中心设置。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct MessageCenterSettings {
    /// Whether announcements are fetched and shown.
    /// 是否获取并显示公告。
    receive_announcements: bool,
}

/// Reads the message-center settings, applying defaults only in memory.
/// 读取消息中心设置，默认值只在内存中补齐。
pub(crate) fn service_message_center_settings(
    state: SharedServiceBridgeState,
) -> InboxResult<MessageCenterSettings> {
    Ok(MessageCenterSettings {
        receive_announcements: provider_configuration_from_state(&state)?.receives_announcements(),
    })
}

/// Saves the message-center settings the user chose; the default value is left out of the file.
/// 保存用户选择的消息中心设置；默认值不写入文件。
pub(crate) async fn service_save_message_center_settings(
    state: SharedServiceBridgeState,
    settings: MessageCenterSettings,
) -> InboxResult<MessageCenterSettings> {
    mutate_provider_configuration(
        &state,
        "Failed to save the message center settings",
        |configuration| {
            configuration.announcements_enabled =
                (!settings.receive_announcements).then_some(false);
            Ok(())
        },
        |_, _| (),
    )
    .await?;
    if !settings.receive_announcements
        && let Ok(mut cache) = announcement_cache().lock()
    {
        *cache = None;
    }
    Ok(settings)
}

/// Last announcement document fetched in this process.
/// 本进程最近获取的公告文档。
struct CachedAnnouncements {
    region: OpenVulcanRegion,
    etag: Option<String>,
    document: OpenVulcanAnnouncementDocument,
}

/// Announcement cache of this process.
/// 本进程的公告缓存。
fn announcement_cache() -> &'static Mutex<Option<CachedAnnouncements>> {
    static CACHE: OnceLock<Mutex<Option<CachedAnnouncements>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

/// Stable storage name of a region in the local announcement marks.
/// 本地公告标记中区域的稳定存储名。
fn region_key(region: OpenVulcanRegion) -> &'static str {
    match region {
        OpenVulcanRegion::China => "china",
        OpenVulcanRegion::Global => "global",
    }
}

/// Region whose announcements apply: the signed-in account's, otherwise China for a Simplified
/// Chinese interface and the international deployment for every other language.
/// 适用公告的区域：已登录账户所属区域；未登录时简体中文界面取中国区，其他语言取国际区。
async fn announcement_region(state: &SharedServiceBridgeState) -> InboxResult<OpenVulcanRegion> {
    if let Some(region) = signed_in_credential(state)
        .await
        .map_err(WebuiServiceRpcError::Service)?
        .and_then(|(_, secret)| secret.openvulcan_region())
    {
        return Ok(region);
    }
    Ok(if interface_locale(state) == "zh-CN" {
        OpenVulcanRegion::China
    } else {
        OpenVulcanRegion::Global
    })
}

/// Tells whether `now` lies in an announcement's window. The user center publishes only current
/// announcements, but the offline copy may outlive their end; a time that does not parse excludes it.
/// 判断 `now` 是否位于公告的时间窗口内。用户中心只发布当前公告，但离线副本可能超过其结束时间；
/// 无法解析的时间会排除该公告。
fn within_window(starts_at: &str, ends_at: Option<&str>, now: time::OffsetDateTime) -> bool {
    let parse = |value: &str| {
        time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339).ok()
    };
    parse(starts_at).is_some_and(|start| start <= now)
        && ends_at.is_none_or(|end| parse(end).is_some_and(|end| now < end))
}

/// Tells whether an announcement's targeting includes this installation; a bound that is not an
/// exact `x.y.z` version excludes it rather than guessing.
/// 判断公告的定向条件是否包含本安装；不是精确 `x.y.z` 版本的边界会排除该公告，而不是猜测。
fn targets_this_installation(
    target: &vulcan_provider_adapters::OpenVulcanAnnouncementTarget,
) -> bool {
    let above_min = target
        .min_version
        .as_deref()
        .is_none_or(|minimum| vulcan_product_info::version_at_least(minimum).unwrap_or(false));
    let below_max = target
        .max_version
        .as_deref()
        .is_none_or(|maximum| vulcan_product_info::version_at_most(maximum).unwrap_or(false));
    let system = target.os.is_empty()
        || target
            .os
            .iter()
            .any(|os| os.eq_ignore_ascii_case(std::env::consts::OS));
    above_min && below_max && system
}

/// One announcement as the message center shows it, in the interface language.
/// 消息中心以界面语言显示的一条公告。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AnnouncementView {
    id: String,
    kind: String,
    priority: String,
    starts_at: String,
    ends_at: Option<String>,
    /// Locale of the content shown.
    /// 所显示内容的语言。
    locale: String,
    title: String,
    /// Markdown body.
    /// Markdown 正文。
    body: String,
    link: Option<OpenVulcanAnnouncementLink>,
    seen: bool,
    dismissed: bool,
}

/// Announcements that apply to this installation.
/// 适用于本安装的公告。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AnnouncementsView {
    /// Whether the user receives announcements at all.
    /// 用户是否接收公告。
    enabled: bool,
    /// Region the announcements come from.
    /// 公告所属区域。
    region: Option<OpenVulcanRegion>,
    /// Whether the user center could not be reached and the list is the last one fetched.
    /// 是否无法连接用户中心、列表为最近一次获取的结果。
    offline: bool,
    announcements: Vec<AnnouncementView>,
    /// Announcements not yet seen in the message center.
    /// 尚未在消息中心看过的公告数。
    unseen: usize,
}

/// Fetches the region's announcements (conditionally, by ETag), keeps those that target this
/// installation, picks the interface language and adds the local seen and dismissed marks.
/// 获取该区域的公告（按 ETag 条件获取），保留定向到本安装的公告，选择界面语言，并附上本地的已看与已关闭标记。
pub(crate) async fn service_list_announcements(
    state: SharedServiceBridgeState,
) -> InboxResult<AnnouncementsView> {
    if !provider_configuration_from_state(&state)?.receives_announcements() {
        return Ok(AnnouncementsView {
            enabled: false,
            region: None,
            offline: false,
            announcements: Vec::new(),
            unseen: 0,
        });
    }
    let region = announcement_region(&state).await?;
    let cached_etag = announcement_cache().lock().ok().and_then(|cache| {
        cache
            .as_ref()
            .filter(|cached| cached.region == region)
            .and_then(|cached| cached.etag.clone())
    });
    let fetched = vulcan_provider_adapters::request_openvulcan_announcements(
        &inbox_http()?,
        region,
        cached_etag.as_deref(),
    )
    .await;
    let (document, offline, refreshed) = {
        let mut cache = announcement_cache()
            .lock()
            .map_err(|_| String::from("The announcement cache is unavailable."))?;
        let cached = cache.as_ref().filter(|cached| cached.region == region);
        match (fetched, cached) {
            (Ok(OpenVulcanAnnouncementsFetch::Current { etag, document }), _) => {
                *cache = Some(CachedAnnouncements {
                    region,
                    etag,
                    document: document.clone(),
                });
                (document, false, true)
            }
            (Ok(OpenVulcanAnnouncementsFetch::NotModified), Some(cached)) => {
                (cached.document.clone(), false, false)
            }
            (Ok(OpenVulcanAnnouncementsFetch::NotModified), None) => {
                return Err(WebuiLocalizedMessage::new("serviceError.inbox.invalidAnswer").into());
            }
            (Err(OpenVulcanInboxError::Transport), Some(cached)) => {
                (cached.document.clone(), true, false)
            }
            (Err(error), _) => return Err(inbox_error(error)),
        }
    };
    let database = {
        let guard = state
            .lock()
            .map_err(|error| format!("Failed to lock service state: {error}"))?;
        guard
            .controller_runtime
            .clone()
            .ok_or_else(|| String::from("The service is still starting; storage is not ready."))?
    };
    if refreshed {
        let current: Vec<String> = document
            .announcements
            .iter()
            .map(|announcement| announcement.id.clone())
            .collect();
        prune_announcement_states(&database, region_key(region), &current).await?;
    }
    let marks = load_announcement_states(&database, region_key(region)).await?;
    let locale = interface_locale(&state);
    let now = time::OffsetDateTime::now_utc();
    let announcements: Vec<AnnouncementView> = document
        .announcements
        .into_iter()
        .filter(|announcement| {
            targets_this_installation(&announcement.target)
                && within_window(
                    &announcement.starts_at,
                    announcement.ends_at.as_deref(),
                    now,
                )
        })
        .filter_map(|announcement| {
            let (content_locale, content) = announcement
                .content
                .get_key_value(locale.as_str())
                .or_else(|| {
                    announcement
                        .content
                        .get_key_value(vulcan_i18n::DEFAULT_LOCALE)
                })
                .or_else(|| announcement.content.iter().next())
                .map(|(key, value)| (key.clone(), value.clone()))?;
            let mark = marks.get(&announcement.id).copied().unwrap_or_default();
            Some(AnnouncementView {
                id: announcement.id,
                kind: announcement.kind,
                priority: announcement.priority,
                starts_at: announcement.starts_at,
                ends_at: announcement.ends_at,
                locale: content_locale,
                title: content.title,
                body: content.body,
                link: announcement.link,
                seen: mark.seen_at_ms.is_some(),
                dismissed: mark.dismissed_at_ms.is_some(),
            })
        })
        .collect();
    let unseen = announcements
        .iter()
        .filter(|announcement| !announcement.seen)
        .count();
    Ok(AnnouncementsView {
        enabled: true,
        region: Some(region),
        offline,
        announcements,
        unseen,
    })
}

/// Records announcements as seen or their banners as closed in the current region.
/// 在当前区域中把公告记为已看，或把其横幅记为已关闭。
pub(crate) async fn service_mark_announcements(
    state: SharedServiceBridgeState,
    announcement_ids: Vec<String>,
    dismissed: bool,
) -> InboxResult<()> {
    if announcement_ids.is_empty()
        || announcement_ids.len() > vulcan_provider_adapters::OPENVULCAN_MARK_READ_MAX_IDS
        || announcement_ids.iter().any(|id| {
            id.strip_prefix("an_").is_none_or(|rest| {
                rest.len() != 26
                    || !rest
                        .bytes()
                        .all(|byte| byte.is_ascii_lowercase() || (b'2'..=b'7').contains(&byte))
            })
        })
    {
        return Err(WebuiServiceRpcError::InvalidParams(String::from(
            "announcementIds must hold 1 to 100 announcement ids",
        )));
    }
    let region = announcement_region(&state).await?;
    let database = {
        let guard = state
            .lock()
            .map_err(|error| format!("Failed to lock service state: {error}"))?;
        guard
            .controller_runtime
            .clone()
            .ok_or_else(|| String::from("The service is still starting; storage is not ready."))?
    };
    let mark = if dismissed {
        AnnouncementMark::Dismissed
    } else {
        AnnouncementMark::Seen
    };
    mark_announcements(
        &database,
        region_key(region),
        &announcement_ids,
        mark,
        u64::try_from(crate::current_unix_millis_value()).unwrap_or(0),
    )
    .await
    .map_err(WebuiServiceRpcError::Service)
}

/// Unread personal messages of the signed-in user.
/// 已登录用户的未读个人消息。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MessageSummaryView {
    /// Whether an OpenVulcan account is signed in.
    /// 是否已登录 OpenVulcan 账户。
    signed_in: bool,
    /// Whether the deployment serves personal messages.
    /// 部署是否提供个人消息。
    available: bool,
    unread: u64,
    latest_id: Option<String>,
}

/// Reads the unread summary; a signed-out user simply has no personal messages.
/// 读取未读摘要；未登录用户没有个人消息。
pub(crate) async fn service_message_summary(
    state: SharedServiceBridgeState,
) -> InboxResult<MessageSummaryView> {
    let Some((_, secret)) = signed_in_credential(&state)
        .await
        .map_err(WebuiServiceRpcError::Service)?
    else {
        return Ok(MessageSummaryView {
            signed_in: false,
            available: false,
            unread: 0,
            latest_id: None,
        });
    };
    let summary =
        vulcan_provider_adapters::request_openvulcan_message_summary(&inbox_http()?, &secret)
            .await
            .map_err(inbox_error)?;
    Ok(MessageSummaryView {
        signed_in: true,
        available: summary.is_some(),
        unread: summary.as_ref().map_or(0, |summary| summary.unread),
        latest_id: summary.and_then(|summary| summary.latest_id),
    })
}

/// Lists the signed-in user's personal messages, newest first.
/// 列出已登录用户的个人消息，按从新到旧排列。
pub(crate) async fn service_list_messages(
    state: SharedServiceBridgeState,
    cursor: Option<String>,
    unread_only: bool,
) -> InboxResult<OpenVulcanMessagePage> {
    let secret = required_credential(&state).await?;
    vulcan_provider_adapters::list_openvulcan_messages(
        &inbox_http()?,
        &secret,
        cursor.as_deref(),
        unread_only,
    )
    .await
    .map_err(inbox_error)
}

/// Marks personal messages read: exactly one of `ids` or `all_before`.
/// 将个人消息标记为已读：`ids` 与 `all_before` 二者恰好其一。
pub(crate) async fn service_mark_messages_read(
    state: SharedServiceBridgeState,
    ids: Option<Vec<String>>,
    all_before: Option<String>,
) -> InboxResult<OpenVulcanMarkReadResult> {
    let selection = match (ids, all_before) {
        (Some(ids), None) => OpenVulcanMarkRead::Ids(ids),
        (None, Some(id)) => OpenVulcanMarkRead::AllBefore(id),
        _ => {
            return Err(WebuiServiceRpcError::InvalidParams(String::from(
                "pass exactly one of ids or allBefore",
            )));
        }
    };
    let secret = required_credential(&state).await?;
    vulcan_provider_adapters::mark_openvulcan_messages_read(&inbox_http()?, &secret, &selection)
        .await
        .map_err(inbox_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn announcements_show_only_inside_their_window() {
        let now = time::OffsetDateTime::parse(
            "2026-11-07T12:00:00Z",
            &time::format_description::well_known::Rfc3339,
        )
        .expect("valid time");
        assert!(within_window("2026-11-07T00:00:00Z", None, now));
        assert!(within_window(
            "2026-11-07T00:00:00Z",
            Some("2026-11-08T00:00:00+08:00"),
            now
        ));
        assert!(!within_window("2026-11-08T00:00:00Z", None, now));
        assert!(!within_window(
            "2026-11-01T00:00:00Z",
            Some("2026-11-07T12:00:00Z"),
            now
        ));
        assert!(!within_window("tomorrow", None, now));
        assert!(!within_window("2026-11-01T00:00:00Z", Some("never"), now));
    }

    #[test]
    fn targeting_requires_every_condition() {
        let current = vulcan_product_info::version().to_owned();
        let target = |min: Option<&str>, max: Option<&str>, os: &[&str]| {
            vulcan_provider_adapters::OpenVulcanAnnouncementTarget {
                min_version: min.map(str::to_owned),
                max_version: max.map(str::to_owned),
                os: os.iter().map(|value| (*value).to_owned()).collect(),
            }
        };
        assert!(targets_this_installation(&target(None, None, &[])));
        assert!(targets_this_installation(&target(
            Some(&current),
            Some(&current),
            &[std::env::consts::OS]
        )));
        assert!(!targets_this_installation(&target(
            Some("999.0.0"),
            None,
            &[]
        )));
        assert!(!targets_this_installation(&target(
            None,
            Some("0.0.0"),
            &[]
        )));
        assert!(!targets_this_installation(&target(None, None, &["plan9"])));
        assert!(!targets_this_installation(&target(
            Some("latest"),
            None,
            &[]
        )));
    }

    #[test]
    fn user_center_failures_become_catalog_messages() {
        let rate_limited = inbox_error(OpenVulcanInboxError::Rejected {
            status: 429,
            code: String::from("rate_limited"),
            retry_after_secs: Some(61),
        });
        let message = rate_limited.localized().expect("catalog message");
        assert_eq!(message.key, "serviceError.inbox.rateLimited");
        assert_eq!(message.params["minutes"], "2");
        let expired = inbox_error(OpenVulcanInboxError::Rejected {
            status: 401,
            code: String::from("unauthorized"),
            retry_after_secs: None,
        });
        assert_eq!(
            expired.localized().expect("catalog message").key,
            "serviceError.inbox.signInExpired"
        );
        assert_eq!(
            inbox_error(OpenVulcanInboxError::InvalidArgument("x")).code(),
            "invalid_params"
        );
    }
}
