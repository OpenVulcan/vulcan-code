# 原生 Codebase Memory 集成退出说明

## 当前状态

VulcanCode 不再内置或启动 Codebase Memory Sidecar，也不再安装、同步或打包 CBM 原生运行时。本文件只记录迁移与数据保留边界，不再是有效的托管 MCP 运行时契约。

代码语义主链由进程内的 Vulcan Context Engine（VCE）承担。AI 侧只暴露一个原生 `codebase_retrieval` 工具；索引生命周期、维护和数据查询继续由宿主服务与项目管理页面持有，不通过 MCP 传输。

## 已退出的原生能力

- CBM Controller、reconcile、recovery 和进程监督。
- managed CBM MCP 配置、上下文注入和工具投影。
- 项目设置中的 CBM 控制入口与 Workbench 状态投影。
- `make deps`、`make init-system` 和 `make sync` 中的 CBM runtime 安装与同步。
- 仓库内用于下载、校验、打包和同步 CBM sidecar 的脚本。

## 迁移与数据保留

- 历史项目表中的 `codebase_memory_enabled` 字段暂时保留，只作为一次性迁移输入读取。
- `project_vce_settings.migrated_from_codebase_memory` 记录迁移是否已经执行，重复加载不会再次覆盖用户后续修改。
- 旧开关为 `true` 时首次生成的 VCE 设置保持启用意图；旧开关为 `false` 时保持禁用。
- VulcanCode 不删除旧 CBM 缓存、数据库或已同步二进制。版本回退或用户手工清理仍可处理这些历史文件。
- 新项目不再创建或依赖 CBM 数据。

## 插件化边界

CBM 若以后作为插件提供，必须使用独立插件标识、独立工具名和独立生命周期，不能冒充或覆盖原生 `codebase_retrieval`，也不能恢复宿主内置的 managed MCP 主链。插件安装与卸载不得影响 VCE、VMM 或知识库的数据命名空间。

## 相关文档

- [`VCE 功能入口`](../../crates/vulcan-context-engine/docs/vce-functional-entry.md)
- [`VCE 工具面`](../../crates/vulcan-context-engine/docs/vce-tool-surface.md)
- [`开发环境初始化`](../development-environment.md)
