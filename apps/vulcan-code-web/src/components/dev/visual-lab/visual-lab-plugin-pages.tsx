import * as React from "react"

import {
  MetricCard, MetricGrid, PageBody, PageDetails, PageEmptyState, PageNotice, PageSection, PageTable, PageToolbar,
} from "@/components/page"
import { createPluginSettingsPage, type PluginSettingsHost } from "@/components/plugins/plugin-settings-page"
import type { PluginCredentialStatus, PluginSettingsLayout } from "@/components/plugins/plugin-settings-layout"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"

import type { VisualLabDemoProps, VisualLabText } from "./visual-lab-shared"

// Real mounts load plugin-only styles at the mount boundary; the lab renders the same components directly, so it loads them here.
// 真实挂载在挂载边界加载插件专用样式；实验室直接渲染相同组件，因此在此加载。
import "@/styles/plugin-host-components.css"

// DEMO_LATENCY_MS makes every synthetic action resolve asynchronously, like a real plugin action.
// DEMO_LATENCY_MS 让每个合成 Action 都异步完成，与真实插件 Action 一致。
export const DEMO_LATENCY_MS = 350

// DEMO_LICENSE is the package document the header license tag opens.
// DEMO_LICENSE 是标题区许可证标签打开的包内文档。
const DEMO_LICENSE = "Example License 1.0\n\nPermission is granted to use this demo plugin for visual review only."

// DEMO_LAYOUT is a complete settings layout v1: header, tabs, every control, an inline section, a credential, an action and a data view.
// DEMO_LAYOUT 是完整的 v1 设置版面：标题区、标签、全部控件、行内区块、凭据、操作与数据视图。
const DEMO_LAYOUT: PluginSettingsLayout = {
  schema_version: 1,
  config: { read_action: "settings-get", write_action: "settings-update" },
  header: {
    status: { ready_message_key: "status.ready", unavailable_message_key: "status.unavailable" },
    links: [{ url: "https://github.com/example/plugin", label_message_key: "header.repository", icon: "github" }],
    documents: [{ id: "license", asset: "LICENSE", label_message_key: "header.license", title_message_key: "header.licenseTitle", icon: "license" }],
  },
  tabs: [
    { id: "general", title_message_key: "tab.general", sections: ["quick", "behavior", "account"] },
    { id: "usage", title_message_key: "tab.usage", sections: ["usage"] },
  ],
  sections: [
    {
      id: "quick", title_message_key: "quick.title", layout: "inline",
      items: [
        { type: "field", key: "telemetry", control: "switch", label_message_key: "telemetry.label", description_message_key: "telemetry.description" },
        { type: "field", key: "compact", control: "switch", label_message_key: "compact.label", description_message_key: "compact.description" },
      ],
    },
    {
      id: "behavior", title_message_key: "behavior.title", description_message_key: "behavior.description",
      items: [
        { type: "field", key: "enabled", control: "switch", label_message_key: "enabled.label", description_message_key: "enabled.description" },
        { type: "field", key: "timeoutMs", control: "select", label_message_key: "timeout.label", value_format: "duration_ms", options: [{ value: 15000 }, { value: 30000 }, { value: 60000 }] },
        { type: "field", key: "maxResults", control: "number", label_message_key: "maxResults.label", description_message_key: "maxResults.description" },
        { type: "field", key: "endpoint", control: "text", label_message_key: "endpoint.label", placeholder_message_key: "endpoint.placeholder" },
        { type: "field", key: "scope", control: "select", label_message_key: "scope.label", options: [{ value: "selected", label_message_key: "scope.selected" }, { value: "all", label_message_key: "scope.all" }] },
        {
          type: "field", key: "commands", control: "checklist", label_message_key: "commands.label", description_message_key: "commands.description",
          selection: "exclude", visible_when: { key: "scope", values: ["all"] },
          option_groups: [{ id: "git", title_message_key: "commands.git", values: ["git status", "git diff", "git log"] }, { id: "files", title_message_key: "commands.files", values: ["ls", "find", "grep"] }],
        },
      ],
    },
    {
      id: "account", title_message_key: "account.title", description_message_key: "account.description",
      links: [{ url: "https://example.com/api-keys", label_message_key: "account.link" }],
      items: [
        { type: "credential", id: "apiKey", label_message_key: "apiKey.label", description_message_key: "apiKey.description", placeholder_message_key: "apiKey.placeholder", status_action: "credential-status", set_action: "credential-set", set_argument: "apiKey", delete_action: "credential-delete" },
        { type: "action", id: "test", action: "connection-test", label_message_key: "test.label", description_message_key: "test.description", success_message_key: "test.success" },
      ],
    },
    {
      id: "usage", title_message_key: "usage.title", description_message_key: "usage.description",
      data: {
        action: "usage-get", refreshable: true, show_loaded_at: true,
        arguments: [{ name: "rangeDays", label_message_key: "usage.range", value_format: "days", default: 30, options: [{ value: 7 }, { value: 30 }, { value: 90 }] }],
      },
      blocks: [
        {
          type: "metrics", web: { variant: "tiles", columns: 4 }, items: [
            { label_message_key: "usage.saved", path: "summary.saved", format: "compact" },
            { label_message_key: "usage.rate", path: "summary.rate", format: "percent" },
            { label_message_key: "usage.commands", path: "summary.commands", format: "compact" },
            { label_message_key: "usage.capacity", ratio: { numerator: "summary.input", denominator: "summary.output" }, format: "ratio" },
          ],
        },
        {
          type: "chart", value_label_message_key: "usage.saved", value_path: "saved", value_format: "number", detail: { path: "rate", format: "percent" },
          empty_message_key: "usage.empty", web: { variant: "bars" }, tui: { variant: "sparkline" },
          periods: [
            { id: "daily", label_message_key: "usage.daily", path: "daily", label_paths: ["date"], label_format: "date" },
            { id: "monthly", label_message_key: "usage.monthly", path: "monthly", label_paths: ["month"], label_format: "month" },
          ],
        },
        {
          type: "table", path: "byCommand", title_message_key: "usage.byCommand", count_message_key: "usage.top", rank: true, empty_message_key: "usage.empty",
          columns: [
            { path: "command", label_message_key: "usage.command", format: "code" },
            { path: "count", label_message_key: "usage.count", format: "number" },
            { path: "saved", label_message_key: "usage.saved", format: "number" },
            { path: "avgMs", label_message_key: "usage.time", format: "duration_ms" },
            { path: "saved", label_message_key: "usage.impact", format: "bar" },
          ],
        },
      ],
    },
  ],
}

// demoUsage returns deterministic synthetic statistics whose size follows the selected range.
// demoUsage 返回确定的合成统计，数据量随所选范围变化。
function demoUsage(rangeDays: number) {
  const daily = Array.from({ length: Math.min(rangeDays, 14) }, (_, index) => ({
    date: `2026-09-${String(16 + index).padStart(2, "0")}`,
    saved: 18_000 + ((index * 7919) % 23) * 1_400,
    rate: 60 + ((index * 31) % 25),
  }))
  return {
    summary: { saved: rangeDays * 41_200, rate: 72.4, commands: rangeDays * 104, input: 5_400_000, output: 1_500_000 },
    daily,
    monthly: [{ month: "2026-07", saved: 820_000, rate: 70.2 }, { month: "2026-08", saved: 1_040_000, rate: 73.9 }, { month: "2026-09", saved: 1_236_000, rate: 74.1 }],
    byCommand: [
      { command: "git diff", count: 412, saved: 512_300, avgMs: 84 },
      { command: "cargo test", count: 166, saved: 398_000, avgMs: 2_140 },
      { command: "rg", count: 980, saved: 201_400, avgMs: 41 },
      { command: "ls", count: 1_204, saved: 64_800, avgMs: 12 },
    ],
  }
}

// DEMO_SCHEMA is the configuration schema the renderer checks drafts against before Core re-validates.
// DEMO_SCHEMA 是渲染器在 Core 复核前用于检查草稿的配置 Schema。
const DEMO_SCHEMA = {
  type: "object",
  properties: {
    enabled: { type: "boolean" },
    telemetry: { type: "boolean" },
    compact: { type: "boolean" },
    timeoutMs: { type: "integer", enum: [15000, 30000, 60000] },
    maxResults: { type: "integer", minimum: 1, maximum: 50 },
    endpoint: { type: "string", pattern: "^https://" },
    scope: { type: "string", enum: ["selected", "all"] },
    commands: { type: "array", items: { type: "string" } },
  },
}

// demoMessages resolves the plugin-owned message keys of the demo layout in the active locale.
// demoMessages 以当前语言解析示例版面的插件自有消息键。
// Parameters: text is the locale-bound copy selector. Returns the message resolver.
// 参数：text 为已绑定语言的文案选择器。返回消息解析函数。
function demoMessages(text: VisualLabText): (key: string) => string {
  const table: Record<string, string> = {
    "behavior.title": text("行为", "Behavior"),
    "behavior.description": text("全部字段来自 settings 版面声明，宿主自动渲染。", "Every field comes from the settings layout; the host renders it automatically."),
    "enabled.label": text("启用插件", "Enable plugin"),
    "enabled.description": text("关闭后保留配置但不再执行。", "Keeps configuration but stops running when off."),
    "timeout.label": text("请求超时", "Request timeout"),
    "maxResults.label": text("最大结果数", "Maximum results"),
    "maxResults.description": text("范围由配置 Schema 决定。", "The range comes from the configuration schema."),
    "endpoint.label": text("服务地址", "Endpoint"),
    "endpoint.placeholder": "https://",
    "scope.label": text("作用范围", "Scope"),
    "scope.selected": text("仅选中的命令", "Selected commands only"),
    "scope.all": text("全部命令（可排除）", "All commands (with exclusions)"),
    "commands.label": text("排除的命令", "Excluded commands"),
    "commands.description": text("仅在作用范围为全部时显示。", "Shown only when the scope is all commands."),
    "commands.git": "Git",
    "commands.files": text("文件", "Files"),
    "account.title": text("账号", "Account"),
    "account.description": text("凭据只写不读；状态由插件 Action 返回。", "Credentials are write-only; status comes from a plugin action."),
    "account.link": text("获取 API 密钥", "Get an API key"),
    "apiKey.label": text("API 密钥", "API key"),
    "apiKey.description": text("保存后显示验证状态。", "Validation status appears after saving."),
    "apiKey.placeholder": "sk-…",
    "test.label": text("测试连接", "Test connection"),
    "test.description": text("调用插件声明的操作 Action。", "Calls an action declared by the plugin."),
    "test.success": text("连接正常。", "Connection works."),
    "status.ready": text("可用", "Available"),
    "status.unavailable": text("不可用", "Unavailable"),
    "header.repository": "example/plugin",
    "header.license": "Example-1.0",
    "header.licenseTitle": text("示例许可证", "Example license"),
    "tab.general": text("设置", "Settings"),
    "tab.usage": text("统计", "Statistics"),
    "quick.title": text("快速开关", "Quick switches"),
    "telemetry.label": text("关闭遥测", "Disable telemetry"),
    "telemetry.description": text("行内区块把说明放进帮助提示。", "Inline sections put the description in a help tooltip."),
    "compact.label": text("紧凑输出", "Compact output"),
    "compact.description": text("仅限无条件的开关字段。", "Only unconditional switch fields may be inline."),
    "usage.title": text("用量统计", "Usage statistics"),
    "usage.description": text("数据视图：一个读取 Action 渲染为指标、周期图与表格；切换周期不再请求。", "A data view: one read action rendered as metrics, a period chart and a table; switching periods sends no request."),
    "usage.range": text("统计范围", "Range"),
    "usage.saved": text("节省 Token", "Tokens saved"),
    "usage.rate": text("平均节省", "Average savings"),
    "usage.commands": text("命令数", "Commands"),
    "usage.capacity": text("上下文容量", "Context capacity"),
    "usage.empty": text("暂无数据", "No data yet"),
    "usage.daily": text("每日", "Daily"),
    "usage.monthly": text("每月", "Monthly"),
    "usage.byCommand": text("按命令", "By command"),
    "usage.top": text("前 {{count}} 个命令", "Top {{count}} commands"),
    "usage.command": text("命令", "Command"),
    "usage.count": text("次数", "Count"),
    "usage.time": text("耗时", "Time"),
    "usage.impact": text("影响", "Impact"),
  }
  return (key) => table[key] ?? key
}

// useDemoSettingsHost builds one in-memory plugin host whose state survives locale changes.
// useDemoSettingsHost 构建一个内存插件宿主，其状态在切换语言时保留。
// Parameters: onAction records host calls in the lab status line. Returns the stable host capabilities except message.
// 参数：onAction 在实验室状态行记录宿主调用。返回除 message 外的稳定宿主能力。
function useDemoSettingsHost(onAction: (label: string) => void): Omit<PluginSettingsHost, "message"> {
  const store = React.useRef({
    config: {
      enabled: true, telemetry: false, compact: true, timeoutMs: 30000, maxResults: 20, endpoint: "https://api.example.com", scope: "selected", commands: [] as string[],
    } as Record<string, unknown>,
    credential: { configured: false } as PluginCredentialStatus,
  })
  const recorder = React.useRef(onAction)
  recorder.current = onAction
  return React.useMemo(() => ({
    invoke: async <Result,>(actionId: string, argumentsValue: unknown): Promise<Result> => {
      await new Promise((resolve) => window.setTimeout(resolve, DEMO_LATENCY_MS))
      const state = store.current
      if (actionId === "settings-update") state.config = { ...(argumentsValue as Record<string, unknown>) }
      if (actionId === "credential-set") state.credential = { configured: true, validationStatus: "untested" }
      if (actionId === "credential-delete") state.credential = { configured: false }
      if (actionId === "connection-test" && state.credential.configured) state.credential = { configured: true, validationStatus: "valid" }
      if (actionId !== "settings-get" && actionId !== "credential-status") recorder.current(`invoke ${actionId}`)
      if (actionId === "settings-get" || actionId === "settings-update") return state.config as Result
      if (actionId.startsWith("credential-")) return state.credential as Result
      if (actionId === "usage-get") return demoUsage((argumentsValue as { rangeDays: number }).rangeDays) as Result
      return {} as Result
    },
    openExternalUrl: async (url: string) => recorder.current(`openExternalUrl ${url}`),
    readTextAsset: async (path: string) => {
      recorder.current(`readTextAsset ${path}`)
      return DEMO_LICENSE
    },
    subscribe: () => () => undefined,
  }), [])
}

// VisualLabPluginSettings renders the real declarative settings renderer against an in-memory plugin host.
// VisualLabPluginSettings 以内存插件宿主驱动真实的声明式设置渲染器。
// Parameters: props supplies copy and the local action recorder. Returns the rendered page plus its source layout.
// 参数：props 提供文案与本地操作记录器。返回渲染出的页面及其源版面。
export function VisualLabPluginSettings({ text, onAction }: VisualLabDemoProps) {
  const host = useDemoSettingsHost(onAction)
  // message changes with the locale, so the page component is rebuilt while the host state is kept.
  // message 随语言变化，因此重建页面组件，同时保留宿主状态。
  const message = React.useMemo(() => demoMessages(text), [text])
  const SettingsPage = React.useMemo(() => createPluginSettingsPage(DEMO_LAYOUT, DEMO_SCHEMA, { ...host, message }), [host, message])
  return (
    <>
      <PageNotice tone="info" title={text("声明式设置页", "Declarative settings page")}>
        {text(
          "插件只声明 kind: settings 版面与配置 Schema，宿主用页面套件渲染、校验并保存。下方是真实渲染器，所有 Action 都在本页内存中模拟。",
          "A plugin declares only a kind: settings layout and its configuration schema; the host renders, validates and saves it with the page kit. Below is the real renderer; every action is simulated in memory on this page.",
        )}
      </PageNotice>
      <SettingsPage />
      <PageDetails label={text("版面声明（ui/settings.json，由 plugin.yaml 的 kind: settings 挂载引用）", "Layout declaration (ui/settings.json, referenced by a kind: settings mount in plugin.yaml)")}>
        <pre>{JSON.stringify(DEMO_LAYOUT, null, 2)}</pre>
      </PageDetails>
    </>
  )
}

// MODULE_SOURCE is the module page below written as a plugin author writes it, for side-by-side review.
// MODULE_SOURCE 是下方模块页面按插件作者写法写出的源码，便于对照验收。
const MODULE_SOURCE = `export function createPluginPanel(sdk) {
  const h = sdk.React.createElement
  const { PageBody, PageToolbar, MetricGrid, MetricCard, PageSection, PageTable, Button } = sdk.components
  return function Panel() {
    return h(PageBody, null,
      h(PageToolbar, { actions: h(Button, { size: "sm", variant: "outline" }, sdk.t("refresh", "Refresh")) }),
      h(MetricGrid, null, h(MetricCard, { icon: "requests", label: "Requests", value: "1,204" })),
      h(PageSection, { title: "Recent jobs" },
        h(PageTable, { columns, rows, layout: "fixed" })))
  }
}`

// VisualLabPluginModule renders a module page composed only from sdk.components, beside its authoring source.
// VisualLabPluginModule 渲染只由 sdk.components 组成的模块页面，并附上其编写源码。
// Parameters: props supplies copy and the local action recorder. Returns the module page sample.
// 参数：props 提供文案与本地操作记录器。返回模块页面样本。
export function VisualLabPluginModule({ text, onAction }: VisualLabDemoProps) {
  const [jobs, setJobs] = React.useState(2)
  const rows = Array.from({ length: jobs }, (_, index) => ({
    key: `job-${index}`,
    cells: { name: `index-${index + 1}`, state: <Badge variant={index === 0 ? "secondary" : "outline"}>{index === 0 ? text("运行中", "Running") : text("完成", "Done")}</Badge>, duration: `${(index + 1) * 12}s` },
  }))
  return (
    <>
      <PageNotice tone="info" title={text("模块页面", "Module page")}>
        {text(
          "非设置节点可以自行组合版面，但只能使用 sdk.components 与 plugin-host-components.css 中发布的类名；构建会拒绝注入样式、操作 DOM、颜色字面量与固定定位。",
          "Non-settings mounts compose their own layout, but only from sdk.components and the class names published in plugin-host-components.css; the build rejects style injection, DOM access, color literals and fixed positioning.",
        )}
      </PageNotice>
      <PageBody>
        <PageToolbar actions={<>
          <Button size="sm" variant="outline" onClick={() => { setJobs(0); onAction(text("清空", "Clear")) }}>{text("清空", "Clear")}</Button>
          <Button size="sm" variant="outline" onClick={() => { setJobs((value) => value + 1); onAction(text("刷新", "Refresh")) }}>{text("刷新", "Refresh")}</Button>
        </>} />
        <MetricGrid>
          <MetricCard icon="requests" label={text("任务", "Jobs")} value={String(jobs)} />
          <MetricCard icon="duration" label={text("平均耗时", "Average duration")} value={jobs ? `${((jobs + 1) * 6).toFixed(0)}s` : "—"} />
        </MetricGrid>
        <PageSection title={text("最近任务", "Recent jobs")} status={<Badge variant="secondary">{jobs}</Badge>}>
          <PageTable
            layout="fixed"
            label={text("最近任务", "Recent jobs")}
            columns={[{ key: "name", label: text("名称", "Name") }, { key: "state", label: text("状态", "State"), width: "120px" }, { key: "duration", label: text("耗时", "Duration"), numeric: true, width: "96px" }]}
            rows={rows}
            empty={<PageEmptyState title={text("暂无任务", "No jobs yet")} />}
          />
        </PageSection>
      </PageBody>
      <PageDetails label={text("模块源码（ui/index.mjs）", "Module source (ui/index.mjs)")}>
        <pre>{MODULE_SOURCE}</pre>
      </PageDetails>
    </>
  )
}
