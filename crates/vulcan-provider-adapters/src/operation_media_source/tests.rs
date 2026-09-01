//! Declared binding traversal and private project-source resolution regressions.
//! 声明绑定遍历及私有项目来源解析回归。

use super::*;
use serde_json::json;
use std::sync::Mutex;

/// Metadata-only host fixture recording exact lookups without reading files or using a provider.
/// 不读取文件也不使用供应商、记录精确查询的仅元数据宿主夹具。
#[derive(Debug, Default)]
struct Resolver {
    /// First-lookup order captured independently of duplicate occurrence budgeting.
    /// 独立于重复出现预算记录的首次查询顺序。
    calls: Mutex<Vec<String>>,
    /// Fault injection for a host returning a different resource identity.
    /// 宿主返回不同资源身份的故障注入。
    substitute: bool,
}

#[async_trait::async_trait]
impl ProviderToolMediaSourceResolver for Resolver {
    /// Returns immutable fixture metadata for `reference`, or the explicitly injected failure.
    /// 为`reference`返回不可变夹具元数据，或者显式注入的失败。
    async fn resolve_source(&self, reference: &str) -> Result<ProviderToolMediaSource, String> {
        self.calls.lock().unwrap().push(reference.to_string());
        if reference == "project-resource://404" {
            return Err("Project source is unavailable.".into());
        }
        Ok(ProviderToolMediaSource {
            reference: if self.substitute {
                "project-resource://9".into()
            } else {
                reference.into()
            },
            local_path: PathBuf::from("host-owned-entity"),
            file_name: "original.pdf".into(),
            mime_type: "application/pdf".into(),
            source_identity: crate::PreparedMediaSourceIdentity {
                byte_len: 3,
                content_sha256: "a".repeat(64),
            },
        })
    }
}

/// Builds the exact scalar/string-array binding at `pointer`, with no inferred candidate fields.
/// 在`pointer`构建精确标量/字符串数组绑定，不推断候选字段。
fn binding(pointer: &str) -> crate::OperationUploadBinding {
    crate::OperationUploadBinding {
        source_path: pointer.into(),
        target_path: pointer.into(),
        item_source_path: None,
        item_target_path: None,
        item_match_path: None,
        item_match_value: None,
        upload_channel_refs: vec!["fixture.inline".into()],
        mime_type: None,
        mime_types_by_extension: Default::default(),
        required: true,
        remove_source: false,
        target_encoding: crate::provider_manifest::OperationUploadTargetEncoding::Reference,
    }
}

/// Builds a frozen operation owning the supplied ordered `bindings`; no network operation is executed.
/// 构建持有所给有序`bindings`的冻结操作；不执行网络操作。
fn operation(bindings: Vec<crate::OperationUploadBinding>) -> crate::OperationDeclaration {
    crate::OperationDeclaration {
        operation_id: "fixture.declared-media".into(),
        operation: "image_edit".into(),
        carrier: crate::OperationCarrier::ProviderTool,
        input_resources: vec![vulcan_ai_contract::ResourceKind::File],
        output_resources: Vec::new(),
        tool_target_refs: vec!["fixture.target".into()],
        tool_target_template_refs: Vec::new(),
        request_profile_refs: Vec::new(),
        response_profile_refs: Vec::new(),
        upload_channel_refs: vec!["fixture.inline".into()],
        upload_bindings: bindings,
        async_execution: None,
        timeout_ms: Some(60_000),
        idempotent: false,
    }
}

/// Resolves scalar, string-array and discriminated object-array inputs in declared order without recursive discovery.
/// 按声明顺序解析标量、字符串数组及判别对象数组输入，不进行递归发现。
#[tokio::test]
async fn declared_sources_preserve_occurrences_and_resolve_only_owned_fields() -> Result<(), String>
{
    // Only image-typed items belong to this object-array binding.
    // 只有图片类型条目属于本对象数组绑定。
    let mut items = binding("/items");
    items.item_source_path = Some("/image/source".into());
    items.item_target_path = Some("/image/source".into());
    items.item_match_path = Some("/kind".into());
    items.item_match_value = Some(json!("image"));
    // JSON Pointer escaping is part of the declaration, not a field-name heuristic.
    // JSON指针转义属于声明，而非字段名启发式。
    let operation = operation(vec![
        binding("/first"),
        binding("/slash~1key"),
        items,
        binding("/managed"),
    ]);
    // Undeclared text and discriminator-mismatched paths must remain untouched.
    // 未声明正文及判别不匹配路径必须保持不变。
    let arguments = json!({ "first": "project-resource://2", "slash/key": ["project-resource://1", "project-resource://2", "https://example.test/a"],
        "items": [{"kind": "text", "image": {"source": "project-resource://404"}}, {"kind": "image", "image": {"source": "project-resource://3"}}],
        "prompt": "project-resource://404", "managed": "attachment://legacy" });
    // The collector used by the physical engine keeps duplicate occurrences for policy accounting.
    // 物理引擎使用的收集器为策略统计保留重复出现。
    let inputs = crate::upload_engine::operation_upload_inputs(&operation, &arguments)?;
    assert_eq!(
        inputs
            .iter()
            .map(|input| (
                input.binding_index,
                input.reference,
                input.item_index,
                input.source_was_array
            ))
            .collect::<Vec<_>>(),
        vec![
            (0, "project-resource://2", None, false),
            (1, "project-resource://1", None, true),
            (1, "project-resource://2", None, true),
            (1, "https://example.test/a", None, true),
            (2, "project-resource://3", Some(1), true),
            (3, "attachment://legacy", None, false),
        ]
    );
    // Host resolution deduplicates only immutable metadata; it never rewrites JSON or drops budget entries.
    // 宿主解析只去重不可变元数据；绝不改写JSON或丢弃预算条目。
    let resolver = Resolver::default();
    let sources =
        resolve_declared_operation_media_sources(Some(&operation), &arguments, &resolver).await?;
    assert_eq!(
        *resolver.calls.lock().unwrap(),
        [
            "project-resource://2",
            "project-resource://1",
            "project-resource://3",
            "attachment://legacy"
        ]
    );
    assert_eq!(sources.len(), 4);
    assert_eq!(sources[0].mime_type, "application/pdf");
    assert_eq!(arguments["first"], "project-resource://2");
    assert!(!arguments.to_string().contains("host-owned-entity"));
    Ok(())
}

/// A malformed later binding fails before any earlier host source lookup.
/// 格式错误的后续绑定在任何前项宿主查询之前失败。
#[tokio::test]
async fn declared_sources_prevalidate_all_shapes_before_resolution() {
    // Two required sources make late-shape errors observable without provider effects.
    // 两个必填来源让后项形态错误可观察且没有供应商副作用。
    let operation = operation(vec![binding("/first"), binding("/second")]);
    for arguments in [
        json!({"first": "project-resource://1"}),
        json!({"first": "project-resource://1", "second": ["project-resource://2", 7]}),
        json!({"first": "project-resource://1", "second": []}),
    ] {
        // Each attempt starts with an empty metadata lookup ledger.
        // 每次尝试都从空元数据查询账本开始。
        let resolver = Resolver::default();
        assert!(
            resolve_declared_operation_media_sources(Some(&operation), &arguments, &resolver)
                .await
                .is_err()
        );
        assert!(resolver.calls.lock().unwrap().is_empty());
    }
}

/// Optional absence and non-media operations cannot authorize unrelated project lookups.
/// 可选缺失及非媒体操作不能授权无关项目查询。
#[tokio::test]
async fn declared_sources_ignore_optional_absence_and_unbound_text() -> Result<(), String> {
    // Optional source absence follows the Manifest instead of searching alternate JSON keys.
    // 可选来源缺失遵循Manifest，不搜索替代JSON键。
    let mut optional = binding("/source");
    optional.required = false;
    let operation = operation(vec![optional]);
    let resolver = Resolver::default();
    for arguments in [
        json!({"other": "project-resource://404"}),
        json!({"source": [], "prompt": "project-resource://404"}),
    ] {
        assert!(
            resolve_declared_operation_media_sources(Some(&operation), &arguments, &resolver)
                .await?
                .is_empty()
        );
    }
    assert!(
        resolve_declared_operation_media_sources(
            None,
            &json!({"source": "project-resource://404"}),
            &resolver
        )
        .await?
        .is_empty()
    );
    assert!(resolver.calls.lock().unwrap().is_empty());
    Ok(())
}

/// Failed or mismatched host identities never become substitute file inputs.
/// 失败或不匹配的宿主身份绝不变成替代文件输入。
#[tokio::test]
async fn declared_sources_reject_failed_or_substituted_host_proof() {
    // The operation has exactly one declared resource authority.
    // 操作具有且仅具有一个声明资源权威。
    let operation = operation(vec![binding("/source")]);
    let resolver = Resolver {
        substitute: true,
        ..Default::default()
    };
    assert!(
        resolve_declared_operation_media_sources(
            Some(&operation),
            &json!({"source": "project-resource://1"}),
            &resolver
        )
        .await
        .unwrap_err()
        .contains("differs")
    );
    let resolver = Resolver::default();
    assert!(
        resolve_declared_operation_media_sources(
            Some(&operation),
            &json!({"source": "project-resource://404"}),
            &resolver
        )
        .await
        .unwrap_err()
        .contains("unavailable")
    );
}
