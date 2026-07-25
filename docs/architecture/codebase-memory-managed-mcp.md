# 托管 Codebase Memory MCP

## 定位

Vulcan Code 通过独立 stdio MCP Sidecar 集成 Codebase Memory，不使用 FFI，也不复用用户手动配置的普通 MCP。一个 Sidecar 同时维护全部已启用项目；业务会话仅决定工具调用时使用哪个项目，不决定进程或 watcher 生命周期。Sidecar 生命周期从属于 Vulcan Code 应用：应用级托管 stdio 分区存活期间保持运行，Vulcan Code 退出并关闭最终 stdio 前端后自动停止。

## 项目状态

- `projects.codebase_memory_enabled` 是唯一持久化启用意图，默认值为 `0`。
- 项目停用和移除只把项目从下一次全量同步中移除，不请求删除既有索引。
- 零个项目启用时不保留托管 MCP 分区。
- 会话创建、结束、归档只刷新宿主授权表，不触发项目同步或 Sidecar 启停。
- Vulcan Code 应用退出会关闭托管 MCP 分区；CBM daemon 在最终托管 stdio 前端断开后停止并释放 watcher。

## 私有协议

宿主要求协议 `vulcan.codebase-memory/1`。托管分区发布前，initialize 身份必须满足：

- `serverInfo.name = vulcan-codebase-memory-mcp`
- `serverInfo.version` 与打包清单 `version` 完全一致
- 已协商的标准 MCP `protocolVersion` 非空

私有协议版本由打包清单、专用 initialize 产品身份、控制请求与控制响应共同固定。Sidecar 只以互斥的 `--vulcan-managed` 参数启动，不接受普通 MCP 参数组合；宿主通过 `VULCAN_CBM_CACHE_DIR` 显式传入缓存根目录，同时保持 `inherit_environment = false`。宿主还会确认下列工具全部存在：

- 控制工具：`vulcan_sync_projects`、`vulcan_project_status`、`vulcan_reindex_project`
- AI 只读工具：`get_graph_schema`、`search_graph`、`query_graph`、`index_status`、`check_index_coverage`、`trace_path`、`get_architecture`、`get_code_snippet`、`search_code`、`detect_changes`

控制工具、写入工具和项目管理工具不会进入 AI 工具集合。模型可见 Schema 会删除 `_vulcan`、`project`、`repo_path`；模型若仍提交任一保留字段，调用会被拒绝。

`vulcan_sync_projects` 必须回显 `contract` 与 `generation`，并为请求中的每个项目返回唯一接纳结果；缺项、重复项、额外项目或代次不一致均视为协议错误。项目状态和重建索引响应也必须回显相同的 `contract` 与精确 `project_id`。

三个控制工具都必须在 30 秒内返回确认；同步与重建索引只负责配置 Watcher 或接受后台任务，不得把完整索引过程阻塞在 MCP 调用中。

每次 AI 工具调用前，宿主重新检查根会话仍归属于同一启用项目，并注入：

```json
{
  "_vulcan": {
    "contract": "vulcan.codebase-memory/1",
    "project_id": "稳定项目标识",
    "pwd": "项目实体中的绝对根目录",
    "session_id": "当前真实执行会话标识"
  }
}
```

## 运行时资产

CBM 仓库只构建并发布 `vulcan-codebase-memory-mcp` 专版产物，不再生成公共版二进制、Windows launcher/payload 双文件、UI 包、npm 包、PyPI 包或 MCP Registry 条目。生产编译固定启用 `CBM_VULCAN_PRODUCT=1`，只开放 Vulcan 托管协议入口、内部守护进程/索引 Worker 入口以及 `--help`、`--version`；普通 CLI、Hook 管理和通用 MCP 参数组合均不属于产品面。

发布矩阵固定为五个原生目标：

- `linux-x86_64`
- `linux-aarch64`
- `macos-x86_64`
- `macos-aarch64`
- `windows-x86_64`

每个平台必须在对应原生 runner 上编译，禁止把 Linux 交叉编译环境作为 Windows 产物的前置条件。Windows x64 使用 MSYS2 CLANG64 提供的原生 Windows Clang、Make 与 zlib；macOS Intel/Apple Silicon 和 Linux x64/ARM64 分别使用对应原生 GitHub runner。

运行时只读取：

```text
output/dependencies/runtimes/codebase-memory/manifest.json
```

清单必须固定协议、版本、平台、相对可执行文件名和小写 SHA-256。宿主会在启动前验证平台、路径边界和文件摘要；不会从 `PATH` 选择公共 `codebase-memory-mcp`。Sidecar cache root 位于 Vulcan Code 的用户可写应用数据目录，通过唯一显式环境变量 `VULCAN_CBM_CACHE_DIR` 传入，不写入打包资产目录。

CBM 源码只允许在 CBM 仓库或其发布 CI 中编译。Vulcan Code 的 `make build`、`make br` 与 `make release` 只构建宿主，不检出、不调用也不编译 CBM 源码。

当前联调阶段由开发者直接复制已编译的原生专版产物，或调用下列仅负责身份校验、复制和生成清单的本地打包脚本：

```powershell
pwsh -File scripts/package_codebase_memory_runtime.ps1 `
  -BinaryPath <专用二进制> `
  -Version <固定版本> `
  -Platform windows-x86_64
```

`-Platform` 必须与 Rust 的 `std::env::consts::OS-ARCH` 一致；脚本拒绝非原生平台、非上述五个平台和非专版文件名。Windows x64 当前应使用 `windows-x86_64`。脚本会先执行 `--version` 校验产物身份与版本，再复制二进制，并根据复制后的字节生成清单和 SHA-256。

CBM 正式发布并具备线上原生资产后，版本、平台映射、下载地址与 SHA-256 校验应统一接入 `make deps` 的宿主依赖安装流程。该流程只下载并验证发布资产，不从源码编译 CBM；在正式资产元数据可用前不预设仓库地址、标签或资产文件名。

## 故障语义

- 资产缺失、摘要不匹配、协议工具缺失或同步失败时，项目启用意图保持不变，状态显示为 `degraded`。
- 不会静默回退到用户 MCP、公共二进制或 FFI。
- MCP 传输自动重连后会重新校验身份与工具契约，再按单调 generation 重放当前权威项目集合。
- 再次保存设置或应用重启也会按当前权威项目集合重新协调。
- Windows 双边契约测试由 CBM 仓库的 `scripts/test-vulcan-managed-windows.ps1` 驱动，覆盖多项目同步、隐藏路由参数、后台索引 Worker 与应用级 daemon 退出。
