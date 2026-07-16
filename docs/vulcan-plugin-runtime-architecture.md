# Vulcan 插件首发架构与清单 v1

## 1. 文档定位

Vulcan 插件是宿主持有的扩展包。一个插件可以只贡献 Skill、Agent、提示词或静态 UI 内容，也可以声明 Lua 工具、Hook、受管执行资源和 MCP Server。

本文件只定义首次公开发布的 `schema_version: 1`。宿主不读取任何开发阶段临时清单结构，也不根据包内字段推断插件来源。

## 2. 核心不变量

1. `plugin.yaml` 是包内能力声明的唯一事实来源。
2. 插件来源由安装位置决定：随应用发布的目录属于内建来源，用户安装目录属于用户来源。
3. 清单使用严格字段校验；未知字段、重复标识、悬空引用和越界路径均直接拒绝。
4. 内容型插件不需要 Lua Runtime 或执行环境。
5. Lua、MCP 和其他可执行组件共享 `execution` 中的受管资源，但彼此是独立协议。
6. Shell、系统 `PATH` 和全局 npm 环境不属于插件执行边界。
7. Secret 只存在于操作系统凭据库，清单、普通配置、日志和 Service DTO 都不得包含 Secret 值。
8. 插件目录与插件 MCP 分区通过同一个 ExtensionGeneration 发布；失败候选不得进入新执行。

## 3. 包结构

最小内容型插件只需要清单及其贡献文件：

```text
example-plugin/
├─ plugin.yaml
├─ skills/
├─ agents/
├─ ui/
└─ locales/
```

可执行插件可以增加声明过的运行资源：

```text
example-plugin/
├─ plugin.yaml
├─ runtime/
│  └─ plugin.bundle.lua
├─ node/
├─ bin/
├─ package.json
└─ pnpm-lock.yaml
```

所有清单路径都必须是插件包内的规范化相对路径。符号链接、重解析点、绝对路径和 `..` 越界都被拒绝。

## 4. 顶层清单

```yaml
schema_version: 1
id: example-plugin
version: 1.0.0
default_enabled: false

display:
  name: Example Plugin
  capability: 提供示例扩展能力

platforms:
  excluded_targets: []

permissions: []
required_files: []

execution:
  dependencies:
    install_mode: bundled
  native_binaries: []
  node_entries: []
  python_entries: []

mcp:
  servers: []

ui:
  mounts: []

prompt_contributions: []
locales: {}
```

`lua_runtime` 和 `config` 是可选对象。`execution`、`mcp`、`ui`、`prompt_contributions` 与 `locales` 可以省略并使用空值。

插件配置 Schema 必须是包内自包含文档；`$ref`、`$dynamicRef` 与 `$recursiveRef` 只允许本地 fragment，禁止通过外部 URI 读取网络或宿主文件。

清单没有来源类别字段。插件不能自行声明为内建插件，也不能改变宿主确定的安装来源。

## 5. 通用执行资源

`execution.dependencies` 描述插件私有依赖环境：

- `manifest`：包内依赖清单路径。
- `install_mode: bundled`：只使用随包制品。
- `install_mode: registry`：只有用户明确批准注册表网络权限后才允许安装。

`execution.native_binaries` 以逻辑 ID 映射精确 Rust target triple 对应的包内二进制。

`execution.node_entries` 与 `execution.python_entries` 以逻辑 ID 声明包内脚本和工作目录。是否可执行仍由权限、宿主受管运行时和调用协议共同决定。

依赖安装使用插件私有目录、冻结锁文件和禁用 install scripts 的安装方式，不写入系统全局依赖环境。

## 6. Lua Runtime

需要 Lua 协议时使用顶层 `lua_runtime`：

```yaml
lua_runtime:
  bundle: runtime/plugin.bundle.lua
  lua_modules: []
  invoke_function: invoke
  shutdown_function: shutdown
  max_active_sessions: 8
  lifecycle:
    install: on_install
    enable: on_enable
    disable: on_disable
    uninstall: on_uninstall
  actions: []
  tools: []
  hooks: []
```

生命周期、Action、Tool 与 Hook 函数必须真实存在于同一个已声明 bundle。宿主自有字段不得出现在模型可见参数 Schema 中。插件更新候选复用 `install` 完成候选初始化；失败时使用 `uninstall` 执行对应补偿。

## 7. MCP Server

每个 MCP Server 都由插件内 `id`、显示名称和唯一 transport 构成。宿主生成带插件来源命名空间的稳定 Server ID，插件不能覆盖用户 MCP 或其他插件的 Server。

### 7.1 本地 stdio

```yaml
mcp:
  servers:
    - id: local-server
      name: Local Server
      transport:
        type: stdio
        launcher:
          type: node_entry
          entry_id: server
        args: []
        env: {}
```

允许的 launcher 只有：

- `node_entry`：引用 `execution.node_entries`。
- `node_package_bin`：引用插件私有依赖中的精确 package 与 bin。
- `native_binary`：引用 `execution.native_binaries`。

宿主直接解析并启动入口，不调用 Shell，也不搜索系统 `PATH`。

插件 stdio 子进程会清除未声明的宿主环境，只注入受管启动器所需值及清单通过明确 `literal`、`config`、`secret` 或 `environment` 来源解析出的变量。用户自行配置的 MCP 仍保留其独立环境继承策略。

### 7.2 远程 HTTP 与 SSE

```yaml
mcp:
  servers:
    - id: remote-server
      name: Remote Server
      transport:
        type: http
        url:
          source: literal
          value: https://mcp.example.com
        headers:
          Authorization:
            source: secret
            key: access-token
```

远程 endpoint 必须是无内嵌凭据、无 fragment 的 HTTPS URL。Header 名必须通过标准 HTTP Header 校验。SSE 表示 MCP 定义的旧式 SSE 传输，本身仍属于首发 v1 的正式 transport。

### 7.3 值来源

MCP 参数、环境变量、URL、Header 与 OAuth 字段只能选择一个明确来源：

- `literal`：清单中的非敏感字面值。
- `config`：插件配置契约中的精确键。
- `secret`：插件与 Server 命名空间下的 Secret Store 键。
- `environment`：显式命名的宿主环境变量，同时要求 `secrets_read` 权限。

宿主不会在多个候选字段间猜测值来源。

## 8. 安装、更新与卸载事务

### 8.1 安装

1. 在包外检查清单、路径、引用、权限与内容指纹。
2. 用户查看绑定到精确内容指纹的权限预览；预览包含全部 MCP Server 的 ID、transport 与 Secret Key 名称，并批准完整权限集合和注册表网络许可。
3. 收集本地 MCP 安装验证所需的精确 Secret 集合。
4. 复制到私有 staging，重新计算指纹并写入授权与安装记录。
5. 准备私有依赖环境。
6. 只启动本地 stdio MCP 候选，完成 initialize、`tools/list`、Schema 校验和确认关闭。
7. 执行安装生命周期，准备不可变目录与 MCP 访问面。
8. 原子发布 ExtensionGeneration；任一步失败都恢复包、状态覆盖与 Secret。

远程 MCP 在安装时只做静态验证，不发起网络连接；首次启用时必须通过真实就绪门槛。

### 8.2 更新

更新只允许针对已停用且完全排空的用户插件：

1. 旧包被保留在唯一私有事务目录。
2. 新包作为候选进入固定用户插件位置并写入候选记录。
3. 依赖、本地 MCP、生命周期与 Hook 声明全部验证。
4. 发布前再次计算完整候选指纹，防止生命周期或外部步骤改写包内容。
5. 发布成功后删除旧包、旧环境和不再声明的 Secret。
6. 发布前失败恢复旧包和原始记录；发布后清理失败作为已提交状态的独立诊断上报。

### 8.3 停用与卸载

停用先拒绝新租约，再排空旧 generation，撤销所属 MCP 分区并运行停用回调。卸载只允许已停用、活动会话为零且旧 MCP Lease 已排空的用户插件。包提交删除后再清理其 Secret 命名空间。

## 9. ExtensionGeneration 与运行时隔离

ExtensionGeneration 包含单调 revision、不可变插件目录和同代 MCP Runtime 访问面。一次 Agent 执行只捕获一次 generation：

- 新执行只看到新目录、新 Lua Tool/Skill/Agent/UI 投影和新 MCP 分区。
- 已开始执行继续持有旧 generation 和 Lease。
- 插件分区替换不重启用户 MCP 或其他插件分区。
- 旧 generation 排空后显式关闭 transport；本地进程必须确认退出。

## 10. MCP 状态与动态工具

Server 状态包含连接、就绪、退避、失败、停止等阶段。启动、initialize、列表、远程连接、关闭和 generation 排空均使用宿主固定上限。

`tools/list_changed` 触发完整列表重新读取、完整 Schema 校验和一次性快照替换。MCP 工具输入 Schema 固定按 JSON Schema Draft 2020-12 元 Schema 与编译器双重校验；`$ref`、`$dynamicRef` 与 `$recursiveRef` 只允许本地 fragment。任一工具非法时保留上一份完整成功快照，不发布部分更新。

连接失败使用带抖动的指数退避；连续失败达到上限后进入 Failed，用户重试、重新启用或新 generation 才能恢复。单个 Server 失败只隔离自身。

## 11. 日志与 Secret 安全

本地 MCP stderr 最多保留最近 200 行或 64 KiB。日志按完整行缓冲后再脱敏，能处理跨 pipe read 的 Secret；超长未终止行整行丢弃。替换值按长度降序处理，避免重叠 Secret 的后缀泄漏。

Service DTO 只暴露 endpoint 摘要、状态、generation、重试次数、能力数量与脱敏错误。WebUI 只能从所属插件详情查看状态、日志、重试和授权入口，不能在用户 MCP 设置中单独编辑插件 MCP。

## 12. 验证门槛

发布前必须通过：

1. 清单与路径单元测试。
2. 安装、更新、启停、卸载及失败补偿测试。
3. MCP 本地/远程传输、Schema、重连、动态列表和关闭测试。
4. Secret 白名单、日志跨分片脱敏和 HTTPS 边界测试。
5. Rust fmt、workspace check、workspace test 与严格 Clippy。
6. Web lint、类型检查和生产构建。
