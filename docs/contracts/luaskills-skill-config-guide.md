# LuaSkills 技能包配置接入指南

## 1. 适用范围

本文说明 Vulcan Code 对 LuaSkills 0.5.9 技能包配置契约的正式支持方式，面向技能包作者、宿主维护者和桌面端使用者。

本实现只支持 LuaSkills 0.5.9 正式契约，不执行旧配置探测、迁移、别名兼容、旧文件提示或自动清理。

### 1.1 正式依赖与构建入口

根目录 `Cargo.toml` 精确锁定 `luaskills = "=0.5.9"`；`Cargo.lock` 必须从 crates.io 注册表解析该版本及其校验和。应用构建只使用普通的 `make build`、`make br` 和对应 release 入口，不再通过 Make 目标或脚本覆盖本地 LuaSkills 检出。

本次锁文件中的依赖身份为：

```text
name = "luaskills"
version = "0.5.9"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "3f04f7301f2fd516b14f5a6a90849294b51dbd09156d0fccba5cd74ad31633b9"
```

`MAX_RUNTIME_SESSION_LEASES_PER_MANAGER` 等所需接口由正式 0.5.9 公开。宿主的会话入场容量直接引用上游常量，不再维护兼容镜像。本指南后文保留的本地上游路径与测试取证仅解释历史行为和待改进边界，不是构建来源。

本文中描述上游契约的章节（见 §3）同样以该 crate 的对应版本为准，本仓库不复制其字段校验。

## 2. 宿主持久化位置

Vulcan Code 为宿主中的 LuaSkills 引擎传入同一个固定配置根：

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

插件执行引擎独立于普通技能管理引擎；共享配置路径不表示共享引擎或执行锁。当前接入及迁移边界见[插件执行引擎归属](plugin-execution-engine.md)。

## 3. 技能包作者声明配置

> **契约归属。** 本节字段契约由上游 `luaskills` crate 拥有，版本与校验和由根 `Cargo.toml`
> （`luaskills = "=0.5.9"`）与 `Cargo.lock` 锁定；本仓库不实现、不校验本节字段，上游源码也不随
> 本仓库分发。因此本节无法在本仓库内验证：升级 `luaskills` 版本时必须回读上游对应版本的源码
> 并同步本节，不能以本节作为行为依据。

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

正式类型只有 `integer`、`string`、`float`、`enum` 和 `boolean`。技能包应直接遵守 0.5.9 的字段、类型、约束和安全整数规则，不应依赖宿主修正错误声明。

## 4. 技能包内读取配置

Lua 代码只能访问当前技能包自己的配置命名空间。使用 LuaSkills 0.5.9 提供的正式接口：

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

LuaSkills VM 池的宿主默认值由 `vulcan-luaskills-config::LuaSkillsRuntimeSettings::default()`
定义：主 VM 池最小 4、最大 8，多余空闲 VM 在 60 秒后可回收；隔离 runLua 池最小 1、最大 4，
多余空闲 VM 在 30 秒后可回收。缺少 `luaskills-runtime-setting.json` 时使用这些内存默认值，
不会自动创建用户配置文件。新安装配置模板 `config/luaskills-runtime-setting.json` 与编译默认值
通过测试保持一致；默认值变更时必须同步模板和本段说明。已有配置仍按用户保存值加载，
资源同步仅在目标文件缺失时播种，不覆盖已有配置；已有文件缺少必填池字段仍显式报错。

服务端先读取不含值的声明，只为 `sensitive=false` 且当前合法的配置项读取和规范化有效值。写入结果同样会删除敏感项的规范值，再跨越 RPC 边界。

`sensitive=true` 是宿主披露策略，不代表 LuaSkills 配置文件提供静态加密。配置仍位于当前用户的数据目录中，部署方必须依靠操作系统账户、文件权限和磁盘安全策略保护该目录。秘密不得写入默认值、示例、日志、错误信息或技能提示词。

`managedRuntime.invokeDefaultTimeoutMs` 的系统默认值为 `60000`。当技能通过受管运行时调用
`node.invoke` 等入口且没有显式传入 `timeout_ms` 时，调用最多等待 60 秒；技能显式声明的正数
`timeout_ms` 仍然优先。旧版配置中的 `null` 会在读取时规范化为 `60000`，不会继续保留无限等待。
该配置只约束受管 Node/Python 调用，不会强制中断普通 Lua handler、同步 FFI 或
`vulcan.process.exec`；这些边界必须由对应技能或 LuaSkills 运行时自身提供明确上限。

LuaSkills 宿主会在调用等待 5 秒、30 秒以及之后每 60 秒输出一次阶段诊断。`engine_read_lock_wait`
表示仍在等待宿主引擎读锁，`engine_write_lock_wait` 表示仍在等待宿主引擎写锁，`skill_body_or_host_call`
表示已经进入 Lua 技能或其宿主调用；诊断中的 `blocking_runtime_operation` 表示 RunLua 生命周期
等通用阻塞操作已经开始。诊断只包含规范工具名、进程内调用关联 ID、阶段与耗时，不包含完整参数。

宿主引擎使用读写锁：普通技能调用、一次性 runLua、持久 runLua 会话与插件 System lease 只取读锁，
因此宿主锁不再阻断独立调用并发进入 LuaSkills。普通技能与一次性 runLua 分别使用主池和 runLua 池；
插件 System lease 使用每租约专用 VM，不占用这两个池。池的 `maxSize` 限制 VM 租约数，但不单独决定实际执行并发：
LuaSkills 0.5.9 的文件型 runLua 仍在执行期间持有进程工作目录锁；System lease 使用租约逻辑目录，
不修改进程工作目录、不取得该锁。不同插件可并行，同一插件的同用途运行时仍由宿主执行门串行。
宿主引擎写锁仍会排空全部读取；插件依赖安装等管理动作不在本次并发改造范围内。
技能安装、更新、
卸载、启停、重载与技能包配置写入取写锁，并在排空在途读锁之后才变更引擎状态。单次锁等待超过
1 秒时另输出 `engine-read-lock-slow` 或 `engine-write-lock-slow`；写锁慢诊断只包含管理动作标签
（如 `refresh`、`install`），不包含技能标识与来源定位值。`runtime-config` 虽只暴露只读的
`describe` 与 `validate` 动作，但其上游入口声明在 `&mut LuaEngine` 上，因此仍取写锁。

## 9. 外部修改、显式刷新与事件

LuaSkills 0.5.9 文件监听器会观察两个固定存储。WebUI 打开配置弹窗时轮询有序配置事件：

- 发现当前技能包发生本地写入或外部重载后，重新加载描述与校验状态；
- 保留用户尚未提交的草稿；
- 展示外部变更提示；
- 重载失败时只展示不包含秘密值的结构化错误。

服务内部还提供严格的 `service_refresh_luaskill_config` 宿主管理 RPC。请求只允许可选的 `storeScope`，其值由 LuaSkills 正式契约校验；省略时刷新两个已配置存储。该入口不暴露给 AI，也不作为 WebUI 的任意文件读取通道。

事件游标过期时，服务先按上游稳定错误恢复到最老保留事件之前的边界，重新构建当前 LuaSkills 配置摘要，再继续处理保留事件。不会把旧事件内容猜测成当前状态。

## 10. 生命周期

- 技能更新后，LuaSkills 重新加载声明并按新声明校验既有值；
- `update_luaskill` 直接投影上游更新事务已提交的运行时目录，不再追加第二次 `reload_from_roots`。
  无版本变化时同样直接读取当前目录；显式刷新仍保留其重载语义。
- 同步 FFI 调用返回前必须结束并等待其原生工作线程退出。宿主写锁排空在途调用不等于
  排空 DLL 内的全局后台线程；VM 销毁后 `ffi.load` 可能释放动态库，残留线程不得继续执行库代码。
  CodeKit 0.2.3 的 Tokei 统计使用每次调用独立的 Rayon 作用域线程池，退出前等待线程及其线程局部
  析构完成；不得改回进程级全局线程池或仅丢弃线程池句柄而不等待退出。
  该修复由独立 CodeKit 技能发布提供，重编译宿主不会自动替换已安装的旧版 FFI 依赖。
- 启用、停用和普通重载不修改配置文件；
- 卸载技能不会删除对应配置；
- 不再声明的持久化键显示为 orphaned，只有用户执行明确删除操作时才清理；
- ROOT 与 USER 同名技能分别保留在 `system-skills` 与 `skills` 存储中；
- 配置不完整只影响状态与技能自身行为，不会让宿主自动隐藏或注销技能工具。

## 11. 排障顺序

遇到配置问题时按以下顺序检查：

1. 根 `Cargo.toml` 与 `Cargo.lock` 是否均锁定 `luaskills 0.5.9`，且来源为注册表；
2. `skill.yaml` 的 `config` 是否位于顶层且符合正式 schema；
3. 生效技能来自 ROOT 还是 USER，以确认对应存储作用域；
4. `runtime-config validate` 或 WebUI 校验结果中的稳定错误码；
5. 是否发生 revision 冲突；
6. 配置文件外部修改是否产生 reload error 事件。

不要通过增加旧字段回退、候选路径或多格式解析来绕过声明错误。

## 12. 执行目录与租约语义

### 12.1 权威归属与同步触发

本节对应正式 `luaskills 0.5.9`，上游权威为 `src/runtime/engine/logical_cwd.rs`、`lease.rs`、`runlua.rs`、`managed_io.rs`、`process_session.rs` 及 `contracts/system-lease-execution.md`。升级时必须重新核对这些入口；本文不定义另一套执行协议。

### 12.2 System 逻辑工作目录

- 宿主仍传已授权的绝对目录；省略目录仍表示包根，不能用删除字段替代路径授权。
- System 租约在 VM 内保存逻辑目录。受管文件接口、标准 IO 兼容、`loadfile/dofile`、Lua 模块搜索、动态库路径、命令解析与子进程工作目录按该目录解析。
- System 执行不修改进程目录，也不取得普通文件型 runLua 的全局目录锁；普通 runLua 保留原行为。
- System 拒绝直接 `os.execute`、`os.exit`、`os.setlocale`、`os.tmpname`，原始 FFI 不向插件开放。子进程使用已解析的绝对程序与明确子进程目录。
- 同一 Lua VM 仍然独占；此更改不提供同一 VM 并发，也不把可信原生模块变成安全沙箱。
- Windows 根相对与盘符相对路径拒绝；中文、空格路径经宿主文件接口读取，原生 C 函数自身的文件名编码限制仍属于对应函数契约。

### 12.3 原生依赖与插件桥接审计

`plugin-bridge/vulcan-plugin-bridge/runtime/bridge.lua` 由宿主以绝对路径读取并通过源码注入；包根、授权目录与进程规格均来自宿主校验。

Context7 的 `lcurl.safe` 只接收 URL、请求头、超时、TLS 校验选项与内存回调，未传入相对证书或文件路径；`openssl.cipher` 操作内存 AES-GCM，`openssl.rand` 返回随机字节。当前声明列表无修改进程目录的模块。后续新增原生模块或文件参数时必须重新审计，不能假设 Lua 包装能够截获 C 内部文件访问。

### 12.4 验证与限制

上游明确阻塞的宿主回调尚未放行时，另一 System 租约已完成；同时持有普通 runLua 目录锁并真实改变进程目录时，System 文件、模块与命令仍使用自己的目录。恢复旧全局锁后这两项回归均失败。

正式 0.5.9 对应上游提交 `70bb42d`，Windows、Ubuntu、macOS 原生 CI：<https://github.com/LuaSkills/luaskills/actions/runs/35960551210>。同租约串行和宿主取消归属由 VulcanCode 另行测试，不能以这份上游证据替代真实页面验收。

## 13. 安装来源策略与可达路径

本节记录上游来源策略与 Vulcan 消费入口的对应关系，以及由此产生的可达性结论。

### 13.1 上游支持的四种来源类型

`SkillInstallSourceType` 共四个变体，但**并非都可用**（`skill/manager.rs:794-810`、`:858-874`）：

| 变体 | 安装 | 更新 | 说明 |
| --- | --- | --- | --- |
| `Github` | 可用 | 可用 | 正常可用 |
| `OfficialHub` | 可用 | 可用 | 正常可用 |
| `Url` | **无条件 Err** | **无条件 Err** | `public URL skill install is disabled by source policy` |
| `PrivateUrlManifest` | 可用 | 可用 | 见 13.3 的前置条件 |

`Url` 的 `Err` 分支**不带任何条件判断**，因此该来源类型在上游已经死亡。

### 13.2 Vulcan 侧曾存在的三处死来源入口（均已修复）

| 入口 | 位置 | 修复后行为 |
| --- | --- | --- |
| 安装/更新 RPC | `parse_skill_install_source_type` | 拒绝 `url`，消息点名 `github` |
| WebUI 安装对话框 | `lua-skills-panel.tsx` 来源类型下拉 | 移除 `URL` 选项，仅保留 GitHub |
| ROOT 引导技能配置 | `RootSkillBootstrapSourceType::into_runtime_source_type` | 返回 `Result`，拒绝并带上技能标识 |

第三处刻意**保留该变体可反序列化**：直接删除变体会让既有配置在解析期变成一条不可读的 serde `unknown variant` 错误，修复原因就无从报告。

`official_hub` 与 `private_url_manifest` 是**上游支持但宿主未实现**的来源类型，必须保持「未知类型」错误，**不得静默映射到 `github`**。

### 13.3 `PrivateUrlManifest` 的三项前置条件

`skill/manager.rs:1262-1298` 要求：

1. `plane == SkillOperationPlane::System` —— **仅限宿主系统权威面**；
2. `config.enable_private_url_skill_install == true`；
3. **manifest URL 与 manifest 内声明的 archive URL** 都必须通过 `private_skill_source_allowlist` 校验。

Vulcan 的**配置侧已齐备**：`enable_private_url_skill_install` 与 `private_skill_source_allowlist` 都已从运行时设置注入 `host_options`（`luaskills_bridge.rs`、`luaskills_runtime.rs`）。

### 13.4 本地 HTTP 验收与生产来源边界

Vulcan 生产入口仍只接受 `github` 来源，并继续拒绝用户 `url` 来源；本地验收通过**测试专用的 GitHub API 与站点基址**把同一生产 `install_luaskill` / `update_luaskill` → Service Runtime → `LuaEngine` 路径指向回环 HTTP 服务。该做法不新增生产来源类型，也不放宽 `PrivateUrlManifest` 的 System 权威面约束。

`production_install_and_update_use_local_github_routes_safely` 已从最终磁盘和请求计数验证：

- 冷安装、升级与新版本目录发布；
- 已验证缓存的热重装，以及关闭网络后的零请求缓存命中；
- 校验和不匹配、截断响应、非法 ZIP、校验和正确但 Lua 内容无效时，旧版本和已验证缓存保持可用；
- 上游真实文件系统测试 `dependency_install_rejects_preexisting_target_symlink` 继续覆盖预存目标符号链接拒绝；ZIP crate 不保留可构造的符号链接文件类型位，因此不使用无效的伪 ZIP 符号链接夹具冒充验收。

上游下载客户端现在同时设置 10 秒建连超时和 5 分钟整次请求超时；`blocking_http_client_bounds_stalled_requests` 复用生产客户端构建器，以缩短的确定性测试时限证明服务接受连接后不返回响应也会有界失败。

### 13.5 可达验收结论

本地回环 HTTP 的安装、更新、缓存、离线与失败回滚验收针对当时的本地源码 HEAD 完成。该历史结论不表示生产 WebUI/RPC 已支持任意 URL；当前构建来源与版本以第 1 节的正式 0.5.9 注册表依赖为准。

## 14. 可延后项的结论与后续入口

方案「本地验证矩阵」第 6 条要求可延后项**分别记录结论和后续入口**，并规定「不得在报告中称这些能力已经修复」。本节即该记录；每项给出归属、取证、结论与后续入口。

### 14.1 安装分阶段接口（方案四.1）

- **取证：** `install_luaskill` 在入口取 `engine.write()`；`LuaEngine::install_skill_in_root` 是同步方法，内部 `apply_skill_request_in_root` 依次完成下载、校验、解压与提交，期间无 `await`、无线程派生。写锁因此**跨越整个网络 I/O**。
- **分阶段接口不可行：** `PreparedSkill*` 事务字段为 `pub(crate)`，宿主无法驱动「先暂存、后取锁应用」的事务。
- **结论：** 按方案四.1 的保留分支处理——**保留写锁**，并把安装期排队**作为明确限制显示**（安装进度横幅与更新进度弹窗在 `inFlight` 期间披露其他技能操作需要等待）。**该能力未修复，只是被如实呈现。**
- **更新进度交互：** 点击更新后立即打开弹窗，首个服务事件到达前显示准备状态；随后按 `correlationId` 排除点击前保留的旧操作，展示当前阶段、`percent` 与下载字节数。当前 LuaSkills 0.5.9 仅在下载字节总量已知时提供下载百分比，解析、解压等无数值阶段显示不确定进度；下载达到 100% 不代表整次更新完成。服务操作与本地请求结束后保留完成或失败结果，供用户关闭，关闭后的同一终态不会再次弹出。
- **后续入口：** 需上游先公开不可变暂存区与代次检查接口，宿主才可能分阶段。

### 14.2 插件租约逻辑目录

已由正式 LuaSkills 0.5.9 实现并接入，具体契约与证据见 §12。System 租约统一采用逻辑目录，不保留全局锁回退。普通文件型 runLua 的独立限制见下一项。

### 14.3 文件型 runLua 工作目录虚拟化（方案四.3）

- **取证：** `runlua.rs` 依赖进程级 cwd（`set_current_dir(entry_dir)`），且 `engine.rs:966` 的 `fs.copy` 与 `:1657` 的 `process.exec` 按进程 cwd 解析相对路径。
- **结论：** 未实施。逐 VM 工作目录解析只能在其与原行为在文件读取、模块加载、子进程 cwd、失败、取消和跨工作区隔离上**全部等价**后才可考虑；当前**保留有界串行**。
- **后续入口：** 需先提供改前行为样本集作为等价性判据。

### 14.4 插件 lease 源码与编译缓存（方案四.6）

- **结论：** 未测量、未实施。磁盘读取与 `load` 编译耗时**尚未分开测量**，因此没有依据判断重复读取是否占主要成本。
- **后续入口：** 先测量再取舍。实施时须以可信包身份与内容代次缓存不可变源码，在安装/更新/停用及代次变化时失效，且**不得跨 VM 复用绑定了沙箱环境的 Lua 闭包**；编译缓存还须上游证明同版本 VM、权限与沙箱隔离等价。

### 14.5 Managed IO 外部覆盖策略（方案五.2）

- **归属：** Managed IO 的发布规则由**上游**实现。Vulcan 只通过 `capabilities.enable_managed_io_compat`（默认 `true`）传递开关，见 `luaskills_bridge.rs` 与 `luaskills_runtime.rs`。
- **结论：** Vulcan 侧**不存在**声称自动合并外部修改的产品路径，因此无需修正的冲突声明。**该行为未被验证，也未做任何改动。**
- **后续入口：** 若上游确认采用完整权威缓冲覆盖，凡需要保留外部修改的产品路径必须**显式声明冲突或约束**。

### 14.6 受管运行时缓存失效扩展测试（方案五.3）

- **归属：** 计划缓存位于上游 `runtime/managed_package.rs:285-669`（`managed_runtime_plan_cache`、`managed_runtime_plan_cache_hits`、`cached_/cache_/evict_managed_runtime_plan`），**全部 `pub(crate)`**，宿主不可触及。
- **结论：** 「相同 owner 命中、锁文件或清单改变后失效、不同 owner 隔离」是**上游内部行为**，只能在上游内验收。Vulcan 侧已验收的部分是受管运行时**安装目标与清单校验**（既有测试覆盖严格目标、清单不匹配、版本意外与平台矩阵）。
- **后续入口：** 上游为该缓存补充命中/失效/隔离测试；Vulcan 侧不重复实现。

### 14.7 共同边界

除 §14.2 已完成外，其余项目仍按各自条目所列边界处理；不能用插件并发修复推导普通 runLua、离线缓存或 Managed IO 已全部改善。

## 15. Agent 请求期线程入场审计（方案四.4）

方案四.4 要求「对 Agent 每次请求新建 OS 线程的入口增加按调用类别区分的有界入场」。本节记录入口审计、类别差异与设计边界。

### 15.1 真实入场类别与容量来源

本任务约束的是 `vulcan-agent-runtime::luaskills_runtime` 在异步调用包装中为 LuaSkills 阻塞操作创建的 OS 工作线程。此前把 `agent_runtime/conversation_hooks` 的 VMM 会话钩子列为本节四个入口，是一次范围识别错误；它们不是 LuaSkills VM 池的借用入口，本次没有修改其调度语义。

LuaSkills 相关工作线程按上游实际资源拆成三类：

| 类别 | 宿主入场容量 | 上游对应资源 |
| --- | --- | --- |
| 普通技能调用 | `runtime_settings.vm_pool.max_size` | 主 VM 池 |
| 一次性文件型/内联 runLua | `runtime_settings.run_lua_pool.max_size` | 隔离 runLua VM 池 |
| 公开持久 runLua 会话操作 | `LUASKILLS_RUNTIME_SESSION_LEASE_CAPACITY`，当前为 8 | 公共 Runtime Session manager 的租约上限 |

公开持久会话没有复用一次性 runLua 默认上限 4。LuaSkills 0.5.9 对 manager 的持久会话租约上限公开 `MAX_RUNTIME_SESSION_LEASES_PER_MANAGER`；Vulcan 的会话入场器直接引用该常量，保持与上游容量一致。宿主只对公开会话入口使用该容量；System lease 与插件运行时仍由独立 manager 管理，不与公开会话的入场器混用。

### 15.2 入场与取消语义

所有三类调用都在 `std::thread::Builder::spawn` 之前异步获取其独立 `tokio::sync::Semaphore` 许可。等待许可期间只有异步任务排队，不创建 OS 线程；若调用方在排队时取消，许可 future 随任务销毁。

取得许可后，许可所有权被移入工作线程，并在该线程真正退出时释放。调用方在工作已经开始后取消，只会丢弃结果等待者，不会提前释放许可，也不会允许额外线程突破上限。等待 5 秒、30 秒及之后每分钟输出一次带调用 ID、操作与阶段的诊断；这不把排队伪装成死锁。

### 15.3 验收结果

- `host_admission_bounds_workers_and_preserves_worker_owned_permits` 确定性验证两路入场、排队取消与工作线程持有许可；
- `real_skill_runlua_and_system_calls_overlap_inside_host_callbacks` 使用真实普通技能、一次性 runLua 与 System lease 路径验证并行边界，并在 8/16/64 路压力下检查线程峰值有界及完成后回落；
- 公开会话容量测试验证 8 条租约可创建，第 9 条由上游返回 `lease_limit_exceeded`；
- 数据库 4 worker / 64 槽与 `try_send` 饱和语义保持不变，因为等待 VM 的调用尚未进入数据库回调，不能把池等待机械换算成数据库负载。

### 15.4 当前限制

带 `cwd` 的 System lease 和文件型 runLua 仍通过进程级 `lock_runlua_cwd_guard` 串行修改进程工作目录；本次入场只约束线程数量，不宣称消除了这条 cwd 串行。插件 bundle 的受限环境仍未暴露 `vulcan.models`，内部模型接线也没有扩大插件沙箱权限。
