# 03：生产节点迁移映射

## 当前校验快照

2026-08-31，第16阶段完成后的结构快照；原主文件14353→398行。共41个Rust产物：398行根、19个生产子模块、19个实际测试用例文件、1个测试分组入口与1个共享夹具文件。本原始文件及全部产物已完成逐文件终审，完整依据见03-FINAL-REVIEW.md；此前阶段记录保留历史证据，本映射对应最终源码。

- 原生产区域250个节点均唯一保留：239个迁移到职责模块，11个仍在根文件。
- 原103个测试及11个辅助节点另见测试映射。
- 节点词法比较忽略注释、格式及必要的父模块可见性，保留字符串、原始字符串与表达式；另核查前置测试/派生/序列化属性及可见性变化。
- 全部364个节点唯一保留，前置属性一致；362个严格词法一致，2个测试仅有已精确审核的rustfmt表达式块折叠。所有250个生产区域节点严格词法一致。Schema测试限定转换两处pointer返回match分支；Apply Patch测试限定去掉同一panic宏闭包的单表达式块；每项要求其余全部token相同，通用比较器不放宽。
- 32个原对外公开顶层节点：3个仍在根定义、29个经根显式重导出。另有15个原crate可见顶层函数保留原路径和范围。
- 两个对外媒体常量和一个crate内工具媒体时长常量保持原访问路径；18个顶层常量均保全。公开与crate内可见严格区分，不将后者误报为对外API。
- 空Error实现保留；客户端、共享缓存容器及构造关系没有复制，业务表达式未改。

## 产物及摘要

| 相对源码路径 | 行数 | SHA-256 |
| --- | ---: | --- |
| `crates/vulcan-provider-adapters/src/execution.rs` | 398 | `e56f31aed5efda645873cd322eff5429f11c439e19933491e57cda5ffb36b969` |
| `crates/vulcan-provider-adapters/src/execution/wire_capture.rs` | 534 | `64420426782f0671ebbc4951d7b984a0d8f8891978e284e6f8e92b041aa1f0cb` |
| `crates/vulcan-provider-adapters/src/execution/wire_capture/tests.rs` | 26 | `478d3d10e7ab36f06e915025245600592dfec404fac83c309509c69d9057dd47` |
| `crates/vulcan-provider-adapters/src/execution/usage.rs` | 130 | `5f3352bda3f46e475cca0eec522517d303ae647d361902a834180d9fc3d48b65` |
| `crates/vulcan-provider-adapters/src/execution/usage/tests.rs` | 209 | `e743cbbc862246d9a09d80bd54dabea9d4e6a81bb402dd9c068b20c0af894cd7` |
| `crates/vulcan-provider-adapters/src/execution/transport_diagnostics.rs` | 241 | `6dd8182ec084d94d30f6daf86fd2dc5e7cc8b6ce37a9c35ae9978475dcf723fd` |
| `crates/vulcan-provider-adapters/src/execution/transport_diagnostics/tests.rs` | 19 | `2bc1de2af2d4f9688a8f68962a62b6d97e1041f9f9582dda79b48d2ad73e1442` |
| `crates/vulcan-provider-adapters/src/execution/tool_output.rs` | 222 | `cf012ee0a6326d860cdf6c4ba8ff5520942429520222edee65e27ba555c3f837` |
| `crates/vulcan-provider-adapters/src/execution/test_support.rs` | 210 | `7f5c46fe2261b2f16203604ff7b5e52bfed4683f66539142ce80dd15cf48e060` |
| `crates/vulcan-provider-adapters/src/execution/tests.rs` | 6 | `e5f6eceea3e96db99d7c0e823a15cb9e47addf140e2aea0527cc08bb56173e23` |
| `crates/vulcan-provider-adapters/src/execution/tests/provider_contracts.rs` | 575 | `1bdd47fc9682dca38e91a6264addf60df2fa8197c960d0c694793456b9a1ee23` |
| `crates/vulcan-provider-adapters/src/execution/tests/protocol_fidelity.rs` | 608 | `3084185bc4fb4fea23180911e9a86b87f9c86b8eb900ea8c9f14cb16018275b2` |
| `crates/vulcan-provider-adapters/src/execution/tests/manifest_equivalence.rs` | 349 | `26ea7025f37024eaa12cf5a1129b6e894c03bfc2a6e7ada17d42a4b8523033d8` |
| `crates/vulcan-provider-adapters/src/execution/stream_state.rs` | 68 | `5908f29e5fbf58e65f8dafc72fdefcb9b7704db1dfbefc07f303ef9eef9cef49` |
| `crates/vulcan-provider-adapters/src/execution/stream_state/tests.rs` | 21 | `7429b9d8202396d1e3a169a3fdce3bae1e445de3400f74cd5df0fad745857bb7` |
| `crates/vulcan-provider-adapters/src/execution/route.rs` | 295 | `3c6efdd928de984109e9522ba5d6c385e70bbd913759554275ed6a792994fc71` |
| `crates/vulcan-provider-adapters/src/execution/route/tests.rs` | 111 | `b8c9b8813da3183aff6bd8254a20dfaec6accd7ff67f5c174de38a2f41228295` |
| `crates/vulcan-provider-adapters/src/execution/response_metadata.rs` | 239 | `9cf97e359b2f86f4c78d9e9faa4ccf3fab9512c8239c4788b74ea162df81a405` |
| `crates/vulcan-provider-adapters/src/execution/request_headers/tests.rs` | 140 | `7d8e88fadfd1ef708fc7296066cbb6fde3ae2a237b7dec664ddeb7ab6ba74a4f` |
| `crates/vulcan-provider-adapters/src/execution/request_encoding.rs` | 1089 | `d7644564cd9b096125ad954a8d094f8d72d2a97385f9d9e29bc1b6e390f9fbf8` |
| `crates/vulcan-provider-adapters/src/execution/response_metadata/tests.rs` | 150 | `6ab57ed886aabee9939fe2d9e3f6ae5bc63202663333462fc93777b28ba4d033` |
| `crates/vulcan-provider-adapters/src/execution/response_body.rs` | 104 | `92960702c98ba9856616884e06c8df4eda225d63f75084599dabbdade2a1f49d` |
| `crates/vulcan-provider-adapters/src/execution/responses_websocket.rs` | 890 | `c68830f5055e46bb2fe9728f62cd8905076a0270f31b33ac2e6cf75739b1d90a` |
| `crates/vulcan-provider-adapters/src/execution/responses_websocket/tests.rs` | 419 | `0b7d70ba77a0683078ae449eeeff00a40141e03881dc676e2bf30499736b5a00` |
| `crates/vulcan-provider-adapters/src/execution/responses_compact.rs` | 466 | `a2797445595b443a48999db0892daefe3c4e12e9289327857b8a662fb0918819` |
| `crates/vulcan-provider-adapters/src/execution/request_encoding/tests.rs` | 378 | `1ec69415dc7b98b72494e6a17bd60d95b3e84f94d025907f357ae23fad1a3532` |
| `crates/vulcan-provider-adapters/src/execution/projection_profiles.rs` | 849 | `af2f112929f8d877d6babf844172c213bd83c1050f2453fde470eb80644cb30e` |
| `crates/vulcan-provider-adapters/src/execution/responses_compact/tests.rs` | 128 | `3818781eb2002184af14950ced611033b8d13339b8ce06ac68bfabebd45842c3` |
| `crates/vulcan-provider-adapters/src/execution/resource_download.rs` | 202 | `9e23f12ec663d3055efed8ef173aedec7faaa98166aec98e6113fd63fefcb33f` |
| `crates/vulcan-provider-adapters/src/execution/request_headers.rs` | 312 | `03d43c717ea250c41258f58ec77ce502dbfd06a0946f281d04e0b546bca38fba` |
| `crates/vulcan-provider-adapters/src/execution/projection_profiles/tests.rs` | 120 | `f4f3d9122e33e02fc0414d7bb11a8be35bf34af0fb55358a72095f675ed847d6` |
| `crates/vulcan-provider-adapters/src/execution/prepared_media.rs` | 1291 | `3fc5414514d7f406b92a85cfc3beb4a26ebc93000418df8581f37e6449255d71` |
| `crates/vulcan-provider-adapters/src/execution/http_transport/tests.rs` | 694 | `a9e2096fcb8f35be36b43f534a6aed854303df34ad16f9a70c6fe7a31e9d54fb` |
| `crates/vulcan-provider-adapters/src/execution/framing.rs` | 403 | `a3e9c0a3e959441e04379fd1ec6ae5858929931e7d36b7130b86f341fea1607c` |
| `crates/vulcan-provider-adapters/src/execution/prepared_media/tests.rs` | 427 | `ffe75c687706789c05a33288204c3fda5296c345038092a5146b79fb2d45f8ca` |
| `crates/vulcan-provider-adapters/src/execution/json_template.rs` | 666 | `5ac33a9d8aa0f30f1e58ee6f6fce6865389c7975b0ee0710ff78cd304157b700` |
| `crates/vulcan-provider-adapters/src/execution/framing/tests.rs` | 124 | `18c3727728cc8722a1570120e0481bcb8ae8daacc7d1505a1473c5668dffbb68` |
| `crates/vulcan-provider-adapters/src/execution/failure.rs` | 759 | `894b65d12c84345edb644e604faae6f38951f0283327fbdd9784032143bab1ed` |
| `crates/vulcan-provider-adapters/src/execution/json_template/tests.rs` | 171 | `b163abcbc274ce1edcaba782327fa841faebd1b09f91b778514d52ed9a7bfc8b` |
| `crates/vulcan-provider-adapters/src/execution/http_transport.rs` | 556 | `e30db5f59235df9f6fdd60d009f872373ba735999c146feb54926ef6d8f9130e` |
| `crates/vulcan-provider-adapters/src/execution/failure/tests.rs` | 272 | `a6213775bbff16efd3e426b238f1c387decc3d4e4ba99aec3a77c3da6c2d9b71` |

## 全部生产区域节点

行号分别属于冻结前根文件与当前产物，不能混用。本表同时保留尚未迁移节点。

| 节点 | 原始行 | 当前相对源码路径 | 当前行 | 结论 |
| --- | --- | --- | --- | --- |
| `trait:ProtocolRuntimeSource` | 57–61 | `execution.rs` | 81–85 | 等价，根保留 |
| `ProtocolRuntimeSource for Arc<LlmProtocolRuntime>::into_protocol_runtime` | 65–67 | `execution.rs` | 89–91 | 等价，根保留 |
| `struct:ProviderExecutionRoute` | 113–156 | `execution/route.rs` | 21–64 | 等价，已迁移 |
| `ProviderExecutionRoute::resolve` | 172–260 | `execution/route.rs` | 80–168 | 等价，已迁移 |
| `ProviderExecutionRoute::request_url` | 269–280 | `execution/route.rs` | 177–188 | 等价，已迁移 |
| `ProviderExecutionRoute::model_parameters_conflicting_with_request_field` | 294–331 | `execution/route.rs` | 202–239 | 等价，已迁移 |
| `struct:ProviderExecutionFailure` | 337–341 | `execution/failure.rs` | 15–19 | 等价，已迁移 |
| `std::fmt::Display for ProviderExecutionFailure::fmt` | 346–348 | `execution/failure.rs` | 24–26 | 等价，已迁移 |
| `struct:ResponsesCompactResult` | 356–368 | `execution/responses_compact.rs` | 42–54 | 等价，已迁移 |
| `struct:ProviderWireRequestCapture` | 373–392 | `execution/wire_capture.rs` | 18–37 | 等价，已迁移 |
| `struct:ProviderWireRequestTraceDispatch` | 396–403 | `execution/wire_capture.rs` | 41–48 | 等价，已迁移 |
| `enum:ProviderWireRequestCaptureTarget` | 407–414 | `execution/wire_capture.rs` | 52–59 | 等价，已迁移 |
| `struct:ProviderWireCaptureTargets` | 418–425 | `execution/wire_capture.rs` | 63–70 | 等价，已迁移 |
| `struct:ProviderWireResponseCapture` | 430–479 | `execution/wire_capture.rs` | 75–124 | 等价，已迁移 |
| `struct:ProviderWireResponseChunkCapture` | 484–497 | `execution/wire_capture.rs` | 129–142 | 等价，已迁移 |
| `struct:ProviderWireResponseFrameCapture` | 502–518 | `execution/wire_capture.rs` | 147–163 | 等价，已迁移 |
| `struct:ProviderWireResponseCaptureGuard` | 530–540 | `execution/wire_capture.rs` | 175–185 | 等价，已迁移 |
| `ProviderWireResponseCaptureGuard::new` | 555–580 | `execution/wire_capture.rs` | 200–228 | 等价，已迁移 |
| `ProviderWireResponseCaptureGuard::observe_http_headers` | 589–602 | `execution/wire_capture.rs` | 237–250 | 等价，已迁移 |
| `ProviderWireResponseCaptureGuard::observe_chunk` | 611–629 | `execution/wire_capture.rs` | 259–277 | 等价，已迁移 |
| `ProviderWireResponseCaptureGuard::observe_websocket_frame` | 638–657 | `execution/wire_capture.rs` | 286–305 | 等价，已迁移 |
| `ProviderWireResponseCaptureGuard::observe_body_bytes` | 666–674 | `execution/wire_capture.rs` | 314–322 | 等价，已迁移 |
| `ProviderWireResponseCaptureGuard::set_outcome` | 683–687 | `execution/wire_capture.rs` | 331–335 | 等价，已迁移 |
| `ProviderWireResponseCaptureGuard::observe_normalized_event` | 696–713 | `execution/wire_capture.rs` | 344–361 | 等价，已迁移 |
| `ProviderWireResponseCaptureGuard::observe_failure` | 722–729 | `execution/wire_capture.rs` | 370–377 | 等价，已迁移 |
| `ProviderWireResponseCaptureGuard::set_outcome_if_unresolved` | 738–744 | `execution/wire_capture.rs` | 386–392 | 等价，已迁移 |
| `Drop for ProviderWireResponseCaptureGuard::drop` | 750–764 | `execution/wire_capture.rs` | 398–412 | 等价，已迁移 |
| `provider_wire_capture_timestamp_ms` | 769–773 | `execution/wire_capture.rs` | 417–421 | 等价，已迁移 |
| `type:ProviderExecutionStream` | 777–778 | `execution.rs` | 100–101 | 等价，根保留 |
| `struct:ProviderToolResourceDownloadPolicy` | 783–790 | `execution/resource_download.rs` | 9–16 | 等价，已迁移 |
| `ProviderToolResourceDownloadPolicy::validate_url` | 805–807 | `execution/resource_download.rs` | 31–33 | 等价，已迁移 |
| `ProviderToolResourceDownloadPolicy::validate_url_with_exact_origin` | 823–830 | `execution/resource_download.rs` | 49–56 | 等价，已迁移 |
| `ProviderToolResourceDownloadPolicy::validate_url_with_optional_exact_origin` | 845–891 | `execution/resource_download.rs` | 71–117 | 等价，已迁移 |
| `provider_resource_urls_share_origin` | 906–912 | `execution/resource_download.rs` | 132–138 | 等价，已迁移 |
| `provider_resource_host_suffix_is_safe` | 926–953 | `execution/resource_download.rs` | 152–179 | 等价，已迁移 |
| `provider_resource_host_matches_suffixes` | 967–976 | `execution/resource_download.rs` | 193–202 | 等价，已迁移 |
| `struct:ProviderToolGeneratedResource` | 981–1001 | `execution/tool_output.rs` | 10–30 | 等价，已迁移 |
| `struct:ProviderToolSearchResult` | 1006–1022 | `execution/tool_output.rs` | 35–51 | 等价，已迁移 |
| `struct:ProviderToolSearchOutput` | 1027–1034 | `execution/tool_output.rs` | 56–63 | 等价，已迁移 |
| `struct:ProviderToolExtractResult` | 1041–1054 | `execution/tool_output.rs` | 70–83 | 等价，已迁移 |
| `struct:ProviderToolExtractFailure` | 1059–1066 | `execution/tool_output.rs` | 88–95 | 等价，已迁移 |
| `struct:ProviderToolExtractOutput` | 1071–1078 | `execution/tool_output.rs` | 100–107 | 等价，已迁移 |
| `struct:ProviderToolEmbeddingVector` | 1083–1090 | `execution/tool_output.rs` | 112–119 | 等价，已迁移 |
| `struct:ProviderToolEmbeddingOutput` | 1095–1102 | `execution/tool_output.rs` | 124–131 | 等价，已迁移 |
| `struct:ProviderToolRerankResult` | 1107–1117 | `execution/tool_output.rs` | 136–146 | 等价，已迁移 |
| `struct:ProviderToolRerankOutput` | 1122–1129 | `execution/tool_output.rs` | 151–158 | 等价，已迁移 |
| `struct:ProviderToolCompositeOutput` | 1134–1153 | `execution/tool_output.rs` | 163–182 | 等价，已迁移 |
| `enum:ProviderToolNormalizedOutput` | 1159–1181 | `execution/tool_output.rs` | 188–210 | 等价，已迁移 |
| `struct:ProviderToolExecutionResult` | 1186–1193 | `execution/tool_output.rs` | 215–222 | 等价，已迁移 |
| `struct:ProviderExecutionClient` | 1198–1215 | `execution.rs` | 106–123 | 等价，根保留 |
| `struct:ResponsesWebsocketSessionCacheEntry` | 1220–1227 | `execution/responses_websocket.rs` | 61–68 | 等价，已迁移 |
| `struct:PreparedMediaCacheEntry` | 1232–1254 | `execution/prepared_media.rs` | 32–54 | 等价，已迁移 |
| `struct:PreparedConversationMedia` | 1259–1266 | `execution/prepared_media.rs` | 59–66 | 等价，已迁移 |
| `PreparedConversationMedia::prepared_reference` | 1277–1279 | `execution/prepared_media.rs` | 77–79 | 等价，已迁移 |
| `PreparedConversationMedia::source_identity` | 1283–1288 | `execution/prepared_media.rs` | 83–88 | 等价，已迁移 |
| `struct:PreparedMediaSourceIdentity` | 1294–1301 | `execution/prepared_media.rs` | 94–101 | 等价，已迁移 |
| `struct:PreparedMediaReferenceSnapshot` | 1306–1319 | `execution/prepared_media.rs` | 106–119 | 等价，已迁移 |
| `struct:ConversationPreparedMediaRetention` | 1324–1331 | `execution/prepared_media.rs` | 124–131 | 等价，已迁移 |
| `struct:ConversationMediaUploadContract` | 1336–1352 | `execution/prepared_media.rs` | 136–152 | 等价，已迁移 |
| `struct:PreparedMediaSourceFingerprint` | 1357–1367 | `execution/prepared_media.rs` | 157–167 | 等价，已迁移 |
| `struct:PreparedMediaCacheKeyInput` | 1371–1396 | `execution/prepared_media.rs` | 171–196 | 等价，已迁移 |
| `struct:PreparedMediaCacheWrite` | 1400–1422 | `execution/prepared_media.rs` | 200–222 | 等价，已迁移 |
| `struct:PreparedMediaSourceRead` | 1426–1433 | `execution/prepared_media.rs` | 226–233 | 等价，已迁移 |
| `prepared_media_source_fingerprint` | 1451–1459 | `execution/prepared_media.rs` | 251–259 | 等价，已迁移 |
| `read_prepared_media_source` | 1475–1546 | `execution/prepared_media.rs` | 275–346 | 等价，已迁移 |
| `resolve_conversation_media_upload_channel` | 1560–1606 | `execution/prepared_media.rs` | 360–406 | 等价，已迁移 |
| `upload_channel_contract_fingerprint` | 1612–1637 | `execution/prepared_media.rs` | 410–435 | 等价，已迁移 |
| `prepared_media_key` | 1639–1683 | `execution/prepared_media.rs` | 439–483 | 等价，已迁移 |
| `prepared_media_cache_key` | 1693–1727 | `execution/prepared_media.rs` | 493–527 | 等价，已迁移 |
| `prune_prepared_media_cache` | 1731–1747 | `execution/prepared_media.rs` | 531–547 | 等价，已迁移 |
| `current_prepared_media_unix_millis` | 1751–1756 | `execution/prepared_media.rs` | 551–556 | 等价，已迁移 |
| `provider_tool_reference_is_reusable` | 1760–1768 | `execution/prepared_media.rs` | 560–568 | 等价，已迁移 |
| `ProviderExecutionClient::new` | 1783–1794 | `execution.rs` | 138–149 | 等价，根保留 |
| `ProviderExecutionClient::prepare_local_media` | 1822–1947 | `execution/prepared_media.rs` | 597–722 | 等价，已迁移 |
| `ProviderExecutionClient::conversation_media_upload_channel_id` | 1961–1969 | `execution/prepared_media.rs` | 736–744 | 等价，已迁移 |
| `ProviderExecutionClient::conversation_media_upload_contract` | 1973–1993 | `execution/prepared_media.rs` | 748–768 | 等价，已迁移 |
| `ProviderExecutionClient::read_media_source_identity` | 1997–2008 | `execution/prepared_media.rs` | 772–783 | 等价，已迁移 |
| `ProviderExecutionClient::prepared_provider_tool_media_reference` | 2029–2076 | `execution/prepared_media.rs` | 804–851 | 等价，已迁移 |
| `ProviderExecutionClient::remember_provider_tool_media_reference` | 2096–2160 | `execution/prepared_media.rs` | 871–935 | 等价，已迁移 |
| `ProviderExecutionClient::cached_prepared_media_reference` | 2164–2182 | `execution/prepared_media.rs` | 939–957 | 等价，已迁移 |
| `ProviderExecutionClient::remember_persisted_media_reference` | 2187–2232 | `execution/prepared_media.rs` | 962–1007 | 等价，已迁移 |
| `ProviderExecutionClient::remember_prepared_media` | 2236–2254 | `execution/prepared_media.rs` | 1011–1029 | 等价，已迁移 |
| `ProviderExecutionClient::remember_prepared_media_with_wall_clock` | 2259–2279 | `execution/prepared_media.rs` | 1034–1054 | 等价，已迁移 |
| `ProviderExecutionClient::remember_conversation_prepared_media` | 2283–2302 | `execution/prepared_media.rs` | 1058–1077 | 等价，已迁移 |
| `ProviderExecutionClient::remember_prepared_media_with_wall_clock_and_retention` | 2307–2385 | `execution/prepared_media.rs` | 1082–1160 | 等价，已迁移 |
| `ProviderExecutionClient::remember_prepared_media_cache_entry` | 2399–2465 | `execution/prepared_media.rs` | 1174–1240 | 等价，已迁移 |
| `ProviderExecutionClient::prepared_media_reference` | 2469–2489 | `execution/prepared_media.rs` | 1244–1264 | 等价，已迁移 |
| `ProviderExecutionClient::prepared_media_url` | 2493–2512 | `execution/prepared_media.rs` | 1268–1287 | 等价，已迁移 |
| `ProviderExecutionClient::execute_provider_tool` | 2528–2554 | `execution.rs` | 165–191 | 等价，根保留 |
| `ProviderExecutionClient::execute_provider_tool_with_profiles` | 2571–2596 | `execution.rs` | 208–233 | 等价，根保留 |
| `ProviderExecutionClient::encode_request` | 2610–2632 | `execution/request_encoding.rs` | 43–65 | 等价，已迁移 |
| `ProviderExecutionClient::encode_required_disabled_reasoning_request` | 2650–2664 | `execution/request_encoding.rs` | 83–97 | 等价，已迁移 |
| `ProviderExecutionClient::encode_request_with_credential` | 2679–2687 | `execution/request_encoding.rs` | 112–120 | 等价，已迁移 |
| `ProviderExecutionClient::encode_effective_request_with_credential` | 2703–2746 | `execution/responses_websocket.rs` | 85–128 | 等价，已迁移 |
| `ProviderExecutionClient::execute_responses_compact` | 2762–3012 | `execution/responses_compact.rs` | 71–321 | 等价，已迁移 |
| `ProviderExecutionClient::execute` | 3028–3045 | `execution.rs` | 249–266 | 等价，根保留 |
| `ProviderExecutionClient::execute_with_wire_request_capture` | 3062–3082 | `execution.rs` | 283–303 | 等价，根保留 |
| `ProviderExecutionClient::execute_with_wire_api_trace_capture` | 3098–3119 | `execution.rs` | 319–340 | 等价，根保留 |
| `ProviderExecutionClient::execute_with_optional_wire_captures` | 3135–3173 | `execution.rs` | 356–394 | 等价，根保留 |
| `ProviderExecutionClient::execute_responses_websocket` | 3177–3624 | `execution/responses_websocket.rs` | 132–579 | 等价，已迁移 |
| `ProviderExecutionClient::responses_websocket_session` | 3628–3674 | `execution/responses_websocket.rs` | 583–629 | 等价，已迁移 |
| `ProviderExecutionClient::responses_websocket_disabled` | 3688–3698 | `execution/responses_websocket.rs` | 643–653 | 等价，已迁移 |
| `ProviderExecutionClient::disable_responses_websocket` | 3702–3708 | `execution/responses_websocket.rs` | 657–663 | 等价，已迁移 |
| `ProviderExecutionClient::execute_http` | 3712–4226 | `execution/http_transport.rs` | 38–552 | 等价，已迁移 |
| `struct:ProfiledUsageEventGate` | 4236–4243 | `execution/usage.rs` | 16–23 | 等价，已迁移 |
| `ProfiledUsageEventGate::stage` | 4258–4273 | `execution/usage.rs` | 38–53 | 等价，已迁移 |
| `ProfiledUsageEventGate::observe` | 4277–4283 | `execution/usage.rs` | 57–63 | 等价，已迁移 |
| `ProfiledUsageEventGate::ensure_released` | 4287–4294 | `execution/usage.rs` | 67–74 | 等价，已迁移 |
| `extract_profiled_usage_events` | 4299–4318 | `execution/usage.rs` | 79–98 | 等价，已迁移 |
| `extract_profiled_usage_value_events` | 4322–4347 | `execution/usage.rs` | 102–127 | 等价，已迁移 |
| `request_without_nonpersistent_responses_continuation` | 4361–4373 | `execution/request_encoding.rs` | 1077–1089 | 等价，已迁移 |
| `responses_websocket_request_for_active_session` | 4388–4400 | `execution/responses_websocket.rs` | 679–691 | 等价，已迁移 |
| `responses_websocket_retry_control` | 4414–4441 | `execution/responses_websocket.rs` | 705–732 | 等价，已迁移 |
| `responses_websocket_control_failure` | 4456–4470 | `execution/responses_websocket.rs` | 747–761 | 等价，已迁移 |
| `abandon_responses_websocket_attempt` | 4484–4490 | `execution/responses_websocket.rs` | 775–781 | 等价，已迁移 |
| `responses_websocket_cache_key` | 4494–4513 | `execution/responses_websocket.rs` | 785–804 | 等价，已迁移 |
| `oldest_idle_responses_websocket_session_key` | 4527–4539 | `execution/responses_websocket.rs` | 818–830 | 等价，已迁移 |
| `insert_bounded_responses_websocket_breaker` | 4554–4571 | `execution/responses_websocket.rs` | 845–862 | 等价，已迁移 |
| `responses_websocket_url` | 4575–4596 | `execution/responses_websocket.rs` | 866–887 | 等价，已迁移 |
| `websocket_headers` | 4600–4616 | `execution/request_headers.rs` | 293–309 | 等价，已迁移 |
| `emit_provider_transport_trace` | 4620–4633 | `execution/transport_diagnostics.rs` | 14–27 | 等价，已迁移 |
| `emit_provider_transport_http_status_trace` | 4644–4657 | `execution/transport_diagnostics.rs` | 38–51 | 等价，已迁移 |
| `emit_provider_transport_failure_trace` | 4668–4688 | `execution/transport_diagnostics.rs` | 62–82 | 等价，已迁移 |
| `trace_provider_transport_failure` | 4704–4712 | `execution/transport_diagnostics.rs` | 98–106 | 等价，已迁移 |
| `emit_responses_compact_response_shape_trace` | 4722–4796 | `execution/transport_diagnostics.rs` | 116–190 | 等价，已迁移 |
| `bounded_trace_label` | 4810–4822 | `execution/transport_diagnostics.rs` | 204–216 | 等价，已迁移 |
| `provider_transport_trace_enabled` | 4826–4833 | `execution/transport_diagnostics.rs` | 220–227 | 等价，已迁移 |
| `stream_event_has_semantic_output` | 4847–4857 | `execution/stream_state.rs` | 20–30 | 等价，已迁移 |
| `update_semantic_output_observation` | 4861–4868 | `execution/stream_state.rs` | 34–41 | 等价，已迁移 |
| `validate_stream_event` | 4884–4892 | `execution/stream_state.rs` | 57–65 | 等价，已迁移 |
| `request_projection_profiles_cover_parameter` | 4901–4927 | `execution/request_encoding.rs` | 130–156 | 等价，已迁移 |
| `validate_request_policy` | 4931–4958 | `execution/request_encoding.rs` | 160–187 | 等价，已迁移 |
| `validate_request_projection_guards_before_encoding` | 4978–5006 | `execution/request_encoding.rs` | 207–235 | 等价，已迁移 |
| `project_responses_baseline_for_provider` | 5017–5079 | `execution/request_encoding.rs` | 246–308 | 等价，已迁移 |
| `upstream_protocol_for_route` | 5083–5085 | `execution/route.rs` | 244–248 | 等价，已迁移 |
| `route_uses_native_codex_responses` | 5099–5101 | `execution/route.rs` | 262–264 | 等价，已迁移 |
| `upstream_protocol_for_profile_id` | 5105–5119 | `execution/route.rs` | 268–282 | 等价，已迁移 |
| `prepare_target_reasoning_replay_for_responses` | 5129–5155 | `execution/request_encoding.rs` | 318–344 | 等价，已迁移 |
| `attach_target_tool_replay_to_responses` | 5171–5223 | `execution/request_encoding.rs` | 360–412 | 等价，已迁移 |
| `project_provider_request` | 5227–5248 | `execution/request_encoding.rs` | 416–437 | 等价，已迁移 |
| `validate_required_disabled_reasoning_wire` | 5265–5283 | `execution/request_encoding.rs` | 454–472 | 等价，已迁移 |
| `wire_body_explicitly_disables_reasoning` | 5297–5321 | `execution/request_encoding.rs` | 486–510 | 等价，已迁移 |
| `chat_body_explicitly_disables_reasoning` | 5338–5408 | `execution/request_encoding.rs` | 527–597 | 等价，已迁移 |
| `wire_body_explicitly_enables_reasoning` | 5422–5457 | `execution/request_encoding.rs` | 611–646 | 等价，已迁移 |
| `json_pointer_is_disabled_label` | 5471–5475 | `execution/request_encoding.rs` | 660–664 | 等价，已迁移 |
| `disabled_reasoning_label` | 5489–5494 | `execution/request_encoding.rs` | 678–683 | 等价，已迁移 |
| `json_pointer_is_numeric_zero` | 5508–5514 | `execution/request_encoding.rs` | 697–703 | 等价，已迁移 |
| `json_pointer_is_numeric_nonzero` | 5528–5534 | `execution/request_encoding.rs` | 717–723 | 等价，已迁移 |
| `project_secondary_route` | 5548–5569 | `execution/request_encoding.rs` | 737–758 | 等价，已迁移 |
| `apply_request_projection_profiles` | 5578–5589 | `execution/projection_profiles.rs` | 19–30 | 等价，已迁移 |
| `execute_request_profile_fixture` | 5605–5611 | `execution/projection_profiles.rs` | 46–52 | 等价，已迁移 |
| `project_json_request_profiles` | 5628–5664 | `execution/projection_profiles.rs` | 69–105 | 等价，已迁移 |
| `project_json_response_profiles` | 5679–5685 | `execution/projection_profiles.rs` | 120–126 | 等价，已迁移 |
| `apply_response_projection_profiles` | 5689–5707 | `execution/projection_profiles.rs` | 130–148 | 等价，已迁移 |
| `project_response_record_text` | 5711–5729 | `execution/projection_profiles.rs` | 152–170 | 等价，已迁移 |
| `apply_response_mapping_step` | 5733–5904 | `execution/projection_profiles.rs` | 174–345 | 等价，已迁移 |
| `ensure_response_body_source` | 5908–5918 | `execution/projection_profiles.rs` | 349–359 | 等价，已迁移 |
| `write_response_json_pointer` | 5922–5952 | `execution/projection_profiles.rs` | 363–393 | 等价，已迁移 |
| `write_or_delete_response` | 5956–5967 | `execution/projection_profiles.rs` | 397–408 | 等价，已迁移 |
| `enum:JsonTemplateRender` | 5981–5988 | `execution/json_template.rs` | 16–23 | 等价，已迁移 |
| `struct:JsonTemplateContext` | 5992–6002 | `execution/json_template.rs` | 27–37 | 等价，已迁移 |
| `render_complete_json_template` | 6017–6043 | `execution/json_template.rs` | 52–78 | 等价，已迁移 |
| `render_json_template_value` | 6047–6092 | `execution/json_template.rs` | 82–127 | 等价，已迁移 |
| `json_template_registered_form` | 6096–6126 | `execution/json_template.rs` | 131–161 | 等价，已迁移 |
| `render_json_template_form` | 6130–6163 | `execution/json_template.rs` | 165–198 | 等价，已迁移 |
| `json_template_source` | 6167–6200 | `execution/json_template.rs` | 202–235 | 等价，已迁移 |
| `render_json_template_reference` | 6204–6234 | `execution/json_template.rs` | 239–269 | 等价，已迁移 |
| `render_json_template_trimmed_string` | 6249–6285 | `execution/json_template.rs` | 284–320 | 等价，已迁移 |
| `render_json_template_map` | 6289–6351 | `execution/json_template.rs` | 324–386 | 等价，已迁移 |
| `render_json_template_enum` | 6355–6387 | `execution/json_template.rs` | 390–422 | 等价，已迁移 |
| `render_json_template_condition` | 6391–6411 | `execution/json_template.rs` | 426–446 | 等价，已迁移 |
| `render_json_template_presence_condition` | 6425–6458 | `execution/json_template.rs` | 460–493 | 等价，已迁移 |
| `render_json_template_array_length_condition` | 6472–6502 | `execution/json_template.rs` | 507–537 | 等价，已迁移 |
| `render_json_template_concat` | 6506–6542 | `execution/json_template.rs` | 541–577 | 等价，已迁移 |
| `render_json_template_array_concat` | 6556–6588 | `execution/json_template.rs` | 591–623 | 等价，已迁移 |
| `render_json_template_coalesce` | 6602–6628 | `execution/json_template.rs` | 637–663 | 等价，已迁移 |
| `apply_mapping_step` | 6630–6813 | `execution/projection_profiles.rs` | 412–595 | 等价，已迁移 |
| `write_json_pointer` | 6830–6865 | `execution/projection_profiles.rs` | 612–647 | 等价，已迁移 |
| `delete_json_pointer` | 6869–6875 | `execution/projection_profiles.rs` | 651–657 | 等价，已迁移 |
| `delete_pointer_recursive` | 6877–6921 | `execution/projection_profiles.rs` | 659–703 | 等价，已迁移 |
| `pointer_value` | 6925–6927 | `execution/projection_profiles.rs` | 707–709 | 等价，已迁移 |
| `pointer_segments` | 6931–6936 | `execution/projection_profiles.rs` | 713–718 | 等价，已迁移 |
| `write_or_delete` | 6951–6962 | `execution/projection_profiles.rs` | 733–744 | 等价，已迁移 |
| `projection_source_value` | 6966–6987 | `execution/projection_profiles.rs` | 748–769 | 等价，已迁移 |
| `projection_scalar_key` | 7001–7008 | `execution/projection_profiles.rs` | 783–790 | 等价，已迁移 |
| `request_field_value` | 7012–7042 | `execution/projection_profiles.rs` | 794–824 | 等价，已迁移 |
| `model_parameter_to_value` | 7046–7064 | `execution/projection_profiles.rs` | 828–846 | 等价，已迁移 |
| `validate_structured_output_projection` | 7079–7138 | `execution/request_encoding.rs` | 774–833 | 等价，已迁移 |
| `project_custom_model_request` | 7142–7173 | `execution/request_encoding.rs` | 837–868 | 等价，已迁移 |
| `model_parameter_json_value` | 7177–7200 | `execution/request_encoding.rs` | 872–895 | 等价，已迁移 |
| `set_json_path` | 7204–7235 | `execution/request_encoding.rs` | 899–930 | 等价，已迁移 |
| `delete_json_path` | 7239–7258 | `execution/request_encoding.rs` | 934–953 | 等价，已迁移 |
| `project_sampling_temperature` | 7272–7300 | `execution/request_encoding.rs` | 967–995 | 等价，已迁移 |
| `request_body_mut` | 7314–7322 | `execution/request_encoding.rs` | 1009–1017 | 等价，已迁移 |
| `project_credential_request` | 7337–7365 | `execution/request_encoding.rs` | 1032–1060 | 等价，已迁移 |
| `publish_wire_request_capture` | 7369–7404 | `execution/wire_capture.rs` | 425–460 | 等价，已迁移 |
| `provider_wire_request_capture` | 7420–7435 | `execution/wire_capture.rs` | 476–491 | 等价，已迁移 |
| `provider_wire_request_capture_headers` | 7449–7475 | `execution/wire_capture.rs` | 505–531 | 等价，已迁移 |
| `execution_headers` | 7491–7707 | `execution/request_headers.rs` | 29–245 | 等价，已迁移 |
| `insert_coding_device_identity_headers` | 7721–7751 | `execution/request_headers.rs` | 259–289 | 等价，已迁移 |
| `struct:StreamFrame` | 7756–7763 | `execution/framing.rs` | 11–18 | 等价，已迁移 |
| `struct:IncrementalFrameParser` | 7768–7784 | `execution/framing.rs` | 23–39 | 等价，已迁移 |
| `IncrementalFrameParser::new` | 7799–7807 | `execution/framing.rs` | 54–62 | 等价，已迁移 |
| `IncrementalFrameParser::push` | 7821–7839 | `execution/framing.rs` | 76–97 | 等价，已迁移 |
| `IncrementalFrameParser::finish` | 7848–7850 | `execution/framing.rs` | 106–108 | 等价，已迁移 |
| `IncrementalFrameParser::drain_frames` | 7864–7891 | `execution/framing.rs` | 122–149 | 等价，已迁移 |
| `IncrementalFrameParser::retained` | 7900–7902 | `execution/framing.rs` | 158–160 | 等价，已迁移 |
| `IncrementalFrameParser::retained_len` | 7911–7913 | `execution/framing.rs` | 169–171 | 等价，已迁移 |
| `IncrementalFrameParser::compact_if_worthwhile` | 7917–7926 | `execution/framing.rs` | 175–184 | 等价，已迁移 |
| `IncrementalFrameParser::validate_frame_size` | 7940–7948 | `execution/framing.rs` | 198–206 | 等价，已迁移 |
| `IncrementalFrameParser::buffer_limit_error` | 7957–7962 | `execution/framing.rs` | 215–220 | 等价，已迁移 |
| `find_record_end` | 7977–7997 | `execution/framing.rs` | 235–255 | 等价，已迁移 |
| `sse_line_ending_len` | 8011–8017 | `execution/framing.rs` | 269–275 | 等价，已迁移 |
| `parse_stream_record` | 8031–8042 | `execution/framing.rs` | 289–300 | 等价，已迁移 |
| `parse_sse_record` | 8056–8082 | `execution/framing.rs` | 314–340 | 等价，已迁移 |
| `parse_json_line_record` | 8096–8111 | `execution/framing.rs` | 354–369 | 等价，已迁移 |
| `parse_single_json_record` | 8125–8142 | `execution/framing.rs` | 383–400 | 等价，已迁移 |
| `ensure_leading_slash` | 8146–8152 | `execution/route.rs` | 286–292 | 等价，已迁移 |
| `pre_dispatch_failure` | 8156–8182 | `execution/failure.rs` | 33–59 | 等价，已迁移 |
| `protocol_failure` | 8186–8212 | `execution/failure.rs` | 63–89 | 等价，已迁移 |
| `llm_core_protocol_failure` | 8216–8242 | `execution/failure.rs` | 93–119 | 等价，已迁移 |
| `prepare_responses_compact_body` | 8257–8283 | `execution/responses_compact.rs` | 337–363 | 等价，已迁移 |
| `decode_responses_compact_result` | 8297–8327 | `execution/responses_compact.rs` | 377–407 | 等价，已迁移 |
| `compact_protocol_failure` | 8331–8357 | `execution/failure.rs` | 123–149 | 等价，已迁移 |
| `compact_transport_failure` | 8371–8397 | `execution/failure.rs` | 163–189 | 等价，已迁移 |
| `stream_protocol_failure` | 8413–8418 | `execution/failure.rs` | 205–210 | 等价，已迁移 |
| `stream_protocol_failure_for_transport` | 8432–8462 | `execution/failure.rs` | 224–254 | 等价，已迁移 |
| `stream_completion_failure` | 8478–8483 | `execution/failure.rs` | 270–275 | 等价，已迁移 |
| `stream_completion_failure_for_transport` | 8497–8511 | `execution/failure.rs` | 289–303 | 等价，已迁移 |
| `transport_failure` | 8515–8541 | `execution/failure.rs` | 307–333 | 等价，已迁移 |
| `http_send_transport_failure` | 8556–8570 | `execution/failure.rs` | 348–362 | 等价，已迁移 |
| `stream_transport_failure` | 8585–8595 | `execution/failure.rs` | 377–387 | 等价，已迁移 |
| `stream_idle_timeout_failure` | 8609–8622 | `execution/failure.rs` | 401–414 | 等价，已迁移 |
| `stream_transport_failure_with_websocket_close_code` | 8626–8637 | `execution/failure.rs` | 418–429 | 等价，已迁移 |
| `dispatched_stream_transport_failure` | 8651–8666 | `execution/failure.rs` | 443–458 | 等价，已迁移 |
| `dispatched_stream_failure` | 8680–8714 | `execution/failure.rs` | 472–506 | 等价，已迁移 |
| `provider_transport_unix_millis` | 8718–8725 | `execution/transport_diagnostics.rs` | 231–238 | 等价，已迁移 |
| `allowlisted_rate_limit_headers` | 8729–8757 | `execution/response_metadata.rs` | 21–49 | 等价，已迁移 |
| `bounded_provider_diagnostic_header_value` | 8773–8793 | `execution/response_metadata.rs` | 65–85 | 等价，已迁移 |
| `provider_request_id_from_headers` | 8807–8825 | `execution/response_metadata.rs` | 99–117 | 等价，已迁移 |
| `validate_response_content_type` | 8840–8870 | `execution/response_metadata.rs` | 132–162 | 等价，已迁移 |
| `response_headers_protocol_failure` | 8884–8912 | `execution/failure.rs` | 520–548 | 等价，已迁移 |
| `read_responses_compact_body_with_timeout` | 8928–8968 | `execution/responses_compact.rs` | 423–463 | 等价，已迁移 |
| `read_bounded_response_text` | 8984–9002 | `execution/response_body.rs` | 24–42 | 等价，已迁移 |
| `read_bounded_response_text_with_capture` | 9017–9064 | `execution/response_body.rs` | 57–104 | 等价，已迁移 |
| `parse_retry_after_headers` | 9079–9105 | `execution/response_metadata.rs` | 177–203 | 等价，已迁移 |
| `parse_decimal_seconds_milliseconds` | 9119–9138 | `execution/response_metadata.rs` | 217–236 | 等价，已迁移 |
| `is_known_overload_message` | 9142–9146 | `execution/failure.rs` | 552–556 | 等价，已迁移 |
| `http_failure` | 9150–9303 | `execution/failure.rs` | 560–713 | 等价，已迁移 |
| `sanitize_provider_execution_failure` | 9318–9346 | `execution/failure.rs` | 728–756 | 等价，已迁移 |

## 顶层常量保全

| 常量 | 原始行 | 当前路径与行 | 原可见性 | 结论 |
| --- | --- | --- | --- | --- |
| `PREFERRED_INLINE_MEDIA_MAX_BYTES` | 49 | `execution/prepared_media.rs:19` | `pub` | 唯一且等价 |
| `VULCAN_CODE_USER_AGENT` | 53 | `execution/request_headers.rs:13` | `private` | 唯一且等价 |
| `RESPONSES_COMPACT_TIMEOUT_IDLE_MULTIPLIER` | 72 | `execution/responses_compact.rs:37` | `private` | 唯一且等价 |
| `PROVIDER_TRANSPORT_TRACE_ENV` | 76 | `execution/transport_diagnostics.rs:10` | `private` | 唯一且等价 |
| `PROVIDER_DIAGNOSTIC_HEADER_VALUE_MAX_BYTES` | 80 | `execution/response_metadata.rs:13` | `private` | 唯一且等价 |
| `PROVIDER_DIAGNOSTIC_HEADER_TRUNCATION_SUFFIX` | 84 | `execution/response_metadata.rs:17` | `private` | 唯一且等价 |
| `CODEX_RESPONSES_WEBSOCKET_BETA` | 88 | `execution/responses_websocket.rs:44` | `private` | 唯一且等价 |
| `RESPONSES_WEBSOCKET_PREVIOUS_RESPONSE_NOT_FOUND_CODE` | 92 | `execution/responses_websocket.rs:48` | `private` | 唯一且等价 |
| `PREPARED_MEDIA_CACHE_MAX_ENTRIES` | 96 | `execution/prepared_media.rs:23` | `private` | 唯一且等价 |
| `PROVIDER_TOOL_MEDIA_CACHE_DURATION_MS` | 99 | `execution.rs:96` | `pub(crate)` | 唯一且等价 |
| `RESPONSES_WEBSOCKET_SESSION_MAX_ENTRIES` | 102 | `execution/responses_websocket.rs:52` | `private` | 唯一且等价 |
| `RESPONSES_WEBSOCKET_BREAKER_MAX_ENTRIES` | 105 | `execution/responses_websocket.rs:56` | `private` | 唯一且等价 |
| `CONVERSATION_MEDIA_INPUT_OPERATION_ID` | 108 | `execution/prepared_media.rs:27` | `pub` | 唯一且等价 |
| `PROVIDER_WIRE_RESPONSE_CAPTURE_MAX_BYTES` | 522 | `execution/wire_capture.rs:167` | `private` | 唯一且等价 |
| `PROVIDER_WIRE_RESPONSE_CAPTURE_MAX_CHUNKS` | 526 | `execution/wire_capture.rs:171` | `private` | 唯一且等价 |
| `PROFILED_USAGE_GATE_MAX_PENDING_EVENTS` | 4231 | `execution/usage.rs:11` | `private` | 唯一且等价 |
| `JSON_TEMPLATE_MAX_ARRAY_ITEMS` | 5973 | `execution/json_template.rs:8` | `private` | 唯一且等价 |
| `JSON_TEMPLATE_MAX_OUTPUT_BYTES` | 5977 | `execution/json_template.rs:12` | `private` | 唯一且等价 |

## 可见性审核

以下70个节点只增加父模块可见性，其中测试夹具仍受测试条件限制，未将生产私有项升级为crate或对外API。

- `enum:ProviderWireRequestCaptureTarget`：`pub(super)`。
- `struct:ProviderWireCaptureTargets`：`pub(super)`。
- `struct:ProviderWireResponseCaptureGuard`：`pub(super)`。
- `ProviderWireResponseCaptureGuard::new`：`pub(super)`。
- `ProviderWireResponseCaptureGuard::observe_http_headers`：`pub(super)`。
- `ProviderWireResponseCaptureGuard::observe_chunk`：`pub(super)`。
- `ProviderWireResponseCaptureGuard::observe_websocket_frame`：`pub(super)`。
- `ProviderWireResponseCaptureGuard::set_outcome`：`pub(super)`。
- `ProviderWireResponseCaptureGuard::observe_normalized_event`：`pub(super)`。
- `ProviderWireResponseCaptureGuard::observe_failure`：`pub(super)`。
- `ProviderWireResponseCaptureGuard::set_outcome_if_unresolved`：`pub(super)`。
- `struct:ResponsesWebsocketSessionCacheEntry`：`pub(super)`。
- `struct:PreparedMediaCacheEntry`：`pub(super)`。
- `ProviderExecutionClient::execute_responses_websocket`：`pub(super)`。
- `ProviderExecutionClient::responses_websocket_disabled`：`pub(super)`。
- `ProviderExecutionClient::execute_http`：`pub(super)`。
- `struct:ProfiledUsageEventGate`：`pub(super)`。
- `ProfiledUsageEventGate::stage`：`pub(super)`。
- `ProfiledUsageEventGate::observe`：`pub(super)`。
- `ProfiledUsageEventGate::ensure_released`：`pub(super)`。
- `extract_profiled_usage_events`：`pub(super)`。
- `extract_profiled_usage_value_events`：`pub(super)`。
- `request_without_nonpersistent_responses_continuation`：`pub(super)`。
- `responses_websocket_cache_key`：`pub(super)`。
- `websocket_headers`：`pub(super)`。
- `emit_provider_transport_trace`：`pub(super)`。
- `emit_provider_transport_http_status_trace`：`pub(super)`。
- `emit_provider_transport_failure_trace`：`pub(super)`。
- `trace_provider_transport_failure`：`pub(super)`。
- `emit_responses_compact_response_shape_trace`：`pub(super)`。
- `update_semantic_output_observation`：`pub(super)`。
- `validate_stream_event`：`pub(super)`。
- `validate_request_policy`：`pub(super)`。
- `validate_request_projection_guards_before_encoding`：`pub(super)`。
- `upstream_protocol_for_route`：`pub(super)`。
- `route_uses_native_codex_responses`：`pub(super)`。
- `project_provider_request`：`pub(super)`。
- `apply_request_projection_profiles`：`pub(super)`。
- `apply_response_projection_profiles`：`pub(super)`。
- `render_complete_json_template`：`pub(super)`。
- `apply_mapping_step`：`pub(super)`。
- `project_credential_request`：`pub(super)`。
- `publish_wire_request_capture`：`pub(super)`。
- `provider_wire_request_capture`：`pub(super)`。
- `struct:StreamFrame`：`pub(super)`。
- `struct:IncrementalFrameParser`：`pub(super)`。
- `IncrementalFrameParser::new`：`pub(super)`。
- `IncrementalFrameParser::push`：`pub(super)`。
- `IncrementalFrameParser::finish`：`pub(super)`。
- `protocol_failure`：`pub(super)`。
- `llm_core_protocol_failure`：`pub(super)`。
- `compact_protocol_failure`：`pub(super)`。
- `compact_transport_failure`：`pub(super)`。
- `stream_protocol_failure`：`pub(super)`。
- `stream_protocol_failure_for_transport`：`pub(super)`。
- `stream_completion_failure`：`pub(super)`。
- `stream_completion_failure_for_transport`：`pub(super)`。
- `stream_transport_failure`：`pub(super)`。
- `stream_idle_timeout_failure`：`pub(super)`。
- `stream_transport_failure_with_websocket_close_code`：`pub(super)`。
- `provider_transport_unix_millis`：`pub(super)`。
- `allowlisted_rate_limit_headers`：`pub(super)`。
- `provider_request_id_from_headers`：`pub(super)`。
- `validate_response_content_type`：`pub(super)`。
- `read_bounded_response_text_with_capture`：`pub(super)`。
- `bundled_manifest_route`：`pub(super)`。
- `minimal_request`：`pub(super)`。
- `read_test_http_request`：`pub(super)`。
- `pinned_openrouter_route`：`pub(super)`。
- `deepseek_route`：`pub(super)`。

此前四个必要字段的父模块可见性保持：`StreamFrame.event_name/data` 和 `ProviderWireCaptureTargets.request/response`。媒体缓存项仅类型对父可见，全部字段私有；解析器缓冲、用量门控、Wire守卫和模板上下文内部状态保持私有。

根保留唯一共享客户端、构造和短执行门面；19个职责模块涵盖路由、编码、媒体、映射、协议流与诊断。HTTP/压缩/原生推理共用有界正文；HTTP和WebSocket共用流状态与用量门控；活动Socket游标证明、连接缓存项与恢复同域。原32公开顶层项、15个crate函数均保留根路径。两个必要父域字段族以及媒体/WebSocket缓存项的类型可见性保持最小范围；缓存项字段私有。唯一空Error实现随错误类型迁移。

## 充分性

03号及全部41个Rust产物已完成逐文件终审，根398行、19个生产子模块职责充分；完整依据见03-FINAL-REVIEW.md。整体完成6/15，另9个原始文件及最终连续五轮审核仍未完成，全局0/5，整体目标和计划保持进行中。
