# 02：公共AI设置层生产节点迁移映射

## 当前检查点

2026-08-31，第15阶段。原16244行根文件当前341行；32个Rust产物合计17710行，新增模块声明、明确导入及职责/参数说明计入物理行数。02号已完成全部职责归集，终审与验证结果见02-FINAL-REVIEW.md；是否闭环以终审结论为准。

原始源摘要：`64f893a762edcafb3b2abb7ab2bde2cc8d1a0fee43dfd82ba0c7e2e037c05070`。314原始节点全部一一对应，266生产节点完整；不以行数减少作为充分性判定。

词法审核保留表达式、字面量、参数、返回类型及属性，只忽略注释、空白与普通列表可选尾逗号，可见性单独精确比对。311节点严格等价，3节点只有下述精确格式差异，没有通用格式兜底。46项测试体严格等价，另1项仅有5个实名单表达式match分支花括号格式变化，断言全部保留。

## 全部生产节点

| 原节点 | 原始行 | 当前产物与行 | 状态 | 原可见性→当前声明 |
| --- | ---: | --- | --- | --- |
| `struct:PublicAiSettingsSnapshot` | 70–216 | `crates/vulcan-service-kernel/src/public_ai_settings/snapshot.rs:40` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiCustomModelProtocolPreferenceRecord` | 222–232 | `crates/vulcan-service-kernel/src/public_ai_settings/snapshot.rs:192` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiCustomModelTemplateDecisionRecord` | 238–248 | `crates/vulcan-service-kernel/src/public_ai_settings/snapshot.rs:208` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiCustomModelConnectionAvailabilityRecord` | 254–264 | `crates/vulcan-service-kernel/src/public_ai_settings/snapshot.rs:224` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `enum:PublicAiModelDirectoryKind` | 270–283 | `crates/vulcan-service-kernel/src/public_ai_settings/catalog_projection.rs:15` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiConnectionModelAccessPolicyRecord` | 289–302 | `crates/vulcan-service-kernel/src/public_ai_settings/catalog_projection.rs:34` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiInferenceRouteDiagnostic` | 308–324 | `crates/vulcan-service-kernel/src/public_ai_settings/inference_settings.rs:32` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiInternalInferenceCandidate` | 330–361 | `crates/vulcan-service-kernel/src/public_ai_settings/inference_settings.rs:54` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiProviderToolTargetSnapshot` | 367–380 | `crates/vulcan-service-kernel/src/public_ai_settings/snapshot.rs:240` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiMultimodalUnderstandingCandidateRecord` | 386–393 | `crates/vulcan-service-kernel/src/public_ai_settings/snapshot.rs:259` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiMediaUnderstandingModelCandidate` | 399–421 | `crates/vulcan-service-kernel/src/public_ai_settings/media_candidates.rs:20` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiSessionProviderToolConflict` | 427–437 | `crates/vulcan-service-kernel/src/public_ai_settings/session_tools.rs:37` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiSessionProviderToolSettingsSnapshot` | 443–461 | `crates/vulcan-service-kernel/src/public_ai_settings/session_tools.rs:53` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `provider_tool_target_config_v1` | 465–472 | `crates/vulcan-service-kernel/src/public_ai_settings/session_tools.rs:110` | 按职责迁移；严格等价 | `private`→`private` |
| `provider_tool_selection_config_from_parts_v1` | 489–561 | `crates/vulcan-service-kernel/src/public_ai_settings/session_tools.rs:134` | 按职责迁移；严格等价 | `pub(crate)`→`pub(crate)` |
| `provider_tool_selection_config_v1` | 565–576 | `crates/vulcan-service-kernel/src/public_ai_settings/session_tools.rs:220` | 按职责迁移；严格等价 | `private`→`private` |
| `struct:PublicAiConnectionSnapshot` | 581–624 | `crates/vulcan-service-kernel/src/public_ai_settings/snapshot.rs:271` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `PublicAiConnectionSnapshot::new` | 641–686 | `crates/vulcan-service-kernel/src/public_ai_settings/snapshot.rs:331` | 按职责迁移；严格等价 | `private`→`private` |
| `struct:PublicAiModelRouteRecord` | 693–731 | `crates/vulcan-service-kernel/src/public_ai_settings/catalog_projection.rs:53` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiModelOverrideRecord` | 737–750 | `crates/vulcan-service-kernel/src/public_ai_settings/snapshot.rs:383` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiModelAccessRecord` | 756–772 | `crates/vulcan-service-kernel/src/public_ai_settings/snapshot.rs:402` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiModelCapabilityOverrideRecord` | 778–788 | `crates/vulcan-service-kernel/src/public_ai_settings/snapshot.rs:424` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiConnectionAddRequest` | 794–819 | `crates/vulcan-service-kernel/src/public_ai_settings/connections.rs:33` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiConnectionUpdateRequest` | 825–854 | `crates/vulcan-service-kernel/src/public_ai_settings/connections.rs:64` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiUsageRefreshSettingsRequest` | 860–864 | `crates/vulcan-service-kernel/src/public_ai_settings/general_settings.rs:24` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiProviderPrioritySaveRequest` | 870–877 | `crates/vulcan-service-kernel/src/public_ai_settings/general_settings.rs:34` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiContextWindowPolicyOverridesSaveRequest` | 883–887 | `crates/vulcan-service-kernel/src/public_ai_settings/general_settings.rs:47` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiContextCompactionWindowNodesSaveRequest` | 893–897 | `crates/vulcan-service-kernel/src/public_ai_settings/general_settings.rs:57` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiModelSelectionRequest` | 903–917 | `crates/vulcan-service-kernel/src/public_ai_settings/model_selection.rs:27` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiModelOverridesSaveRequest` | 923–936 | `crates/vulcan-service-kernel/src/public_ai_settings/model_settings.rs:87` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiModelEnabledSaveRequest` | 942–956 | `crates/vulcan-service-kernel/src/public_ai_settings/model_settings.rs:106` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiInternalModelEnabledSaveRequest` | 962–972 | `crates/vulcan-service-kernel/src/public_ai_settings/inference_settings.rs:91` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiInferenceRouteSaveRequest` | 978–992 | `crates/vulcan-service-kernel/src/public_ai_settings/inference_settings.rs:107` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiInferenceTestRequest` | 998–1005 | `crates/vulcan-service-kernel/src/public_ai_settings/inference_testing.rs:26` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiInferenceTestResult` | 1011–1036 | `crates/vulcan-service-kernel/src/public_ai_settings/inference_testing.rs:39` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiInternalModelOverridesSaveRequest` | 1042–1055 | `crates/vulcan-service-kernel/src/public_ai_settings/inference_settings.rs:127` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiProviderToolSelectionSaveRequest` | 1061–1068 | `crates/vulcan-service-kernel/src/public_ai_settings/provider_tools.rs:29` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiMultimodalUnderstandingCandidateSaveRequest` | 1074–1084 | `crates/vulcan-service-kernel/src/public_ai_settings/provider_tools.rs:42` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiMediaUnderstandingSelectionSaveRequest` | 1090–1097 | `crates/vulcan-service-kernel/src/public_ai_settings/provider_tools.rs:58` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiProviderExtensionTargetSetSaveRequest` | 1103–1110 | `crates/vulcan-service-kernel/src/public_ai_settings/provider_tools.rs:71` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiProviderToolTestRequest` | 1116–1126 | `crates/vulcan-service-kernel/src/public_ai_settings/provider_tools.rs:84` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiProviderToolCapabilityEnabledSaveRequest` | 1132–1139 | `crates/vulcan-service-kernel/src/public_ai_settings/provider_tools.rs:100` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiSessionProviderToolSelectionsSaveRequest` | 1145–1164 | `crates/vulcan-service-kernel/src/public_ai_settings/session_tools.rs:77` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiDefaultConnectionSaveRequest` | 1170–1177 | `crates/vulcan-service-kernel/src/public_ai_settings/connections.rs:99` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiConnectionRemoveRequest` | 1183–1190 | `crates/vulcan-service-kernel/src/public_ai_settings/connections.rs:112` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiCustomProviderSaveRequest` | 1196–1260 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_providers.rs:38` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiCustomProviderProbeResult` | 1266–1279 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_providers.rs:108` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiCustomModelValidationRequest` | 1285–1295 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:34` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiCustomModelValidationResult` | 1301–1317 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:50` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiCustomProviderRemoveRequest` | 1323–1331 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_providers.rs:127` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiCustomModelsSaveRequest` | 1337–1348 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:72` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiCustomModelProtocolPreferenceItem` | 1354–1361 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:89` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiCustomModelProtocolPreferencesSaveRequest` | 1367–1374 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:102` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiCustomProviderModelDefaultsSaveRequest` | 1380–1387 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:115` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiCustomModelTemplateDecisionSaveRequest` | 1393–1403 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:128` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiCustomModelTemplateSuggestionsRequest` | 1409–1413 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:144` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiCustomModelTemplateSuggestionsBatchRequest` | 1423–1427 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:154` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiCustomModelTemplateSuggestionsRecord` | 1433–1440 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:164` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiCustomModelTemplateDecisionBatchItem` | 1446–1453 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:177` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiCustomModelTemplateDecisionsBatchSaveRequest` | 1459–1466 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:190` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiCustomModelConnectionAvailabilitySaveRequest` | 1472–1482 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:203` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiConnectionModelEnabledSaveRequest` | 1488–1498 | `crates/vulcan-service-kernel/src/public_ai_settings/model_settings.rs:29` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiConnectionModelsEnabledSaveRequest` | 1504–1511 | `crates/vulcan-service-kernel/src/public_ai_settings/model_settings.rs:45` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiModelCapabilityOverridesSaveRequest` | 1517–1527 | `crates/vulcan-service-kernel/src/public_ai_settings/model_settings.rs:58` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiProviderCapabilityOverridesSaveRequest` | 1533–1540 | `crates/vulcan-service-kernel/src/public_ai_settings/model_settings.rs:74` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PreparedCustomProviderCandidate` | 1544–1563 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_providers.rs:139` | 按职责迁移；严格等价 | `private`→`private` |
| `struct:CustomProviderActivationTarget` | 1568–1581 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_providers.rs:163` | 按职责迁移；严格等价 | `private`→`private` |
| `struct:CustomProviderCandidateObservation` | 1585–1595 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_providers.rs:180` | 按职责迁移；严格等价 | `private`→`private` |
| `struct:CustomProviderConnectionValidationOutcome` | 1600–1613 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_providers.rs:195` | 按职责迁移；严格等价 | `private`→`private` |
| `prepare_custom_provider_candidate` | 1627–1793 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_providers.rs:224` | 按职责迁移；严格等价 | `private`→`private` |
| `custom_provider_validation_configuration` | 1808–1845 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_providers.rs:405` | 按职责迁移；严格等价 | `private`→`private` |
| `custom_provider_connection_contract_changed` | 1860–1914 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_providers.rs:457` | 按职责迁移；严格等价 | `private`→`private` |
| `custom_provider_removed_protocols` | 1930–1975 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_providers.rs:527` | 按职责迁移；严格等价 | `private`→`private` |
| `custom_provider_edit_validation_candidates` | 1989–2041 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_providers.rs:586` | 按职责迁移；严格等价 | `private`→`private` |
| `validate_custom_provider_edit_connections` | 2055–2122 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_providers.rs:652` | 按职责迁移；严格等价 | `private`→`private` |
| `migrate_custom_provider_connections` | 2139–2199 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_providers.rs:736` | 按职责迁移；严格等价 | `private`→`private` |
| `migrate_custom_provider_inference_route` | 2218–2252 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_providers.rs:815` | 按职责迁移；严格等价 | `private`→`private` |
| `observe_custom_provider_candidate` | 2267–2296 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_providers.rs:864` | 按职责迁移；严格等价 | `private`→`private` |
| `manual_fallback_available` | 2300–2307 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_providers.rs:907` | 按职责迁移；严格等价 | `private`→`private` |
| `pending_validation_available` | 2311–2318 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_providers.rs:928` | 按职责迁移；严格等价 | `private`→`private` |
| `custom_provider_save_requires_pending_validation` | 2332–2339 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_providers.rs:949` | 按职责迁移；严格等价 | `private`→`private` |
| `custom_provider_observation_is_accepted` | 2353–2361 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_providers.rs:970` | 按职责迁移；严格等价 | `private`→`private` |
| `service_probe_public_ai_custom_provider` | 2375–2391 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_providers.rs:992` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `service_save_public_ai_custom_provider` | 2395–2783 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_providers.rs:1022` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `service_validate_public_ai_custom_model` | 2798–2936 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:228` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `service_remove_public_ai_custom_provider` | 2950–3096 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_providers.rs:1424` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `service_save_public_ai_custom_models` | 3113–3272 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:383` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `service_save_public_ai_custom_model_protocol_preferences` | 3287–3353 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:557` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `apply_custom_model_replacement` | 3365–3387 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:640` | 按职责迁移；严格等价 | `private`→`private` |
| `service_save_public_ai_custom_provider_model_defaults` | 3391–3446 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:676` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `replace_custom_provider_model_defaults` | 3456–3470 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:746` | 按职责迁移；严格等价 | `private`→`private` |
| `service_public_ai_custom_model_template_suggestions` | 3474–3481 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:774` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `service_public_ai_custom_model_template_suggestions_batch` | 3495–3523 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:795` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `service_save_public_ai_custom_model_template_decision` | 3527–3584 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:837` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `service_save_public_ai_custom_model_template_decisions_batch` | 3600–3669 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:910` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `service_save_public_ai_custom_model_connection_availability` | 3673–3746 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:993` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `validate_custom_model_connection_availability_choice` | 3761–3773 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:1081` | 按职责迁移；严格等价 | `private`→`private` |
| `connection_supports_model` | 3789–3801 | `crates/vulcan-service-kernel/src/public_ai_settings/model_settings.rs:136` | 按职责迁移；严格等价 | `private`→`private` |
| `set_connection_models_enabled_respecting_provider_access` | 3813–3848 | `crates/vulcan-service-kernel/src/public_ai_settings/model_settings.rs:165` | 按职责迁移；严格等价 | `private`→`private` |
| `service_save_public_ai_connection_model_enabled` | 3852–3953 | `crates/vulcan-service-kernel/src/public_ai_settings/model_settings.rs:214` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `service_save_public_ai_connection_models_enabled` | 3957–4057 | `crates/vulcan-service-kernel/src/public_ai_settings/model_settings.rs:329` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `service_save_public_ai_model_capability_overrides` | 4074–4132 | `crates/vulcan-service-kernel/src/public_ai_settings/model_settings.rs:446` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `service_save_public_ai_provider_capability_overrides` | 4136–4175 | `crates/vulcan-service-kernel/src/public_ai_settings/model_settings.rs:518` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `referenced_custom_model_ids` | 4179–4219 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:1107` | 按职责迁移；严格等价 | `private`→`private` |
| `model_remains_on_connection` | 4235–4252 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:1163` | 按职责迁移；严格等价 | `private`→`private` |
| `struct:PublicAiOAuthStartRequest` | 4258–4272 | `crates/vulcan-service-kernel/src/public_ai_settings/oauth.rs:29` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `struct:PublicAiOAuthCommitRequest` | 4278–4298 | `crates/vulcan-service-kernel/src/public_ai_settings/oauth.rs:49` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `service_start_public_ai_oauth` | 4302–4372 | `crates/vulcan-service-kernel/src/public_ai_settings/oauth.rs:83` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `service_public_ai_oauth_status` | 4376–4383 | `crates/vulcan-service-kernel/src/public_ai_settings/oauth.rs:167` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `service_cancel_public_ai_oauth` | 4387–4394 | `crates/vulcan-service-kernel/src/public_ai_settings/oauth.rs:188` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `service_commit_public_ai_oauth_connection` | 4398–4639 | `crates/vulcan-service-kernel/src/public_ai_settings/oauth.rs:209` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `service_public_ai_settings_snapshot` | 4643–4648 | `crates/vulcan-service-kernel/src/public_ai_settings.rs:269` | 保留原根；严格等价 | `pub(super)`→`pub(super)` |
| `service_add_public_ai_connection` | 4652–4714 | `crates/vulcan-service-kernel/src/public_ai_settings/connections.rs:133` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `add_public_ai_connection_with_secret` | 4718–4990 | `crates/vulcan-service-kernel/src/public_ai_settings/connections.rs:209` | 按职责迁移；严格等价 | `private`→`pub(super)` |
| `validate_connection_credential` | 5009–5035 | `crates/vulcan-service-kernel/src/public_ai_settings/connections.rs:498` | 按职责迁移；严格等价 | `private`→`private` |
| `registered_service_entry_kind` | 5051–5058 | `crates/vulcan-service-kernel/src/public_ai_settings/connections.rs:540` | 按职责迁移；严格等价 | `private`→`private` |
| `registered_service_entry` | 5073–5093 | `crates/vulcan-service-kernel/src/public_ai_settings/connections.rs:562` | 按职责迁移；严格等价 | `private`→`private` |
| `registered_credential_profile` | 5108–5124 | `crates/vulcan-service-kernel/src/public_ai_settings/connections.rs:597` | 按职责迁移；严格等价 | `private`→`private` |
| `service_update_public_ai_connection` | 5126–5437 | `crates/vulcan-service-kernel/src/public_ai_settings/connections.rs:627` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `apply_connection_update` | 5453–5496 | `crates/vulcan-service-kernel/src/public_ai_settings/connections.rs:954` | 按职责迁移；严格等价 | `private`→`pub(super)` |
| `rollback_oauth_reauthorization_secret` | 5511–5528 | `crates/vulcan-service-kernel/src/public_ai_settings/oauth.rs:465` | 按职责迁移；严格等价 | `private`→`private` |
| `rollback_replaced_secret` | 5539–5555 | `crates/vulcan-service-kernel/src/public_ai_settings/connections.rs:1013` | 按职责迁移；严格等价 | `private`→`private` |
| `enum:ManagementSecretMutation` | 5559–5579 | `crates/vulcan-service-kernel/src/public_ai_settings/connections.rs:1033` | 按职责迁移；严格等价 | `private`→`private` |
| `ManagementSecretMutation::resulting_reference` | 5584–5592 | `crates/vulcan-service-kernel/src/public_ai_settings/connections.rs:1068` | 按职责迁移；严格等价 | `private`→`private` |
| `ManagementSecretMutation::requires_validation` | 5596–5598 | `crates/vulcan-service-kernel/src/public_ai_settings/connections.rs:1090` | 按职责迁移；严格等价 | `private`→`private` |
| `ManagementSecretMutation::removes_key` | 5602–5604 | `crates/vulcan-service-kernel/src/public_ai_settings/connections.rs:1106` | 按职责迁移；严格等价 | `private`→`private` |
| `prepare_management_secret_mutation` | 5609–5649 | `crates/vulcan-service-kernel/src/public_ai_settings/connections.rs:1123` | 按职责迁移；严格等价 | `private`→`private` |
| `rollback_management_secret_mutation` | 5653–5683 | `crates/vulcan-service-kernel/src/public_ai_settings/connections.rs:1177` | 按职责迁移；严格等价 | `private`→`private` |
| `delete_connection_secrets` | 5687–5701 | `crates/vulcan-service-kernel/src/public_ai_settings/connections.rs:1221` | 按职责迁移；严格等价 | `private`→`private` |
| `restore_connection_secrets` | 5719–5738 | `crates/vulcan-service-kernel/src/public_ai_settings/connections.rs:1253` | 按职责迁移；严格等价 | `private`→`private` |
| `combine_results` | 5742–5748 | `crates/vulcan-service-kernel/src/public_ai_settings/publication.rs:31` | 按职责迁移；严格等价 | `private`→`pub(super)` |
| `compensated_operation_error` | 5763–5780 | `crates/vulcan-service-kernel/src/public_ai_settings/publication.rs:55` | 按职责迁移；严格等价 | `private`→`pub(super)` |
| `restore_public_ai_configuration_activation_in_transaction` | 5798–5832 | `crates/vulcan-service-kernel/src/public_ai_settings/publication.rs:90` | 按职责迁移；严格等价 | `private`→`pub(super)` |
| `restore_public_ai_runtime_activation` | 5849–5859 | `crates/vulcan-service-kernel/src/public_ai_settings/publication.rs:141` | 按职责迁移；严格等价 | `private`→`pub(super)` |
| `mutate_public_ai_configuration_and_publish_in_transaction` | 5879–5901 | `crates/vulcan-service-kernel/src/public_ai_settings/publication.rs:171` | 按职责迁移；精确格式等价 | `private`→`pub(super)` |
| `mutate_public_ai_configuration_and_publish_without_session_model_reconciliation_in_transaction` | 5918–5952 | `crates/vulcan-service-kernel/src/public_ai_settings/publication.rs:214` | 按职责迁移；严格等价 | `private`→`pub(super)` |
| `mutate_public_ai_configuration_and_publish_sessions_in_transaction` | 5973–6016 | `crates/vulcan-service-kernel/src/public_ai_settings/publication.rs:269` | 按职责迁移；严格等价 | `private`→`pub(super)` |
| `mutate_public_ai_configuration_and_publish_using_in_transaction` | 6033–6078 | `crates/vulcan-service-kernel/src/public_ai_settings/publication.rs:329` | 按职责迁移；严格等价 | `private`→`pub(super)` |
| `delete_provider_credential_secrets` | 6082–6120 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_providers.rs:1584` | 按职责迁移；严格等价 | `private`→`private` |
| `restore_provider_credential_secrets` | 6124–6144 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_providers.rs:1636` | 按职责迁移；严格等价 | `private`→`private` |
| `service_save_public_ai_default_connection` | 6148–6198 | `crates/vulcan-service-kernel/src/public_ai_settings/connections.rs:1286` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `service_remove_public_ai_connection` | 6202–6340 | `crates/vulcan-service-kernel/src/public_ai_settings/connections.rs:1350` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `service_sync_public_ai_connection` | 6344–6449 | `crates/vulcan-service-kernel/src/public_ai_settings/connections.rs:1502` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `activate_synced_provider_catalogs` | 6471–6493 | `crates/vulcan-service-kernel/src/public_ai_settings/provider_synchronization.rs:26` | 按职责迁移；严格等价 | `private`→`private` |
| `enum:ProviderProjectionPublicationFailure` | 6498–6505 | `crates/vulcan-service-kernel/src/public_ai_settings/provider_synchronization.rs:53` | 按职责迁移；严格等价 | `private`→`private` |
| `struct:StagedProviderActivationCancellationGuard` | 6509–6525 | `crates/vulcan-service-kernel/src/public_ai_settings/provider_synchronization.rs:64` | 按职责迁移；严格等价 | `private`→`private` |
| `StagedProviderActivationCancellationGuard<'a>::new` | 6540–6553 | `crates/vulcan-service-kernel/src/public_ai_settings/provider_synchronization.rs:95` | 按职责迁移；严格等价 | `private`→`private` |
| `StagedProviderActivationCancellationGuard<'a>::disarm` | 6557–6559 | `crates/vulcan-service-kernel/src/public_ai_settings/provider_synchronization.rs:122` | 按职责迁移；严格等价 | `private`→`private` |
| `Drop for StagedProviderActivationCancellationGuard<'_>::drop` | 6565–6583 | `crates/vulcan-service-kernel/src/public_ai_settings/provider_synchronization.rs:140` | 按职责迁移；严格等价 | `private`→`private` |
| `required_provider_activation_snapshots` | 6598–6640 | `crates/vulcan-service-kernel/src/public_ai_settings/provider_synchronization.rs:173` | 按职责迁移；严格等价 | `private`→`private` |
| `compensate_unpublished_provider_activation` | 6656–6676 | `crates/vulcan-service-kernel/src/public_ai_settings/provider_synchronization.rs:231` | 按职责迁移；严格等价 | `private`→`private` |
| `publish_synced_provider_projections_or_restore` | 6692–6834 | `crates/vulcan-service-kernel/src/public_ai_settings/provider_synchronization.rs:267` | 按职责迁移；严格等价 | `private`→`private` |
| `compensated_provider_projection_report` | 6848–6858 | `crates/vulcan-service-kernel/src/public_ai_settings/provider_synchronization.rs:423` | 按职责迁移；严格等价 | `private`→`private` |
| `finalize_provider_sync_report` | 6874–6954 | `crates/vulcan-service-kernel/src/public_ai_settings/provider_synchronization.rs:449` | 按职责迁移；严格等价 | `private`→`private` |
| `bounded_provider_manifest_rpc_error` | 6968–6970 | `crates/vulcan-service-kernel/src/public_ai_settings/provider_synchronization.rs:543` | 按职责迁移；严格等价 | `private`→`private` |
| `service_sync_provider_manifests` | 6972–7056 | `crates/vulcan-service-kernel/src/public_ai_settings/provider_synchronization.rs:565` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `service_get_provider_manifest_status` | 7065–7090 | `crates/vulcan-service-kernel/src/public_ai_settings/provider_synchronization.rs:663` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `service_preview_provider_manifest` | 7104–7146 | `crates/vulcan-service-kernel/src/public_ai_settings/provider_synchronization.rs:702` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `service_rollback_provider_manifest` | 7155–7215 | `crates/vulcan-service-kernel/src/public_ai_settings/provider_synchronization.rs:758` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `service_sync_provider_manifest` | 7224–7274 | `crates/vulcan-service-kernel/src/public_ai_settings/provider_synchronization.rs:832` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `service_query_public_ai_connection_usage` | 7278–7286 | `crates/vulcan-service-kernel/src/public_ai_settings/connections.rs:1621` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `service_save_public_ai_usage_refresh_settings` | 7290–7322 | `crates/vulcan-service-kernel/src/public_ai_settings/general_settings.rs:75` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `service_save_public_ai_provider_priority` | 7326–7371 | `crates/vulcan-service-kernel/src/public_ai_settings/general_settings.rs:121` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `service_save_public_ai_context_window_policy_overrides` | 7375–7397 | `crates/vulcan-service-kernel/src/public_ai_settings/general_settings.rs:180` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `service_save_public_ai_context_compaction_window_nodes` | 7401–7436 | `crates/vulcan-service-kernel/src/public_ai_settings/general_settings.rs:216` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `plan_session_model_selection_for_next_provider_request` | 7466–7505 | `crates/vulcan-service-kernel/src/public_ai_settings/model_selection.rs:59` | 按职责迁移；严格等价 | `private`→`private` |
| `service_select_public_ai_model` | 7509–7750 | `crates/vulcan-service-kernel/src/public_ai_settings/model_selection.rs:112` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `service_save_public_ai_model_overrides` | 7754–7803 | `crates/vulcan-service-kernel/src/public_ai_settings/model_settings.rs:571` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `service_save_public_ai_model_enabled` | 7807–7928 | `crates/vulcan-service-kernel/src/public_ai_settings/model_settings.rs:634` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `service_save_public_ai_internal_model_enabled` | 7942–7984 | `crates/vulcan-service-kernel/src/public_ai_settings/inference_settings.rs:154` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `service_save_public_ai_internal_model_overrides` | 7998–8069 | `crates/vulcan-service-kernel/src/public_ai_settings/inference_settings.rs:210` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `service_save_public_ai_inference_route` | 8083–8172 | `crates/vulcan-service-kernel/src/public_ai_settings/inference_settings.rs:295` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `service_test_public_ai_inference_route` | 8187–8351 | `crates/vulcan-service-kernel/src/public_ai_settings/inference_testing.rs:79` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `configured_inference_service_for_settings` | 8365–8375 | `crates/vulcan-service-kernel/src/public_ai_settings/inference_testing.rs:257` | 按职责迁移；严格等价 | `private`→`pub(super)` |
| `settings_inference_test_caller` | 8391–8419 | `crates/vulcan-service-kernel/src/public_ai_settings/inference_testing.rs:283` | 按职责迁移；严格等价 | `private`→`private` |
| `configured_settings_inference_test_route` | 8435–8463 | `crates/vulcan-service-kernel/src/public_ai_settings/inference_testing.rs:327` | 按职责迁移；严格等价 | `private`→`private` |
| `preferred_embedding_task` | 8477–8489 | `crates/vulcan-service-kernel/src/public_ai_settings/inference_testing.rs:369` | 按职责迁移；严格等价 | `private`→`private` |
| `preferred_embedding_output_kind` | 8503–8520 | `crates/vulcan-service-kernel/src/public_ai_settings/inference_testing.rs:395` | 按职责迁移；严格等价 | `private`→`private` |
| `preferred_embedding_encoding` | 8534–8548 | `crates/vulcan-service-kernel/src/public_ai_settings/inference_testing.rs:426` | 按职责迁移；严格等价 | `private`→`private` |
| `preferred_rerank_truncation` | 8562–8579 | `crates/vulcan-service-kernel/src/public_ai_settings/inference_testing.rs:454` | 按职责迁移；严格等价 | `private`→`private` |
| `fixed_rerank_test_candidates` | 8593–8607 | `crates/vulcan-service-kernel/src/public_ai_settings/inference_testing.rs:485` | 按职责迁移；严格等价 | `private`→`private` |
| `validate_embedding_test_response` | 8621–8683 | `crates/vulcan-service-kernel/src/public_ai_settings/inference_testing.rs:513` | 按职责迁移；严格等价 | `private`→`private` |
| `validate_rerank_test_response` | 8697–8731 | `crates/vulcan-service-kernel/src/public_ai_settings/inference_testing.rs:589` | 按职责迁移；严格等价 | `private`→`private` |
| `service_save_public_ai_provider_tool_selection` | 8735–8792 | `crates/vulcan-service-kernel/src/public_ai_settings/provider_tools.rs:121` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `service_save_public_ai_multimodal_understanding_candidate` | 8806–8861 | `crates/vulcan-service-kernel/src/public_ai_settings/provider_tools.rs:192` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `service_save_public_ai_media_understanding_selection` | 8875–9002 | `crates/vulcan-service-kernel/src/public_ai_settings/provider_tools.rs:261` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `service_save_public_ai_provider_extension_target_set` | 9006–9043 | `crates/vulcan-service-kernel/src/public_ai_settings/provider_tools.rs:402` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `service_test_public_ai_provider_tool` | 9057–9080 | `crates/vulcan-service-kernel/src/public_ai_settings/provider_tools.rs:453` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `service_save_public_ai_provider_tool_capability_enabled` | 9084–9149 | `crates/vulcan-service-kernel/src/public_ai_settings/provider_tools.rs:490` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `service_public_ai_session_provider_tool_selections` | 9153–9221 | `crates/vulcan-service-kernel/src/public_ai_settings/session_tools.rs:245` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `resolved_session_provider_tool_settings` | 9227–9260 | `crates/vulcan-service-kernel/src/public_ai_settings/session_tools.rs:319` | 按职责迁移；严格等价 | `private`→`private` |
| `project_session_provider_tool_settings` | 9266–9276 | `crates/vulcan-service-kernel/src/public_ai_settings/session_tools.rs:358` | 按职责迁移；严格等价 | `private`→`private` |
| `service_rebuild_public_ai_session_provider_tool_selections` | 9282–9324 | `crates/vulcan-service-kernel/src/public_ai_settings/session_tools.rs:384` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `reconcile_session_provider_tool_settings` | 9339–9513 | `crates/vulcan-service-kernel/src/public_ai_settings/session_tools.rs:441` | 按职责迁移；严格等价 | `private`→`private` |
| `service_save_public_ai_session_provider_tool_selections` | 9517–9589 | `crates/vulcan-service-kernel/src/public_ai_settings/session_tools.rs:629` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `validate_provider_tool_session_settings` | 9593–9626 | `crates/vulcan-service-kernel/src/public_ai_settings/session_tools.rs:715` | 按职责迁移；严格等价 | `pub(crate)`→`pub(crate)` |
| `provider_tool_session_conflicts` | 9630–9775 | `crates/vulcan-service-kernel/src/public_ai_settings/session_tools.rs:762` | 按职责迁移；严格等价 | `private`→`private` |
| `media_understanding_backend_is_available` | 9791–9827 | `crates/vulcan-service-kernel/src/public_ai_settings/provider_tools.rs:571` | 按职责迁移；严格等价 | `private`→`pub(super)` |
| `provider_tool_capability_backend_is_available` | 9842–9859 | `crates/vulcan-service-kernel/src/public_ai_settings/provider_tools.rs:622` | 按职责迁移；严格等价 | `private`→`private` |
| `filter_current_provider_tool_selections` | 9875–9890 | `crates/vulcan-service-kernel/src/public_ai_settings/provider_tools.rs:655` | 按职责迁移；严格等价 | `private`→`pub(super)` |
| `validate_provider_extension_target_sets` | 9894–9947 | `crates/vulcan-service-kernel/src/public_ai_settings/provider_tools.rs:684` | 按职责迁移；严格等价 | `private`→`pub(super)` |
| `session_media_understanding_modes_from_configuration` | 9961–9979 | `crates/vulcan-service-kernel/src/public_ai_settings/session_tools.rs:921` | 按职责迁移；严格等价 | `pub(crate)`→`pub(crate)` |
| `session_media_understanding_modes_from_runtime` | 9993–10017 | `crates/vulcan-service-kernel/src/public_ai_settings/session_tools.rs:953` | 按职责迁移；严格等价 | `private`→`private` |
| `session_media_understanding_modes_to_runtime` | 10021–10031 | `crates/vulcan-service-kernel/src/public_ai_settings/session_tools.rs:991` | 按职责迁移；严格等价 | `private`→`private` |
| `session_provider_tool_selection_from_contract` | 10035–10042 | `crates/vulcan-service-kernel/src/public_ai_settings/session_tools.rs:1015` | 按职责迁移；严格等价 | `private`→`private` |
| `session_provider_tool_selections_from_runtime` | 10046–10068 | `crates/vulcan-service-kernel/src/public_ai_settings/session_tools.rs:1036` | 按职责迁移；严格等价 | `private`→`private` |
| `session_provider_extension_target_sets_from_runtime` | 10072–10093 | `crates/vulcan-service-kernel/src/public_ai_settings/session_tools.rs:1072` | 按职责迁移；严格等价 | `private`→`private` |
| `session_provider_tool_enabled_capabilities_from_runtime` | 10097–10109 | `crates/vulcan-service-kernel/src/public_ai_settings/session_tools.rs:1107` | 按职责迁移；严格等价 | `private`→`private` |
| `provider_tool_capability_key` | 10113–10118 | `crates/vulcan-service-kernel/src/public_ai_settings/session_tools.rs:1133` | 按职责迁移；严格等价 | `pub(crate)`→`pub(crate)` |
| `public_ai_session_media_understanding_bridge_kinds` | 10137–10200 | `crates/vulcan-service-kernel/src/public_ai_settings/session_tools.rs:1157` | 按职责迁移；严格等价 | `pub(crate)`→`pub(crate)` |
| `model_selection_is_available` | 10216–10255 | `crates/vulcan-service-kernel/src/public_ai_settings/model_selection.rs:371` | 按职责迁移；严格等价 | `private`→`private` |
| `preferred_fallback_model_selection` | 10260–10271 | `crates/vulcan-service-kernel/src/public_ai_settings/model_selection.rs:425` | 按职责迁移；严格等价 | `private`→`private` |
| `ensure_active_model_selection` | 10275–10305 | `crates/vulcan-service-kernel/src/public_ai_settings/model_selection.rs:450` | 按职责迁移；严格等价 | `private`→`pub(super)` |
| `preferred_available_connection_for_model` | 10321–10364 | `crates/vulcan-service-kernel/src/public_ai_settings/model_selection.rs:496` | 按职责迁移；严格等价 | `private`→`private` |
| `plan_reconciled_session_model_selections` | 10379–10397 | `crates/vulcan-service-kernel/src/public_ai_settings/model_selection.rs:554` | 按职责迁移；严格等价 | `private`→`pub(super)` |
| `reconcile_missing_session_model_selections` | 10415–10440 | `crates/vulcan-service-kernel/src/public_ai_settings/model_selection.rs:592` | 按职责迁移；严格等价 | `pub(crate)`→`pub(crate)` |
| `first_enabled_model_selection` | 10455–10492 | `crates/vulcan-service-kernel/src/public_ai_settings/model_selection.rs:634` | 按职责迁移；严格等价 | `private`→`pub(super)` |
| `preferred_replacement_model_selection` | 10508–10553 | `crates/vulcan-service-kernel/src/public_ai_settings/model_selection.rs:689` | 按职责迁移；严格等价 | `private`→`pub(super)` |
| `reconcile_disabled_connection_sessions` | 10569–10584 | `crates/vulcan-service-kernel/src/public_ai_settings/model_selection.rs:750` | 按职责迁移；严格等价 | `private`→`pub(super)` |
| `reconcile_explicitly_revoked_session_model_selections` | 10599–10631 | `crates/vulcan-service-kernel/src/public_ai_settings/model_selection.rs:780` | 按职责迁移；严格等价 | `private`→`pub(super)` |
| `public_ai_runtime_and_configuration` | 10635–10648 | `crates/vulcan-service-kernel/src/public_ai_settings.rs:288` | 保留原根；严格等价 | `pub(crate)`→`pub(crate)` |
| `public_ai_available_model_selectors` | 10664–10669 | `crates/vulcan-service-kernel/src/public_ai_settings.rs:317` | 保留原根；严格等价 | `pub(crate)`→`pub(crate)` |
| `connection_model_directory_kind` | 10685–10727 | `crates/vulcan-service-kernel/src/public_ai_settings/catalog_projection.rs:107` | 按职责迁移；严格等价 | `private`→`private` |
| `build_connection_model_access_policy_records` | 10741–10760 | `crates/vulcan-service-kernel/src/public_ai_settings/catalog_projection.rs:163` | 按职责迁移；严格等价 | `private`→`pub(super)` |
| `effective_catalog_model_protocol_id` | 10774–10781 | `crates/vulcan-service-kernel/src/public_ai_settings/catalog_projection.rs:196` | 按职责迁移；严格等价 | `private`→`pub(super)` |
| `catalog_model_supports_text_conversation` | 10797–10804 | `crates/vulcan-service-kernel/src/public_ai_settings/catalog_projection.rs:219` | 按职责迁移；严格等价 | `private`→`pub(super)` |
| `public_ai_model_binding_source_id` | 10818–10828 | `crates/vulcan-service-kernel/src/public_ai_settings/catalog_projection.rs:240` | 按职责迁移；严格等价 | `private`→`private` |
| `custom_model_route_unavailable_reason` | 10844–10899 | `crates/vulcan-service-kernel/src/public_ai_settings/catalog_projection.rs:266` | 按职责迁移；严格等价 | `private`→`private` |
| `build_public_ai_model_route_records` | 10915–11017 | `crates/vulcan-service-kernel/src/public_ai_settings/catalog_projection.rs:337` | 按职责迁移；严格等价 | `private`→`pub(super)` |
| `build_public_ai_settings_snapshot` | 11021–11385 | `crates/vulcan-service-kernel/src/public_ai_settings/snapshot.rs:448` | 按职责迁移；严格等价 | `private`→`pub(super)` |
| `build_internal_inference_route_diagnostics` | 11400–11441 | `crates/vulcan-service-kernel/src/public_ai_settings/inference_settings.rs:399` | 按职责迁移；严格等价 | `private`→`pub(super)` |
| `push_internal_inference_route_diagnostic` | 11456–11491 | `crates/vulcan-service-kernel/src/public_ai_settings/inference_settings.rs:455` | 按职责迁移；严格等价 | `private`→`private` |
| `media_understanding_resource_kind` | 11517–11526 | `crates/vulcan-service-kernel/src/public_ai_settings/media_candidates.rs:56` | 按职责迁移；严格等价 | `private`→`pub(super)` |
| `model_route_supports_media_understanding` | 11540–11552 | `crates/vulcan-service-kernel/src/public_ai_settings/media_candidates.rs:79` | 按职责迁移；严格等价 | `private`→`pub(super)` |
| `protocol_projected_media_capabilities` | 11567–11597 | `crates/vulcan-service-kernel/src/public_ai_settings/media_candidates.rs:106` | 按职责迁移；严格等价 | `private`→`private` |
| `build_media_understanding_model_candidates` | 11611–11690 | `crates/vulcan-service-kernel/src/public_ai_settings/media_candidates.rs:150` | 按职责迁移；严格等价 | `private`→`pub(super)` |
| `build_internal_inference_candidates` | 11692–11860 | `crates/vulcan-service-kernel/src/public_ai_settings/inference_settings.rs:504` | 按职责迁移；严格等价 | `private`→`pub(super)` |
| `resolve_knowledge_internal_inference_candidate` | 11864–11900 | `crates/vulcan-service-kernel/src/public_ai_settings/inference_settings.rs:688` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `sync_knowledge_inference_profile` | 11908–11966 | `crates/vulcan-service-kernel/src/public_ai_settings/inference_settings.rs:744` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `remove_knowledge_inference_profile` | 11970–11994 | `crates/vulcan-service-kernel/src/public_ai_settings/inference_settings.rs:816` | 按职责迁移；严格等价 | `pub(super)`→`pub(crate)` |
| `validate_internal_model_overrides` | 11998–12109 | `crates/vulcan-service-kernel/src/public_ai_settings/inference_validation.rs:22` | 按职责迁移；严格等价 | `private`→`pub(super)` |
| `validate_vmm_llm_reasoning_policy` | 12123–12137 | `crates/vulcan-service-kernel/src/public_ai_settings/inference_validation.rs:147` | 按职责迁移；严格等价 | `private`→`pub(super)` |
| `validate_internal_model_parameters` | 12141–12190 | `crates/vulcan-service-kernel/src/public_ai_settings/inference_validation.rs:175` | 按职责迁移；严格等价 | `private`→`private` |
| `validate_positive_limit` | 12194–12214 | `crates/vulcan-service-kernel/src/public_ai_settings/inference_validation.rs:238` | 按职责迁移；严格等价 | `private`→`private` |
| `validate_internal_reasoning_effort` | 12218–12240 | `crates/vulcan-service-kernel/src/public_ai_settings/inference_validation.rs:272` | 按职责迁移；严格等价 | `private`→`pub(super)` |
| `ensure_no_llm_overrides` | 12244–12254 | `crates/vulcan-service-kernel/src/public_ai_settings/inference_validation.rs:308` | 按职责迁移；严格等价 | `private`→`private` |
| `ensure_no_embedding_overrides` | 12258–12269 | `crates/vulcan-service-kernel/src/public_ai_settings/inference_validation.rs:332` | 按职责迁移；严格等价 | `private`→`private` |
| `ensure_no_rerank_overrides` | 12273–12283 | `crates/vulcan-service-kernel/src/public_ai_settings/inference_validation.rs:357` | 按职责迁移；严格等价 | `private`→`private` |
| `invalidate_context_compaction_window_budget_fingerprints` | 12297–12316 | `crates/vulcan-service-kernel/src/public_ai_settings/publication.rs:388` | 按职责迁移；严格等价 | `private`→`private` |
| `publish_public_ai_workbench_after_context_compaction_window_nodes_change` | 12332–12362 | `crates/vulcan-service-kernel/src/public_ai_settings/publication.rs:423` | 按职责迁移；严格等价 | `private`→`pub(super)` |
| `publish_public_ai_workbench` | 12376–12383 | `crates/vulcan-service-kernel/src/public_ai_settings/publication.rs:467` | 按职责迁移；严格等价 | `pub(crate)`→`pub(crate)` |
| `publish_public_ai_workbench_without_session_model_reconciliation` | 12398–12414 | `crates/vulcan-service-kernel/src/public_ai_settings/publication.rs:489` | 按职责迁移；严格等价 | `private`→`private` |
| `publish_public_ai_workbench_with_commit` | 12430–12449 | `crates/vulcan-service-kernel/src/public_ai_settings/publication.rs:521` | 按职责迁移；严格等价 | `private`→`pub(super)` |
| `struct:PublicAiWorkspaceReconciliation` | 12454–12464 | `crates/vulcan-service-kernel/src/public_ai_settings/publication.rs:545` | 按职责迁移；严格等价 | `private`→`pub(super)` |
| `PublicAiWorkspaceReconciliation::route_revocations` | 12479–12485 | `crates/vulcan-service-kernel/src/public_ai_settings/publication.rs:570` | 按职责迁移；严格等价 | `private`→`private` |
| `PublicAiWorkspaceReconciliation::workspace_only` | 12499–12505 | `crates/vulcan-service-kernel/src/public_ai_settings/publication.rs:590` | 按职责迁移；严格等价 | `private`→`pub(super)` |
| `PublicAiWorkspaceReconciliation::context_budget_invalidations` | 12519–12525 | `crates/vulcan-service-kernel/src/public_ai_settings/publication.rs:610` | 按职责迁移；严格等价 | `private`→`private` |
| `publish_public_ai_workbench_with_session_reconciliation_and_commit` | 12546–12573 | `crates/vulcan-service-kernel/src/public_ai_settings/publication.rs:637` | 按职责迁移；精确格式等价 | `private`→`pub(super)` |
| `publish_public_ai_workbench_with_workspace_reconciliation_and_commit` | 12593–12853 | `crates/vulcan-service-kernel/src/public_ai_settings/publication.rs:687` | 按职责迁移；严格等价 | `private`→`pub(super)` |
| `remove_connection_from_configuration` | 12857–12997 | `crates/vulcan-service-kernel/src/public_ai_settings/connections.rs:1643` | 按职责迁移；严格等价 | `private`→`pub(super)` |
| `validate_custom_provider_identity` | 13001–13015 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_providers.rs:1670` | 按职责迁移；严格等价 | `private`→`pub(super)` |
| `validate_removable_custom_provider_identity` | 13030–13040 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_providers.rs:1699` | 按职责迁移；严格等价 | `private`→`private` |
| `validate_custom_provider_model_exists` | 13044–13057 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:1194` | 按职责迁移；严格等价 | `private`→`private` |
| `validate_custom_template_decision` | 13061–13088 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:1221` | 按职责迁移；严格等价 | `private`→`private` |
| `validate_non_empty` | 13092–13097 | `crates/vulcan-service-kernel/src/public_ai_settings.rs:336` | 保留原根；严格等价 | `private`→`private` |
| `validate_custom_model_protocol_preferences` | 13113–13140 | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:1264` | 按职责迁移；严格等价 | `private`→`private` |

## 精确格式差异

两个生产函数仅允许实名声明泛型序列在大于号前增加一个尾逗号。连接请求测试仅允许以下5个完整反序列化表达式分支调整花括号及必要逗号：accepted的add/update/oauth，rejected的add/oauth。每个完整分支必须唯一匹配指定请求类型、输入和map表达式，再比较函数全部token；无语句、控制流、断言或忽略属性改变。不得推广为忽略任意泛型、花括号或表达式。

| 节点 | 原token→当前token | 已审核的唯一差异 |
| --- | ---: | --- |
| `mutate_public_ai_configuration_and_publish_in_transaction` | 129→130 | 声明泛型列表末尾增加一个rustfmt逗号，函数体不变 |
| `publish_public_ai_workbench_with_session_reconciliation_and_commit` | 132→133 | 声明泛型列表末尾增加一个rustfmt逗号，函数体不变 |
| `connection_requests_reject_user_selected_protocol` | 389→388 | 5个指定serde单表达式match分支花括号调整；完整表达式及断言不变 |

## 接口与可见性

- 原127个crate可见顶层项均保留；其中124个迁移项通过原根明确重导出保持路径，其他仍在原根。
- 原根pub(super)与子模块pub(crate)同为crate范围；原pub(crate)入口继续同域重导出。原私有助手移到子模块pub(super)仍限设置域，没有新增对外API。
- 158处节点声明、31处字段调整已按明确字段名逐token核对；原未调整可见性及字段类型、名称、顺序、属性保持不变。
- 目录投影、总快照、通用设置、媒体候选及原十组共十四组类型重导出仅为保留原契约路径使用带原因的expect(unused_imports)，不对其他实现或导入压制警告；不得为了消除提示扩大可见性或伪造调用。

| 迁移的原顶层项 | 当前定义 | 原根入口 |
| --- | --- | --- |
| `PublicAiSettingsSnapshot` | `crates/vulcan-service-kernel/src/public_ai_settings/snapshot.rs:40` | 原名与原crate范围重导出 |
| `PublicAiCustomModelProtocolPreferenceRecord` | `crates/vulcan-service-kernel/src/public_ai_settings/snapshot.rs:192` | 原名与原crate范围重导出 |
| `PublicAiCustomModelTemplateDecisionRecord` | `crates/vulcan-service-kernel/src/public_ai_settings/snapshot.rs:208` | 原名与原crate范围重导出 |
| `PublicAiCustomModelConnectionAvailabilityRecord` | `crates/vulcan-service-kernel/src/public_ai_settings/snapshot.rs:224` | 原名与原crate范围重导出 |
| `PublicAiModelDirectoryKind` | `crates/vulcan-service-kernel/src/public_ai_settings/catalog_projection.rs:15` | 原名与原crate范围重导出 |
| `PublicAiConnectionModelAccessPolicyRecord` | `crates/vulcan-service-kernel/src/public_ai_settings/catalog_projection.rs:34` | 原名与原crate范围重导出 |
| `PublicAiInferenceRouteDiagnostic` | `crates/vulcan-service-kernel/src/public_ai_settings/inference_settings.rs:32` | 原名与原crate范围重导出 |
| `PublicAiInternalInferenceCandidate` | `crates/vulcan-service-kernel/src/public_ai_settings/inference_settings.rs:54` | 原名与原crate范围重导出 |
| `PublicAiProviderToolTargetSnapshot` | `crates/vulcan-service-kernel/src/public_ai_settings/snapshot.rs:240` | 原名与原crate范围重导出 |
| `PublicAiMultimodalUnderstandingCandidateRecord` | `crates/vulcan-service-kernel/src/public_ai_settings/snapshot.rs:259` | 原名与原crate范围重导出 |
| `PublicAiMediaUnderstandingModelCandidate` | `crates/vulcan-service-kernel/src/public_ai_settings/media_candidates.rs:20` | 原名与原crate范围重导出 |
| `PublicAiSessionProviderToolConflict` | `crates/vulcan-service-kernel/src/public_ai_settings/session_tools.rs:37` | 原名与原crate范围重导出 |
| `PublicAiSessionProviderToolSettingsSnapshot` | `crates/vulcan-service-kernel/src/public_ai_settings/session_tools.rs:53` | 原名与原crate范围重导出 |
| `provider_tool_selection_config_from_parts_v1` | `crates/vulcan-service-kernel/src/public_ai_settings/session_tools.rs:134` | 原名与原crate范围重导出 |
| `PublicAiConnectionSnapshot` | `crates/vulcan-service-kernel/src/public_ai_settings/snapshot.rs:271` | 原名与原crate范围重导出 |
| `PublicAiModelRouteRecord` | `crates/vulcan-service-kernel/src/public_ai_settings/catalog_projection.rs:53` | 原名与原crate范围重导出 |
| `PublicAiModelOverrideRecord` | `crates/vulcan-service-kernel/src/public_ai_settings/snapshot.rs:383` | 原名与原crate范围重导出 |
| `PublicAiModelAccessRecord` | `crates/vulcan-service-kernel/src/public_ai_settings/snapshot.rs:402` | 原名与原crate范围重导出 |
| `PublicAiModelCapabilityOverrideRecord` | `crates/vulcan-service-kernel/src/public_ai_settings/snapshot.rs:424` | 原名与原crate范围重导出 |
| `PublicAiConnectionAddRequest` | `crates/vulcan-service-kernel/src/public_ai_settings/connections.rs:33` | 原名与原crate范围重导出 |
| `PublicAiConnectionUpdateRequest` | `crates/vulcan-service-kernel/src/public_ai_settings/connections.rs:64` | 原名与原crate范围重导出 |
| `PublicAiUsageRefreshSettingsRequest` | `crates/vulcan-service-kernel/src/public_ai_settings/general_settings.rs:24` | 原名与原crate范围重导出 |
| `PublicAiProviderPrioritySaveRequest` | `crates/vulcan-service-kernel/src/public_ai_settings/general_settings.rs:34` | 原名与原crate范围重导出 |
| `PublicAiContextWindowPolicyOverridesSaveRequest` | `crates/vulcan-service-kernel/src/public_ai_settings/general_settings.rs:47` | 原名与原crate范围重导出 |
| `PublicAiContextCompactionWindowNodesSaveRequest` | `crates/vulcan-service-kernel/src/public_ai_settings/general_settings.rs:57` | 原名与原crate范围重导出 |
| `PublicAiModelSelectionRequest` | `crates/vulcan-service-kernel/src/public_ai_settings/model_selection.rs:27` | 原名与原crate范围重导出 |
| `PublicAiModelOverridesSaveRequest` | `crates/vulcan-service-kernel/src/public_ai_settings/model_settings.rs:87` | 原名与原crate范围重导出 |
| `PublicAiModelEnabledSaveRequest` | `crates/vulcan-service-kernel/src/public_ai_settings/model_settings.rs:106` | 原名与原crate范围重导出 |
| `PublicAiInternalModelEnabledSaveRequest` | `crates/vulcan-service-kernel/src/public_ai_settings/inference_settings.rs:91` | 原名与原crate范围重导出 |
| `PublicAiInferenceRouteSaveRequest` | `crates/vulcan-service-kernel/src/public_ai_settings/inference_settings.rs:107` | 原名与原crate范围重导出 |
| `PublicAiInferenceTestRequest` | `crates/vulcan-service-kernel/src/public_ai_settings/inference_testing.rs:26` | 原名与原crate范围重导出 |
| `PublicAiInferenceTestResult` | `crates/vulcan-service-kernel/src/public_ai_settings/inference_testing.rs:39` | 原名与原crate范围重导出 |
| `PublicAiInternalModelOverridesSaveRequest` | `crates/vulcan-service-kernel/src/public_ai_settings/inference_settings.rs:127` | 原名与原crate范围重导出 |
| `PublicAiProviderToolSelectionSaveRequest` | `crates/vulcan-service-kernel/src/public_ai_settings/provider_tools.rs:29` | 原名与原crate范围重导出 |
| `PublicAiMultimodalUnderstandingCandidateSaveRequest` | `crates/vulcan-service-kernel/src/public_ai_settings/provider_tools.rs:42` | 原名与原crate范围重导出 |
| `PublicAiMediaUnderstandingSelectionSaveRequest` | `crates/vulcan-service-kernel/src/public_ai_settings/provider_tools.rs:58` | 原名与原crate范围重导出 |
| `PublicAiProviderExtensionTargetSetSaveRequest` | `crates/vulcan-service-kernel/src/public_ai_settings/provider_tools.rs:71` | 原名与原crate范围重导出 |
| `PublicAiProviderToolTestRequest` | `crates/vulcan-service-kernel/src/public_ai_settings/provider_tools.rs:84` | 原名与原crate范围重导出 |
| `PublicAiProviderToolCapabilityEnabledSaveRequest` | `crates/vulcan-service-kernel/src/public_ai_settings/provider_tools.rs:100` | 原名与原crate范围重导出 |
| `PublicAiSessionProviderToolSelectionsSaveRequest` | `crates/vulcan-service-kernel/src/public_ai_settings/session_tools.rs:77` | 原名与原crate范围重导出 |
| `PublicAiDefaultConnectionSaveRequest` | `crates/vulcan-service-kernel/src/public_ai_settings/connections.rs:99` | 原名与原crate范围重导出 |
| `PublicAiConnectionRemoveRequest` | `crates/vulcan-service-kernel/src/public_ai_settings/connections.rs:112` | 原名与原crate范围重导出 |
| `PublicAiCustomProviderSaveRequest` | `crates/vulcan-service-kernel/src/public_ai_settings/custom_providers.rs:38` | 原名与原crate范围重导出 |
| `PublicAiCustomProviderProbeResult` | `crates/vulcan-service-kernel/src/public_ai_settings/custom_providers.rs:108` | 原名与原crate范围重导出 |
| `PublicAiCustomModelValidationRequest` | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:34` | 原名与原crate范围重导出 |
| `PublicAiCustomModelValidationResult` | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:50` | 原名与原crate范围重导出 |
| `PublicAiCustomProviderRemoveRequest` | `crates/vulcan-service-kernel/src/public_ai_settings/custom_providers.rs:127` | 原名与原crate范围重导出 |
| `PublicAiCustomModelsSaveRequest` | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:72` | 原名与原crate范围重导出 |
| `PublicAiCustomModelProtocolPreferenceItem` | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:89` | 原名与原crate范围重导出 |
| `PublicAiCustomModelProtocolPreferencesSaveRequest` | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:102` | 原名与原crate范围重导出 |
| `PublicAiCustomProviderModelDefaultsSaveRequest` | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:115` | 原名与原crate范围重导出 |
| `PublicAiCustomModelTemplateDecisionSaveRequest` | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:128` | 原名与原crate范围重导出 |
| `PublicAiCustomModelTemplateSuggestionsRequest` | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:144` | 原名与原crate范围重导出 |
| `PublicAiCustomModelTemplateSuggestionsBatchRequest` | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:154` | 原名与原crate范围重导出 |
| `PublicAiCustomModelTemplateSuggestionsRecord` | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:164` | 原名与原crate范围重导出 |
| `PublicAiCustomModelTemplateDecisionBatchItem` | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:177` | 原名与原crate范围重导出 |
| `PublicAiCustomModelTemplateDecisionsBatchSaveRequest` | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:190` | 原名与原crate范围重导出 |
| `PublicAiCustomModelConnectionAvailabilitySaveRequest` | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:203` | 原名与原crate范围重导出 |
| `PublicAiConnectionModelEnabledSaveRequest` | `crates/vulcan-service-kernel/src/public_ai_settings/model_settings.rs:29` | 原名与原crate范围重导出 |
| `PublicAiConnectionModelsEnabledSaveRequest` | `crates/vulcan-service-kernel/src/public_ai_settings/model_settings.rs:45` | 原名与原crate范围重导出 |
| `PublicAiModelCapabilityOverridesSaveRequest` | `crates/vulcan-service-kernel/src/public_ai_settings/model_settings.rs:58` | 原名与原crate范围重导出 |
| `PublicAiProviderCapabilityOverridesSaveRequest` | `crates/vulcan-service-kernel/src/public_ai_settings/model_settings.rs:74` | 原名与原crate范围重导出 |
| `service_probe_public_ai_custom_provider` | `crates/vulcan-service-kernel/src/public_ai_settings/custom_providers.rs:992` | 原名与原crate范围重导出 |
| `service_save_public_ai_custom_provider` | `crates/vulcan-service-kernel/src/public_ai_settings/custom_providers.rs:1022` | 原名与原crate范围重导出 |
| `service_validate_public_ai_custom_model` | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:228` | 原名与原crate范围重导出 |
| `service_remove_public_ai_custom_provider` | `crates/vulcan-service-kernel/src/public_ai_settings/custom_providers.rs:1424` | 原名与原crate范围重导出 |
| `service_save_public_ai_custom_models` | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:383` | 原名与原crate范围重导出 |
| `service_save_public_ai_custom_model_protocol_preferences` | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:557` | 原名与原crate范围重导出 |
| `service_save_public_ai_custom_provider_model_defaults` | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:676` | 原名与原crate范围重导出 |
| `service_public_ai_custom_model_template_suggestions` | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:774` | 原名与原crate范围重导出 |
| `service_public_ai_custom_model_template_suggestions_batch` | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:795` | 原名与原crate范围重导出 |
| `service_save_public_ai_custom_model_template_decision` | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:837` | 原名与原crate范围重导出 |
| `service_save_public_ai_custom_model_template_decisions_batch` | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:910` | 原名与原crate范围重导出 |
| `service_save_public_ai_custom_model_connection_availability` | `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:993` | 原名与原crate范围重导出 |
| `service_save_public_ai_connection_model_enabled` | `crates/vulcan-service-kernel/src/public_ai_settings/model_settings.rs:214` | 原名与原crate范围重导出 |
| `service_save_public_ai_connection_models_enabled` | `crates/vulcan-service-kernel/src/public_ai_settings/model_settings.rs:329` | 原名与原crate范围重导出 |
| `service_save_public_ai_model_capability_overrides` | `crates/vulcan-service-kernel/src/public_ai_settings/model_settings.rs:446` | 原名与原crate范围重导出 |
| `service_save_public_ai_provider_capability_overrides` | `crates/vulcan-service-kernel/src/public_ai_settings/model_settings.rs:518` | 原名与原crate范围重导出 |
| `PublicAiOAuthStartRequest` | `crates/vulcan-service-kernel/src/public_ai_settings/oauth.rs:29` | 原名与原crate范围重导出 |
| `PublicAiOAuthCommitRequest` | `crates/vulcan-service-kernel/src/public_ai_settings/oauth.rs:49` | 原名与原crate范围重导出 |
| `service_start_public_ai_oauth` | `crates/vulcan-service-kernel/src/public_ai_settings/oauth.rs:83` | 原名与原crate范围重导出 |
| `service_public_ai_oauth_status` | `crates/vulcan-service-kernel/src/public_ai_settings/oauth.rs:167` | 原名与原crate范围重导出 |
| `service_cancel_public_ai_oauth` | `crates/vulcan-service-kernel/src/public_ai_settings/oauth.rs:188` | 原名与原crate范围重导出 |
| `service_commit_public_ai_oauth_connection` | `crates/vulcan-service-kernel/src/public_ai_settings/oauth.rs:209` | 原名与原crate范围重导出 |
| `service_add_public_ai_connection` | `crates/vulcan-service-kernel/src/public_ai_settings/connections.rs:133` | 原名与原crate范围重导出 |
| `service_update_public_ai_connection` | `crates/vulcan-service-kernel/src/public_ai_settings/connections.rs:627` | 原名与原crate范围重导出 |
| `service_save_public_ai_default_connection` | `crates/vulcan-service-kernel/src/public_ai_settings/connections.rs:1286` | 原名与原crate范围重导出 |
| `service_remove_public_ai_connection` | `crates/vulcan-service-kernel/src/public_ai_settings/connections.rs:1350` | 原名与原crate范围重导出 |
| `service_sync_public_ai_connection` | `crates/vulcan-service-kernel/src/public_ai_settings/connections.rs:1502` | 原名与原crate范围重导出 |
| `service_sync_provider_manifests` | `crates/vulcan-service-kernel/src/public_ai_settings/provider_synchronization.rs:565` | 原名与原crate范围重导出 |
| `service_get_provider_manifest_status` | `crates/vulcan-service-kernel/src/public_ai_settings/provider_synchronization.rs:663` | 原名与原crate范围重导出 |
| `service_preview_provider_manifest` | `crates/vulcan-service-kernel/src/public_ai_settings/provider_synchronization.rs:702` | 原名与原crate范围重导出 |
| `service_rollback_provider_manifest` | `crates/vulcan-service-kernel/src/public_ai_settings/provider_synchronization.rs:758` | 原名与原crate范围重导出 |
| `service_sync_provider_manifest` | `crates/vulcan-service-kernel/src/public_ai_settings/provider_synchronization.rs:832` | 原名与原crate范围重导出 |
| `service_query_public_ai_connection_usage` | `crates/vulcan-service-kernel/src/public_ai_settings/connections.rs:1621` | 原名与原crate范围重导出 |
| `service_save_public_ai_usage_refresh_settings` | `crates/vulcan-service-kernel/src/public_ai_settings/general_settings.rs:75` | 原名与原crate范围重导出 |
| `service_save_public_ai_provider_priority` | `crates/vulcan-service-kernel/src/public_ai_settings/general_settings.rs:121` | 原名与原crate范围重导出 |
| `service_save_public_ai_context_window_policy_overrides` | `crates/vulcan-service-kernel/src/public_ai_settings/general_settings.rs:180` | 原名与原crate范围重导出 |
| `service_save_public_ai_context_compaction_window_nodes` | `crates/vulcan-service-kernel/src/public_ai_settings/general_settings.rs:216` | 原名与原crate范围重导出 |
| `service_select_public_ai_model` | `crates/vulcan-service-kernel/src/public_ai_settings/model_selection.rs:112` | 原名与原crate范围重导出 |
| `service_save_public_ai_model_overrides` | `crates/vulcan-service-kernel/src/public_ai_settings/model_settings.rs:571` | 原名与原crate范围重导出 |
| `service_save_public_ai_model_enabled` | `crates/vulcan-service-kernel/src/public_ai_settings/model_settings.rs:634` | 原名与原crate范围重导出 |
| `service_save_public_ai_internal_model_enabled` | `crates/vulcan-service-kernel/src/public_ai_settings/inference_settings.rs:154` | 原名与原crate范围重导出 |
| `service_save_public_ai_internal_model_overrides` | `crates/vulcan-service-kernel/src/public_ai_settings/inference_settings.rs:210` | 原名与原crate范围重导出 |
| `service_save_public_ai_inference_route` | `crates/vulcan-service-kernel/src/public_ai_settings/inference_settings.rs:295` | 原名与原crate范围重导出 |
| `service_test_public_ai_inference_route` | `crates/vulcan-service-kernel/src/public_ai_settings/inference_testing.rs:79` | 原名与原crate范围重导出 |
| `service_save_public_ai_provider_tool_selection` | `crates/vulcan-service-kernel/src/public_ai_settings/provider_tools.rs:121` | 原名与原crate范围重导出 |
| `service_save_public_ai_multimodal_understanding_candidate` | `crates/vulcan-service-kernel/src/public_ai_settings/provider_tools.rs:192` | 原名与原crate范围重导出 |
| `service_save_public_ai_media_understanding_selection` | `crates/vulcan-service-kernel/src/public_ai_settings/provider_tools.rs:261` | 原名与原crate范围重导出 |
| `service_save_public_ai_provider_extension_target_set` | `crates/vulcan-service-kernel/src/public_ai_settings/provider_tools.rs:402` | 原名与原crate范围重导出 |
| `service_test_public_ai_provider_tool` | `crates/vulcan-service-kernel/src/public_ai_settings/provider_tools.rs:453` | 原名与原crate范围重导出 |
| `service_save_public_ai_provider_tool_capability_enabled` | `crates/vulcan-service-kernel/src/public_ai_settings/provider_tools.rs:490` | 原名与原crate范围重导出 |
| `service_public_ai_session_provider_tool_selections` | `crates/vulcan-service-kernel/src/public_ai_settings/session_tools.rs:245` | 原名与原crate范围重导出 |
| `service_rebuild_public_ai_session_provider_tool_selections` | `crates/vulcan-service-kernel/src/public_ai_settings/session_tools.rs:384` | 原名与原crate范围重导出 |
| `service_save_public_ai_session_provider_tool_selections` | `crates/vulcan-service-kernel/src/public_ai_settings/session_tools.rs:629` | 原名与原crate范围重导出 |
| `validate_provider_tool_session_settings` | `crates/vulcan-service-kernel/src/public_ai_settings/session_tools.rs:715` | 原名与原crate范围重导出 |
| `session_media_understanding_modes_from_configuration` | `crates/vulcan-service-kernel/src/public_ai_settings/session_tools.rs:921` | 原名与原crate范围重导出 |
| `provider_tool_capability_key` | `crates/vulcan-service-kernel/src/public_ai_settings/session_tools.rs:1133` | 原名与原crate范围重导出 |
| `public_ai_session_media_understanding_bridge_kinds` | `crates/vulcan-service-kernel/src/public_ai_settings/session_tools.rs:1157` | 原名与原crate范围重导出 |
| `reconcile_missing_session_model_selections` | `crates/vulcan-service-kernel/src/public_ai_settings/model_selection.rs:592` | 原名与原crate范围重导出 |
| `resolve_knowledge_internal_inference_candidate` | `crates/vulcan-service-kernel/src/public_ai_settings/inference_settings.rs:688` | 原名与原crate范围重导出 |
| `sync_knowledge_inference_profile` | `crates/vulcan-service-kernel/src/public_ai_settings/inference_settings.rs:744` | 原名与原crate范围重导出 |
| `remove_knowledge_inference_profile` | `crates/vulcan-service-kernel/src/public_ai_settings/inference_settings.rs:816` | 原名与原crate范围重导出 |
| `publish_public_ai_workbench` | `crates/vulcan-service-kernel/src/public_ai_settings/publication.rs:467` | 原名与原crate范围重导出 |

## 字段与常量

- 媒体候选七字段向原设置域开放以保持候选选择和跨层构造；连接更新九字段供OAuth原样构造；工作台协调仅changed_session_ids与revoked_session_ids供原会话选择回归读取。
- 内部推理候选的route/catalog_generation/enabled/capabilities四字段从原根pub(super)改为子模块pub(crate)，保留原crate范围；该类型其他六字段仍私有。
- 目录路由记录九字段在推理及快照跨域回归中实际使用，改为pub(super)保持设置域；connection_id/service_entry_id仍私有。首次编译指出快照回归还需要provider_id/model_id，已按源码补齐，不扩大到crate。
- 知识库用途常量原根pub(super)改为inference_settings中pub(crate)，原根同名pub(super)重导出；常量值、唯一归属及声明范围分别精确核对。
- 工作台预算失效集合及取消守卫字段保持私有；协调类型与三个构造方法、取消守卫与new/disarm/Drop整体归位。71个原类型均保留。

| 类型 | 调整字段 | 实际边界 |
| --- | --- | --- |
| `PublicAiInternalInferenceCandidate` | `route` | 原根pub(super)→子模块pub(crate)，仍为crate范围 |
| `PublicAiInternalInferenceCandidate` | `catalog_generation` | 原根pub(super)→子模块pub(crate)，仍为crate范围 |
| `PublicAiInternalInferenceCandidate` | `enabled` | 原根pub(super)→子模块pub(crate)，仍为crate范围 |
| `PublicAiInternalInferenceCandidate` | `capabilities` | 原根pub(super)→子模块pub(crate)，仍为crate范围 |
| `PublicAiMediaUnderstandingModelCandidate` | `provider_display_name` | 原设置域私有→子模块pub(super)，仍限设置域 |
| `PublicAiMediaUnderstandingModelCandidate` | `service_entry_display_name` | 原设置域私有→子模块pub(super)，仍限设置域 |
| `PublicAiMediaUnderstandingModelCandidate` | `connection_display_name` | 原设置域私有→子模块pub(super)，仍限设置域 |
| `PublicAiMediaUnderstandingModelCandidate` | `model_display_name` | 原设置域私有→子模块pub(super)，仍限设置域 |
| `PublicAiMediaUnderstandingModelCandidate` | `selection` | 原设置域私有→子模块pub(super)，仍限设置域 |
| `PublicAiMediaUnderstandingModelCandidate` | `catalog_generation` | 原设置域私有→子模块pub(super)，仍限设置域 |
| `PublicAiMediaUnderstandingModelCandidate` | `capabilities` | 原设置域私有→子模块pub(super)，仍限设置域 |
| `PublicAiModelRouteRecord` | `protocol_id` | 原设置域私有→子模块pub(super)，仍限设置域 |
| `PublicAiModelRouteRecord` | `available_protocols` | 原设置域私有→子模块pub(super)，仍限设置域 |
| `PublicAiModelRouteRecord` | `binding_source` | 原设置域私有→子模块pub(super)，仍限设置域 |
| `PublicAiModelRouteRecord` | `binding_rule_id` | 原设置域私有→子模块pub(super)，仍限设置域 |
| `PublicAiModelRouteRecord` | `manifest_revision` | 原设置域私有→子模块pub(super)，仍限设置域 |
| `PublicAiModelRouteRecord` | `executable` | 原设置域私有→子模块pub(super)，仍限设置域 |
| `PublicAiModelRouteRecord` | `unavailable_reason` | 原设置域私有→子模块pub(super)，仍限设置域 |
| `PublicAiModelRouteRecord` | `provider_id` | 原设置域私有→子模块pub(super)，仍限设置域 |
| `PublicAiModelRouteRecord` | `model_id` | 原设置域私有→子模块pub(super)，仍限设置域 |
| `PublicAiConnectionUpdateRequest` | `connection_id` | 原设置域私有→子模块pub(super)，仍限设置域 |
| `PublicAiConnectionUpdateRequest` | `display_name` | 原设置域私有→子模块pub(super)，仍限设置域 |
| `PublicAiConnectionUpdateRequest` | `service_entry_id` | 原设置域私有→子模块pub(super)，仍限设置域 |
| `PublicAiConnectionUpdateRequest` | `credential_profile_id` | 原设置域私有→子模块pub(super)，仍限设置域 |
| `PublicAiConnectionUpdateRequest` | `credential_mode` | 原设置域私有→子模块pub(super)，仍限设置域 |
| `PublicAiConnectionUpdateRequest` | `secret` | 原设置域私有→子模块pub(super)，仍限设置域 |
| `PublicAiConnectionUpdateRequest` | `management_key` | 原设置域私有→子模块pub(super)，仍限设置域 |
| `PublicAiConnectionUpdateRequest` | `remove_management_key` | 原设置域私有→子模块pub(super)，仍限设置域 |
| `PublicAiConnectionUpdateRequest` | `service_origin_override` | 原设置域私有→子模块pub(super)，仍限设置域 |
| `PublicAiWorkspaceReconciliation` | `changed_session_ids` | 原设置域私有→子模块pub(super)，仍限设置域 |
| `PublicAiWorkspaceReconciliation` | `revoked_session_ids` | 原设置域私有→子模块pub(super)，仍限设置域 |

- 常量 `KNOWLEDGE_CURATION_PURPOSE_ID`：原64行→`crates/vulcan-service-kernel/src/public_ai_settings/inference_settings.rs:26`，唯一且词法相同。
- 常量 `MAX_CUSTOM_MODEL_TEMPLATE_BATCH_SIZE`：原1417行→`crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs:28`，唯一且词法相同。

## 本检查点产物摘要

| Rust产物 | 行数 | SHA-256 |
| --- | ---: | --- |
| `crates/vulcan-service-kernel/src/public_ai_settings.rs` | 341 | `711d7776f7f7321f3d76d8f23ad3522d72a90dc27f575dd4157ed2983e565217` |
| `crates/vulcan-service-kernel/src/public_ai_settings/snapshot.rs` | 815 | `3ac7dec127823e8a25875966fe11947942a0caba1643a3e8fb84ae44da0ed61a` |
| `crates/vulcan-service-kernel/src/public_ai_settings/snapshot/tests.rs` | 153 | `f78f862322879905affbc52f7e9f63ab3e11a9e1309ec98df7245de4666c398f` |
| `crates/vulcan-service-kernel/src/public_ai_settings/session_tools.rs` | 1223 | `1b3a6499658a254e582b0b627c10c46ecb5a3058c82fb3066a4ef72600169127` |
| `crates/vulcan-service-kernel/src/public_ai_settings/session_tools/tests.rs` | 358 | `efaee9f720ba03aefd239b5db61dcb2c83f5ea88b9565d8f6239221e1e455955` |
| `crates/vulcan-service-kernel/src/public_ai_settings/publication.rs` | 950 | `79271646ba0d38603f9bdae7c2a5aba6716ba28de3e88b950c3f2b8c8a20c325` |
| `crates/vulcan-service-kernel/src/public_ai_settings/publication/tests.rs` | 112 | `376f82d10ace12555cfffc25929e3587f033487e7c696f472dc8a222175deda3` |
| `crates/vulcan-service-kernel/src/public_ai_settings/provider_tools.rs` | 740 | `22fddcb40be0091e71837b5cc6ff6b232d336f1e69b43b75aab6648f34fc6e87` |
| `crates/vulcan-service-kernel/src/public_ai_settings/provider_tools/tests.rs` | 67 | `d208a1c1679fb96f4fc036985fbacf271f42c97fe689321b9c69aff0605633a2` |
| `crates/vulcan-service-kernel/src/public_ai_settings/provider_synchronization.rs` | 885 | `0ff9b6f6a68016b1feda835ad6010ce92aa70e08ffaa39d4561d35db30f27ccc` |
| `crates/vulcan-service-kernel/src/public_ai_settings/provider_synchronization/tests.rs` | 209 | `3edeeb0b64541853253a6949036fa8c3d5ef5d814bb7a48d32eb4b860424d8e4` |
| `crates/vulcan-service-kernel/src/public_ai_settings/oauth.rs` | 482 | `6f125025bfa6b1d00f0b9387d086411e1dbc52a1ca62617d3c33b17bd19abda5` |
| `crates/vulcan-service-kernel/src/public_ai_settings/model_settings.rs` | 755 | `4a0e5904570dbc528a54e16a600d50af952c3d266e48149e638598cac396f5cf` |
| `crates/vulcan-service-kernel/src/public_ai_settings/model_selection.rs` | 815 | `868d7fa255a4dfc378252e0210edeeba5b753a34191ea224ccb6cdcc756a9dda` |
| `crates/vulcan-service-kernel/src/public_ai_settings/model_selection/tests.rs` | 395 | `8140771d35251427fd912fbd17c99bc9d0604c97fc853fd7e91bb9d5d3c306ca` |
| `crates/vulcan-service-kernel/src/public_ai_settings/media_candidates.rs` | 232 | `15ea5db03817301050a640e49425dda67b8a60e9b7e88ea8e4f91d4ec66d1604` |
| `crates/vulcan-service-kernel/src/public_ai_settings/media_candidates/tests.rs` | 101 | `2682d49074442981f6e19c213c5b1835d8e010693f346f84401123e95b784fee` |
| `crates/vulcan-service-kernel/src/public_ai_settings/inference_validation.rs` | 370 | `3cb2f8c66b271d407363ee370238cf892f01e017b44a6c63930cdad5a605cd91` |
| `crates/vulcan-service-kernel/src/public_ai_settings/custom_providers.rs` | 1712 | `149d4eda8190a64d80ccc6dfb5e4960af924a41e972a869ddb58e729216fc945` |
| `crates/vulcan-service-kernel/src/public_ai_settings/general_settings.rs` | 251 | `b6553f8b8dc66d04924930039e01938c60db0f8ec64d8874fbdcdc8e55b71af7` |
| `crates/vulcan-service-kernel/src/public_ai_settings/inference_validation/tests.rs` | 55 | `60cc6c46d926f484d287f6579f7d6df1d90bf77af806cd1c6f59483bf1f9ae6b` |
| `crates/vulcan-service-kernel/src/public_ai_settings/inference_testing.rs` | 626 | `40c554c1db1f3965af4ea6a5845cfd043c933ca1a8f63fe8a0ca50508ceaf7b0` |
| `crates/vulcan-service-kernel/src/public_ai_settings/inference_settings.rs` | 843 | `e43799738f685e21b97ddf88f686c8371fa3b1c8e21a28e0b9932445cb027df1` |
| `crates/vulcan-service-kernel/src/public_ai_settings/catalog_projection/tests.rs` | 225 | `a4bde48b28d2cfb8eda16823ec5211c260138f44d1c7333ac4566a53bb315d60` |
| `crates/vulcan-service-kernel/src/public_ai_settings/catalog_projection.rs` | 442 | `d13432ab1cd9722c96e0d4431ecebb129e5294fa20d1ea86d797a743b26df850` |
| `crates/vulcan-service-kernel/src/public_ai_settings/inference_testing/tests.rs` | 87 | `a58fae1d8adb8e1c1ea6908eb5d269c522c5b3d033622f0a85934fbbca9e4d49` |
| `crates/vulcan-service-kernel/src/public_ai_settings/custom_models.rs` | 1294 | `cdc183d1fcc6cdc9720dd41be7cadc745756484a08b7d7bf5fdfc3e5356a0b01` |
| `crates/vulcan-service-kernel/src/public_ai_settings/connections.rs` | 1786 | `e7633100400e27ca9e7cfa360dbe197f8e2c44f57946b194b88688027daa81ab` |
| `crates/vulcan-service-kernel/src/public_ai_settings/custom_providers/tests.rs` | 863 | `b6f8e15e5d30ef465c53d4dda09c3763f6739f456646975f8e998f38ed3f6f6a` |
| `crates/vulcan-service-kernel/src/public_ai_settings/connections/tests.rs` | 205 | `57db7f0d2de80ae87844c26699c1e9a1b2c1378c95a64fb9e7e42df85c6c4871` |
| `crates/vulcan-service-kernel/src/public_ai_settings/inference_settings/tests.rs` | 215 | `e69979d4dceb267749dd44f45fa9cb1ec8a17417fedb23baddcea4f0bc412556` |
| `crates/vulcan-service-kernel/src/public_ai_settings/custom_models/tests.rs` | 103 | `e543cd6c81134cef4507342a4fd3b99ac2c970d658adabf7512740e055b5c568` |
