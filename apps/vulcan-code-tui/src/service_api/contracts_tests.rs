// Regression tests for frozen WebUI requests and credential-safe DTO projections.
// 冻结 WebUI 请求及凭据安全 DTO 投影的回归测试。

use super::*;
use crate::model::SettingValue;
use serde_json::json;
/// Knowledge readers preserve the exact WebUI browse and corpus request envelopes.
/// 知识库阅读器保留准确 WebUI 浏览及语料请求结构。
#[test]
fn knowledge_document_requests_and_display_confirmation_match_webui() {
    // Null folder and empty root folder are distinct documented requests.
    // 空值目录与空字符串根目录为不同的文档定义请求。
    let all = knowledge_base::list_managed_knowledge_documents("library", None, 21, 20);
    assert_eq!(all.method, "service_list_knowledge_library_documents");
    assert_eq!(
        all.params,
        json!({"request":{"libraryId":"library","folderPath":null,"limit":21,"offset":20}})
    );
    assert_eq!(
        knowledge_base::list_managed_knowledge_documents("library", Some(""), 21, 0)
            .param("/request/folderPath"),
        ""
    );
    assert_eq!(
        knowledge_base::read_managed_knowledge_document("library", "doc", "display").params,
        json!({"libraryId":"library","documentId":"doc","corpus":"display"})
    );
    // Only ready documents may enter the distinct display-build confirmation envelope.
    // 只有就绪文档可进入独立展示构建确认结构。
    let preview = knowledge_base::preview_knowledge_display("library");
    // Exact server preflight response includes an ignored document.
    // 准确服务预检响应包含被忽略文档。
    let response = json!({"fingerprint":"f","entries":[{"key":"a.md","disposition":"ready"},{"key":"b.md","disposition":"ignored"}]});
    // Confirmed unexecuted build call.
    // 已确认且尚未执行的构建调用。
    let build = confirm_preview(&preview, &response, &Value::Null).unwrap();
    assert_eq!(build.method, "service_build_knowledge_display_index");
    assert_eq!(
        build.params,
        json!({"libraryId":"library","confirmation":{"previewFingerprint":"f","selectedFileKeys":["a.md"]}})
    );
    assert!(
        confirm_preview(
            &preview,
            &json!({"fingerprint":"","entries":[]}),
            &Value::Null
        )
        .is_none()
    );
    assert!(confirm_preview(&preview,&json!({"fingerprint":"f","entries":[{"key":"a","disposition":"ready"},{"key":"a","disposition":"ready"}]}),&Value::Null).is_none());
}

/// Keeps unrelated settings intact and rejects absent or non-finite editable fields.
/// 保持无关设置完整，并拒绝缺失字段或非有限的可编辑值。
#[test]
fn editors_keep_siblings_and_validate_numeric_domains() {
    // This complete save document represents a real retry configuration group.
    // 此完整保存文档代表真实重试配置组。
    let base = json!({"limit":3,"backoff":0.5,"nested":{"enabled":true},"secret":"synthetic-only"});
    // Setter.
    // 已确认字段编辑器。
    let setter = setter::Setter::Dto {
        base: base.clone(),
        pointer: "/limit".into(),
        save: settings_actions::save_provider_retry_settings,
    };
    // Exact service call.
    // 准确服务调用。
    let call = setter.apply(&SettingValue::Number(5)).unwrap();
    assert_eq!(call.param("/request/nested/enabled"), true);
    assert_eq!(call.param("/request/backoff"), 0.5);
    assert_eq!(call.param("/request/limit"), 5);
    assert_eq!(base["limit"], 3);
    assert!(!format!("{setter:?} {call:?}").contains("synthetic-only"));
    // Missing.
    // 缺少必需字段的测试输入。
    let missing = setter::Setter::Dto {
        base: base.clone(),
        pointer: "/absent".into(),
        save: settings_actions::save_provider_retry_settings,
    };
    assert!(missing.apply(&SettingValue::Number(1)).is_none());
    // Decimal.
    // 有界小数编辑器。
    let decimal = setter::Setter::Decimal {
        base,
        pointer: "/backoff".into(),
        min: 0.0,
        max: 1.0,
        save: settings_actions::save_provider_retry_settings,
    };
    for invalid in ["NaN", "inf", "-0.01", "1.01"] {
        assert!(decimal.apply(&SettingValue::Text(invalid.into())).is_none());
    }
    assert_eq!(
        decimal
            .apply(&SettingValue::Text("0.75".into()))
            .unwrap()
            .param("/request/backoff"),
        0.75
    );
}

/// Imports only ready identities and retains a nullable source identity for rescans.
/// 仅导入就绪身份，并为重新扫描保留允许为空的来源身份。
#[test]
fn knowledge_import_freezes_fingerprint_and_ready_files() {
    // The preview has both an admitted file and a rejected file.
    // 预检同时包含一个准入文件及一个被拒绝文件。
    let preview = knowledge_base::preview_knowledge_import("library", "C:/demo", Some("source"));
    // Result.
    // 服务结果。
    let mut result = json!({"fingerprint":"fingerprint-1","entries":[
        {"disposition":"ready","key":"a.md"},{"disposition":"rejected","key":"b.md"}]});
    // Exact service call.
    // 准确服务调用。
    let call = knowledge_base::import_knowledge_directory(&preview, &result).unwrap();
    assert_eq!(call.param("/request/previewFingerprint"), "fingerprint-1");
    assert_eq!(call.param("/request/sourceId"), "source");
    assert_eq!(call.param("/request/selectedFileKeys"), &json!(["a.md"]));
    result["entries"]
        .as_array_mut()
        .unwrap()
        .push(json!({"disposition":"ready","key":"a.md"}));
    assert!(knowledge_base::import_knowledge_directory(&preview, &result).is_none());
    result["entries"] = json!([]);
    assert!(knowledge_base::import_knowledge_directory(&preview, &result).is_some());
    result["fingerprint"] = json!("");
    assert!(knowledge_base::import_knowledge_directory(&preview, &result).is_none());
}

/// Installation consent preserves declared permissions and collects only named transient secrets.
/// 安装同意保留已声明权限，并仅收集具名的临时密钥。
#[test]
fn plugin_confirmation_keeps_permissions_fingerprint_and_secret_metadata() {
    // Values are synthetic; the preview exposes secret names without a secret value.
    // 值均为合成数据；预检仅公开密钥名称，不公开密钥值。
    let preview = skills_actions::inspect_local_plugin("C:/demo/plugin");
    // Result.
    // 服务结果。
    let result = json!({"contentFingerprint":"package-1",
        "permissions":["filesystem_read","dependency_registry_network"],
        "installSecretRequirements":[{"serverId":"server-1","secretKey":"API_CREDENTIAL"}]});
    // Exact service call.
    // 准确服务调用。
    let call = skills_actions::install_local_plugin(&preview, &result, true).unwrap();
    assert_eq!(call.method, "service_update_local_plugin");
    assert_eq!(call.param("/contentFingerprint"), "package-1");
    assert_eq!(call.param("/approvedPermissions"), &result["permissions"]);
    assert_eq!(call.param("/allowRegistryDependencies"), true);
    assert_eq!(call.param("/initialSecrets/0/secretKey"), "API_CREDENTIAL");
    assert_eq!(call.param("/initialSecrets/0/value"), "");
}

/// Native destructive confirmation cannot replace a preview's identity, version or command ID.
/// 原生破坏性确认不能替换预检的身份、版本或命令标识。
#[test]
fn vmm_project_confirmation_preserves_authority_and_blocks_protected_delete() {
    // Preview.
    // 只读预检。
    let preview = memory_actions::native_project_clear_preview(101);
    // Result.
    // 服务结果。
    let result = json!({"data":{"kind":"native_clear_preview","value":{"projectId":101,"storageVersion":9}}});
    // Exact service call.
    // 准确服务调用。
    let call = memory_actions::native_project_confirmation(&preview, &result).unwrap();
    assert_eq!(
        call.param("/request/commandId"),
        preview.param("/request/commandId")
    );
    assert_eq!(call.param("/request/payload/expectedStorageVersion"), 9);
    // Mismatch.
    // 身份不匹配测试结果。
    let mismatch = json!({"data":{"value":{"projectId":102,"storageVersion":9}}});
    assert!(memory_actions::native_project_confirmation(&preview, &mismatch).is_none());
    // Delete.
    // 已声明删除预检。
    let delete = memory_actions::native_project_delete_preview(101);
    // Protected.
    // 受保护项目测试结果。
    let protected = json!({"data":{"value":{"storageVersion":9,"project":{"projectId":101,"protected":true,
        "ownerVulcanProjectIds":[],"linkedVulcanProjectIds":[]}}}});
    assert!(memory_actions::native_project_confirmation(&delete, &protected).is_none());
}

/// Credential removal retains schema-owned identifiers while every arbitrary input value is blanked.
/// 凭据清理保留结构持有的标识，同时清空每个任意命名的输入值。
#[test]
fn scrubbing_preserves_schema_keys_and_redacts_all_secret_input_containers() {
    // Value.
    // 当前协议值。
    let mut value = json!({"headers":[{"key":"X-Custom","value":"h-secret"}],
        "env":[{"key":"CUSTOM","value":"e-secret"}],
        "initialSecrets":[{"serverId":"s","secretKey":"credential-name","value":"i-secret"}],
        "headersFromEnv":[{"key":"Authorization","value":"TOKEN_VARIABLE"}],
        "credentials":[{"key":"access_token","value":"t-secret"}],
        "repositoryKey":"repo","entries":[{"key":"file.md"}],"commandId":"id"});
    scrub_secrets(&mut value);
    for pointer in [
        "/headers/0/value",
        "/env/0/value",
        "/initialSecrets/0/value",
        "/credentials/0/value",
    ] {
        assert_eq!(value.pointer(pointer).unwrap(), "");
    }
    assert_eq!(value["initialSecrets"][0]["secretKey"], "credential-name");
    assert_eq!(value["headersFromEnv"][0]["value"], "TOKEN_VARIABLE");
    assert_eq!(value["entries"][0]["key"], "file.md");
}
