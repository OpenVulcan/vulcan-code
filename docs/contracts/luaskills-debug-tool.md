# luaskills-debug 安装与 `luaskills_debug` 工具

luaskills-debug 是独立发布的 LuaSkills 开发工具（仓库 `LuaSkills/luaskills-debug`），内嵌与宿主相同的
LuaSkills 运行时，提供 `new`、`validate`、`package`、`call` 等命令。Vulcan Code 不随包携带它，而是由
用户在「调试 Lua 技能」页面或由 AI 通过 `luaskills_debug` 工具按需安装；内置提示词技能 `luaskills-dev`
指导 AI 用它开发用户自己的 LuaSkills。

## 一、唯一安装器

- 安装器只有一处：`crates/vulcan-service-runtime/src/luaskills_debug_tool.rs`。设置页、TUI 与模型工具都经
  Core 的同一安装任务执行，不得另写下载逻辑，也不得让 AI 用命令自行下载。
- 系列（`major.minor`）派生自内嵌 `luaskills` 依赖版本 `VULCAN_LUASKILLS_DEPENDENCY_VERSION`。安装最新
  release；最新版本不在该系列时安装 `v{series}.0`。版本不匹配直接报错，不降级、不替换。
- 下载走 LuaSkills 引擎的网络设置（GitHub 镜像、令牌、是否允许下载）；禁止下载时明确报错。
- 每个资产先按 `luaskills-debug-checksums.txt` 与 GitHub 摘要双重校验，再解压到同级临时目录、校验清单
  `debug-tool-manifest.json` 与版本，最后原子提交到
  `<用户 Vulcan Code 根>/tools/luaskills-debug/<version>`。安装目录以排他文件锁防止并发安装。
- 旧版本不自动删除；状态选择匹配系列中的最高版本。`runtime/resources/luaskills-packages-manifest.json`
  存在即视为调试运行时已设置（由包内 `setup_runtime` 写出）。

## 二、Core 任务与 RPC

| RPC | 结果 |
| --- | --- |
| `service_luaskills_debug_status` | 状态 DTO，含最近一次安装任务 `task` |
| `service_start_luaskills_debug_install` | 启动安装；已有运行中的安装时返回该任务 |
| `service_luaskills_debug_install_status { taskId }` | 按稳定标识查询任务 |

任务是进程内唯一槽位，状态为 `running`、`succeeded`、`failed`，阶段键为 `queued`、`resolving`、
`downloading`、`verifying`、`extracting`、`completed`。界面按阶段键渲染目录文案；错误只保留英文诊断。

## 三、模型工具

- `luaskills_debug` 只有 `status` 与 `install` 两个动作，参数严格解析，拒绝未知字段。
- 仅在 LuaSkills 运行时存在时注册；计划模式只允许 `status`。
- 权限分类：`status` 为 `Ungoverned`，`install`（及任何异常参数）为 `Opaque`，按扩展效果走正常权限管线。
- 宿主拒绝派生代理执行 `install`；安装结果返回与 `status` 相同的 snake_case 状态 JSON。
- 工具说明要求先调用 `skill("luaskills-dev")`；luaskills-debug 本身由 AI 经 `shell_command` 运行，沿用其
  正常权限检查。
