# 插件界面编写契约

本文件说明插件如何提供界面：设置页一律用声明式版面由宿主渲染；版面可以带标题区、标签页与只读数据视图（指标、周期图、排名表），因此“设置 + 路由选择 + 统计”这类完整面板也能整页声明。声明式能力确实无法覆盖的其他入口可以自行编写 ESM 模块，但应由宿主页面套件组成页面，并遵守自定义样式限制。

## 契约归属与同步触发点

本文件转录以下源码的当前契约，不是第二权威；字段形状以 Rust 类型与生成 Schema 的共同校验为准。

| 内容 | 权威来源 |
| --- | --- |
| 挂载类型、入口与导出规则 | `crates/vulcan-plugin-runtime/src/manifest.rs`（`VulcanPluginUiMountManifest`）、`catalog.rs` |
| 设置版面字段与语义校验 | `crates/vulcan-plugin-runtime/src/manifest/settings_layout.rs`、`settings_layout/data_view.rs`、生成的 [plugin-settings-layout.schema.json](plugin-settings-layout.schema.json) |
| 设置版面渲染 | `apps/vulcan-code-web/src/components/plugins/plugin-settings-page.tsx` 及同目录 `plugin-settings-*`、`plugin-data-*`、`plugin-bar-chart.tsx` |
| 模块 SDK | `apps/vulcan-code-web/src/components/plugins/dynamic-plugin-mount-panel.tsx`（`PluginUiSdk`） |
| 模块可用组件 | `apps/vulcan-code-web/src/components/plugins/plugin-ui-sdk-components.ts`（`PLUGIN_UI_SDK_COMPONENTS`） |
| 页面套件 | `apps/vulcan-code-web/src/components/page/`、`src/ui-kit/styles/page-kit.css` |
| 自定义样式词汇与限制 | `src/ui-kit/styles/features/plugin-host-components.css`、`src/ui-kit/styles/plugin-tokens.css`、`scripts/check-ui-architecture.mjs`（`checkPluginUiStyle`） |

以上任一来源变化（新增控件、条目类型、SDK 字段、组件或样式规则）时，须在同一任务中回读源码并同步本文件。

## 选择挂载类型

`plugin.yaml` 的 `ui.mounts[]` 每项声明一个入口：

| `kind` | 入口 | `export` | 渲染方 | 用途 |
| --- | --- | --- | --- | --- |
| `settings` | 包内 `.json` 设置版面 | 禁止 | 宿主页面套件 | 插件配置、凭据、连接测试等设置 |
| `module` | 包内未压缩 `.mjs` | 必填，命名导出的工厂 | 插件模块 | 统计、浏览器、工作流等非设置页面 |

规则：

- **设置页必须用 `kind: settings`。** 不要在模块里手写设置表单、保存按钮或凭据输入；版面由宿主统一提供读取、校验、草稿、保存与放弃、只写凭据及错误展示。
- **优先整页声明。** 设置、选择清单与统计能用版面表达时，一个插件只声明一个 `kind: settings` 挂载，用 `tabs` 分页；例如 RTK（设置 / 路由 / 统计三个标签）与 Context7（认证、请求行为、用量）。不要因为想要统计或分页就把同一面板拆成设置挂载 + 模块挂载。
- 只有声明式能力确实无法表达的页面（例如带价格编辑的用量统计、浏览器）才使用 `kind: module`；同一插件可以同时声明两类挂载。
- `point` 为 `skills_plugin_detail`、`workspace_main` 或 `conversation_right_panel`；`label` 或 `label_message_key` 命名导航项，`icon` 只能用宿主图标枚举。
- 入口与其引用的资源都要列入 `required_files`。

## 设置版面（`kind: settings`）

版面文件是 `schema_version: 1` 的 JSON，键名使用 snake_case。宿主为每个区块渲染一个 `PageSection`，每个条目渲染为一行；当前可见内容有字段时页面底部出现粘性保存栏（只含数据区块的标签不显示保存栏）。

```json
{
  "schema_version": 1,
  "config": { "read_action": "settings-get", "write_action": "settings-update" },
  "sections": [
    {
      "id": "behavior",
      "title_message_key": "behaviorTitle",
      "description_message_key": "behaviorHint",
      "items": [
        { "type": "field", "key": "enabled", "control": "switch", "label_message_key": "enabledLabel" },
        {
          "type": "field", "key": "timeoutMs", "control": "select", "value_format": "duration_ms",
          "label_message_key": "timeoutLabel",
          "options": [{ "value": 10000 }, { "value": 30000 }]
        }
      ],
      "links": [{ "url": "https://example.com/docs", "label_message_key": "docsLink" }]
    }
  ]
}
```

### 顶层

| 字段 | 说明 |
| --- | --- |
| `schema_version` | 必须为 `1`。 |
| `config` | 有任何 `field` 时必填，没有字段时禁止。`read_action` 须为 `effect: read`，`write_action` 须为 `effect: mutation`，两者都要声明 `plugin_ui` 调用方。宿主以读取结果为草稿，保存时提交完整配置对象。 |
| `sections` | 至少一个区块。 |
| `header` | 可选页面标题区，显示在标签行尾部（无标签时在页面顶部工具栏）。见[标题区](#标题区-header)。 |
| `tabs` | 可选标签页；声明时至少两个，每个区块恰好属于一个标签。见[标签页](#标签页-tabs)。 |

### 区块

`id`（标识符，版面内唯一）、`title_message_key`、可选 `description_message_key`、可选 `links`（必须是绝对 `https://` 地址，由宿主经 `openExternalUrl` 打开，显示在区块底部）。区块二选一：

- **条目区块**：`items` 至少一项（字段、凭据、操作）。可选 `layout`：`rows`（默认，每条一行）或 `inline`（并排紧凑复选框，说明放入帮助提示，窄屏自动单列）；`inline` 只能包含无 `visible_when` 的 `switch` 字段。
- **数据区块**：`data` 绑定一个读取 Action，`blocks` 至少一项，不得声明 `items`。见[数据区块](#数据区块-data)。

### 标题区 `header`

| 字段 | 说明 |
| --- | --- |
| `status` | `{ ready_message_key, unavailable_message_key }`：按配置读取结果显示可用 / 不可用状态标记；要求版面有 `config`。 |
| `links[]` | `{ url, label_message_key, aria_label_message_key?, icon }`，`url` 必须为 `https://`，`icon` 为 `link` 或 `github`。显示为紧凑元数据标签。 |
| `documents[]` | `{ id, asset, label_message_key, title_message_key, description_message_key?, aria_label_message_key?, icon }`：点击后在宿主对话框中显示包内文本文件，`asset` 必须列入 `required_files`，`icon` 为 `document` 或 `license`；`id` 不得重复。 |

### 标签页 `tabs`

`tabs[]` 每项 `{ id, title_message_key, sections }`，`sections` 按显示顺序列出区块 `id`。至少两个标签、`id` 不重复、每个区块恰好出现一次且必须属于某个标签。标签首次打开时才挂载其内容，因此数据区块的读取只在其标签首次显示时发生。

### 数据区块 `data`

`data` 描述读取来源，`blocks` 描述如何展示同一次读取结果：

| `data` 字段 | 说明 |
| --- | --- |
| `action` | 必须为 `effect: read` 且声明 `plugin_ui` 调用方。可在清单中配合 `result_cache_ms` 使用宿主缓存。 |
| `arguments[]` | `{ name, label_message_key, options, value_format?, default }`：区块标题栏中的固定选项选择器；`name` 为输入属性名，至少两个选项、值不重复，`default` 必须是选项之一，每个选项组合都必须满足输入 Schema。 |
| `refreshable` | 显示“刷新”按钮；刷新绕过宿主缓存（`refresh: true`）。 |
| `show_loaded_at` | 显示“上次加载”时间。 |
| `available_path` + `unavailable_message_key` | 必须同时声明；路径为布尔值，非 `true` 时显示不可用空状态。 |
| `error_path` | 仅可与 `available_path` 同用；字符串原因显示在不可用空状态中。 |
| `refresh_topics[]` | 插件事件主题（点分标识符，不重复）；收到事件即重新读取。 |

路径（`*_path`、`path`）均为点分标识符，按 Action 输出 Schema 逐级解析（支持本地 `$ref`）；属性不在 Schema 中或类型与格式不符时校验失败。Schema 未声明子属性的位置不做约束。

`blocks[]` 按 `type` 区分：

| `type` | 字段 | 说明 |
| --- | --- | --- |
| `metrics` | `items[]`、可选 `title_message_key`、`help_message_key`（需要标题）、`source_path` + `empty_message_key`（须同时声明） | 每项 `{ label_message_key, path 或 ratio, format }`；`ratio: { numerator, denominator }` 必须使用 `ratio` 格式，分母按至少 1 计算。`source_path` 指向的对象缺失或为 null 时显示 `empty_message_key`。 |
| `chart` | `periods[]`、`value_label_message_key`、`value_path`、`value_format`（默认 `number`）、可选 `detail: { path, format }`、`limit`（默认 14，1–60）、`empty_message_key`、可选标题 | 每个周期 `{ id, label_message_key, path, label_paths（1–2 个）, label_format }`，`path` 指向行数组；多个周期时显示本地切换按钮，切换不发请求，只取最近 `limit` 行。值格式限 `number`、`compact`、`percent`、`duration_ms`，标签格式限 `text`、`date`、`month`。 |
| `table` | `path`（行数组）、`columns[]`、`empty_message_key`、可选 `title_message_key`、`count_message_key`（需要标题，文案必须含 `{{count}}`）、`rank` | 每列 `{ path, label_message_key, format }`；`rank` 在前面加名次列；`bar` 格式列显示相对本列最大值的影响条。 |

值格式 `format`：`text`、`code`、`number`、`compact`、`percent`（数值即百分数，如 `72.4`）、`ratio`、`duration_ms`、`date`、`month`（ISO 日期按 UTC 显示）、`timestamp_ms`、`date_unix_sec`、`bar`（仅表格列）。缺失或 null 显示为“—”。

#### 界面提示 `web` / `tui`

每个数据块可带可选的界面提示，只影响呈现，不改变数据；未声明时使用默认呈现，未列出的字段或不适用于该块类型的取值会被清单校验拒绝：

| 提示 | 适用块 | 取值 | WebUI 默认 |
| --- | --- | --- | --- |
| `web.hidden` / `tui.hidden` | 全部 | 布尔；在该界面隐藏此块 | 显示 |
| `web.variant` | `metrics` | `cards`（宿主指标卡片）、`tiles`（紧凑着色方块，适合四项摘要） | `cards` |
| `web.variant` | `chart` | `bars`（柱状图）、`line`（折线图） | `bars` |
| `web.columns` | `metrics` | 1–6 列（窄屏折为两列） | 自适应 |
| `tui.variant` | `chart` | `sparkline`、`table` | — |

WebUI 只读取 `web` 提示，TUI 只读取 `tui` 提示。TUI 中 `chart` 默认显示为迷你走势图，`tui.variant: table`
改为每个数据点一行；`metrics` 显示为数值卡片条，`table` 显示为每行一个条目（第一列为名称，其余列为说明）。

### 条目 `type: field`

编辑插件配置 Schema 的一个**顶层属性**；同一 `key` 只能出现一次。

| `control` | 属性类型 | 专属字段 |
| --- | --- | --- |
| `switch` | `boolean` | — |
| `select` | `integer` 或 `string` | 必填 `options`；可选 `value_format` |
| `text` | `string` | 可选 `placeholder_message_key` |
| `number` | `integer`、`number` 或数字模式的 `string` | 可选 `placeholder_message_key` |
| `checklist` | 元素为 `string` 的 `array` | 必填 `option_groups`；可选 `selection` |

- `options[]`：`value` 为字符串或整数，必须满足属性 Schema 且不重复；`label_message_key` 可省略，此时按 `value_format`（`plain`、`duration_ms`、`days`）本地化显示数值。
- `option_groups[]`：`id`、`title_message_key`、`values`（字符串原样显示）；各组之间值不得重复，且必须满足 `items` Schema。
- `selection`：`include` 保存勾选项（默认）；`exclude` 保存未勾选项，适合“全部启用、个别排除”且需覆盖未来新增值的场景。
- `visible_when: { key, values }`：仅当同版面另一字段取列出值之一时显示本行；值必须满足该字段 Schema。
- `description_message_key` 说明效果；数值范围等校验错误由宿主依据属性 Schema 显示。
- 专属字段只能用于对应控件，否则清单校验失败。

### 条目 `type: credential`

只写密钥。`id`、`label_message_key`、可选 `description_message_key` 与 `placeholder_message_key`：

- `status_action`：`effect: read`，输出 Schema 必须有布尔 `configured`；可选输出 `validationStatus`（`unknown`、`untested`、`valid`、`invalid`）用于显示状态徽标。
- `set_action`：`effect: mutation`，输入只能有一个必填字符串属性，名称即 `set_argument`。
- `delete_action`（可选）：`effect: mutation`，接受空对象；宿主要求二次确认。

宿主从不回读已保存的密钥。

### 条目 `type: action`

无参数运行一个声明了 `plugin_ui` 调用方、接受空对象的 Action，例如连接测试。`label_message_key` 同时作为按钮文字；`success_message_key` 在成功后显示。

Action 完成后宿主只重读凭据校验状态，不重置配置草稿；状态读取失败也保留尚未保存的编辑，并在操作行显示错误。

### 文案与校验

- 所有 `*_message_key` 必须存在于插件 `en` 语言资源且非空；其余语言按[多语言要求](plugin-development-guide.md#多语言资源与界面)补齐。
- 运行包校验：`cargo run --locked -q -p vulcan-plugin-runtime --features manifest-schema --bin vulcan-plugin-manifest -- validate plugins/<id>`。版面错误会指明挂载、字段与具体规则。

## 模块页面（`kind: module`）

模块导出工厂 `createPluginPanel(sdk)`（名称由 `export` 指定），返回一个 React 组件。模块是未编译 ESM，使用 `sdk.React.createElement`，不得自带 React 或任何第三方 UI 库。

### SDK 字段

| 字段 | 用途 |
| --- | --- |
| `React` | 宿主 React 实例。 |
| `context` | `conversationId`（当前会话，可能为空）、`clientHost`（实际宿主壳）。 |
| `invoke(actionId, args, { refresh?, signal? })` | 调用本插件声明的 Action；相同读取由宿主合并，`signal` 只取消本视图的结果投递。 |
| `readTextAsset(path)` | 读取清单声明的 UTF-8 文本资源，例如许可证。 |
| `openExternalUrl(url)` | 经宿主在系统浏览器中打开外部地址；只接受不含凭据的 `https://` 地址，不设域名白名单。 |
| `locale`、`t(key, fallback)`、`formatNumber(value)`、`formatTokenCount(value)` | 当前语言、插件文案与数值格式化；Token 数量一律用 `formatTokenCount`（1000 以下精确，其后为一位小数的 K/M/B/T），与主界面同源；语言变化时宿主重建组件。 |
| `events.subscribe(topic, listener)`、`events.latestRevision(topic)` | 仅含修订号的失效通知，用于重新读取。 |
| `notifications.success/error(message)` | 用户发起操作的结果提示。 |
| `components` | 下文的宿主组件；不在列表内的组件不可用。 |

### 页面结构

页面根使用 `PageBody`，按以下顺序组合：

1. `PageTabs`：页面内多个任务区域；`actions` 放页面状态（`StatusPill`）。每个区域内容放进 `PageTabPanel active={…}`，切换时保留已访问区域的状态。
2. `PageToolbar`：起始侧放筛选与上下文徽标，`actions` 放刷新、时间范围等页面命令。起始侧的 `Input` 由页面套件统一宽度，不要为搜索框另设尺寸类。
3. 页面级反馈：`PageNotice tone="error|warning|info|success"`，原始诊断放 `details`，恢复命令放 `actions`。
4. 内容：每个关注点一个 `PageSection`（`title`、`description`、`status`、`actions`、`footer`）；指标用 `MetricGrid` 加 `MetricCard`；表格用 `PageTable`；键值事实用 `PageFacts`；列表用 `PageItemList`/`PageItem`；空内容用 `PageEmptyState`；首屏加载用 `StreamPlaceholder`。
5. 分页放在所属 `PageSection` 的 `footer`；表格局部加载与错误留在所属区块，不覆盖整页。

`PageTable` 的列用 `{ key, label, numeric?, wrap?, width? }`，行用 `{ key, cells, selected? }`；密集明细表使用 `layout: "fixed"`、列 `width` 与 `minWidth`，在窄容器中横向滚动而不挤压。

### 可用组件

| 类别 | 组件 |
| --- | --- |
| 页面布局 | `PageBody`、`PageToolbar`、`PageTabs`、`PageTabPanel`、`PageActionBar` |
| 区块与行 | `PageSection`、`PageRow`、`PageHint`、`PageGuide` |
| 反馈 | `PageNotice`、`PageEmptyState`、`StreamPlaceholder`、`StatusPill` |
| 数据 | `PageFacts`、`PageDetails`、`MetricCard`、`MetricGrid`、`PageItemList`、`PageItem`、`PageTable` |
| 控件 | `Badge`、`Button`、`Input`、`Checkbox`、`Switch`、`SelectControl`、`HelpTooltip`、`PluginMetadataLink` |
| 对话框 | `Dialog`、`DialogContent`、`DialogHeader`、`DialogTitle`、`DialogDescription`、`DialogFooter`、`TextDocumentDialog` |
| 图表 | `ActivityHeatmap`、`ActivityHeatmapLegend`、`TimeSeriesLineChart`（可选 `formatValue` 提示格式、`formatAxisValue` 纵轴格式）、`BarChart`（`{ ariaLabel, points: [{ key, label, value, title? }], emptyLabel }`）、`ChartLegend` |
| 宿主投影 | `CoreBrowserArea`、`ClientBrowserArea`（仅内置浏览器插件） |

`MetricCard.icon` 只能用 `activity`、`agent`、`calendar`、`duration`、`internal`、`model`、`requests`、`success`、`subagent`、`tokens`。

### 自定义样式限制

插件不携带样式表。`apps/vulcan-code-web` 的 `pnpm run check:ui-architecture`（`pnpm run build` 的一部分）对 `plugins/*/ui/*.mjs`（测试文件除外）强制执行：

- 禁止 `style` 元素、`CSSStyleSheet` 与 `adoptedStyleSheets` 等样式注入。
- 禁止 `document.*`、`innerHTML`、`outerHTML`、`insertAdjacentHTML` 与 `dangerouslySetInnerHTML`；一切渲染经 `sdk.React` 与 `sdk.components`。
- 禁止颜色字面量（十六进制、`rgb()`、`hsl()`、`oklch()` 等）；颜色只用主题变量，例如 `var(--primary)`、`var(--muted-foreground)`、`var(--chart-1)` 至 `var(--chart-5)`、`var(--destructive)`。
- 禁止 `position: fixed/sticky` 与 `zIndex`；浮层由宿主组件（`Dialog`、`HelpTooltip`）提供。
- 禁止像素字号；字号用 `var(--vcn-font-size-xs|sm|md|lg|xl)` 或交给组件。
- `className` 只能使用 `src/ui-kit/styles/features/plugin-host-components.css` 中定义的类名；宿主套件类名（`vk-*`）与页面套件内部类名都不是插件词汇。确需插件专属修饰时，在该样式表新增以插件名为前缀的规则（如 `.plugin-statistics-*`），只修饰单元格内容或图表，不重建区块、卡片、表格或工具栏。
- 内联 `style` 仅用于数据驱动的尺寸，例如占比条宽度，且颜色同样只用主题变量。

## 验证

- 插件包：上文的 `validate` 命令与 `make.ps1 sync`。
- 模块：`node --test plugins/<id>/ui/*.test.mjs`；测试可以为 SDK 组件打桩，但应保留真实语义结构（区块、表格、提示）。
- 宿主：在 `apps/vulcan-code-web` 运行 `pnpm run check:ui-architecture` 与 `pnpm exec vitest run src/components/plugins`。
- 视觉对照：设置 → 视觉实验室的「页面套件」「插件设置页」「插件模块页」分类以真实组件渲染本文全部结构；设置页分类直接驱动真实的声明式渲染器（Action 在内存中模拟），可用于核对版面与样式。
