//! Request-local validation bypasses only pending status and conversation visibility.
//! 请求局部验证仅绕过待验证状态与会话可见性。

use crate::runtime::test_support::{
    custom_connection, custom_definition_with_discovery_mode, install_current_custom_provider,
};
use crate::{
    CustomModelDefinition, CustomProviderModelDiscoveryMode, CustomProviderProtocol,
    PersistedAiConfiguration, PublicAiRuntime,
};
use vulcan_ai_contract::{AiOperation, ModelCapabilities};

/// Validation keeps the published generation unchanged and retains exact ownership and enabled checks.
/// 验证保持已发布代际不变，并保留精确归属与启用检查。
/// Returns after resolving all supported custom text protocols without credentials or network access.
/// 在不访问凭据或网络的情况下解析全部被测自定义文本协议后返回。
#[test]
fn explicit_validation_selection_preserves_published_gates_and_exact_routes() {
    for protocol in [
        CustomProviderProtocol::AnthropicMessages,
        CustomProviderProtocol::OpenAiChat,
        CustomProviderProtocol::OpenAiResponses,
    ] {
        // The runtime activates the same manual descriptor-to-manifest path used by settings.
        // 运行时激活与设置相同的手工描述符到清单路径。
        let runtime = PublicAiRuntime::unavailable().unwrap();
        // The configuration owns two pending accounts but only one exact provider/model declaration.
        // 配置持有两个待验证账号，但仅声明一个精确供应商与模型。
        let mut configuration = PersistedAiConfiguration::default();
        install_current_custom_provider(
            &mut configuration,
            custom_definition_with_discovery_mode(
                "custom:validation",
                &[protocol],
                CustomProviderModelDiscoveryMode::Manual,
            ),
        );
        for id in ["pending", "other-pending"] {
            configuration
                .connection_graph
                .connections
                .insert(id.into(), custom_connection(id, "custom:validation"));
            configuration
                .set_connection_pending_validation(id, true)
                .unwrap();
        }
        configuration
            .custom_models
            .entry("custom:validation".into())
            .or_default()
            .insert(
                "validation-model".into(),
                CustomModelDefinition {
                    id: "validation-model".into(),
                    display_name: "Validation model".into(),
                    enabled: true,
                    capabilities: ModelCapabilities {
                        operations: vec![AiOperation::ConversationRespond],
                        ..ModelCapabilities::default()
                    },
                    request_projection: Default::default(),
                    tool_schema_projection: Default::default(),
                    system_binding: None,
                },
            );
        configuration.set_model_enabled("custom:validation", "validation-model", false);
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .unwrap();
        // Capture immutable activation evidence before creating any validation-only view.
        // 在创建任何验证专用视图之前捕获不可变激活证据。
        let before = runtime.configuration_snapshot().unwrap();
        // Generation identity must not advance merely to test a pending account.
        // 代际身份不能仅因测试待验证账号而推进。
        let generation = runtime.catalog_generation();
        // Exact validation resolves the declared protocol even when conversation visibility is disabled.
        // 即使会话可见性被禁用，精确验证仍解析声明协议。
        let selection = runtime
            .resolve_connection_model_validation_selection(
                "pending",
                "custom:validation",
                "validation-model",
            )
            .unwrap();
        assert_eq!(selection.route.connection.id, "pending");
        assert_eq!(selection.route.provider.id, "custom:validation");
        assert_eq!(
            selection.route.protocol_route.protocol_profile_id,
            protocol.profile_id()
        );
        assert_eq!(runtime.configuration_snapshot().unwrap(), before);
        assert_eq!(runtime.catalog_generation(), generation);
        for id in ["pending", "other-pending"] {
            assert!(
                runtime
                    .resolve_connection_model_selection(
                        &configuration,
                        id,
                        "custom:validation",
                        "validation-model",
                    )
                    .is_err()
            );
        }
        // Validation cannot borrow another provider, invent catalog membership, or enable a disabled account.
        // 验证不能借用其他供应商、虚构目录成员或启用被禁用的账号。
        assert!(
            runtime
                .resolve_connection_model_validation_selection(
                    "pending",
                    "custom:another",
                    "validation-model",
                )
                .is_err()
        );
        assert!(
            runtime
                .resolve_connection_model_validation_selection(
                    "pending",
                    "custom:validation",
                    "missing-model",
                )
                .is_err()
        );
        assert!(
            runtime
                .resolve_connection_model_validation_selection(
                    "missing-account",
                    "custom:validation",
                    "validation-model",
                )
                .is_err()
        );
        configuration
            .set_connection_enabled("pending", false)
            .unwrap();
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .unwrap();
        assert!(
            runtime
                .resolve_connection_model_validation_selection(
                    "pending",
                    "custom:validation",
                    "validation-model",
                )
                .is_err()
        );
    }
}
