// Permission judge page: switch and backend, the cloud decision route or the local model status,
// the LLM review route, speed tests, the model registry and the CUDA runtime, mirroring the WebUI
// permission judge settings area. Every edit saves at once through the complete save request.
// 权限判定页：开关与后端、云端判定路由或本机模型状态、LLM 复核路由、速度测试、模型登记表与 CUDA
// 执行库，镜像 WebUI 权限判定设置区。每次编辑都通过完整保存请求立即保存。

use super::*;
use crate::service_api::{permission_judge as judge, settings_actions as settings};

// Consumer profiles and the review purpose owned by the service kernel.
// 服务内核持有的消费者配置档与复核用途。
pub(super) const JUDGE_PROFILE: &str = "permission_judge";
const REVIEW_PROFILE: &str = "permission_review";
const REVIEW_PURPOSE: &str = "tool_call_review";
// Choice value that saves no explicit reasoning effort.
// 不保存显式思考档位的选择值。
const MODEL_DEFAULT_EFFORT: &str = "model-default";

/// Labels a judge backend code; unknown codes render as themselves.
/// 为判定后端代码生成标签；未知代码按原样显示。
pub(super) fn backend_label(code: &str) -> String {
    match code {
        "local" => tr!("settings.permissionJudge.backend.local"),
        "cloud" => tr!("settings.permissionJudge.backend.cloud"),
        other => other.to_string(),
    }
}

/// Labels a device code; unknown codes render as themselves.
/// 为设备代码生成标签；未知代码按原样显示。
pub(super) fn device_label(code: &str) -> String {
    match code {
        "cuda" => tr!("settings.permissionJudge.device.cuda"),
        "webgpu" => tr!("settings.permissionJudge.device.webgpu"),
        "cpu" => tr!("settings.permissionJudge.device.cpu"),
        other => other.to_string(),
    }
}

/// Labels a device's unavailable reason code.
/// 为设备不可用原因代码生成标签。
pub(super) fn reason_label(code: &str) -> String {
    match code {
        "runtime_missing" => tr!("settings.permissionJudge.reason.runtimeMissing"),
        "unsupported_platform" => tr!("settings.permissionJudge.reason.unsupportedPlatform"),
        "driver_missing" => tr!("settings.permissionJudge.reason.driverMissing"),
        "driver_too_old" => tr!("settings.permissionJudge.reason.driverTooOld"),
        "onnxruntime_missing" => tr!("settings.permissionJudge.reason.onnxruntimeMissing"),
        "cuda_libraries_missing" => tr!("settings.permissionJudge.reason.cudaLibrariesMissing"),
        other => other.to_string(),
    }
}

/// Labels a worker phase and picks its tone.
/// 为 worker 阶段生成标签并选择色调。
pub(super) fn phase_label(code: &str) -> (String, Tone) {
    match code {
        "not_loaded" => (tr!("settings.permissionJudge.phase.notLoaded"), Tone::Muted),
        "warming" => (tr!("settings.permissionJudge.phase.warming"), Tone::Warn),
        "ready" => (tr!("settings.permissionJudge.phase.ready"), Tone::Good),
        "failed" => (tr!("settings.permissionJudge.phase.failed"), Tone::Bad),
        other => (other.to_string(), Tone::Muted),
    }
}

/// Labels a CUDA installation phase.
/// 为 CUDA 安装阶段生成标签。
fn install_phase_label(code: &str) -> String {
    match code {
        "downloading" => tr!("settings.permissionJudge.cuda.phase.downloading"),
        "verifying" => tr!("settings.permissionJudge.cuda.phase.verifying"),
        "extracting" => tr!("settings.permissionJudge.cuda.phase.extracting"),
        "completed" => tr!("settings.permissionJudge.cuda.phase.completed"),
        other => other.to_string(),
    }
}

/// Labels where a CUDA library folder was found.
/// 为 CUDA 库文件夹的来源生成标签。
fn source_label(code: &str) -> String {
    match code {
        "managed" => tr!("settings.permissionJudge.cuda.source.managed"),
        "toolkit" => tr!("settings.permissionJudge.cuda.source.toolkit"),
        "search_path" => tr!("settings.permissionJudge.cuda.source.searchPath"),
        other => other.to_string(),
    }
}

/// Labels an official CUDA download link.
/// 为官方 CUDA 下载链接生成标签。
fn link_label(code: &str) -> String {
    match code {
        "cuda_toolkit" => tr!("settings.permissionJudge.cuda.link.toolkit"),
        "cuda_toolkit_archive" => tr!("settings.permissionJudge.cuda.link.toolkitArchive"),
        "nvidia_driver" => tr!("settings.permissionJudge.cuda.link.driver"),
        "cuda_redistributables" => tr!("settings.permissionJudge.cuda.link.redistributables"),
        "cuda_compatibility" => tr!("settings.permissionJudge.cuda.link.compatibility"),
        "onnxruntime_cuda_requirements" => {
            tr!("settings.permissionJudge.cuda.link.onnxruntimeRequirements")
        }
        other => other.to_string(),
    }
}

/// Labels an installable CUDA runtime part.
/// 为可安装的 CUDA 执行库部分生成标签。
fn component_label(code: &str) -> String {
    match code {
        "onnxruntime" => tr!("settings.permissionJudge.cuda.onnxruntime"),
        "cuda_libraries" => tr!("settings.permissionJudge.cuda.libraries"),
        other => other.to_string(),
    }
}

/// Labels a reasoning effort a model declares.
/// 为模型声明的思考档位生成标签。
fn effort_label(code: &str) -> String {
    match code {
        "none" => tr!("composer.reasoning.level.none"),
        "adaptive" => tr!("composer.reasoning.level.adaptive"),
        "minimal" => tr!("composer.reasoning.level.minimal"),
        "low" => tr!("composer.reasoning.level.low"),
        "medium" => tr!("composer.reasoning.level.medium"),
        "high" => tr!("composer.reasoning.level.high"),
        "xhigh" => tr!("composer.reasoning.level.xHigh"),
        "max" => tr!("composer.reasoning.level.max"),
        "ultra" => tr!("composer.reasoning.level.ultra"),
        other => other.to_string(),
    }
}

/// Renders a duration in milliseconds, or a dash when the reading is absent.
/// 渲染毫秒时长；读数缺失时显示破折号。
fn ms(value: &Value) -> String {
    value.as_f64().map_or_else(
        || "—".into(),
        |value| tr!("settings.permissionJudge.ms", value = format!("{value:.1}")),
    )
}

/// Renders a size the way the WebUI does: one decimal, trailing `.0` dropped.
/// 按 WebUI 的方式渲染大小：保留一位小数，去掉末尾的 `.0`。
fn format_bytes(bytes: f64) -> String {
    const UNITS: [&str; 5] = ["KB", "MB", "GB", "TB", "PB"];
    // Non-negative whole bytes, as the WebUI normalizes them.
    // 与 WebUI 一致的非负整数字节数。
    let bytes = if bytes.is_finite() {
        bytes.max(0.0).round()
    } else {
        0.0
    };
    if bytes < 1024.0 {
        return format!("{bytes} B");
    }
    let mut unit = 1024.0;
    for (index, suffix) in UNITS.iter().enumerate() {
        // One decimal, rounded half away from zero as `Math.round` does for positives.
        // 保留一位小数，正数按 `Math.round` 的方式四舍五入。
        let scaled = (bytes / unit * 10.0).round() / 10.0;
        if scaled < 1024.0 || index == UNITS.len() - 1 {
            let text = format!("{scaled:.1}");
            return format!("{} {suffix}", text.strip_suffix(".0").unwrap_or(&text));
        }
        unit *= 1024.0;
    }
    unreachable!("the last unit always returns")
}

/// Renders an optional size, or a dash when the reading is absent.
/// 渲染可选大小；读数缺失时显示破折号。
fn optional_bytes(value: &Value) -> String {
    value.as_f64().map_or_else(|| "—".into(), format_bytes)
}

/// The complete save request for the saved choices; an unset backend is local and an unset
/// device follows the recommendation, as the WebUI draft does.
/// 已保存选择对应的完整保存请求；未设置的后端为本机，未设置的设备跟随推荐值，与 WebUI 草稿一致。
fn save_request(snapshot: &Value) -> Value {
    // Saved device, else the recommendation.
    // 已保存设备，否则为推荐设备。
    let device = match v(snapshot, "/device") {
        Value::Null => v(snapshot, "/recommendedDevice").clone(),
        device => device.clone(),
    };
    json!({
        "backend": v(snapshot, "/backend"),
        "device": device,
        "cudaLibraryDirectory": v(snapshot, "/cuda/lockedDirectory"),
        "llmMayAllowBlocked": b(snapshot, "/llmMayAllowBlocked"),
        "recordDecisions": b(snapshot, "/recordDecisions"),
        "approvalTimeoutSeconds": n(snapshot, "/approvalTimeoutSeconds"),
    })
}

/// Locks a CUDA library folder; an empty entry uses the one found.
/// 锁定 CUDA 库文件夹；留空则使用探测到的文件夹。
fn set_cuda_directory(base: &Value, value: &SettingValue) -> Option<ServiceCall> {
    let mut request = base.clone();
    let directory = value.as_str().trim();
    request["cudaLibraryDirectory"] = if directory.is_empty() {
        Value::Null
    } else {
        json!(directory)
    };
    Some(judge::save_permission_judge_settings(request))
}

/// Mirrors `permissionJudgeCloudCandidates`: validated internal decision routes.
/// 镜像 `permissionJudgeCloudCandidates`：已验证的内部判定路由。
fn cloud_candidate(candidate: &Value) -> bool {
    b(candidate, "/enabled")
        && s(candidate, "/route/usage_scope") == "internal_background"
        && s(candidate, "/route/operation") == "decision_evaluate"
        && s(candidate, "/route/binding/kind") == "action"
}

/// Mirrors `permissionReviewCandidates`: internal conversation routes that answer with JSON.
/// 镜像 `permissionReviewCandidates`：能以 JSON 作答的内部对话路由。
fn review_candidate(candidate: &Value) -> bool {
    b(candidate, "/enabled")
        && s(candidate, "/route/usage_scope") == "internal_background"
        && s(candidate, "/route/operation") == "conversation_respond"
        && s(candidate, "/route/binding/kind") == "protocol"
        && matches!(
            v(candidate, "/capabilities/structured_output").as_str(),
            Some("json_schema" | "json_object")
        )
}

/// One choice option per candidate route, after the unconfigured option.
/// 未配置选项之后，每条候选路由一个选择项。
fn route_options(candidates: &[&Value]) -> Vec<(String, String)> {
    let mut options = vec![("null".to_string(), tr!("tui.field.unconfigured"))];
    for candidate in candidates {
        options.push((
            v(candidate, "/route").to_string(),
            format!(
                "{} / {} / {}",
                s(candidate, "/providerDisplayName"),
                s(candidate, "/connectionDisplayName"),
                s(candidate, "/modelDisplayName")
            ),
        ));
    }
    options
}

/// Saves the selected cloud decision route, or removes it.
/// 保存所选云端判定路由，或将其移除。
fn select_cloud_route(_: &Value, value: &SettingValue) -> Option<ServiceCall> {
    Some(settings::save_public_ai_inference_route(
        JUDGE_PROFILE,
        None,
        "decision_evaluate",
        serde_json::from_str(value.as_str()).ok()?,
        None,
    ))
}

/// Saves the selected review route, keeping the saved effort only when the new model declares it.
/// 保存所选复核路由；仅当新模型声明已保存的档位时才保留它。
fn select_review_route(context: &Value, value: &SettingValue) -> Option<ServiceCall> {
    let route: Value = serde_json::from_str(value.as_str()).ok()?;
    // Saved effort, kept when the chosen model declares it.
    // 已保存档位，所选模型声明它时保留。
    let effort = context
        .get("effort")
        .and_then(Value::as_str)
        .filter(|effort| {
            !route.is_null()
                && a(context, "/candidates").iter().any(|candidate| {
                    v(candidate, "/route") == &route
                        && a(candidate, "/capabilities/reasoning_efforts")
                            .iter()
                            .any(|declared| declared == effort)
                })
        });
    Some(settings::save_public_ai_inference_route(
        REVIEW_PROFILE,
        Some(REVIEW_PURPOSE),
        "conversation_respond",
        route,
        effort,
    ))
}

/// Saves the review route with the chosen effort; the default option saves none.
/// 以所选档位保存复核路由；默认选项不保存档位。
fn select_review_effort(route: &Value, value: &SettingValue) -> Option<ServiceCall> {
    let effort = value.as_str();
    Some(settings::save_public_ai_inference_route(
        REVIEW_PROFILE,
        Some(REVIEW_PURPOSE),
        "conversation_respond",
        route.clone(),
        (effort != MODEL_DEFAULT_EFFORT).then_some(effort),
    ))
}

/// Builds the permission judge page from the settings snapshot, the live status and the public
/// AI routes.
/// 根据设置快照、实时状态与公共 AI 路由构建权限判定页。
pub(super) fn rows(app: &App) -> Vec<Row> {
    let snapshot = app.slots.value(Slot::PermissionJudgeSettings);
    if snapshot.is_null() {
        return Vec::new();
    }
    // The polled status, or the one the snapshot carried until the first poll arrives.
    // 轮询到的状态；首次轮询到达前使用快照携带的状态。
    let status = match app.slots.value(Slot::PermissionJudgeStatus) {
        Value::Null => v(snapshot, "/status"),
        status => status,
    };
    let ai = app.slots.value(Slot::PublicAiSettings);
    let base = save_request(snapshot);
    // The saved backend is what makes the judge configured; until then both setups are shown.
    // 已保存的后端决定判定器是否已配置；在此之前同时显示两种设置。
    let saved = v(snapshot, "/backend").as_str();
    let configured = saved.is_some();
    let local = saved != Some("cloud");
    let cloud = saved != Some("local");
    let active_model = a(snapshot, "/models")
        .iter()
        .find(|model| v(model, "/id") == v(snapshot, "/activeModelId"));
    let cloud_route = v(ai, &format!("/inferenceProfiles/{JUDGE_PROFILE}/decision"));

    let mut rows = vec![button(
        tr!("setupGuide.open"),
        UiAction::GoPage(PageId::SetupGuide),
        Tone::Normal,
    )];
    rows.push(info(
        tr!("settings.permissionJudge.backend"),
        match saved {
            Some(backend) => format!(
                "{} · {}",
                tr!("settings.permissionJudge.state.configured"),
                backend_label(backend)
            ),
            None => tr!("settings.permissionJudge.state.unset"),
        },
        if configured { Tone::Good } else { Tone::Warn },
    ));
    rows.push(note(
        tr!("settings.permissionJudge.backendHint"),
        Tone::Muted,
    ));
    // Each other backend is offered only once it is set up, so the judge never points at one
    // that cannot run.
    // 其他后端只有在设置完成后才提供，因此判定器不会指向无法运行的后端。
    for (backend, ready, missing) in [
        (
            "local",
            active_model.is_some(),
            tr!("settings.permissionJudge.needsModel"),
        ),
        (
            "cloud",
            !cloud_route.is_null(),
            tr!("settings.permissionJudge.needsCloudRoute"),
        ),
    ] {
        if saved == Some(backend) {
            continue;
        }
        if ready {
            let mut request = base.clone();
            request["backend"] = json!(backend);
            let name = backend_label(backend);
            rows.push(button(
                if configured {
                    tr!("settings.permissionJudge.switchBackend", backend = name)
                } else {
                    tr!("settings.permissionJudge.useBackend", backend = name)
                },
                call(judge::save_permission_judge_settings(request)),
                Tone::Normal,
            ));
        } else if !configured {
            rows.push(note(
                format!("{} · {missing}", backend_label(backend)),
                Tone::Muted,
            ));
        }
    }
    if local {
        device_rows(&mut rows, snapshot, &base);
    }
    rows.push(
        toggle(
            tr!("settings.permissionJudge.llmMayAllowBlocked"),
            b(&base, "/llmMayAllowBlocked"),
            dto(
                base.clone(),
                "/llmMayAllowBlocked",
                judge::save_permission_judge_settings,
            ),
        )
        .detail(tr!("settings.permissionJudge.llmMayAllowBlockedHint")),
    );

    if local {
        status_rows(&mut rows, snapshot, status);
    }
    if cloud {
        cloud_rows(&mut rows, ai, cloud_route);
    }
    review_rows(&mut rows, ai);
    approval_rows(&mut rows, snapshot, &base);
    benchmark_rows(&mut rows, app, snapshot, status, configured);
    if local {
        model_rows(&mut rows, app, snapshot);
        cuda_rows(&mut rows, snapshot, status, &base);
    }
    records_rows(&mut rows, app, &base);
    rows
}

/// How long an approval waits for the user before it is denied; the limit is the one the service publishes.
/// 审批在被拒绝前等待用户的时长；上限取自服务发布的值。
fn approval_rows(rows: &mut Vec<Row>, snapshot: &Value, base: &Value) {
    rows.push(section(tr!("settings.permissionJudge.approval.title")));
    rows.push(
        number(
            tr!("settings.permissionJudge.approval.timeout"),
            n(base, "/approvalTimeoutSeconds"),
            (0, n(snapshot, "/maxApprovalTimeoutSeconds"), 5),
            dto(
                base.clone(),
                "/approvalTimeoutSeconds",
                judge::save_permission_judge_settings,
            ),
        )
        .detail(tr!("settings.permissionJudge.approval.timeoutHint")),
    );
}

/// The device choice: usable devices only, with the unusable ones and the recommendation noted.
/// 设备选择：只列可用设备，并注明不可用设备与推荐设备。
fn device_rows(rows: &mut Vec<Row>, snapshot: &Value, base: &Value) {
    let devices = a(snapshot, "/devices");
    rows.push(choice(
        tr!("settings.permissionJudge.device"),
        devices
            .iter()
            .filter(|device| v(device, "/unavailableReason").is_null())
            .map(|device| (s(device, "/device"), device_label(&s(device, "/device"))))
            .collect(),
        s(base, "/device"),
        dto(
            base.clone(),
            "/device",
            judge::save_permission_judge_settings,
        ),
    ));
    if let Some(recommended) = v(snapshot, "/recommendedDevice").as_str() {
        rows.push(note(
            tr!(
                "settings.permissionJudge.recommended",
                device = device_label(recommended)
            ),
            Tone::Muted,
        ));
    }
    for device in devices {
        if let Some(reason) = v(device, "/unavailableReason").as_str() {
            rows.push(note(
                format!(
                    "{} · {}",
                    device_label(&s(device, "/device")),
                    reason_label(reason)
                ),
                Tone::Muted,
            ));
        }
    }
}

/// The local model's phase, memory, GPU use and latency.
/// 本机模型的阶段、内存、GPU 占用与延迟。
fn status_rows(rows: &mut Vec<Row>, snapshot: &Value, status: &Value) {
    let worker = v(status, "/worker");
    let (phase, tone) = phase_label(&s(worker, "/phase"));
    rows.push(section(tr!("settings.permissionJudge.status")));
    // Registered name of the loaded model, else its id.
    // 已加载模型的登记名称，否则为其标识。
    let model = a(snapshot, "/models")
        .iter()
        .find(|model| v(model, "/id") == v(worker, "/modelId"))
        .map(|model| s(model, "/name"))
        .or_else(|| v(worker, "/modelId").as_str().map(str::to_string))
        .unwrap_or_else(|| "—".into());
    rows.push(info(
        tr!("settings.permissionJudge.status.model"),
        format!("{model} · {phase}"),
        tone,
    ));
    if s(worker, "/phase") == "failed" {
        rows.push(note(
            format!(
                "{} {}",
                tr!("settings.permissionJudge.status.loadFailed"),
                s(worker, "/error")
            ),
            Tone::Bad,
        ));
        rows.push(button(
            tr!("settings.permissionJudge.reload"),
            call(judge::reload_permission_judge()),
            Tone::Accent,
        ));
    }
    let issue = v(snapshot, "/activeModelIssue");
    if !issue.is_null() {
        rows.push(note(
            format!(
                "{} {}",
                tr!(
                    "settings.permissionJudge.activeModelIssue",
                    code = s(issue, "/code")
                ),
                s(issue, "/detail")
            ),
            Tone::Bad,
        ));
    }
    rows.push(info(
        tr!("settings.permissionJudge.status.device"),
        v(worker, "/device")
            .as_str()
            .map_or_else(|| "—".into(), device_label),
        Tone::Normal,
    ));
    rows.push(info(
        tr!("settings.permissionJudge.status.process"),
        v(worker, "/processId")
            .as_u64()
            .map_or_else(|| "—".into(), |id| id.to_string()),
        Tone::Normal,
    ));
    rows.push(info(
        tr!("settings.permissionJudge.status.memory"),
        tr!(
            "settings.permissionJudge.status.memoryValue",
            workingSet = optional_bytes(v(worker, "/workingSetBytes")),
            private = optional_bytes(v(worker, "/privateBytes")),
        ),
        Tone::Normal,
    ));
    rows.push(info(
        tr!("settings.permissionJudge.status.gpuMemory"),
        tr!(
            "settings.permissionJudge.status.gpuMemoryValue",
            dedicated = optional_bytes(v(worker, "/gpuDedicatedBytes")),
            shared = optional_bytes(v(worker, "/gpuSharedBytes")),
        ),
        Tone::Normal,
    ));
    rows.push(info(
        tr!("settings.permissionJudge.status.gpuUtilization"),
        v(worker, "/gpuUtilizationPercent")
            .as_f64()
            .map_or_else(|| "—".into(), |percent| format!("{percent:.0}%")),
        Tone::Normal,
    ));
    rows.push(info(
        tr!("settings.permissionJudge.status.startup"),
        tr!(
            "settings.permissionJudge.status.startupValue",
            verify = ms(v(worker, "/verifyMs")),
            load = ms(v(worker, "/loadMs")),
            warmup = ms(v(worker, "/warmupMs")),
        ),
        Tone::Normal,
    ));
    rows.push(info(
        tr!("settings.permissionJudge.status.latency"),
        tr!(
            "settings.permissionJudge.status.latencyValue",
            p50 = ms(v(worker, "/recentP50Ms")),
            p95 = ms(v(worker, "/recentP95Ms")),
            count = n(worker, "/evaluations"),
        ),
        Tone::Normal,
    ));
}

/// The cloud decision route, what leaves the machine, and a billed connection test.
/// 云端判定路由、离开本机的数据说明，以及一次计费的连接测试。
pub(super) fn cloud_rows(rows: &mut Vec<Row>, ai: &Value, saved: &Value) {
    rows.push(section(tr!("settings.permissionJudge.cloud.title")));
    rows.push(note(
        tr!("settings.permissionJudge.cloud.description"),
        Tone::Muted,
    ));
    rows.push(note(
        tr!("settings.permissionJudge.cloud.dataNotice"),
        Tone::Warn,
    ));
    let candidates: Vec<&Value> = a(ai, "/internalInferenceCandidates")
        .iter()
        .filter(|candidate| cloud_candidate(candidate))
        .collect();
    let selected = candidates
        .iter()
        .any(|candidate| v(candidate, "/route") == saved);
    if !saved.is_null() && !selected {
        rows.push(note(
            tr!("settings.permissionJudge.cloud.unavailable"),
            Tone::Warn,
        ));
    }
    if candidates.is_empty() {
        rows.push(note(
            tr!("settings.permissionJudge.cloud.noCandidate"),
            Tone::Muted,
        ));
    }
    rows.push(choice(
        tr!("settings.permissionJudge.cloud.route"),
        route_options(&candidates),
        saved.to_string(),
        Edit::Call(Setter::Build {
            ctx: Value::Null,
            build: select_cloud_route,
        }),
    ));
    if selected {
        rows.push(button(
            tr!("settings.permissionJudge.cloud.test"),
            confirm(
                tr!("settings.permissionJudge.cloud.test"),
                tr!("tui.field.paidTest"),
                tr!("tui.field.confirm"),
                call(settings::test_public_ai_inference_route(
                    JUDGE_PROFILE,
                    "decision_evaluate",
                )),
            ),
            Tone::Normal,
        ));
    }
}

/// The reviewing LLM route and its reasoning effort.
/// 复核 LLM 路由及其思考档位。
fn review_rows(rows: &mut Vec<Row>, ai: &Value) {
    rows.push(section(tr!("settings.permissionJudge.review.title")));
    rows.push(note(
        tr!("settings.permissionJudge.review.description"),
        Tone::Muted,
    ));
    let profile = v(ai, &format!("/inferenceProfiles/{REVIEW_PROFILE}"));
    let saved = v(profile, &format!("/llm_routes/{REVIEW_PURPOSE}"));
    let effort = v(profile, &format!("/llm_reasoning_efforts/{REVIEW_PURPOSE}"));
    let candidates: Vec<&Value> = a(ai, "/internalInferenceCandidates")
        .iter()
        .filter(|candidate| review_candidate(candidate))
        .collect();
    let selected = candidates
        .iter()
        .find(|candidate| v(candidate, "/route") == saved);
    if !saved.is_null() && selected.is_none() {
        rows.push(note(
            tr!("settings.permissionJudge.review.unavailable"),
            Tone::Warn,
        ));
    }
    if candidates.is_empty() {
        rows.push(note(
            tr!("settings.permissionJudge.review.noCandidate"),
            Tone::Muted,
        ));
    }
    rows.push(choice(
        tr!("settings.permissionJudge.review.route"),
        route_options(&candidates),
        saved.to_string(),
        Edit::Call(Setter::Build {
            ctx: json!({ "effort": effort, "candidates": candidates }),
            build: select_review_route,
        }),
    ));
    if let Some(candidate) = selected {
        let mut options = vec![(
            MODEL_DEFAULT_EFFORT.to_string(),
            tr!("settings.permissionJudge.review.reasoningDefault"),
        )];
        options.extend(
            a(candidate, "/capabilities/reasoning_efforts")
                .iter()
                .filter_map(Value::as_str)
                .map(|declared| (declared.to_string(), effort_label(declared))),
        );
        rows.push(choice(
            tr!("settings.permissionJudge.review.reasoning"),
            options,
            effort.as_str().unwrap_or(MODEL_DEFAULT_EFFORT).to_string(),
            Edit::Call(Setter::Build {
                ctx: saved.clone(),
                build: select_review_effort,
            }),
        ));
    }
}

/// Speed tests of one call or ten at once, and the latest result.
/// 一条或同时十条的速度测试，以及最近一次结果。
fn benchmark_rows(
    rows: &mut Vec<Row>,
    app: &App,
    snapshot: &Value,
    status: &Value,
    configured: bool,
) {
    // The saved backend decides what the test runs on, as in the WebUI.
    // 与 WebUI 一致，由已保存的后端决定测试对象。
    let saved_cloud = configured && s(snapshot, "/backend") == "cloud";
    let ready = saved_cloud
        || (s(snapshot, "/backend") != "cloud" && s(status, "/worker/phase") == "ready");
    rows.push(section(tr!("settings.permissionJudge.benchmark")));
    rows.push(note(
        if saved_cloud {
            tr!("settings.permissionJudge.benchmarkCloudHint")
        } else {
            tr!("settings.permissionJudge.benchmarkHint")
        },
        Tone::Muted,
    ));
    if ready {
        for (batch, label) in [
            (1, tr!("settings.permissionJudge.benchmarkSingle")),
            (10, tr!("settings.permissionJudge.benchmarkBatch")),
        ] {
            let run = call(judge::benchmark_permission_judge(batch));
            // Cloud tests are billed requests, so they ask first.
            // 云端测试是计费请求，因此先确认。
            let action = if saved_cloud {
                confirm(
                    label.clone(),
                    tr!("tui.field.paidTest"),
                    tr!("tui.field.confirm"),
                    run,
                )
            } else {
                run
            };
            rows.push(button(label, action, Tone::Normal));
        }
    }
    let result = app.slots.value(Slot::PermissionJudgeBenchmark);
    if !result.is_null() {
        let text = match v(result, "/device").as_str() {
            Some(device) => tr!(
                "settings.permissionJudge.benchmarkResult",
                batch = n(result, "/batchSize"),
                device = device_label(device),
                p50 = ms(v(result, "/p50Ms")),
                p95 = ms(v(result, "/p95Ms")),
                perItem = ms(v(result, "/perItemMs")),
                rate = format!(
                    "{:.1}",
                    v(result, "/itemsPerSecond").as_f64().unwrap_or(0.0)
                ),
            ),
            None => tr!(
                "settings.permissionJudge.benchmarkCloudResult",
                batch = n(result, "/batchSize"),
                model = s(result, "/modelId"),
                total = ms(v(result, "/p50Ms")),
                perItem = ms(v(result, "/perItemMs")),
                rate = format!(
                    "{:.1}",
                    v(result, "/itemsPerSecond").as_f64().unwrap_or(0.0)
                ),
            ),
        };
        rows.push(note(text, Tone::Normal));
    }
    if let Some(error) = app.slot_errors.get(&Slot::PermissionJudgeBenchmark) {
        rows.push(note(
            format!(
                "{} {error}",
                tr!("settings.permissionJudge.benchmarkFailed")
            ),
            Tone::Bad,
        ));
    }
}

/// Registered models with switch and remove actions, rollback, and registration forms.
/// 已登记模型及其切换、移除操作、回退，以及登记表单。
fn model_rows(rows: &mut Vec<Row>, app: &App, snapshot: &Value) {
    rows.push(section(tr!("settings.permissionJudge.models")));
    rows.push(note(
        tr!("settings.permissionJudge.modelsHint"),
        Tone::Muted,
    ));
    let models = a(snapshot, "/models");
    if models.is_empty() {
        rows.push(note(
            tr!("settings.permissionJudge.modelsEmpty"),
            Tone::Muted,
        ));
    }
    let active = v(snapshot, "/activeModelId");
    for model in models {
        let id = s(model, "/id");
        let in_use = v(model, "/id") == active;
        // Official package family name, else its id.
        // 官方模型包家族名称，否则为其标识。
        let package = v(model, "/knownPackageId").as_str().map(|package| {
            a(snapshot, "/knownPackages")
                .iter()
                .find(|known| s(known, "/id") == package)
                .map_or_else(|| package.to_string(), |known| s(known, "/displayName"))
        });
        let kind = match package {
            Some(name) => tr!("settings.permissionJudge.modelOfficial", name = name),
            None => tr!("settings.permissionJudge.modelCustom"),
        };
        let mut row = item(&id, s(model, "/name"), kind).lines(vec![
            tr!(
                "settings.permissionJudge.modelMeta",
                policy = s(model, "/policyVersion"),
                precision = s(model, "/precision"),
                size = format_bytes(v(model, "/totalBytes").as_f64().unwrap_or(0.0)),
            ),
            s(model, "/path"),
        ]);
        if in_use {
            row = row.badge(tr!("settings.permissionJudge.modelActive"), Tone::Good);
        } else {
            row = row.actions(vec![
                action(
                    tr!("settings.permissionJudge.activate"),
                    call(judge::activate_permission_judge_model(&id)),
                    Tone::Accent,
                ),
                action(
                    tr!("settings.permissionJudge.remove"),
                    confirm(
                        tr!("settings.permissionJudge.remove"),
                        s(model, "/path"),
                        tr!("settings.permissionJudge.remove"),
                        call(judge::remove_permission_judge_model(&id)),
                    ),
                    Tone::Bad,
                ),
            ]);
        }
        rows.push(row);
    }
    let previous = v(snapshot, "/previousModelId");
    if let Some(model) = models.iter().find(|model| v(model, "/id") == previous) {
        rows.push(button(
            tr!(
                "settings.permissionJudge.rollback",
                name = s(model, "/name")
            ),
            call(judge::rollback_permission_judge_model()),
            Tone::Normal,
        ));
    }
    register_rows(rows, app, snapshot);
}

/// Labels a package check failure code; unknown codes keep the code in a generic sentence.
/// 为模型包检查失败代码生成标签；未知代码放入通用句子中保留。
fn package_issue_label(code: &str) -> String {
    match code {
        "invalid_path" => tr!("settings.permissionJudge.packageIssue.invalidPath"),
        "unreadable" => tr!("settings.permissionJudge.packageIssue.unreadable"),
        "malformed" => tr!("settings.permissionJudge.packageIssue.malformed"),
        "unsupported" => tr!("settings.permissionJudge.packageIssue.unsupported"),
        "missing_file" => tr!("settings.permissionJudge.packageIssue.missingFile"),
        "unsafe_entry" => tr!("settings.permissionJudge.packageIssue.unsafeEntry"),
        "size_mismatch" => tr!("settings.permissionJudge.packageIssue.sizeMismatch"),
        "digest_mismatch" => tr!("settings.permissionJudge.packageIssue.digestMismatch"),
        other => tr!(
            "settings.permissionJudge.packageIssue.unknown",
            code = other
        ),
    }
}

/// The folder check form and its latest result, then the two registration forms: an official
/// package folder, or a custom model folder.
/// 文件夹检查表单及其最近结果，然后是两个登记表单：官方模型包文件夹或自定义模型文件夹。
pub(super) fn register_rows(rows: &mut Vec<Row>, app: &App, snapshot: &Value) {
    rows.push(button(
        tr!("settings.permissionJudge.packageCheck"),
        form(
            json!({ "path": "" }),
            vec![field(
                tr!("settings.permissionJudge.registerPath"),
                "/path",
                false,
                false,
            )],
            judge::inspect_permission_judge_package,
        ),
        Tone::Normal,
    ));
    let slot = Slot::PermissionJudgePackageInspection;
    // The checked folder comes from the request that produced the result.
    // 被检查的文件夹来自产生该结果的请求。
    let checked = app
        .slots
        .loader(slot)
        .map(|call| {
            call.param("/request/path")
                .as_str()
                .unwrap_or("")
                .to_string()
        })
        .unwrap_or_default();
    let inspection = app.slots.value(slot);
    let issue = v(inspection, "/issue");
    let package = v(inspection, "/package");
    if !issue.is_null() {
        rows.push(info(
            package_issue_label(&s(issue, "/code")),
            format!("{checked}\n{}", s(issue, "/detail")),
            Tone::Bad,
        ));
    } else if !package.is_null() {
        let known = v(package, "/knownPackageId").as_str().map(|package| {
            a(snapshot, "/knownPackages")
                .iter()
                .find(|known| s(known, "/id") == package)
                .map_or_else(|| package.to_string(), |known| s(known, "/displayName"))
        });
        let kind = match &known {
            Some(name) => tr!("settings.permissionJudge.modelOfficial", name = name),
            None => tr!("settings.permissionJudge.modelCustom"),
        };
        rows.push(info(
            format!("{} · {kind}", tr!("settings.permissionJudge.packageValid")),
            format!(
                "{checked}\n{}",
                tr!(
                    "settings.permissionJudge.modelMeta",
                    policy = s(package, "/policyVersion"),
                    precision = s(package, "/precision"),
                    size = format_bytes(v(package, "/totalBytes").as_f64().unwrap_or(0.0)),
                )
            ),
            Tone::Good,
        ));
        if b(package, "/registered") {
            rows.push(note(
                tr!("settings.permissionJudge.packageRegistered"),
                Tone::Muted,
            ));
        } else if known.is_none() {
            rows.push(note(
                tr!("settings.permissionJudge.packageCustomHint"),
                Tone::Muted,
            ));
        }
    }
    if let Some(error) = app.slot_errors.get(&slot) {
        rows.push(note(
            format!(
                "{} {error}",
                tr!("settings.permissionJudge.packageCheckFailed")
            ),
            Tone::Bad,
        ));
    }
    for custom in [false, true] {
        let label = if custom {
            format!(
                "{} · {}",
                tr!("settings.permissionJudge.register"),
                tr!("settings.permissionJudge.registerCustom")
            )
        } else {
            tr!("settings.permissionJudge.register")
        };
        rows.push(button(
            label,
            form(
                json!({ "path": "", "name": "", "custom": custom }),
                vec![
                    field(
                        tr!("settings.permissionJudge.registerPath"),
                        "/path",
                        false,
                        false,
                    ),
                    field(
                        tr!("settings.permissionJudge.registerName"),
                        "/name",
                        false,
                        true,
                    ),
                ],
                judge::register_permission_judge_model,
            ),
            Tone::Accent,
        ));
    }
}

/// The driver, detected CUDA folders, one-click installs and official links.
/// 驱动、探测到的 CUDA 文件夹、一键安装与官方链接。
fn cuda_rows(rows: &mut Vec<Row>, snapshot: &Value, status: &Value, base: &Value) {
    let cuda = v(snapshot, "/cuda");
    rows.push(section(tr!("settings.permissionJudge.cuda")));
    if !b(cuda, "/supported") {
        rows.push(note(
            tr!("settings.permissionJudge.cuda.unsupported"),
            Tone::Muted,
        ));
        return;
    }
    rows.push(note(tr!("settings.permissionJudge.cudaHint"), Tone::Muted));
    let driver = v(cuda, "/driver");
    let mut driver_text = if driver.is_null() {
        tr!("settings.permissionJudge.cuda.driverMissing")
    } else {
        tr!(
            "settings.permissionJudge.cuda.driverValue",
            version = s(driver, "/version"),
            gpus = a(driver, "/gpus")
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", "),
        )
    };
    if let (Some(major), Some(cuda_major)) = (
        v(cuda, "/minimumDriverMajor").as_u64(),
        v(cuda, "/cudaMajor").as_u64(),
    ) {
        driver_text = format!(
            "{driver_text} · {}",
            tr!(
                "settings.permissionJudge.cuda.driverMinimum",
                major = major,
                cuda = cuda_major
            )
        );
    }
    rows.push(info(
        tr!("settings.permissionJudge.cuda.driver"),
        driver_text,
        Tone::Normal,
    ));
    rows.push(info(
        tr!("settings.permissionJudge.cuda.onnxruntime"),
        v(cuda, "/onnxruntimeDirectory").as_str().map_or_else(
            || tr!("settings.permissionJudge.cuda.notInstalled"),
            str::to_string,
        ),
        Tone::Normal,
    ));
    rows.push(info(
        tr!("settings.permissionJudge.cuda.libraries"),
        tr!(
            "settings.permissionJudge.cuda.required",
            libraries = a(cuda, "/requiredLibraries")
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", "),
        ),
        Tone::Normal,
    ));
    let locked = v(cuda, "/lockedDirectory");
    for detected in a(cuda, "/detectedDirectories") {
        let directory = s(detected, "/directory");
        let row = item(
            &format!("cuda-{directory}"),
            source_label(&s(detected, "/source")),
            directory.clone(),
        );
        rows.push(if v(detected, "/directory") == locked {
            row.badge(tr!("settings.permissionJudge.cuda.locked"), Tone::Good)
        } else {
            let mut request = base.clone();
            request["cudaLibraryDirectory"] = json!(directory);
            row.actions(vec![action(
                tr!("settings.permissionJudge.cuda.useDirectory"),
                call(judge::save_permission_judge_settings(request)),
                Tone::Normal,
            )])
        });
    }
    rows.push(text(
        tr!("settings.permissionJudge.cuda.directory"),
        s(cuda, "/lockedDirectory"),
        Edit::Call(Setter::Build {
            ctx: base.clone(),
            build: set_cuda_directory,
        }),
    ));
    let missing = a(cuda, "/lockedDirectoryMissing");
    if !locked.is_null() && !missing.is_empty() {
        rows.push(note(
            tr!(
                "settings.permissionJudge.cuda.lockedMissing",
                libraries = missing
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
            Tone::Warn,
        ));
    }
    let install = v(status, "/cudaInstall");
    let running = b(install, "/running");
    rows.push(section(tr!("settings.permissionJudge.cuda.install")));
    for archive in a(cuda, "/archives") {
        let component = s(archive, "/component");
        let row = item(
            &format!("archive-{component}"),
            component_label(&component),
            format!(
                "{} · {}",
                s(archive, "/name"),
                format_bytes(v(archive, "/size").as_f64().unwrap_or(0.0))
            ),
        );
        rows.push(if running {
            row
        } else {
            row.actions(vec![action(
                tr!("settings.permissionJudge.cuda.installAction"),
                call(judge::install_permission_judge_cuda(&component)),
                Tone::Accent,
            )])
        });
    }
    if !install.is_null() {
        let component = component_label(&s(install, "/component"));
        if running {
            let progress = match (
                v(install, "/completedBytes").as_f64(),
                v(install, "/totalBytes")
                    .as_f64()
                    .filter(|total| *total > 0.0),
            ) {
                (Some(done), Some(total)) => {
                    format!("{} / {}", format_bytes(done), format_bytes(total))
                }
                _ => String::new(),
            };
            rows.push(note(
                tr!(
                    "settings.permissionJudge.cuda.installing",
                    component = component,
                    phase = v(install, "/phase")
                        .as_str()
                        .map_or_else(|| "…".into(), install_phase_label),
                    progress = progress,
                ),
                Tone::Info,
            ));
        } else if let Some(error) = v(install, "/error").as_str() {
            rows.push(note(
                format!(
                    "{} {error}",
                    tr!("settings.permissionJudge.cuda.installFailed")
                ),
                Tone::Bad,
            ));
        } else {
            rows.push(note(
                tr!(
                    "settings.permissionJudge.cuda.installed",
                    component = component,
                    directory = s(install, "/directory"),
                ),
                Tone::Good,
            ));
        }
    }
    let links = a(cuda, "/links");
    if !links.is_empty() {
        rows.push(
            item("cuda-links", tr!("settings.permissionJudge.cuda.links"), "").actions(
                links
                    .iter()
                    .map(|link| {
                        action(
                            link_label(&s(link, "/id")),
                            call(crate::service_api::project_actions::open_external_url(&s(
                                link, "/url",
                            ))),
                            Tone::Normal,
                        )
                    })
                    .collect(),
            ),
        );
    }
}

// Records per page, as in the WebUI decision log page.
// 每页记录数，与 WebUI 判定记录页一致。
const RECORDS_PAGE_SIZE: u32 = 50;
// Judge result filter values; "failed" keeps records the judge could not score.
// 判定结果筛选值；"failed" 保留判定器无法评分的记录。
const RECORD_RESULTS: [&str; 4] = ["allow", "review", "block", "failed"];
// Choice value that keeps every judge result or workspace.
// 保留全部判定结果或工作区的选择值。
const RECORDS_ALL: &str = "";

/// The complete records request: `filters` is the shown request (or null) and `before` the
/// cursor; any filter change starts again from the newest record.
/// 完整的记录请求：`filters` 为当前显示的请求（或 null），`before` 为游标；任何筛选变化都从最新记录重新开始。
pub(super) fn records_request(filters: &Value, before: Option<&str>) -> Value {
    json!({
        "label": v(filters, "/label"),
        "workspace": v(filters, "/workspace"),
        "markedOnly": b(filters, "/markedOnly"),
        "before": before,
        "limit": RECORDS_PAGE_SIZE,
    })
}

/// Replaces one filter of the shown request and reads the newest page.
/// 替换当前请求中的一个筛选条件并读取最新一页。
fn set_record_filter(filters: &Value, field: &str, value: Value) -> ServiceCall {
    let mut next = filters.clone();
    next[field] = value;
    judge::load_permission_judge_records(records_request(&next, None))
}

fn set_record_result(filters: &Value, value: &SettingValue) -> Option<ServiceCall> {
    let result = value.as_str();
    Some(set_record_filter(
        filters,
        "label",
        if result == RECORDS_ALL {
            Value::Null
        } else {
            json!(result)
        },
    ))
}

fn set_record_workspace(filters: &Value, value: &SettingValue) -> Option<ServiceCall> {
    let workspace = value.as_str();
    Some(set_record_filter(
        filters,
        "workspace",
        if workspace == RECORDS_ALL {
            Value::Null
        } else {
            json!(workspace)
        },
    ))
}

fn set_record_marked_only(filters: &Value, value: &SettingValue) -> Option<ServiceCall> {
    Some(set_record_filter(
        filters,
        "markedOnly",
        json!(value.as_bool()),
    ))
}

/// Labels a judge result filter value.
/// 为判定结果筛选值生成标签。
fn record_result_label(code: &str) -> String {
    match code {
        "allow" => tr!("settings.permissionJudge.records.label.allow"),
        "review" => tr!("settings.permissionJudge.records.label.review"),
        "block" => tr!("settings.permissionJudge.records.label.block"),
        "failed" => tr!("settings.permissionJudge.records.label.failed"),
        other => other.to_string(),
    }
}

/// Labels the layer that decided a recorded call.
/// 为决定所记录调用的层生成标签。
fn record_outcome_label(code: &str) -> String {
    match code {
        "judge_allowed" => tr!("settings.permissionJudge.records.outcome.judgeAllowed"),
        "review_allowed" => tr!("settings.permissionJudge.records.outcome.reviewAllowed"),
        "review_denied" => tr!("settings.permissionJudge.records.outcome.reviewDenied"),
        "user_allowed" => tr!("settings.permissionJudge.records.outcome.userAllowed"),
        "user_denied" => tr!("settings.permissionJudge.records.outcome.userDenied"),
        "failed" => tr!("settings.permissionJudge.records.outcome.failed"),
        "unattended_denied" => tr!("settings.permissionJudge.records.outcome.unattendedDenied"),
        "approval_expired" => tr!("settings.permissionJudge.records.outcome.approvalExpired"),
        other => other.to_string(),
    }
}

/// Mirrors the WebUI `callSummary`: the call's command or path, else the tool name.
/// 镜像 WebUI `callSummary`：调用的命令或路径，否则为工具名称。
fn record_summary(record: &Value) -> String {
    let arguments: Value = serde_json::from_str(&s(record, "/arguments")).unwrap_or(Value::Null);
    ["command", "path", "destination"]
        .iter()
        .find_map(|key| {
            v(&arguments, &format!("/{key}"))
                .as_str()
                .filter(|text| !text.is_empty())
        })
        .map_or_else(|| s(record, "/toolName"), str::to_string)
}

/// The decision log: filters, one page of records with marks, paging and clearing, mirroring the
/// WebUI decision log page. Sample export downloads a file and stays in the WebUI.
/// 判定记录：筛选、一页带标记的记录、翻页与清除，镜像 WebUI 判定记录页。样本导出会下载文件，仅在 WebUI 提供。
fn records_rows(rows: &mut Vec<Row>, app: &App, base: &Value) {
    rows.push(section(tr!("settings.permissionJudge.records.title")));
    rows.push(
        toggle(
            tr!("settings.permissionJudge.records.enabled"),
            b(base, "/recordDecisions"),
            dto(
                base.clone(),
                "/recordDecisions",
                judge::save_permission_judge_settings,
            ),
        )
        .detail(tr!("settings.permissionJudge.records.enabledHint")),
    );
    let page = app.slots.value(Slot::PermissionJudgeRecords);
    if let Some(error) = app.slot_errors.get(&Slot::PermissionJudgeRecords) {
        rows.push(note(
            format!(
                "{} {error}",
                tr!("settings.permissionJudge.records.loadFailed")
            ),
            Tone::Bad,
        ));
    }
    if page.is_null() {
        return;
    }
    let filters = app
        .slots
        .loader(Slot::PermissionJudgeRecords)
        .map_or(Value::Null, |loader| v(&loader.params, "/request").clone());
    rows.push(note(
        tr!(
            "settings.permissionJudge.records.hint",
            records = n(page, "/retentionRecords"),
            days = n(page, "/retentionDays"),
        ),
        Tone::Muted,
    ));
    rows.push(info(
        tr!(
            "settings.permissionJudge.records.count",
            count = n(page, "/total")
        ),
        "",
        Tone::Normal,
    ));
    let mut results = vec![(
        RECORDS_ALL.to_string(),
        tr!("settings.permissionJudge.records.all"),
    )];
    results.extend(RECORD_RESULTS.map(|code| (code.to_string(), record_result_label(code))));
    rows.push(choice(
        tr!("settings.permissionJudge.records.filterResult"),
        results,
        v(&filters, "/label")
            .as_str()
            .unwrap_or(RECORDS_ALL)
            .to_string(),
        Edit::Call(Setter::Build {
            ctx: filters.clone(),
            build: set_record_result,
        }),
    ));
    let workspaces = a(page, "/workspaces");
    if !workspaces.is_empty() {
        let mut options = vec![(
            RECORDS_ALL.to_string(),
            tr!("settings.permissionJudge.records.allWorkspaces"),
        )];
        options.extend(
            workspaces
                .iter()
                .filter_map(Value::as_str)
                .map(|workspace| (workspace.to_string(), workspace.to_string())),
        );
        rows.push(choice(
            tr!("settings.permissionJudge.records.detail.workspace"),
            options,
            v(&filters, "/workspace")
                .as_str()
                .unwrap_or(RECORDS_ALL)
                .to_string(),
            Edit::Call(Setter::Build {
                ctx: filters.clone(),
                build: set_record_workspace,
            }),
        ));
    }
    rows.push(toggle(
        tr!("settings.permissionJudge.records.markedOnly"),
        b(&filters, "/markedOnly"),
        Edit::Call(Setter::Build {
            ctx: filters.clone(),
            build: set_record_marked_only,
        }),
    ));

    let records = a(page, "/records");
    if records.is_empty() {
        rows.push(note(
            tr!("settings.permissionJudge.records.empty"),
            Tone::Muted,
        ));
    }
    for record in records {
        rows.push(record_row(record));
    }
    let mut paging = Vec::new();
    if !v(&filters, "/before").is_null() {
        paging.push(action(
            tr!("tui.judgeRecords.newest"),
            call(judge::load_permission_judge_records(records_request(
                &filters, None,
            ))),
            Tone::Normal,
        ));
    }
    if b(page, "/hasMore")
        && let Some(last) = records.last()
    {
        paging.push(action(
            tr!("tui.judgeRecords.older"),
            call(judge::load_permission_judge_records(records_request(
                &filters,
                Some(&s(last, "/id")),
            ))),
            Tone::Normal,
        ));
    }
    if !paging.is_empty() {
        rows.push(item("judge-records-paging", tr!("tui.knowledge.page"), "").actions(paging));
    }
    if n(page, "/total") > 0 {
        let clear = tr!("settings.permissionJudge.records.clear");
        rows.push(button(
            clear.clone(),
            confirm(
                tr!("settings.permissionJudge.records.clearTitle"),
                tr!(
                    "settings.permissionJudge.records.clearDescription",
                    count = n(page, "/total"),
                ),
                tr!("settings.permissionJudge.records.clearConfirm"),
                call(judge::clear_permission_judge_records()),
            ),
            Tone::Bad,
        ));
    }
}

/// One record: what was called, how each layer decided, and marks when the judge scored it.
/// 一条记录：调用了什么、各层如何决定；判定器评过分时提供标记。
fn record_row(record: &Value) -> Row {
    let id = s(record, "/id");
    let judge_result = match v(record, "/judge") {
        Value::Null => None,
        judge => Some(v(judge, "/label").as_str().unwrap_or("failed").to_string()),
    };
    let detail = [
        super::settings::utc_time(n(record, "/judgedAtMs")),
        record_outcome_label(&s(record, "/outcome")),
    ]
    .join(" · ");
    let mut lines = vec![
        [
            Some(s(record, "/toolName")),
            v(record, "/workspaceRoot").as_str().map(str::to_string),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · "),
    ];
    let trace = v(record, "/judge/trace");
    if !trace.is_null() {
        lines.push(format!(
            "{} {} · {} {}",
            tr!("settings.permissionJudge.records.detail.model"),
            s(trace, "/model"),
            tr!("settings.permissionJudge.records.detail.judgeTime"),
            ms(v(record, "/judge/elapsedMs")),
        ));
    }
    if let Some(error) = v(record, "/judge/error").as_str() {
        lines.push(format!(
            "{} {error}",
            tr!("settings.permissionJudge.records.detail.error")
        ));
    }
    if !v(record, "/review").is_null() {
        let reason = v(record, "/review/reason")
            .as_str()
            .or_else(|| v(record, "/review/error").as_str())
            .unwrap_or("—");
        lines.push(format!(
            "{} {reason} · {}",
            tr!("settings.permissionJudge.records.detail.review"),
            ms(v(record, "/review/elapsedMs")),
        ));
    }
    let mut row = item(&id, record_summary(record), detail).lines(lines);
    if let Some(result) = &judge_result {
        let tone = match result.as_str() {
            "allow" => Tone::Good,
            "review" => Tone::Warn,
            "block" => Tone::Bad,
            _ => Tone::Muted,
        };
        row = row.badge(record_result_label(result), tone);
    }
    let correction = v(record, "/correction").as_str();
    if let Some(correction) = correction {
        row = row.badge(
            tr!(
                "settings.permissionJudge.records.marked",
                label = record_result_label(correction),
            ),
            Tone::Accent,
        );
    }
    if trace.is_null() {
        return row;
    }
    row.actions(
        RECORD_RESULTS[..3]
            .iter()
            .map(|label| {
                // Choosing the current mark again clears it, as in the WebUI.
                // 与 WebUI 一致，再次选择当前标记会清除它。
                let next = (correction != Some(*label)).then_some(*label);
                action(
                    record_result_label(label),
                    call(judge::mark_permission_judge_record(&id, next)),
                    if correction == Some(*label) {
                        Tone::Accent
                    } else {
                        Tone::Normal
                    },
                )
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // Sizes render as the WebUI's `formatBytes` does.
    // 大小的渲染与 WebUI 的 `formatBytes` 一致。
    #[test]
    fn sizes_follow_the_webui_format() {
        assert_eq!(format_bytes(512.0), "512 B");
        assert_eq!(format_bytes(1024.0), "1 KB");
        assert_eq!(format_bytes(1536.0), "1.5 KB");
        assert_eq!(format_bytes(3.0 * 1024.0 * 1024.0 * 1024.0), "3 GB");
    }

    // A saved review effort survives a route change only when the new model declares it.
    // 仅当新模型声明已保存档位时，路由变更才保留该档位。
    #[test]
    fn review_route_keeps_only_a_declared_effort() {
        let keeps =
            json!({"route": {"model_id": "a"}, "capabilities": {"reasoning_efforts": ["high"]}});
        let lacks =
            json!({"route": {"model_id": "b"}, "capabilities": {"reasoning_efforts": ["low"]}});
        let context = json!({ "effort": "high", "candidates": [keeps, lacks] });
        let kept = select_review_route(
            &context,
            &SettingValue::Choice(json!({"model_id": "a"}).to_string()),
        )
        .unwrap();
        assert_eq!(kept.param("/request/reasoningEffort"), "high");
        assert_eq!(kept.param("/request/purposeId"), REVIEW_PURPOSE);
        let dropped = select_review_route(
            &context,
            &SettingValue::Choice(json!({"model_id": "b"}).to_string()),
        )
        .unwrap();
        assert!(dropped.param("/request/reasoningEffort").is_null());
        let cleared = select_review_route(&context, &SettingValue::Choice("null".into())).unwrap();
        assert!(cleared.param("/request/route").is_null());
        assert!(cleared.param("/request/reasoningEffort").is_null());
    }

    // Every edit sends the complete save request, keeping the other saved choices.
    // 每次编辑都发送完整保存请求，并保留其他已保存选择。
    #[test]
    fn edits_send_the_complete_save_request() {
        let snapshot = json!({
            "backend": null, "device": null, "recommendedDevice": "webgpu",
            "llmMayAllowBlocked": true, "recordDecisions": true, "cuda": {"lockedDirectory": "D:/cuda"},
            "approvalTimeoutSeconds": 90,
        });
        let base = save_request(&snapshot);
        assert_eq!(
            base,
            json!({"backend": null, "device": "webgpu", "cudaLibraryDirectory": "D:/cuda",
                "llmMayAllowBlocked": true, "recordDecisions": true, "approvalTimeoutSeconds": 90})
        );
        // Other edits never configure the judge as a side effect.
        // 其他编辑不会顺带配置判定器。
        let cleared = set_cuda_directory(&base, &SettingValue::Text("  ".into())).unwrap();
        assert!(cleared.param("/request/cudaLibraryDirectory").is_null());
        assert!(cleared.param("/request/backend").is_null());
    }
}
