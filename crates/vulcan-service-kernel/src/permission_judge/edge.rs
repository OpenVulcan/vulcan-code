//! Cloud judge: the signed-in OpenVulcan account obtains the endpoints, site marker and a
//! short-lived token per model, and each escalated call goes straight to the selected endpoint.
//! 云端判定：已登录的 OpenVulcan 账户按模型获取端点、来源标记与短期令牌，每次升级调用直接发往
//! 选中的端点。
//!
//! Tokens are cached per model and account and renewed shortly before they expire; an endpoint
//! that rejects a token gets one fresh token and one retry. Every other failure goes to the next
//! layer of the pipeline and never allows the call; a request never moves to another endpoint.
//! 令牌按模型与账户缓存，并在到期前不久续期；端点拒绝令牌时换一次新令牌并重试一次。其他任何失败
//! 都交给管线的下一层，绝不放行调用；请求不会改投其他端点。

use std::collections::BTreeMap;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;
use vulcan_permission_judge::protocol::JudgeInput;
use vulcan_permission_judge::{
    EDGE_JUDGE_PATH, EDGE_SERVER_TIMING_HEADER, EDGE_SITE_HEADER, EdgeAnswer, EdgeJudgeRequest,
    EdgeJudgeResponse, EdgeOutcome, EdgeServerTiming, edge_outcomes, hosted_policy, render_call,
};
use vulcan_provider_adapters::{
    CredentialExecutionSecret, OpenVulcanEdgeAccessError, OpenVulcanEdgeEndpoint,
    request_openvulcan_edge_access, request_openvulcan_edge_models,
};
use vulcan_service_api::{
    PermissionJudgeEdgeRequestAttempt, PermissionJudgeEdgeRequestTest,
    PermissionJudgeEdgeTestAnswer, PermissionJudgeEdgeTestInput, PermissionJudgeEdgeTestPreset,
    PermissionJudgeEdgeTestResult,
};

use super::PermissionJudgeRuntime;
use super::usage::{JudgeRoute, JudgeUsage, JudgeUsageOutcome, record_judge_usage};
use crate::public_ai_settings::openvulcan_account::signed_in_credential;
use crate::{SharedServiceBridgeState, current_v1_unix_millis};

/// A cached token is renewed once less than this much of its lifetime remains.
/// 缓存令牌剩余有效期少于该值时续期。
const ACCESS_REFRESH_AHEAD_MS: u64 = 2 * 60 * 1_000;

/// Longest wait for one edge judgement request; the call then goes to the next layer.
/// 一次边缘判定请求的最长等待时间；超时后调用交给下一层。
const EDGE_JUDGE_TIMEOUT: Duration = Duration::from_secs(15);

/// Largest edge answer read; ten results take a few kilobytes.
/// 读取的最大边缘回答；十条结果只有数 KB。
const EDGE_MAX_RESPONSE_BYTES: usize = 256 * 1024;

/// Site marker, endpoints and token for one model. It has no `Debug` so the token cannot reach a
/// log.
/// 单个模型的来源标记、端点与令牌。不实现 `Debug`，使令牌无法进入日志。
#[derive(Clone)]
pub(super) struct Grant {
    /// Deployment id sent with every judge request.
    /// 随每个判定请求发送的部署 id。
    pub(super) site: String,
    /// Endpoints of the model in the deployment's priority order; never empty.
    /// 按部署优先顺序排列的该模型端点；从不为空。
    pub(super) endpoints: Vec<OpenVulcanEdgeEndpoint>,
    /// Bearer token every endpoint of the model accepts.
    /// 该模型每个端点都接受的 Bearer 令牌。
    pub(super) token: String,
}

/// Grant issued to one account.
/// 签发给某个账户的授权。
struct CachedAccess {
    /// Account connection the token was issued for.
    /// 令牌签发所针对的账户连接。
    connection_id: String,
    /// Site marker, endpoints and token.
    /// 来源标记、端点与令牌。
    grant: Grant,
    /// Token expiry in Unix milliseconds.
    /// 令牌到期时间（Unix 毫秒）。
    expires_at_ms: u64,
}

/// Edge access per edge model id, shared by every judgement of the process.
/// 按边缘模型 id 保存的边缘访问，由进程内所有判定共享。
#[derive(Default)]
pub(super) struct EdgeAccessCache {
    /// Cached access by edge model id; the lock also keeps concurrent calls to one renewal.
    /// 按边缘模型 id 缓存的访问；该锁同时使并发调用只发起一次续期。
    entries: Mutex<BTreeMap<String, CachedAccess>>,
}

/// Endpoint one judge request goes to.
/// 一次判定请求发往的端点。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Target {
    /// Endpoint id, named in failures.
    /// 端点 id，在失败说明中写明。
    pub(super) endpoint_id: String,
    /// Endpoint base URL.
    /// 端点基础地址。
    pub(super) endpoint: String,
    /// Site marker sent in [`EDGE_SITE_HEADER`].
    /// 在 [`EDGE_SITE_HEADER`] 中发送的来源标记。
    pub(super) site: String,
}

/// What one judge request produced.
/// 一次判定请求的结果。
enum Answer {
    /// The endpoint answered.
    /// 端点已作答。
    Judged(EdgeJudgeResponse),
    /// The endpoint rejected the token.
    /// 端点拒绝了令牌。
    TokenRejected,
}

/// A judge request that failed.
/// 失败的判定请求。
#[derive(Debug)]
struct Failure {
    /// English trace message.
    /// 英文轨迹说明。
    message: String,
    /// The endpoint looks unusable for this account, so endpoint selection should run again.
    /// 端点看起来对该账户不可用，端点选择应重新运行。
    reprobe: bool,
}

impl From<String> for Failure {
    fn from(message: String) -> Self {
        Self {
            message,
            reprobe: false,
        }
    }
}

/// The process HTTP client for edge requests; it never follows redirects, so the token reaches
/// only the endpoint the user center named.
/// 进程内用于边缘请求的 HTTP 客户端；不跟随重定向，使令牌只发往用户中心指定的端点。
pub(super) fn http() -> Result<&'static reqwest::Client, String> {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    if let Some(client) = CLIENT.get() {
        return Ok(client);
    }
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| String::from("Failed to create the edge judge transport."))?;
    Ok(CLIENT.get_or_init(|| client))
}

impl EdgeAccessCache {
    /// Returns the grant for `model_id`, requesting a new one when none is cached for this
    /// account, the cached one is about to expire, or `renew` is set.
    /// 返回 `model_id` 的授权；当前账户没有缓存、缓存即将到期或 `renew` 为真时请求新的。
    pub(super) async fn access(
        &self,
        connection_id: &str,
        secret: &CredentialExecutionSecret,
        model_id: &str,
        renew: bool,
    ) -> Result<Grant, String> {
        let mut entries = self.entries.lock().await;
        let now = u64::try_from(current_v1_unix_millis()?).unwrap_or_default();
        if !renew
            && let Some(cached) = entries.get(model_id)
            && cached.connection_id == connection_id
            && cached.expires_at_ms.saturating_sub(now) > ACCESS_REFRESH_AHEAD_MS
        {
            return Ok(cached.grant.clone());
        }
        entries.remove(model_id);
        let access = request_openvulcan_edge_access(http()?, secret, model_id)
            .await
            .map_err(|error| match error {
                OpenVulcanEdgeAccessError::ModelUnavailable => {
                    format!("OpenVulcan does not currently enable the cloud model `{model_id}`.")
                }
                OpenVulcanEdgeAccessError::Failed(reason) => reason,
            })?;
        let grant = Grant {
            site: access.site,
            endpoints: access.endpoints,
            token: access.access_token,
        };
        entries.insert(
            model_id.to_string(),
            CachedAccess {
                connection_id: connection_id.to_string(),
                grant: grant.clone(),
                expires_at_ms: access.expires_at_unix_ms,
            },
        );
        Ok(grant)
    }
}

/// Trace message for an endpoint that refused the call over the caller's rate limit; the wait it
/// asks for is named when its `Retry-After` header holds whole seconds.
/// 端点因调用方超出速率上限而拒绝调用时的轨迹说明；`Retry-After` 头为整数秒时写明所需等待。
fn rate_limited(retry_after: Option<&str>) -> String {
    match retry_after.and_then(|value| value.trim().parse::<u64>().ok()) {
        Some(seconds) => format!(
            "The cloud judge rate limit for this account and installation was reached; it accepts calls again in {seconds} seconds."
        ),
        None => String::from(
            "The cloud judge rate limit for this account and installation was reached.",
        ),
    }
}

/// What an endpoint sent back to one judge request, before it is interpreted.
/// 端点对一次判定请求返回的内容，尚未解释。
struct Reply {
    /// HTTP status.
    /// HTTP 状态。
    status: reqwest::StatusCode,
    /// `Retry-After` header.
    /// `Retry-After` 头。
    retry_after: Option<String>,
    /// `Server-Timing` header.
    /// `Server-Timing` 头。
    server_timing: Option<String>,
    /// Body, at most [`EDGE_MAX_RESPONSE_BYTES`].
    /// 正文，至多 [`EDGE_MAX_RESPONSE_BYTES`]。
    body: Vec<u8>,
}

/// Sends one judge request to `target` with `token` and the target's site marker and reads the
/// reply.
/// 用 `token` 与目标的来源标记向 `target` 发送一次判定请求并读取回复。
async fn post(target: &Target, token: &str, request: &EdgeJudgeRequest) -> Result<Reply, Failure> {
    let id = &target.endpoint_id;
    let unreachable = |message: String| Failure {
        message,
        reprobe: true,
    };
    let response = http()?
        .post(format!("{}{EDGE_JUDGE_PATH}", target.endpoint))
        .timeout(EDGE_JUDGE_TIMEOUT)
        .bearer_auth(token)
        .header(EDGE_SITE_HEADER, &target.site)
        .json(request)
        .send()
        .await
        .map_err(|error| {
            unreachable(if error.is_timeout() {
                format!("The edge judge endpoint `{id}` did not answer in time.")
            } else {
                format!("The edge judge endpoint `{id}` could not be reached.")
            })
        })?;
    let header = |name: &str| {
        response
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string)
    };
    let (status, retry_after, server_timing) = (
        response.status(),
        header(reqwest::header::RETRY_AFTER.as_str()),
        header(EDGE_SERVER_TIMING_HEADER),
    );
    let body = response.bytes().await.map_err(|_| {
        unreachable(format!(
            "The answer of the edge judge endpoint `{id}` could not be read."
        ))
    })?;
    if body.len() > EDGE_MAX_RESPONSE_BYTES {
        return Err(String::from("The edge judge answer is too large.").into());
    }
    Ok(Reply {
        status,
        retry_after,
        server_timing,
        body: body.to_vec(),
    })
}

/// Error code of an endpoint's error body `{ "error": <code>, "message": <text> }`; empty when the
/// body has none.
/// 端点错误正文 `{ "error": <代码>, "message": <说明> }` 中的错误代码；正文没有时为空。
fn error_code(body: &[u8]) -> String {
    serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .and_then(|value| value.get("error")?.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// Interprets the reply `target` sent.
/// 解释 `target` 返回的回复。
fn interpret(target: &Target, reply: &Reply) -> Result<Answer, Failure> {
    let (id, status) = (&target.endpoint_id, reply.status);
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Ok(Answer::TokenRejected);
    }
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(rate_limited(reply.retry_after.as_deref()).into());
    }
    if !status.is_success() {
        let code = error_code(&reply.body);
        let site = &target.site;
        return Err(match code.as_str() {
            "site_required" => Failure::from(format!(
                "The edge judge endpoint `{id}` did not accept the site marker `{site}` this OpenVulcan account was issued."
            )),
            "site_not_accepted" => Failure {
                message: format!(
                    "The edge judge endpoint `{id}` does not serve accounts of the OpenVulcan site `{site}`; choose another endpoint or return to automatic selection."
                ),
                reprobe: true,
            },
            _ => Failure {
                message: format!(
                    "The edge judge endpoint `{id}` refused the call with HTTP {} `{code}`.",
                    status.as_u16()
                ),
                reprobe: status.is_server_error(),
            },
        });
    }
    serde_json::from_slice(&reply.body)
        .map(Answer::Judged)
        .map_err(|_| String::from("The edge judge answer violates its contract.").into())
}

/// Sends one judge request to `target` with `token` and the target's site marker.
/// 用 `token` 与目标的来源标记向 `target` 发送一次判定请求。
async fn send(target: &Target, token: &str, request: &EdgeJudgeRequest) -> Result<Answer, Failure> {
    interpret(target, &post(target, token, request).await?)
}

/// Sends `request` to the target and with the token `access` returns; when the endpoint rejects the
/// token, `access` is asked once for a renewed one and the request is sent once more.
/// 按 `access` 返回的目标与令牌发送 `request`；端点拒绝令牌时，向 `access` 请求一次续期的令牌并再
/// 发送一次。
async fn exchange<F, Fut>(
    mut access: F,
    request: &EdgeJudgeRequest,
) -> Result<EdgeJudgeResponse, Failure>
where
    F: FnMut(bool) -> Fut,
    Fut: std::future::Future<Output = Result<(Target, String), String>>,
{
    let (target, token) = access(false).await?;
    if let Answer::Judged(response) = send(&target, &token, request).await? {
        return Ok(response);
    }
    let (target, token) = access(true).await?;
    match send(&target, &token, request).await? {
        Answer::Judged(response) => Ok(response),
        Answer::TokenRejected => {
            Err(String::from("The edge judge rejected a freshly issued access token.").into())
        }
    }
}

/// Lists the cloud models the signed-in account's deployment currently serves, sorted
/// by id.
/// 列出已登录账户所属部署当前提供的云端模型，按 id 排序。
pub(super) async fn list_models(state: &SharedServiceBridgeState) -> Result<Vec<String>, String> {
    let (_, secret) = signed_in_credential(state)
        .await?
        .ok_or_else(|| String::from("Sign in to OpenVulcan to list the cloud models."))?;
    request_openvulcan_edge_models(http()?, &secret).await
}

/// Judges `inputs` with the official model `model_id` on the locked endpoint, or the selected one
/// when `locked` is `None`; one checked outcome per input, in order, with the checkpoint the node
/// reported. Every judgement sent for the signed-in account is recorded in the usage ledger.
/// 用官方模型 `model_id` 在锁定的端点（`locked` 为 `None` 时为选中的端点）上判定 `inputs`；每条输入
/// 按顺序得到一条已检查的结果，并附节点报告的检查点。为已登录账户发出的每次判定都记入用量账本。
pub(super) async fn judge(
    state: &SharedServiceBridgeState,
    runtime: &PermissionJudgeRuntime,
    model_id: &str,
    locked: Option<&str>,
    inputs: Vec<JudgeInput>,
) -> Result<EdgeAnswer, String> {
    let (connection_id, secret) = signed_in_credential(state)
        .await?
        .ok_or_else(|| String::from("Sign in to OpenVulcan before using the cloud judge."))?;
    let started_at_ms = current_v1_unix_millis()?;
    let answer = judge_signed_in(runtime, model_id, locked, inputs, &connection_id, &secret).await;
    let finished_at_ms = current_v1_unix_millis()?;
    let outcomes = match &answer {
        Ok(answer) => answer
            .outcomes
            .iter()
            .map(|outcome| match outcome {
                EdgeOutcome::Evaluated { input_tokens, .. } => JudgeUsageOutcome::Evaluated {
                    input_tokens: *input_tokens,
                },
                EdgeOutcome::InputTokenLimit => {
                    JudgeUsageOutcome::InputTokenLimit { input_tokens: None }
                }
            })
            .collect(),
        Err(_) => vec![JudgeUsageOutcome::Failed],
    };
    for outcome in outcomes {
        record_judge_usage(
            state,
            JudgeUsage {
                route: JudgeRoute::Edge {
                    connection_id: connection_id.clone(),
                },
                model: model_id.to_string(),
                started_at_ms,
                finished_at_ms,
                outcome,
            },
        );
    }
    answer
}

/// Judges `inputs` for the signed-in account `connection_id` holding `secret`; see [`judge`].
/// 为持有 `secret` 的已登录账户 `connection_id` 判定 `inputs`；见 [`judge`]。
async fn judge_signed_in(
    runtime: &PermissionJudgeRuntime,
    model_id: &str,
    locked: Option<&str>,
    inputs: Vec<JudgeInput>,
    connection_id: &str,
    secret: &CredentialExecutionSecret,
) -> Result<EdgeAnswer, String> {
    let expected = inputs.len();
    let request = EdgeJudgeRequest {
        request_id: format!("permission-judge-{}", uuid::Uuid::new_v4()),
        inputs,
    };
    let response = exchange(
        |renew| async move {
            let grant = runtime
                .edge_access
                .access(connection_id, secret, model_id, renew)
                .await?;
            let target = runtime.edge_select.target(model_id, &grant, locked)?;
            Ok((target, grant.token))
        },
        &request,
    )
    .await
    .map_err(|failure| {
        if failure.reprobe {
            runtime.edge_select.request_probe();
        }
        failure.message
    })?;
    edge_outcomes(
        &hosted_policy().version,
        &request.request_id,
        expected,
        response,
    )
}

/// Requests one request test sends, one after another.
/// 一次请求测试依次发送的请求数。
const REQUEST_TEST_ATTEMPTS: usize = 5;

/// Longest command a request test accepts from the user, in characters.
/// 请求测试接受的用户命令最大长度（字符）。
const REQUEST_TEST_MAX_COMMAND_CHARS: usize = 64 * 1024;

/// Hex digits of the oversized sample; well over the largest token limit an official model has
/// (32 768 tokens), so the endpoint answers it without running the model.
/// 超长示例的十六进制位数；远超官方模型的最大 token 上限（32 768），端点不运行模型即作答。
const OVERSIZED_SAMPLE_HEX_DIGITS: usize = 120_000;

/// Command a request test sends.
/// 请求测试发送的命令。
fn test_command(input: &PermissionJudgeEdgeTestInput) -> Result<String, String> {
    Ok(match input {
        PermissionJudgeEdgeTestInput::Preset { preset } => match preset {
            PermissionJudgeEdgeTestPreset::Ordinary => String::from("git status"),
            PermissionJudgeEdgeTestPreset::Destructive => String::from("rm -rf ~"),
            PermissionJudgeEdgeTestPreset::DatabaseReset => {
                String::from("psql -d app -c 'DROP SCHEMA public CASCADE'")
            }
            PermissionJudgeEdgeTestPreset::Oversized => {
                let digits = (0..OVERSIZED_SAMPLE_HEX_DIGITS)
                    .map(|index| char::from(b"0123456789abcdef"[(index * 7 + index / 16) % 16]))
                    .collect::<String>();
                format!("echo {digits} > payload.hex")
            }
        },
        PermissionJudgeEdgeTestInput::Custom { command } => {
            if command.trim().is_empty() {
                return Err(String::from("Enter a command to send."));
            }
            if command.chars().count() > REQUEST_TEST_MAX_COMMAND_CHARS {
                return Err(format!(
                    "The command is longer than {REQUEST_TEST_MAX_COMMAND_CHARS} characters."
                ));
            }
            command.clone()
        }
    })
}

/// The endpoint `endpoint_id` of `grant`.
/// `grant` 中的端点 `endpoint_id`。
fn offered_target(grant: &Grant, model_id: &str, endpoint_id: &str) -> Result<Target, String> {
    let endpoint = grant
        .endpoints
        .iter()
        .find(|endpoint| endpoint.endpoint_id == endpoint_id)
        .ok_or_else(|| {
            format!(
                "OpenVulcan does not offer the endpoint `{endpoint_id}` for the cloud model `{model_id}`."
            )
        })?;
    Ok(Target {
        endpoint_id: endpoint.endpoint_id.clone(),
        endpoint: endpoint.endpoint.clone(),
        site: grant.site.clone(),
    })
}

/// Sends `request` and times it from sending until the whole reply is read.
/// 发送 `request` 并计时，从发送到读完整个回复。
async fn timed_post(
    target: &Target,
    token: &str,
    request: &EdgeJudgeRequest,
) -> (f64, Result<Reply, Failure>) {
    let started = Instant::now();
    let reply = post(target, token, request).await;
    (started.elapsed().as_secs_f64() * 1_000.0, reply)
}

/// Reports one request of a request test: its stage times, the answer and, when the answer would
/// not count as a judgement, why, in the words a real judgement would record.
/// 报告请求测试中的一个请求：各阶段耗时、回答，以及回答不能作为判定时的原因（措辞与真实判定记录的相同）。
fn test_attempt(
    target: &Target,
    request: &EdgeJudgeRequest,
    total_ms: f64,
    reply: Result<Reply, Failure>,
    token_renewed: bool,
) -> PermissionJudgeEdgeRequestAttempt {
    let mut attempt = PermissionJudgeEdgeRequestAttempt {
        request_id: request.request_id.clone(),
        total_ms,
        network_ms: None,
        endpoint_ms: None,
        node_ms: None,
        queue_ms: None,
        compute_ms: None,
        http_status: None,
        token_renewed,
        answer: None,
        error_code: None,
        error: None,
    };
    let reply = match reply {
        Ok(reply) => reply,
        Err(failure) => {
            attempt.error = Some(failure.message);
            return attempt;
        }
    };
    let timing = reply
        .server_timing
        .as_deref()
        .map(EdgeServerTiming::parse)
        .unwrap_or_default();
    attempt.network_ms = timing.edge.map(|edge| (total_ms - edge).max(0.0));
    attempt.endpoint_ms = timing.edge;
    attempt.node_ms = timing.node;
    attempt.queue_ms = timing.queue;
    attempt.compute_ms = timing.compute;
    attempt.http_status = Some(reply.status.as_u16());
    if !reply.status.is_success() {
        attempt.error_code = Some(error_code(&reply.body)).filter(|code| !code.is_empty());
    }
    attempt.error = match interpret(target, &reply) {
        Ok(Answer::Judged(response)) => {
            attempt.answer = Some(PermissionJudgeEdgeTestAnswer {
                region: response.region.clone(),
                model: response.model.clone(),
                checkpoint_sha256: response.checkpoint_sha256.clone(),
                policy_version: response.policy_version.clone(),
                results: response
                    .results
                    .iter()
                    .map(|result| PermissionJudgeEdgeTestResult {
                        label: result.label,
                        logits: result.logits,
                        input_tokens: result.input_tokens,
                        model_evaluated: result.model_evaluated,
                        reason_code: result.reason_code.clone(),
                    })
                    .collect(),
            });
            edge_outcomes(
                &hosted_policy().version,
                &request.request_id,
                request.inputs.len(),
                response,
            )
            .err()
        }
        Ok(Answer::TokenRejected) => Some(String::from(
            "The edge judge rejected a freshly issued access token.",
        )),
        Err(failure) => Some(failure.message),
    };
    attempt
}

/// Sends [`REQUEST_TEST_ATTEMPTS`] judge requests carrying `input`, one after another, to the
/// endpoint `endpoint_id` of the official model `model_id`, through the transport real judgements
/// use, and reports each. The token is obtained first so the user center stays out of the timings;
/// an endpoint that rejects it gets one renewed token. The run stops after a request that could not
/// reach the endpoint. Endpoint selection is neither consulted nor changed.
/// 将携带 `input` 的 [`REQUEST_TEST_ATTEMPTS`] 个判定请求依次发往官方模型 `model_id` 的端点
/// `endpoint_id`，使用真实判定所用的传输，并逐个报告。先取得令牌，使用户中心不计入耗时；端点拒绝令牌
/// 时续期一次。某个请求无法到达端点后停止。既不参考也不改变端点选择。
pub(super) async fn test_endpoint(
    state: &SharedServiceBridgeState,
    runtime: &PermissionJudgeRuntime,
    model_id: &str,
    endpoint_id: &str,
    input: &PermissionJudgeEdgeTestInput,
) -> Result<PermissionJudgeEdgeRequestTest, String> {
    let settings = hosted_policy()
        .resolve_settings(&Default::default())
        .map_err(|error| error.to_string())?
        .to_map();
    let arguments = serde_json::json!({ "command": test_command(input)? }).to_string();
    let rendered = render_call(None, "shell_command", &arguments);
    let (connection_id, secret) = signed_in_credential(state).await?.ok_or_else(|| {
        String::from("Sign in to OpenVulcan before testing a cloud judge endpoint.")
    })?;
    let mut grant = runtime
        .edge_access
        .access(&connection_id, &secret, model_id, false)
        .await?;
    let mut target = offered_target(&grant, model_id, endpoint_id)?;
    let (endpoint, site) = (target.endpoint.clone(), target.site.clone());
    let mut attempts = Vec::with_capacity(REQUEST_TEST_ATTEMPTS);
    for _ in 0..REQUEST_TEST_ATTEMPTS {
        let request = EdgeJudgeRequest {
            request_id: format!("permission-judge-test-{}", uuid::Uuid::new_v4()),
            inputs: vec![JudgeInput {
                settings: settings.clone(),
                rendered: rendered.clone(),
            }],
        };
        let (mut total_ms, mut reply) = timed_post(&target, &grant.token, &request).await;
        let rejected =
            matches!(&reply, Ok(reply) if reply.status == reqwest::StatusCode::UNAUTHORIZED);
        if rejected {
            match runtime
                .edge_access
                .access(&connection_id, &secret, model_id, true)
                .await
                .and_then(|renewed| {
                    let renewed_target = offered_target(&renewed, model_id, endpoint_id)?;
                    Ok((renewed, renewed_target))
                }) {
                Ok((renewed, renewed_target)) => {
                    (grant, target) = (renewed, renewed_target);
                    (total_ms, reply) = timed_post(&target, &grant.token, &request).await;
                }
                Err(error) => reply = Err(Failure::from(error)),
            }
        }
        let attempt = test_attempt(&target, &request, total_ms, reply, rejected);
        let stop = attempt.http_status.is_none();
        attempts.push(attempt);
        if stop {
            break;
        }
    }
    let mut totals = attempts
        .iter()
        .filter(|attempt| attempt.answer.is_some() && attempt.error.is_none())
        .map(|attempt| attempt.total_ms)
        .collect::<Vec<_>>();
    totals.sort_by(f64::total_cmp);
    Ok(PermissionJudgeEdgeRequestTest {
        model_id: model_id.to_string(),
        endpoint_id: endpoint_id.to_string(),
        endpoint,
        site,
        rendered_chars: rendered.chars().count() as u64,
        succeeded: totals.len() as u32,
        min_ms: totals.first().copied(),
        median_ms: totals.len().checked_sub(1).map(|last| totals[last / 2]),
        max_ms: totals.last().copied(),
        attempts,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex as StdMutex};

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use vulcan_permission_judge::JudgeLabel;

    use super::*;

    /// Starts a loopback endpoint that answers each request by its bearer token: `good` is judged,
    /// `busy` is saturated, `limited` is rate limited, `foreign` is from an unaccepted site and
    /// anything else is rejected; a request without a site marker is refused first. Returns the
    /// base URL and the token and site marker of each request seen.
    /// 启动回环端点，按 Bearer 令牌回答每个请求：`good` 返回判定，`busy` 返回饱和，`limited` 返回
    /// 限速，`foreign` 表示来源不被接受，其余一律拒绝；没有来源标记的请求先被拒绝。返回基础地址与
    /// 收到的每个请求的令牌与来源标记。
    async fn endpoint() -> (String, Arc<StdMutex<Vec<(String, String)>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let seen = Arc::new(StdMutex::new(Vec::new()));
        let record = Arc::clone(&seen);
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    return;
                };
                let mut buffer = Vec::new();
                let mut chunk = [0_u8; 4096];
                // Read the head, then the body its Content-Length announces.
                // 先读取请求头，再读取 Content-Length 声明的正文。
                let (head_end, length) = loop {
                    let read = stream.read(&mut chunk).await.unwrap();
                    buffer.extend_from_slice(&chunk[..read]);
                    if let Some(end) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&buffer[..end]).to_ascii_lowercase();
                        let length = head
                            .lines()
                            .find_map(|line| line.strip_prefix("content-length:"))
                            .map_or(0, |value| value.trim().parse::<usize>().unwrap());
                        break (end + 4, length);
                    }
                };
                while buffer.len() < head_end + length {
                    let read = stream.read(&mut chunk).await.unwrap();
                    buffer.extend_from_slice(&chunk[..read]);
                }
                let head = String::from_utf8_lossy(&buffer[..head_end]).to_string();
                let token = head
                    .lines()
                    .find_map(|line| line.strip_prefix("authorization: Bearer "))
                    .unwrap_or_default()
                    .to_string();
                let site = head
                    .lines()
                    .find_map(|line| line.strip_prefix("x-openvulcan-site: "))
                    .unwrap_or_default()
                    .to_string();
                let request: serde_json::Value =
                    serde_json::from_slice(&buffer[head_end..head_end + length]).unwrap();
                record.lock().unwrap().push((token.clone(), site.clone()));
                let (status, extra, body) = match token.as_str() {
                    _ if site.is_empty() => (
                        "400 Bad Request",
                        "",
                        serde_json::json!({"error": "site_required", "message": "site"}),
                    ),
                    "good" => (
                        "200 OK",
                        "",
                        serde_json::json!({
                            "request_id": request["request_id"], "region": "hk", "model": "m",
                            "checkpoint_sha256": "a".repeat(64), "policy_version": "p", "results": []
                        }),
                    ),
                    "busy" => (
                        "503 Service Unavailable",
                        "",
                        serde_json::json!({"error": "region_saturated", "message": "busy"}),
                    ),
                    "limited" => (
                        "429 Too Many Requests",
                        "retry-after: 30\r\n",
                        serde_json::json!({"error": "rate_limited", "message": "slow down"}),
                    ),
                    "foreign" => (
                        "403 Forbidden",
                        "",
                        serde_json::json!({"error": "site_not_accepted", "message": "site"}),
                    ),
                    _ => (
                        "401 Unauthorized",
                        "",
                        serde_json::json!({"error": "invalid_access_token", "message": "no"}),
                    ),
                };
                let body = body.to_string();
                let reply = format!(
                    "HTTP/1.1 {status}\r\ncontent-type: application/json\r\n{extra}content-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(reply.as_bytes()).await.unwrap();
            }
        });
        (format!("http://{address}"), seen)
    }

    fn request() -> EdgeJudgeRequest {
        EdgeJudgeRequest {
            request_id: String::from("r1"),
            inputs: Vec::new(),
        }
    }

    fn target(endpoint: &str, site: &str) -> Target {
        Target {
            endpoint_id: String::from("e1"),
            endpoint: endpoint.to_string(),
            site: site.to_string(),
        }
    }

    /// Runs `exchange` with a token per renewal step and returns the result and the renewals asked.
    /// 以每个续期步骤对应的令牌运行 `exchange`，返回结果与请求的续期序列。
    async fn run(
        endpoint: &str,
        first: &str,
        renewed: &str,
    ) -> (Result<EdgeJudgeResponse, Failure>, Vec<bool>) {
        let asked = StdMutex::new(Vec::new());
        let result = exchange(
            |renew| {
                asked.lock().unwrap().push(renew);
                let token = if renew { renewed } else { first };
                std::future::ready(Ok((target(endpoint, "global"), token.to_string())))
            },
            &request(),
        )
        .await;
        (result, asked.into_inner().unwrap())
    }

    fn tokens(seen: &StdMutex<Vec<(String, String)>>) -> Vec<String> {
        seen.lock()
            .unwrap()
            .iter()
            .map(|(token, _)| token.clone())
            .collect()
    }

    // A valid token is used once with no renewal, and every request carries the site marker.
    // 有效令牌只使用一次，不续期，且每个请求都带有来源标记。
    #[tokio::test]
    async fn a_valid_token_is_not_renewed() {
        let (base, seen) = endpoint().await;
        let (result, asked) = run(&base, "good", "unused").await;
        assert_eq!(result.unwrap().request_id, "r1");
        assert_eq!(asked, [false]);
        assert_eq!(
            *seen.lock().unwrap(),
            [(String::from("good"), String::from("global"))]
        );
    }

    // A rejected token is renewed once and the request is sent once more.
    // 被拒绝的令牌续期一次，请求再发送一次。
    #[tokio::test]
    async fn a_rejected_token_is_renewed_once_and_retried_once() {
        let (base, seen) = endpoint().await;
        let (result, asked) = run(&base, "stale", "good").await;
        assert!(result.is_ok());
        assert_eq!(asked, [false, true]);
        assert_eq!(tokens(&seen), ["stale", "good"]);

        let (base, seen) = endpoint().await;
        let (result, asked) = run(&base, "stale", "stale").await;
        assert!(!result.unwrap_err().reprobe);
        assert_eq!(asked, [false, true]);
        assert_eq!(seen.lock().unwrap().len(), 2);
    }

    // Any other refusal is an error without renewal or retry; server errors ask for a new probe.
    // 其他拒绝直接成为错误，不续期也不重试；服务端错误要求重新探测。
    #[tokio::test]
    async fn other_refusals_fail_without_a_retry() {
        let (base, seen) = endpoint().await;
        let (result, asked) = run(&base, "busy", "good").await;
        let failure = result.unwrap_err();
        assert!(failure.message.contains("region_saturated"));
        assert!(failure.reprobe);
        assert_eq!(asked, [false]);
        assert_eq!(seen.lock().unwrap().len(), 1);
    }

    // A rate-limited call is its own refusal, carries the requested wait and is not retried.
    // 被限速的调用是独立的拒绝，带有所需等待时间且不重试。
    #[tokio::test]
    async fn a_rate_limited_call_is_refused_without_a_retry() {
        let (base, seen) = endpoint().await;
        let (result, asked) = run(&base, "limited", "good").await;
        let failure = result.unwrap_err();
        assert_eq!(failure.message, rate_limited(Some("30")));
        assert!(!failure.reprobe);
        assert_eq!(asked, [false]);
        assert_eq!(seen.lock().unwrap().len(), 1);
        assert_ne!(rate_limited(Some("30")), rate_limited(None));
        assert_eq!(rate_limited(Some("soon")), rate_limited(None));
    }

    // Site refusals are distinct failures; an endpoint that does not serve the site asks for a new
    // probe, and neither renews the token.
    // 来源拒绝是独立的失败；不服务该来源的端点要求重新探测，两者都不续期令牌。
    #[tokio::test]
    async fn site_refusals_fail_without_renewal() {
        let (base, seen) = endpoint().await;
        let (result, asked) = run(&base, "foreign", "good").await;
        let foreign = result.unwrap_err();
        assert!(foreign.reprobe);
        assert_eq!(asked, [false]);

        let missing = send(&target(&base, ""), "good", &request())
            .await
            .err()
            .unwrap();
        assert!(!missing.reprobe);
        assert_ne!(missing.message, foreign.message);
        assert_eq!(seen.lock().unwrap().len(), 2);
    }

    // An endpoint that cannot be reached asks for a new probe.
    // 无法连接的端点要求重新探测。
    #[tokio::test]
    async fn an_unreachable_endpoint_asks_for_a_new_probe() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let failure = send(
            &target(&format!("http://{address}"), "global"),
            "good",
            &request(),
        )
        .await
        .err()
        .unwrap();
        assert!(failure.reprobe);
    }

    /// A request test request of one input, and a reply to it.
    /// 含一条输入的请求测试请求及其回复。
    fn one_input_request() -> EdgeJudgeRequest {
        EdgeJudgeRequest {
            request_id: String::from("permission-judge-test-1"),
            inputs: vec![JudgeInput {
                settings: BTreeMap::new(),
                rendered: String::from("Tool: shell_command"),
            }],
        }
    }

    fn reply(status: u16, server_timing: Option<&str>, body: serde_json::Value) -> Reply {
        Reply {
            status: reqwest::StatusCode::from_u16(status).unwrap(),
            retry_after: None,
            server_timing: server_timing.map(str::to_string),
            body: body.to_string().into_bytes(),
        }
    }

    fn judged(policy_version: &str) -> serde_json::Value {
        serde_json::json!({
            "request_id": "permission-judge-test-1", "region": "us-west", "model": "m",
            "checkpoint_sha256": "a".repeat(64), "policy_version": policy_version,
            "results": [{"label": "review", "logits": [0.1, 2.0, -1.0], "input_tokens": 41,
                         "model_evaluated": true}]
        })
    }

    // A judged answer reports every stage, the network share and the answer, without an error.
    // 已判定的回答报告每个阶段、网络部分与回答内容，没有错误。
    #[test]
    fn a_judged_request_reports_its_stages_and_answer() {
        let attempt = test_attempt(
            &target("https://e1", "global"),
            &one_input_request(),
            120.0,
            Ok(reply(
                200,
                Some("edge;dur=45, node;dur=40.5, queue;dur=2, compute;dur=31.5"),
                judged(&hosted_policy().version),
            )),
            false,
        );
        assert_eq!(attempt.http_status, Some(200));
        assert_eq!(attempt.network_ms, Some(75.0));
        assert_eq!(
            (
                attempt.endpoint_ms,
                attempt.node_ms,
                attempt.queue_ms,
                attempt.compute_ms
            ),
            (Some(45.0), Some(40.5), Some(2.0), Some(31.5))
        );
        assert_eq!(attempt.error, None);
        let answer = attempt.answer.unwrap();
        assert_eq!(answer.region, "us-west");
        assert_eq!(answer.results[0].label, JudgeLabel::Review);
        assert_eq!(answer.results[0].input_tokens, Some(41));
    }

    // An answer a real judgement would refuse is shown with the refusal.
    // 真实判定会拒绝的回答与拒绝原因一同显示。
    #[test]
    fn an_answer_failing_the_client_checks_keeps_the_answer_and_the_reason() {
        let attempt = test_attempt(
            &target("https://e1", "global"),
            &one_input_request(),
            80.0,
            Ok(reply(200, None, judged("another-policy"))),
            false,
        );
        assert!(attempt.answer.is_some());
        assert!(attempt.error.is_some());
        assert_eq!(attempt.network_ms, None);
    }

    // A refusal keeps the endpoint's code and the message a real judgement records.
    // 拒绝保留端点的错误代码以及真实判定记录的说明。
    #[test]
    fn a_refused_request_reports_the_code_and_the_judgement_failure() {
        let target = target("https://e1", "global");
        let refused = reply(
            503,
            Some("edge;dur=3"),
            serde_json::json!({"error": "region_saturated", "message": "busy"}),
        );
        let expected = interpret(&target, &refused).err().unwrap().message;
        let attempt = test_attempt(&target, &one_input_request(), 9.0, Ok(refused), true);
        assert_eq!(attempt.http_status, Some(503));
        assert_eq!(attempt.error_code.as_deref(), Some("region_saturated"));
        assert_eq!(attempt.error, Some(expected));
        assert!(attempt.answer.is_none() && attempt.token_renewed);
    }

    // A request that never reached the endpoint has no status, which stops the run.
    // 未到达端点的请求没有状态，测试随之停止。
    #[test]
    fn an_unreachable_request_has_no_status() {
        let attempt = test_attempt(
            &target("https://e1", "global"),
            &one_input_request(),
            15_000.0,
            Err(Failure::from(String::from("unreachable"))),
            false,
        );
        assert_eq!(attempt.http_status, None);
        assert_eq!(attempt.error.as_deref(), Some("unreachable"));
    }

    // Custom commands must not be blank or too long; the oversized sample exceeds the token limit
    // in characters alone.
    // 自定义命令不得为空白或过长；超长示例仅按字符数就超过 token 上限。
    #[test]
    fn test_commands_check_custom_input_and_build_the_samples() {
        let custom = |command: &str| {
            test_command(&PermissionJudgeEdgeTestInput::Custom {
                command: command.to_string(),
            })
        };
        assert!(custom("  ").is_err());
        assert!(custom(&"x".repeat(REQUEST_TEST_MAX_COMMAND_CHARS + 1)).is_err());
        assert_eq!(custom("ls -la").unwrap(), "ls -la");
        let oversized = test_command(&PermissionJudgeEdgeTestInput::Preset {
            preset: PermissionJudgeEdgeTestPreset::Oversized,
        })
        .unwrap();
        assert!(oversized.len() > OVERSIZED_SAMPLE_HEX_DIGITS);
    }
}
