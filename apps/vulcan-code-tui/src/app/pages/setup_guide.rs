// Setup guide page: its first step is the permission judge, answered by a local model, a cloud
// judge or not now, mirroring the WebUI setup guide dialog. Only a backend's confirm button saves
// the system settings; not now and leaving the page write nothing, so an unset judge stays unset.
// 初始化引导页：第一步是权限判定，可选本机模型、云端判定或暂不启用，镜像 WebUI 初始化引导对话框。
// 只有后端的确认按钮会保存系统设置；暂不启用与离开页面不写入任何内容，未设定的判定保持未设定。

use super::permission_judge::{
    JUDGE_PROFILE, cloud_rows, device_label, phase_label, reason_label, register_rows,
};
use super::*;
use crate::service_api::permission_judge as judge;

/// The save request that configures the judge on `backend`; the other saved fields stay as they are.
/// 将判定器配置为 `backend` 的保存请求；其他已保存字段保持不变。
fn choice_request(snapshot: &Value, backend: &str) -> Value {
    // Saved device, else the recommendation.
    // 已保存设备，否则为推荐设备。
    let device = match v(snapshot, "/device") {
        Value::Null => v(snapshot, "/recommendedDevice").clone(),
        device => device.clone(),
    };
    json!({
        "backend": backend,
        "device": device,
        "cudaLibraryDirectory": v(snapshot, "/cuda/lockedDirectory"),
        "llmMayAllowBlocked": b(snapshot, "/llmMayAllowBlocked"),
        "recordDecisions": b(snapshot, "/recordDecisions"),
    })
}

/// The confirm button of one choice, or the reason it cannot be confirmed yet.
/// 单个选择的确认按钮，或其暂时无法确认的原因。
fn confirm_row(label: String, request: Option<Value>, missing: String) -> Row {
    match request {
        Some(request) => button(
            label,
            call(judge::save_permission_judge_settings(request)),
            Tone::Accent,
        ),
        None => info(label, missing, Tone::Warn),
    }
}

/// Builds the setup guide page from the judge settings, the live status and the public AI routes.
/// 根据判定设置、实时状态与公共 AI 路由构建初始化引导页。
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
    let cloud_route = v(ai, &format!("/inferenceProfiles/{JUDGE_PROFILE}/decision"));

    let mut rows = vec![note(tr!("setupGuide.judge.intro"), Tone::Muted)];
    local_rows(&mut rows, app, snapshot, status);

    rows.push(section(tr!("setupGuide.cloud.title")));
    rows.push(note(tr!("setupGuide.cloud.description"), Tone::Muted));
    cloud_rows(&mut rows, ai, cloud_route);
    rows.push(button(
        tr!("setupGuide.cloud.providers"),
        UiAction::GoPage(PageId::Providers),
        Tone::Normal,
    ));
    rows.push(confirm_row(
        tr!("setupGuide.cloud.confirm"),
        (!cloud_route.is_null()).then(|| choice_request(snapshot, "cloud")),
        tr!("settings.permissionJudge.needsCloudRoute"),
    ));

    rows.push(section(tr!("setupGuide.off.title")));
    rows.push(note(tr!("setupGuide.off.description"), Tone::Muted));
    rows.push(button(
        tr!("setupGuide.off.confirm"),
        UiAction::Area(Area::Chat),
        Tone::Normal,
    ));
    rows
}

/// The local choice: detected devices, registered models with the one in use and its phase, the
/// registration forms and the confirm button, which waits for a model in use.
/// 本机选项：探测到的设备、已登记模型及使用中模型的阶段、登记表单与确认按钮；确认需等待使用中模型。
fn local_rows(rows: &mut Vec<Row>, app: &App, snapshot: &Value, status: &Value) {
    rows.push(section(tr!("setupGuide.local.title")));
    rows.push(note(tr!("setupGuide.local.description"), Tone::Muted));
    let devices = a(snapshot, "/devices")
        .iter()
        .map(|device| {
            let label = device_label(&s(device, "/device"));
            match v(device, "/unavailableReason").as_str() {
                Some(reason) => format!("{label} · {}", reason_label(reason)),
                None => label,
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    rows.push(note(
        tr!("setupGuide.local.devices", devices = devices),
        Tone::Normal,
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
    let models = a(snapshot, "/models");
    if models.is_empty() {
        rows.push(note(tr!("setupGuide.local.howTo"), Tone::Muted));
    }
    let active = v(snapshot, "/activeModelId");
    let worker = v(status, "/worker");
    for model in models {
        let id = s(model, "/id");
        let row = item(&id, s(model, "/name"), s(model, "/path"));
        rows.push(if v(model, "/id") == active {
            let (phase, tone) = phase_label(&s(worker, "/phase"));
            row.badge(
                format!("{} · {phase}", tr!("settings.permissionJudge.modelActive")),
                tone,
            )
        } else {
            row.actions(vec![action(
                tr!("settings.permissionJudge.activate"),
                call(judge::activate_permission_judge_model(&id)),
                Tone::Accent,
            )])
        });
    }
    if s(worker, "/phase") == "failed"
        && let Some(error) = v(worker, "/error").as_str()
    {
        rows.push(info(
            tr!("settings.permissionJudge.status.loadFailed"),
            error,
            Tone::Bad,
        ));
    }
    register_rows(rows, app, snapshot);
    let in_use = models.iter().any(|model| v(model, "/id") == active);
    rows.push(confirm_row(
        tr!("setupGuide.local.confirm"),
        in_use.then(|| choice_request(snapshot, "local")),
        tr!("settings.permissionJudge.needsModel"),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An unset judge with a recommended device and a saved CUDA folder.
    /// 未设定的判定，带推荐设备与已保存的 CUDA 文件夹。
    fn unset() -> Value {
        json!({
            "backend": null, "device": null, "recommendedDevice": "cpu",
            "llmMayAllowBlocked": true, "recordDecisions": true, "cuda": { "lockedDirectory": "D:/cuda" },
        })
    }

    // Choosing a backend saves it with the recommended device and keeps the other saved fields.
    // 选择后端时以推荐设备保存，并保留其他已保存字段。
    #[test]
    fn choosing_a_backend_saves_the_recommended_device() {
        assert_eq!(
            choice_request(&unset(), "local"),
            json!({
                "backend": "local", "device": "cpu",
                "cudaLibraryDirectory": "D:/cuda", "llmMayAllowBlocked": true,
                "recordDecisions": true,
            })
        );
    }
}
