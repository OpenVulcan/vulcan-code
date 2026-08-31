# 13 号客户端生产符号迁移记录

基线提交：`111540689b777ddc1229a6f17293a42818f2155e`。原文件：`crates/vulcan-provider-adapters/src/client.rs`。

## 完整性和可见性

- 原生产区 117 个类型／trait／函数／方法节点全部保留（包含原 `#[cfg(test)]` 块追加测试适配器）；原测试区 41 个测试与 6 个夹具函数全部保留。共 164 个原符号无新增、无删除。
- 逐符号比较保留字符串字面量，忽略注释、布局、可选尾逗号及已记录的可见性调整，行为差异 0；多行原始 JSON 字符串未因取消外层缩进而改变内容。
- 根文件保留公开 trait、发现错误类型、客户端类型、两个构造器、代际查询和统一 trait 操作编排；私有字段及唯一共享目录句柄不变。
- `MiniMaxKeyRegistration` 在私有注册模块定义，再由原 `client` 路径公开重导出；`lib.rs` 的原根导出保持不动，外部使用路径和公开签名不变。
- 模型发现模块仅把根编排／注册模块实际调用的 3 个函数及 2 个客户端方法开放到 `pub(super)`；其余分页与解析内部函数仍私有。
- HTTP 模块只将跨职责复用的请求超时、读取、鉴权 GET、结构解析和诊断函数设为 `pub(super)`；有界读取错误及块追加器仍私有。
- 六项用量查询限定为 `pub(in crate::client)`，通过 `account_usage` 的 `pub(super)` 导出供根调度；响应 DTO、计费规则与字段均不公开。共享十进制函数只对 `account_usage` 子树可见。
- 二级路由模块只开放查询入口到父域，响应结构和投影函数保持私有；没有为测试放大生产字段可见性。
- 模块依赖：发现和二级路由依赖 HTTP；注册依赖 HTTP 和发现的目录边界；各用量协议依赖 HTTP、共用观测时钟及按需的十进制运算。不存在兄弟模块循环。

## 逐符号映射

| 符号 | 原行 | 新文件 | 新行 |
| --- | ---: | --- | ---: |
| `manifest_protocol_authorizes_model_variant` | 57 | `client/model_discovery.rs` | 42 |
| `enum:BoundedResponseReadError` | 87 | `client/http.rs` | 24 |
| `BoundedResponseReadError::into_model_discovery_error` | 104 | `client/http.rs` | 41 |
| `enum:ModelDiscoveryFailureKind` | 117 | `client.rs` | 38 |
| `struct:ModelDiscoveryError` | 141 | `client.rs` | 62 |
| `ModelDiscoveryError::new` | 163 | `client.rs` | 84 |
| `ModelDiscoveryError::invalid_response` | 182 | `client.rs` | 103 |
| `ModelDiscoveryError::failed` | 198 | `client.rs` | 119 |
| `ModelDiscoveryError::from_http_status` | 214 | `client.rs` | 135 |
| `std::fmt::Display for ModelDiscoveryError::fmt` | 230 | `client.rs` | 151 |
| `trait:ProviderMetadataClient` | 240 | `client.rs` | 161 |
| `trait ProviderMetadataClient::bundled_manifest` | 253 | `client.rs` | 174 |
| `struct:HttpProviderMetadataClient` | 331 | `client.rs` | 252 |
| `struct:MiniMaxKeyRegistration` | 347 | `client/minimax_registration.rs` | 12 |
| `HttpProviderMetadataClient::new` | 369 | `client.rs` | 278 |
| `HttpProviderMetadataClient::with_bundled_manifests` | 392 | `client.rs` | 301 |
| `HttpProviderMetadataClient::register_minimax_key` | 416 | `client/minimax_registration.rs` | 34 |
| `resolve_minimax_region` | 442 | `client/minimax_registration.rs` | 60 |
| `minimax_key_profile_id` | 470 | `client/minimax_registration.rs` | 88 |
| `probe_minimax_region` | 494 | `client/minimax_registration.rs` | 112 |
| `route_is_available_for_discovery` | 534 | `client.rs` | 322 |
| `ProviderMetadataClient for HttpProviderMetadataClient::discover_models` | 540 | `client.rs` | 328 |
| `ProviderMetadataClient for HttpProviderMetadataClient::query_account_usage` | 693 | `client.rs` | 481 |
| `ProviderMetadataClient for HttpProviderMetadataClient::discover_secondary_routes` | 770 | `client.rs` | 558 |
| `ProviderMetadataClient for HttpProviderMetadataClient::bundled_manifest` | 787 | `client.rs` | 575 |
| `HttpProviderMetadataClient::catalog_generation` | 807 | `client.rs` | 595 |
| `HttpProviderMetadataClient::discover_for_route` | 816 | `client/model_discovery.rs` | 72 |
| `HttpProviderMetadataClient::enrich_catalog_for_protocol` | 968 | `client/model_discovery.rs` | 224 |
| `build_manifest_static_catalog` | 1013 | `client/model_discovery.rs` | 269 |
| `default_discovery_path` | 1063 | `client/model_discovery.rs` | 319 |
| `discover_google_code_assist_models` | 1090 | `client/model_discovery.rs` | 346 |
| `send_metadata_get` | 1150 | `client/http.rs` | 53 |
| `send_metadata_get_with_authentication` | 1176 | `client/http.rs` | 79 |
| `read_bounded_model_discovery_body` | 1278 | `client/http.rs` | 181 |
| `read_bounded_provider_metadata_body` | 1306 | `client/http.rs` | 209 |
| `parse_provider_metadata_json` | 1338 | `client/http.rs` | 241 |
| `read_bounded_utf8_response_body` | 1367 | `client/http.rs` | 273 |
| `append_bounded_model_discovery_chunk` | 1414 | `client/http.rs` | 320 |
| `append_bounded_response_chunk` | 1437 | `client/http.rs` | 343 |
| `validate_discovered_catalog_bounds` | 1469 | `client/model_discovery.rs` | 416 |
| `append_unique_discovered_models` | 1529 | `client/model_discovery.rs` | 476 |
| `register_pagination_cursor` | 1585 | `client/model_discovery.rs` | 532 |
| `validate_pagination_boundary` | 1611 | `client/model_discovery.rs` | 558 |
| `discover_anthropic_models` | 1626 | `client/model_discovery.rs` | 573 |
| `discover_anthropic_compatible_models` | 1703 | `client/model_discovery.rs` | 650 |
| `discover_gemini_models` | 1791 | `client/model_discovery.rs` | 738 |
| `query_openrouter_model_endpoints` | 1861 | `client/secondary_routes.rs` | 26 |
| `openrouter_endpoint_option` | 1931 | `client/secondary_routes.rs` | 96 |
| `non_empty_text` | 1968 | `client/secondary_routes.rs` | 133 |
| `openrouter_metric` | 1974 | `client/secondary_routes.rs` | 139 |
| `query_minimax_token_plan` | 1988 | `client/account_usage/minimax.rs` | 15 |
| `query_kimi_coding_plan` | 2040 | `client/account_usage/kimi.rs` | 15 |
| `query_command_code_usage` | 2091 | `client/account_usage/command_code.rs` | 31 |
| `command_code_usage_url` | 2147 | `client/account_usage/command_code.rs` | 87 |
| `send_command_code_usage_get` | 2181 | `client/account_usage/command_code.rs` | 121 |
| `decode_command_code_usage` | 2223 | `client/account_usage/command_code.rs` | 163 |
| `command_code_decimal` | 2376 | `client/account_usage/command_code.rs` | 316 |
| `command_code_credit_window` | 2395 | `client/account_usage/command_code.rs` | 335 |
| `command_code_request_window` | 2435 | `client/account_usage/command_code.rs` | 375 |
| `command_code_plan_name` | 2480 | `client/account_usage/command_code.rs` | 420 |
| `decode_kimi_coding_plan_usage` | 2500 | `client/account_usage/kimi.rs` | 56 |
| `kimi_membership_plan_name` | 2575 | `client/account_usage/kimi.rs` | 131 |
| `kimi_usage_window` | 2593 | `client/account_usage/kimi.rs` | 149 |
| `kimi_metric_identifier` | 2649 | `client/account_usage/kimi.rs` | 205 |
| `kimi_unnamed_limit_metric` | 2666 | `client/account_usage/kimi.rs` | 222 |
| `kimi_device_name` | 2697 | `client/account_usage/kimi.rs` | 253 |
| `query_tavily_credits` | 2717 | `client/account_usage/tavily.rs` | 26 |
| `tavily_usage_window` | 2824 | `client/account_usage/tavily.rs` | 133 |
| `query_openrouter_credits` | 2869 | `client/account_usage/openrouter.rs` | 27 |
| `query_deepseek_balance` | 2926 | `client/account_usage/deepseek.rs` | 27 |
| `decode_deepseek_balance` | 2965 | `client/account_usage/deepseek.rs` | 66 |
| `validate_deepseek_decimal` | 3030 | `client/account_usage/deepseek.rs` | 131 |
| `unsupported_usage` | 3038 | `client.rs` | 605 |
| `minimax_quota_windows` | 3066 | `client/account_usage/minimax.rs` | 77 |
| `struct:MiniMaxQuotaWindowInput` | 3105 | `client/account_usage/minimax.rs` | 116 |
| `minimax_quota_window` | 3150 | `client/account_usage/minimax.rs` | 161 |
| `current_unix_milliseconds` | 3185 | `client/account_usage/mod.rs` | 26 |
| `controlled_error_summary` | 3204 | `client/http.rs` | 375 |
| `append_controlled_error_field` | 3262 | `client/http.rs` | 433 |
| `struct:DeepSeekBalanceResponse` | 3279 | `client/account_usage/deepseek.rs` | 140 |
| `struct:DeepSeekBalanceInfo` | 3291 | `client/account_usage/deepseek.rs` | 152 |
| `struct:CommandCodeWhoamiResponse` | 3309 | `client/account_usage/command_code.rs` | 441 |
| `struct:CommandCodeUserIdentity` | 3324 | `client/account_usage/command_code.rs` | 456 |
| `struct:CommandCodeOrganizationIdentity` | 3333 | `client/account_usage/command_code.rs` | 465 |
| `struct:CommandCodeCreditsResponse` | 3343 | `client/account_usage/command_code.rs` | 475 |
| `struct:CommandCodeCreditPools` | 3356 | `client/account_usage/command_code.rs` | 488 |
| `struct:CommandCodeWindowLimits` | 3375 | `client/account_usage/command_code.rs` | 507 |
| `struct:CommandCodeRequestWindow` | 3391 | `client/account_usage/command_code.rs` | 523 |
| `struct:CommandCodeSubscriptionResponse` | 3409 | `client/account_usage/command_code.rs` | 541 |
| `struct:CommandCodeSubscription` | 3422 | `client/account_usage/command_code.rs` | 554 |
| `struct:CommandCodeUsageSummary` | 3441 | `client/account_usage/command_code.rs` | 573 |
| `struct:TavilyUsageResponse` | 3459 | `client/account_usage/tavily.rs` | 168 |
| `struct:TavilyKeyUsage` | 3471 | `client/account_usage/tavily.rs` | 180 |
| `struct:TavilyAccountUsage` | 3489 | `client/account_usage/tavily.rs` | 198 |
| `struct:OpenRouterCreditsResponse` | 3516 | `client/account_usage/openrouter.rs` | 74 |
| `struct:OpenRouterCreditsData` | 3525 | `client/account_usage/openrouter.rs` | 83 |
| `subtract_non_negative_decimal` | 3536 | `client/account_usage/decimal.rs` | 6 |
| `add_non_negative_decimals` | 3577 | `client/account_usage/decimal.rs` | 50 |
| `normalized_unsigned_decimal_digits` | 3616 | `client/account_usage/decimal.rs` | 89 |
| `format_unsigned_decimal_digits` | 3659 | `client/account_usage/decimal.rs` | 132 |
| `struct:OpenRouterModelEndpointsResponse` | 3685 | `client/secondary_routes.rs` | 154 |
| `struct:OpenRouterModelEndpointsData` | 3694 | `client/secondary_routes.rs` | 163 |
| `struct:OpenRouterModelEndpointRecord` | 3704 | `client/secondary_routes.rs` | 173 |
| `struct:OpenRouterEndpointPricingRecord` | 3731 | `client/secondary_routes.rs` | 200 |
| `struct:MiniMaxQuotaResponse` | 3746 | `client/account_usage/minimax.rs` | 192 |
| `struct:MiniMaxQuotaBaseResponse` | 3758 | `client/account_usage/minimax.rs` | 204 |
| `struct:MiniMaxQuotaModelRemain` | 3771 | `client/account_usage/minimax.rs` | 217 |
| `enum:KimiDecimal` | 3817 | `client/account_usage/kimi.rs` | 263 |
| `KimiDecimal::as_f64` | 3829 | `client/account_usage/kimi.rs` | 275 |
| `KimiDecimal::exact` | 3839 | `client/account_usage/kimi.rs` | 285 |
| `KimiDecimal::lossless` | 3848 | `client/account_usage/kimi.rs` | 294 |
| `struct:KimiUsageDetail` | 3859 | `client/account_usage/kimi.rs` | 305 |
| `struct:KimiUsageWindow` | 3909 | `client/account_usage/kimi.rs` | 355 |
| `struct:KimiUsageLimit` | 3923 | `client/account_usage/kimi.rs` | 369 |
| `struct:KimiUsageResponse` | 3953 | `client/account_usage/kimi.rs` | 399 |
| `struct:KimiUsageUser` | 3971 | `client/account_usage/kimi.rs` | 417 |
| `struct:KimiUsageMembership` | 3981 | `client/account_usage/kimi.rs` | 427 |
