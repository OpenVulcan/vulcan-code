# VulcanCode 预接入边界

## 当前目的

本 crate 是从独立仓库 `D:\openvulcan\vulcan-context-engine` 一次性白名单迁入的 VCE 领域内核。来源提交为 `654951cadc59f6902b4d3ac37a432de90e4a905f`，许可证为 MIT。迁入后以 VulcanCode 工作区版本为后续唯一维护来源，不建立子模块、subtree 或运行时跨仓依赖。

当前阶段只让 VCE 成为可编译、可测试、可由上层直接调用的 workspace member，不产生运行时消费者。此边界使后续 VMM 原生集成可以继续独立演进。

## 已包含

- `VceEngine` 的刷新、检索、状态快照、管理查询和工作区覆盖层入口。
- `VceAiPort`、`VceSourcePort`、`VceIndexPort` 三个宿主注入端口。
- 结构分析、递归回退、稳定 Blob/Chunk 身份、分批向量化、事务化索引发布、多通道检索、RRF、重排和覆盖选择。
- OCE 工具语义到 VulcanCode 的静态映射；只建议向模型暴露 `codebase_retrieval`，其余生命周期操作均为 HostOnly。
- 可选且默认关闭的 `host-ai-contract` feature，用于复用 VulcanCode 共同 `HostAiPort` 契约。
- 黑盒公开 API、共同 AI 契约投影和零消费者依赖图测试。

## 明确未接入

- 不修改 service bootstrap、服务状态、NativeToolCatalog、Agent prompt 或工具执行链。
- 不订阅 `vulcan-file-watcher`，不自动扫描或刷新项目。
- 不创建或打开 LanceDB、SQLite、Milvus、模型客户端或 rerank 客户端。
- 不依赖 VMM、VMM-Core、CBM、CodeKit、`vulcan-agent-runtime` 或 `vulcan-service-kernel`。
- 不增加 WebUI 页面、服务 API、项目设置或数据维护命令。

## 后续真实集成入口

后续集成任务应由 VulcanCode 上层分别实现并注册三个端口：

1. 以共同 `HostAiPort` 能力注册构建 `VceAiPort`，原生进程内 consumer 固定使用 `InternalBackground`（序列化为 `internal_background`）usage scope，并固定 consumer profile、向量空间和各用途路由；`external_service` 不得用于 VCE。
2. 以代码语义专属 namespace 实现 `VceIndexPort`，其 SQLite/LanceDB 生命周期完全归宿主持有，不与 VMM 或知识库表和向量空间混用。
3. 以项目快照和现有文件监听事件实现 `VceSourcePort`，初始扫描调用 `RefreshRequest::full`，稳定文件变化批次调用 `RefreshRequest::paths`。
4. 由宿主把 `VceToolCatalog` 中唯一 AI 可见映射转换为 NativeToolCatalog 定义；HostOnly 映射只供服务、管理页和监听生命周期调用。
5. 接入后更新或替换 `workspace_isolation.rs` 的“零消费者”断言，再增加端到端刷新、存储恢复、项目切换和工具结果投影测试。

VCE 仍只解决代码语义逻辑。存储实例、AI 能力注册、文件事件、工具注册、权限与页面管理始终由 VulcanCode 上层负责。

宿主 AI 配置变化后，VCE 适配器必须刷新 `HostAiCapabilityHandle` 和 capability snapshot，不能继续持有首次注册产生的陈旧 handle。VCE 自身不构造 caller context 或宿主 handle；其领域引擎会在每次刷新、检索和管理投影时读取当前 `VceAiPort::capability_snapshot()`。
