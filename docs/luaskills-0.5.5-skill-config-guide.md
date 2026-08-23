# LuaSkills 0.5.5 技能包配置接入指南

## 1. 适用范围

本文说明 Vulcan Code 对 LuaSkills 0.5.5 技能包配置契约的正式支持方式，面向技能包作者、宿主维护者和桌面端使用者。

本实现只支持 LuaSkills 0.5.5 正式契约，不执行旧配置探测、迁移、别名兼容、旧文件提示或自动清理。

## 2. 宿主持久化位置

Vulcan Code 为两个 LuaSkills 引擎传入同一个固定配置根：

```text
<application_data_root>/luaskills-config
```

生产环境默认展开为：

```text
<user-vulcan-root>/code/luaskills-config/
├── skills/
│   └── config.json
└── system-skills/
    └── config.json
```

其中 `<user-vulcan-root>` 会随构建配置变化：release 为 `~/.vulcan`，debug 为 `~/.vulcan-debug`。

普通 USER 技能使用 `skills/config.json`，ROOT 系统技能使用 `system-skills/config.json`。配置根与技能安装目录、构建产物和运行时资源目录相互独立。

## 3. 技能包作者声明配置

配置声明必须位于 `skill.yaml` 顶层 `config`，不能挂在单个 entry 下。以下示例展示常用字段：

```yaml
id: example-configured-skill
name: Example Configured Skill
version: 1.0.0
enabled: true

config:
  - key: api_token
    type: string
    required: true
    sensitive: true
    title: API Token
    description: Token used to call the remote service.
    format: password

  - key: retry_count
    type: integer
    required: false
    default: 3
    title: Retry Count
    description: Maximum request retry count.
    constraints:
      minimum: 0
      maximum: 10

  - key: output_mode
    type: enum
    required: true
    default: compact
    title: Output Mode
    description: Select the output detail level.
    options:
      - value: compact
        label: Compact
        description: Return concise output.
      - value: detailed
        label: Detailed
        description: Return detailed output.

  - key: debug_enabled
    type: boolean
    required: false
    default: false
    title: Debug
    description: Enable package debug behavior.
    advanced: true
    restart_required: true
```

正式类型只有 `integer`、`string`、`float`、`enum` 和 `boolean`。技能包应直接遵守 0.5.5 的字段、类型、约束和安全整数规则，不应依赖宿主修正错误声明。

## 4. 技能包内读取配置

Lua 代码只能访问当前技能包自己的配置命名空间。使用 LuaSkills 0.5.5 提供的正式接口：

```lua
local token = vulcan.config.get("api_token")
local has_mode = vulcan.config.has("output_mode")
local entries = vulcan.config.list()
local descriptor = vulcan.config.describe()
local status = vulcan.config.status()
```

包内如需修改自身配置，可使用 `vulcan.config.set` 和 `vulcan.config.delete`。对跨字段业务约束，应在技能包自己的 `config_validator` 中实现，使 WebUI、宿主校验和实际调用共享同一结果。

## 5. WebUI 管理行为

LuaSkills 技能列表仅对声明了配置的技能显示配置入口，并显示“配置完整”或“配置不完整”状态。

配置弹窗按声明动态渲染：

- `boolean` 使用开关；
- `enum` 使用声明选项；
- `integer` 和 `float` 使用数值输入，并应用上下界；
- `string` 使用单行、多行、密码或路径提示对应的输入形式；
- 按 `group`、`order` 排序；
- `advanced: true` 放入折叠的高级配置区域；
- `deprecated`、`restart_required` 和校验错误均明确展示。

敏感项只显示“已配置”状态。服务端不会把敏感有效值返回给 WebUI；用户新输入的敏感值只存在于当前弹窗草稿，提交成功后立即清空。

所有写入和删除都携带打开弹窗时观察到的 `revision`。如果发生 `CONFIG_REVISION_CONFLICT`，界面会重新读取最新修订并保留未提交草稿，由用户确认后再次保存，不会自动覆盖其他运行时的修改。

## 6. AI 可见能力

宿主提供只读原生工具 `runtime-config`，只接受：

- `describe`：查看生效技能包的配置结构；
- `validate`：查看指定技能包的完整性与合法性。

宿主固定使用 `mode=effective` 和 `include_values=false`。AI 不能通过该工具执行 `get`、`list`、`set`、`delete`、`refresh`，也不能读取敏感或非敏感的当前配置值。

## 7. 配置缺失处理

技能包声明必填项但没有显式值或默认值时：

1. LuaSkills 把对应项标记为 `missing`；
2. 技能包配置状态为不完整；
3. WebUI 显示缺失项与校验消息；
4. 用户在配置弹窗补齐后，宿主以 CAS 批量提交；
5. 提交成功后重新校验并刷新技能目录状态。

宿主不为缺失值生成候选值，不从环境变量、旧文件或其他字段猜测配置。

技能包需要引导 AI 处理缺失配置时，应使用不包含值的英文提示，例如：

```text
This skill package configuration is incomplete. Call runtime-config with
action=describe and skill_id=example-configured-skill, explain the missing
declarations to the user, and ask the user to configure them in the LuaSkills
WebUI. Do not request or relay sensitive values.
```

## 8. 安全边界

服务端先读取不含值的声明，只为 `sensitive=false` 且当前合法的配置项读取和规范化有效值。写入结果同样会删除敏感项的规范值，再跨越 RPC 边界。

`sensitive=true` 是宿主披露策略，不代表 LuaSkills 配置文件提供静态加密。配置仍位于当前用户的数据目录中，部署方必须依靠操作系统账户、文件权限和磁盘安全策略保护该目录。秘密不得写入默认值、示例、日志、错误信息或技能提示词。

`managedRuntime.invokeDefaultTimeoutMs` 的系统默认值为 `60000`。当技能通过受管运行时调用
`node.invoke` 等入口且没有显式传入 `timeout_ms` 时，调用最多等待 60 秒；技能显式声明的正数
`timeout_ms` 仍然优先。旧版配置中的 `null` 会在读取时规范化为 `60000`，不会继续保留无限等待。
该配置只约束受管 Node/Python 调用，不会强制中断普通 Lua handler、同步 FFI 或
`vulcan.process.exec`；这些边界必须由对应技能或 LuaSkills 运行时自身提供明确上限。

LuaSkills 宿主会在调用等待 5 秒、30 秒以及之后每 60 秒输出一次阶段诊断。`engine_lock_wait`
表示仍在等待进程级引擎锁，`skill_body_or_host_call` 表示已经进入 Lua 技能或其宿主调用；诊断
中的 `blocking_runtime_operation` 表示 RunLua 生命周期等通用阻塞操作已经开始。诊断只包含规范
工具名、进程内调用关联 ID、阶段与耗时，不包含完整参数。

## 9. 外部修改、显式刷新与事件

LuaSkills 0.5.5 文件监听器会观察两个固定存储。WebUI 打开配置弹窗时轮询有序配置事件：

- 发现当前技能包发生本地写入或外部重载后，重新加载描述与校验状态；
- 保留用户尚未提交的草稿；
- 展示外部变更提示；
- 重载失败时只展示不包含秘密值的结构化错误。

服务内部还提供严格的 `service_refresh_luaskill_config` 宿主管理 RPC。请求只允许可选的 `storeScope`，其值由 LuaSkills 正式契约校验；省略时刷新两个已配置存储。该入口不暴露给 AI，也不作为 WebUI 的任意文件读取通道。

事件游标过期时，服务先按上游稳定错误恢复到最老保留事件之前的边界，重新构建当前 LuaSkills 配置摘要，再继续处理保留事件。不会把旧事件内容猜测成当前状态。

## 10. 生命周期

- 技能更新后，LuaSkills 重新加载声明并按新声明校验既有值；
- 启用、停用和普通重载不修改配置文件；
- 卸载技能不会删除对应配置；
- 不再声明的持久化键显示为 orphaned，只有用户执行明确删除操作时才清理；
- ROOT 与 USER 同名技能分别保留在 `system-skills` 与 `skills` 存储中；
- 配置不完整只影响状态与技能自身行为，不会让宿主自动隐藏或注销技能工具。

## 11. 排障顺序

遇到配置问题时按以下顺序检查：

1. 根 `Cargo.toml` 与 `Cargo.lock` 是否均锁定 `luaskills 0.5.5`；
2. `skill.yaml` 的 `config` 是否位于顶层且符合正式 schema；
3. 生效技能来自 ROOT 还是 USER，以确认对应存储作用域；
4. `runtime-config validate` 或 WebUI 校验结果中的稳定错误码；
5. 是否发生 revision 冲突；
6. 配置文件外部修改是否产生 reload error 事件。

不要通过增加旧字段回退、候选路径或多格式解析来绕过声明错误。
