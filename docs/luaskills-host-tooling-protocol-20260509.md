# LuaSkills 宿主工具协议与结果桥接改造方案（2026-05-09）

## 1. 背景与问题定义

当前 `LuaSkills` 已经具备较好的通用工具承载能力，适合继续作为 `VulcanCode`、MCP 暴露层、以及未来 AI Coding 工具体系的统一能力基建。但现阶段仍存在一个核心短板：

**LuaSkills 的调用结果仍以“文本技能结果”为中心，缺少宿主可直接消费的结构化变化信息。**

这会直接导致以下问题：

1. 编辑类工具即使已经成功修改文件，也只能返回文本摘要，宿主无法稳定得知：
   - 修改了哪些文件
   - 每个文件的状态是新增、修改、删除还是重命名
   - 对应 diff 是什么
   - 当前结果是预览还是已应用
   - 是否存在诊断信息、格式化信息、警告信息
2. `VulcanCode` 无法直接驱动原生 diff 面板，只能从 Markdown 或普通文本中“猜测”变更内容。
3. 一部分未来能力并不适合直接开放给 AI，例如动态 AST Tree、宿主缓存快照、结构树同步工具，但这些能力仍然应当复用 LuaSkills runtime，而不是另起一套宿主私有协议。
4. 目前 “系统 skills / MCP / 宿主私有工具 / 插件能力” 的边界不够清晰，后续会导致实现分裂。

因此，本方案的目标不是只给某一个 `skill` 加字段，而是要把 `LuaSkills` 升级为：

- **统一工具运行时**
- **统一宿主结果协议桥**
- **统一 AI / 宿主双层消费能力源**

---

## 2. 总体设计目标

本次改造需要同时满足以下目标：

1. 保持 LuaSkills 现有“文本工具结果”兼容能力不被破坏。
2. 为宿主增加一套**默认关闭、显式开启**的结构化结果桥接能力。
3. 允许某些 `skill entry` 仅供宿主调用，而不直接暴露给 AI。
4. 让编辑类工具能够稳定返回变更集，供 `VulcanCode` 原生 diff 面板直接消费。
5. 让宿主专用工具（例如动态 AST Tree）也走 LuaSkills runtime，而不是绕过 LuaSkills 另起协议。
6. 让 LuaSkills 成为 AI Coding 场景下的**统一工具能力源**，MCP 和 Plugin 均退化为适配层或装配层。

---

## 3. 总体原则

本方案建议正式采用以下四条原则：

### 3.1 LuaSkills-first

绝大多数 AI Coding 工具能力均应优先由 LuaSkills 实现，包括：

- 文件读取
- 文件列表
- 搜索 / 命中
- Patch / Edit / Preview
- AST / CodeKit
- 记忆能力
- 工作区分析能力
- 本地 HTTP / Curl 类能力

### 3.2 Plugin-as-package-and-switch

Skill 是最小能力单元，Plugin 是打包、装配、启停单元。

也就是说：

- **Skill** 负责能力实现
- **Plugin** 负责一组 Skills 的组织、默认启用状态、依赖声明、宿主扩展与动态开关

但这里必须增加一个重要边界：

**并不是所有系统层能力都应当插件化。**

有一类能力应当作为 **SystemSkills** 单独存在，它们是：

- 宿主稳定提供的能力
- 宿主功能固定依赖的能力
- 不允许按普通技能那样安装 / 卸载 / 启停 / 替换的能力
- 往往也不直接暴露给 AI

例如：

- 动态 AST Tree
- 宿主缓存同步技能
- 宿主结构快照技能
- 仅供宿主内部面板消费的结构树技能

因此这一条应当理解为：

- **常规技能** 可以由 Plugin 负责打包与启停
- **SystemSkills** 不属于普通 Plugin 管理面

### 3.3 MCP-as-adapter

MCP 不再是唯一能力实现层，而是对外暴露层。

同一个 LuaSkills 能力可以：

- 直接给 `VulcanCode` 宿主调用
- 由宿主选择性暴露给 AI
- 通过 MCP 适配成对外工具

能力本体仍然只保留一份。

### 3.4 Host-as-policy-and-native-surface

宿主负责：

- 权限
- 工作区边界
- 能力开关
- 是否对 AI 可见
- 原生 UI 面板消费
- diff / tree / diagnostics / artifact 等原生渲染

LuaSkills 不负责宿主 UI 行为，只负责输出标准化结果。

---

## 4. 对当前 LuaSkills 现状的判断

当前 LuaSkills 运行时这几处是本次改造的主要落点：

- `D:/projects/vulcan-luaskills/src/runtime/result.rs`
- `D:/projects/vulcan-luaskills/src/runtime/engine.rs`
- `D:/projects/vulcan-luaskills/src/host/options.rs`
- `D:/projects/vulcan-luaskills/src/runtime/entry.rs`
- `D:/projects/vulcan-luaskills/src/ffi.rs`
- `D:/projects/vulcan-luaskills/src/ffi_standard.rs`

当前事实如下：

1. `RuntimeInvocationResult` 只有：
   - `content`
   - `overflow_mode`
   - `template_hint`
   - `content_bytes`
   - `content_lines`
2. Lua 返回契约仍为：
   - `content[, overflow_mode[, template_hint]]`
3. `LuaRuntimeCapabilityOptions` 已经存在宿主能力开关先例：
   - `enable_skill_management_bridge`
   - `enable_managed_io_compat`
4. `vulcan.context.client_capabilities` 已经能注入到 Lua runtime。
5. `RuntimeEntryDescriptor` 当前只描述：
   - 名称
   - 技能归属
   - 描述
   - 参数
   尚未显式描述“是否给 AI 暴露”“是否宿主专用”等开放级别。

因此，本方案应顺着现有结构演进，而不是推倒重来。

---

## 5. 方案总览

本方案建议新增三套核心能力：

1. **宿主结果桥接层**
   - 新增 `host_result`
2. **宿主能力开关层**
   - 默认关闭，宿主显式开启
3. **工具开放级别层**
   - 区分 `ai_public` / `host_only` / `hybrid`

三者关系如下：

```text
Lua Skill Entry
    -> 返回 content（通用文本层）
    -> 可选返回 host_result（宿主结构层）

Host Capabilities
    -> 决定是否允许 host_result 生效
    -> 决定允许哪些 host_result.kind
    -> 决定哪些 entry 对 AI 可见

Host / VulcanCode
    -> 消费 host_result
    -> 驱动 diff / tree / diagnostics 等原生 UI
```

---

## 6. 新的统一结果协议

### 6.1 保留现有通用文本层

必须保留现有文本结果能力：

- `content`
- `overflow_mode`
- `template_hint`

原因：

- 兼容现有 skills
- 兼容 CLI
- 兼容通用 MCP 包装
- 兼容其他宿主

### 6.2 新增宿主结构化结果层

建议在 `RuntimeInvocationResult` 中新增：

```rust
pub host_result: Option<serde_json::Value>
```

这层只供宿主消费，不要求一定展示给 AI。

### 6.3 宿主结构化结果的统一信封

建议所有宿主结构化结果统一采用如下顶层信封：

```json
{
  "version": "v1",
  "kind": "change_set",
  "title": "修改了 2 个文件",
  "summary": "已应用补丁并完成格式化",
  "payload": { ... }
}
```

字段说明如下：

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `version` | string | 是 | 当前宿主结果协议版本，首版固定 `v1` |
| `kind` | string | 是 | 结果类型，用于宿主选择消费方式 |
| `title` | string | 否 | 给宿主面板使用的简短标题 |
| `summary` | string | 否 | 给宿主日志或概览使用的简短摘要 |
| `payload` | object | 是 | 具体结构化结果主体 |

### 6.4 Lua 返回约定扩展

建议 Lua 技能返回约定扩展为：

```lua
return content, overflow_mode, template_hint, host_result
```

要求如下：

1. `content` 仍然必须是字符串。
2. `overflow_mode` 与 `template_hint` 保持现状。
3. 第 4 返回值 `host_result` 为可选值，仅在宿主启用结果桥接时生效。
4. 当宿主未启用该能力时，运行时必须忽略 `host_result`，而不是报错。

这样可以做到：

- 老技能零改动
- 新技能逐步接入
- 宿主可控开启

---

## 7. 宿主能力开关设计

### 7.1 Host Options 静态开关

建议在 `LuaRuntimeCapabilityOptions` 中新增：

```rust
pub enable_host_result_bridge: bool
```

默认值：

```rust
enable_host_result_bridge: false
```

作用：

- 总开关
- 宿主未开启时，任何结构化宿主结果都不生效

### 7.2 Request Context 动态能力

建议在 `vulcan.context.client_capabilities` 中新增：

```json
{
  "host_result": {
    "enabled": true,
    "allowed_kinds": ["change_set", "structured_tree"],
    "max_payload_bytes": 524288
  }
}
```

作用：

- 按请求细粒度控制
- 宿主可以限制当前调用允许返回哪些 `kind`
- 宿主可以限制最大结果体积

### 7.3 为什么不做成普通工具参数

本方案明确不建议将结果桥接做成普通公开参数，例如：

- `RETURN_HOST_RESULT=true`
- `ENABLE_DIFF_RESULT=true`

原因：

1. 结果桥接属于宿主能力协商，不是业务输入。
2. 公开参数会污染模型工具 schema。
3. 模型不应负责决定宿主是否需要结构化面板结果。
4. 这与 `LUASKILL_SID` 一类“宿主管理、默认隐藏”的设计哲学不一致。

**结论：**

- 结果桥接能力走宿主开关与隐藏上下文
- 不走公开参数

---

## 8. `host_result.kind` 设计

本方案建议第一版采用联合类型设计，而不是只做一个“diff 字段”。

### 8.1 首批正式支持的 kind

第一批建议正式支持：

1. `change_set`
2. `structured_tree`

### 8.2 预留但不在首批实现的 kind

可预留但暂不实现：

1. `match_list`
2. `artifact`
3. `host_document`
4. `diagnostic_bundle`

---

## 9. `change_set` 协议

### 9.1 使用场景

用于所有“有文件变化性质”的工具，例如：

- `vulcan-file-edit`
- `vulcan-file-write`
- `vulcan-codekit-patch`
- 将来的批量 patch 工具

### 9.2 顶层示例

```json
{
  "version": "v1",
  "kind": "change_set",
  "title": "修改了 2 个文件",
  "summary": "已应用补丁并完成格式化",
  "payload": {
    "mode": "applied",
    "workspace_root": "D:/openvulcan/vulcan-code",
    "files": [
      {
        "path": "src/a.lua",
        "display_path": "src/a.lua",
        "status": "modified",
        "diff_unified": "...",
        "additions": 12,
        "deletions": 4,
        "before_text": "...",
        "after_text": "..."
      },
      {
        "path": "src/b.lua",
        "display_path": "src/b.lua",
        "status": "created",
        "diff_unified": "...",
        "additions": 30,
        "deletions": 0
      }
    ],
    "diagnostics": [],
    "warnings": []
  }
}
```

### 9.3 `payload` 字段定义

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `mode` | string | 是 | `preview` 或 `applied` |
| `workspace_root` | string | 否 | 变化所归属的工作区根 |
| `files` | array | 是 | 变更文件列表 |
| `diagnostics` | array | 否 | 宿主可展示的诊断信息 |
| `warnings` | array | 否 | 非阻断警告 |

### 9.4 `files[]` 字段定义

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `path` | string | 是 | 规范化后的真实路径或相对工作区路径 |
| `display_path` | string | 否 | 给 UI 展示用的简化路径 |
| `status` | string | 是 | `created` / `modified` / `deleted` / `renamed` |
| `diff_unified` | string | 是 | 统一 diff 文本 |
| `additions` | number | 否 | 新增行数 |
| `deletions` | number | 否 | 删除行数 |
| `before_text` | string | 否 | 变更前文本 |
| `after_text` | string | 否 | 变更后文本 |
| `rename_from` | string | 否 | 重命名来源路径 |

### 9.5 设计要求

1. `change_set` 必须可直接驱动宿主 diff 面板。
2. `mode=preview` 时不能视作已修改工作区。
3. `mode=applied` 时应当能为宿主提供最终生效的变化信息。
4. `diff_unified` 是最小必须字段。
5. `before_text` / `after_text` 可按预算、能力或隐私策略选择性返回。

---

## 10. `structured_tree` 协议

### 10.1 使用场景

用于宿主专用结构结果，例如：

- 动态 AST Tree
- 宿主缓存的目录树
- 结构化符号树
- 会话内最新代码结构快照

### 10.2 顶层示例

```json
{
  "version": "v1",
  "kind": "structured_tree",
  "title": "AST Tree 已刷新",
  "summary": "已同步当前目录的最新结构树",
  "payload": {
    "tree_kind": "ast",
    "source_root": "D:/openvulcan/vulcan-code/apps/vulcan-code-gui/src",
    "revision": "workspace-generation-128",
    "nodes": [
      {
        "id": "file:app.rs",
        "kind": "file",
        "label": "app.rs",
        "path": "app.rs",
        "children": [
          {
            "id": "fn:view_selected_session",
            "kind": "function",
            "label": "view_selected_session",
            "range": {
              "start_line": 4600,
              "end_line": 4890
            }
          }
        ]
      }
    ]
  }
}
```

### 10.3 设计要求

1. `structured_tree` 不要求直接对 AI 可见。
2. 宿主可以拿到这棵树后自行决定：
   - 直接缓存
   - 直接渲染
   - 重新压缩成文本后再喂给 AI
3. `revision` 必须能反映宿主侧“这是一份最新树”的代次或版本。
4. 动态 AST Tree 等宿主固定依赖能力建议实现为 `SystemSkill + HostOnly`，而不是普通可管理技能。

---

## 11. 工具开放级别设计

当前 `RuntimeEntryDescriptor` 只描述“有哪些 entry”，但无法表达“对谁可见”。  
建议在运行时入口描述中新增：

```rust
pub exposure: RuntimeEntryExposure
```

其中：

```rust
enum RuntimeEntryExposure {
    AiPublic,
    HostOnly,
    Hybrid,
}
```

### 11.1 含义定义

#### `AiPublic`

直接可暴露给 AI 的工具入口，例如：

- `vulcan-file-read`
- `vulcan-file-list`
- `vulcan-codekit-rg`

#### `HostOnly`

只允许宿主直接调用，不进入 AI 工具列表，例如：

- 动态 AST Tree
- 宿主缓存刷新工具
- 宿主结构重建工具

#### `Hybrid`

宿主可调用，也可按宿主策略选择性暴露给 AI，例如：

- 某些 preview 工具
- 某些安全级别较高的代码改写工具

### 11.2 设计价值

有了 `exposure` 之后，以下需求就有统一承载方式：

1. 不是所有 skills 都必须直接给 AI 用
2. 动态宿主工具可以仍然走 LuaSkills runtime
3. 宿主可以基于产品模式决定是否让某类工具对 AI 可见

---

## 12. 宿主专用工具与 AI 工具的统一模型

本方案建议采用统一模型：

- **同一套 LuaSkills runtime**
- **同一套 entry registry**
- **不同的 exposure**
- **不同的 host_result.kind**

也就是说，未来：

1. `vulcan-file-read`
   - `AiPublic`
   - 普通文本结果为主

2. `vulcan-file-edit`
   - `Hybrid`
   - 文本结果 + `change_set`

3. `dynamic-ast-tree`
   - `SystemSkill + HostOnly`
   - 文本结果可仅作日志摘要
   - 结构主结果走 `structured_tree`

这会极大降低“工具一部分放宿主，一部分放 skill”的混乱度。

---

## 13. SystemSkills 与插件化分层建议

### 13.1 结论

建议明确拆成三层，而不是笼统地把所有系统 skills 插件化：

1. `Core Runtime`
2. `SystemSkills`
3. `Plugins`

其中：

- `Core Runtime` 不插件化
- `SystemSkills` 不属于普通插件技能管理面
- `Plugins` 才是普通可安装、可启停、可替换的技能打包单元

分层建议如下：

#### Core Runtime（不可插件化）

必须留在 LuaSkills / 宿主底层的内容：

- runtime engine
- result protocol
- host capability bridge
- FFI / JSON API
- entry registry

#### SystemSkills（宿主稳定内建技能）

这是一类必须单独存在的宿主层技能面：

- 由宿主稳定提供
- 通常随宿主一并发布
- 不通过普通 skill manager 安装、更新、卸载
- 不允许被用户或 AI 当作普通技能关闭
- 不要求进入普通“可安装技能列表”
- 可以是 `HostOnly`
- 也可以是严格受宿主策略控制的 `Hybrid`

这类能力的典型特征是：

- 宿主功能固定依赖
- 一旦缺失会直接破坏宿主特性
- 结果主要服务宿主原生面板，而不是普通文本输出

典型示例：

- 动态 AST Tree
- 宿主结构树同步工具
- 宿主缓存刷新工具
- 宿主内部工作区结构快照工具

#### Bundled System Plugins（官方可管理插件）

适合做成插件包、默认启用、允许动态启停的能力包：

- `vulcan-file`
- `vulcan-codekit`
- `vulcan-workmem`
- `vulcan-curl`
- 未来某些非硬依赖的结构分析技能

#### User / Project Plugins（用户或项目插件）

按需安装、按需启用的能力包。

### 13.2 目标模型

建议正式采用：

**LuaSkills-first, Plugin-as-package-and-switch, MCP-as-adapter**

即：

- Skill 是能力源
- SystemSkill 是宿主稳定能力源
- Plugin 是常规分发与启停层
- MCP 是外部协议适配层

### 13.3 SystemSkills 的管理原则

建议为 `SystemSkills` 单独定义管理原则：

1. 不进入普通安装/卸载/update 流程。
2. 不进入普通用户可见的系统技能开关面板。
3. 不允许通过常规 skill manager 被移除。
4. 宿主可以决定是否在调试模式下展示它们的只读状态。
5. 若需要版本管理，应由宿主发布流程控制，而不是由用户侧技能生命周期控制。

### 13.4 SystemSkills 与 Exposure 的关系

`SystemSkills` 和 `exposure` 是两个正交维度：

- `SystemSkills` 描述**归属与管理方式**
- `exposure` 描述**是否对 AI 可见**

因此：

- `dynamic-ast-tree` 可以是 `SystemSkill + HostOnly`
- 某些宿主稳定提供的只读分析能力也可以是 `SystemSkill + Hybrid`

但它们都不应落入普通技能安装与启停面。

### 13.5 SystemSkills 的实现建议

为了避免概念上区分了 `SystemSkills`，代码上却仍然落进普通 skill manager，本方案建议同时增加一个“管理平面”概念。

建议运行时与宿主层至少能够区分：

- `system`
- `managed`

其中：

- `system` 表示宿主稳定提供、不可通过普通技能生命周期管理的能力
- `managed` 表示普通可安装、可更新、可卸载、可启停的技能能力

建议落点如下：

1. **Skill / Entry 描述层增加管理平面标记**
   - 例如：`management_plane: system | managed`
2. **SystemSkills 从宿主单独注入的根或清单加载**
   - 不复用普通用户技能安装入口
3. **普通 skill manager 明确拒绝对 `system` 平面执行 install / update / uninstall / disable**
4. **宿主可只读展示 `SystemSkills` 状态**
   - 但不能把它们当作普通技能开关处理
5. **`SystemSkills` 仍可使用 `host_result` 与 `exposure`**
   - 只是管理平面不同

这样才能保证像动态 AST Tree 这样的宿主固定依赖能力，既复用 LuaSkills runtime，又不会落入普通技能治理路径。

---

## 14. FFI 与兼容性策略

### 14.1 Rust 内部 API

可以直接扩展 `RuntimeInvocationResult`：

```rust
pub host_result: Option<serde_json::Value>
```

### 14.2 JSON FFI

JSON FFI 推荐直接增加 `host_result` 字段。  
这层最容易兼容，也最适合新宿主先落地。

### 14.3 标准 C ABI

**不建议直接破坏现有 ABI 结构。**

推荐做法：

1. 保留旧版 `FfiRuntimeInvocationResult`
2. 新增：
   - `FfiRuntimeInvocationResultV2`
   - 或新增 `host_result_json` 的 V2 调用入口

推荐字段：

- `host_result_json`
- `host_result_kind`

这样可以：

- 保持旧宿主稳定
- 新宿主按 V2 接入
- 避免静默 ABI 破坏

---

## 15. 对 Lua 技能作者的返回约束

为避免后续技能生态混乱，建议统一规定：

1. `content` 始终必须返回字符串。
2. `host_result` 只能是：
   - `nil`
   - 可 JSON 化的 table
3. `host_result.kind` 必须使用注册类型。
4. 宿主未开启结果桥接时，技能不应报错。
5. 宿主未允许某个 `kind` 时，运行时应忽略并记录调试日志，而不是抛出终止性错误。

---

## 16. 第一批推荐落地对象

### 第一阶段：协议打底

目标：

- 加 `host_result`
- 加能力开关
- 加 `exposure`

改动重点：

- `runtime/result.rs`
- `runtime/engine.rs`
- `runtime/entry.rs`
- `host/options.rs`
- `ffi.rs`
- `ffi_standard.rs`

### 第二阶段：先打通一个真实变更工具

推荐首个工具：

- `vulcan-file-edit`

要求：

- 支持 `preview`
- 支持 `applied`
- 返回 `change_set`

### 第三阶段：宿主专用结构工具

推荐首个工具：

- `dynamic-ast-tree`

要求：

- `SystemSkill + HostOnly`
- 返回 `structured_tree`

### 第四阶段：系统插件化

将官方系统 skills 逐步按插件单元组织和启停。

---

## 17. `vulcan-file-edit` 的第一版约束建议

第一版不建议一口气做得过大，建议约束如下：

1. 只处理文本文件
2. 只返回统一 diff
3. 只支持：
   - `preview`
   - `apply`
4. 最小宿主结果就是 `change_set`
5. `content` 仍然保留一段给 AI/日志看的简短摘要，例如：
   - `已预览 2 个文件的修改`
   - `已应用 1 个文件的修改`

---

## 18. `dynamic-ast-tree` 的产品定位建议

建议明确它不是普通 AI 工具，而是：

- `SystemSkill + HostOnly`
- 宿主获取最新结构树的内部工具

它的使用流程建议是：

1. 宿主调用 LuaSkills entry
2. LuaSkills 返回：
   - 简短 `content`
   - `host_result.kind = structured_tree`
3. 宿主缓存最新树
4. 宿主决定是否将这棵树压缩成 AI 可读摘要

这样能彻底避免“AI 拿到的是过时树”“为了拿最新树不得不走普通 skill 文本输出”的问题。

---

## 19. 验收标准

本次协议改造完成后，至少应满足以下验收标准：

1. 老技能无改动情况下仍然可运行。
2. 宿主未开启结果桥接时，新技能不会报错。
3. `vulcan-file-edit` 可返回 `change_set`。
4. `VulcanCode` 可直接消费 `change_set` 驱动 diff 面板。
5. `dynamic-ast-tree` 可作为 `SystemSkill + HostOnly` entry 正常注册和调用。
6. 宿主可根据 `exposure` 决定 AI 工具可见性。
7. JSON FFI 支持 `host_result`。
8. 标准 C ABI 保持兼容，不破坏旧宿主。

---

## 20. 推荐实施顺序

建议按以下顺序实施，不建议并行大改：

1. `RuntimeInvocationResult` 扩展
2. Host capability 扩展
3. `RuntimeEntryDescriptor.exposure` 扩展
4. JSON FFI 打通
5. `vulcan-file-edit` 打通 `change_set`
6. `VulcanCode` 原生消费 `change_set`
7. `dynamic-ast-tree` 打通 `structured_tree`
8. 系统 skills 插件化与启停策略

---

## 21. 最终结论

本方案建议正式将 LuaSkills 提升为：

**AI Coding 统一工具基建层**

并明确采用以下整体架构：

- **LuaSkills-first**
- **Plugin-as-package-and-switch**
- **MCP-as-adapter**
- **Host-as-policy-and-native-surface**

同时通过以下三项关键协议改造完成闭环：

1. `host_result`：解决宿主结构化结果问题
2. `exposure`：解决 AI / 宿主工具开放级别问题
3. `enable_host_result_bridge`：解决默认隐藏、宿主显式开启的问题

只要这三项定稳，LuaSkills 就不再只是“文本技能系统”，而会成为：

- AI 可调用工具层
- 宿主内部工具层
- MCP 对外暴露能力源
- 官方系统插件能力源

这也是后续统一 `VulcanCode` 工具体系的最优方向。
