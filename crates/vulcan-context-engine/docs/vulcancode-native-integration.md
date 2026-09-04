# VulcanCode 原生集成边界

## 来源与维护权威

本 crate 从独立仓库 `D:\openvulcan\vulcan-context-engine` 以白名单方式一次性迁入，来源提交为 `654951cadc59f6902b4d3ac37a432de90e4a905f`，许可证为 MIT。迁入后的 VulcanCode 工作区是唯一维护权威，不使用 submodule、subtree 或运行时跨仓依赖。

实现借鉴 OCE 服务端与 `oce-client` 的领域流程，但不复制 MCP、HTTP、Milvus、Provider 客户端或进程模型。

## 领域内核职责

`VceEngine` 负责：

- 全量、路径级和强制重建刷新；
- 稳定 Blob/Chunk/Symbol 身份、结构分析、回退和分批向量化；
- stage、abort、CAS commit 所需的完整事务请求与严格回执校验；
- dense、exact symbol、path 多路召回，RRF、可选重排、路径提升和覆盖选择；
- overlay、状态快照、管理查询和只读维护计划；
- `VceToolCatalog` 中唯一 AI 可见的 `codebase_retrieval` 定义及 HostOnly 生命周期映射。

领域内核只依赖 `VceAiPort`、`VceSourcePort` 和 `VceIndexPort`，不创建外部资源。

## VulcanCode 宿主职责

### HostAI

`vulcan-service-kernel::vce::VceHostAiAdapter` 为每个启用项目向共同 `HostAiPort` 注册固定 capability：

- usage scope 固定为 `InternalBackground`；
- Embedding、查询改写 LLM 与 Rerank 均为启动前必需能力；
- 查询改写固定发送 `ReasoningEffort::None`，不提供用户推理档位；不能明确关闭推理或请求投影无法表达关闭推理的 LLM 不可用于 VCE；
- registration、consumer profile、vector space、purpose、维度与物理指纹在边界校验；
- 每次调用刷新 capability snapshot，不继续使用陈旧 Provider 路由；
- VCE Future 提前丢弃时向调用上下文传播协作取消；已经启动的 LLM execution 还会收到显式取消。

VCE 与 VMM、知识库复用同一 HostAI 契约，但不共享 registration、consumer 数据或向量空间。

### 全局配置与失败关闭

“插件”功能区菜单中的固定“语义引擎 / ContextEngine”设置页是 VCE 唯一模型配置入口；它与“LuaSkills 设置”同级，不属于“插件管理”内容。该页面写入公共 AI 的 `vce` consumer profile，并要求同时配置：

- 支持 Document/Query 专用任务或 `ProviderDefault` 通用任务、Dense Float 且维度可确定的 Embedding 路由；
- 支持宿主校验 JSON 输出且能固定关闭推理的查询改写 LLM 路由；
- 支持 `Disabled` 或 `ProviderDefault` 截断策略的 Rerank 路由。

后端使用当前凭据连接、模型目录、操作绑定、用途范围和请求投影计算唯一权威就绪状态。任一路由缺失、模型被移除或禁用、能力不兼容时均失败关闭：停止项目运行时与文件监听，隐藏项目代码上下文入口，不向 AI 暴露 `codebase_retrieval`，并拒绝直接管理与检索调用。公共 AI 配置每次成功发布后都会触发 VCE 全项目重协调；Embedding 物理指纹变化会重建项目索引，LLM 或 Rerank 路由变化则复用当前索引并刷新 HostAI 路由。

VCE 不持久化 Provider 私有配置，也不提供 LanceDB/SQLite 选择项。模型调用由 VulcanCode 注册的 HostAI 提供，索引存储由 VLDB Controller 的 LanceDB 与 SQLite 数据面提供。

### 数据面

`vulcan-workspace-store::vce_data_plane` 通过 VLDB Controller 持有：

- `vce_` 前缀的 SQLite 项目绑定、向量空间、generation、暂存、checkpoint、状态快照、精确符号、operation、提交回执与清理队列；
- 每个物理指纹和维度隔离的内容、路径 LanceDB 表；
- 事务内 CAS 检查、幂等提交重放、未发布 generation 隐藏、崩溃恢复和派生向量清理。

查询以当前 checkpoint 的完整 Blob 成员集合为可见性边界。未变化 Blob 可跨 generation 复用，因此不使用“只等于最新 generation”的错误过滤规则。

### 源码与监听

`VceProjectSourceAdapter` 只从已规范化项目根读取源码，拒绝父目录穿越、符号链接逃逸、二进制、超大文件和读取期间发生变化的内容。初扫先生成 metadata-only 清单，VCE 只拉取实际缺失 Blob 的正文。

VCE 不创建物理文件 watcher。service-owned `WorkspaceWatcherCenter` 只有一套 notify backend，Git 与 VCE 使用独立 subscriber。宿主在首次全量扫描前先建立项目监听，以关闭扫描与监听之间的事件空窗；VCE subscriber 负责项目路径归并、节流、同项目单飞、取消后收敛、失败重排、重建升级和关闭收敛。

当前编辑器没有可供服务消费的权威未保存文档事件，因此生产集成边界是 filesystem-only。VCE 的 `observe/remove` overlay 入口保留给未来明确的编辑器文档协议，但宿主不会猜测或轮询未保存内容。

### 工具与管理面

`vulcan-agent-runtime` 直接从 `VceToolCatalog` 投影 `codebase_retrieval`。工具参数不能选择项目、路径、namespace、模型或检索参数；项目身份只来自当前执行上下文。Native Tool ID 对 Provider/MCP 同名候选具有保留优先级。

Service API 与 WebUI 提供项目设置、状态、统计、分页文件、Blob/Chunk、operation、完整性和维护计划。修复、重建与暂存清理必须先读取计划，再携带 project、settings revision、checkpoint 和精确 action IDs 执行。

## CBM 退出与数据保留

原生 CBM Controller、managed MCP、工具、WebUI 入口和 runtime 安装/同步脚本已经退出。旧 `codebase_memory_enabled` 只作为一次性迁移输入保留，迁移标记防止覆盖后续 VCE 用户设置。历史 CBM 磁盘数据与已同步运行时不由迁移自动删除。

CBM 未来若以插件形式存在，必须使用独立生命周期和工具身份，不能覆盖 `codebase_retrieval`。

## 验证入口

```powershell
rtk cargo test -p vulcan-context-engine --all-features
rtk cargo test -p vulcan-workspace-store --lib
rtk cargo test -p vulcan-service-kernel --lib
rtk cargo test -p vulcan-agent-runtime --lib
rtk pnpm --dir apps/vulcan-code-web test
rtk pnpm --dir apps/vulcan-code-web run lint
```
