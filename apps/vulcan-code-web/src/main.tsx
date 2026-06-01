import React from "react";
import ReactDOM from "react-dom/client";
import ReactMarkdown from "react-markdown";
import rehypeSanitize from "rehype-sanitize";
import remarkGfm from "remark-gfm";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import {
  Dialog,
  DialogDescription,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import {
  Sidebar,
  SidebarContent,
  SidebarGroup,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarInset,
  SidebarMenu,
  SidebarMenuAction,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
  SidebarTrigger,
  useSidebar,
} from "@/components/ui/sidebar";
import { Toaster } from "@/components/ui/sonner";
import { Textarea } from "@/components/ui/textarea";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import { toast } from "sonner";
import {
  Activity,
  Boxes,
  BrainCircuit,
  Archive,
  ChevronDown,
  ChevronRight,
  CheckCircle2,
  CircleAlert,
  Clock3,
  Code2,
  Copy,
  Database,
  ExternalLink,
  FileText,
  Folder,
  FolderOpen,
  GitBranch,
  Hammer,
  History,
  Link,
  Minus,
  LayoutDashboard,
  MailOpen,
  Maximize2,
  MessageSquareText,
  PanelTopOpen,
  Pencil,
  Pin,
  PlugZap,
  PinOff,
  ScrollText,
  Settings,
  SquarePen,
  SlidersHorizontal,
  Moon,
  Sun,
  TerminalSquare,
  Wrench,
  X,
} from "lucide-react";
import "./styles.css";

// StartupStepStatus mirrors the Rust service API startup status enum.
// StartupStepStatus 对齐 Rust service API 的启动状态枚举。
type StartupStepStatus = "pending" | "running" | "ready" | "warning" | "error";

// MainAreaId describes selectable workbench feature areas.
// MainAreaId 描述可选择的工作台功能区域。
type MainAreaId = "chat" | "projects" | "providers" | "tools" | "plugins" | "memory" | "settings" | "logs";

// ThemeMode describes the two shadcn default theme modes.
// ThemeMode 描述 shadcn 默认的两种主题模式。
type ThemeMode = "light" | "dark";

// DraftConversationSelection describes a not-yet-persisted conversation opened from a project.
// DraftConversationSelection 描述从项目打开但尚未持久化的草稿会话。
interface DraftConversationSelection {
  // Stable local-only draft identifier.
  // 稳定的本地草稿标识。
  id: string;
  // Owning project identifier.
  // 所属项目标识。
  projectId: string;
  // Owning project display name.
  // 所属项目展示名称。
  projectName: string;
}

// StartupStepSnapshot describes one startup step displayed by the startup panel.
// StartupStepSnapshot 描述启动面板展示的单个启动步骤。
interface StartupStepSnapshot {
  // Stable startup step identifier.
  // 稳定启动步骤标识。
  id: string;
  // User-facing startup step label.
  // 面向用户显示的启动步骤名称。
  label: string;
  // Current startup step status.
  // 当前启动步骤状态。
  status: StartupStepStatus;
  // Optional startup detail message.
  // 可选启动详情消息。
  message?: string | null;
}

// StartupSnapshot describes the full startup status returned by the service.
// StartupSnapshot 描述服务返回的完整启动状态。
interface StartupSnapshot {
  // Overall readiness flag.
  // 整体就绪标记。
  ready: boolean;
  // Current startup phase.
  // 当前启动阶段。
  phase: string;
  // Ordered startup steps.
  // 有序启动步骤。
  steps: StartupStepSnapshot[];
  // Recent startup log lines.
  // 最近启动日志行。
  recentLogs: string[];
}

// RuntimeModuleSnapshot describes one runtime module card.
// RuntimeModuleSnapshot 描述单个运行时模块卡片。
interface RuntimeModuleSnapshot {
  // Stable module identifier.
  // 稳定模块标识。
  id: string;
  // User-facing module label.
  // 面向用户显示的模块名称。
  label: string;
  // Current module status.
  // 当前模块状态。
  status: StartupStepStatus;
  // Short module message.
  // 简短模块消息。
  message: string;
}

// ProjectSummary describes one project shown in the sidebar.
// ProjectSummary 描述侧栏中展示的单个项目。
interface ProjectSummary {
  // Stable project identifier.
  // 稳定项目标识。
  id: string;
  // Project display name.
  // 项目显示名称。
  name: string;
  // Project path.
  // 项目路径。
  path: string;
  // Whether the project is currently pinned.
  // 项目当前是否处于置顶状态。
  pinned?: boolean;
}

// ConversationSummary describes one conversation shown in the sidebar.
// ConversationSummary 描述侧栏中展示的单个会话。
interface ConversationSummary {
  // Stable conversation identifier.
  // 稳定会话标识。
  id: string;
  // Conversation title.
  // 会话标题。
  title: string;
  // Conversation status.
  // 会话状态。
  status: string;
  // Optional owning project identifier.
  // 可选所属项目标识。
  projectId?: string | null;
}

// ConversationMessage describes one rendered message in the timeline.
// ConversationMessage 描述时间线中的单条渲染消息。
interface ConversationMessage {
  // Stable message identifier.
  // 稳定消息标识。
  id: string;
  // Stable conversation identifier that owns this message.
  // 拥有该消息的稳定会话标识。
  conversationId: string;
  // Message role.
  // 消息角色。
  role: "user" | "assistant" | "system";
  // Optional transcript title from the persisted session entry.
  // 来自持久化会话项的可选转录标题。
  title?: string | null;
  // Optional durable transcript record kind.
  // 可选持久化转录记录类型。
  recordKind?: string | null;
  // Message body.
  // 消息正文。
  body: string;
  // Optional structured transcript metadata.
  // 可选结构化转录元数据。
  metadata?: unknown;
}

// ConversationBlockKind describes one WebUI-renderable conversation block family.
// ConversationBlockKind 描述 WebUI 可渲染的单个会话块族。
type ConversationBlockKind = "markdown" | "tool-call" | "tool-result" | "command" | "diff" | "thinking" | "notice" | "turn-stats";

// ConversationRenderItemKind describes the projected transcript item family.
// ConversationRenderItemKind 描述投影后的转录展示项族。
type ConversationRenderItemKind = "user" | "assistant" | "system";

// ConversationBlock describes one parsed message block shown inside the transcript.
// ConversationBlock 描述转录区中展示的单个解析后消息块。
interface ConversationBlock {
  // Stable block identifier within the owning message.
  // 所属消息内的稳定块标识。
  id: string;
  // Semantic block kind used for component selection.
  // 用于选择组件的语义块类型。
  kind: ConversationBlockKind;
  // Short block title shown in the compact row or modal.
  // 展示在紧凑行或弹窗中的简短块标题。
  title: string;
  // Full block body used by markdown renderers and detail views.
  // 供 Markdown 渲染器与详情视图使用的完整块正文。
  body: string;
  // Compact summary shown before opening the detail view.
  // 打开详情视图前展示的紧凑摘要。
  summary: string;
  // Optional status text detected from legacy transcript formats.
  // 从旧转录格式中识别出的可选状态文本。
  status?: string;
  // Optional tool name detected from tool-call or tool-result formats.
  // 从工具调用或工具返回格式中识别出的可选工具名称。
  toolName?: string;
  // Optional provider or host-visible tool call identifier.
  // 可选 Provider 或宿主可见工具调用标识。
  toolCallId?: string;
  // Optional host-visible tool result identifier.
  // 可选宿主可见工具返回标识。
  toolResultId?: string;
  // Optional command-session identifier for command-like tool rows.
  // 命令类工具行使用的可选命令会话标识。
  commandSessionId?: string;
}

// ConversationRenderItem describes one row rendered after transcript projection.
// ConversationRenderItem 描述转录投影后的单条渲染行。
interface ConversationRenderItem {
  // Stable projected row identifier.
  // 稳定投影行标识。
  id: string;
  // Projected row role.
  // 投影行角色。
  role: ConversationRenderItemKind;
  // Blocks rendered inside this projected row.
  // 在该投影行中渲染的块集合。
  blocks: ConversationBlock[];
  // Original user message when this row is a user bubble.
  // 当该行为用户气泡时对应的原始用户消息。
  userMessage?: ConversationMessage;
}

// ConversationDetail describes the modal payload opened from one special block.
// ConversationDetail 描述从特殊块打开的弹窗载荷。
interface ConversationDetail {
  // Detail modal title.
  // 详情弹窗标题。
  title: string;
  // Detail modal kind used for visual treatment.
  // 用于视觉处理的详情弹窗类型。
  kind: ConversationBlockKind;
  // Detail body source.
  // 详情正文源码。
  body: string;
  // Optional detail metadata rows.
  // 可选详情元数据行。
  metadata?: Array<[string, string]>;
}

// ToolCallSummary describes one tool call block.
// ToolCallSummary 描述单个工具调用块。
interface ToolCallSummary {
  // Stable tool call identifier.
  // 稳定工具调用标识。
  id: string;
  // Tool display name.
  // 工具显示名称。
  name: string;
  // Tool status.
  // 工具状态。
  status: string;
  // Tool detail text.
  // 工具详情文本。
  detail: string;
}

// FeatureAreaSummary describes one primary navigation area.
// FeatureAreaSummary 描述单个主导航区域。
interface FeatureAreaSummary {
  // Stable feature area identifier.
  // 稳定功能区域标识。
  id: MainAreaId;
  // Feature area display label.
  // 功能区域显示名称。
  label: string;
  // Feature area description.
  // 功能区域描述。
  description: string;
  // Feature area status.
  // 功能区域状态。
  status: string;
}

// PluginSummary describes one plugin shown in plugin center.
// PluginSummary 描述插件中心展示的单个插件。
interface PluginSummary {
  // Stable plugin identifier.
  // 稳定插件标识。
  id: string;
  // Plugin display name.
  // 插件显示名称。
  name: string;
  // Plugin kind.
  // 插件类型。
  kind: string;
  // Plugin status.
  // 插件状态。
  status: string;
  // Plugin capability summary.
  // 插件能力摘要。
  capability: string;
}

// SettingSectionSummary describes one settings group.
// SettingSectionSummary 描述单个设置分组。
interface SettingSectionSummary {
  // Stable settings section identifier.
  // 稳定设置分组标识。
  id: string;
  // Settings section display label.
  // 设置分组显示名称。
  label: string;
  // Settings section description.
  // 设置分组描述。
  description: string;
  // Representative setting item names.
  // 代表性设置项名称。
  items: string[];
}

// ProviderSummary describes one provider card.
// ProviderSummary 描述单个 Provider 卡片。
interface ProviderSummary {
  // Stable provider identifier.
  // 稳定 Provider 标识。
  id: string;
  // Provider display name.
  // Provider 显示名称。
  name: string;
  // Provider status.
  // Provider 状态。
  status: string;
  // Enabled model count text.
  // 已启用模型数量文本。
  enabledModels: string;
}

// MemorySummary describes one VMM memory area item.
// MemorySummary 描述单个 VMM 记忆区域项目。
interface MemorySummary {
  // Stable memory identifier.
  // 稳定记忆标识。
  id: string;
  // Memory display title.
  // 记忆显示标题。
  title: string;
  // Memory category.
  // 记忆分类。
  category: string;
  // Memory state.
  // 记忆状态。
  state: string;
}

// CommandSessionSummary describes one command session.
// CommandSessionSummary 描述单个命令会话。
interface CommandSessionSummary {
  // Stable command session identifier.
  // 稳定命令会话标识。
  id: string;
  // Command text.
  // 命令文本。
  command: string;
  // Command status.
  // 命令状态。
  status: string;
  // Recent output preview.
  // 最近输出预览。
  preview: string;
}

// LogEntrySummary describes one diagnostic log entry.
// LogEntrySummary 描述单条诊断日志。
interface LogEntrySummary {
  // Stable log entry identifier.
  // 稳定日志条目标识。
  id: string;
  // Log level.
  // 日志级别。
  level: string;
  // Log source.
  // 日志来源。
  source: string;
  // Log message.
  // 日志消息。
  message: string;
}

// WorkbenchSnapshot describes the first WebUI workbench state.
// WorkbenchSnapshot 描述第一版 WebUI 工作台状态。
interface WorkbenchSnapshot {
  // Application version.
  // 应用版本。
  appVersion: string;
  // Runtime module cards.
  // 运行时模块卡片。
  modules: RuntimeModuleSnapshot[];
  // Project summaries.
  // 项目摘要。
  projects: ProjectSummary[];
  // Conversation summaries.
  // 会话摘要。
  conversations: ConversationSummary[];
  // Currently selected conversation id.
  // 当前选中的会话标识。
  selectedConversationId?: string | null;
  // Conversation messages.
  // 会话消息。
  messages: ConversationMessage[];
  // Tool call summaries.
  // 工具调用摘要。
  toolCalls: ToolCallSummary[];
  // Main feature areas.
  // 主功能区域。
  featureAreas: FeatureAreaSummary[];
  // Plugin summaries.
  // 插件摘要。
  plugins: PluginSummary[];
  // Settings sections.
  // 设置分组。
  settings: SettingSectionSummary[];
  // Provider summaries.
  // Provider 摘要。
  providers: ProviderSummary[];
  // VMM memory summaries.
  // VMM 记忆摘要。
  memories: MemorySummary[];
  // Command session summaries.
  // 命令会话摘要。
  commandSessions: CommandSessionSummary[];
  // Diagnostic log entries.
  // 诊断日志条目。
  logs: LogEntrySummary[];
}

// TauriInvoke invokes a backend command when the app runs inside Tauri.
// TauriInvoke 在应用运行于 Tauri 内部时调用后端命令。
type TauriInvoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

// TauriListen subscribes to backend service events when running inside Tauri.
// TauriListen 在 Tauri 内运行时订阅后端服务事件。
type TauriListen = <T>(event: string, handler: (event: { payload: T }) => void) => Promise<() => void>;

// ServiceEvent describes the tagged event emitted by the Tauri service bridge.
// ServiceEvent 描述 Tauri 服务桥接层发出的带标签事件。
type ServiceEvent =
  | { kind: "startup"; payload: StartupSnapshot }
  | { kind: "workbench"; payload: WorkbenchSnapshot };

// BrowserFallbackStartup is used when the WebUI runs outside Tauri during frontend development.
// BrowserFallbackStartup 用于 WebUI 在 Tauri 外部进行前端开发时展示。
const browserFallbackStartup: StartupSnapshot = {
  ready: true,
  phase: "browser-preview",
  steps: [
    { id: "service-process", label: "Core Service", status: "warning", message: "浏览器预览模式：未连接 Tauri service bridge" },
    { id: "luaskills", label: "LuaSkills", status: "pending", message: "等待桌面壳提供真实状态" },
    { id: "vldb", label: "VLDB", status: "pending", message: "等待桌面壳提供真实状态" },
    { id: "mcp", label: "MCP Runtime", status: "pending", message: "等待桌面壳提供真实状态" },
    { id: "workspace", label: "Workspace", status: "ready", message: "前端布局预览已就绪" },
  ],
  recentLogs: ["[webui] browser preview mode"],
};

// BrowserFallbackWorkbench is used when the WebUI runs outside Tauri during frontend development.
// BrowserFallbackWorkbench 用于 WebUI 在 Tauri 外部进行前端开发时展示。
const browserFallbackWorkbench: WorkbenchSnapshot = {
  appVersion: "0.1.0",
  modules: [
    { id: "core", label: "Core Service", status: "warning", message: "浏览器预览模式" },
    { id: "luaskills", label: "LuaSkills", status: "pending", message: "等待真实状态" },
    { id: "vldb", label: "VLDB", status: "pending", message: "等待真实状态" },
    { id: "mcp", label: "MCP Runtime", status: "pending", message: "等待真实状态" },
    { id: "workspace", label: "Workspace", status: "ready", message: "前端布局预览" },
  ],
  projects: [{ id: "preview", name: "vulcan-code", path: "D:/openvulcan/vulcan-code", pinned: true }],
  conversations: [{ id: "preview", title: "Tauri WebUI 迁移预览", status: "前端预览", projectId: "preview" }],
  selectedConversationId: "preview",
  messages: [
    { id: "preview-user", conversationId: "preview", role: "user", body: "新 WebUI 需要完整承接当前界面显示。" },
    {
      id: "preview-assistant",
      conversationId: "preview",
      role: "assistant",
      body: "当前页面展示控制台、设置、插件、VMM、Provider、工具和诊断区域的第一版布局。\n\n:::tool-call tool_name=service_startup_snapshot tool_call_id=preview-service-startup status=preview\n## Tool Call\n\n`service_startup_snapshot`\n\n## Arguments\n\n```json\n{}\n```\n:::\n\n```diff\ndiff --git a/apps/vulcan-code-web/src/main.tsx b/apps/vulcan-code-web/src/main.tsx\n+ 接入 Markdown 会话渲染\n+ 特殊工具块支持详情窗口\n```\n\n后续真实会话会把工具调用、命令输出和 diff 分块展示。",
    },
    {
      id: "preview-turn-stats",
      conversationId: "preview",
      role: "assistant",
      title: "Turn Stats · completed",
      recordKind: "turn-stats",
      body: "状态：已完成\nProvider 调用：2\n工具调用：3\n输入 Tokens：45310\n输出 Tokens：1943\n思考 Tokens：1182\n总 Tokens：47253",
      metadata: {
        turn_stats: {
          status: "completed",
          provider_call_count: 2,
          tool_call_count: 3,
          input_tokens: 45310,
          output_tokens: 1943,
          reasoning_tokens: 1182,
          total_tokens: 47253,
          cached_input_tokens: 34944,
          duration_ms: 51275,
          output_tokens_per_second: 37.894,
        },
      },
    },
    {
      id: "preview-tool-result",
      conversationId: "preview",
      role: "assistant",
      title: "工具返回 · service_startup_snapshot",
      recordKind: "tool-result-output",
      body: "已运行 1 条命令\n\nCore Service 当前处于 browser-preview 模式。",
      metadata: {
        tool_call_id: "preview-service-startup",
        tool_name: "service_startup_snapshot",
        tool_result_id: "tool-result-preview-service-startup",
      },
    },
  ],
  toolCalls: [{ id: "preview-tool", name: "service_startup_snapshot", status: "preview", detail: "浏览器模式使用 fallback 数据。" }],
  featureAreas: [
    { id: "chat", label: "控制台", description: "控制台、消息流与模型输入区", status: "预览" },
    { id: "projects", label: "项目", description: "工作区项目列表与绑定状态", status: "预览" },
    { id: "providers", label: "模型", description: "Provider、模型、能力配置", status: "预览" },
    { id: "tools", label: "工具", description: "工具调用、命令会话与输出", status: "预览" },
    { id: "plugins", label: "插件", description: "LuaSkills 与高级插件控制面板", status: "预览" },
    { id: "memory", label: "VMM 记忆", description: "用户、项目、画像与记忆节点", status: "预览" },
    { id: "settings", label: "设置", description: "运行时、界面、权限与远程访问设置", status: "预览" },
    { id: "logs", label: "诊断", description: "启动日志、错误与运行时事件", status: "预览" },
  ],
  plugins: [
    { id: "vulcan-codekit", name: "Vulcan CodeKit", kind: "LuaSkill", status: "预览", capability: "代码搜索、AST 定位、结构化 patch" },
    { id: "vulcan-curl", name: "Vulcan Curl", kind: "LuaSkill", status: "预览", capability: "HTTP GET/POST 与受控请求" },
    { id: "vulcan-testkit", name: "Vulcan TestKit", kind: "LuaSkill", status: "预览", capability: "构建、测试、日志压缩与诊断" },
    { id: "ai-short-drama", name: "AI 短剧工作流", kind: "Workflow WebUI", status: "规划中", capability: "脚本、素材、播放器与工作流控制面板" },
  ],
  settings: [
    { id: "runtime", label: "运行时", description: "Core Service、LuaSkills、VLDB、MCP", items: ["启动模式", "运行时目录", "MCP 工具策略"] },
    { id: "providers", label: "模型与 Provider", description: "模型供应商、默认模型、工具能力与推理配置", items: ["默认 Provider", "默认模型", "工具调用", "推理强度"] },
    { id: "plugins", label: "插件", description: "LuaSkills、Rust lib 插件、WebUI 控制面板", items: ["插件目录", "权限声明", "WebUI 面板"] },
    { id: "remote", label: "远程访问", description: "未来浏览器、手机、远程客户端访问 Core Service 的设置", items: ["监听地址", "访问令牌", "远程开关", "审计日志"] },
  ],
  providers: [
    { id: "openai", name: "OpenAI Compatible", status: "预览", enabledModels: "按本地配置" },
    { id: "openrouter", name: "OpenRouter", status: "预览", enabledModels: "按本地配置" },
    { id: "alibaba-cn", name: "Alibaba CN", status: "预览", enabledModels: "按本地配置" },
    { id: "local", name: "本地模型", status: "待接入", enabledModels: "0" },
  ],
  memories: [
    { id: "user-profile", title: "用户画像", category: "Profile", state: "等待真实 VMM 节点接入" },
    { id: "project-memory", title: "项目记忆", category: "Project", state: "等待真实 VMM 节点接入" },
    { id: "conversation-memory", title: "会话记忆", category: "Conversation", state: "等待真实 VMM 节点接入" },
    { id: "conflict-review", title: "冲突审计", category: "Review", state: "等待真实 VMM 冲突视图接入" },
  ],
  commandSessions: [
    { id: "preview-command", command: "pnpm --dir apps\\vulcan-code-web run build", status: "preview", preview: "前端预览命令会话" },
    { id: "preview-cargo", command: "cargo check -p vulcan-code-tauri", status: "preview", preview: "Tauri 桌面壳检查入口" },
  ],
  logs: [
    { id: "preview-log", level: "info", source: "webui", message: "浏览器预览模式已加载" },
    { id: "preview-legacy", level: "info", source: "iced", message: "旧 iced GUI 保留为 reference 入口" },
  ],
};

// AreaIconMap maps feature ids to lucide icons.
// AreaIconMap 将功能区域标识映射到 lucide 图标。
const areaIconMap: Record<MainAreaId, React.ReactNode> = {
  chat: <LayoutDashboard size={17} />,
  projects: <Boxes size={17} />,
  providers: <SlidersHorizontal size={17} />,
  tools: <Hammer size={17} />,
  plugins: <Wrench size={17} />,
  memory: <BrainCircuit size={17} />,
  settings: <Settings size={17} />,
  logs: <ScrollText size={17} />,
};

// resolveInvoke loads the Tauri invoke function when available.
// resolveInvoke 在可用时加载 Tauri invoke 函数。
async function resolveInvoke(): Promise<TauriInvoke | null> {
  if (!("__TAURI_INTERNALS__" in window)) {
    return null;
  }

  const api = await import("@tauri-apps/api/core");
  return api.invoke as TauriInvoke;
}

// resolveListen loads the Tauri event listener when available.
// resolveListen 在可用时加载 Tauri 事件监听器。
async function resolveListen(): Promise<TauriListen | null> {
  if (!("__TAURI_INTERNALS__" in window)) {
    return null;
  }

  const api = await import("@tauri-apps/api/event");
  return api.listen as TauriListen;
}

// loadStartupSnapshot loads startup state from Tauri or fallback data.
// loadStartupSnapshot 从 Tauri 或回退数据加载启动状态。
async function loadStartupSnapshot(): Promise<StartupSnapshot> {
  const invoke = await resolveInvoke();
  if (!invoke) {
    return browserFallbackStartup;
  }
  return invoke<StartupSnapshot>("service_startup_snapshot");
}

// loadWorkbenchSnapshot loads workbench state from Tauri or fallback data.
// loadWorkbenchSnapshot 从 Tauri 或回退数据加载工作台状态。
async function loadWorkbenchSnapshot(): Promise<WorkbenchSnapshot> {
  const invoke = await resolveInvoke();
  if (!invoke) {
    return browserFallbackWorkbench;
  }
  return invoke<WorkbenchSnapshot>("service_workbench_snapshot");
}

// loadConversationMessages loads one conversation body from the Tauri service.
// loadConversationMessages 从 Tauri 服务加载单个会话正文。
async function loadConversationMessages(sessionId: string): Promise<ConversationMessage[]> {
  const invoke = await resolveInvoke();
  if (!invoke) {
    return browserFallbackWorkbench.messages.filter((message) => message.conversationId === sessionId);
  }
  return invoke<ConversationMessage[]>("service_session_messages", { sessionId });
}

// openProjectDirectoryPath asks the desktop shell to reveal one filesystem folder.
// openProjectDirectoryPath 请求桌面壳打开单个文件系统文件夹。
async function openProjectDirectoryPath(path: string): Promise<string | null> {
  const invoke = await resolveInvoke();
  if (!invoke) {
    return "浏览器预览模式下无法直接打开资源管理器。";
  }
  return invoke<string>("service_open_project_directory_path", { path });
}

// openProjectDirectory asks the desktop shell to reveal one project folder.
// openProjectDirectory 请求桌面壳打开单个项目文件夹。
async function openProjectDirectory(projectId: string): Promise<string | null> {
  const invoke = await resolveInvoke();
  if (!invoke) {
    return "浏览器预览模式下无法直接打开资源管理器。";
  }
  return invoke<string>("service_open_project_directory", { projectId });
}

// toggleProjectPinned asks the desktop shell to toggle one project pinned state.
// toggleProjectPinned 请求桌面壳切换单个项目的置顶状态。
async function toggleProjectPinned(projectId: string): Promise<WorkbenchSnapshot | null> {
  const invoke = await resolveInvoke();
  if (!invoke) {
    return null;
  }
  return invoke<WorkbenchSnapshot>("service_toggle_project_pinned", { projectId });
}

// renameProject asks the desktop shell to apply a new project display name.
// renameProject 请求桌面壳应用新的项目显示名称。
async function renameProject(projectId: string, name: string): Promise<WorkbenchSnapshot | null> {
  const invoke = await resolveInvoke();
  if (!invoke) {
    return null;
  }
  return invoke<WorkbenchSnapshot>("service_rename_project", { projectId, name });
}

// archiveProjectConversations asks the desktop shell to archive all conversations under one project.
// archiveProjectConversations 请求桌面壳归档单个项目下的全部会话。
async function archiveProjectConversations(projectId: string): Promise<WorkbenchSnapshot | null> {
  const invoke = await resolveInvoke();
  if (!invoke) {
    return null;
  }
  return invoke<WorkbenchSnapshot>("service_archive_project_conversations", { projectId });
}

// renameConversation asks the desktop shell to apply a new conversation display name.
// renameConversation 请求桌面壳应用新的会话显示名称。
async function renameConversation(conversationId: string, name: string): Promise<WorkbenchSnapshot | null> {
  const invoke = await resolveInvoke();
  if (!invoke) {
    return null;
  }
  return invoke<WorkbenchSnapshot>("service_rename_conversation", { conversationId, name });
}

// archiveConversation asks the desktop shell to archive one conversation.
// archiveConversation 请求桌面壳归档单个会话。
async function archiveConversation(conversationId: string): Promise<WorkbenchSnapshot | null> {
  const invoke = await resolveInvoke();
  if (!invoke) {
    return null;
  }
  return invoke<WorkbenchSnapshot>("service_archive_conversation", { conversationId });
}

// App renders the Vulcan Code WebUI workbench.
// App 渲染 Vulcan Code WebUI 工作台。
function App() {
  // startup stores the latest startup snapshot.
  // startup 保存最近一次启动快照。
  const [startup, setStartup] = React.useState<StartupSnapshot | null>(null);
  // workbench stores the latest workbench snapshot.
  // workbench 保存最近一次工作台快照。
  const [workbench, setWorkbench] = React.useState<WorkbenchSnapshot | null>(null);
  // error stores the latest loading error.
  // error 保存最近一次加载错误。
  const [error, setError] = React.useState<string | null>(null);

  React.useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | null = null;

    // bootstrap loads the first startup and workbench snapshots.
    // bootstrap 加载第一份启动和工作台快照。
    async function bootstrap() {
      try {
        const listen = await resolveListen();
        if (listen) {
          unlisten = await listen<ServiceEvent>("vulcan://service-event", (event) => {
            if (event.payload.kind === "startup") {
              setStartup(event.payload.payload);
            }
            if (event.payload.kind === "workbench") {
              setWorkbench(event.payload.payload);
            }
          });
        }
        const [nextStartup, nextWorkbench] = await Promise.all([
          loadStartupSnapshot(),
          loadWorkbenchSnapshot(),
        ]);
        if (!cancelled) {
          setStartup(nextStartup);
          setWorkbench(nextWorkbench);
        }
      } catch (nextError) {
        if (!cancelled) {
          setError(nextError instanceof Error ? nextError.message : String(nextError));
        }
      }
    }

    void bootstrap();
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  if (error) {
    return <ErrorScreen error={error} />;
  }

  if (!startup || !workbench) {
    return <LoadingScreen />;
  }

  return (
    <TooltipProvider>
      <WorkbenchLayout startup={startup} workbench={workbench} />
      <Toaster />
    </TooltipProvider>
  );
}

// LoadingScreen renders the earliest frontend loading state.
// LoadingScreen 渲染最早期的前端加载状态。
function LoadingScreen() {
  return (
    <main className="loading-screen">
      <Activity className="spin" size={28} />
      <span>正在连接 Vulcan Core Service...</span>
    </main>
  );
}

// ErrorScreen renders a startup failure.
// ErrorScreen 渲染启动失败状态。
function ErrorScreen(props: { error: string }) {
  return (
    <main className="loading-screen error">
      <CircleAlert size={28} />
      <span>启动失败：{props.error}</span>
    </main>
  );
}

// WorkbenchLayout renders the GUI-like workbench shell.
// WorkbenchLayout 渲染接近 GUI 的工作台外壳。
function WorkbenchLayout(props: { startup: StartupSnapshot; workbench: WorkbenchSnapshot }) {
  // activeArea stores the currently selected feature area.
  // activeArea 保存当前选中的功能区域。
  const [activeArea, setActiveArea] = React.useState<MainAreaId>("chat");
  // theme stores the current shadcn visual theme.
  // theme 保存当前 shadcn 视觉主题。
  const [theme, setTheme] = React.useState<ThemeMode>(() => initialThemeMode());
  // selectedConversationId stores the conversation selected in the sidebar.
  // selectedConversationId 保存侧栏当前选中的会话。
  const [selectedConversationId, setSelectedConversationId] = React.useState<string | null>(
    props.workbench.selectedConversationId ?? props.workbench.conversations[0]?.id ?? null,
  );
  // draftConversation stores the local-only conversation opened before the first user message is sent.
  // draftConversation 保存发送第一条用户消息前打开的本地草稿会话。
  const [draftConversation, setDraftConversation] = React.useState<DraftConversationSelection | null>(null);
  // workbenchState stores local message hydration updates.
  // workbenchState 保存本地会话正文水合更新。
  const [workbenchState, setWorkbenchState] = React.useState<WorkbenchSnapshot>(props.workbench);
  // loadingConversationId stores the conversation currently loading from the service.
  // loadingConversationId 保存当前正在从服务加载正文的会话标识。
  const [loadingConversationId, setLoadingConversationId] = React.useState<string | null>(null);
  // conversationLoadError stores the latest conversation hydration error.
  // conversationLoadError 保存最近一次会话正文水合错误。
  const [conversationLoadError, setConversationLoadError] = React.useState<string | null>(null);
  // renameTarget stores the project or conversation currently being renamed.
  // renameTarget 保存当前正在重命名的项目或会话目标。
  const [renameTarget, setRenameTarget] = React.useState<RenameTarget | null>(null);
  // archiveProjectTarget stores the project waiting for archive confirmation.
  // archiveProjectTarget 保存正在等待归档确认的项目。
  const [archiveProjectTarget, setArchiveProjectTarget] = React.useState<ProjectSummary | null>(null);
  // pinnedConversationIds stores the locally pinned conversations until persistence is added.
  // pinnedConversationIds 保存本地置顶的会话，直到后续接入持久化。
  const [pinnedConversationIds, setPinnedConversationIds] = React.useState<Set<string>>(() => new Set());
  // hydratedConversationIds stores conversations already requested from the service.
  // hydratedConversationIds 保存已经向服务请求过正文的会话标识。
  const [hydratedConversationIds, setHydratedConversationIds] = React.useState<Set<string>>(
    () => new Set(props.workbench.messages.map((message) => message.conversationId)),
  );
  // activeFeature stores the selected feature summary.
  // activeFeature 保存当前选中的功能区域摘要。
  const activeFeature = workbenchState.featureAreas.find((area) => area.id === activeArea) ?? workbenchState.featureAreas[0];
  // selectedConversation stores the conversation used by the console header.
  // selectedConversation 保存控制台标题栏使用的当前会话。
  const selectedConversation = workbenchState.conversations.find((conversation) => conversation.id === selectedConversationId);
  // selectedDraftConversation stores the draft used by the console header and empty state.
  // selectedDraftConversation 保存控制台标题栏与空状态使用的草稿会话。
  const selectedDraftConversation = selectedConversationId === draftConversation?.id ? draftConversation : null;
  // showConsoleSidebar limits project and conversation context to the console view.
  // showConsoleSidebar 将项目与会话上下文限定在控制台页面内。
  const showConsoleSidebar = activeArea === "chat";

  // applyProjectActionSnapshot merges a project-menu operation result into the local workbench.
  // applyProjectActionSnapshot 将项目菜单操作结果合并进本地工作台。
  const applyProjectActionSnapshot = React.useCallback((snapshot: WorkbenchSnapshot | null) => {
    if (!snapshot) {
      return;
    }
    setWorkbenchState((current) => mergeWorkbenchSnapshot(current, snapshot));
    setDraftConversation(null);
    setSelectedConversationId((current) => {
      if (current && snapshot.conversations.some((conversation) => conversation.id === current)) {
        return current;
      }
      return snapshot.selectedConversationId ?? snapshot.conversations[0]?.id ?? null;
    });
  }, []);

  // handleProjectMenuAction executes immediate project-tree context menu commands.
  // handleProjectMenuAction 执行可立即处理的项目树右键菜单命令。
  const handleProjectMenuAction = React.useCallback(async (action: ProjectMenuAction, project: ProjectSummary) => {
    try {
      if (action === "open-directory") {
        const message = await openProjectDirectory(project.id);
        toast.success(message ?? `已请求打开项目目录：${project.name}`);
        return;
      }

      if (action === "rename") {
        setRenameTarget({ kind: "project", id: project.id, name: project.name });
        return;
      }

      if (action === "archive-conversations") {
        setArchiveProjectTarget(project);
        return;
      }

      const snapshot = await toggleProjectPinned(project.id);
      if (snapshot) {
        applyProjectActionSnapshot(snapshot);
        toast.success(project.pinned ? `已取消置顶项目：${project.name}` : `已置顶项目：${project.name}`);
      } else {
        const fallback = applyPreviewProjectAction(workbenchState, project.id, action);
        applyProjectActionSnapshot(fallback.snapshot);
        toast.success(fallback.message);
      }
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    }
  }, [applyProjectActionSnapshot, workbenchState]);

  // handleCreateConversation creates and selects a new empty conversation for a project.
  // handleCreateConversation 为项目创建并选中一个新的空白会话。
  const handleCreateConversation = React.useCallback(async (project: ProjectSummary) => {
    const draftId = `draft-${project.id}-${Date.now()}`;
    setDraftConversation({
      id: draftId,
      projectId: project.id,
      projectName: project.name,
    });
    setSelectedConversationId(draftId);
    setConversationLoadError(null);
    setLoadingConversationId(null);
  }, []);

  // handleConversationMenuAction executes conversation-tree context menu commands.
  // handleConversationMenuAction 执行会话树右键菜单命令。
  const handleConversationMenuAction = React.useCallback(async (action: ConversationMenuAction, conversation: ConversationSummary) => {
    try {
      if (action === "rename") {
        setRenameTarget({ kind: "conversation", id: conversation.id, name: conversation.title });
        return;
      }
      if (action === "pin") {
        setPinnedConversationIds((current) => {
          const next = new Set(current);
          if (next.has(conversation.id)) {
            next.delete(conversation.id);
            toast.success(`已取消置顶对话：${conversation.title}`);
          } else {
            next.add(conversation.id);
            toast.success(`已置顶对话：${conversation.title}`);
          }
          return next;
        });
        return;
      }
      if (action === "archive") {
        const snapshot = await archiveConversation(conversation.id);
        if (snapshot) {
          applyProjectActionSnapshot(snapshot);
        } else {
          applyProjectActionSnapshot(applyPreviewConversationAction(workbenchState, conversation.id, action).snapshot);
        }
        toast.success(`已归档对话：${conversation.title}`);
        return;
      }
      if (action === "open-directory") {
        const project = projectForConversation(workbenchState, conversation);
        if (!project) {
          toast.error("没有找到这个对话所属的项目。");
          return;
        }
        const message = await openProjectDirectoryPath(project.path);
        toast.success(message ?? `已请求打开项目目录：${project.name}`);
        return;
      }
      if (action === "copy-workdir") {
        const project = projectForConversation(workbenchState, conversation);
        if (!project) {
          toast.error("没有找到这个对话所属的项目。");
          return;
        }
        await navigator.clipboard.writeText(project.path);
        toast.success("已复制工作目录。");
        return;
      }
      if (action === "copy-session-id") {
        await navigator.clipboard.writeText(conversation.id);
        toast.success("已复制会话 ID。");
        return;
      }
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    }
  }, [applyProjectActionSnapshot, workbenchState]);

  // handleRenameConfirm commits the current rename dialog value.
  // handleRenameConfirm 提交当前重命名对话框值。
  const handleRenameConfirm = React.useCallback(async (target: RenameTarget, name: string) => {
    const normalizedName = name.trim();
    if (!normalizedName) {
      toast.error("名称不能为空。");
      return;
    }
    try {
      const snapshot = target.kind === "project"
        ? await renameProject(target.id, normalizedName)
        : await renameConversation(target.id, normalizedName);
      if (snapshot) {
        applyProjectActionSnapshot(snapshot);
      } else if (target.kind === "project") {
        applyProjectActionSnapshot(applyPreviewProjectRename(workbenchState, target.id, normalizedName).snapshot);
      } else {
        applyProjectActionSnapshot(applyPreviewConversationRename(workbenchState, target.id, normalizedName).snapshot);
      }
      setRenameTarget(null);
      toast.success(target.kind === "project" ? `已重命名项目：${normalizedName}` : `已重命名对话：${normalizedName}`);
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    }
  }, [applyProjectActionSnapshot, workbenchState]);

  // handleArchiveProjectConfirm archives all conversations under the selected project.
  // handleArchiveProjectConfirm 归档选定项目下的全部对话。
  const handleArchiveProjectConfirm = React.useCallback(async () => {
    if (!archiveProjectTarget) {
      return;
    }
    try {
      const snapshot = await archiveProjectConversations(archiveProjectTarget.id);
      if (snapshot) {
        applyProjectActionSnapshot(snapshot);
      } else {
        applyProjectActionSnapshot(applyPreviewProjectAction(workbenchState, archiveProjectTarget.id, "archive-conversations").snapshot);
      }
      toast.success(`已归档 ${projectConversationCount(workbenchState, archiveProjectTarget.id)} 个对话。`);
      setArchiveProjectTarget(null);
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    }
  }, [applyProjectActionSnapshot, archiveProjectTarget, workbenchState]);

  React.useEffect(() => {
    document.documentElement.classList.toggle("dark", theme === "dark");
    document.documentElement.style.colorScheme = theme;
    window.localStorage.setItem("vulcan-webui-theme", theme);
  }, [theme]);

  React.useEffect(() => {
    setWorkbenchState((current) => mergeWorkbenchSnapshot(current, props.workbench));
    const nextSelected = props.workbench.selectedConversationId ?? props.workbench.conversations[0]?.id ?? null;
    setSelectedConversationId((current) => {
      if (current && current === draftConversation?.id) {
        return current;
      }
      if (current && props.workbench.conversations.some((conversation) => conversation.id === current)) {
        return current;
      }
      return nextSelected;
    });
  }, [draftConversation?.id, props.workbench]);

  React.useEffect(() => {
    if (!selectedConversationId) {
      return;
    }
    if (selectedConversationId === draftConversation?.id) {
      return;
    }
    const conversationId = selectedConversationId;
    if (hydratedConversationIds.has(conversationId)) {
      return;
    }

    let cancelled = false;
    setLoadingConversationId(conversationId);
    setConversationLoadError(null);

    // hydrateConversation loads the selected conversation without reloading the whole workbench.
    // hydrateConversation 在不重载整个工作台的情况下加载当前选中会话正文。
    async function hydrateConversation() {
      try {
        const messages = await loadConversationMessages(conversationId);
        if (cancelled) {
          return;
        }
        setWorkbenchState((current) => mergeConversationMessages(current, conversationId, messages));
        setHydratedConversationIds((current) => new Set(current).add(conversationId));
      } catch (nextError) {
        if (!cancelled) {
          setConversationLoadError(nextError instanceof Error ? nextError.message : String(nextError));
        }
      } finally {
        if (!cancelled) {
          setLoadingConversationId(null);
        }
      }
    }

    void hydrateConversation();
    return () => {
      cancelled = true;
    };
  }, [draftConversation?.id, hydratedConversationIds, selectedConversationId]);

  return (
    <div className={`app-shell ${showConsoleSidebar ? "with-console-sidebar" : "without-console-sidebar"}`}>
      <header className="main-menu" aria-label="主菜单">
        <BrandHeader />
        <FeatureNav
          areas={workbenchState.featureAreas}
          activeArea={activeArea}
          onSelect={setActiveArea}
        />
        <GlobalActions
          startup={props.startup}
          theme={theme}
          onToggleTheme={() => setTheme((current) => current === "dark" ? "light" : "dark")}
          onOpenSettings={() => setActiveArea("settings")}
        />
      </header>
      <main className="workspace">
        {showConsoleSidebar ? (
          <ConsoleShell
            startup={props.startup}
            workbench={workbenchState}
            selectedDraftConversation={selectedDraftConversation}
            selectedConversation={selectedConversation}
            selectedConversationId={selectedConversationId}
            loadingConversationId={loadingConversationId}
            conversationLoadError={conversationLoadError}
            pinnedConversationIds={pinnedConversationIds}
            onConversationMenuAction={handleConversationMenuAction}
            onCreateConversation={handleCreateConversation}
            onProjectMenuAction={handleProjectMenuAction}
            onSelectConversation={setSelectedConversationId}
          />
        ) : (
          <>
            <Topbar
              activeFeature={activeFeature}
            />
            <AreaContent
              activeArea={activeArea}
              startup={props.startup}
              workbench={workbenchState}
              selectedConversationId={selectedConversationId}
              loadingConversationId={loadingConversationId}
              conversationLoadError={conversationLoadError}
            />
          </>
        )}
      </main>
      <footer className="statusbar">
        <span>版本 {props.workbench.appVersion}</span>
        <span>Core Service: {props.startup.phase}</span>
        <span>旧 iced GUI 保留为 reference</span>
      </footer>
      <RenameDialog
        target={renameTarget}
        onCancel={() => setRenameTarget(null)}
        onConfirm={handleRenameConfirm}
      />
      <ArchiveProjectDialog
        conversationCount={archiveProjectTarget ? projectConversationCount(workbenchState, archiveProjectTarget.id) : 0}
        project={archiveProjectTarget}
        onCancel={() => setArchiveProjectTarget(null)}
        onConfirm={handleArchiveProjectConfirm}
      />
    </div>
  );
}

// initialThemeMode returns the persisted shadcn theme, defaulting to dark for continuity.
// initialThemeMode 返回持久化的 shadcn 主题，默认使用暗色以保持连续性。
function initialThemeMode(): ThemeMode {
  const stored = window.localStorage.getItem("vulcan-webui-theme");
  return stored === "light" || stored === "dark" ? stored : "dark";
}

// mergeWorkbenchSnapshot merges backend workbench updates without discarding lazily loaded messages.
// mergeWorkbenchSnapshot 合并后端工作台更新且不丢弃已懒加载的会话正文。
function mergeWorkbenchSnapshot(current: WorkbenchSnapshot, incoming: WorkbenchSnapshot): WorkbenchSnapshot {
  const incomingIds = new Set(incoming.messages.map((message) => message.id));
  const retainedMessages = current.messages.filter((message) => !incomingIds.has(message.id));
  return {
    ...incoming,
    messages: [...retainedMessages, ...incoming.messages],
  };
}

// mergeConversationMessages replaces one conversation message window in the local workbench state.
// mergeConversationMessages 替换本地工作台状态中单个会话的消息窗口。
function mergeConversationMessages(
  workbench: WorkbenchSnapshot,
  conversationId: string,
  messages: ConversationMessage[],
): WorkbenchSnapshot {
  return {
    ...workbench,
    messages: [
      ...workbench.messages.filter((message) => message.conversationId !== conversationId),
      ...messages,
    ],
  };
}

// ProjectMenuAction names project-tree context menu commands.
// ProjectMenuAction 命名项目树右键菜单命令。
type ProjectMenuAction = "toggle-pin" | "open-directory" | "rename" | "archive-conversations";

// ConversationMenuAction names conversation-tree context menu commands.
// ConversationMenuAction 命名会话树右键菜单命令。
type ConversationMenuAction =
  | "pin"
  | "rename"
  | "archive"
  | "open-directory"
  | "copy-workdir"
  | "copy-session-id";

// RenameTarget describes one pending rename dialog target.
// RenameTarget 描述一个待处理的重命名对话框目标。
interface RenameTarget {
  // Target kind used to route the final rename command.
  // 用于路由最终重命名命令的目标类型。
  kind: "project" | "conversation";
  // Stable target identifier.
  // 稳定目标标识。
  id: string;
  // Current display name.
  // 当前显示名称。
  name: string;
}

// applyPreviewProjectAction simulates project-menu commands for browser-only preview mode.
// applyPreviewProjectAction 为纯浏览器预览模式模拟项目菜单命令。
function applyPreviewProjectAction(
  workbench: WorkbenchSnapshot,
  projectId: string,
  action: ProjectMenuAction,
): { snapshot: WorkbenchSnapshot; message: string } {
  const project = workbench.projects.find((candidate) => candidate.id === projectId);
  if (!project) {
    return { snapshot: workbench, message: "没有找到对应项目。" };
  }

  if (action === "toggle-pin") {
    const pinned = !project.pinned;
    return {
      snapshot: {
        ...workbench,
        projects: workbench.projects.map((candidate) => candidate.id === projectId ? { ...candidate, pinned } : candidate),
      },
      message: pinned ? `已将项目 ${project.name} 置顶。` : `已取消项目 ${project.name} 的置顶状态。`,
    };
  }

  const archivedConversationIds = new Set(
    workbench.conversations
      .filter((conversation) => conversation.projectId === projectId)
      .map((conversation) => conversation.id),
  );
  const conversations = workbench.conversations.filter((conversation) => !archivedConversationIds.has(conversation.id));
  const selectedConversationId = archivedConversationIds.has(workbench.selectedConversationId ?? "")
    ? conversations[0]?.id ?? null
    : workbench.selectedConversationId;

  return {
    snapshot: {
      ...workbench,
      conversations,
      selectedConversationId,
      messages: workbench.messages.filter((message) => !archivedConversationIds.has(message.conversationId)),
    },
    message: `已归档项目 ${project.name} 下的对话。`,
  };
}

// applyPreviewProjectRename simulates project rename for browser-only preview mode.
// applyPreviewProjectRename 为纯浏览器预览模式模拟项目重命名。
function applyPreviewProjectRename(
  workbench: WorkbenchSnapshot,
  projectId: string,
  name: string,
): { snapshot: WorkbenchSnapshot } {
  return {
    snapshot: {
      ...workbench,
      projects: workbench.projects.map((candidate) => candidate.id === projectId ? { ...candidate, name } : candidate),
    },
  };
}

// applyPreviewConversationRename simulates conversation rename for browser-only preview mode.
// applyPreviewConversationRename 为纯浏览器预览模式模拟会话重命名。
function applyPreviewConversationRename(
  workbench: WorkbenchSnapshot,
  conversationId: string,
  title: string,
): { snapshot: WorkbenchSnapshot } {
  return {
    snapshot: {
      ...workbench,
      conversations: workbench.conversations.map((conversation) => (
        conversation.id === conversationId ? { ...conversation, title } : conversation
      )),
    },
  };
}

// applyPreviewConversationAction simulates conversation-menu commands for browser-only preview mode.
// applyPreviewConversationAction 为纯浏览器预览模式模拟会话菜单命令。
function applyPreviewConversationAction(
  workbench: WorkbenchSnapshot,
  conversationId: string,
  action: ConversationMenuAction,
): { snapshot: WorkbenchSnapshot } {
  if (action !== "archive") {
    return { snapshot: workbench };
  }
  const conversations = workbench.conversations.filter((conversation) => conversation.id !== conversationId);
  const selectedConversationId = workbench.selectedConversationId === conversationId
    ? conversations[0]?.id ?? null
    : workbench.selectedConversationId;
  return {
    snapshot: {
      ...workbench,
      conversations,
      selectedConversationId,
      messages: workbench.messages.filter((message) => message.conversationId !== conversationId),
    },
  };
}

// projectForConversation returns the project that owns one conversation.
// projectForConversation 返回拥有某个会话的项目。
function projectForConversation(
  workbench: WorkbenchSnapshot,
  conversation: ConversationSummary,
): ProjectSummary | null {
  const fallbackProject = workbench.projects[0] ?? null;
  return workbench.projects.find((project) => project.id === conversation.projectId) ?? fallbackProject;
}

// projectConversationCount returns the visible conversation count under one project.
// projectConversationCount 返回单个项目下的可见会话数量。
function projectConversationCount(workbench: WorkbenchSnapshot, projectId: string): number {
  return workbench.conversations.filter((conversation) => conversation.projectId === projectId).length;
}

// markdownPlugins stores the Markdown extension set shared by inline and modal renderers.
// markdownPlugins 保存内联与弹窗渲染器共享的 Markdown 扩展集合。
const markdownPlugins = [remarkGfm];

// markdownSanitizers stores the HTML sanitization pipeline for untrusted transcript content.
// markdownSanitizers 保存用于不可信转录内容的 HTML 清理管线。
const markdownSanitizers = [rehypeSanitize];

// directiveKinds maps Vulcan directive names to WebUI conversation block kinds.
// directiveKinds 将 Vulcan 指令名称映射为 WebUI 会话块类型。
const directiveKinds: Record<string, ConversationBlockKind> = {
  "tool-call": "tool-call",
  "tool-result": "tool-result",
  "turn-stats": "turn-stats",
  thinking: "thinking",
  diff: "diff",
  notice: "notice",
};

// detailEligibleBlockKinds lists block types that should open a dedicated detail modal.
// detailEligibleBlockKinds 列出应打开专用详情弹窗的块类型。
const detailEligibleBlockKinds = new Set<ConversationBlockKind>(["tool-call", "tool-result", "command", "diff", "notice"]);

// projectConversationRenderItems merges related low-level transcript rows for display.
// projectConversationRenderItems 合并相关底层转录行以生成最终展示项。
function projectConversationRenderItems(messages: ConversationMessage[]): ConversationRenderItem[] {
  const items: ConversationRenderItem[] = [];
  const pendingToolItems = new Map<string, ConversationRenderItem>();

  for (const message of messages) {
    if (message.role === "user") {
      items.push({
        id: message.id,
        role: "user",
        blocks: [],
        userMessage: message,
      });
      continue;
    }

    const blocks = parseConversationBlocks(message);
    for (const block of blocks) {
      if (isMergeableToolBlock(block)) {
        const key = toolMergeKey(block);
        const existing = key ? pendingToolItems.get(key) : undefined;
        if (existing) {
          existing.blocks = mergeToolBlocks(existing.blocks, block);
          continue;
        }

        const item: ConversationRenderItem = {
          id: block.id,
          role: message.role === "system" ? "system" : "assistant",
          blocks: [block],
        };
        items.push(item);
        if (key) {
          pendingToolItems.set(key, item);
        }
        continue;
      }

      items.push({
        id: block.id,
        role: message.role === "system" ? "system" : "assistant",
        blocks: [block],
      });
    }
  }

  return items;
}

// isMergeableToolBlock returns whether a block belongs to one tool execution row.
// isMergeableToolBlock 返回某个块是否属于同一工具执行行。
function isMergeableToolBlock(block: ConversationBlock): boolean {
  return block.kind === "tool-call" || block.kind === "tool-result" || block.kind === "command";
}

// toolMergeKey resolves the grouping key used for call/result merging.
// toolMergeKey 解析调用与返回合并使用的分组键。
function toolMergeKey(block: ConversationBlock): string | null {
  return block.toolCallId ?? block.commandSessionId ?? block.toolName ?? null;
}

// mergeToolBlocks merges one tool result into an existing tool execution row.
// mergeToolBlocks 将一个工具返回合并到已有工具执行行中。
function mergeToolBlocks(blocks: ConversationBlock[], incoming: ConversationBlock): ConversationBlock[] {
  const withoutSameKind = blocks.filter((block) => block.kind !== incoming.kind);
  return [...withoutSameKind, incoming].sort((left, right) => toolBlockOrder(left.kind) - toolBlockOrder(right.kind));
}

// toolBlockOrder returns stable order for call/result display and detail assembly.
// toolBlockOrder 返回调用与返回展示及详情组装的稳定顺序。
function toolBlockOrder(kind: ConversationBlockKind): number {
  if (kind === "tool-call") return 0;
  if (kind === "command") return 1;
  if (kind === "tool-result") return 2;
  return 3;
}

// parseConversationBlocks converts legacy transcript text into renderable WebUI blocks.
// parseConversationBlocks 将旧转录文本转换为可渲染的 WebUI 块。
function parseConversationBlocks(message: ConversationMessage): ConversationBlock[] {
  if (message.role === "user") {
    return [conversationMarkdownBlock(message.id, 0, message.body)];
  }

  const structuredKind = structuredMessageBlockKind(message);
  if (structuredKind) {
    return [conversationSpecialBlock(message.id, 0, structuredKind, message.body, structuredMessageAttributes(message))];
  }

  const source = message.body.replace(/\r\n/g, "\n");
  const directiveBlocks = parseDirectiveConversationBlocks(message.id, source);
  if (directiveBlocks.some((block) => block.kind !== "markdown")) {
    return directiveBlocks;
  }

  return parseFallbackConversationBlocks(message.id, source);
}

// structuredMessageBlockKind maps service-provided record kinds to WebUI block kinds.
// structuredMessageBlockKind 将服务提供的记录类型映射到 WebUI 块类型。
function structuredMessageBlockKind(message: ConversationMessage): ConversationBlockKind | null {
  if (message.recordKind === "tool-call-output") return "tool-call";
  if (message.recordKind === "tool-result-output") {
    const hostKind = metadataString(message.metadata, ["kind", "host_result.kind", "hostResult.kind"]);
    if (hostKind === "command_session") return "command";
    if (hostKind === "change_set") return "diff";
    return "tool-result";
  }
  if (message.recordKind === "thinking-output") return "thinking";
  if (message.recordKind === "turn-stats") return "turn-stats";
  if (message.recordKind === "compaction-status") return "notice";
  return null;
}

// structuredMessageAttributes extracts render metadata from the service payload.
// structuredMessageAttributes 从服务载荷中提取渲染元数据。
function structuredMessageAttributes(message: ConversationMessage): Record<string, string> {
  const attributes: Record<string, string> = {};
  const toolName = metadataString(message.metadata, [
    "tool_name",
    "toolName",
    "host_result.tool_name",
    "hostResult.toolName",
    "payload.tool_name",
    "payload.toolName",
    "tool.name",
    "name",
  ]) ?? extractToolNameFromText(message.title ?? "");
  const toolCallId = metadataString(message.metadata, [
    "tool_call_id",
    "toolCallId",
    "host_result.tool_call_id",
    "hostResult.toolCallId",
    "payload.tool_call_id",
    "payload.toolCallId",
  ]);
  const toolResultId = metadataString(message.metadata, [
    "tool_result_id",
    "toolResultId",
    "host_result.tool_result_id",
    "hostResult.toolResultId",
    "payload.tool_result_id",
    "payload.toolResultId",
  ]);
  const commandSessionId = metadataString(message.metadata, [
    "command_session_id",
    "commandSessionId",
    "host_result.command_session_id",
    "hostResult.commandSessionId",
    "payload.command_session_id",
    "payload.commandSessionId",
  ]);
  const status = metadataToolFailureStatus(message.metadata)
    ?? metadataString(message.metadata, [
      "tool_display_state",
      "toolDisplayState",
      "display_state",
      "displayState",
      "status",
      "state",
      "payload.status",
      "host_result.status",
      "hostResult.status",
    ])
    ?? turnStatsStatus(message.metadata);
  const summary = turnStatsSummary(message.metadata);
  if (message.title) attributes.title = message.title;
  if (message.recordKind) attributes.record_kind = message.recordKind;
  if (toolName) attributes.tool_name = toolName;
  if (toolCallId) attributes.tool_call_id = toolCallId;
  if (toolResultId) attributes.tool_result_id = toolResultId;
  if (commandSessionId) attributes.command_session_id = commandSessionId;
  if (status) attributes.status = status;
  if (summary) attributes.summary = summary;
  return attributes;
}

// metadataString reads a dotted path string from unknown JSON metadata.
// metadataString 从未知 JSON 元数据中读取点分路径字符串。
function metadataString(metadata: unknown, paths: string[]): string | null {
  for (const path of paths) {
    const value = path.split(".").reduce<unknown>((current, segment) => {
      if (current && typeof current === "object" && segment in current) {
        return (current as Record<string, unknown>)[segment];
      }
      return undefined;
    }, metadata);
    if (typeof value === "string" && value.trim()) {
      return value.trim();
    }
  }
  return null;
}

// metadataToolFailureStatus detects host-side tool failures that are stored as structured metadata.
// metadataToolFailureStatus 识别以结构化元数据保存的宿主侧工具失败状态。
function metadataToolFailureStatus(metadata: unknown): string | null {
  const hostKind = metadataString(metadata, ["host_result.kind", "hostResult.kind", "kind"]);
  if (hostKind === "tool_error") {
    return "failed";
  }
  const payloadStatus = metadataString(metadata, ["payload.status", "status"]);
  return payloadStatus === "failed" || payloadStatus === "error" ? "failed" : null;
}

// turnStatsObject reads the structured turn-stats payload from transcript metadata.
// turnStatsObject 从转录元数据中读取结构化轮次统计载荷。
function turnStatsObject(metadata: unknown): Record<string, unknown> | null {
  if (!metadata || typeof metadata !== "object") {
    return null;
  }
  const value = (metadata as Record<string, unknown>).turn_stats ?? (metadata as Record<string, unknown>).turnStats;
  return value && typeof value === "object" ? value as Record<string, unknown> : null;
}

// turnStatsStatus reads the compact turn-stats terminal status.
// turnStatsStatus 读取紧凑轮次统计终态。
function turnStatsStatus(metadata: unknown): string | null {
  const stats = turnStatsObject(metadata);
  const status = stats?.status;
  return typeof status === "string" && status.trim() ? status.trim() : null;
}

// turnStatsSummary builds the same compact summary line used by the legacy GUI.
// turnStatsSummary 构建与旧 GUI 一致的紧凑摘要行。
function turnStatsSummary(metadata: unknown): string | null {
  const stats = turnStatsObject(metadata);
  if (!stats) {
    return null;
  }
  const parts = [
    `${numberMetadata(stats.provider_call_count ?? stats.providerCallCount)} 次调用`,
    `${numberMetadata(stats.tool_call_count ?? stats.toolCallCount)} 个工具`,
  ];
  const totalTokens = numberMetadata(stats.total_tokens ?? stats.totalTokens);
  if (totalTokens > 0) {
    parts.push(`${totalTokens} tokens`);
  }
  const cachedTokens = numberMetadata(stats.cached_input_tokens ?? stats.cachedInputTokens);
  if (cachedTokens > 0) {
    parts.push(`缓存命中 ${cachedTokens}`);
  }
  const speed = numericMetadata(stats.output_tokens_per_second ?? stats.outputTokensPerSecond);
  if (speed && Number.isFinite(speed) && speed > 0) {
    parts.push(`${speed.toFixed(1)}/s`);
  }
  return parts.join(" · ");
}

// numberMetadata reads a safe integer from unknown JSON values.
// numberMetadata 从未知 JSON 值中读取安全整数。
function numberMetadata(value: unknown): number {
  const numberValue = numericMetadata(value);
  return numberValue && Number.isFinite(numberValue) ? Math.max(0, Math.trunc(numberValue)) : 0;
}

// numericMetadata reads a number from unknown JSON values.
// numericMetadata 从未知 JSON 值中读取数字。
function numericMetadata(value: unknown): number | null {
  if (typeof value === "number") {
    return value;
  }
  if (typeof value === "string" && value.trim()) {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : null;
  }
  return null;
}

// parseDirectiveConversationBlocks parses Vulcan triple-colon transcript directives.
// parseDirectiveConversationBlocks 解析 Vulcan 三冒号转录指令。
function parseDirectiveConversationBlocks(messageId: string, source: string): ConversationBlock[] {
  const blocks: ConversationBlock[] = [];
  const pattern = /^:::(\S+)([^\n]*)\n([\s\S]*?)\n:::\s*$/gm;
  let lastIndex = 0;
  let blockIndex = 0;
  let match: RegExpExecArray | null;

  while ((match = pattern.exec(source)) !== null) {
    const before = source.slice(lastIndex, match.index);
    if (before.trim()) {
      blockIndex = pushFallbackSegments(blocks, messageId, blockIndex, before.trim());
    }

    const directiveName = match[1] ?? "notice";
    const attributes = parseDirectiveAttributes(match[2] ?? "");
    const body = match[3] ?? "";
    const kind = directiveKinds[directiveName] ?? "notice";
    blocks.push(conversationSpecialBlock(messageId, blockIndex, kind, body, attributes));
    blockIndex += 1;
    lastIndex = pattern.lastIndex;
  }

  const after = source.slice(lastIndex);
  if (after.trim()) {
    pushFallbackSegments(blocks, messageId, blockIndex, after.trim());
  }

  return blocks;
}

// pushFallbackSegments appends Markdown and diff segments while preserving block indexes.
// pushFallbackSegments 追加 Markdown 与 diff 片段并保持块索引连续。
function pushFallbackSegments(
  blocks: ConversationBlock[],
  messageId: string,
  startIndex: number,
  source: string,
): number {
  let nextIndex = startIndex;
  for (const segment of splitFencedDiffSegments(source)) {
    if (!segment.body.trim()) {
      continue;
    }
    if (segment.kind === "diff") {
      blocks.push(conversationSpecialBlock(messageId, nextIndex, "diff", segment.body, {}));
    } else {
      blocks.push(conversationMarkdownBlock(messageId, nextIndex, segment.body));
    }
    nextIndex += 1;
  }
  return nextIndex;
}

// parseDirectiveAttributes parses loose key=value attributes from one directive header.
// parseDirectiveAttributes 从单条指令头中解析宽松的 key=value 属性。
function parseDirectiveAttributes(source: string): Record<string, string> {
  const attributes: Record<string, string> = {};
  const tokens = source.trim().match(/(?:[^\s"']+|"[^"]*"|'[^']*')+/g) ?? [];
  for (const token of tokens) {
    const [rawKey, ...rawValueParts] = token.split("=");
    const key = rawKey.trim();
    if (!key) {
      continue;
    }
    const value = rawValueParts.join("=").trim().replace(/^["']|["']$/g, "");
    attributes[key] = value || "true";
  }
  return attributes;
}

// parseFallbackConversationBlocks detects common legacy tool, command, and diff formats.
// parseFallbackConversationBlocks 识别常见旧版工具、命令与 diff 格式。
function parseFallbackConversationBlocks(messageId: string, source: string): ConversationBlock[] {
  const segments = splitFencedDiffSegments(source);
  if (segments.some((segment) => segment.kind === "diff")) {
    return segments.map((segment, index) => {
      if (segment.kind === "diff") {
        return conversationSpecialBlock(messageId, index, "diff", segment.body, {});
      }
      return conversationMarkdownBlock(messageId, index, segment.body);
    });
  }

  const toolKind = detectToolBlockKind(source);
  if (toolKind) {
    return [conversationSpecialBlock(messageId, 0, toolKind, source, detectToolAttributes(source))];
  }

  return [conversationMarkdownBlock(messageId, 0, source)];
}

// splitFencedDiffSegments separates Markdown around diff fences and raw git patches.
// splitFencedDiffSegments 将 diff 围栏与原始 git patch 周围的 Markdown 分离。
function splitFencedDiffSegments(source: string): Array<{ kind: "markdown" | "diff"; body: string }> {
  const segments: Array<{ kind: "markdown" | "diff"; body: string }> = [];
  const pattern = /```(?:diff|patch)\n([\s\S]*?)```|(^diff --git [\s\S]*?)(?=\n\n(?![+\- @\\])|\n#{1,6}\s|\n:::\S+|\s*$)/gm;
  let lastIndex = 0;
  let match: RegExpExecArray | null;

  while ((match = pattern.exec(source)) !== null) {
    const before = source.slice(lastIndex, match.index);
    if (before.trim()) {
      segments.push({ kind: "markdown", body: before.trim() });
    }
    segments.push({ kind: "diff", body: (match[1] ?? match[2] ?? "").trim() });
    lastIndex = pattern.lastIndex;
  }

  const after = source.slice(lastIndex);
  if (after.trim()) {
    segments.push({ kind: "markdown", body: after.trim() });
  }

  return segments.length > 0 ? segments : [{ kind: "markdown", body: source }];
}

// detectToolBlockKind classifies legacy text-only tool and runtime blocks.
// detectToolBlockKind 对旧版纯文本工具与运行时块进行分类。
function detectToolBlockKind(source: string): ConversationBlockKind | null {
  const normalized = source.toLowerCase();
  if (normalized.includes("turn stats") || normalized.includes("provider 调用") || normalized.includes("总 tokens")) {
    return "turn-stats";
  }
  if (source.includes("## Tool Call") || normalized.includes("tool_call_id") || normalized.includes("tool call")) {
    return "tool-call";
  }
  if (source.includes("## Tool Result") || normalized.includes("tool_result_id") || normalized.includes("tool result")) {
    return "tool-result";
  }
  if (hasLegacyCommandBlockMarker(source)) {
    return "command";
  }
  if (hasLegacyThinkingBlockMarker(source)) {
    return "thinking";
  }
  return null;
}

// hasLegacyCommandBlockMarker detects explicit command-session transcript blocks without matching ordinary tool lists.
// hasLegacyCommandBlockMarker 检测明确的命令会话转录块，避免误匹配普通工具清单。
function hasLegacyCommandBlockMarker(source: string): boolean {
  const normalized = source.toLowerCase();
  return normalized.includes("command_session_id")
    || /^#{1,6}\s*(command session|命令会话)\b/im.test(source)
    || /^(command session|命令会话)\s*[:：]/im.test(source)
    || /(?:类型|kind|type)\s*[:：=]\s*(?:command_session|command session|命令会话)/i.test(source);
}

// hasLegacyThinkingBlockMarker detects explicit thinking transcript blocks without matching normal prose.
// hasLegacyThinkingBlockMarker 检测明确的思考转录块，避免误匹配普通正文。
function hasLegacyThinkingBlockMarker(source: string): boolean {
  return /^#{1,6}\s*(thinking|思考过程|思考)\b/im.test(source)
    || /^(thinking|思考过程|思考)\s*[:：]/im.test(source)
    || /(?:类型|kind|type)\s*[:：=]\s*(?:thinking|思考过程|思考)/i.test(source);
}

// detectToolAttributes extracts compact metadata from legacy tool text.
// detectToolAttributes 从旧版工具文本中提取紧凑元数据。
function detectToolAttributes(source: string): Record<string, string> {
  const attributes: Record<string, string> = {};
  const toolName = extractToolNameFromText(source);
  const toolCallId = source.match(/tool_call_id["'`:\s=]+([A-Za-z0-9_.:-]+)/i)?.[1];
  const toolResultId = source.match(/tool_result_id["'`:\s=]+([A-Za-z0-9_.:-]+)/i)?.[1];
  const commandSessionId = source.match(/command_session_id["'`:\s=]+([A-Za-z0-9_.:-]+)/i)?.[1];
  const status = extractToolStatusFromText(source);
  if (toolName) attributes.tool_name = toolName;
  if (toolCallId) attributes.tool_call_id = toolCallId;
  if (toolResultId) attributes.tool_result_id = toolResultId;
  if (commandSessionId) attributes.command_session_id = commandSessionId;
  if (status) attributes.status = status;
  return attributes;
}

// extractToolNameFromText recovers a tool name from legacy localized titles and Markdown bodies.
// extractToolNameFromText 从旧版本地化标题与 Markdown 正文中恢复工具名称。
function extractToolNameFromText(source: string): string | null {
  const normalized = source.replace(/\r\n/g, "\n");
  const patterns = [
    /##\s*Tool Call\s*\n+\s*`([^`]+)`/im,
    /##\s*Tool Result\s*\n+\s*`([^`]+)`/im,
    /tool_name["'`:\s=]+([A-Za-z0-9_.:-]+)/i,
    /"tool_name"\s*:\s*"([^"]+)"/i,
    /"toolName"\s*:\s*"([^"]+)"/i,
    /工具调用\s*[·:：]\s*[A-Za-z0-9_.:-]+\s*[·:：]\s*([A-Za-z0-9_.:-]+)/,
    /(?:工具调用|工具返回)\s*[·:：]?\s*([A-Za-z0-9_.:-]+)/,
    /调用\s+([A-Za-z0-9_.:-]+)\s+工具/,
    /Tool (?:Call|Result)\s+([A-Za-z0-9_.:-]+)/i,
    /Call\s+([A-Za-z0-9_.:-]+)/i,
  ];
  for (const pattern of patterns) {
    const value = normalized.match(pattern)?.[1]?.trim();
    if (value && !genericToolNameLabels.has(value)) {
      return value;
    }
  }
  return null;
}

// extractToolStatusFromText recovers a display state from legacy tool output text.
// extractToolStatusFromText 从旧版工具输出文本中恢复显示状态。
function extractToolStatusFromText(source: string): string | null {
  const normalized = source.toLowerCase();
  if (/失败|报错|错误|failed|error|panic|exit code\s*-\d|退出码\s*-\d/i.test(source)) {
    return "failed";
  }
  if (/已完成|完成|completed|success|succeeded/i.test(source)) {
    return "completed";
  }
  if (/运行中|进行中|running|pending|in progress/i.test(normalized)) {
    return "running";
  }
  return null;
}

// genericToolNameLabels lists localized labels that are headings rather than actual tool names.
// genericToolNameLabels 列出只是标题而不是实际工具名的本地化标签。
const genericToolNameLabels = new Set(["工具调用", "工具返回", "Tool Call", "Tool Result", "Raw Tool Result"]);

// conversationMarkdownBlock creates a standard Markdown block descriptor.
// conversationMarkdownBlock 创建标准 Markdown 块描述。
function conversationMarkdownBlock(messageId: string, index: number, body: string): ConversationBlock {
  return {
    id: `${messageId}-markdown-${index}`,
    kind: "markdown",
    title: "Markdown",
    body,
    summary: compactTextPreview(body),
  };
}

// conversationSpecialBlock creates a semantic block with compact summary metadata.
// conversationSpecialBlock 创建带紧凑摘要元数据的语义块。
function conversationSpecialBlock(
  messageId: string,
  index: number,
  kind: ConversationBlockKind,
  body: string,
  attributes: Record<string, string>,
): ConversationBlock {
  const toolName = attributes.tool_name ?? attributes.name ?? extractToolNameFromText(attributes.title ?? "") ?? extractToolNameFromText(body);
  const title = attributes.title ?? blockTitle(kind, toolName);
  const status = normalizeToolDisplayState(attributes.status ?? attributes.state ?? extractToolStatusFromText(body));
  return {
    id: `${messageId}-${kind}-${index}`,
    kind,
    title,
    body: body.trim(),
    summary: attributes.summary ?? compactTextPreview(body),
    status,
    toolName,
    toolCallId: attributes.tool_call_id,
    toolResultId: attributes.tool_result_id,
    commandSessionId: attributes.command_session_id,
  };
}

// normalizeToolDisplayState maps provider and host state words into WebUI display states.
// normalizeToolDisplayState 将 Provider 与宿主状态词映射为 WebUI 展示状态。
function normalizeToolDisplayState(status?: string | null): string | undefined {
  if (!status?.trim()) {
    return undefined;
  }
  const normalized = status.trim().toLowerCase();
  if (normalized === "failed" || normalized === "error" || normalized === "errored" || normalized === "failure") {
    return "failed";
  }
  if (normalized === "completed" || normalized === "complete" || normalized === "success" || normalized === "succeeded" || normalized === "ready") {
    return "completed";
  }
  if (normalized === "running" || normalized === "pending" || normalized === "preview" || normalized === "in_progress") {
    return "running";
  }
  if (status.includes("失败") || status.includes("错误")) {
    return "failed";
  }
  if (status.includes("完成") || status.includes("成功")) {
    return "completed";
  }
  if (status.includes("运行") || status.includes("进行")) {
    return "running";
  }
  return status.trim();
}

// blockTitle returns a localized title for one semantic transcript block.
// blockTitle 返回单个语义转录块的本地化标题。
function blockTitle(kind: ConversationBlockKind, toolName?: string): string {
  if (kind === "tool-call") return toolName ? `工具调用 · ${toolName}` : "工具调用";
  if (kind === "tool-result") return toolName ? `工具返回 · ${toolName}` : "工具返回";
  if (kind === "command") return "命令会话";
  if (kind === "diff") return "Git Diff";
  if (kind === "thinking") return "思考过程";
  if (kind === "turn-stats") return "Turn Stats";
  if (kind === "notice") return "运行提示";
  return "Markdown";
}

// compactTextPreview returns one short single-line preview for cards and modal headers.
// compactTextPreview 返回用于卡片与弹窗标题区的单行短预览。
function compactTextPreview(source: string): string {
  const text = source
    .replace(/```[\s\S]*?```/g, "代码块")
    .replace(/[#>*_`|~-]/g, " ")
    .replace(/\s+/g, " ")
    .trim();
  if (!text) {
    return "无正文内容";
  }
  return text.length > 140 ? `${text.slice(0, 140)}...` : text;
}

// copyText writes transcript detail text to the system clipboard when available.
// copyText 在可用时将转录详情文本写入系统剪贴板。
async function copyText(text: string): Promise<void> {
  await navigator.clipboard?.writeText(text);
}

// BrandHeader renders the product brand area.
// BrandHeader 渲染产品品牌区域。
function BrandHeader() {
  return (
    <div className="brand">
      <div className="brand-mark">V</div>
      <div>
        <strong>Vulcan Code</strong>
        <span>WebUI</span>
      </div>
    </div>
  );
}

// FeatureNav renders the primary workbench navigation.
// FeatureNav 渲染工作台主导航。
function FeatureNav(props: { areas: FeatureAreaSummary[]; activeArea: MainAreaId; onSelect: (area: MainAreaId) => void }) {
  return (
    <nav className="feature-nav" aria-label="主功能">
      {props.areas.map((area) => (
        <Button
          className="feature-button"
          key={area.id}
          onClick={() => props.onSelect(area.id)}
          size="sm"
          title={area.description}
          type="button"
          variant={props.activeArea === area.id ? "secondary" : "ghost"}
        >
          {areaIconMap[area.id]}
          <span>{area.label}</span>
        </Button>
      ))}
    </nav>
  );
}

// GlobalActions renders app-wide controls in the primary menu bar.
// GlobalActions 渲染主菜单栏中的全局控制。
function GlobalActions(props: {
  startup: StartupSnapshot;
  theme: ThemeMode;
  onToggleTheme: () => void;
  onOpenSettings: () => void;
}) {
  // isPreviewMode detects fallback browser data instead of a live service bridge.
  // isPreviewMode 检测当前是否为浏览器回退预览数据而非真实服务桥接。
  const isPreviewMode = props.startup.phase === "browser-preview";
  // statusLabel translates internal startup phases into compact user-facing labels.
  // statusLabel 将内部启动阶段转换为面向用户的紧凑显示文案。
  const statusLabel = isPreviewMode ? "预览" : props.startup.ready ? "在线" : props.startup.phase;
  // statusKind keeps preview visually distinct from a live ready service.
  // statusKind 让预览状态与真实在线服务在视觉上保持区分。
  const statusKind = isPreviewMode ? "warning" : props.startup.ready ? "ready" : "running";

  return (
    <div className="global-actions">
      <StatusPill status={statusKind} label={statusLabel} />
      <Button
        aria-label={props.theme === "dark" ? "切换为亮色主题" : "切换为暗色主题"}
        className="theme-toggle"
        onClick={props.onToggleTheme}
        size="sm"
        type="button"
        variant="outline"
      >
        {props.theme === "dark" ? <Moon size={16} /> : <Sun size={16} />}
        <span>{props.theme === "dark" ? "Dark" : "Light"}</span>
      </Button>
      <Button aria-label="打开设置" className="icon-button" onClick={props.onOpenSettings} size="icon" type="button" variant="outline">
        <Settings size={18} />
      </Button>
    </div>
  );
}

// Topbar renders the current feature title for non-console pages.
// Topbar 渲染非控制台页面的当前功能标题。
function Topbar(props: {
  activeFeature?: FeatureAreaSummary;
}) {
  return (
    <header className="topbar">
      <div>
        <p className="eyebrow">Vulcan Code WebUI</p>
        <h1>{props.activeFeature?.label ?? "工作台"}</h1>
        <span>{props.activeFeature?.description ?? "Core Service 驱动的新工作台"}</span>
      </div>
    </header>
  );
}

// ConsoleShell renders the shadcn dashboard-like fused sidebar and inset console area.
// ConsoleShell 渲染类似 shadcn dashboard 的融合侧栏与 inset 控制台区域。
function ConsoleShell(props: {
  startup: StartupSnapshot;
  workbench: WorkbenchSnapshot;
  selectedDraftConversation: DraftConversationSelection | null;
  selectedConversation?: ConversationSummary;
  selectedConversationId: string | null;
  loadingConversationId: string | null;
  conversationLoadError: string | null;
  pinnedConversationIds: Set<string>;
  onConversationMenuAction: (action: ConversationMenuAction, conversation: ConversationSummary) => void;
  onCreateConversation: (project: ProjectSummary) => void;
  onProjectMenuAction: (action: ProjectMenuAction, project: ProjectSummary) => void;
  onSelectConversation: (conversationId: string) => void;
}) {
  return (
    <SidebarProvider className="console-shell">
      <ProjectSidebar
        projects={props.workbench.projects}
        conversations={props.workbench.conversations}
        pinnedConversationIds={props.pinnedConversationIds}
        selectedConversationId={props.selectedConversationId}
        onConversationMenuAction={props.onConversationMenuAction}
        onCreateConversation={props.onCreateConversation}
        onProjectMenuAction={props.onProjectMenuAction}
        onSelectConversation={props.onSelectConversation}
      />
      <SidebarInset>
        <ConsoleHeader draftConversation={props.selectedDraftConversation} conversation={props.selectedConversation} />
        <ChatArea
          startup={props.startup}
          workbench={props.workbench}
          selectedDraftConversation={props.selectedDraftConversation}
          selectedConversationId={props.selectedConversationId}
          loadingConversationId={props.loadingConversationId}
          conversationLoadError={props.conversationLoadError}
        />
      </SidebarInset>
    </SidebarProvider>
  );
}

// ConsoleHeader renders the inset title bar for the current conversation.
// ConsoleHeader 渲染当前会话的 inset 标题栏。
function ConsoleHeader(props: {
  conversation?: ConversationSummary;
  draftConversation: DraftConversationSelection | null;
}) {
  return (
    <header className="console-header">
      <div className="console-header-main">
        <SidebarTrigger />
        <span className="console-header-separator" aria-hidden="true" />
        <h1>{props.draftConversation ? "新会话" : props.conversation?.title ?? "会话"}</h1>
      </div>
    </header>
  );
}

// ProjectSidebar renders projects and conversations.
// ProjectSidebar 渲染项目与会话。
function ProjectSidebar(props: {
  projects: ProjectSummary[];
  conversations: ConversationSummary[];
  pinnedConversationIds: Set<string>;
  selectedConversationId: string | null;
  onConversationMenuAction: (action: ConversationMenuAction, conversation: ConversationSummary) => void;
  onCreateConversation: (project: ProjectSummary) => void;
  onProjectMenuAction: (action: ProjectMenuAction, project: ProjectSummary) => void;
  onSelectConversation: (conversationId: string) => void;
}) {
  // sidebarState selects the expanded tree interaction or the collapsed project pop menu.
  // sidebarState 选择展开树交互或折叠项目弹出菜单交互。
  const { state: sidebarState } = useSidebar();
  // openProjectIds stores the project folders currently expanded in the tree.
  // openProjectIds 保存当前在树中展开的项目文件夹。
  const [openProjectIds, setOpenProjectIds] = React.useState<Set<string>>(
    () => new Set(props.projects.map((project) => project.id)),
  );
  // conversationsByProject groups conversations beneath their owning project folder.
  // conversationsByProject 将会话归组到所属项目文件夹下。
  const conversationsByProject = React.useMemo(() => {
    const fallbackProjectId = props.projects[0]?.id ?? "default-project";
    return props.conversations.reduce<Record<string, ConversationSummary[]>>((groups, conversation) => {
      const projectId = conversation.projectId ?? fallbackProjectId;
      groups[projectId] = groups[projectId] ?? [];
      groups[projectId].push(conversation);
      return groups;
    }, {});
  }, [props.conversations, props.projects]);
  // sortedConversationsByProject keeps pinned conversations above normal rows.
  // sortedConversationsByProject 让置顶会话显示在普通行之前。
  const sortedConversationsByProject = React.useMemo(() => {
    return Object.fromEntries(
      Object.entries(conversationsByProject).map(([projectId, conversations]) => [
        projectId,
        [...conversations].sort((left, right) => {
          const leftPinned = props.pinnedConversationIds.has(left.id);
          const rightPinned = props.pinnedConversationIds.has(right.id);
          return Number(rightPinned) - Number(leftPinned);
        }),
      ]),
    );
  }, [conversationsByProject, props.pinnedConversationIds]);
  // toggleProjectOpen toggles one project folder without changing the selected conversation.
  // toggleProjectOpen 切换单个项目文件夹展开状态且不改变当前会话选择。
  const toggleProjectOpen = React.useCallback((projectId: string) => {
    setOpenProjectIds((current) => {
      const next = new Set(current);
      if (next.has(projectId)) {
        next.delete(projectId);
      } else {
        next.add(projectId);
      }
      return next;
    });
  }, []);

  return (
    <Sidebar collapsible="icon" variant="inset">
      <SidebarHeader>
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton tooltip="控制台">
              <LayoutDashboard size={16} />
              <span className="vcn-sidebar-label strong">控制台</span>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarHeader>
      <SidebarContent>
        <SidebarGroup>
          <SidebarGroupLabel><Boxes size={15} /><span>项目</span></SidebarGroupLabel>
          <SidebarMenu>
            {props.projects.map((project) => {
              const isOpen = openProjectIds.has(project.id);
              const folderName = projectFolderName(project.path);
              const showOriginalFolderName = folderName.length > 0 && folderName !== project.name;
              const projectConversations = sortedConversationsByProject[project.id] ?? [];
              return (
                <SidebarMenuItem className="project-tree-item" key={project.id}>
                  {sidebarState === "collapsed" ? (
                    <CollapsedProjectMenu
                      conversations={projectConversations}
                      onCreateConversation={props.onCreateConversation}
                      onSelectConversation={props.onSelectConversation}
                      project={project}
                      selectedConversationId={props.selectedConversationId}
                    />
                  ) : (
                    <ProjectContextMenu project={project} onAction={props.onProjectMenuAction}>
                      <SidebarMenuButton
                        aria-expanded={isOpen}
                        className="project-folder-button"
                        onClick={() => toggleProjectOpen(project.id)}
                        tooltip={`${project.name}\n${project.path}`}
                      >
                        {isOpen ? <FolderOpen size={16} /> : <Folder size={16} />}
                        <span className="vcn-sidebar-label project-folder-label">
                          <strong>{project.name}</strong>
                          {showOriginalFolderName ? <small>{folderName}</small> : null}
                        </span>
                      </SidebarMenuButton>
                    </ProjectContextMenu>
                  )}
                  <SidebarMenuAction
                    aria-label={`在 ${project.name} 中创建新会话`}
                    className="project-create-conversation"
                    onClick={() => props.onCreateConversation(project)}
                    showOnHover
                    title="新建会话"
                    type="button"
                  >
                    <SquarePen size={13} />
                  </SidebarMenuAction>
                  {isOpen && sidebarState !== "collapsed" ? (
                    <SidebarMenu className="project-conversation-list">
                      {projectConversations.map((conversation) => (
                        <SidebarMenuItem key={conversation.id}>
                          <ConversationContextMenu
                            conversation={conversation}
                            isPinned={props.pinnedConversationIds.has(conversation.id)}
                            onAction={props.onConversationMenuAction}
                          >
                            <SidebarMenuButton
                              active={props.selectedConversationId === conversation.id}
                              className="conversation-tree-button"
                              onClick={() => props.onSelectConversation(conversation.id)}
                              tooltip={`${conversation.title}\n${conversation.status}`}
                            >
                              <span className="vcn-sidebar-label conversation-tree-label">
                                <strong>{conversation.title}</strong>
                              </span>
                              {props.pinnedConversationIds.has(conversation.id) ? <Pin size={13} /> : null}
                            </SidebarMenuButton>
                          </ConversationContextMenu>
                        </SidebarMenuItem>
                      ))}
                    </SidebarMenu>
                  ) : null}
                </SidebarMenuItem>
              );
            })}
            {props.projects.length === 0 ? (
              <SidebarMenuItem>
                <SidebarMenuButton className="muted" tooltip="暂无项目">
                  <Folder size={16} />
                  <span className="vcn-sidebar-label">
                    <strong>暂无项目</strong>
                    <small>等待 Workspace 快照载入</small>
                  </span>
                </SidebarMenuButton>
              </SidebarMenuItem>
            ) : null}
          </SidebarMenu>
        </SidebarGroup>
      </SidebarContent>
    </Sidebar>
  );
}

// CollapsedProjectMenu renders project actions as a pop menu while the sidebar is collapsed.
// CollapsedProjectMenu 在侧栏折叠时以弹出菜单展示项目操作。
function CollapsedProjectMenu(props: {
  // Project that owns the collapsed entry.
  // 折叠入口所属项目。
  project: ProjectSummary;
  // Conversations shown below the create command.
  // 显示在新建命令下方的会话列表。
  conversations: ConversationSummary[];
  // Currently selected conversation id.
  // 当前选中的会话标识。
  selectedConversationId: string | null;
  // Opens a new draft conversation for the project.
  // 为项目打开一个新的草稿会话。
  onCreateConversation: (project: ProjectSummary) => void;
  // Selects one existing conversation.
  // 选择一个已有会话。
  onSelectConversation: (conversationId: string) => void;
}) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <SidebarMenuButton
          aria-label={`${props.project.name} 会话菜单`}
          className="project-folder-button collapsed-project-button"
          tooltip={props.project.name}
        >
          <Folder size={16} />
          <span className="vcn-sidebar-label project-folder-label">
            <strong>{props.project.name}</strong>
          </span>
        </SidebarMenuButton>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="collapsed-project-menu" side="right" sideOffset={8}>
        <DropdownMenuItem onSelect={() => props.onCreateConversation(props.project)}>
          <SquarePen size={15} />
          <span>新建会话</span>
        </DropdownMenuItem>
        <DropdownMenuSeparator />
        {props.conversations.length > 0 ? (
          props.conversations.map((conversation) => (
            <DropdownMenuItem
              className={cn(props.selectedConversationId === conversation.id && "active")}
              key={conversation.id}
              onSelect={() => props.onSelectConversation(conversation.id)}
            >
              <span className="collapsed-project-menu-title">{conversation.title}</span>
            </DropdownMenuItem>
          ))
        ) : (
          <DropdownMenuItem disabled>
            <span>暂无会话</span>
          </DropdownMenuItem>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

// ProjectContextMenu renders project-level right-click commands.
// ProjectContextMenu 渲染项目级右键命令。
function ProjectContextMenu(props: {
  project: ProjectSummary;
  children: React.ReactNode;
  onAction: (action: ProjectMenuAction, project: ProjectSummary) => void;
}) {
  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>{props.children}</ContextMenuTrigger>
      <ContextMenuContent className="project-context-menu">
        <ContextMenuItem onSelect={() => props.onAction("toggle-pin", props.project)}>
          <PinOff size={15} />
          <span>{props.project.pinned ? "取消置顶项目" : "置顶项目"}</span>
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => props.onAction("open-directory", props.project)}>
          <FolderOpen size={15} />
          <span>在资源管理器中打开</span>
        </ContextMenuItem>
        <ContextMenuItem disabled>
          <PanelTopOpen size={15} />
          <span>创建永久工作树</span>
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => props.onAction("rename", props.project)}>
          <Pencil size={15} />
          <span>重命名项目</span>
        </ContextMenuItem>
        <ContextMenuItem disabled onSelect={() => props.onAction("archive-conversations", props.project)}>
          <Archive size={15} />
          <span>归档对话</span>
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem disabled variant="destructive">
          <Minus size={15} />
          <span>移除</span>
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  );
}

// RenameDialog renders the controlled project or conversation rename form.
// RenameDialog 渲染受控的项目或会话重命名表单。
function RenameDialog(props: {
  target: RenameTarget | null;
  onCancel: () => void;
  onConfirm: (target: RenameTarget, name: string) => void;
}) {
  // value stores the pending display name edited by the user.
  // value 保存用户正在编辑的显示名称。
  const [value, setValue] = React.useState("");

  React.useEffect(() => {
    setValue(props.target?.name ?? "");
  }, [props.target]);

  const title = props.target?.kind === "project" ? "重命名项目" : "重命名对话";
  const description = props.target?.kind === "project"
    ? "输入新的项目显示名称。项目路径不会改变。"
    : "输入新的对话名称。";

  return (
    <Dialog open={Boolean(props.target)} onOpenChange={(open) => !open && props.onCancel()}>
      <DialogContent className="rename-dialog">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>
        <form
          className="dialog-form"
          onSubmit={(event) => {
            event.preventDefault();
            if (props.target) {
              props.onConfirm(props.target, value);
            }
          }}
        >
          <Input
            autoFocus
            value={value}
            onChange={(event) => setValue(event.target.value)}
            placeholder="输入新的名称"
          />
          <DialogFooter>
            <Button type="button" variant="ghost" onClick={props.onCancel}>取消</Button>
            <Button type="submit">确认</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

// ArchiveProjectDialog confirms bulk conversation archiving for one project.
// ArchiveProjectDialog 确认单个项目下的对话批量归档。
function ArchiveProjectDialog(props: {
  project: ProjectSummary | null;
  conversationCount: number;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  return (
    <Dialog open={Boolean(props.project)} onOpenChange={(open) => !open && props.onCancel()}>
      <DialogContent className="archive-dialog">
        <DialogHeader>
          <DialogTitle>归档 {props.conversationCount} 个对话？</DialogTitle>
          <DialogDescription>
            这会将 {props.project?.name ?? "当前项目"} 中的对话归档。之后你可以在已归档对话中找到它们。
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button type="button" variant="ghost" onClick={props.onCancel}>取消</Button>
          <Button type="button" variant="destructive" onClick={props.onConfirm}>全部归档</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

// ConversationContextMenu renders conversation-level right-click commands.
// ConversationContextMenu 渲染会话级右键命令。
function ConversationContextMenu(props: {
  conversation: ConversationSummary;
  children: React.ReactNode;
  isPinned: boolean;
  onAction: (action: ConversationMenuAction, conversation: ConversationSummary) => void;
}) {
  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>{props.children}</ContextMenuTrigger>
      <ContextMenuContent className="conversation-context-menu">
        <ContextMenuItem onSelect={() => props.onAction("pin", props.conversation)}>
          <Pin size={15} />
          <span>{props.isPinned ? "取消置顶对话" : "置顶对话"}</span>
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => props.onAction("rename", props.conversation)}>
          <Pencil size={15} />
          <span>重命名对话</span>
        </ContextMenuItem>
        <ContextMenuItem disabled onSelect={() => props.onAction("archive", props.conversation)}>
          <Archive size={15} />
          <span>归档对话</span>
        </ContextMenuItem>
        <ContextMenuItem disabled>
          <MailOpen size={15} />
          <span>标记为未读</span>
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem onSelect={() => props.onAction("open-directory", props.conversation)}>
          <FolderOpen size={15} />
          <span>在资源管理器中打开</span>
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => props.onAction("copy-workdir", props.conversation)}>
          <Copy size={15} />
          <span>复制工作目录</span>
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => props.onAction("copy-session-id", props.conversation)}>
          <FileText size={15} />
          <span>复制会话 ID</span>
        </ContextMenuItem>
        <ContextMenuItem disabled>
          <Link size={15} />
          <span>复制深度链接</span>
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem disabled>
          <GitBranch size={15} />
          <span>派生到本地</span>
        </ContextMenuItem>
        <ContextMenuItem disabled>
          <GitBranch size={15} />
          <span>派生到新工作树</span>
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem disabled>
          <ExternalLink size={15} />
          <span>在新窗口中打开</span>
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  );
}

// projectFolderName extracts the folder name from a platform path.
// projectFolderName 从平台路径中提取文件夹名称。
function projectFolderName(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? "";
}

// AreaContent renders the selected workbench feature area.
// AreaContent 渲染选中的工作台功能区域。
function AreaContent(props: {
  activeArea: MainAreaId;
  startup: StartupSnapshot;
  workbench: WorkbenchSnapshot;
  selectedConversationId: string | null;
  loadingConversationId: string | null;
  conversationLoadError: string | null;
}) {
  if (props.activeArea === "projects") {
    return <ProjectsArea projects={props.workbench.projects} modules={props.workbench.modules} />;
  }
  if (props.activeArea === "providers") {
    return <ProvidersArea providers={props.workbench.providers} modules={props.workbench.modules} />;
  }
  if (props.activeArea === "tools") {
    return <ToolsArea toolCalls={props.workbench.toolCalls} commandSessions={props.workbench.commandSessions} />;
  }
  if (props.activeArea === "plugins") {
    return <PluginsArea plugins={props.workbench.plugins} />;
  }
  if (props.activeArea === "memory") {
    return <MemoryArea memories={props.workbench.memories} />;
  }
  if (props.activeArea === "settings") {
    return <SettingsArea settings={props.workbench.settings} />;
  }
  if (props.activeArea === "logs") {
    return <LogsArea startup={props.startup} logs={props.workbench.logs} />;
  }
  return (
    <ChatArea
      startup={props.startup}
      workbench={props.workbench}
      selectedDraftConversation={null}
      selectedConversationId={props.selectedConversationId}
      loadingConversationId={props.loadingConversationId}
      conversationLoadError={props.conversationLoadError}
    />
  );
}

// ChatArea renders the conversation workspace and composer.
// ChatArea 渲染会话工作区与输入区域。
function ChatArea(props: {
  startup: StartupSnapshot;
  workbench: WorkbenchSnapshot;
  selectedDraftConversation: DraftConversationSelection | null;
  selectedConversationId: string | null;
  loadingConversationId: string | null;
  conversationLoadError: string | null;
}) {
  const selectedConversation = props.workbench.conversations.find((conversation) => conversation.id === props.selectedConversationId);
  const messages = props.workbench.messages.filter((message) => message.conversationId === props.selectedConversationId);
  const isLoading = props.loadingConversationId === props.selectedConversationId;
  const selectedProject = selectedConversation ? projectForConversation(props.workbench, selectedConversation) : null;
  const emptyProjectName = props.selectedDraftConversation?.projectName ?? selectedProject?.name ?? "当前项目";
  const isEmptyConversation = Boolean(
    props.selectedDraftConversation
      || (selectedConversation && !isLoading && !props.conversationLoadError && messages.length === 0),
  );

  return (
    <div className={cn("area-grid chat-grid", isEmptyConversation && "empty-conversation-grid")}>
      {isEmptyConversation ? (
        <EmptyConversationCenter projectName={emptyProjectName} />
      ) : (
        <>
          <ConversationWorkspace
            conversation={selectedConversation}
            messages={messages}
            isLoading={isLoading}
            error={props.conversationLoadError}
          />
          <ComposerPanel />
        </>
      )}
    </div>
  );
}

// ProjectsArea renders project and workspace status.
// ProjectsArea 渲染项目与工作区状态。
function ProjectsArea(props: { projects: ProjectSummary[]; modules: RuntimeModuleSnapshot[] }) {
  return (
    <div className="area-grid two-columns">
      <section className="panel">
        <PanelTitle icon={<Boxes size={17} />} title="项目管理" />
        <div className="card-list">
          {props.projects.map((project) => (
            <article className="data-card" key={project.id}>
              <strong>{project.name}</strong>
              <span>{project.path}</span>
            </article>
          ))}
        </div>
      </section>
      <RuntimeModulesPanel modules={props.modules} />
    </div>
  );
}

// ProvidersArea renders model provider status.
// ProvidersArea 渲染模型 Provider 状态。
function ProvidersArea(props: { providers: ProviderSummary[]; modules: RuntimeModuleSnapshot[] }) {
  return (
    <div className="area-grid two-columns">
      <section className="panel">
        <PanelTitle icon={<SlidersHorizontal size={17} />} title="模型与 Provider" />
        <div className="card-grid">
          {props.providers.map((provider) => (
            <article className="data-card" key={provider.id}>
              <StatusPill status="ready" label={provider.status} />
              <strong>{provider.name}</strong>
              <span>已启用模型：{provider.enabledModels}</span>
            </article>
          ))}
        </div>
      </section>
      <RuntimeModulesPanel modules={props.modules.filter((module) => module.id === "providers" || module.id === "core")} />
    </div>
  );
}

// ToolsArea renders tool calls and command sessions.
// ToolsArea 渲染工具调用与命令会话。
function ToolsArea(props: { toolCalls: ToolCallSummary[]; commandSessions: CommandSessionSummary[] }) {
  return (
    <div className="area-grid two-columns">
      <ToolPanel toolCalls={props.toolCalls} />
      <CommandSessionsPanel commandSessions={props.commandSessions} />
    </div>
  );
}

// PluginsArea renders plugin control center.
// PluginsArea 渲染插件控制中心。
function PluginsArea(props: { plugins: PluginSummary[] }) {
  return (
    <section className="panel full-height">
      <PanelTitle icon={<Wrench size={17} />} title="插件中心" />
      <div className="card-grid">
        {props.plugins.map((plugin) => (
          <article className="data-card" key={plugin.id}>
            <StatusPill status={plugin.status === "已启用" ? "ready" : "warning"} label={plugin.status} />
            <strong>{plugin.name}</strong>
            <span>{plugin.kind}</span>
            <p>{plugin.capability}</p>
          </article>
        ))}
      </div>
    </section>
  );
}

// MemoryArea renders VMM memory entry points.
// MemoryArea 渲染 VMM 记忆入口。
function MemoryArea(props: { memories: MemorySummary[] }) {
  return (
    <section className="panel full-height">
      <PanelTitle icon={<BrainCircuit size={17} />} title="VMM 记忆" />
      <div className="card-grid">
        {props.memories.map((memory) => (
          <article className="data-card" key={memory.id}>
            <StatusPill status="warning" label={memory.category} />
            <strong>{memory.title}</strong>
            <span>{memory.state}</span>
          </article>
        ))}
      </div>
    </section>
  );
}

// SettingsArea renders settings sections.
// SettingsArea 渲染设置分组。
function SettingsArea(props: { settings: SettingSectionSummary[] }) {
  return (
    <section className="panel full-height">
      <PanelTitle icon={<Settings size={17} />} title="设置" />
      <div className="settings-grid">
        {props.settings.map((section) => (
          <article className="settings-card" key={section.id}>
            <strong>{section.label}</strong>
            <p>{section.description}</p>
            <div className="chip-row">
              {section.items.map((item) => (
                <span className="chip" key={item}>{item}</span>
              ))}
            </div>
          </article>
        ))}
      </div>
    </section>
  );
}

// LogsArea renders startup and diagnostic logs.
// LogsArea 渲染启动与诊断日志。
function LogsArea(props: { startup: StartupSnapshot; logs: LogEntrySummary[] }) {
  return (
    <div className="area-grid two-columns">
      <StartupPanel startup={props.startup} />
      <section className="panel">
        <PanelTitle icon={<History size={17} />} title="诊断日志" />
        <div className="log-list">
          {props.logs.map((log) => (
            <article className="log-row" key={log.id}>
              <StatusPill status={log.level === "info" ? "ready" : "warning"} label={log.level} />
              <strong>{log.source}</strong>
              <span>{log.message}</span>
            </article>
          ))}
        </div>
      </section>
    </div>
  );
}

// StartupPanel renders service startup progress.
// StartupPanel 渲染服务启动进度。
function StartupPanel(props: { startup: StartupSnapshot }) {
  return (
    <section className="panel startup-panel">
      <PanelTitle icon={<PlugZap size={17} />} title="启动状态" />
      <div className="step-list">
        {props.startup.steps.map((step) => (
          <div className="step-item" key={step.id}>
            <StatusIcon status={step.status} />
            <div>
              <strong>{step.label}</strong>
              <span>{step.message ?? "等待状态更新"}</span>
            </div>
          </div>
        ))}
      </div>
      <div className="log-preview">
        {props.startup.recentLogs.map((line) => (
          <code key={line}>{line}</code>
        ))}
      </div>
    </section>
  );
}

// RuntimeModulesPanel renders module status cards.
// RuntimeModulesPanel 渲染模块状态卡片。
function RuntimeModulesPanel(props: { modules: RuntimeModuleSnapshot[] }) {
  return (
    <section className="panel modules-panel">
      <PanelTitle icon={<Database size={17} />} title="运行时模块" />
      <div className="module-grid">
        {props.modules.map((module) => (
          <article className="module-card" key={module.id}>
            <StatusPill status={module.status} label={module.status} />
            <strong>{module.label}</strong>
            <span>{module.message}</span>
          </article>
        ))}
      </div>
    </section>
  );
}

// ConversationWorkspace renders the conversation timeline replacement.
// ConversationWorkspace 渲染会话时间线替代组件。
function ConversationWorkspace(props: {
  conversation?: ConversationSummary;
  messages: ConversationMessage[];
  isLoading: boolean;
  error: string | null;
}) {
  // detail stores the currently opened transcript detail modal.
  // detail 保存当前打开的转录详情弹窗。
  const [detail, setDetail] = React.useState<ConversationDetail | null>(null);
  const renderItems = React.useMemo(() => projectConversationRenderItems(props.messages), [props.messages]);

  return (
    <section className="conversation-panel">
      <div className="timeline">
        {props.isLoading ? (
          <article className="message-card system">
            <span>system</span>
            <p>正在从 Core Service 加载当前会话正文...</p>
          </article>
        ) : null}
        {props.error ? (
          <article className="message-card system error-message">
            <span>system</span>
            <p>会话正文加载失败：{props.error}</p>
          </article>
        ) : null}
        {props.messages.length === 0 ? (
          <article className="message-card system">
            <span>system</span>
            <p>{props.conversation ? "当前会话正文尚未载入或没有可见消息。" : "尚未选择会话，等待工作区数据载入。"}</p>
          </article>
        ) : null}
        {renderItems.map((item) => (
          <ConversationRenderItemView item={item} key={item.id} onOpenDetail={setDetail} />
        ))}
      </div>
      {detail ? <ConversationDetailModal detail={detail} onClose={() => setDetail(null)} /> : null}
    </section>
  );
}

// ConversationRenderItemView renders one projected transcript item.
// ConversationRenderItemView 渲染单条投影后的转录展示项。
function ConversationRenderItemView(props: {
  item: ConversationRenderItem;
  onOpenDetail: (detail: ConversationDetail) => void;
}) {
  const userBody = React.useMemo(
    () => stripUserPromptHeading(props.item.userMessage?.body ?? ""),
    [props.item.userMessage?.body],
  );
  if (props.item.role === "user") {
    return (
      <article className="message-row user">
        <div className="user-bubble">
          <p>{userBody}</p>
        </div>
      </article>
    );
  }

  return (
    <article className={`message-row ${props.item.role}`}>
      <div className="assistant-flow">
        {props.item.role === "system" ? <span className="message-role-label">SYSTEM</span> : null}
        <ConversationBlocksView blocks={props.item.blocks} onOpenDetail={props.onOpenDetail} />
      </div>
    </article>
  );
}

// stripUserPromptHeading removes legacy transcript headings from visible user bubbles.
// stripUserPromptHeading 从可见用户气泡中移除旧版转录标题。
function stripUserPromptHeading(body: string): string {
  return body
    .replace(/^\s*(?:#{1,6}\s*)?(?:\*\*)?(?:用户指令|User Prompt|USER)(?:\*\*)?\s*\n+/i, "")
    .trim();
}

// ConversationBlockView dispatches one parsed block to its visual renderer.
// ConversationBlockView 将单个解析块分派给对应视觉渲染器。
function ConversationBlocksView(props: {
  blocks: ConversationBlock[];
  onOpenDetail: (detail: ConversationDetail) => void;
}) {
  if (props.blocks.length > 1 && props.blocks.every(isMergeableToolBlock)) {
    return <ToolExecutionBlock blocks={props.blocks} onOpenDetail={props.onOpenDetail} />;
  }

  const [block] = props.blocks;
  if (!block) {
    return null;
  }
  if (block.kind === "markdown") {
    return <MarkdownMessage content={block.body} />;
  }
  return <SpecialConversationBlock block={block} onOpenDetail={props.onOpenDetail} />;
}

// MarkdownMessage renders assistant Markdown with Vulcan-specific component styling.
// MarkdownMessage 使用 Vulcan 定制组件样式渲染 assistant Markdown。
function MarkdownMessage(props: { content: string }) {
  return (
    <div className="markdown-message">
      <ReactMarkdown
        components={{
          a: ({ children, href }) => (
            <a href={href} rel="noreferrer" target="_blank">{children}</a>
          ),
          code: ({ children, className }) => {
            const inline = !className;
            return inline ? <code className="inline-code">{children}</code> : <code className={className}>{children}</code>;
          },
          pre: ({ children }) => <pre className="markdown-code-block">{children}</pre>,
          table: ({ children }) => <div className="markdown-table-wrap"><table>{children}</table></div>,
        }}
        rehypePlugins={markdownSanitizers}
        remarkPlugins={markdownPlugins}
      >
        {props.content}
      </ReactMarkdown>
    </div>
  );
}

// SpecialConversationBlock renders compact tool, command, diff, thinking, and notice rows.
// SpecialConversationBlock 渲染紧凑的工具、命令、diff、思考与提示行。
function SpecialConversationBlock(props: {
  block: ConversationBlock;
  onOpenDetail: (detail: ConversationDetail) => void;
}) {
  if (props.block.kind === "turn-stats") {
    return <TurnStatsBlock block={props.block} />;
  }

  if (isMergeableToolBlock(props.block)) {
    return <ToolExecutionBlock blocks={[props.block]} onOpenDetail={props.onOpenDetail} />;
  }
  if (props.block.kind === "thinking") {
    return <ThinkingBlock block={props.block} />;
  }

  const Icon = blockIcon(props.block.kind);
  const canOpen = detailEligibleBlockKinds.has(props.block.kind);
  return (
    <button
      className={`special-block ${props.block.kind}`}
      disabled={!canOpen}
      onClick={() => {
        if (canOpen) {
          props.onOpenDetail({
            title: props.block.title,
            kind: props.block.kind,
            body: props.block.body,
            metadata: [
              ["类型", blockTitle(props.block.kind, props.block.toolName)],
              ...(props.block.status ? [["状态", props.block.status] as [string, string]] : []),
              ...(props.block.toolName ? [["工具", props.block.toolName] as [string, string]] : []),
            ],
          });
        }
      }}
      type="button"
    >
      <div className="special-block-main">
        <Icon size={16} />
        <div>
          <strong>{props.block.title}</strong>
          <span>{props.block.summary}</span>
        </div>
      </div>
      {props.block.status ? <StatusPill status={props.block.status} label={props.block.status} /> : null}
      {canOpen ? <Maximize2 size={15} /> : null}
    </button>
  );
}

// ThinkingBlock renders an inline collapsible thinking transcript block.
// ThinkingBlock 渲染可在原地折叠展开的内联思考转录块。
function ThinkingBlock(props: { block: ConversationBlock }) {
  // expanded stores whether the thinking body is currently visible.
  // expanded 保存思考正文当前是否可见。
  const [expanded, setExpanded] = React.useState(false);

  return (
    <section className={`thinking-block ${expanded ? "expanded" : "collapsed"}`}>
      <button
        aria-expanded={expanded}
        className="thinking-toggle"
        onClick={() => setExpanded((current) => !current)}
        type="button"
      >
        <span className="thinking-toggle-label">
          <BrainCircuit size={15} />
          <span>思考过程</span>
        </span>
        {expanded ? <ChevronDown size={15} /> : <ChevronRight size={15} />}
      </button>
      {expanded ? (
        <div className="thinking-body">
          <MarkdownMessage content={props.block.body} />
        </div>
      ) : null}
    </section>
  );
}

// ToolExecutionBlock renders one compact merged tool execution row.
// ToolExecutionBlock 渲染单条紧凑的合并工具执行行。
function ToolExecutionBlock(props: {
  blocks: ConversationBlock[];
  onOpenDetail: (detail: ConversationDetail) => void;
}) {
  const callBlock = props.blocks.find((block) => block.kind === "tool-call");
  const resultBlock = props.blocks.find((block) => block.kind === "tool-result");
  const commandBlock = props.blocks.find((block) => block.kind === "command");
  const primaryBlock = resultBlock ?? commandBlock ?? callBlock ?? props.blocks[0];
  if (!primaryBlock) {
    return null;
  }

  const summary = toolExecutionSummary(props.blocks);
  const toolName = toolExecutionName(props.blocks);
  const displayState = toolExecutionDisplayState(props.blocks);
  const detailBody = props.blocks
    .map((block) => `## ${blockTitle(block.kind, block.toolName)}\n\n${block.body}`)
    .join("\n\n---\n\n");
  const metadata = [
    ...(primaryBlock.toolCallId ? [["调用", primaryBlock.toolCallId] as [string, string]] : []),
    ...(toolName ? [["工具", toolName] as [string, string]] : []),
    ["状态", toolDisplayStateLabel(displayState)] as [string, string],
    ...(resultBlock ? [["结果", "已返回"] as [string, string]] : []),
  ];

  return (
    <button
      className="tool-execution-row"
      onClick={() => props.onOpenDetail({
        title: summary,
        kind: primaryBlock.kind,
        body: detailBody,
        metadata,
      })}
      type="button"
    >
      <Wrench size={15} />
      <span>{summary}</span>
    </button>
  );
}

// toolExecutionSummary builds the compact transcript row summary for one tool execution.
// toolExecutionSummary 为单次工具执行构建紧凑转录行摘要。
function toolExecutionSummary(blocks: ConversationBlock[]): string {
  const commandBlock = blocks.find((block) => block.kind === "command");
  const resultBlock = blocks.find((block) => block.kind === "tool-result");
  const callBlock = blocks.find((block) => block.kind === "tool-call");
  const source = `${resultBlock?.body ?? ""}\n${callBlock?.body ?? ""}`;
  const toolName = toolExecutionName(blocks);
  const statusLabel = toolDisplayStateLabel(toolExecutionDisplayState(blocks));
  if (commandBlock && !toolName) {
    return commandBlock.summary.includes("命令")
      ? commandBlock.summary
      : "已运行 1 条命令";
  }

  const editedFiles = source.match(/已编辑\s+(\d+)\s+个文件|edited\s+(\d+)\s+file/i);
  let activity: string | null = null;
  if (editedFiles) {
    const count = editedFiles[1] ?? editedFiles[2] ?? "1";
    activity = `已编辑 ${count} 个文件`;
  }

  const commands = source.match(/已运行\s+(\d+)\s+条命令|ran\s+(\d+)\s+command/i);
  if (!activity && commands) {
    const count = commands[1] ?? commands[2] ?? "1";
    activity = `已运行 ${count} 条命令`;
  }

  if (toolName) {
    return `调用 ${toolName} 工具 · ${statusLabel}${activity ? ` · ${activity}` : ""}`;
  }
  return `工具执行 · ${statusLabel}${activity ? ` · ${activity}` : ""}`;
}

// toolExecutionName resolves the best human-visible tool name across merged call and result blocks.
// toolExecutionName 从合并后的调用与返回块中解析最佳人类可见工具名。
function toolExecutionName(blocks: ConversationBlock[]): string | null {
  for (const block of blocks) {
    const name = block.toolName ?? extractToolNameFromText(block.title) ?? extractToolNameFromText(block.body);
    if (name) {
      return name;
    }
  }
  return null;
}

// toolExecutionDisplayState resolves the stable compact display state for one tool execution row.
// toolExecutionDisplayState 解析单条工具执行行使用的稳定紧凑显示状态。
function toolExecutionDisplayState(blocks: ConversationBlock[]): "failed" | "completed" | "running" {
  const source = blocks.map((block) => `${block.status ?? ""}\n${block.title}\n${block.body}`).join("\n");
  const textState = normalizeToolDisplayState(extractToolStatusFromText(source));
  if (textState === "failed") {
    return "failed";
  }
  if (blocks.some((block) => block.status === "failed" || block.status === "error")) {
    return "failed";
  }
  if (blocks.some((block) => block.kind === "tool-result" || block.kind === "command")) {
    return "completed";
  }
  if (textState === "completed") {
    return "completed";
  }
  return "running";
}

// toolDisplayStateLabel returns the localized label used inside compact tool rows.
// toolDisplayStateLabel 返回紧凑工具行中使用的本地化状态文案。
function toolDisplayStateLabel(status: "failed" | "completed" | "running" | string): string {
  if (status === "failed") return "失败";
  if (status === "completed") return "已完成";
  return "进行中";
}

// TurnStatsBlock renders the compact non-clickable per-turn statistics row.
// TurnStatsBlock 渲染不可点击的紧凑按轮次统计横条。
function TurnStatsBlock(props: { block: ConversationBlock }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <div className={`turn-stats-row ${props.block.status ?? "completed"}`}>
          <span aria-hidden="true">{turnStatsIcon(props.block.status)}</span>
          <strong>{props.block.summary || props.block.title}</strong>
        </div>
      </TooltipTrigger>
      <TooltipContent className="turn-stats-tooltip" side="top" sideOffset={8}>
        <pre>{props.block.body}</pre>
      </TooltipContent>
    </Tooltip>
  );
}

// turnStatsIcon returns the compact status mark used by turn statistics.
// turnStatsIcon 返回轮次统计使用的紧凑状态标记。
function turnStatsIcon(status?: string): string {
  if (status === "failed") return "!";
  if (status === "stopped") return "■";
  return "◔";
}

// blockIcon returns the icon component used by one special transcript block.
// blockIcon 返回单个特殊转录块使用的图标组件。
function blockIcon(kind: ConversationBlockKind) {
  if (kind === "diff") return FileText;
  if (kind === "thinking") return BrainCircuit;
  if (kind === "command") return TerminalSquare;
  if (kind === "tool-call" || kind === "tool-result") return Hammer;
  return ScrollText;
}

// ConversationDetailModal renders the enlarged detail surface for special transcript blocks.
// ConversationDetailModal 渲染特殊转录块的放大详情界面。
function ConversationDetailModal(props: { detail: ConversationDetail; onClose: () => void }) {
  const DetailIcon = blockIcon(props.detail.kind);
  const detailType = blockTitle(props.detail.kind);
  const detailSubtitle = props.detail.metadata
    ?.map(([label, value]) => `${label}: ${value}`)
    .join(" · ");
  return (
    <Dialog open onOpenChange={(open) => !open && props.onClose()}>
      <DialogContent className="detail-modal sm:max-w-[min(1080px,96vw)]" showCloseButton={false}>
        <header className="detail-modal-header">
          <div className="detail-title-row">
            <div className="detail-title-icon">
              <DetailIcon size={18} />
            </div>
            <div className="detail-title-copy">
              <span>{detailType}</span>
              <DialogTitle>{props.detail.title}</DialogTitle>
              {detailSubtitle ? <p>{detailSubtitle}</p> : null}
            </div>
          </div>
          <div className="detail-modal-actions">
            <Button aria-label="复制详情" className="icon-button" onClick={() => void copyText(props.detail.body)} size="icon" type="button" variant="outline">
              <Copy size={17} />
            </Button>
            <Button aria-label="关闭详情" className="icon-button" onClick={props.onClose} size="icon" type="button" variant="outline">
              <X size={17} />
            </Button>
          </div>
        </header>
        <div className="detail-modal-body">
          {props.detail.kind === "diff" ? (
            <pre className="diff-detail"><code>{props.detail.body}</code></pre>
          ) : (
            <MarkdownMessage content={props.detail.body} />
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}

// ComposerPanel renders the first-version message input surface.
// ComposerPanel 渲染第一版消息输入区域。
function ComposerPanel() {
  // draftInput stores the current unsent composer text.
  // draftInput 保存当前尚未发送的输入区文本。
  const [draftInput, setDraftInput] = React.useState("");
  // placeholder describes the platform-specific send and newline keyboard behavior.
  // placeholder 描述平台相关的发送与换行键盘行为。
  const placeholder = React.useMemo(() => {
    return `输入当前会话要执行的任务指令（${composerShortcutHint()}）`;
  }, []);

  // handleComposerSend handles the current placeholder-only send action.
  // handleComposerSend 处理当前仅占位的发送动作。
  const handleComposerSend = React.useCallback(() => {
    if (!draftInput.trim()) {
      toast.info("请输入任务指令。");
      return;
    }
    toast.info("发送已拦截：真实发送链路待接入。");
  }, [draftInput]);

  // handleComposerKeyDown maps Enter to send while keeping Shift+Enter as a newline.
  // handleComposerKeyDown 将 Enter 映射为发送，并保留 Shift+Enter 换行。
  const handleComposerKeyDown = React.useCallback((event: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key !== "Enter" || event.shiftKey || event.nativeEvent.isComposing) {
      return;
    }
    event.preventDefault();
    handleComposerSend();
  }, [handleComposerSend]);

  return (
    <section className="composer-panel">
      <Textarea
        aria-label="当前会话输入"
        onChange={(event) => setDraftInput(event.currentTarget.value)}
        onKeyDown={handleComposerKeyDown}
        placeholder={placeholder}
        value={draftInput}
      />
      <div className="composer-toolbar">
        <div className="composer-toolbar-left">
          <Button size="sm" type="button" variant="outline">build</Button>
          <Button size="sm" type="button" variant="outline">模型</Button>
          <Button size="sm" type="button" variant="outline">工具</Button>
        </div>
        <Button className="send-button" onClick={handleComposerSend} type="button">发送</Button>
      </div>
    </section>
  );
}

// composerShortcutHint returns a platform-specific keyboard hint for the composer placeholder.
// composerShortcutHint 返回输入区 placeholder 使用的平台化快捷键提示。
function composerShortcutHint(): string {
  const platformText = typeof navigator === "undefined"
    ? ""
    : `${navigator.platform} ${navigator.userAgent}`.toLowerCase();
  if (platformText.includes("mac")) {
    return "⏎ 发送，Shift + ⏎ 换行";
  }
  return "⏎ 发送，Shift + ⏎ 换行";
}

// EmptyConversationCenter renders the centered first-prompt state for a blank conversation.
// EmptyConversationCenter 渲染空白会话的居中首次输入状态。
function EmptyConversationCenter(props: { projectName: string }) {
  return (
    <section className="empty-conversation-center">
      <h2>我们应该在 {props.projectName} 中构建什么？</h2>
      <ComposerPanel />
    </section>
  );
}

// ToolPanel renders tool call blocks.
// ToolPanel 渲染工具调用块。
function ToolPanel(props: { toolCalls: ToolCallSummary[] }) {
  return (
    <section className="panel tool-panel">
      <PanelTitle icon={<Hammer size={17} />} title="工具调用" />
      <div className="tool-list">
        {props.toolCalls.map((toolCall) => (
          <article className="tool-card" key={toolCall.id}>
            <div>
              <TerminalSquare size={16} />
              <strong>{toolCall.name}</strong>
            </div>
            <StatusPill status={toolCall.status === "ready" ? "ready" : "warning"} label={toolCall.status} />
            <p>{toolCall.detail}</p>
          </article>
        ))}
      </div>
    </section>
  );
}

// CommandSessionsPanel renders command session summaries.
// CommandSessionsPanel 渲染命令会话摘要。
function CommandSessionsPanel(props: { commandSessions: CommandSessionSummary[] }) {
  return (
    <section className="panel">
      <PanelTitle icon={<TerminalSquare size={17} />} title="命令会话" />
      <div className="tool-list">
        {props.commandSessions.map((session) => (
          <article className="tool-card" key={session.id}>
            <div>
              <TerminalSquare size={16} />
              <strong>{session.command}</strong>
            </div>
            <StatusPill status={session.status === "passed" ? "ready" : "warning"} label={session.status} />
            <p>{session.preview}</p>
          </article>
        ))}
      </div>
    </section>
  );
}

// PanelTitle renders a section title with an icon.
// PanelTitle 渲染带图标的区域标题。
function PanelTitle(props: { icon: React.ReactNode; title: string }) {
  return (
    <div className="panel-title">
      {props.icon}
      <h2>{props.title}</h2>
    </div>
  );
}

// StatusIcon renders an icon for a startup status.
// StatusIcon 渲染启动状态图标。
function StatusIcon(props: { status: StartupStepStatus }) {
  if (props.status === "ready") {
    return <CheckCircle2 className="status-icon ready" size={18} />;
  }
  if (props.status === "warning" || props.status === "error") {
    return <CircleAlert className={`status-icon ${props.status}`} size={18} />;
  }
  return <Clock3 className="status-icon pending" size={18} />;
}

// StatusPill renders a compact text status badge.
// StatusPill 渲染紧凑文本状态徽标。
function StatusPill(props: { status: StartupStepStatus | string; label: string }) {
  return <Badge className={cn("status-pill", props.status)} variant="outline">{props.label}</Badge>;
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
