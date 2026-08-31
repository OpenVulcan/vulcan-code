# 01：供应商运行时逐文件终审

## 结论与范围

2026-08-31：原始 `crates/vulcan-provider-adapters/src/runtime.rs` 的结构性拆分及逐文件审核闭环。原文件 20951 行，当前根模块 631 行；产物共 51 个 Rust 文件，其中 23 个含生产职责，其他为测试、测试接线及夹具。全部产物继续保留在最终全局审核范围，本记录不是一次全局审核，也不代表整个目标完成。

基线提交：`111540689b777ddc1229a6f17293a42818f2155e`。
最终根摘要：`fde2f8c613dd6bc68f319c397fda66a1ca242f4c2a546319969cca64a036901e`。
完整逐项证据见 [生产映射及内容摘要](01-PRODUCTION-MOVES.md)、[测试及夹具映射](01-TEST-MOVES.md) 和 [分段决策／审核记录](01-vulcan-provider-adapters-src-runtime.md)。

## 为什么拆分，以及现在为何足够

原文件同时持有凭据刷新、媒体上传、任务恢复、目录来源／策略、配置激活、会话租约、工具执行和模型选择等不同生命周期。现按实际类型、状态所有权及调用链分离，不是按固定行数切片。

根模块保留唯一 `PublicAiRuntime`、不可变已发布代际和激活前镜像，以及构造、共用时钟／截止预算、元数据入口。它们是多个职责真正共享的所有权边界；拆成额外中间对象会增加协调层或诱导复制状态，当前没有这种需要。

最大模块为 1507 行查询投影：其方法仍围绕同代际目录读取、设置视图和候选校验，底层来源、覆盖及发布生命周期已经分别归位。1224 行激活、1130 行目录和 1033 行媒体各自保持完整事务或生命周期；不为了继续缩小行数而切断发布／补偿、发现状态机或上传取消顺序。选择层已分离 Profile、修订和共用用户覆盖，不再承担执行或账号存储。

| 文件 | 行数 | 实际归属 | 对应原始测试数／覆盖 |
| --- | ---: | --- | --- |
| `runtime.rs` | 631 | 运行时组装、唯一共享状态与通用入口 | 2 |
| `runtime/credentials.rs` | 662 | 秘密仓库契约、冻结账号租约及实时凭据入口 | 3 |
| `runtime/media.rs` | 1033 | 本地媒体、上传预检／进度及媒体输入约束 | 3 |
| `runtime/async_tasks.rs` | 983 | 异步任务恢复、观测、截止与取消 | 7 |
| `runtime/activation.rs` | 1224 | 暂存清单、完整代际发布与补偿 | 15 |
| `runtime/catalog.rs` | 1130 | 发现、状态、来源暂存与目录镜像更新 | 5 |
| `runtime/catalog_sources.rs` | 426 | 签名／手工／发现来源及变体生成 | 3 |
| `runtime/catalog_policy.rs` | 730 | 无状态成员、能力、访问与绑定投影编排 | 17 |
| `runtime/catalog_policy/capabilities.rs` | 346 | 能力默认、限制与宿主策略 | 5 |
| `runtime/catalog_policy/account_access.rs` | 153 | 套餐及额度准入 | 4 |
| `runtime/catalog_policy/bindings.rs` | 352 | 协议绑定冻结与变体一致性 | 2 |
| `runtime/catalog_views.rs` | 1507 | 同代际设置投影、支持目录与候选校验 | 14 |
| `runtime/usage.rs` | 280 | 账号用量观测与访问证据保留 | 1 |
| `runtime/secondary_routes.rs` | 245 | 精确路由身份缓存、发现与有界淘汰 | 2 |
| `runtime/session_leases.rs` | 294 | 会话路由所有权与可逆撤销 Guard | 5 |
| `runtime/tool_targets.rs` | 501 | 清单工具目录、目标准入与冻结契约 | 3 |
| `runtime/tool_execution.rs` | 541 | 同一预算内的工具调用编排 | 由目标、异步、媒体与桥接消费者覆盖 |
| `runtime/tool_diagnostics.rs` | 110 | 脱敏及有界诊断 | 1 |
| `runtime/model_execution.rs` | 663 | 冻结模型执行与显式远端压缩 | 由凭据、调度与传输消费者覆盖 |
| `runtime/model_overrides.rs` | 223 | 共用用户投影、参数默认与覆盖校验 | 3 |
| `runtime/selection.rs` | 692 | 同代际账号、模型与精确路由选择 | 25 |
| `runtime/selection/profiles.rs` | 310 | 签名模型规则、显式空替换与资源约束 | 3 |
| `runtime/selection/revision.rs` | 153 | 语义目录与 Wire 路由摘要 | 2 |

共用测试支持为 399 行，策略测试适配为 82 行；原始 25 个测试／夹具节点全部保留。7 行选择测试入口只声明四个测试子模块，不制造生产抽象。

## 终审核查与问题闭环

| 检查项 | 当前证据与结论 |
| --- | --- |
| 节点完整性 | CodeKit 原始与当前均为 430 个类型／函数节点。逐项完整声明与函数体的 Rust 词法序列相等；0 缺失、0 重复、0 意外新增，全部前置属性一致。原生产区域 280 个节点中 260 个归入职责模块、20 个留在根；原生产区域的有效测试适配函数仍保留且单独标明。 |
| 非节点内容 | 12 个常量唯一存在，类型和值相等；每段根文件扣除精确迁移块与接线后，完整余留词法序列相等。原文件无 `include!`、文件位置宏或 `#[path]`，没有遗漏相对位置语义。 |
| 接线及公共契约 | 原始 31 个顶层公开项全部仍由根定义或重导出，crate 保持 `pub use runtime::*`；公共方法签名不变。共享内部入口只按真实调用放宽至父模块，状态字段不因测试放宽，条件编译及 Serde 属性保持。 |
| 状态与顺序 | 没有新增运行时状态实例。凭据仍按冻结账号取最新秘密；配置／目录仍先构建完整代际后发布；选择只消费冻结绑定与对应变体；租约撤销先校验再删除且未提交恢复；工具／异步预算和取消顺序均保留。 |
| 测试对应 | 原 125 项全部保留，123 项按实际职责迁移、两项根测试保留。全部名称、函数体、断言及属性匹配，并通过实际测试发现和执行；没有通过删除／忽略／弱化测试取得通过。 |
| 自动修复 | 移除迁移遗留导入；修正公开选择兼容参数、显式空 Profile、测试中的激活时序、根容器目录镜像、初始化旧供应商名单及暂存／发布说明。修复均不改变生产或测试表达式。 |
| 编辑安全复核 | 末尾实现块和根测试模块的闭合括号被明确保留；一次逆序导入补丁被上下文检查拒绝后，按当前实际位置重新定位，未绕过校验。所有节点及根余留比较最终通过。 |
| 充分性 | 生产与测试均已按职责归位，全部 51 个产物纳入映射；本组无超过 5000 行文件。结论依据职责和状态边界，不仅依据行数下降。 |

## 最终验证

全部命令通过 `rtk` 执行，保留仓库默认编译并发；共享清单与进程状态测试串行。

- `cargo check -p vulcan-provider-adapters --all-targets`：通过，无新增警告。
- `cargo check -p vulcan-provider-adapters --all-targets --features test-support`：通过。
- `cargo test -p vulcan-provider-adapters --lib runtime:: -- --list`：125 项，与完整迁移路径精确匹配。
- `cargo test -p vulcan-provider-adapters --lib -- --test-threads=1`：644 项通过；终审核查旧说明并修正后再次通过，最后一次 27.77 秒。
- `cargo clippy -p vulcan-provider-adapters --lib -- -D warnings`：通过。测试目标由全部目标编译和实际回归验证，未声称通过测试风格全仓治理。
- `cargo test -p vulcan-agent-runtime --lib provider_runtime_dispatch::tests:: -- --test-threads=1`：5 项通过。
- `cargo test -p vulcan-agent-runtime --lib provider_transport_vulcan:: -- --test-threads=1`：36 项通过。
- `cargo test -p vulcan-agent-runtime --lib provider_tool_bridge::tests:: -- --test-threads=1`：15 项通过。
- `cargo check -p vulcan-service-kernel`：9 个关联包编译通过。
- `cargo test -p vulcan-provider-adapters --doc`：通过，实际 0 项，不计为额外覆盖。
- 定向 `rustfmt --check --edition 2024`（根及其全部子模块）、`git diff --check`：通过。

未执行真实供应商账号、远程凭据续期、桌面上传或实际远端压缩；单元与本地消费者验证不表述为这些外部验收已经完成。

## 执行变更总结

1. 核心调整：拆开不相干生命周期，冻结状态仍唯一持有，原始行为、公开路径和有效测试保全。
2. 文件清单：修改原 `runtime.rs`；新增其目录下 50 个 Rust 产物；更新本记录、主记录、生产／测试映射、全局台账与实体计划。具体 51 个 Rust 路径和摘要全部列于生产映射。无删除文件，无提交、暂存或推送。
3. 关键代码：原生产区域 260 个节点按凭据、目录、激活、选择、执行等职责迁移；只改模块接线、必要内部可见性及事实明确的注释，不新增协议、库、Schema 或兼容回退。
4. 遗留与注意：01 无未关闭的拆分问题，但整个任务仍有十个原始目标未处理；当前逐文件 5／15、全局 0／5。全部产物后续仍须参加五轮连续完整零问题审核。

## 全仓复扫

按原忽略规则重新枚举 Rust 源码，目前严格超过 5000 行的文件为以下十项，与冻结基线剩余目标一致；未产生新的大文件。

| 当前文件 | 行数 |
| --- | ---: |
| `crates/vulcan-service-kernel/src/public_ai_settings.rs` | 16244 |
| `crates/vulcan-provider-adapters/src/execution.rs` | 14353 |
| `crates/vulcan-workspace-store/src/knowledge_base/runtime.rs` | 10521 |
| `crates/vulcan-plugin-runtime/src/runtime.rs` | 9716 |
| `crates/vulcan-provider-adapters/src/provider_manifest/validation.rs` | 9462 |
| `crates/vulcan-service-kernel/src/provider_manifest_sync.rs` | 9042 |
| `crates/vulcan-service-kernel/src/configured_inference.rs` | 7140 |
| `crates/vulcan-host-tools/src/file_runtime.rs` | 6537 |
| `crates/vulcan-service-kernel/src/session_runtime_registry_v1.rs` | 6521 |
| `crates/vulcan-provider-adapters/src/configuration.rs` | 5539 |
