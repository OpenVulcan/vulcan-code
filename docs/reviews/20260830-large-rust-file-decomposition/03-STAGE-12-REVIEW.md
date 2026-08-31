# 03：第九至十二阶段拆分后审核

## 范围与状态

- 日期：2026-08-31；继续原15文件目标，不缩减逐文件审核与最终连续五轮完整审核要求。
- 本轮拆出强类型失败、响应元数据、请求头、传输诊断四个职责；根7284→5246行。
- 根当前SHA-256：`c09e8f9b6cb89a3b495bb97ea160fcfe1f4dc2f52402c508241cae52d98d98f1`。
- 当前28个Rust产物：根、14个生产子模块、12个测试子文件、1个共享测试夹具。完整节点、测试路径及文件摘要见 `03-PRODUCTION-MOVES.md`、`03-TEST-MOVES.md`。
- 本批次迁移部分的结构审核与完整验证通过；03号整体尚未拆完，不计逐文件闭环或全局审核轮次。当前仍5／15、全局0／5。

## 拆分与不再细切的依据

| 产物 | 行数 | 职责与测试匹配 |
| --- | ---: | --- |
| `execution/failure.rs` | 759 | 唯一错误类型及完整分派/协议/传输分类、HTTP归类与最终脱敏 |
| `execution/failure/tests.rs` | 272 | 11项直接失败边界、重试、安全诊断回归 |
| `execution/response_metadata.rs` | 239 | 限流白名单、声明顺序请求标识、内容类型与Retry-After精确解析 |
| `execution/response_metadata/tests.rs` | 150 | 6项头事实、上限和精度回归 |
| `execution/request_headers.rs` | 312 | 完整最终头合并、类型化凭据、设备身份、WebSocket握手转换 |
| `execution/request_headers/tests.rs` | 140 | 2项头归属和原生Codex用户代理回归 |
| `execution/transport_diagnostics.rs` | 241 | 可选安全追踪、响应形状及真实宿主事件时间 |
| `execution/transport_diagnostics/tests.rs` | 19 | 1项有界ASCII结构标签回归 |

失败类别不能按相似字段合并或按每个短构造器另建文件；它们共同表达重试所依据的分派事实。元数据只是响应事实，不决策重试。请求头的协议、服务入口、路由和凭据来源有不同权限，必须保留统一合并顺序和明确限制。诊断的有限结构投影及开关同域，时钟虽然与追踪同属遥测，却不依赖追踪开关。

这些模块均已有完整职责，不按小函数继续碎片化；根剩余网络编排仍需后续处理。

## 行为与接口审核

### 失败分类

- `reqwest::Error::is_connect()` 是连接建立失败的现有证据；其他发送错误保留分派状态未知，不猜成未分派。
- 分派前协议错误与已分派流协议错误保持区别；截断或一般流传输失败的安全重试仍受是否已有语义输出约束。
- 空闲超时保留原显式子类型与干净检查点重放约定，不能按一般流错误一律改成不可重试。
- 远端压缩传输失败仍标为已分派且可重试；压缩协议失败已分派且不可重试。
- HTTP状态先按原code/type/message归类，再缩减和脱敏；最终精确密钥替换和通用URL处理顺序不变。
- 语义观察加重试安全的跨层测试仍在根，未为错误模块引入流状态机或复制错误类型。

### 响应事实

- 请求标识仅按路由声明顺序读取；未声明时不猜测常见头名。
- 限流诊断仍为原11项白名单；头值继续保持UTF-8边界和明确截断标记。
- 内容类型维持声明允许项、大小写不敏感及忽略参数规则；缺失、非法及不匹配仍显式失败。
- Retry-After纯十进制算法保持毫秒精度与非零余数向上取整；HTTP日期来源标记、过去日期归零和非法值拒绝不变。
- 解析入口继续供原生推理调用，不复制解析逻辑或把头事实当重试策略。

### 出站头与诊断

- 协议头、服务入口头、路由头和最后凭据/应用身份合并顺序不变；各层保留字段限制不统一改写。
- 认证投影仍基于实际类型化凭据；Codex账号、Google配额项目的必需/可选区别不变。
- 会话、工具及上传共用唯一Coding设备身份实现；既有设备名和设备ID缺省来源保持原样，没有增加猜测兜底。
- 原生Codex专有用户代理与所有非Codex协议的VulcanCode身份保持。非Codex常量移至请求头域，仅对父域开放，真实HTTP抓包测试显式引用。
- 可选日志不新增错误正文、请求头、凭据或响应值；形状标签保留64字符ASCII规范化、32个键与16项输出详情上限。
- 失败追踪包装器原样返回同一强类型错误；宿主时间戳独立于日志开关，纪元/整数溢出错误不变。

### 结构边界

- 原32个对外顶层节点：4个仍在根定义、28个经根显式重导出；15个原crate顶层函数保留原路径和范围。
- 18个顶层常量唯一且值等价；两个对外媒体常量及一个crate工具媒体时长常量保持原路径。
- 空 `std::error::Error` 实现与唯一错误类型一并迁移，未丢失或重复。它不被AST节点统计，因此单独在全部产物核对恰一份。
- 累计60个原私有节点仅开放到父模块，其中4个共享测试夹具仍仅测试编译；没有把生产私有节点改成crate或对外API。
- 唯一客户端、共享缓存字段、克隆和构造关系未变；无新依赖、协议、配置或持久化结构。

## 节点与测试保全

- 原364节点全部唯一对应，无缺失、复制或新增替代实现；前置属性保持。
- 250个生产区域节点全部严格词法一致；213项已迁移，37项仍在根。
- 原103测试全部发现并在644包级测试中实际通过；53项随职责迁移、50项原路径保留。无删除、ignore或断言弱化。
- 11个辅助节点完整保留；4个共享夹具已归位，7个仍留根。
- 全体严格词法比较363项一致；唯一差异仍是第八阶段Schema测试的两个单表达式match分支被rustfmt折叠。
- 本轮重新执行该命名用例的精确token证明：只变换Chat和Anthropic两个已查明pointer返回分支，要求其余所有token一致；原231、新229规范化token，输入、路径、断言及控制语义不变。通用比较器不放宽。详细依据见 `03-STAGE-08-REVIEW.md`。
- Kimi、MiniMax完整认证编码、真实HTTP抓包及语义观察后的失败等仍是跨层测试，没有为减少根行数强行塞入纯助手模块。

## 实际验证

以下终端命令均通过rtk执行；编译保留默认并发四，共享清单和进程资源测试使用单线程。

| 验证 | 结果 |
| --- | --- |
| `cargo check -p vulcan-provider-adapters --all-targets` | 通过，无新增警告 |
| `cargo check -p vulcan-provider-adapters --all-targets --features test-support` | 通过 |
| `cargo test -p vulcan-provider-adapters --lib -- --list` | 644项，精确执行域103项；未误计其他域16项 |
| `cargo test -p vulcan-provider-adapters --lib execution::failure::tests:: -- --test-threads=1` | 第九阶段11项通过 |
| `cargo test -p vulcan-provider-adapters --lib -- --test-threads=1` | 最终644通过，0失败、0忽略，27.77秒 |
| `cargo clippy -p vulcan-provider-adapters --lib -- -D warnings` | 通过 |
| `cargo test -p vulcan-agent-runtime --lib provider_runtime_dispatch::tests:: -- --test-threads=1` | 5通过 |
| `cargo test -p vulcan-agent-runtime --lib provider_transport_vulcan:: -- --test-threads=1` | 36通过 |
| `cargo test -p vulcan-agent-runtime --lib provider_tool_bridge::tests:: -- --test-threads=1` | 15通过 |
| `cargo test -p vulcan-service-kernel --lib media_upload::tests:: -- --test-threads=1` | 12通过 |
| `cargo check -p vulcan-service-kernel` | 9个关联包通过，24.61秒 |
| `cargo test -p vulcan-provider-adapters --doc` | 通过，实际0项，不计额外覆盖 |
| 格式检查、`git diff --check` | 通过 |

Windows既有链接器创建导出库提示未抑制；不宣称全仓测试风格Clippy通过。消费者合计68项全部通过，不重复累计同一用例多次运行。

## 自动修复与复验

1. 失败类型迁移后，根不再需要 `AiExecutionError`；清理该残留导入。
2. 元数据迁移后，根不再需要 `RetryAfterSource` 和 `ProtocolTransportProfile`；归还所属模块并清理根导入。
3. 请求头迁移后，清理根认证投影、HeaderMap、Codex用户代理及原生路由判断导入；原跨层测试仍使用的VulcanCode常量改为明确测试导入。
4. 以上均为迁移引起的导入归属调整；未增加警告抑制、未改业务表达式或测试断言。双特性检查、严格生产Clippy、完整包级回归复验通过。
5. 所有迁移节点、顶层常量、空Error实现和公开入口都有单独保全证据；不能仅凭编译成功判断没有遗漏。

## 充分性及剩余工作

03号当前仍不足以闭环：HTTP/WebSocket传输、活动连接缓存与熔断、远端压缩、共享流状态及50项测试最终归类尚未完成。后续按实际调用边界继续；根即使降至5000行以下也不会移出目标范围。

本批次审核不计作03号完整逐文件终审，也不计入全局连续五轮。整体仍5／15，全局0／5。

真实供应商账号、桌面提交、远端上传和压缩未验收；本地测试不冒充这些外部验收。

## 本轮变更

- 新增：上述8个Rust文件及本阶段审核记录。
- 修改：执行根、当前生产/测试映射、主记录、覆盖台账和实体计划。
- 删除：无文件或有效测试删除。
- 未提交、暂存、推送、清理或还原工作树；其他已完成拆分保留。
