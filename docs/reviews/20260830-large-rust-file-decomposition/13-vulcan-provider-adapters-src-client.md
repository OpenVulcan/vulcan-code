# 13：crates/vulcan-provider-adapters/src/client.rs 拆分记录

## 基线

- 原始路径：`crates/vulcan-provider-adapters/src/client.rs`。
- 原始行数：5695。
- 原始内容摘要（SHA-256）：`ec5e0f56126149334d3b0d80aa8e148345c15e4252f8c29ebc9596a331757146`。
- 基线提交：`111540689b777ddc1229a6f17293a42818f2155e`。
- 当前状态：已完成结构拆分、测试迁移和逐文件审核；继续纳入全局完整审核范围。

## 职责、类型与调用证据

现有文件持有公开的 `ProviderMetadataClient` 契约、`HttpProviderMetadataClient` 客户端与共享目录句柄，以及强类型发现错误；同时混合了模型发现／分页／Manifest 授权、HTTP 响应上限和诊断脱敏、MiniMax 注册探测、OpenRouter 二级路由、六种账号用量协议及精确十进制运算。用量响应 DTO 与其解析实现目前相隔数千行。

`lib.rs` 公开 `client` 并执行 `pub use client::*`；`PublicAiRuntime` 通过 `with_bundled_manifests` 持有同一目录句柄，刷新目录使用该客户端及已验证路由函数。公开路径和唯一目录句柄必须保留。供应商接入与 Manifest 作者手册已经读取，此次不修改清单、协议选择、账号用途或网络权限。

## 拆分决策与充分性

- 为什么需要拆分：发现目录、账号用量、注册探测和二级路由不是同一职责；按类型归属把响应 DTO 与解析／查询放在一起，能消除跨数千行查找，同时保留统一客户端门面。
- 哪些部分不需要拆分及原因：公开 trait、强类型错误、客户端共享状态和入口编排保留根文件；不把一条分页请求或一个账号用量事务拆成机械步骤。精确十进制运算保持单份实现，由实际四类用量消费者复用。
- 预期模块与类型归属：`model_discovery`、`http`、`minimax_registration`、`secondary_routes`；`account_usage` 按既有协议归属拆为 CommandCode、Kimi、MiniMax、Tavily、OpenRouter、DeepSeek 与共享十进制模块。仅对应现有宿主原语，不新增供应商规则或备用协议。
- 当前拆分是否足够：足够。根文件 619 行，保留公开契约、唯一客户端与分派编排；生产子模块按真实职责拆分，最大 795 行。账号用量按现有独立协议归属，DTO 与解析同处；31 行用量入口只负责模块接线和共用时钟，不再堆积业务。

## 测试保全与匹配

- 原始测试清单、数量、属性和夹具：41 项有效测试，包括真实本地 HTTP 请求、分页、鉴权分类、精确金额和清单授权；全部通过，0 忽略。6 个 HTTP／模型夹具全部保留。
- 测试迁移映射：见 [逐项测试映射](13-TEST-MOVES.md)。41 项全部保留，其中 28 个普通测试、13 个异步测试；6 个 HTTP／模型夹具只在模型发现测试中保留一份。
- 测试发现与实际执行：包级拆分前后均为 644 项，客户端 41 项按映射改模块路径，其余 603 项名称不变；完整发现列表完全匹配。客户端 41 项及包级 644 项全部通过，失败和忽略均为 0。

## 实施与验证

原文件 5695→619 行，新增 12 个生产子模块（含用量入口）和 9 个就近测试文件：

| 模块 | 生产行数 | 有效测试数 | 测试行数 |
| --- | ---: | ---: | ---: |
| 根契约与分派 | 619 | 由模型发现集成测试覆盖编排 | — |
| `model_discovery` | 795 | 22 | 1257 |
| `http` | 448 | 3 | 53 |
| `minimax_registration` | 145 | 2 | 42 |
| `secondary_routes` | 210 | 原文件无独立直测 | — |
| `account_usage` 入口 | 31 | 通过协议用例共用 | — |
| `account_usage/command_code` | 589 | 4 | 165 |
| `account_usage/kimi` | 435 | 3 | 70 |
| `account_usage/minimax` | 260 | 2 | 54 |
| `account_usage/tavily` | 223 | 2 | 27 |
| `account_usage/openrouter` | 90 | 原余额算法测试迁入共享十进制模块 | — |
| `account_usage/deepseek` | 168 | 1 | 22 |
| `account_usage/decimal` | 156 | 2 | 33 |

实际命令均通过 `rtk` 执行，保留仓库编译并发，共享清单和 HTTP 夹具验证串行：

- `cargo check -p vulcan-provider-adapters --all-targets`：通过。
- `cargo test -p vulcan-provider-adapters --lib -- --list`：644 项，与完整迁移预期逐项相等。
- `cargo test -p vulcan-provider-adapters --lib client:: -- --test-threads=1 --quiet`：41 项通过。
- `cargo test -p vulcan-provider-adapters --lib -- --test-threads=1 --quiet`：644 项通过。
- `cargo clippy -p vulcan-provider-adapters --lib -- -D warnings`：通过。
- `cargo check -p vulcan-service-kernel`：通过，包括 Provider 配置、Agent 与 Service 消费者链。
- `cargo test -p vulcan-provider-adapters --doc`：通过，实际 0 项文档测试，不将其记为额外覆盖。
- 本目标全部 22 个文件的定向 `rustfmt --check --edition 2024 --config skip_children=true` 与 `git diff --check`：通过。

没有执行真实账号用量网络调用或桌面重启验收；本地 HTTP 夹具不是已认证供应商端到端验收。没有修改资源或同步运行产物。

## 拆分后审核与自动修复

1. **结构与完整性**：全部生产区 117 个节点与测试区 47 个函数逐项核对，0 新增、0 丢失、0 行为差异。生产区数量包含原有测试专用块追加桥接函数。详见 [生产映射与可见性](13-PRODUCTION-MOVES.md)。
2. **协议和状态归属**：公开 trait 与客户端私有状态保持根所有权；模型发现仍使用唯一目录句柄和显式 Manifest 权限。六类用量协议的输入／输出 DTO 与现有解析归属一一对应，没有引入具名供应商的额外执行分支或协议探测。
3. **测试匹配**：HTTP 真实夹具、分页上限、循环拒绝、鉴权分类、精确金额、清单代际与模型协议变体覆盖完整保留；用量的“未知上限”和“无限”仍分别测试，没有为了过测合并语义。
4. **接口和条件编译**：原公开路径、trait 签名及原 `#[async_trait]`、Serde 属性保持；测试仍由各自 `#[cfg(test)]` 接线。源码没有需重定位的 `include!` 或文件位置宏。
5. **自动修复记录**：移除误贴在 Tavily 查询前的 OpenRouter 中文注释；修正 OpenAI／Anthropic HTTP 夹具参数注释归属及自定义目录路径说明，均为源码事实明确的文档修正。没有删除任何测试、修改断言或生产逻辑。首次拆分编译与严格检查通过，格式和回归复验通过。
6. **充分性**：根职责稳定，发现、HTTP、注册、二级路由与账号用量已分离；分页循环及精确金额算法保持完整，不做固定行数切片。逐文件审核闭环；全局连续零问题仍为 0／5，不能把本次包级测试当作全局轮次。
