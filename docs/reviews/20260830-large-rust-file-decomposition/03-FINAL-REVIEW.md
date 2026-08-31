# 03：基础执行层逐文件终审

## 范围与结论状态

- 日期：2026-08-31。审查原始 `crates/vulcan-provider-adapters/src/execution.rs` 及其全部拆分产物，不以某一阶段或根文件行数代替全文件范围。
- 原始14353行，SHA-256：`6165dc24fbdb253f282503f346cec7860c69b4ed21fe9a30233860e133d088d1`。
- 当前根398行，SHA-256：`e56f31aed5efda645873cd322eff5429f11c439e19933491e57cda5ffb36b969`。
- 当前41个Rust产物：20个生产文件（根加19子模块）、21个仅测试编译文件（19个实际用例文件、一个六行分组入口、一个共享夹具）。
- 全部物理行共14871，新增的模块说明、明确导入和双语注释使总行数增加；根变短不是删除生产行为或测试。
- 全部结构、行为保全、包级及消费者验证已通过，03号逐文件终审闭环。当前已完成01、03、05、09、13、14，共6／15；全局连续零问题仍0／5。
- 整体15文件目标尚未完成；本次逐文件审核不是全范围五轮中的一轮。

## 为什么需要拆分

原文件同时持有冻结路由、媒体缓存、请求投影、模板解释、HTTP/WebSocket及压缩传输、用量与错误语义、线路捕获和测试。它们有不同状态所有者、输入事实与生命周期，集中一处难以审查调用顺序、可见性及测试匹配。

本次使用Rust原生模块和同一类型的实现块迁移，不引入核心依赖、协议、Schema或新状态容器。先追踪类型与调用，再按完整职责移动，不使用行数切片或仅搬走测试。

## 全部生产产物的归属与充分性

| 产物 | 行数 | 保留在一起的职责及不继续细切理由 |
| --- | ---: | --- |
| `execution.rs` | 398 | 共享客户端、构造、稳定导出、短执行门面、传输选择与统一错误脱敏；11个节点留根有完整共同归属 |
| `execution/route.rs` | 295 | 冻结路由身份、URL与精确协议归类；不按协议短分支拆散 |
| `execution/request_encoding.rs` | 1089 | 参数准入、协议编码、Profile/模型/凭据投影、最终Schema及思考约束；完整变换顺序同域 |
| `execution/projection_profiles.rs` | 849 | 封闭Profile映射；请求与响应来源及失败边界不同，保留不同入口与共享路径原语 |
| `execution/json_template.rs` | 666 | 带深度、数组与输出上限的递归模板解释器；上下文和限制同域 |
| `execution/prepared_media.rs` | 1291 | 媒体源身份、指纹复检、共享引用缓存与不同保留策略；缓存项字段私有，不复制缓存 |
| `execution/resource_download.rs` | 202 | 显式资源下载权限和主机后缀约束；不与实际传输或业务输出混合 |
| `execution/tool_output.rs` | 222 | 统一工具输出类型族及其序列化合同；类型不按字段零散拆开 |
| `execution/failure.rs` | 759 | 唯一强类型失败、分派证据、协议/传输/HTTP分类与脱敏；保持不同重试语义 |
| `execution/response_metadata.rs` | 239 | 声明顺序请求标识、内容类型、限流诊断和精确Retry-After事实；不决策重试 |
| `execution/request_headers.rs` | 312 | 最终出站头与凭据/应用身份边界；会话、工具和上传复用设备身份 |
| `execution/response_body.rs` | 104 | HTTP错误、压缩与原生推理共用的有界正文读取，保留独立捕获与原空闲超时 |
| `execution/responses_compact.rs` | 466 | 非流式压缩请求、完整请求截止时间与可重放检查点结果；不混入SSE状态 |
| `execution/http_transport.rs` | 556 | 一次完整HTTP请求和流解析生命周期；终态yield前捕获、错误用量及收尾顺序不切散 |
| `execution/responses_websocket.rs` | 890 | 连接缓存项、会话串行锁、游标证明、预览、分派和恢复；状态唯一归属 |
| `execution/stream_state.rs` | 68 | 两种传输共用的规范事件状态校验和语义输出观察；不重复安全判断 |
| `execution/framing.rs` | 403 | 带字节与帧上限的增量SSE/JSONL/NDJSON/JSON分帧，缓冲实现私有 |
| `execution/usage.rs` | 130 | Profile用量抽取、去重和响应开始前暂存门控；HTTP/WebSocket复用 |
| `execution/wire_capture.rs` | 534 | 请求快照、原始响应守卫、全量哈希及有限索引；首失败与取消/终态捕获同域 |
| `execution/transport_diagnostics.rs` | 241 | 可选有界日志、形状标签和独立宿主遥测时钟；不新增敏感字段 |

以上所有产物均有明确归属；最大生产文件1291行，没有把原巨型文件换名整体移到另一个巨型文件。根的两个工具调用方法保留目标归属校验并委托原工具执行引擎；它们是客户端短入口，不另建只有转发行为的碎片模块。

## 完整结构与接口审核

- 对原364个AST节点逐一映射，250个生产区域节点、103测试、11辅助节点全部唯一保留，无遗漏、重复或新增替代实现。
- 239个生产区域节点已归入职责模块；11个客户端、类型与入口节点留根。
- 32个原对外公开顶层项：3个仍在根定义、29个经根重导出；15个原crate函数保留原路径与范围。
- 18个顶层常量均唯一且值等价。两个公开媒体常量和一个crate工具媒体时长常量保持访问路径；后者留根维持原内部路径，不引入未使用重导出或警告抑制。
- 累计70个原私有节点仅开放到父域；没有提升成crate或对外API。Wire目标和返回帧的必要字段按原调用范围开放；媒体/WebSocket缓存项仅类型对父域开放，字段私有。
- 额外逐项核对156个结构体字段：只有Wire捕获目标的request/response与分帧结果的event_name/data四个必要字段从私有变为父域可见；其他字段可见性不变。常量仅VulcanCode用户代理按真实跨层测试需要开放至父域，未新增对外常量。
- 空 `std::error::Error` 实现不被AST统计，已在全部产物额外核对恰一份并随唯一错误类型迁移。
- 从根按全部 `mod` 声明遍历：41文件全部可达，40条模块边无悬空路径；21个测试文件均继承或直接受 `cfg(test)` 约束，生产模块没有导入测试夹具。
- 客户端构造、克隆及三个共享状态容器保持原表达式，没有复制缓存或改变锁的所有者。

## 行为审核要点

### 请求与投影

保持原准入和变换顺序：请求参数/字段守卫、核心Responses基线、目标协议转换、供应商/自定义/Profile/凭据投影与最终线路证明。思考和结构化输出约束仍检查最终请求体。自定义点分隔路径与Profile JSON Pointer保留不同语义。

普通非持久化Responses的游标清除归入纯请求规范化；活动Socket的精确游标所有权判断仍在WebSocket域，HTTP不依赖Socket模块规范化请求。

### 共享媒体、捕获与错误

媒体来源指纹、物化后复检、契约身份、复用作用域和持久会话/进程工具引用的不同过期策略未变。捕获保持诊断与业务边界：追踪写入失败不阻断原请求，原始字节先观察，首失败优先，终态或取消仍发布。

连接建立、分派状态未知、已分派协议错误、语义输出后传输失败、截断与空闲超时保持不同分类；没有用统一错误包装抹去安全重试证据。HTTP原始code/type/message先归类再脱敏，精确秘密替换顺序不变。

### HTTP、压缩与WebSocket

- HTTP保留响应元数据事件、失败正文用量、分帧/Profile/解码顺序、用量暂存与释放；终态捕获仍在yield前固定。
- 压缩保留专用字段裁剪、非空原指令、头截止时间及完整请求剩余时间；响应必须存在合法可重放项和有效不透明检查点。
- 有界正文读取仍由三条调用链共用。未因迁移调整字节上限、UTF-8宽容解码、空闲超时或捕获顺序。
- WebSocket保留缓存前后双检、仅淘汰无外部持有者的会话、时间与键的确定性LRU、粘性熔断上限及非多路复用会话锁。
- 有效请求预览读取与分派同一缓存；控制帧恢复、真实传输失败熔断和本地协议拒绝弃用响应保持原分支，未增加协议或账号回退。
- 认证头、设备身份、请求标识声明顺序、Retry-After精度和可选诊断安全边界保持。

## 全部测试匹配与保全

| 职责 | 原有测试数 | 当前归属 |
| --- | ---: | --- |
| 分帧 | 4 | framing/tests.rs |
| 用量及真实清单声明 | 6 | usage/tests.rs |
| 原始捕获守卫 | 1 | wire_capture/tests.rs |
| 有界模板 | 3 | json_template/tests.rs |
| 媒体生命周期 | 8 | prepared_media/tests.rs |
| Profile映射 | 5 | projection_profiles/tests.rs |
| 路由 | 2 | route/tests.rs |
| 请求编码内部约束 | 5 | request_encoding/tests.rs |
| 失败分类与脱敏 | 11 | failure/tests.rs |
| 响应元数据 | 6 | response_metadata/tests.rs |
| 请求头 | 2 | request_headers/tests.rs |
| 诊断 | 1 | transport_diagnostics/tests.rs |
| 压缩检查点 | 3 | responses_compact/tests.rs |
| HTTP真实网络 | 6 | http_transport/tests.rs |
| 共用语义观察 | 1 | stream_state/tests.rs |
| Socket复用与恢复 | 6 | responses_websocket/tests.rs |
| 跨协议保真 | 8 | tests/protocol_fidelity.rs |
| 供应商合同 | 18 | tests/provider_contracts.rs |
| 当前清单等价 | 7 | tests/manifest_equivalence.rs |

合计103，全部可发现且在644包级回归中通过。没有删除、忽略、改名、弱化断言或增加占位用例。

资源下载策略及工具结果类型继续由工具桥接消费者覆盖；共享正文没有新建虚假独立用例，原HTTP错误路径及原生推理回归保持。这里不声称新增了此前不存在的网络分支覆盖。

11个辅助节点均保留：共享test_support中5个函数及1个测试专用运行时来源trait实现；其余5个分别是WebSocket私有游标夹具、供应商合同私有路由3项、清单等价私有客户端。所有夹具都有实际调用，不复制通用构造。

旧测试名含legacy并不意味着无效。当前这些用例仍验证真实清单、核心用量声明及最终Wire合同，因此保留原断言，而非按名称删除。

## 两项限定格式差异

250生产节点全部严格词法一致；整个364节点中362项严格一致、2项测试存在rustfmt引起的精确格式变化。101测试体严格一致，其余2项已经分别证明，不把它们误报成严格一致。

1. `encode_request_preserves_structured_output_schema_for_supported_protocols`：Chat与Anthropic两个match分支分别只返回固定JSON Pointer。去掉单表达式块后231→229规范化token；只允许这两个精确分支的变换，其余全部相同。
2. `apply_patch_standard_function_survives_production_protocol_projection`：去掉一次 `unwrap_or_else(|error| { panic!(...) })` 的闭包块，717→715规范化token。闭包内只有同一发散panic宏，无声明、附加语句或拥有型局部值；错误文本仍是 `replay {profile_id}, legacy={legacy}: {error:?}`，输入、历史投影与全部assert不变。

检测器初次报告第二项为未审核差异并停止后续验证；逐token定位和语义核验后才加入该命名用例的精确证明。通用词法比较器未放宽，不忽略任意大括号，也不以测试通过替代差异解释。

## 实际验证记录

全部shell命令通过rtk；保留默认Cargo编译并发四，涉及清单及进程资源的测试明确使用单线程。

| 验证 | 结果 |
| --- | --- |
| `cargo check -p vulcan-provider-adapters --all-targets` | 通过，无新增警告 |
| `cargo check -p vulcan-provider-adapters --all-targets --features test-support` | 通过 |
| `cargo clippy -p vulcan-provider-adapters --lib -- -D warnings` | 通过 |
| `cargo test -p vulcan-provider-adapters --lib -- --list` | 644项；按精确execution前缀对应103项，其他域16项不误计 |
| `cargo test -p vulcan-provider-adapters --lib -- --test-threads=1` | 644通过，0失败、0忽略，27.93秒 |
| 第十四阶段HTTP定向回归 | 6通过 |
| 第十五阶段WebSocket定向回归 | 6通过 |
| 调度消费者 `provider_runtime_dispatch::tests::` | 5通过 |
| 传输消费者 `provider_transport_vulcan::` | 36通过 |
| 工具桥接消费者 `provider_tool_bridge::tests::` | 15通过 |
| 服务媒体消费者 `media_upload::tests::` | 12通过 |
| `cargo check -p vulcan-service-kernel` | 9个关联包通过，23.38秒 |
| `cargo test -p vulcan-provider-adapters --doc` | 通过，实际0项，不计额外覆盖 |
| 格式检查、`git diff --check` | 通过 |

消费者合计68项通过，不把同一用例多次运行重复计数。Windows既有链接器创建导出库提示未抑制；不声明全仓测试风格Clippy通过。

## 发现、自动修复及复验

1. 每次迁移后清理只属于原模块的残留导入，测试导入回到对应测试域；无警告抑制或宽泛可见性兜底。
2. 编码前单步映射守卫在早期阶段确实由父域使用，按实际调用开放必要入口；未复制映射实现。
3. 模板和媒体阶段发现两处函数说明错挂在相邻常量/助手上，已归还真实方法；Schema约束说明补齐英文首行。
4. 本轮复核发现Codex游标夹具只被三个WebSocket用例使用，已从共享夹具收回测试文件私有；未留下多余公共测试接口。
5. 最终归类中补齐六处原仅英文的供应商测试逻辑注释中文行，修正根模块说明；没有改业务或测试表达式。
6. 两项格式差异分别按上述精确规则复验；364节点、18常量、属性、模块可达性、103测试发现及最终执行均重新核查。

当前结构充分性与全部本地验证均已成立，03号状态改为逐文件闭环，无未解决的本次拆分问题。真实账号供应商调用、桌面交互、远端上传/压缩未执行，不冒充外部验收。

## 全仓复扫

2026-08-31按相同忽略规则重新枚举Rust文件。严格超过5000行的9个文件均来自原名单，无新大文件；03号及其全部产物继续保留在最终全局审核范围。

| 剩余原始路径 | 当前行数 |
| --- | ---: |
| `crates/vulcan-service-kernel/src/public_ai_settings.rs` | 16244 |
| `crates/vulcan-workspace-store/src/knowledge_base/runtime.rs` | 10521 |
| `crates/vulcan-plugin-runtime/src/runtime.rs` | 9716 |
| `crates/vulcan-provider-adapters/src/provider_manifest/validation.rs` | 9462 |
| `crates/vulcan-service-kernel/src/provider_manifest_sync.rs` | 9042 |
| `crates/vulcan-service-kernel/src/configured_inference.rs` | 7140 |
| `crates/vulcan-host-tools/src/file_runtime.rs` | 6537 |
| `crates/vulcan-service-kernel/src/session_runtime_registry_v1.rs` | 6521 |
| `crates/vulcan-provider-adapters/src/configuration.rs` | 5539 |

## 执行变更总结

### 核心修复与调整概述

原执行巨型文件按十九个实际生产职责拆分，保留398行共享客户端与稳定入口；全部有效测试及辅助实现归位，接口、状态所有者和生产表达式保持。已解决迁移导入、夹具可见性和注释归属问题。

### 文件变更清单

- 修改：`crates/vulcan-provider-adapters/src/execution.rs`；其完整子目录、实体计划、覆盖台账及03号记录持续更新。
- 新增：`execution/`下40个Rust文件，完整路径、行数与摘要见 `03-PRODUCTION-MOVES.md`；包括19个生产子模块及21个测试条件文件。
- 新增记录：原始测试基线、当前生产/测试映射、各阶段审核及本终审记录；原始目标主记录持续保留全部决策。
- 删除：没有删除文件或有效测试。

### 关键代码调整详情

同一ProviderExecutionClient的方法按领域分布到原生Rust实现块；公开路径经根重导出。HTTP、Socket、压缩各自生命周期完整；通用编码、映射、正文、流状态、用量和失败分类保持唯一实现。测试按真实被测边界接入，不以改断言配合拆分。

### 遗留问题与注意事项

03号已完成逐文件闭环。整体仍需处理另外9个原始目标并完成连续五轮全范围零问题审核，当前6／15、全局0／5；不能因本文件终审通过而归档整体计划。没有提交、暂存、推送、清理或还原用户工作树。阶段记录中的早期状态保留为历史证据，最新状态以本终审和覆盖台账为准。
