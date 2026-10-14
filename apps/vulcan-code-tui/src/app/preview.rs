// Confirmation of two-step service actions: a preview result opens a dialog whose accept sends
// the second step that `service_api::confirm_preview` builds, as the WebUI dialogs do.
// 两步服务操作的确认：预检结果打开对话框，接受后发送由 `service_api::confirm_preview` 构建的
// 第二步，与 WebUI 对话框一致。

use serde_json::Value;

use super::App;
use super::action::{UiAction, confirm};
use crate::backend::Command;
use crate::model::NoticeSeverity;
use crate::service_api::{CallKind, Preview, ServiceCall, Slot, confirm_preview};
use crate::tr;

/// Builds editable fields from a validated preview and its frozen second-step call.
/// 根据已验证的预检及冻结的第二步调用构建可编辑字段。
/// `None` rejects missing schema evidence; an empty list means no further input is required.
/// `None` 拒绝缺少协议证据的结果；空列表表示无需继续输入。
fn input_fields(
    preview: Preview,
    result: &Value,
    next: &ServiceCall,
) -> Option<Vec<super::form::FormField>> {
    use super::form::FormField;
    match preview {
        Preview::KnowledgeImport => Some(vec![FormField {
            title: tr!("tui.preview.selectedFiles"),
            pointer: "/request/selectedFileKeys".to_string(),
            secret: false,
            optional: true,
            parse: |input| {
                Some(Value::Array(
                    input
                        .lines()
                        .map(str::trim)
                        .filter(|key| !key.is_empty())
                        .map(|key| Value::String(key.to_string()))
                        .collect(),
                ))
            },
            validate: ready_subset,
        }]),
        Preview::KnowledgeDisplay => Some(vec![FormField {
            title: tr!("tui.preview.selectedFiles"),
            pointer: "/confirmation/selectedFileKeys".to_string(),
            secret: false,
            optional: true,
            parse: |input| {
                Some(Value::Array(
                    input
                        .lines()
                        .map(str::trim)
                        .filter(|key| !key.is_empty())
                        .map(|key| Value::String(key.to_string()))
                        .collect(),
                ))
            },
            validate: ready_display_subset,
        }]),
        Preview::PluginInstall => next
            .param("/initialSecrets")
            .as_array()?
            .iter()
            .enumerate()
            .map(|(index, requirement)| {
                Some(FormField {
                    title: format!(
                        "{} · {} / {}",
                        tr!("tui.field.secret"),
                        requirement.get("serverId")?.as_str()?,
                        requirement.get("secretKey")?.as_str()?
                    ),
                    pointer: format!("/initialSecrets/{index}/value"),
                    secret: true,
                    optional: false,
                    parse: |input| (!input.is_empty()).then(|| Value::String(input.to_string())),
                    validate: |_, _| true,
                })
            })
            .collect(),
        Preview::VceEmbedding => result
            .get("projects")?
            .as_array()?
            .iter()
            .enumerate()
            .map(|(index, project)| {
                // A hard-rebuild reason makes compatibility unavailable in the upstream UI.
                // 上游界面在存在强制重建原因时不允许选择兼容。
                let hard_rebuild = project.pointer("/impact/hardRebuildReason")?;
                Some(FormField {
                    title: if hard_rebuild.is_null() {
                        tr!(
                            "tui.preview.projectDecision",
                            project = project.get("projectName")?.as_str()?
                        )
                    } else {
                        tr!(
                            "tui.preview.projectRebuild",
                            project = project.get("projectName")?.as_str()?
                        )
                    },
                    pointer: format!(
                        "/request/vceEmbeddingConfirmation/decisions/{index}/decision"
                    ),
                    secret: false,
                    optional: false,
                    parse: if hard_rebuild.is_null() {
                        |input| {
                            matches!(input, "compatible" | "rebuild")
                                .then(|| Value::String(input.to_string()))
                        }
                    } else {
                        |input| (input == "rebuild").then(|| Value::String(input.to_string()))
                    },
                    validate: |_, _| true,
                })
            })
            .collect(),
        _ => Some(Vec::new()),
    }
}

/// Accepts only distinct keys already included in the server's ready-file whitelist.
/// 仅接受服务端就绪文件允许列表中已包含且互不重复的键。
/// `request` is the frozen import call; `selection` is the parsed string array.
/// `request` 为冻结的导入调用；`selection` 为解析后的字符串数组。
fn ready_subset(request: &Value, selection: &Value) -> bool {
    // Missing arrays are a protocol error, never permission to import an unreviewed file.
    // 缺少数组属于协议错误，不能视为导入未审查文件的许可。
    let (Some(ready), Some(selected)) = (
        request
            .pointer("/request/selectedFileKeys")
            .and_then(Value::as_array),
        selection.as_array(),
    ) else {
        return false;
    };
    // Track exact file keys, preserving case because the service owns file identity.
    // 记录准确文件键并保留大小写，因为文件身份由服务端定义。
    let mut seen = std::collections::BTreeSet::new();
    selected
        .iter()
        .all(|key| key.as_str().is_some() && ready.contains(key) && seen.insert(key.as_str()))
}

/// Validates a display-build selection against its distinct frozen confirmation envelope.
/// 对照展示构建独立的冻结确认结构验证选择。
/// `request` contains confirmation.selectedFileKeys; `selection` may only narrow it.
/// `request` 包含 confirmation.selectedFileKeys；`selection` 只能缩减该集合。
/// Returns false for duplicate, unknown or malformed keys.
/// 重复、未知或格式错误的键返回 false。
fn ready_display_subset(request: &Value, selection: &Value) -> bool {
    // Display builds and directory imports have different documented envelope paths.
    // 展示构建与目录导入的文档定义结构路径不同。
    let (Some(ready), Some(selected)) = (
        request
            .pointer("/confirmation/selectedFileKeys")
            .and_then(Value::as_array),
        selection.as_array(),
    ) else {
        return false;
    };
    // Exact server keys retain their case and identity.
    // 准确服务键保留其大小写及身份。
    let mut seen = std::collections::BTreeSet::new();
    selected
        .iter()
        .all(|key| key.as_str().is_some() && ready.contains(key) && seen.insert(key.as_str()))
}

/// Opens the confirmation of a validated preview result and its exact second-step request.
/// 打开已验证预检结果及其准确第二步请求的确认界面。
pub fn open(app: &mut App, call: ServiceCall, result: Value) {
    let CallKind::Preview(preview) = call.kind else {
        return;
    };
    // The workbench supplies installed plugin origin without an alternate data source.
    // 工作台提供已安装插件来源，不引入其他数据来源。
    let workbench = app.slots.value(Slot::Workbench);
    // The next call freezes server-issued identities before any input is collected.
    // 在收集输入前，下一步调用先冻结服务端签发的身份。
    let Some(mut next) = confirm_preview(&call, &result, workbench) else {
        app.toast(NoticeSeverity::Warning, tr!("tui.preview.unconfirmable"));
        return;
    };
    // Localized copy describes the exact operation being approved.
    // 本地化文案说明正在批准的准确操作。
    let (title, description, label) = match preview {
        Preview::KnowledgeImport => (
            tr!("knowledge.files.title"),
            tr!("tui.preview.readyFiles"),
            tr!("knowledge.files.confirmImport"),
        ),
        Preview::KnowledgeDisplay => (
            tr!("tui.knowledge.buildDisplay"),
            tr!("tui.preview.readyFiles"),
            tr!("tui.field.confirm"),
        ),
        Preview::PluginInstall => (
            tr!("skills.plugins.installLocalPlugin"),
            tr!("tui.preview.permissions"),
            if next.function == "update_local_plugin" {
                tr!("skills.plugins.authorizeAndUpdate")
            } else {
                tr!("skills.plugins.authorizeAndInstall")
            },
        ),
        Preview::VceMaintenance { .. } => (
            tr!("codeContext.area.confirmIndexMaintenance"),
            tr!("tui.preview.maintenance"),
            tr!("codeContext.area.confirmIndexMaintenance"),
        ),
        Preview::VceEmbedding => (
            tr!("tui.field.embeddingReview"),
            tr!("tui.field.embeddingDecision"),
            tr!("tui.field.confirm"),
        ),
        Preview::VmmProject => (
            tr!("vmm.nativeProjects.title"),
            tr!("tui.preview.projectImpact"),
            tr!("vmm.nativeProjects.confirm"),
        ),
        Preview::VmmRemoval => (
            tr!("memory.removal.title"),
            tr!("memory.removal.description"),
            tr!("memory.removal.confirm"),
        ),
        Preview::VmmRetention => (
            tr!("memory.maintenance.runNow"),
            tr!("memory.maintenance.cleanupPreviewDescription"),
            tr!("memory.maintenance.runNow"),
        ),
        Preview::VmmVector => (
            tr!("tui.field.vectorRebuild"),
            tr!("tui.preview.vectorImpact"),
            tr!("tui.field.confirm"),
        ),
        Preview::VmmVectorAdoption => (
            tr!("memory.maintenance.keepVectorsTitle"),
            tr!("tui.preview.vectorImpact"),
            tr!("memory.maintenance.keepVectors"),
        ),
    };
    // Show the initial target set; subsequent input may only narrow or explicitly decide it.
    // 展示初始目标集；后续输入仅可缩减目标或明确选择决定。
    let frozen = match preview {
        Preview::KnowledgeImport => next.param("/request/selectedFileKeys").clone(),
        Preview::KnowledgeDisplay => next.param("/confirmation/selectedFileKeys").clone(),
        Preview::PluginInstall => next.param("/approvedPermissions").clone(),
        Preview::VceMaintenance { .. } => next.param("/request/actionIds").clone(),
        Preview::VceEmbedding => next
            .param("/request/vceEmbeddingConfirmation/decisions")
            .clone(),
        _ => next.param("/request/payload").clone(),
    };
    // Field definitions enforce the same choices allowed by the WebUI preflight dialog.
    // 字段定义约束为 WebUI 预检对话框允许的相同选择。
    let Some(fields) = input_fields(preview, &result, &next) else {
        app.toast(NoticeSeverity::Warning, tr!("tui.preview.unconfirmable"));
        return;
    };
    // A prepared call owns no secret copy while the transient form collects values.
    // 临时表单收集输入期间，预备调用不持有密钥副本。
    let accept = if fields.is_empty() {
        UiAction::Send(Command::Call(next))
    } else {
        // Move the request rather than cloning installation-only credentials.
        // 移动请求，避免复制仅供安装使用的凭据。
        let request = std::mem::take(&mut next.params);
        UiAction::Form(Box::new(super::form::Form {
            request,
            fields,
            index: 0,
            save: super::form::FormSubmit::Prepared(next),
        }))
    };
    // The scrollable sheet exposes every server-planned target before the final consent.
    // 可滚动面板在最终同意前展示全部服务端规划目标。
    let sheet = super::overlay::DetailSheet::new(title.clone())
        .section(tr!("tui.preview.summary"), description.clone())
        .section(
            tr!("tui.preview.details"),
            serde_json::to_string_pretty(&result).expect("JSON preview"),
        )
        .section(
            tr!("tui.preview.approval"),
            serde_json::to_string_pretty(&frozen).expect("JSON frozen targets"),
        )
        .action(
            label.clone(),
            true,
            confirm(title, description, label, accept),
        );
    super::action::perform(app, UiAction::Detail(Box::new(sheet)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A ready-file input can narrow the frozen set, but cannot add or repeat a key.
    /// 就绪文件输入可以缩减冻结集合，但不能新增或重复键。
    #[test]
    fn import_selection_is_a_distinct_subset() {
        // Exact protocol request.
        // 协议请求文档。
        let request = json!({"request":{"selectedFileKeys":["a.md","b.md"]}});
        assert!(ready_subset(&request, &json!(["b.md"])));
        assert!(ready_subset(&request, &json!([])));
        assert!(!ready_subset(&request, &json!(["a.md", "a.md"])));
        assert!(!ready_subset(&request, &json!(["outside.md"])));
        assert!(!ready_subset(&Value::Null, &json!(["a.md"])));
    }

    /// Display selection uses its own envelope and rejects import envelopes or unreviewed keys.
    /// 展示选择使用自身结构，并拒绝导入结构或未审查键。
    #[test]
    fn display_selection_is_a_distinct_subset_in_its_own_envelope() {
        // Frozen display whitelist differs structurally from an import request.
        // 冻结展示允许列表在结构上与导入请求不同。
        let request = json!({"confirmation":{"selectedFileKeys":["a.md","b.md"]}});
        assert!(ready_display_subset(&request, &json!(["b.md"])));
        assert!(ready_display_subset(&request, &json!([])));
        assert!(!ready_display_subset(&request, &json!(["a.md", "a.md"])));
        assert!(!ready_display_subset(&request, &json!(["outside.md"])));
        assert!(!ready_display_subset(
            &json!({"request":{"selectedFileKeys":["a.md"]}}),
            &json!(["a.md"])
        ));
    }

    /// A hard rebuild is mandatory while other projects allow an explicit compatible decision.
    /// 强制重建不可更改，其他项目则允许明确选择兼容。
    #[test]
    fn embedding_fields_use_the_service_hard_rebuild_gate() {
        // Review.
        // 服务端预检审查结果。
        let review = json!({"revision":"r","candidate":null,"projects":[
            {"projectName":"Compatible","compatible":true,"impact":{"scope":{"project_id":"a"},"hardRebuildReason":null}},
            {"projectName":"Changed","compatible":false,"impact":{"scope":{"project_id":"b"},"hardRebuildReason":"schema changed"}}]});
        // Next.
        // 明确的下一步请求或值。
        let next = crate::service_api::vce::save_reviewed_embedding(&review).unwrap();
        // Fields.
        // 顺序输入字段。
        let fields = input_fields(Preview::VceEmbedding, &review, &next).unwrap();
        assert!((fields[0].parse)("compatible").is_some());
        assert!((fields[1].parse)("compatible").is_none());
        assert!((fields[1].parse)("rebuild").is_some());
        assert!(next.params.pointer(&fields[1].pointer).is_some());
    }
}
