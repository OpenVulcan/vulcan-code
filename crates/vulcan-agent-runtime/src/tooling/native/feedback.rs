//! `feedback`: drafts a problem report or suggestion to the OpenVulcan team for the user to review,
//! and links a pull request to one of the user's feedback items. Drafting never sends anything: the
//! user reviews the filtered preview in the feedback window and sends it themself. Linking writes to
//! the OpenVulcan user center through the same service the feedback list uses.
//! `feedback`：起草发给 OpenVulcan 团队的问题报告或建议供用户检查，并把拉取请求关联到用户的一条反馈。
//! 起草从不发送任何内容：用户在反馈窗口中检查过滤后的预览并亲自发送。关联经反馈列表使用的同一服务写入
//! OpenVulcan 用户中心。

use super::*;
use vulcan_feedback::FeedbackKind;
use vulcan_feedback::limits::{BODY_MAX_CHARS, TITLE_MAX_CHARS};

/// Stable identifier of the feedback tool.
/// 反馈工具的稳定标识。
pub const FEEDBACK_TOOL_ID: &str = "feedback";

/// The `feedback` action that only drafts; it has no side effects.
/// `feedback` 中只起草的动作；没有副作用。
pub const FEEDBACK_DRAFT_ACTION: &str = "draft";

/// The `feedback` action that links a pull request on the OpenVulcan user center.
/// `feedback` 中在 OpenVulcan 用户中心关联拉取请求的动作。
pub const FEEDBACK_LINK_PR_ACTION: &str = "link_pr";

/// Status of a draft result: shown to the user, nothing sent.
/// 草稿结果的状态：已展示给用户，未发送任何内容。
pub const FEEDBACK_DRAFT_STATUS: &str = "awaiting_user";

/// Model-visible explanation attached to every draft result.
/// 附在每个草稿结果上的模型可见说明。
const FEEDBACK_DRAFT_MESSAGE: &str = "Shown to the user as a draft. Nothing has been sent; the user reviews the filtered preview and sends it.";

/// One parsed and validated `feedback` call.
/// 一次已解析并校验的 `feedback` 调用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeedbackRequest {
    /// Draft a report for the user to review; title and body are trimmed.
    /// 起草一份供用户检查的报告；标题与正文已去除首尾空白。
    Draft {
        /// Problem or suggestion.
        /// 问题或建议。
        kind: FeedbackKind,
        /// One-line title.
        /// 单行标题。
        title: String,
        /// Markdown description.
        /// Markdown 描述。
        body: String,
    },
    /// Link a pull request to one of the user's feedback items; both values are trimmed.
    /// 将拉取请求关联到用户的一条反馈；两个值均已去除首尾空白。
    LinkPr {
        /// Public id of the feedback item.
        /// 反馈条目的公开编号。
        feedback_id: String,
        /// Pull request address.
        /// 拉取请求地址。
        url: String,
    },
}

/// Wire shape of the arguments; unknown fields and actions are refused.
/// 参数的传输形状；拒绝未知字段与动作。
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum FeedbackArguments {
    /// `draft` arguments.
    /// `draft` 参数。
    Draft {
        kind: FeedbackKind,
        title: String,
        body: String,
    },
    /// `link_pr` arguments.
    /// `link_pr` 参数。
    LinkPr { feedback_id: String, url: String },
}

impl FeedbackRequest {
    /// Parses and validates the arguments of one `feedback` call.
    /// 解析并校验一次 `feedback` 调用的参数。
    pub fn parse(args: &Value) -> Result<Self, String> {
        let arguments = serde_json::from_value::<FeedbackArguments>(args.clone())
            .map_err(|error| format!("Invalid `{FEEDBACK_TOOL_ID}` arguments: {error}"))?;
        match arguments {
            FeedbackArguments::Draft { kind, title, body } => {
                let title = title.trim();
                if title.is_empty() {
                    return Err(String::from("`title` must not be empty."));
                }
                if title.contains(['\n', '\r']) {
                    return Err(String::from("`title` must be a single line."));
                }
                if title.chars().count() > TITLE_MAX_CHARS {
                    return Err(format!(
                        "`title` must be at most {TITLE_MAX_CHARS} characters."
                    ));
                }
                let body = body.trim();
                if body.is_empty() {
                    return Err(String::from("`body` must not be empty."));
                }
                if body.chars().count() > BODY_MAX_CHARS {
                    return Err(format!(
                        "`body` must be at most {BODY_MAX_CHARS} characters."
                    ));
                }
                Ok(Self::Draft {
                    kind,
                    title: title.to_owned(),
                    body: body.to_owned(),
                })
            }
            FeedbackArguments::LinkPr { feedback_id, url } => {
                let feedback_id = feedback_id.trim();
                if feedback_id.is_empty() {
                    return Err(String::from("`feedback_id` must not be empty."));
                }
                let url = url.trim();
                if url.is_empty() {
                    return Err(String::from("`url` must not be empty."));
                }
                Ok(Self::LinkPr {
                    feedback_id: feedback_id.to_owned(),
                    url: url.to_owned(),
                })
            }
        }
    }
}

/// Host-injected feedback port. The host owns the OpenVulcan account and the user-center client, so
/// the tool and the feedback list link pull requests through the same service function.
/// 宿主注入的反馈端口。宿主拥有 OpenVulcan 账户与用户中心客户端，因此工具与反馈列表经同一服务函数关联
/// 拉取请求。
#[async_trait::async_trait]
pub trait NativeFeedbackHost: fmt::Debug + Send + Sync {
    /// Links `url` to the feedback item `feedback_id` for `session_id`, after the permission gate
    /// admitted the call; returns the item's links as the user center reports them. The host still
    /// refuses spawned agents.
    /// 在权限闸门准入后为 `session_id` 将 `url` 关联到反馈条目 `feedback_id`；返回用户中心报告的该条目
    /// 全部链接。宿主仍拒绝派生代理。
    async fn link_pr(
        &self,
        session_id: SessionId,
        feedback_id: String,
        url: String,
    ) -> Result<Value, String>;
}

/// Builds the model-visible `feedback` tool; only `draft` runs in Plan mode.
/// 构建模型可见的 `feedback` 工具；计划模式中只有 `draft` 可以运行。
pub(crate) fn build_feedback_tool() -> Arc<NativeToolDefinition> {
    build_native_tool_definition(
        NativeToolDescriptor {
            id: FEEDBACK_TOOL_ID.to_string(),
            name: FEEDBACK_TOOL_ID.to_string(),
            description: format!(
                "Help the user report a problem or suggestion about Vulcan Code itself to the OpenVulcan team. You can never send feedback: you only draft it.\n\
- `{FEEDBACK_DRAFT_ACTION}`: use it when the user wants to report a bug in, or suggest a change to, Vulcan Code itself (not their own project). Give `kind` (`bug` or `suggestion`), a one-line `title` (at most {TITLE_MAX_CHARS} characters) and a Markdown `body` (at most {BODY_MAX_CHARS} characters) describing what happened, what was expected and how to reproduce it. Drafting never sends anything and has no side effects: the draft is shown to the user, who reviews a filtered preview (secrets, paths and user names removed on this machine) and sends it themself.\n\
- `{FEEDBACK_LINK_PR_ACTION}`: link a pull request the user opened to one of their feedback items. Give the item's `feedback_id` and the pull request `url`, which must start with `{}`. This writes to the OpenVulcan server, so the user approves it unless the session allows it.",
                vulcan_provider_adapters::OPENVULCAN_FEEDBACK_PR_URL_PREFIX,
            ),
            help_text: None,
            ai_visible: true,
            available_in_plan: true,
        },
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": [FEEDBACK_DRAFT_ACTION, FEEDBACK_LINK_PR_ACTION],
                    "description": "`draft` prepares a report for the user to review and send; `link_pr` links a pull request to an existing feedback item."
                },
                "kind": {
                    "type": "string",
                    "enum": ["bug", "suggestion"],
                    "description": "`draft` only: `bug` for a problem, `suggestion` for an idea or request."
                },
                "title": {
                    "type": "string",
                    "description": "`draft` only: one-line title."
                },
                "body": {
                    "type": "string",
                    "description": "`draft` only: Markdown description."
                },
                "feedback_id": {
                    "type": "string",
                    "description": "`link_pr` only: id of the user's feedback item."
                },
                "url": {
                    "type": "string",
                    "description": "`link_pr` only: pull request address."
                }
            },
            "required": ["action"],
            "additionalProperties": false
        }),
        |request| Box::pin(execute_feedback_tool(request)),
    )
}

/// Builds the result of one draft: the same JSON for the model and for the review card.
/// 构建一次草稿的结果：模型与检查卡读取同一份 JSON。
fn draft_result(kind: FeedbackKind, title: &str, body: &str) -> Value {
    json!({
        "status": FEEDBACK_DRAFT_STATUS,
        "draft": { "kind": kind, "title": title, "body": body },
        "message": FEEDBACK_DRAFT_MESSAGE,
    })
}

/// Executes one `feedback` call; only `link_pr` reaches the host.
/// 执行一次 `feedback` 调用；只有 `link_pr` 会访问宿主。
async fn execute_feedback_tool(
    request: NativeToolExecutionRequest,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let payload = match FeedbackRequest::parse(&request.arguments).map_err(NativeToolError::new)? {
        FeedbackRequest::Draft { kind, title, body } => draft_result(kind, &title, &body),
        FeedbackRequest::LinkPr { feedback_id, url } => {
            let host = request
                .feedback
                .clone()
                .ok_or_else(|| NativeToolError::new("Feedback is not available in this run."))?;
            let session_id = request.session_id.ok_or_else(|| {
                NativeToolError::new(format!("`{FEEDBACK_TOOL_ID}` needs a session."))
            })?;
            let links = host
                .link_pr(session_id, feedback_id.clone(), url)
                .await
                .map_err(NativeToolError::new)?;
            json!({ "status": "linked", "feedback_id": feedback_id, "links": links })
        }
    };
    let text = serde_json::to_string_pretty(&payload)
        .map_err(|error| NativeToolError::new(error.to_string()))?;
    Ok(
        VulcanToolInvocationResult::from_model_output(Value::String(text)).with_host_result(
            VulcanToolHostResult {
                kind: FEEDBACK_TOOL_ID.to_string(),
                payload,
            },
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drafts parse strictly, are trimmed, and are bounded by the feedback limits.
    /// 草稿严格解析、去除首尾空白，并受反馈上限约束。
    #[test]
    fn drafts_parse_strictly_within_the_feedback_limits() {
        assert_eq!(
            FeedbackRequest::parse(&json!({
                "action": "draft", "kind": "bug", "title": "  Crash  ", "body": " Steps "
            })),
            Ok(FeedbackRequest::Draft {
                kind: FeedbackKind::Bug,
                title: String::from("Crash"),
                body: String::from("Steps"),
            })
        );
        let title_at_limit = "t".repeat(TITLE_MAX_CHARS);
        let body_at_limit = "b".repeat(BODY_MAX_CHARS);
        assert!(
            FeedbackRequest::parse(&json!({
                "action": "draft", "kind": "suggestion", "title": title_at_limit, "body": body_at_limit
            }))
            .is_ok()
        );
        for args in [
            json!({"action": "draft", "kind": "bug", "title": " ", "body": "x"}),
            json!({"action": "draft", "kind": "bug", "title": "x", "body": "\n"}),
            json!({"action": "draft", "kind": "bug", "title": "a\nb", "body": "x"}),
            json!({"action": "draft", "kind": "bug", "title": "t".repeat(TITLE_MAX_CHARS + 1), "body": "x"}),
            json!({"action": "draft", "kind": "bug", "title": "x", "body": "b".repeat(BODY_MAX_CHARS + 1)}),
            json!({"action": "draft", "kind": "question", "title": "x", "body": "x"}),
            json!({"action": "draft", "kind": "bug", "title": "x"}),
            json!({"action": "draft", "kind": "bug", "title": "x", "body": "x", "send": true}),
            json!({"action": "send", "kind": "bug", "title": "x", "body": "x"}),
            json!({}),
        ] {
            assert!(
                FeedbackRequest::parse(&args).is_err(),
                "{args} must be refused"
            );
        }
    }

    /// Links need a feedback id and an address; the address rule belongs to the host service.
    /// 关联需要反馈编号与地址；地址规则属于宿主服务。
    #[test]
    fn links_need_an_id_and_an_address() {
        assert_eq!(
            FeedbackRequest::parse(&json!({
                "action": "link_pr", "feedback_id": " fb_1 ", "url": " https://example.test/pull/1 "
            })),
            Ok(FeedbackRequest::LinkPr {
                feedback_id: String::from("fb_1"),
                url: String::from("https://example.test/pull/1"),
            })
        );
        for args in [
            json!({"action": "link_pr", "feedback_id": "", "url": "u"}),
            json!({"action": "link_pr", "feedback_id": "fb_1", "url": " "}),
            json!({"action": "link_pr", "feedback_id": "fb_1"}),
            json!({"action": "link_pr", "feedback_id": "fb_1", "url": "u", "title": "x"}),
        ] {
            assert!(
                FeedbackRequest::parse(&args).is_err(),
                "{args} must be refused"
            );
        }
    }

    /// A draft runs without any host, and its result tells the model nothing was sent.
    /// 草稿无需任何宿主即可运行，其结果告诉模型未发送任何内容。
    #[tokio::test]
    async fn draft_needs_no_host_and_reports_awaiting_user() {
        let result = execute_feedback_tool(super::super::tests::test_native_request(json!({
            "action": "draft", "kind": "suggestion", "title": "Idea", "body": "More themes"
        })))
        .await
        .expect("a draft needs no host");
        let host = result.host_result.expect("drafts carry a host result");
        assert_eq!(host.kind, FEEDBACK_TOOL_ID);
        assert_eq!(host.payload["status"], FEEDBACK_DRAFT_STATUS);
        assert_eq!(host.payload["draft"]["kind"], "suggestion");
        assert_eq!(host.payload["draft"]["title"], "Idea");
        assert_eq!(host.payload["draft"]["body"], "More themes");
        assert!(host.payload["message"].is_string());
    }

    /// Linking without a host fails explicitly instead of pretending to succeed.
    /// 没有宿主时关联明确失败，而不是假装成功。
    #[tokio::test]
    async fn link_without_a_host_fails() {
        let error = execute_feedback_tool(super::super::tests::test_native_request(json!({
            "action": "link_pr", "feedback_id": "fb_1", "url": "https://example.test/pull/1"
        })))
        .await
        .expect_err("linking needs the host");
        assert!(!error.message.is_empty());
    }
}
