# AI 扩展能力配置与回合恢复解耦

## 后续纠正：冷启动投影缺失与默认配置重建

用户后续截图确认上一轮仍未解决全部入口：新建会话重开后报 `has no runtime state`。原回归测试提前插入了运行态行，漏掉真实冷启动条件，上一轮的两轮范围复核不能代表此场景已通过。

本轮先移除夹具的预填运行态，定向测试实际复现同一错误，再修改生产代码：

- 能力 GET/SAVE 不再要求 `workspace.session_runtime_state` 必须存在；读取以 metadata 为准，已有执行投影才同步能力字段，不为能力弹窗创建执行适配器或加载回合。
- 新增 `service_rebuild_public_ai_session_provider_tool_selections`，贯通领域路由、RPC 契约登记、前端服务调用和错误态按钮。重建清除会话能力自定义选择，恢复继承全局默认值（`overridden=false`），不是永久保存一份脱离全局的默认值副本。
- 显式重建只替换 `config.selection.provider_tools`，然后按原契约解析其余 metadata；损坏的能力子树可以替换，但无关字段、会话身份或契约损坏仍拒绝，不能借重建覆盖会话。未加载路径保持原子发布及共享注册表门控，常驻路径复用唯一 writer。
- 默认配置解析和能力投影更新共用辅助函数；移除 SAVE 内重复的字段映射，不添加运行态补建或交互加载兜底。
- 前端保留重新加载，并增加“重建为默认配置”及影响范围说明；重建失败保留错误、禁用普通保存，重建中禁用重复操作，成功后重新读取，响应归属继续使用原代次保护。
- 复查时修正重建期间误显示“保存中”、无会话草稿显示重建说明、幂等写入绕过非法时间戳校验三个细节。

新增/扩展覆盖文件：`rpc_dispatch/configuration.rs`、`session_v1/documents.rs`、`settings-actions-service.ts`，以及原能力服务、窄存储、控件、文案与能力元数据测试文件。其他工作树修改保持原样。

本轮验证结果：

- 修复前：撤去预填运行态后，真实 GET 测试复现 `has no runtime state`（失败 1 项）。
- 修复后：`vulcan-service-kernel capability_metadata` 6 项通过，覆盖无适配器读写、真实索引/RPC、重开、默认重建、默认继承与覆盖区分、冷热 owner、后续 flush、并发加载和失败释放。
- `vulcan-workspace-store capability_metadata` 4 项通过，覆盖损坏/缺失交互、能力子树有效/语义损坏/结构损坏/缺失、无关 metadata 保留、非法时间戳与无关损坏拒绝。
- `vulcan-service-kernel rpc_` 16 项通过，包含前端 RPC 名称与内核领域路由一致性。
- `cargo check -p vulcan-service-kernel`、`cargo check -p vulcan-code-tauri` 通过；本轮未构建、替换或重启用户桌面进程。
- `npm run lint` 及能力控件现有 11 项测试通过。实际浏览器使用生产控件与模拟 RPC，验证错误态两种入口、失败保留、进行中互斥、非空全局默认能力自动载入、发送阻塞解除、未触发普通保存，以及切换会话后迟到回包不覆盖新会话。
- 第一轮收敛复核：修正上述细节后，重新追踪 GET/SAVE/REBUILD → 生命周期锁 → 注册表加载门 → 唯一 writer/冷存储原子发布 → 工作台提交；确认提交前失败不发布候选、模型路由租约不参与能力修改。本轮范围无新增问题。
- 第二轮收敛复核：反向核对浏览器按钮与 RPC 契约、空运行态夹具、重开后默认继承、无关字段对比及常驻后续 flush；注册表 44 项测试再次通过，定向格式检查与 `git diff --check` 通过。本轮范围无新增问题，连续两轮完成。未将上一轮四项清单资源失败算作已解决。
- 已删除本次临时浏览器验证页面和夹具，关闭测试页并停止独占 17671 端口的临时 Vite 进程；未更改用户会话数据或正在使用的桌面进程。

## 范围与根因

本次只修复扩展能力配置读写错误依赖交互内容的问题，不批量修改历史会话，不放宽回合完整性校验，不改供应商清单。

已追溯到提交 `acbb626058a983db413bf7283d5defa80e6eaa29`（会话持久化 V1 全链路切换）：迁移配置权威到 metadata 时，能力读取和保存入口都复用了 `session_v1_handle_from_state`。该入口会加载交互分片、重建回合派生状态及投影中断状态，因此配置弹窗间接触发了 `duplicate or mis-ordered closure sequence`。

会话内容能显示并不矛盾：转录 hydration 使用有界事件窗口读取，并不会执行同一条完整运行时重建链。能力配置属于 `metadata.config.selection.provider_tools`，没有读取交互内容的业务需求。

## 实施与收敛

1. 存储窄门面仅读取精确 metadata 文件，校验契约、会话身份及能力配置规范；不读取 segments、runtime 或 compactions。
2. 冷会话保存只原子替换 metadata 中的能力配置并递增文档修订，保留所有其他字段。常驻会话沿现有唯一句柄写入，避免后续 flush 覆盖新配置。
3. 纯配置访问与物理加载共用注册表准入槽；纯配置槽释放后，等待加载器重新申请加载，而不是把“没有发布运行时句柄”误认为加载失败。生命周期锁排除并发归档/删除。
4. GET 以 metadata 为权威，从中生成能力投影；继承默认值只存在于内存中，打开弹窗不会自动改写用户配置。移除原读取入口中的完整会话加载、重复协调、RefCell 提交及惰性持久化链。
5. 能力投影函数由目标生命周期模块移至配置模块，复用原实现，不复制映射逻辑；保持能力设置发布不参与模型路由租约协调的既有边界。
6. 前端明确区分加载中、成功和失败，读取失败时禁止空配置保存，提供重试；会话/读取代次变化后忽略迟到的保存响应。

## 文件覆盖

| 文件 | 本次内容 |
| --- | --- |
| `crates/vulcan-service-kernel/src/public_ai_settings.rs` | 能力 GET/SAVE 改为纯配置访问，移除读取时持久化 |
| `crates/vulcan-service-kernel/src/session_request_config_v1.rs` | 配置访问守卫、共享配置投影、冷热会话测试 |
| `crates/vulcan-service-kernel/src/session_runtime_registry_v1.rs` | 共享准入门及并发/失败释放测试；其余原有修改保留 |
| `crates/vulcan-service-kernel/src/session_goal_lifecycle/state_cache.rs` | 移出能力投影函数并调用配置模块；其余原有修改保留 |
| `crates/vulcan-workspace-store/src/workspace_store/session_v1_store.rs` | metadata 窄读写及损坏/缺失交互存储回归 |
| `crates/vulcan-session-core/src/v1/config.rs`、`document.rs` | 暴露并复用原能力配置校验，无新兼容规则 |
| `crates/vulcan-service-kernel/src/tests/capability_metadata_tests.rs`、`tests.rs` | 真实控制器与 SQLite 索引下的能力 RPC 回归 |
| `apps/vulcan-code-web/src/components/composer/composer-capability-controls.tsx`、`i18n-messages.ts` | 加载状态、重试、保存保护及文案 |

## 验证记录

- `cargo check -p vulcan-service-kernel`、`cargo check -p vulcan-code-tauri`：通过，无编译警告。
- `cargo test -p vulcan-workspace-store capability_metadata --lib -- --test-threads=1`：2 项通过，覆盖损坏交互 JSON、缺失交互/上下文文件、非法配置拒绝、幂等保存、无关字段保持。
- `cargo test -p vulcan-service-kernel capability_metadata --lib -- --test-threads=1`：4 项通过，覆盖真实 RPC、冷热会话切换、后续运行时 flush、并发加载、失败释放；新增首次继承不写盘断言后最终重跑仍全部通过。
- `cargo test -p vulcan-service-kernel session_runtime_registry_v1::tests --lib -- --test-threads=1`：44 项通过。
- `cargo test -p vulcan-service-kernel session_request_config_v1::tests --lib -- --test-threads=1`：7 项通过。
- `cargo test -p vulcan-session-core provider_tool_selection --lib -- --test-threads=1`：1 项通过，确认共用配置验证契约不变。
- `npm run lint`：通过；能力控件现有 Vitest 测试 11 项通过。
- 实际浏览器组件验证（只模拟 RPC，不连接用户服务）：加载时显示等待而非 0/0；保存按钮禁用；完成加载后列表自动可用；切换会话后读取失败显示错误且禁止保存；弹窗内“重新加载”恢复能力列表及发送状态；选择模型后保存可用，点击后确实发送一次保存请求并关闭弹窗。临时页面与临时服务均已清理。
- `git diff --check`：通过。

### 扩大验证的限制

`public_ai_settings::` 下 46 项测试有 42 项通过、4 项失败；四项单独运行仍失败，发生在调用本次配置接口之前：DeepSeek 清单缺失、OpenRouter/Alibaba 未注册、MiniMax 供应商缺失。涉及既有清单/资源加载边界，本次未修改清单、资源产物或放宽断言，不能宣称该组全绿。

本次没有构建、替换并启动用户正在运行的桌面程序；后端 RPC 验证使用隔离数据目录，不修改用户原始损坏交互文件。历史交互完整性错误仍由原验证器如实报告，不再阻挡独立的扩展能力配置。

## 审核记录

- 收敛检查：移除能力 GET 的无意义惰性持久化链；补齐旧会话迟到保存响应保护；能力映射复用并归回配置模块。
- 零问题审核轮次一：按能力 GET/SAVE → 配置访问 → 注册表门控 → metadata/常驻 writer 逐段复核，确认不再触发交互恢复、读取无持久化、归档/加载竞争有排除、失败释放不遗留槽。本次范围无新增问题。
- 零问题审核轮次二：交叉检查真实 RPC、并发加载、后续 flush、原子写入复用、前端加载及保存代次、临时文件清理与最终定向回归。本次范围无新增问题；上述四项资源相关失败仍单独保留，不计入通过。
