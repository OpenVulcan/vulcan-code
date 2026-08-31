# 03：第七至八阶段拆分后审核

## 范围与状态

- 日期：2026-08-31；接续既有目标，不缩小15文件及最终连续五轮完整审核要求。
- 本轮完成冻结路由、同步请求编码职责迁移；根9133→7284行。
- 根当前SHA-256：`db4038d1963ac1322eff1cc889d99d519e5e741d89c25ac6fd0ea36351a5b216`。
- 当前共20个Rust产物：根、10个生产子模块、8个测试子文件、1个共享测试夹具文件。当前完整映射及摘要见 `03-PRODUCTION-MOVES.md`、`03-TEST-MOVES.md`。
- 本轮已迁移部分审核通过；**03号整体尚未拆完，不计逐文件闭环或全局审核轮次**。整体仍5／15，全局0／5。

## 拆分与不继续细切的理由

| 产物 | 当前行数 | 职责及充分性 |
| --- | ---: | --- |
| `execution/route.rs` | 295 | 冻结路由的身份校验、URL、精确协议家族和原生Codex判断同域，不按短分支拆散 |
| `execution/route/tests.rs` | 111 | 两项真实清单回归：内置全集协议映射与专属用量Profile绑定 |
| `execution/request_encoding.rs` | 1063 | 统一请求准入、核心协议编码、供应商/自定义投影和最终线路证明，保持完整变换顺序 |
| `execution/request_encoding/tests.rs` | 378 | 五项最终Schema、思考、自定义覆盖和跨方言拒绝回归 |
| `execution/test_support.rs` | 146 | 原唯一清单夹具加三个原共享夹具；根和子模块共享，不重复实现 |

三个共享夹具为最小请求、固定OpenRouter路由与DeepSeek路由。它们仍仅测试条件编译，所有生产模块均不依赖测试代码。

同步请求编码不持有活动WebSocket会话状态；相应异步有效请求预览继续留在传输编排。跨层Gemini动态路径、认证请求头、协议回放和真实网络分派用例仍保留，不能仅按测试名称搬动。

## 行为与接口审核

- 唯一路由类型的字段、验证、默认Profile复制、修订身份不变；原公开类型和crate协议归类入口由根重导出。
- 编码顺序不变：参数准入→仅请求字段的守卫→核心Responses基线→目标协议转换→采样温度→冻结Profile→二级端点→自定义投影→最终Schema校验。
- 必需无思考校验仍检查完成全部投影后的最终请求体；固定关闭模型也不能通过矛盾的开启思考字段。
- 自定义模型点分隔路径与Profile JSON Pointer保持不同的解析、覆盖与错误语义，不引入模糊兼容。
- 远端压缩实际调用的策略、编码前守卫、供应商投影和凭据投影四个助手仅对父执行域开放；没有复制校验或遗漏压缩分支。
- 原客户端、共享缓存容器、构造和流类型不变；没有引入新核心库、协议、持久化结构或配置。
- 32个对外顶层节点：5个仍在根定义、27个由根显式重导出；15个原crate顶层函数访问路径和范围保留。
- 18个顶层常量唯一且值等价，两个对外常量与一个crate内常量保持既有作用域，未扩大为额外对外API。

## 原始节点与测试保全

- 364原始节点全部唯一保留，无缺失、复制或新增替代实现；前置属性一致。
- 250生产区域节点全部严格词法一致，其中174个已迁移、76个仍在根。
- 103测试全部可发现且实际运行：33项已迁移、70项仍保留原路径；无删除、ignore或断言弱化。
- 11个辅助节点全部保留，其中4个共享夹具已归位，7个仍在根。
- 严格词法比较结果为363一致、1个测试存在以下已核实格式差异；不把它记录成364个严格词法一致。

### 唯一限定格式差异

测试 `encode_request_preserves_structured_output_schema_for_supported_protocols` 去掉一层外部模块缩进后，rustfmt将两个只含返回表达式的match分支折成单行：

- Chat分支仅返回 `encoded.body.pointer("/response_format/json_schema/schema")`。
- Anthropic分支仅返回 `encoded.body.pointer("/output_config/format/schema")`。

旧分支是 `=> { 表达式 }`，新分支是 `=> 表达式,`。内部没有声明、附加语句、拥有型临时值或控制流变化；两个分支的路径与返回表达式逐token相同，Schema输入和assert完全不变。

限定核验只转换该命名用例的这两个精确分支，且要求转换后的全部token序列与新函数相同（原231、新229个规范化token）。任何其他差异仍报错。通用比较器没有放宽、没有增加rustfmt跳过属性，也没有使用“测试通过”掩盖差异。

## 实际验证

所有验证保留编译并发上限，共享清单/进程资源回归使用单线程。

| 验证 | 结果 |
| --- | --- |
| `cargo check -p vulcan-provider-adapters --all-targets` | 通过，无新增警告 |
| `cargo check -p vulcan-provider-adapters --all-targets --features test-support` | 通过 |
| `cargo test -p vulcan-provider-adapters --lib -- --list` | 644项，其中本文件精确103项 |
| `cargo test -p vulcan-provider-adapters --lib execution::route::tests:: -- --test-threads=1` | 第七阶段2项通过 |
| `cargo test -p vulcan-provider-adapters --lib -- --test-threads=1` | 最终644通过，0失败、0忽略，27.83秒 |
| `cargo clippy -p vulcan-provider-adapters --lib -- -D warnings` | 通过 |
| `cargo test -p vulcan-agent-runtime --lib provider_runtime_dispatch::tests:: -- --test-threads=1` | 5通过 |
| `cargo test -p vulcan-agent-runtime --lib provider_transport_vulcan:: -- --test-threads=1` | 36通过 |
| `cargo test -p vulcan-agent-runtime --lib provider_tool_bridge::tests:: -- --test-threads=1` | 15通过 |
| `cargo test -p vulcan-service-kernel --lib media_upload::tests:: -- --test-threads=1` | 12通过 |
| `cargo check -p vulcan-service-kernel` | 9个关联包通过，19.59秒 |
| `cargo test -p vulcan-provider-adapters --doc` | 通过，实际0项，不计额外覆盖 |
| 格式检查、`git diff --check` | 通过 |

消费者合计68项，不重复累计同一用例的多次运行。Windows既有链接器提示未抑制；未声明全仓测试风格Clippy通过。

## 发现、修复与复验

1. 路由和请求编码迁移后残留根导入：逐项移除生产不再使用的导入，仅测试需要的协议/模型参数导入归回测试；双特性检查与严格Clippy通过。
2. 原最终Schema校验函数缺少英文首行说明：补齐与已核实逻辑一致的双语说明，未改函数体。
3. 一项测试出现rustfmt单表达式块折叠：保留实际格式化结果，按上述精确token变换单独留证，断言未变，不增加广泛例外。
4. 请求的输入守卫和压缩分支共享关系在迁移前已追踪；四个必要父域入口完成接线，完整包级和消费方回归通过。

未留下本轮未解决的编译或回归失败。上述阶段性结论不代替03号完整文件终审，更不计入全局连续零问题轮数。

## 当前仍不足

根仍有HTTP/WebSocket传输、活动连接缓存/熔断、远端压缩、请求头、传输诊断、失败分类与70项待最终归类测试。下一轮继续按这些实际职责推进；即使后续根降到5000行以下，也仍需检查所有职责及拆分产物的充分性。

真实供应商账号、桌面提交、远端上传/压缩未执行，本地回归不能冒充这些外部验收。

## 本轮变更

- 新增：路由与请求编码两组生产/测试文件，共4个Rust文件，以及本审核记录。
- 修改：执行根、共享测试夹具、实体计划/台账/主记录和最新迁移映射。
- 删除：没有删除文件或有效测试。
- 未提交、暂存、推送、清理工作树；其他已完成拆分保持。
