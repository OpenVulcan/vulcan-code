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

宿主要求协议 `vulcan.codebase-memory/2`。托管分区发布前，initialize 身份必须满足：

- `serverInfo.name = vulcan-codebase-memory-mcp`
- `serverInfo.version` 与打包清单 `version` 完全一致
- 已协商的标准 MCP `protocolVersion` 非空

私有协议版本由打包清单、专用 initialize 产品身份、控制请求与控制响应共同固定。Sidecar 只以互斥的 `--vulcan-managed` 参数启动，不接受普通 MCP 参数组合；宿主通过 `VULCAN_CBM_CACHE_DIR` 显式传入缓存根目录，同时保持 `inherit_environment = false`。宿主还会确认下列工具全部存在：

- 控制工具：`vulcan_sync_projects`、`vulcan_project_status`、`vulcan_update_project_index`
- AI 只读工具：`get_graph_schema`、`search_graph`、`query_graph`、`index_status`、`check_index_coverage`、`trace_path`、`get_architecture`、`get_code_snippet`、`search_code`、`detect_changes`

控制工具、写入工具和项目管理工具不会进入 AI 工具集合。模型可见 Schema 会删除 `_vulcan`、`project`、`repo_path`；模型若仍提交任一保留字段，调用会被拒绝。

`vulcan_sync_projects` 必须回显 `contract` 与 `generation`，并为请求中的每个项目返回唯一接纳结果；缺项、重复项、额外项目或代次不一致均视为协议错误。项目状态和索引更新响应也必须回显相同的 `contract` 与精确 `project_id`。

三个控制工具都必须在 30 秒内返回确认；同步与索引更新只负责配置 Watcher 或接受后台任务，不得把完整索引过程阻塞在 MCP 调用中。`vulcan_update_project_index` 的 `mode=update` 表示安全增量更新，`mode=rebuild` 表示隔离式全量重建；同模式并发请求合并，不同模式冲突必须显式返回。

## 索引可用性与进度

- 首次索引未发布前，项目状态为初始化中，读取工具不可用，但必须提供当前作业阶段、已处理数量和最近诊断。
- 增量更新与隔离式重建期间，已发布的旧索引继续服务读取请求；只有新索引完整构建并验证成功后才原子切换。
- `vulcan_project_status` 返回已发布可用性、Watcher 状态、索引修订号、图节点/边数量、活动作业、最近终态作业及有界诊断。
- 活动作业必须报告稳定作业标识、请求模式、阶段、启动时间、已处理与总量；无法确定总量时，界面使用不确定进度，不伪造百分比。
- 最近终态作业必须区分 `completed`、`failed` 与 `cancelled`，并保留适合用户恢复操作的错误摘要。
- Vulcan Code 在设置页打开期间主动轮询状态，并在后台维持低频监视；用户无需重新保存设置或手动刷新才能看到终态。
- “更新索引”默认采用 `mode=update`；“完全重建”属于破坏性维护入口，必须二次确认，并明确说明旧索引在切换前仍可用。

每次 AI 工具调用前，宿主重新检查根会话仍归属于同一启用项目，并注入：

```json
{
  "_vulcan": {
    "contract": "vulcan.codebase-memory/2",
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

Vulcan Code 固定使用正式发布的 `v0.1.1` 原生资产。依赖安装命令为：

```powershell
./make.ps1 deps cbm
```

默认 `./make.ps1 deps` 和 `./make.ps1 deps all` 也包含 CBM。安装器只下载当前原生平台资产，先校验仓库固定的 `checksums.txt` SHA-256，再从该清单解析并校验平台压缩包摘要；解压后继续校验专版文件名、`--version` 输出，并根据最终二进制字节生成宿主 manifest。验证后的依赖保存在 `third_party/codebase_memory_runtime`，不直接修改运行中的 output。

`make build`、`make br` 与 `make release` 只把已经安装的依赖同步到 `output/dependencies/runtimes/codebase-memory`，不会访问网络或编译 CBM。同步器把可执行文件与 manifest 作为一致性文件对事务化切换；若运行中的 CBM 锁定目标 EXE，则整组保留旧的已验证版本，绝不单独更新 manifest。版本升级只修改安装器中固定的 Release 标签与 `checksums.txt` 摘要。

`scripts/package_codebase_memory_runtime.ps1` 保留为开发者对本地原生专版产物执行身份校验、复制和 manifest 生成的底层工具，不是正式依赖下载入口。

## 故障语义

- 资产缺失、摘要不匹配、协议工具缺失或同步失败时，项目启用意图保持不变，状态显示为 `degraded`。
- 不会静默回退到用户 MCP、公共二进制或 FFI。
- MCP 传输自动重连后会重新校验身份与工具契约，再按单调 generation 重放当前权威项目集合。
- 再次保存设置或应用重启也会按当前权威项目集合重新协调。
- Windows 双边契约测试由 CBM 仓库的 `scripts/test-vulcan-managed-windows.ps1` 驱动，覆盖多项目同步、隐藏路由参数、后台索引 Worker 与应用级 daemon 退出。
