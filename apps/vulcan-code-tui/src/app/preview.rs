// Confirmed two-step operations freeze their exact service-produced identities.
// 已确认两步操作冻结其精确服务产出身份。

use super::App;
use super::action::{UiAction, confirm};
use crate::backend::Command;
use crate::model::NoticeSeverity;
use crate::service_api::{CallKind, Preview, ServiceCall, Slot, confirm_preview};
use crate::tr;
use serde_json::Value;

/// Builds only fields declared by a validated preview and its frozen next request.
/// 仅构建已验证预检及其冻结下一请求声明的字段。
/// Parameters: preview selects the contract; result owns evidence; next owns the exact request. Returns validated field definitions.
/// 参数：preview 选择契约；result 拥有证据；next 拥有精确请求。返回已验证字段定义。
fn input_fields(
    preview: Preview,
    result: &Value,
    next: &ServiceCall,
) -> Option<Vec<super::form::FormField>> {
    use super::form::FormField;
    match preview {
        Preview::KnowledgePackage => Some(vec![FormField {
            title: tr!("knowledge.name"),
            pointer: "/request/name".to_string(),
            secret: false,
            optional: false,
            parse: |input| {
                (!input.trim().is_empty()).then(|| Value::String(input.trim().to_string()))
            },
            validate: |_, _| true,
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

/// Opens a full immutable review before the final human confirmation.
/// 在最终人工确认前打开完整不可变审阅。
/// Parameters: app owns the modal; call owns the exact preview kind; result owns its evidence. Returns after presenting the review.
/// 参数：app 拥有弹窗；call 拥有精确预检种类；result 拥有其证据。展示审阅后返回。
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
        Preview::KnowledgePackage => (
            tr!("knowledge.package.import"),
            tr!("knowledge.package.independent"),
            tr!("knowledge.package.import"),
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
        Preview::KnowledgePackage => next.param("/request").clone(),
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

    /// Package input changes only the name while preserving exact inspected bytes.
    /// 套件输入仅更改名称，同时保留精确已检查字节。
    /// Parameters: none. Returns after validating the field identity and fingerprint.
    /// 参数：无。验证字段身份及指纹后返回。
    #[test]
    fn package_name_input_keeps_the_inspected_fingerprint() {
        // This synthetic inspection reads no actual file.
        // 此合成检查不读取真实文件。
        let inspection = json!({"archivePath":"X:/synthetic.vkb","sha256":"a".repeat(64),"manifest":{"name":"New library"}});
        // The prepared request freezes its sole archive and complete fingerprint.
        // 预备请求冻结其唯一归档及完整指纹。
        let next = crate::service_api::knowledge_base::confirmed_package_import(&inspection)
            .expect("current inspection");
        // Field lookup follows a stable declared pointer.
        // 字段查找遵循稳定声明指针。
        let fields =
            input_fields(Preview::KnowledgePackage, &inspection, &next).expect("package input");
        // Only the display name can be changed after preview.
        // 预检后仅可更改展示名称。
        let name = fields
            .iter()
            .find(|field| field.pointer == "/request/name")
            .expect("new name field may have evolved");
        assert_eq!(fields.len(), 1);
        assert!((name.parse)(" ").is_none());
        assert_eq!((name.parse)(" Renamed "), Some(json!("Renamed")));
        assert_eq!(next.param("/request/expectedSha256"), &inspection["sha256"]);
        assert_eq!(
            next.param("/request/archivePath"),
            &inspection["archivePath"]
        );
    }

    /// Preserves the existing VCE hard-rebuild confirmation regression.
    /// 保留既有 VCE 强制重建确认回归。
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
