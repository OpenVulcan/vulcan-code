import React from "react";
import ReactDOM from "react-dom/client";
import ReactMarkdown from "react-markdown";
import {
  closestCenter,
  DndContext,
  PointerSensor,
  useSensor,
  useSensors,
  type DragEndEvent,
} from "@dnd-kit/core";
import {
  arrayMove,
  SortableContext,
  useSortable,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import rehypeSanitize from "rehype-sanitize";
import remarkGfm from "remark-gfm";
import { Button } from "@/components/ui/button";
import { StatusPill } from "@/components/shared/status";
import {
  FeatureShell as ExtractedFeatureShell,
  areaIconMap,
  primaryFeatureAreas as buildPrimaryFeatureAreas,
} from "@/components/features/feature-shell";
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
import { Input } from "@/components/ui/input";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "@/components/ui/resizable";
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from "@/components/ui/tabs";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
} from "@/components/ui/select";
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
import { ShinyText } from "@/components/ui/shiny-text";
import { Textarea } from "@/components/ui/textarea";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import { I18nProvider, useI18n, type LocaleCode } from "@/i18n";
import type {
  ConversationMessage,
  ConversationSummary,
  FeatureAreaSummary,
  MainAreaId,
  ProjectSummary,
  SettingsSubAreaId,
  StartupSnapshot,
  WorkbenchSnapshot,
} from "@/types";
import { toast } from "sonner";
import * as LucideIcons from "lucide-react";
import {
  Activity,
  Boxes,
  BrainCircuit,
  Archive,
  ArrowDown,
  ChevronLeft,
  ChevronDown,
  ChevronRight,
  CheckCircle2,
  CircleAlert,
  Clock3,
  Code2,
  Copy,
  Cpu,
  Database,
  FileCode2,
  FileJson2,
  ExternalLink,
  FileText,
  Folder,
  FolderPlus,
  FolderOpen,
  GitBranch,
  Globe,
  Hammer,
  History,
  Link,
  Minus,
  LayoutDashboard,
  MailOpen,
  Maximize2,
  Minimize2,
  MessageSquareText,
  PanelRightClose,
  PanelRightOpen,
  PanelTopOpen,
  Palette,
  Pencil,
  Pin,
  PlugZap,
  PinOff,
  ScrollText,
  Send,
  Settings,
  Shield,
  Square,
  SquarePen,
  SlidersHorizontal,
  Moon,
  MinusIcon,
  Sun,
  SquareIcon,
  TerminalSquare,
  Wrench,
  X,
} from "lucide-react";
import "./styles.css";

// GITHUB_REPOSITORY_URL points to the public project repository.
// GITHUB_REPOSITORY_URL 指向公开项目仓库。
const GITHUB_REPOSITORY_URL = "https://github.com/OpenVulcan/vulcan-code";

// ThemeMode describes the two shadcn default theme modes.
// ThemeMode 描述 shadcn 默认的两种主题模式。
type ThemeMode = "light" | "dark";

// ProjectIconBadgeShape names the badge silhouette used by generated project icons.
// ProjectIconBadgeShape 命名生成式项目图标使用的徽标轮廓。
type ProjectIconBadgeShape = "circle" | "square" | "rounded" | "sharp";

// ProjectIconSelection describes one icon override persisted for a project.
// ProjectIconSelection 描述为项目持久化的单个图标覆盖配置。
interface ProjectIconSelection {
  // Icon pack identifier stored in the project row.
  // 存储在项目行中的图标包标识。
  pack: string;
  // Icon key or encoded badge configuration stored in the project row.
  // 存储在项目行中的图标键或编码后的徽标配置。
  key: string;
}

// ProjectIconBadgeConfig stores the generated badge icon appearance.
// ProjectIconBadgeConfig 保存生成式徽标图标的外观。
interface ProjectIconBadgeConfig {
  // Badge shape used by the generated SVG.
  // 生成 SVG 使用的徽标形状。
  shape: ProjectIconBadgeShape;
  // One Chinese character or up to two Latin letters.
  // 一个中文字符或最多两个英文字母。
  text: string;
  // Badge background color.
  // 徽标背景色。
  background: string;
  // Badge border color.
  // 徽标边框色。
  border: string;
  // Badge text color.
  // 徽标文字色。
  foreground: string;
}

// ProjectIconOption describes one selectable lucide project icon.
// ProjectIconOption 描述一个可选择的 lucide 项目图标。
interface ProjectIconOption {
  // Stable icon key persisted in the project row.
  // 持久化到项目行中的稳定图标键。
  key: string;
  // User-facing icon label.
  // 面向用户显示的图标名称。
  label: string;
  // Lucide icon component rendered by the option.
  // 选项渲染使用的 Lucide 图标组件。
  Icon: React.ComponentType<{ size?: number; className?: string }>;
}

// DEFAULT_PROJECT_BADGE_CONFIG provides the first generated badge preview.
// DEFAULT_PROJECT_BADGE_CONFIG 提供生成式徽标的初始预览配置。
const DEFAULT_PROJECT_BADGE_CONFIG: ProjectIconBadgeConfig = {
  shape: "rounded",
  text: "VC",
  background: "#111827",
  border: "#111827",
  foreground: "#ffffff",
};

// PROJECT_BADGE_COLORS lists quick colors for generated project badges.
// PROJECT_BADGE_COLORS 列出生成式项目徽标的快捷颜色。
const PROJECT_BADGE_COLORS = ["#111827", "#2563eb", "#0891b2", "#16a34a", "#ca8a04", "#dc2626", "#7c3aed", "#db2777"];

// LUCIDE_ICON_STREAM_BATCH_SIZE controls how many icon cells are mounted per incremental pass.
// LUCIDE_ICON_STREAM_BATCH_SIZE 控制每次增量挂载多少个图标单元。
const LUCIDE_ICON_STREAM_BATCH_SIZE = 120;

// LUCIDE_PROJECT_ICON_OPTIONS lists every usable exported Lucide icon for the project picker.
// LUCIDE_PROJECT_ICON_OPTIONS 列出项目选择器可使用的全部 Lucide 导出图标。
const LUCIDE_PROJECT_ICON_OPTIONS: ProjectIconOption[] = buildLucideProjectIconOptions();

// PROJECT_ICON_MAP resolves persisted lucide icon keys to components.
// PROJECT_ICON_MAP 将持久化的 lucide 图标键解析为组件。
const PROJECT_ICON_MAP = new Map<string, ProjectIconOption>(
  [
    ...LUCIDE_PROJECT_ICON_OPTIONS.map((icon) => [icon.key, icon] as const),
    ["folder", { key: "Folder", label: "Folder", Icon: Folder }] as const,
    ["folder-open", { key: "FolderOpen", label: "FolderOpen", Icon: FolderOpen }] as const,
    ["folder-plus", { key: "FolderPlus", label: "FolderPlus", Icon: FolderPlus }] as const,
    ["layout-dashboard", { key: "LayoutDashboard", label: "LayoutDashboard", Icon: LayoutDashboard }] as const,
    ["scroll-text", { key: "ScrollText", label: "ScrollText", Icon: ScrollText }] as const,
    ["message-square-text", { key: "MessageSquareText", label: "MessageSquareText", Icon: MessageSquareText }] as const,
  ],
);

// buildLucideProjectIconOptions extracts usable icon components from lucide-react exports.
// buildLucideProjectIconOptions 从 lucide-react 导出中提取可用图标组件。
function buildLucideProjectIconOptions(): ProjectIconOption[] {
  return Object.entries(LucideIcons)
    .filter(([name, value]) => /^[A-Z]/.test(name) && !name.endsWith("Icon") && name !== "Icon" && isLucideIconComponent(value))
    .map(([name, Icon]) => ({
      key: name,
      label: name,
      Icon: Icon as React.ComponentType<{ size?: number; className?: string }>,
    }))
    .sort((left, right) => left.label.localeCompare(right.label));
}

// isLucideIconComponent accepts both plain components and React.forwardRef icon objects.
// isLucideIconComponent 同时接受普通组件与 React.forwardRef 图标对象。
function isLucideIconComponent(value: unknown): boolean {
  if (typeof value === "function") {
    return true;
  }
  return Boolean(value && typeof value === "object" && "render" in value && typeof (value as { render?: unknown }).render === "function");
}

// ComposerSelectOption describes one local-only composer dropdown choice.
// ComposerSelectOption 描述一个仅用于输入区本地预览的下拉选项。
interface ComposerSelectOption {
  // Stable option value used by the shadcn Select state.
  // shadcn Select 状态使用的稳定选项值。
  value: string;
  // Primary label rendered in the compact trigger.
  // 紧凑触发器中渲染的主标签。
  label: string;
  // Optional secondary line matching the iced dual-line trigger subtitle.
  // 可选副标题，对齐 iced 双行触发器的第二行。
  subtitle?: string;
}

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

// TauriInvoke invokes a backend command when the app runs inside Tauri.
// TauriInvoke 在应用运行于 Tauri 内部时调用后端命令。
type TauriInvoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

// TauriListen subscribes to backend service events when running inside Tauri.
// TauriListen 在 Tauri 内运行时订阅后端服务事件。
type TauriListen = <T>(event: string, handler: (event: { payload: T }) => void) => Promise<() => void>;

// TauriWindowHandle exposes only the window APIs used by the custom title bar.
// TauriWindowHandle 仅暴露自定义标题栏使用的窗口 API。
interface TauriWindowHandle {
  // Minimizes the current desktop window.
  // 最小化当前桌面窗口。
  minimize: () => Promise<void>;
  // Toggles the current desktop window maximized state.
  // 切换当前桌面窗口最大化状态。
  toggleMaximize: () => Promise<void>;
  // Returns whether the current desktop window is maximized.
  // 返回当前桌面窗口是否处于最大化状态。
  isMaximized: () => Promise<boolean>;
  // Closes the current desktop window.
  // 关闭当前桌面窗口。
  close: () => Promise<void>;
  // Subscribes to native window resize events.
  // 订阅原生窗口尺寸变化事件。
  onResized: (handler: () => void) => Promise<() => void>;
}

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
  projects: [
    { id: "preview", name: "vulcan-code", path: "D:/openvulcan/vulcan-code", pinned: true },
    { id: "calc", name: "遗弃之地鬼仆计算器", path: "D:/openvulcan/abandoned-calculator", pinned: true },
    { id: "test-project", name: "测试项目", path: "D:/openvulcan/test-project", pinned: false },
    { id: "luaskills-packages", name: "luaskills-packages", path: "D:/openvulcan/luaskills-packages", pinned: false },
    { id: "vulcan-workmem", name: "vulcan-workmem", path: "D:/openvulcan/vulcan-workmem", pinned: false },
  ],
  conversations: [
    { id: "preview", title: "Tauri WebUI 迁移预览", status: "前端预览", projectId: "preview" },
    { id: "preview-intro", title: "介绍你自己", status: "刚刚", projectId: "preview" },
    { id: "calc-curl", title: "排查 lcurl.safe 模块缺失", status: "3 天", projectId: "calc" },
    { id: "calc-iconv", title: "检查是否缺少 iconv", status: "1 周", projectId: "calc" },
    { id: "test-tools", title: "你有哪些工具", status: "刚刚", projectId: "test-project" },
    { id: "test-playwright", title: "添加 Playwright 浏览器支持", status: "1 分", projectId: "test-project" },
    { id: "luaskills-rust", title: "分析 Rust 插件机制", status: "2 天", projectId: "luaskills-packages" },
    { id: "workmem-plan", title: "VMM 置顶项目对接方案", status: "5 小时", projectId: "vulcan-workmem" },
  ],
  selectedConversationId: "preview",
  messages: [
    { id: "preview-user", conversationId: "preview", role: "user", body: "新 WebUI 需要完整承接当前界面显示。" },
    {
      id: "preview-assistant",
      conversationId: "preview",
      role: "assistant",
      body: "当前页面展示工作台、设置、插件、VMM、Provider、工具和诊断区域的第一版布局。\n\n:::tool-call tool_name=service_startup_snapshot tool_call_id=preview-service-startup status=preview\n## Tool Call\n\n`service_startup_snapshot`\n\n## Arguments\n\n```json\n{}\n```\n:::\n\n```diff\ndiff --git a/apps/vulcan-code-web/src/main.tsx b/apps/vulcan-code-web/src/main.tsx\n+ 接入 Markdown 会话渲染\n+ 特殊工具块支持详情窗口\n```\n\n后续真实会话会把工具调用、命令输出和 diff 分块展示。",
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
    { id: "chat", label: "工作台", description: "项目会话、消息流与模型输入区", status: "可用" },
    { id: "skills", label: "技能", description: "LuaSkills、工具能力与技能包", status: "预览" },
    { id: "automation", label: "自动", description: "自动化任务、工作流与后台执行", status: "规划中" },
    { id: "plugins", label: "插件", description: "插件中心与 WebUI 控制面板", status: "预览" },
    { id: "memory", label: "记忆", description: "VMM 用户、项目与会话记忆", status: "预览" },
    { id: "settings", label: "设置", description: "项目、模型、工具、诊断与系统设置", status: "预览" },
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

// isTauriRuntime returns whether the WebUI is hosted by the desktop Tauri shell.
// isTauriRuntime 返回 WebUI 是否运行在 Tauri 桌面壳中。
function isTauriRuntime(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

// resolveInvoke loads the Tauri invoke function when available.
// resolveInvoke 在可用时加载 Tauri invoke 函数。
async function resolveInvoke(): Promise<TauriInvoke | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const api = await import("@tauri-apps/api/core");
  return api.invoke as TauriInvoke;
}

// resolveListen loads the Tauri event listener when available.
// resolveListen 在可用时加载 Tauri 事件监听器。
async function resolveListen(): Promise<TauriListen | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const api = await import("@tauri-apps/api/event");
  return api.listen as TauriListen;
}

// resolveCurrentWindow loads the Tauri window handle only inside the desktop shell.
// resolveCurrentWindow 仅在桌面壳内加载 Tauri 当前窗口句柄。
async function resolveCurrentWindow(): Promise<TauriWindowHandle | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const api = await import("@tauri-apps/api/window");
  return api.getCurrentWindow() as TauriWindowHandle;
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

// reorderProjects asks the desktop shell to persist one same-section project order.
// reorderProjects 请求桌面壳持久化同一分区内的项目顺序。
async function reorderProjects(projectIds: string[]): Promise<WorkbenchSnapshot | null> {
  const invoke = await resolveInvoke();
  if (!invoke) {
    return null;
  }
  return invoke<WorkbenchSnapshot>("service_reorder_projects", { projectIds });
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

// setProjectIcon asks the desktop shell to persist one project icon override.
// setProjectIcon 请求桌面壳持久化单个项目图标覆盖。
async function setProjectIcon(projectId: string, icon: ProjectIconSelection): Promise<WorkbenchSnapshot | null> {
  const invoke = await resolveInvoke();
  if (!invoke) {
    return null;
  }
  return invoke<WorkbenchSnapshot>("service_set_project_icon", {
    projectId,
    iconPack: icon.pack,
    iconKey: icon.key,
  });
}

// resetProjectIcon asks the desktop shell to clear one project icon override.
// resetProjectIcon 请求桌面壳清除单个项目图标覆盖。
async function resetProjectIcon(projectId: string): Promise<WorkbenchSnapshot | null> {
  const invoke = await resolveInvoke();
  if (!invoke) {
    return null;
  }
  return invoke<WorkbenchSnapshot>("service_reset_project_icon", { projectId });
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

// openExternalUrl opens a trusted external URL in the system browser when hosted by Tauri.
// openExternalUrl 在 Tauri 承载时使用系统默认浏览器打开可信外部链接。
async function openExternalUrl(url: string): Promise<void> {
  const invoke = await resolveInvoke();
  if (!invoke) {
    window.open(url, "_blank", "noopener,noreferrer");
    return;
  }
  await invoke("service_open_external_url", { url });
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
    <I18nProvider>
      <TooltipProvider>
        <WorkbenchLayout startup={startup} workbench={workbench} />
        <Toaster />
      </TooltipProvider>
    </I18nProvider>
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
  // i18n provides localized shell labels and the active language.
  // i18n 提供本地化外壳标签与当前语言。
  const i18n = useI18n();
  // localizedPrimaryFeatureAreas stores the translated top navigation metadata.
  // localizedPrimaryFeatureAreas 保存已翻译的顶部导航元数据。
  const localizedPrimaryFeatureAreas = React.useMemo(() => buildPrimaryFeatureAreas(i18n.t), [i18n.t]);
  // localizedSettingsFeatureArea stores the translated settings metadata.
  // localizedSettingsFeatureArea 保存已翻译的设置元数据。
  const localizedSettingsFeatureArea = React.useMemo<FeatureAreaSummary>(() => ({
    id: "settings",
    label: i18n.t("area.settings"),
    description: i18n.t("area.settings.description"),
    status: i18n.t("feature.status.preview"),
  }), [i18n]);
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
  // draftConversationIdRef keeps the latest draft id without retriggering workbench snapshot hydration.
  // draftConversationIdRef 保存最新草稿标识，避免重新触发工作台快照水合。
  const draftConversationIdRef = React.useRef<string | null>(null);
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
  // projectIconTarget stores the project currently editing its sidebar icon.
  // projectIconTarget 保存当前正在编辑侧栏图标的项目。
  const [projectIconTarget, setProjectIconTarget] = React.useState<ProjectIconDialogTarget | null>(null);
  // archiveProjectTarget stores the project waiting for archive confirmation.
  // archiveProjectTarget 保存正在等待归档确认的项目。
  const [archiveProjectTarget, setArchiveProjectTarget] = React.useState<ProjectSummary | null>(null);
  // pinnedConversationIds stores the locally pinned conversations until persistence is added.
  // pinnedConversationIds 保存本地置顶的会话，直到后续接入持久化。
  const [pinnedConversationIds, setPinnedConversationIds] = React.useState<Set<string>>(() => new Set());
  // activeSettingsSubArea stores the selected settings sidebar page.
  // activeSettingsSubArea 保存设置侧栏当前选择的页面。
  const [activeSettingsSubArea, setActiveSettingsSubArea] = React.useState<SettingsSubAreaId>("general");
  // hydratedConversationIds stores conversations already requested from the service.
  // hydratedConversationIds 保存已经向服务请求过正文的会话标识。
  const [hydratedConversationIds, setHydratedConversationIds] = React.useState<Set<string>>(
    () => new Set(props.workbench.messages.map((message) => message.conversationId)),
  );
  // activeFeature stores the selected feature summary, including settings opened from the global button.
  // activeFeature 保存当前选中的功能区域摘要，并包含从全局按钮打开的设置页。
  const activeFeature = activeArea === "settings"
    ? localizedSettingsFeatureArea
    : localizedPrimaryFeatureAreas.find((area) => area.id === activeArea) ?? localizedPrimaryFeatureAreas[0];
  // selectedConversation stores the conversation used by the console header.
  // selectedConversation 保存控制台标题栏使用的当前会话。
  const selectedConversation = workbenchState.conversations.find((conversation) => conversation.id === selectedConversationId);
  // selectedDraftConversation stores the draft used by the console header and empty state.
  // selectedDraftConversation 保存控制台标题栏与空状态使用的草稿会话。
  const selectedDraftConversation = selectedConversationId === draftConversation?.id ? draftConversation : null;
  // showConsoleSidebar limits project and conversation context to the console view.
  // showConsoleSidebar 将项目与会话上下文限定在控制台页面内。
  const showConsoleSidebar = activeArea === "chat";
  // showWindowControls enables the custom titlebar only inside the Tauri desktop shell.
  // showWindowControls 仅在 Tauri 桌面壳内启用自定义标题栏。
  const showWindowControls = isTauriRuntime();
  // isWindowMaximized tracks the current Tauri window maximize state for the titlebar button.
  // isWindowMaximized 跟踪当前 Tauri 窗口最大化状态，用于标题栏按钮。
  const [isWindowMaximized, setIsWindowMaximized] = React.useState(false);
  // refreshWindowMaximizedState synchronizes the local titlebar state with Tauri.
  // refreshWindowMaximizedState 将本地标题栏状态与 Tauri 同步。
  const refreshWindowMaximizedState = React.useCallback(async () => {
    const windowHandle = await resolveCurrentWindow();
    if (!windowHandle) {
      return;
    }
    setIsWindowMaximized(await windowHandle.isMaximized());
  }, []);
  // toggleWindowMaximize toggles native maximize state and refreshes the icon.
  // toggleWindowMaximize 切换原生最大化状态并刷新图标。
  const toggleWindowMaximize = React.useCallback(async () => {
    const windowHandle = await resolveCurrentWindow();
    if (!windowHandle) {
      return;
    }
    await windowHandle.toggleMaximize();
    window.setTimeout(() => void refreshWindowMaximizedState(), 40);
  }, [refreshWindowMaximizedState]);
  React.useEffect(() => {
    if (!showWindowControls) {
      return;
    }
    void refreshWindowMaximizedState();
  }, [refreshWindowMaximizedState, showWindowControls]);

  React.useEffect(() => {
    if (!showWindowControls) {
      return;
    }

    let cancelled = false;
    let unlisten: null | (() => void) = null;

    // subscribeWindowResize keeps the restore icon aligned when native drag-region double click changes state.
    // subscribeWindowResize 在原生拖动区域双击改变窗口状态后同步还原图标。
    async function subscribeWindowResize() {
      const windowHandle = await resolveCurrentWindow();
      if (!windowHandle || cancelled) {
        return;
      }
      unlisten = await windowHandle.onResized(() => {
        void refreshWindowMaximizedState();
      });
    }

    void subscribeWindowResize();
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [refreshWindowMaximizedState, showWindowControls]);

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

      if (action === "set-icon") {
        setProjectIconTarget({ project });
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

  // handleProjectIconConfirm persists the selected project icon override.
  // handleProjectIconConfirm 持久化选择的项目图标覆盖。
  const handleProjectIconConfirm = React.useCallback(async (project: ProjectSummary, icon: ProjectIconSelection) => {
    const normalizedIcon = icon.pack === "badge"
      ? { pack: icon.pack, key: encodeProjectBadgeKey(decodeProjectBadgeKey(icon.key) ?? DEFAULT_PROJECT_BADGE_CONFIG) }
      : icon;
    try {
      const snapshot = await setProjectIcon(project.id, normalizedIcon);
      if (snapshot) {
        applyProjectActionSnapshot(snapshot);
      } else {
        applyProjectActionSnapshot(applyPreviewProjectIcon(workbenchState, project.id, normalizedIcon).snapshot);
      }
      setProjectIconTarget(null);
      toast.success(`已更新项目图标：${project.name}`);
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    }
  }, [applyProjectActionSnapshot, workbenchState]);

  // handleProjectIconReset clears the selected project icon override.
  // handleProjectIconReset 清除选择的项目图标覆盖。
  const handleProjectIconReset = React.useCallback(async (project: ProjectSummary) => {
    try {
      const snapshot = await resetProjectIcon(project.id);
      if (snapshot) {
        applyProjectActionSnapshot(snapshot);
      } else {
        applyProjectActionSnapshot(applyPreviewProjectIcon(workbenchState, project.id, null).snapshot);
      }
      setProjectIconTarget(null);
      toast.success(`已还原项目图标：${project.name}`);
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
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

  // handleCreateProject opens the future add-project flow from the sidebar heading.
  // handleCreateProject 从侧栏标题入口打开未来的添加项目流程。
  const handleCreateProject = React.useCallback(() => {
    toast.info("添加项目入口已预留，后续接入项目选择与工作区注册流程。");
  }, []);

  // handleReorderProjects persists a same-section project order using the existing order model.
  // handleReorderProjects 使用现有顺序模型持久化同一分区内的项目排序。
  const handleReorderProjects = React.useCallback(async (projectIds: string[]) => {
    setWorkbenchState((current) => applyPreviewProjectOrder(current, projectIds).snapshot);
    try {
      const snapshot = await reorderProjects(projectIds);
      if (snapshot) {
        applyProjectActionSnapshot(snapshot);
      }
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
      try {
        const snapshot = await loadWorkbenchSnapshot();
        applyProjectActionSnapshot(snapshot);
      } catch {
        // Keep the optimistic order when the refresh fallback is unavailable.
        // 刷新回退不可用时保留乐观排序结果。
      }
    }
  }, [applyProjectActionSnapshot]);

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
    draftConversationIdRef.current = draftConversation?.id ?? null;
  }, [draftConversation?.id]);

  React.useEffect(() => {
    setWorkbenchState((current) => mergeWorkbenchSnapshot(current, props.workbench));
    const nextSelected = props.workbench.selectedConversationId ?? props.workbench.conversations[0]?.id ?? null;
    setSelectedConversationId((current) => {
      if (current && current === draftConversationIdRef.current) {
        return current;
      }
      if (current && props.workbench.conversations.some((conversation) => conversation.id === current)) {
        return current;
      }
      return nextSelected;
    });
  }, [props.workbench]);

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
    <div className={`app-shell ${showConsoleSidebar ? "with-console-sidebar" : "without-console-sidebar"} ${showWindowControls ? "tauri-shell" : "browser-shell"}`}>
      <header
        className="main-menu"
        aria-label="主菜单"
      >
        <BrandHeader dragRegion={showWindowControls} />
        <FeatureNav
          areas={localizedPrimaryFeatureAreas}
          activeArea={activeArea}
          onSelect={setActiveArea}
        />
        {showWindowControls ? <div className="titlebar-drag-region" data-tauri-drag-region /> : null}
        <GlobalActions
          activeArea={activeArea}
          startup={props.startup}
          theme={theme}
          locale={i18n.locale}
          onChangeLocale={i18n.setLocale}
          onToggleTheme={() => setTheme((current) => current === "dark" ? "light" : "dark")}
          onOpenSettings={() => setActiveArea("settings")}
        />
        {showWindowControls ? (
          <WindowControls
            isMaximized={isWindowMaximized}
            onMaximizeToggle={toggleWindowMaximize}
            onMaximizeStateRefresh={refreshWindowMaximizedState}
          />
        ) : null}
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
            onCreateProject={handleCreateProject}
            onProjectMenuAction={handleProjectMenuAction}
            onReorderProjects={handleReorderProjects}
            onSelectConversation={setSelectedConversationId}
          />
        ) : (
          <ExtractedFeatureShell
            activeArea={activeArea}
            activeFeature={activeFeature}
            activeSettingsSubArea={activeSettingsSubArea}
            onSelectSettingsSubArea={setActiveSettingsSubArea}
            startup={props.startup}
            workbench={workbenchState}
          />
        )}
      </main>
      <footer className="statusbar">
        <span>版本 {props.workbench.appVersion}</span>
        <span>Core Service: {props.startup.phase}</span>
        <RepositoryLink />
      </footer>
      <RenameDialog
        target={renameTarget}
        onCancel={() => setRenameTarget(null)}
        onConfirm={handleRenameConfirm}
      />
      <ProjectIconDialog
        target={projectIconTarget}
        onCancel={() => setProjectIconTarget(null)}
        onConfirm={handleProjectIconConfirm}
        onReset={handleProjectIconReset}
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

// WindowControls renders Tauri-only custom minimize, maximize, and close buttons.
// WindowControls 渲染仅 Tauri 使用的自定义最小化、最大化与关闭按钮。
function WindowControls(props: {
  // Whether the current Tauri window is maximized.
  // 当前 Tauri 窗口是否处于最大化状态。
  isMaximized: boolean;
  // Toggles maximize or restore using the shared titlebar behavior.
  // 使用共享标题栏行为切换最大化或还原。
  onMaximizeToggle: () => Promise<void>;
  // Refreshes maximize state after native window actions.
  // 在原生窗口操作后刷新最大化状态。
  onMaximizeStateRefresh: () => Promise<void>;
}) {
  // runWindowAction invokes one native window command and reports failures as toasts.
  // runWindowAction 调用单个原生窗口命令，并用 toast 报告失败。
  const runWindowAction = React.useCallback(async (action: "minimize" | "close") => {
    const windowHandle = await resolveCurrentWindow();
    if (!windowHandle) {
      return;
    }
    try {
      await windowHandle[action]();
      await props.onMaximizeStateRefresh();
    } catch (error) {
      toast.error(`窗口操作失败：${error instanceof Error ? error.message : String(error)}`);
    }
  }, [props]);

  // handleMaximizeClick shares the same maximize/restore behavior as titlebar double-click.
  // handleMaximizeClick 与标题栏双击共享同一套最大化/还原行为。
  const handleMaximizeClick = React.useCallback(async () => {
    try {
      await props.onMaximizeToggle();
    } catch (error) {
      toast.error(`窗口操作失败：${error instanceof Error ? error.message : String(error)}`);
    }
  }, [props]);

  return (
    <div className="window-controls" data-no-drag>
      <Button
        aria-label="最小化窗口"
        className="window-control-button"
        onClick={() => void runWindowAction("minimize")}
        size="icon"
        type="button"
        variant="ghost"
      >
        <MinusIcon size={16} />
      </Button>
      <Button
        aria-label={props.isMaximized ? "还原窗口" : "最大化窗口"}
        className="window-control-button"
        onClick={() => void handleMaximizeClick()}
        size="icon"
        type="button"
        variant="ghost"
      >
        {props.isMaximized ? <Minimize2 size={15} /> : <SquareIcon size={13} />}
      </Button>
      <Button
        aria-label="关闭窗口"
        className="window-control-button close"
        onClick={() => void runWindowAction("close")}
        size="icon"
        type="button"
        variant="ghost"
      >
        <X size={16} />
      </Button>
    </div>
  );
}

// RepositoryLink renders the statusbar GitHub repository link.
// RepositoryLink 渲染状态栏 GitHub 仓库链接。
function RepositoryLink() {
  // handleClick opens the repository externally in Tauri and in a new tab in browsers.
  // handleClick 在 Tauri 中外部打开仓库，在浏览器中打开新标签。
  const handleClick = React.useCallback((event: React.MouseEvent<HTMLAnchorElement>) => {
    event.preventDefault();
    void openExternalUrl(GITHUB_REPOSITORY_URL).catch((error) => {
      toast.error(`打开 GitHub 失败：${error instanceof Error ? error.message : String(error)}`);
    });
  }, []);

  return (
    <a
      aria-label="打开 OpenVulcan/vulcan-code GitHub 仓库"
      className="repository-link"
      href={GITHUB_REPOSITORY_URL}
      onClick={handleClick}
      rel="noreferrer"
      target="_blank"
    >
      <GitHubMark />
      <span>OpenVulcan/vulcan-code</span>
    </a>
  );
}

// GitHubMark renders the filled GitHub logo mark used in the statusbar.
// GitHubMark 渲染状态栏使用的填充版 GitHub 标识。
function GitHubMark() {
  return (
    <svg
      aria-hidden="true"
      className="repository-github-mark"
      focusable="false"
      viewBox="0 0 16 16"
    >
      <path
        fill="currentColor"
        d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82A7.65 7.65 0 0 1 8 3.87c.68 0 1.36.09 2 .26 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.01 8.01 0 0 0 16 8c0-4.42-3.58-8-8-8Z"
      />
    </svg>
  );
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
type ProjectMenuAction = "toggle-pin" | "open-directory" | "rename" | "set-icon" | "archive-conversations";

// ProjectIconDialogTarget stores the project currently editing its icon.
// ProjectIconDialogTarget 保存当前正在编辑图标的项目。
interface ProjectIconDialogTarget {
  // Project being edited by the icon dialog.
  // 图标弹窗正在编辑的项目。
  project: ProjectSummary;
}

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

// applyPreviewProjectIcon simulates project icon persistence for browser-only preview mode.
// applyPreviewProjectIcon 为纯浏览器预览模式模拟项目图标持久化。
function applyPreviewProjectIcon(
  workbench: WorkbenchSnapshot,
  projectId: string,
  icon: ProjectIconSelection | null,
): { snapshot: WorkbenchSnapshot } {
  return {
    snapshot: {
      ...workbench,
      projects: workbench.projects.map((candidate) => candidate.id === projectId ? {
        ...candidate,
        iconPack: icon?.pack ?? null,
        iconKey: icon?.key ?? null,
      } : candidate),
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

// applyPreviewProjectOrder simulates same-section project ordering in browser preview mode.
// applyPreviewProjectOrder 在浏览器预览模式下模拟同分区项目排序。
function applyPreviewProjectOrder(workbench: WorkbenchSnapshot, orderedProjectIds: string[]): { snapshot: WorkbenchSnapshot } {
  if (orderedProjectIds.length === 0) {
    return { snapshot: workbench };
  }

  const orderedSet = new Set(orderedProjectIds);
  const sectionPinned = Boolean(workbench.projects.find((project) => project.id === orderedProjectIds[0])?.pinned);
  const orderedProjects: ProjectSummary[] = [];
  for (const projectId of orderedProjectIds) {
    const project = workbench.projects.find((candidate) => candidate.id === projectId);
    if (project && Boolean(project.pinned) === sectionPinned) {
      orderedProjects.push(project);
    }
  }
  const remainingSectionProjects = workbench.projects.filter((project) => (
    Boolean(project.pinned) === sectionPinned && !orderedSet.has(project.id)
  ));
  const oppositeSectionProjects = workbench.projects.filter((project) => Boolean(project.pinned) !== sectionPinned);
  const nextSectionProjects = [...orderedProjects, ...remainingSectionProjects];
  const nextProjects = sectionPinned
    ? [...nextSectionProjects, ...oppositeSectionProjects]
    : [...oppositeSectionProjects, ...nextSectionProjects];
  return { snapshot: { ...workbench, projects: nextProjects } };
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

// encodeProjectBadgeKey serializes a generated badge config for project persistence.
// encodeProjectBadgeKey 将生成式徽标配置序列化用于项目持久化。
function encodeProjectBadgeKey(config: ProjectIconBadgeConfig): string {
  return JSON.stringify(config);
}

// decodeProjectBadgeKey restores a generated badge config from persistence.
// decodeProjectBadgeKey 从持久化内容还原生成式徽标配置。
function decodeProjectBadgeKey(value?: string | null): ProjectIconBadgeConfig | null {
  if (!value) {
    return null;
  }
  try {
    const parsed = JSON.parse(value) as Partial<ProjectIconBadgeConfig>;
    const shape = parsed.shape;
    const text = normalizeProjectBadgeText(parsed.text ?? "");
    if (!shape || !["circle", "square", "rounded", "sharp"].includes(shape) || !text) {
      return null;
    }
    return {
      shape,
      text,
      background: normalizeCssColor(parsed.background, DEFAULT_PROJECT_BADGE_CONFIG.background),
      border: normalizeCssColor(parsed.border, DEFAULT_PROJECT_BADGE_CONFIG.border),
      foreground: normalizeCssColor(parsed.foreground, DEFAULT_PROJECT_BADGE_CONFIG.foreground),
    };
  } catch {
    return null;
  }
}

// ProjectIcon renders either a generated badge, a selected icon, or the default folder state.
// ProjectIcon 渲染生成式徽标、已选图标或默认文件夹状态。
function ProjectIcon(props: { project: ProjectSummary; isOpen?: boolean; size?: number }) {
  if (props.project.iconPack === "badge") {
    const config = decodeProjectBadgeKey(props.project.iconKey) ?? DEFAULT_PROJECT_BADGE_CONFIG;
    return <ProjectBadgeIcon config={config} size={props.size ?? 16} />;
  }

  if (props.project.iconPack === "lucide" && props.project.iconKey) {
    const option = PROJECT_ICON_MAP.get(props.project.iconKey);
    if (option) {
      return <option.Icon size={props.size ?? 16} />;
    }
  }

  return props.isOpen ? <FolderOpen size={props.size ?? 16} /> : <Folder size={props.size ?? 16} />;
}

// ProjectBadgeIcon renders the generated project badge as an inline SVG.
// ProjectBadgeIcon 将生成式项目徽标渲染为内联 SVG。
function ProjectBadgeIcon(props: { config: ProjectIconBadgeConfig; size: number }) {
  const radius = props.config.shape === "circle"
    ? 50
    : props.config.shape === "rounded"
      ? 18
      : props.config.shape === "square"
        ? 8
        : 0;
  return (
    <svg
      aria-hidden="true"
      className="project-badge-icon"
      height={props.size}
      viewBox="0 0 32 32"
      width={props.size}
    >
      <rect
        fill={props.config.background}
        height="28"
        rx={radius}
        stroke={props.config.border}
        strokeWidth="2"
        width="28"
        x="2"
        y="2"
      />
      <text
        dominantBaseline="central"
        fill={props.config.foreground}
        fontFamily="Inter, Segoe UI, sans-serif"
        fontSize={props.config.text.length > 1 ? 12 : 15}
        fontWeight="700"
        textAnchor="middle"
        x="16"
        y="16.5"
      >
        {props.config.text}
      </text>
    </svg>
  );
}

// normalizeProjectBadgeText keeps one Chinese character or two Latin letters.
// normalizeProjectBadgeText 保留一个中文字符或两个英文字母。
function normalizeProjectBadgeText(value: string): string {
  const trimmed = value.trim();
  const chinese = Array.from(trimmed).find((char) => /\p{Script=Han}/u.test(char));
  if (chinese) {
    return chinese;
  }
  return trimmed.replace(/[^a-zA-Z]/g, "").slice(0, 2).toUpperCase();
}

// normalizeCssColor keeps a simple hex color or falls back to a known value.
// normalizeCssColor 保留简单十六进制颜色，或回退到已知值。
function normalizeCssColor(value: unknown, fallback: string): string {
  if (typeof value !== "string") {
    return fallback;
  }
  return /^#[0-9a-fA-F]{6}$/.test(value) ? value : fallback;
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
function BrandHeader(props: {
  // Whether the brand block should behave as a native Tauri drag region.
  // 品牌块是否应作为 Tauri 原生拖动区域。
  dragRegion?: boolean;
}) {
  // t translates shell labels used by the brand block.
  // t 翻译品牌区域使用的外壳标签。
  const { t } = useI18n();
  return (
    <div className="brand" data-tauri-drag-region={props.dragRegion ? "" : undefined}>
      <div className="brand-mark">V</div>
      <div>
        <strong>Vulcan Code</strong>
        <span>{t("app.subtitle")}</span>
      </div>
    </div>
  );
}

// FeatureNav renders the primary workbench navigation.
// FeatureNav 渲染工作台主导航。
function FeatureNav(props: { areas: FeatureAreaSummary[]; activeArea: MainAreaId; onSelect: (area: MainAreaId) => void }) {
  // t translates navigation accessibility labels.
  // t 翻译导航无障碍标签。
  const { t } = useI18n();
  // scrollRef points to the horizontal navigation viewport.
  // scrollRef 指向横向导航视口。
  const scrollRef = React.useRef<HTMLElement | null>(null);
  // navOverflow stores whether each arrow can move the navigation viewport.
  // navOverflow 保存左右箭头是否还能移动导航视口。
  const [navOverflow, setNavOverflow] = React.useState({ left: false, right: false });

  // updateNavOverflow refreshes arrow enabled state from the scroll viewport.
  // updateNavOverflow 根据滚动视口刷新箭头启用状态。
  const updateNavOverflow = React.useCallback(() => {
    const element = scrollRef.current;
    if (!element) {
      setNavOverflow({ left: false, right: false });
      return;
    }
    const maxScrollLeft = element.scrollWidth - element.clientWidth;
    setNavOverflow({
      left: element.scrollLeft > 2,
      right: element.scrollLeft < maxScrollLeft - 2,
    });
  }, []);

  // scrollNav moves the navigation viewport by one compact page.
  // scrollNav 按一个紧凑页宽移动导航视口。
  const scrollNav = React.useCallback((direction: -1 | 1) => {
    const element = scrollRef.current;
    if (!element) {
      return;
    }
    element.scrollBy({ left: direction * Math.max(120, element.clientWidth * 0.7), behavior: "smooth" });
    window.setTimeout(updateNavOverflow, 180);
  }, [updateNavOverflow]);

  React.useEffect(() => {
    updateNavOverflow();
    window.addEventListener("resize", updateNavOverflow);
    return () => window.removeEventListener("resize", updateNavOverflow);
  }, [props.areas.length, updateNavOverflow]);

  return (
    <div className="feature-nav-shell">
      {navOverflow.left ? (
        <Button
          aria-label="Scroll main features left"
          className="feature-nav-arrow"
          onClick={() => scrollNav(-1)}
          size="icon-sm"
          type="button"
          variant="ghost"
        >
          <ChevronLeft size={15} />
        </Button>
      ) : null}
      <nav className="feature-nav" aria-label={t("sidebar.overview")} onScroll={updateNavOverflow} ref={scrollRef}>
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
      {navOverflow.right ? (
        <Button
          aria-label="Scroll main features right"
          className="feature-nav-arrow"
          onClick={() => scrollNav(1)}
          size="icon-sm"
          type="button"
          variant="ghost"
        >
          <ChevronRight size={15} />
        </Button>
      ) : null}
    </div>
  );
}

// GlobalActions renders app-wide controls in the primary menu bar.
// GlobalActions 渲染主菜单栏中的全局控制。
function GlobalActions(props: {
  // Active top-level area used to lock the settings button visual state.
  // 当前顶层区域，用于锁定设置按钮视觉状态。
  activeArea: MainAreaId;
  // Startup snapshot used for the compact online indicator.
  // 紧凑在线状态指示使用的启动快照。
  startup: StartupSnapshot;
  // Current shadcn theme mode.
  // 当前 shadcn 主题模式。
  theme: ThemeMode;
  // Current UI language.
  // 当前 UI 语言。
  locale: LocaleCode;
  // Updates the current UI language.
  // 更新当前 UI 语言。
  onChangeLocale: (locale: LocaleCode) => void;
  // Toggles between light and dark theme.
  // 在亮色与暗色主题之间切换。
  onToggleTheme: () => void;
  // Opens the settings feature area.
  // 打开设置功能区域。
  onOpenSettings: () => void;
}) {
  // t translates global action labels.
  // t 翻译全局操作标签。
  const { t } = useI18n();
  // isPreviewMode detects fallback browser data instead of a live service bridge.
  // isPreviewMode 检测当前是否为浏览器回退预览数据而非真实服务桥接。
  const isPreviewMode = props.startup.phase === "browser-preview";
  // statusLabel translates internal startup phases into compact user-facing labels.
  // statusLabel 将内部启动阶段转换为面向用户的紧凑显示文案。
  const statusLabel = isPreviewMode ? t("global.preview") : props.startup.ready ? t("global.online") : props.startup.phase;
  // statusKind keeps preview visually distinct from a live ready service.
  // statusKind 让预览状态与真实在线服务在视觉上保持区分。
  const statusKind = isPreviewMode ? "warning" : props.startup.ready ? "ready" : "running";

  return (
    <div className="global-actions" data-no-drag>
      <StatusPill status={statusKind} label={statusLabel} />
      <Button
        aria-label={props.theme === "dark" ? t("global.toggleLight") : t("global.toggleDark")}
        className="theme-toggle"
        onClick={props.onToggleTheme}
        size="icon"
        type="button"
        variant="outline"
      >
        {props.theme === "dark" ? <Moon size={16} /> : <Sun size={16} />}
      </Button>
      <Select value={props.locale} onValueChange={(value) => props.onChangeLocale(value as LocaleCode)}>
        <SelectTrigger aria-label={t("global.language")} className="language-select-trigger">
          <span>{props.locale === "zh-CN" ? "中" : "EN"}</span>
        </SelectTrigger>
        <SelectContent align="end" position="popper">
          <SelectItem value="zh-CN">中</SelectItem>
          <SelectItem value="en-US">EN</SelectItem>
        </SelectContent>
      </Select>
      <Button
        aria-pressed={props.activeArea === "settings"}
        aria-label={t("global.openSettings")}
        className="icon-button"
        onClick={props.onOpenSettings}
        size="icon"
        type="button"
        variant={props.activeArea === "settings" ? "secondary" : "outline"}
      >
        <Settings size={18} />
      </Button>
    </div>
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
  onCreateProject: () => void;
  onProjectMenuAction: (action: ProjectMenuAction, project: ProjectSummary) => void;
  onReorderProjects: (projectIds: string[]) => void;
  onSelectConversation: (conversationId: string) => void;
}) {
  return (
    <SidebarProvider className="console-shell">
      <ConsoleShellContent {...props} />
    </SidebarProvider>
  );
}

// ConsoleShellContent renders the workspace shell after sidebar context is available.
// ConsoleShellContent 在侧栏上下文可用后渲染工作台外壳。
function ConsoleShellContent(props: {
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
  onCreateProject: () => void;
  onProjectMenuAction: (action: ProjectMenuAction, project: ProjectSummary) => void;
  onReorderProjects: (projectIds: string[]) => void;
  onSelectConversation: (conversationId: string) => void;
}) {
  // sidebar controls the left project tree that can collapse automatically.
  // sidebar 控制可自动折叠的左侧项目树。
  const sidebar = useSidebar();
  // shellRef measures the available workspace width for automatic panel collapse.
  // shellRef 测量工作台可用宽度，用于自动折叠面板。
  const shellRef = React.useRef<HTMLDivElement | null>(null);
  // previousShellWidthRef tracks resize direction so auto-restore only runs while width grows.
  // previousShellWidthRef 跟踪尺寸变化方向，确保自动恢复只在宽度增长时执行。
  const previousShellWidthRef = React.useRef<number | null>(null);
  // rightPanelOpen stores whether the review and memory panel is visible.
  // rightPanelOpen 保存审核与记忆面板是否可见。
  const [rightPanelOpen, setRightPanelOpen] = React.useState(false);
  // leftAutoCollapsed records automatic left-sidebar collapse caused by width pressure.
  // leftAutoCollapsed 记录宽度压力导致的左侧栏自动折叠。
  const [leftAutoCollapsed, setLeftAutoCollapsed] = React.useState(false);
  // rightAutoCollapsed records automatic right-panel collapse caused by width pressure.
  // rightAutoCollapsed 记录宽度压力导致的右侧面板自动折叠。
  const [rightAutoCollapsed, setRightAutoCollapsed] = React.useState(false);
  // handleLeftPanelToggle clears automatic restore intent when the user toggles the left sidebar manually.
  // handleLeftPanelToggle 在用户手动切换左侧栏时清除自动恢复意图。
  const handleLeftPanelToggle = React.useCallback(() => {
    setLeftAutoCollapsed(false);
  }, []);
  // handleRightPanelToggle clears automatic restore intent when the user toggles the right panel manually.
  // handleRightPanelToggle 在用户手动切换右侧面板时清除自动恢复意图。
  const handleRightPanelToggle = React.useCallback(() => {
    setRightAutoCollapsed(false);
    setRightPanelOpen((current) => !current);
  }, []);

  React.useEffect(() => {
    const element = shellRef.current;
    if (!element || typeof ResizeObserver === "undefined") {
      return;
    }
    const observer = new ResizeObserver(([entry]) => {
      const width = entry.contentRect.width;
      const previousWidth = previousShellWidthRef.current;
      const isGrowing = previousWidth !== null && width > previousWidth + 1;
      previousShellWidthRef.current = width;
      const expandedLeftWidth = 288;
      const collapsedLeftWidth = 46;
      const preferredRightWidth = 480;
      const handleWidth = 9;
      const leftWidth = sidebar.open ? expandedLeftWidth : collapsedLeftWidth;
      const rightWidth = rightPanelOpen ? preferredRightWidth : 0;
      const shellGaps = rightPanelOpen ? handleWidth : 0;
      const mainWidth = width - leftWidth - rightWidth - shellGaps;
      if (mainWidth < 800 && sidebar.open) {
        sidebar.setOpen(false);
        setLeftAutoCollapsed(true);
        return;
      }
      if (mainWidth < 800 && rightPanelOpen) {
        setRightPanelOpen(false);
        setRightAutoCollapsed(true);
        return;
      }
      if (!isGrowing) {
        return;
      }
      if (leftAutoCollapsed && !sidebar.open) {
        const restoredMainWidth = width - expandedLeftWidth - rightWidth - shellGaps;
        if (restoredMainWidth >= 800) {
          sidebar.setOpen(true);
          setLeftAutoCollapsed(false);
          return;
        }
      }
      if (rightAutoCollapsed && !rightPanelOpen) {
        const restoredMainWidth = width - leftWidth - preferredRightWidth - handleWidth;
        if (restoredMainWidth >= 800) {
          setRightPanelOpen(true);
          setRightAutoCollapsed(false);
        }
      }
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, [leftAutoCollapsed, rightAutoCollapsed, rightPanelOpen, sidebar]);

  return (
    <div className="workspace-panel-shell" ref={shellRef}>
      <ProjectSidebar
        projects={props.workbench.projects}
        conversations={props.workbench.conversations}
        pinnedConversationIds={props.pinnedConversationIds}
        selectedConversationId={props.selectedConversationId}
        onConversationMenuAction={props.onConversationMenuAction}
        onCreateConversation={props.onCreateConversation}
        onCreateProject={props.onCreateProject}
        onProjectMenuAction={props.onProjectMenuAction}
        onReorderProjects={props.onReorderProjects}
        onSelectConversation={props.onSelectConversation}
      />
      <SidebarInset className="workspace-inset">
        <WorkspaceResizableBody
          rightPanelOpen={rightPanelOpen}
          startup={props.startup}
          workbench={props.workbench}
          selectedDraftConversation={props.selectedDraftConversation}
          selectedConversation={props.selectedConversation}
          selectedConversationId={props.selectedConversationId}
          loadingConversationId={props.loadingConversationId}
          conversationLoadError={props.conversationLoadError}
          onToggleLeftPanel={handleLeftPanelToggle}
          onToggleRightPanel={handleRightPanelToggle}
        />
      </SidebarInset>
    </div>
  );
}

// ConsoleHeader renders the inset title bar for the current conversation.
// ConsoleHeader 渲染当前会话的 inset 标题栏。
function ConsoleHeader(props: {
  conversation?: ConversationSummary;
  draftConversation: DraftConversationSelection | null;
  rightPanelOpen: boolean;
  onToggleLeftPanel: () => void;
  onToggleRightPanel: () => void;
}) {
  // t translates stable console chrome labels.
  // t 翻译稳定的控制台外壳标签。
  const { t } = useI18n();
  // rightPanelLabel describes the current right panel toggle action.
  // rightPanelLabel 描述当前右侧面板切换动作。
  const rightPanelLabel = props.rightPanelOpen ? t("contextPanel.close") : t("contextPanel.open");

  return (
    <header className="console-header">
      <div className="console-header-main">
        <SidebarTrigger onClick={props.onToggleLeftPanel} />
        <span className="console-header-separator" aria-hidden="true" />
        <h1>{props.draftConversation ? t("console.newConversation") : props.conversation?.title ?? t("console.title")}</h1>
      </div>
      <div className="console-header-actions">
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              aria-label={rightPanelLabel}
              aria-pressed={props.rightPanelOpen}
              className="icon-button"
              onClick={props.onToggleRightPanel}
              size="icon-sm"
              type="button"
              variant={props.rightPanelOpen ? "secondary" : "ghost"}
            >
              {props.rightPanelOpen ? <PanelRightClose size={16} /> : <PanelRightOpen size={16} />}
            </Button>
          </TooltipTrigger>
          <TooltipContent side="bottom">{rightPanelLabel}</TooltipContent>
        </Tooltip>
      </div>
    </header>
  );
}

// WorkspaceResizableBody renders the main chat area and optional right context panel.
// WorkspaceResizableBody 渲染主会话区域与可选右侧上下文面板。
function WorkspaceResizableBody(props: {
  // Whether the right context panel is visible.
  // 右侧上下文面板是否可见。
  rightPanelOpen: boolean;
  // Startup snapshot forwarded to the chat area.
  // 转发给会话区域的启动快照。
  startup: StartupSnapshot;
  // Workbench snapshot forwarded to the chat area.
  // 转发给会话区域的工作台快照。
  workbench: WorkbenchSnapshot;
  // Current draft conversation selection.
  // 当前草稿会话选择。
  selectedDraftConversation: DraftConversationSelection | null;
  // Current selected conversation summary.
  // 当前选中会话摘要。
  selectedConversation?: ConversationSummary;
  // Current selected conversation id.
  // 当前选中会话标识。
  selectedConversationId: string | null;
  // Conversation id currently loading from service.
  // 当前正在从服务载入的会话标识。
  loadingConversationId: string | null;
  // Conversation load error shown by the chat area.
  // 会话区域展示的载入错误。
  conversationLoadError: string | null;
  // Toggles the right context panel.
  // 切换右侧上下文面板。
  onToggleRightPanel: () => void;
  // Records a manual left sidebar toggle.
  // 记录一次手动左侧栏切换。
  onToggleLeftPanel: () => void;
}) {
  return (
    <ResizablePanelGroup className="workspace-resizable-body" defaultLayout={{ workspaceMain: 68, workspaceContext: 32 }} orientation="horizontal">
      <ResizablePanel className="workspace-main-panel" defaultSize={props.rightPanelOpen ? 68 : 100} id="workspaceMain" minSize="800px">
        <section className="workspace-main-shell">
          <ConsoleHeader
            draftConversation={props.selectedDraftConversation}
            conversation={props.selectedConversation}
            rightPanelOpen={props.rightPanelOpen}
            onToggleLeftPanel={props.onToggleLeftPanel}
            onToggleRightPanel={props.onToggleRightPanel}
          />
          <ChatArea
            startup={props.startup}
            workbench={props.workbench}
            selectedDraftConversation={props.selectedDraftConversation}
            selectedConversationId={props.selectedConversationId}
            loadingConversationId={props.loadingConversationId}
            conversationLoadError={props.conversationLoadError}
          />
        </section>
      </ResizablePanel>
      {props.rightPanelOpen ? (
        <>
          <ResizableHandle className="workspace-context-resize-handle" withHandle />
          <ResizablePanel
            className="workspace-context-panel"
            defaultSize={32}
            groupResizeBehavior="preserve-pixel-size"
            id="workspaceContext"
            minSize="320px"
          >
            <WorkspaceContextPanel />
          </ResizablePanel>
        </>
      ) : null}
    </ResizablePanelGroup>
  );
}

// WorkspaceContextPanel renders review and memory tabs for the right context rail.
// WorkspaceContextPanel 渲染右侧上下文栏中的审核与记忆标签页。
function WorkspaceContextPanel() {
  // t translates the right context panel shell labels.
  // t 翻译右侧上下文面板外壳标签。
  const { t } = useI18n();

  return (
    <aside className="context-panel-shell">
      <Tabs className="context-tabs" defaultValue="review">
        <div className="context-panel-header">
          <TabsList className="context-tabs-list">
            <TabsTrigger value="review">{t("contextPanel.review")}</TabsTrigger>
            <TabsTrigger value="memory">{t("contextPanel.memory")}</TabsTrigger>
          </TabsList>
        </div>
        <TabsContent className="context-tab-content" value="review">
          <div className="context-empty-state">
            <GitBranch size={20} />
            <strong>{t("contextPanel.review")}</strong>
            <span>{t("contextPanel.reviewEmpty")}</span>
          </div>
        </TabsContent>
        <TabsContent className="context-tab-content" value="memory">
          <div className="context-empty-state">
            <BrainCircuit size={20} />
            <strong>{t("contextPanel.memory")}</strong>
            <span>{t("contextPanel.memoryEmpty")}</span>
          </div>
        </TabsContent>
      </Tabs>
    </aside>
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
  onCreateProject: () => void;
  onProjectMenuAction: (action: ProjectMenuAction, project: ProjectSummary) => void;
  onReorderProjects: (projectIds: string[]) => void;
  onSelectConversation: (conversationId: string) => void;
}) {
  // t translates stable sidebar labels while project names remain user data.
  // t 翻译稳定侧栏标签，项目名称仍保留为用户数据。
  const { t } = useI18n();
  // sidebarState selects the expanded tree interaction or the collapsed project pop menu.
  // sidebarState 选择展开树交互或折叠项目弹出菜单交互。
  const { state: sidebarState } = useSidebar();
  // openProjectIds stores the project folders currently expanded in the tree.
  // openProjectIds 保存当前在树中展开的项目文件夹。
  const [openProjectIds, setOpenProjectIds] = React.useState<Set<string>>(
    () => new Set(props.projects.map((project) => project.id)),
  );
  // activeProjectDragId records when project sorting is active so hover-only UI can pause.
  // activeProjectDragId 记录项目排序激活状态，以便暂停仅悬停显示的界面。
  const [activeProjectDragId, setActiveProjectDragId] = React.useState<string | null>(null);
  // projectDragSensors enables pointer dragging only after a small movement threshold.
  // projectDragSensors 仅在达到小幅移动阈值后启用指针拖拽。
  const projectDragSensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 6 } }));
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
  // sortedConversationsByProject keeps the service-provided recency order for each project.
  // sortedConversationsByProject 保留服务端提供的项目内最近活动顺序。
  const sortedConversationsByProject = React.useMemo(() => {
    return conversationsByProject;
  }, [conversationsByProject]);
  // pinnedProjects mirrors the iced task tree section split.
  // pinnedProjects 对齐 iced 任务树中的置顶分区拆分。
  const pinnedProjects = React.useMemo(() => props.projects.filter((project) => project.pinned), [props.projects]);
  // regularProjects keeps non-pinned projects in their service-provided order.
  // regularProjects 保留普通项目在服务端快照中的原始顺序。
  const regularProjects = React.useMemo(() => props.projects.filter((project) => !project.pinned), [props.projects]);
  // handleProjectDragEnd persists one dnd-kit same-section reorder result.
  // handleProjectDragEnd 持久化一次 dnd-kit 同分区项目排序结果。
  const handleProjectDragEnd = React.useCallback((sectionProjects: ProjectSummary[], event: DragEndEvent) => {
    setActiveProjectDragId(null);
    if (!event.over || event.active.id === event.over.id) {
      return;
    }
    const sectionIds = sectionProjects.map((project) => project.id);
    const activeIndex = sectionIds.indexOf(String(event.active.id));
    const overIndex = sectionIds.indexOf(String(event.over.id));
    if (activeIndex < 0 || overIndex < 0) {
      return;
    }
    props.onReorderProjects(arrayMove(sectionIds, activeIndex, overIndex));
  }, [props]);
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
  // renderProjectGroupLabel renders the pinned/project heading with optional actions.
  // renderProjectGroupLabel 渲染置顶/项目标题，并按需显示操作入口。
  const renderProjectGroupLabel = (label: string, icon: React.ReactNode, action?: React.ReactNode) => (
    <SidebarGroupLabel className="project-group-label">
      <span className="project-group-label-main">{icon}<span>{label}</span></span>
      {action}
    </SidebarGroupLabel>
  );
  // renderProjectRows renders one project section while preserving the shared row behavior.
  // renderProjectRows 渲染单个项目分区并保留共享项目行行为。
  const renderProjectRows = (projects: ProjectSummary[]) => projects.map((project) => {
    const projectConversations = sortedConversationsByProject[project.id] ?? [];
    return (
      <SortableProjectRow
        conversations={projectConversations}
        isOpen={openProjectIds.has(project.id)}
        isPinnedConversation={(conversationId) => props.pinnedConversationIds.has(conversationId)}
        key={project.id}
        onConversationMenuAction={props.onConversationMenuAction}
        onCreateConversation={props.onCreateConversation}
        onProjectMenuAction={props.onProjectMenuAction}
        onSelectConversation={props.onSelectConversation}
        onToggleOpen={toggleProjectOpen}
        project={project}
        selectedConversationId={props.selectedConversationId}
        sidebarState={sidebarState}
      />
    );
  });
  // renderSortableProjectSection renders one dnd-kit sortable project section.
  // renderSortableProjectSection 渲染一个 dnd-kit 可排序项目分区。
  const renderSortableProjectSection = (projects: ProjectSummary[]) => (
    <DndContext
      collisionDetection={closestCenter}
      onDragCancel={() => setActiveProjectDragId(null)}
      onDragEnd={(event) => handleProjectDragEnd(projects, event)}
      onDragStart={(event) => setActiveProjectDragId(String(event.active.id))}
      sensors={projectDragSensors}
    >
      <SortableContext items={projects.map((project) => project.id)} strategy={verticalListSortingStrategy}>
        {renderProjectRows(projects)}
      </SortableContext>
    </DndContext>
  );

  return (
    <Sidebar className={cn(activeProjectDragId && "project-drag-active")} collapsible="icon" variant="inset">
      <SidebarHeader>
        <SidebarMenu>
          <SidebarMenuItem>
            <div aria-current="page" className="vcn-sidebar-static-item">
              <LayoutDashboard size={16} />
              <span className="vcn-sidebar-label strong">{t("sidebar.console")}</span>
            </div>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarHeader>
      <SidebarContent>
        {pinnedProjects.length > 0 ? (
          <SidebarGroup className="project-section pinned-project-section">
            {renderProjectGroupLabel(t("sidebar.pinned"), <Pin size={15} />)}
            <SidebarMenu>{renderSortableProjectSection(pinnedProjects)}</SidebarMenu>
          </SidebarGroup>
        ) : null}
        <SidebarGroup>
            {renderProjectGroupLabel(t("sidebar.projects"), <Boxes size={15} />, (
              <Button
                aria-label={t("sidebar.addProject")}
                className="project-add-button"
                onClick={props.onCreateProject}
                size="icon-sm"
                title={t("sidebar.addProject")}
                type="button"
                variant="ghost"
              >
                <FolderPlus size={15} />
              </Button>
            ))}
            <SidebarMenu>
              {renderSortableProjectSection(regularProjects)}
              {props.projects.length === 0 ? (
                <SidebarMenuItem>
                  <SidebarMenuButton className="muted" tooltip={t("sidebar.noProjects")}>
                    <Folder size={16} />
                    <span className="vcn-sidebar-label">
                      <strong>{t("sidebar.noProjects")}</strong>
                      <small>{t("sidebar.waitingWorkspace")}</small>
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

// SortableProjectRow renders one dnd-kit sortable project folder and its conversations.
// SortableProjectRow 渲染一个 dnd-kit 可排序项目文件夹及其会话。
function SortableProjectRow(props: {
  // Project represented by this sortable row.
  // 当前可排序行代表的项目。
  project: ProjectSummary;
  // Conversations that belong to the project.
  // 归属于当前项目的会话列表。
  conversations: ConversationSummary[];
  // Whether the project folder is expanded.
  // 当前项目文件夹是否展开。
  isOpen: boolean;
  // Current sidebar collapsed or expanded state.
  // 当前侧栏的折叠或展开状态。
  sidebarState: ReturnType<typeof useSidebar>["state"];
  // Currently selected conversation id.
  // 当前选中的会话标识。
  selectedConversationId: string | null;
  // Checks whether a conversation is pinned.
  // 检查某个会话是否置顶。
  isPinnedConversation: (conversationId: string) => boolean;
  // Toggles the project folder expansion.
  // 切换项目文件夹展开状态。
  onToggleOpen: (projectId: string) => void;
  // Opens a new draft conversation for the project.
  // 为项目打开一个新的草稿会话。
  onCreateConversation: (project: ProjectSummary) => void;
  // Handles project context menu actions.
  // 处理项目右键菜单动作。
  onProjectMenuAction: (action: ProjectMenuAction, project: ProjectSummary) => void;
  // Handles conversation context menu actions.
  // 处理会话右键菜单动作。
  onConversationMenuAction: (action: ConversationMenuAction, conversation: ConversationSummary) => void;
  // Selects one conversation.
  // 选择一个会话。
  onSelectConversation: (conversationId: string) => void;
}) {
  const {
    attributes,
    isDragging,
    listeners,
    setNodeRef,
    transform,
    transition,
  } = useSortable({ id: props.project.id });
  const folderName = projectFolderName(props.project.path);
  const showOriginalFolderName = folderName.length > 0 && folderName !== props.project.name;
  // sortableTransform keeps dnd-kit movement vertical-only and prevents size-based text scaling.
  // sortableTransform 将 dnd-kit 移动限制为仅纵向，并阻止基于尺寸差异的文字缩放。
  const sortableTransform = transform ? { ...transform, x: 0, scaleX: 1, scaleY: 1 } : null;
  const sortableStyle: React.CSSProperties = {
    transform: CSS.Transform.toString(sortableTransform),
    transition,
  };
  return (
    <SidebarMenuItem
      className={cn("project-tree-item", isDragging && "drag-source")}
      ref={setNodeRef}
      style={sortableStyle}
    >
      {props.sidebarState === "collapsed" ? (
        <CollapsedProjectMenu
          conversations={props.conversations}
          onCreateConversation={props.onCreateConversation}
          onSelectConversation={props.onSelectConversation}
          project={props.project}
          selectedConversationId={props.selectedConversationId}
        />
      ) : (
        <div className={cn("project-folder-row", isDragging && "drag-source-row")} {...attributes} {...listeners}>
          <ProjectContextMenu project={props.project} onAction={props.onProjectMenuAction}>
            <SidebarMenuButton
              aria-expanded={props.isOpen}
              className="project-folder-button"
              onClick={() => props.onToggleOpen(props.project.id)}
              tooltip={`${props.project.name}\n${props.project.path}`}
            >
              <ProjectIcon isOpen={props.isOpen} project={props.project} size={16} />
              <span className="vcn-sidebar-label project-folder-label">
                <strong>{props.project.name}</strong>
                {showOriginalFolderName ? <small>{folderName}</small> : null}
              </span>
              <span className="project-folder-arrow" aria-hidden="true">
                {props.isOpen ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
              </span>
            </SidebarMenuButton>
          </ProjectContextMenu>
          <SidebarMenuAction
            aria-label={`在 ${props.project.name} 中创建新会话`}
            className="project-create-conversation"
            onClick={(event) => {
              event.stopPropagation();
              props.onCreateConversation(props.project);
            }}
            data-project-row-action
            showOnHover
            title="新建会话"
            type="button"
          >
            <SquarePen size={13} />
          </SidebarMenuAction>
        </div>
      )}
      {props.isOpen && props.sidebarState !== "collapsed" ? (
        <SidebarMenu className="project-conversation-list">
          {props.conversations.map((conversation) => (
            <SidebarMenuItem key={conversation.id}>
              <ConversationContextMenu
                conversation={conversation}
                isPinned={props.isPinnedConversation(conversation.id)}
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
                  {props.isPinnedConversation(conversation.id) ? <Pin size={13} /> : null}
                </SidebarMenuButton>
              </ConversationContextMenu>
            </SidebarMenuItem>
          ))}
        </SidebarMenu>
      ) : null}
    </SidebarMenuItem>
  );
}

// CollapsedProjectMenu renders project actions as a hover flyout while the sidebar is collapsed.
// CollapsedProjectMenu 在侧栏折叠时以悬停浮层展示项目操作。
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
  // anchorRef points at the collapsed folder row used to place the hover flyout.
  // anchorRef 指向折叠后的文件夹行，用于定位悬停浮层。
  const anchorRef = React.useRef<HTMLDivElement | null>(null);
  // closeTimerRef keeps the flyout alive while the pointer moves from folder to menu.
  // closeTimerRef 在指针从文件夹移动到菜单时保持浮层不被立刻关闭。
  const closeTimerRef = React.useRef<number | null>(null);
  // flyoutOpen tracks whether the hover flyout is currently visible.
  // flyoutOpen 跟踪悬停浮层当前是否可见。
  const [flyoutOpen, setFlyoutOpen] = React.useState(false);
  // flyoutPosition stores viewport coordinates so the menu is not clipped by sidebar scrolling.
  // flyoutPosition 保存视口坐标，避免菜单被侧栏滚动容器裁剪。
  const [flyoutPosition, setFlyoutPosition] = React.useState<{ left: number; top: number } | null>(null);

  // clearCloseTimer cancels a pending delayed close when the pointer returns.
  // clearCloseTimer 在指针返回时取消等待中的延迟关闭。
  const clearCloseTimer = React.useCallback(() => {
    if (closeTimerRef.current !== null) {
      window.clearTimeout(closeTimerRef.current);
      closeTimerRef.current = null;
    }
  }, []);

  // openFlyout positions and shows the collapsed project flyout.
  // openFlyout 定位并显示折叠项目浮层。
  const openFlyout = React.useCallback(() => {
    clearCloseTimer();
    const rect = anchorRef.current?.getBoundingClientRect();
    if (!rect) {
      return;
    }
    setFlyoutPosition({
      left: rect.right + 8,
      top: rect.top,
    });
    setFlyoutOpen(true);
  }, [clearCloseTimer]);

  // scheduleClose delays closing so pointer travel between folder and menu feels stable.
  // scheduleClose 延迟关闭浮层，让指针在文件夹与菜单间移动时保持稳定。
  const scheduleClose = React.useCallback(() => {
    clearCloseTimer();
    closeTimerRef.current = window.setTimeout(() => {
      setFlyoutOpen(false);
    }, 140);
  }, [clearCloseTimer]);

  React.useEffect(() => clearCloseTimer, [clearCloseTimer]);

  // selectConversation closes the flyout before switching to a conversation.
  // selectConversation 在切换会话前关闭浮层。
  const selectConversation = React.useCallback(
    (conversationId: string) => {
      setFlyoutOpen(false);
      props.onSelectConversation(conversationId);
    },
    [props],
  );

  // createConversation closes the flyout before opening a draft conversation.
  // createConversation 在打开草稿会话前关闭浮层。
  const createConversation = React.useCallback(() => {
    setFlyoutOpen(false);
    props.onCreateConversation(props.project);
  }, [props]);

  return (
    <>
      <div
        className="collapsed-project-hover-root"
        onMouseEnter={openFlyout}
        onMouseLeave={scheduleClose}
        ref={anchorRef}
      >
        <SidebarMenuButton
          aria-label={`${props.project.name} 会话菜单`}
          className="project-folder-button collapsed-project-button"
          onClick={(event) => event.preventDefault()}
        >
          <ProjectIcon project={props.project} size={16} />
          <span className="vcn-sidebar-label project-folder-label">
            <strong>{props.project.name}</strong>
          </span>
        </SidebarMenuButton>
      </div>
      {flyoutOpen && flyoutPosition ? (
        <div
          aria-label={`${props.project.name} 会话`}
          className="collapsed-project-flyout"
          onMouseEnter={openFlyout}
          onMouseLeave={scheduleClose}
          role="menu"
          style={{ left: flyoutPosition.left, top: flyoutPosition.top }}
        >
          <div className="collapsed-project-flyout-title" role="presentation">
            {props.project.name}
          </div>
          <button className="collapsed-project-flyout-item" onClick={createConversation} role="menuitem" type="button">
            <SquarePen size={15} />
            <span>新建会话</span>
          </button>
          <div className="collapsed-project-flyout-separator" role="separator" />
          {props.conversations.length > 0 ? (
            props.conversations.map((conversation) => (
              <button
                className={cn(
                  "collapsed-project-flyout-item",
                  props.selectedConversationId === conversation.id && "active",
                )}
                key={conversation.id}
                onClick={() => selectConversation(conversation.id)}
                role="menuitem"
                type="button"
              >
                <span className="collapsed-project-menu-title">{conversation.title}</span>
              </button>
            ))
          ) : (
            <div className="collapsed-project-flyout-item disabled" role="presentation">
              <span>暂无会话</span>
            </div>
          )}
        </div>
      ) : null}
    </>
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
        <ContextMenuItem onSelect={() => props.onAction("set-icon", props.project)}>
          <Palette size={15} />
          <span>设置项目图标</span>
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

// ProjectIconDialog renders the project icon picker and generated badge editor.
// ProjectIconDialog 渲染项目图标选择器与生成式徽标编辑器。
function ProjectIconDialog(props: {
  // Project icon target currently edited by this dialog.
  // 当前弹窗正在编辑的项目图标目标。
  target: ProjectIconDialogTarget | null;
  // Cancels the current icon edit.
  // 取消当前图标编辑。
  onCancel: () => void;
  // Persists the selected icon.
  // 持久化选中的图标。
  onConfirm: (project: ProjectSummary, icon: ProjectIconSelection) => void;
  // Resets the selected project to default folder icons.
  // 将选中项目还原为默认文件夹图标。
  onReset: (project: ProjectSummary) => void;
}) {
  const project = props.target?.project ?? null;
  // selectedIcon stores the currently selected built-in icon.
  // selectedIcon 保存当前选中的内置图标。
  const [selectedIcon, setSelectedIcon] = React.useState<ProjectIconSelection>({ pack: "lucide", key: "folder" });
  // badgeConfig stores the generated badge draft.
  // badgeConfig 保存生成式徽标草稿。
  const [badgeConfig, setBadgeConfig] = React.useState<ProjectIconBadgeConfig>(DEFAULT_PROJECT_BADGE_CONFIG);
  // activeTab stores the visible picker tab.
  // activeTab 保存当前可见的选择器标签。
  const [activeTab, setActiveTab] = React.useState("lucide");
  // iconSearch stores the Lucide icon filter text.
  // iconSearch 保存 Lucide 图标过滤文本。
  const [iconSearch, setIconSearch] = React.useState("");
  // visibleLucideLimit stores the streamed Lucide icon render limit.
  // visibleLucideLimit 保存 Lucide 图标流式渲染上限。
  const [visibleLucideLimit, setVisibleLucideLimit] = React.useState(LUCIDE_ICON_STREAM_BATCH_SIZE);

  React.useEffect(() => {
    if (!project) {
      return;
    }
    if (project.iconPack === "badge") {
      setActiveTab("badge");
      setBadgeConfig(decodeProjectBadgeKey(project.iconKey) ?? {
        ...DEFAULT_PROJECT_BADGE_CONFIG,
        text: normalizeProjectBadgeText(project.name) || DEFAULT_PROJECT_BADGE_CONFIG.text,
      });
      return;
    }
    const key = project.iconPack === "lucide" && project.iconKey && PROJECT_ICON_MAP.has(project.iconKey)
      ? project.iconKey
      : "Folder";
    setActiveTab("lucide");
    setSelectedIcon({ pack: "lucide", key });
    setIconSearch("");
    setVisibleLucideLimit(LUCIDE_ICON_STREAM_BATCH_SIZE);
    setBadgeConfig({
      ...DEFAULT_PROJECT_BADGE_CONFIG,
      text: normalizeProjectBadgeText(project.name) || DEFAULT_PROJECT_BADGE_CONFIG.text,
    });
  }, [project]);

  React.useEffect(() => {
    setVisibleLucideLimit(LUCIDE_ICON_STREAM_BATCH_SIZE);
  }, [iconSearch]);

  // updateBadgeText applies the Chinese-one-or-English-two text rule.
  // updateBadgeText 应用中文一个字或英文两个字母的文字规则。
  const updateBadgeText = React.useCallback((value: string) => {
    setBadgeConfig((current) => ({
      ...current,
      text: normalizeProjectBadgeText(value),
    }));
  }, []);

  if (!project) {
    return null;
  }

  const selectedBadgeIcon: ProjectIconSelection = {
    pack: "badge",
    key: encodeProjectBadgeKey(badgeConfig),
  };
  const finalSelection = activeTab === "badge" ? selectedBadgeIcon : selectedIcon;
  const normalizedSearch = iconSearch.trim().toLowerCase();
  const visibleLucideIcons = normalizedSearch.length === 0
    ? LUCIDE_PROJECT_ICON_OPTIONS
    : LUCIDE_PROJECT_ICON_OPTIONS.filter((option) => option.label.toLowerCase().includes(normalizedSearch));
  const streamedLucideIcons = visibleLucideIcons.slice(0, visibleLucideLimit);
  const hasMoreLucideIcons = streamedLucideIcons.length < visibleLucideIcons.length;

  // handleLucideGridScroll streams the next icon batch when the user reaches the list bottom.
  // handleLucideGridScroll 在用户滚动到列表底部时流式追加下一批图标。
  const handleLucideGridScroll = (event: React.UIEvent<HTMLDivElement>) => {
    const element = event.currentTarget;
    if (!hasMoreLucideIcons) {
      return;
    }
    const distanceFromBottom = element.scrollHeight - element.scrollTop - element.clientHeight;
    if (distanceFromBottom > 96) {
      return;
    }
    setVisibleLucideLimit((current) => Math.min(current + LUCIDE_ICON_STREAM_BATCH_SIZE, visibleLucideIcons.length));
  };

  return (
    <Dialog open={Boolean(project)} onOpenChange={(open) => !open && props.onCancel()}>
      <DialogContent className="project-icon-dialog sm:max-w-[720px]">
        <DialogHeader>
          <DialogTitle>设置项目图标</DialogTitle>
          <DialogDescription>
            为 {project.name} 选择固定图标，或生成一个字母徽标。还原后会重新使用默认文件夹开关图标。
          </DialogDescription>
        </DialogHeader>
        <div className="project-icon-layout">
          <div className="project-icon-preview" aria-label="项目图标预览">
            <ProjectIcon project={{ ...project, iconPack: finalSelection.pack, iconKey: finalSelection.key }} size={42} />
            <strong>{project.name}</strong>
          </div>
          <Tabs className="project-icon-tabs" value={activeTab} onValueChange={setActiveTab}>
            <TabsList className="project-icon-tabs-list">
              <TabsTrigger value="lucide">Lucide</TabsTrigger>
              <TabsTrigger value="badge">自定义</TabsTrigger>
            </TabsList>
            <TabsContent className="project-icon-tab-content" value="lucide">
              <div className="lucide-icon-filter">
                <Input
                  onChange={(event) => setIconSearch(event.target.value)}
                  placeholder="过滤 Lucide 图标"
                  value={iconSearch}
                />
                <span>{streamedLucideIcons.length} / {visibleLucideIcons.length} / {LUCIDE_PROJECT_ICON_OPTIONS.length}</span>
              </div>
              <div className="project-icon-grid lucide-icon-grid" onScroll={handleLucideGridScroll}>
                {streamedLucideIcons.map((option) => (
                  <button
                    aria-pressed={selectedIcon.pack === "lucide" && selectedIcon.key === option.key}
                    className={cn(
                      "project-icon-option",
                      selectedIcon.pack === "lucide" && selectedIcon.key === option.key && "selected",
                    )}
                    key={option.key}
                    onClick={() => setSelectedIcon({ pack: "lucide", key: option.key })}
                    title={option.label}
                    type="button"
                  >
                    <option.Icon size={22} />
                    <span>{option.label}</span>
                  </button>
                ))}
                {hasMoreLucideIcons ? (
                  <button
                    className="project-icon-option lucide-load-more"
                    onClick={() => setVisibleLucideLimit((current) => Math.min(current + LUCIDE_ICON_STREAM_BATCH_SIZE, visibleLucideIcons.length))}
                    type="button"
                  >
                    加载更多
                  </button>
                ) : null}
              </div>
            </TabsContent>
            <TabsContent className="project-icon-tab-content badge-tab" value="badge">
              <div className="badge-editor">
                <label>
                  <span>文字</span>
                  <Input
                    maxLength={4}
                    onChange={(event) => updateBadgeText(event.target.value)}
                    placeholder="中文 1 个或英文 2 个"
                    value={badgeConfig.text}
                  />
                </label>
                <label>
                  <span>形状</span>
                  <Select
                    value={badgeConfig.shape}
                    onValueChange={(shape) => setBadgeConfig((current) => ({ ...current, shape: shape as ProjectIconBadgeShape }))}
                  >
                    <SelectTrigger className="badge-shape-trigger">
                      <span>{badgeShapeLabel(badgeConfig.shape)}</span>
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="circle">圆形</SelectItem>
                      <SelectItem value="square">方形</SelectItem>
                      <SelectItem value="rounded">圆边</SelectItem>
                      <SelectItem value="sharp">方边</SelectItem>
                    </SelectContent>
                  </Select>
                </label>
                <ColorPickerRow
                  label="背景色"
                  value={badgeConfig.background}
                  onChange={(background) => setBadgeConfig((current) => ({ ...current, background }))}
                />
                <ColorPickerRow
                  label="边框色"
                  value={badgeConfig.border}
                  onChange={(border) => setBadgeConfig((current) => ({ ...current, border }))}
                />
                <ColorPickerRow
                  label="字体色"
                  value={badgeConfig.foreground}
                  onChange={(foreground) => setBadgeConfig((current) => ({ ...current, foreground }))}
                />
              </div>
            </TabsContent>
          </Tabs>
        </div>
        <DialogFooter>
          <Button type="button" variant="ghost" onClick={props.onCancel}>取消</Button>
          <Button type="button" variant="outline" onClick={() => props.onReset(project)}>还原默认</Button>
          <Button type="button" onClick={() => props.onConfirm(project, finalSelection)}>设置</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

// ColorPickerRow renders one color input with quick color swatches.
// ColorPickerRow 渲染一个带快捷色块的颜色输入。
function ColorPickerRow(props: {
  // User-facing color field label.
  // 面向用户显示的颜色字段名称。
  label: string;
  // Current hex color.
  // 当前十六进制颜色。
  value: string;
  // Updates the current hex color.
  // 更新当前十六进制颜色。
  onChange: (value: string) => void;
}) {
  return (
    <div className="color-picker-row">
      <span>{props.label}</span>
      <input
        aria-label={props.label}
        className="color-picker-input"
        onChange={(event) => props.onChange(event.target.value)}
        type="color"
        value={props.value}
      />
      <div className="color-swatch-row">
        {PROJECT_BADGE_COLORS.map((color) => (
          <button
            aria-label={`${props.label} ${color}`}
            className="color-swatch"
            key={color}
            onClick={() => props.onChange(color)}
            style={{ backgroundColor: color }}
            type="button"
          />
        ))}
      </div>
    </div>
  );
}

// badgeShapeLabel returns the Chinese label for a generated badge shape.
// badgeShapeLabel 返回生成式徽标形状的中文标签。
function badgeShapeLabel(shape: ProjectIconBadgeShape): string {
  switch (shape) {
    case "circle":
      return "圆形";
    case "square":
      return "方形";
    case "rounded":
      return "圆边";
    case "sharp":
      return "方边";
  }
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
  // workspaceRef stores the scrollable conversation timeline element.
  // workspaceRef 保存可滚动的会话时间线元素。
  const workspaceRef = React.useRef<HTMLElement | null>(null);
  // isAtConversationBottom tracks whether the user is viewing the latest conversation content.
  // isAtConversationBottom 跟踪用户是否正在查看最新的会话内容。
  const [isAtConversationBottom, setIsAtConversationBottom] = React.useState(true);
  const selectedConversation = props.workbench.conversations.find((conversation) => conversation.id === props.selectedConversationId);
  const messages = props.workbench.messages.filter((message) => message.conversationId === props.selectedConversationId);
  const isLoading = props.loadingConversationId === props.selectedConversationId;
  // showConversationLoadingOnly is true only when no cached messages can be shown during hydration.
  // showConversationLoadingOnly 仅在水合期间没有可展示缓存消息时为真。
  const showConversationLoadingOnly = isLoading && messages.length === 0 && !props.conversationLoadError;
  const selectedProject = selectedConversation ? projectForConversation(props.workbench, selectedConversation) : null;
  const emptyProjectName = props.selectedDraftConversation?.projectName ?? selectedProject?.name ?? "当前项目";
  const isEmptyConversation = Boolean(
    props.selectedDraftConversation
      || (selectedConversation && !showConversationLoadingOnly && !props.conversationLoadError && messages.length === 0),
  );

  // updateConversationBottomState refreshes the bottom affordance from the timeline scroll position.
  // updateConversationBottomState 根据时间线滚动位置刷新底部跳转按钮状态。
  const updateConversationBottomState = React.useCallback(() => {
    const element = workspaceRef.current;
    if (!element) {
      setIsAtConversationBottom(true);
      return;
    }
    const distanceFromBottom = element.scrollHeight - element.scrollTop - element.clientHeight;
    setIsAtConversationBottom(distanceFromBottom <= 24);
  }, []);

  // scrollConversationToBottom moves the timeline to the latest content.
  // scrollConversationToBottom 将时间线滚动到最新内容。
  const scrollConversationToBottom = React.useCallback((behavior: ScrollBehavior = "smooth") => {
    const element = workspaceRef.current;
    if (!element) {
      return;
    }
    const safeBehavior = behavior === "auto" || behavior === "instant" || behavior === "smooth" ? behavior : "smooth";
    element.scrollTo({ top: element.scrollHeight, behavior: safeBehavior });
  }, []);

  // jumpConversationToBottom immediately pins the timeline to its latest rendered height.
  // jumpConversationToBottom 立即把时间线固定到最新已渲染高度。
  const jumpConversationToBottom = React.useCallback(() => {
    const element = workspaceRef.current;
    if (!element) {
      return false;
    }
    element.scrollTop = element.scrollHeight;
    return true;
  }, []);

  React.useLayoutEffect(() => {
    if (!props.selectedConversationId || isEmptyConversation || showConversationLoadingOnly) {
      return;
    }

    let secondFrame: number | null = null;
    let thirdFrame: number | null = null;
    jumpConversationToBottom();
    const firstFrame = window.requestAnimationFrame(() => {
      jumpConversationToBottom();
      secondFrame = window.requestAnimationFrame(() => {
        jumpConversationToBottom();
        thirdFrame = window.requestAnimationFrame(() => {
          jumpConversationToBottom();
          updateConversationBottomState();
        });
        updateConversationBottomState();
      });
    });

    return () => {
      window.cancelAnimationFrame(firstFrame);
      if (secondFrame !== null) {
        window.cancelAnimationFrame(secondFrame);
      }
      if (thirdFrame !== null) {
        window.cancelAnimationFrame(thirdFrame);
      }
    };
  }, [
    isEmptyConversation,
    jumpConversationToBottom,
    messages.length,
    props.selectedConversationId,
    showConversationLoadingOnly,
    updateConversationBottomState,
  ]);

  React.useEffect(() => {
    updateConversationBottomState();
  }, [messages.length, props.selectedConversationId, updateConversationBottomState]);

  return (
    <div className={cn("area-grid chat-grid", isEmptyConversation && "empty-conversation-grid", showConversationLoadingOnly && "loading-conversation-grid")}>
      {showConversationLoadingOnly ? (
        <ConversationLoadingCenter />
      ) : isEmptyConversation ? (
        <EmptyConversationCenter projectName={emptyProjectName} />
      ) : (
        <>
          <ConversationWorkspace
            conversation={selectedConversation}
            messages={messages}
            error={props.conversationLoadError}
            onScrollStateChange={updateConversationBottomState}
            scrollRef={workspaceRef}
          />
          <ComposerDock onScrollToBottom={() => scrollConversationToBottom()} showScrollToBottom={!isAtConversationBottom} />
        </>
      )}
    </div>
  );
}

// ConversationWorkspace renders the conversation timeline replacement.
// ConversationWorkspace 渲染会话时间线替代组件。
function ConversationWorkspace(props: {
  conversation?: ConversationSummary;
  messages: ConversationMessage[];
  error: string | null;
  onScrollStateChange: () => void;
  scrollRef: React.RefObject<HTMLElement | null>;
}) {
  // detail stores the currently opened transcript detail modal.
  // detail 保存当前打开的转录详情弹窗。
  const [detail, setDetail] = React.useState<ConversationDetail | null>(null);
  const renderItems = React.useMemo(() => projectConversationRenderItems(props.messages), [props.messages]);

  return (
    <section className="conversation-panel" onScroll={props.onScrollStateChange} ref={props.scrollRef}>
      <div className="timeline">
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
      <TooltipContent className="turn-stats-tooltip" showArrow={false} side="right" sideOffset={8}>
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

// ConversationLoadingCenter renders the cache-miss conversation hydration placeholder.
// ConversationLoadingCenter 渲染无缓存会话水合时的居中占位提示。
function ConversationLoadingCenter() {
  return (
    <section className="conversation-loading-center" aria-live="polite" aria-busy="true">
      <ShinyText text="✨ 正在加载会话信息" />
    </section>
  );
}

// ComposerDock renders composer-related floating controls.
// ComposerDock 渲染输入区相关的浮动控件。
function ComposerDock(props: {
  // showScrollToBottom controls whether the bottom-jump button is visible.
  // showScrollToBottom 控制是否显示跳转到底部按钮。
  showScrollToBottom: boolean;
  // onScrollToBottom moves the current conversation to its latest content.
  // onScrollToBottom 将当前会话移动到最新内容。
  onScrollToBottom: () => void;
}) {
  return (
    <div className="composer-dock">
      {props.showScrollToBottom ? (
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              aria-label="滚动到底部"
              className="scroll-to-bottom-button"
              onClick={props.onScrollToBottom}
              size="icon"
              type="button"
              variant="outline"
            >
              <ArrowDown size={18} />
            </Button>
          </TooltipTrigger>
          <TooltipContent side="top">滚动到底部</TooltipContent>
        </Tooltip>
      ) : null}
      <ComposerPanel />
    </div>
  );
}

// MODEL_SELECT_OPTIONS mirrors the iced model trigger shape while service binding is pending.
// MODEL_SELECT_OPTIONS 在服务绑定完成前按 iced 模型触发器结构提供预览选项。
const MODEL_SELECT_OPTIONS: ComposerSelectOption[] = [
  { value: "deepseek-v4-pro", label: "DeepSeek V4 Pro", subtitle: "DeepSeek" },
  { value: "deepseek-v4", label: "DeepSeek V4", subtitle: "DeepSeek" },
  { value: "qwen3-coder", label: "Qwen3 Coder", subtitle: "Alibaba" },
];

// ComposerPanel renders the first-version message input surface.
// ComposerPanel 渲染第一版消息输入区域。
function ComposerPanel() {
  // t translates stable composer labels while model names remain provider data.
  // t 翻译稳定输入区标签，模型名称仍保留为 provider 数据。
  const { t } = useI18n();
  // draftInput stores the current unsent composer text.
  // draftInput 保存当前尚未发送的输入区文本。
  const [draftInput, setDraftInput] = React.useState("");
  // executionMode stores the local preview execution-mode selection.
  // executionMode 保存本地预览用的执行模式选择。
  const [executionMode, setExecutionMode] = React.useState("build");
  // modelChoice stores the local preview model selection.
  // modelChoice 保存本地预览用的模型选择。
  const [modelChoice, setModelChoice] = React.useState("deepseek-v4-pro");
  // aiCapabilityChoice stores the local preview AI capability selection.
  // aiCapabilityChoice 保存本地预览用的 AI 能力选择。
  const [aiCapabilityChoice, setAiCapabilityChoice] = React.useState("extended");
  // reasoningEffort stores the local preview reasoning-effort selection.
  // reasoningEffort 保存本地预览用的思考档位选择。
  const [reasoningEffort, setReasoningEffort] = React.useState("x-high");
  // isComposerRunning stores the local send/stop preview state.
  // isComposerRunning 保存输入区发送/停止的本地预览状态。
  const [isComposerRunning, setIsComposerRunning] = React.useState(false);
  // executionModeOptions mirrors iced ExecutionMode::ALL using current language labels.
  // executionModeOptions 使用当前语言标签对齐 iced 的 ExecutionMode::ALL。
  const executionModeOptions = React.useMemo<ComposerSelectOption[]>(() => [
    { value: "build", label: t("composer.mode.execute") },
    { value: "plan", label: t("composer.mode.plan") },
  ], [t]);
  // aiCapabilityOptions mirrors the iced AI capability quick-control trigger.
  // aiCapabilityOptions 对齐 iced 的 AI 能力快捷控制触发器。
  const aiCapabilityOptions = React.useMemo<ComposerSelectOption[]>(() => [
    { value: "extended", label: t("composer.capability.extended"), subtitle: t("composer.capability.extended.subtitle") },
    { value: "basic", label: t("composer.capability.basic"), subtitle: t("composer.capability.basic.subtitle") },
  ], [t]);
  // reasoningEffortOptions mirrors provider-config ReasoningEffortLevel labels plus the DeepSeek Max override.
  // reasoningEffortOptions 对齐 provider-config 的思考档位标签，并包含 DeepSeek 的 Max 覆盖标签。
  const reasoningEffortOptions = React.useMemo<ComposerSelectOption[]>(() => [
    { value: "off", label: t("composer.reasoning.off"), subtitle: t("composer.reasoning.subtitle") },
    { value: "low", label: t("composer.reasoning.low"), subtitle: t("composer.reasoning.subtitle") },
    { value: "medium", label: t("composer.reasoning.medium"), subtitle: t("composer.reasoning.subtitle") },
    { value: "high", label: t("composer.reasoning.high"), subtitle: t("composer.reasoning.subtitle") },
    { value: "x-high", label: t("composer.reasoning.max"), subtitle: t("composer.reasoning.subtitle") },
  ], [t]);
  // placeholder describes the platform-specific send and newline keyboard behavior.
  // placeholder 描述平台相关的发送与换行键盘行为。
  const placeholder = React.useMemo(() => {
    return t("composer.placeholder").replace("{shortcut}", composerShortcutHint(t("composer.shortcutHint")));
  }, [t]);

  // handleComposerSend handles the current placeholder-only send action.
  // handleComposerSend 处理当前仅占位的发送动作。
  const handleComposerSend = React.useCallback(() => {
    if (isComposerRunning) {
      setIsComposerRunning(false);
      toast.info("停止已拦截：真实停止链路待接入。");
      return;
    }
    if (!draftInput.trim()) {
      toast.info("请输入任务指令。");
      return;
    }
    setIsComposerRunning(true);
    toast.info("发送已拦截：真实发送链路待接入。");
  }, [draftInput, isComposerRunning]);

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
        aria-label={t("composer.aria.input")}
        onChange={(event) => setDraftInput(event.currentTarget.value)}
        onKeyDown={handleComposerKeyDown}
        placeholder={placeholder}
        value={draftInput}
      />
      <div className="composer-toolbar">
        <div className="composer-toolbar-left">
          <ComposerSelectControl
            ariaLabel={t("composer.aria.mode")}
            className="mode"
            fallbackLabel={t("composer.selectFallback")}
            onValueChange={setExecutionMode}
            options={executionModeOptions}
            value={executionMode}
          />
          <ComposerSelectControl
            ariaLabel={t("composer.aria.model")}
            className="model"
            fallbackLabel={t("composer.selectFallback")}
            icon={<Cpu size={15} />}
            onValueChange={setModelChoice}
            options={MODEL_SELECT_OPTIONS}
            value={modelChoice}
          />
          <ComposerSelectControl
            ariaLabel={t("composer.aria.capability")}
            className="capability"
            fallbackLabel={t("composer.selectFallback")}
            icon={<SlidersHorizontal size={15} />}
            onValueChange={setAiCapabilityChoice}
            options={aiCapabilityOptions}
            value={aiCapabilityChoice}
          />
        </div>
        <div className="composer-toolbar-right">
          <ComposerSelectControl
            ariaLabel={t("composer.aria.reasoning")}
            className="reasoning"
            fallbackLabel={t("composer.selectFallback")}
            onValueChange={setReasoningEffort}
            options={reasoningEffortOptions}
            value={reasoningEffort}
          />
          <Button
            aria-label={isComposerRunning ? t("composer.aria.stop") : t("composer.aria.send")}
            className={cn("send-button", isComposerRunning && "running")}
            onClick={handleComposerSend}
            size="icon"
            type="button"
          >
            {isComposerRunning ? <Square fill="currentColor" size={15} strokeWidth={0} /> : <Send size={18} />}
          </Button>
        </div>
      </div>
    </section>
  );
}

// ComposerSelectControl renders one iced-aligned compact composer selector.
// ComposerSelectControl 渲染一个对齐 iced 的紧凑输入区选择器。
function ComposerSelectControl(props: {
  // Accessible label for the dropdown trigger.
  // 下拉触发器的无障碍标签。
  ariaLabel: string;
  // Localized fallback label shown when the current value is missing.
  // 当前值缺失时显示的本地化回退标签。
  fallbackLabel: string;
  // Optional visual icon rendered in the left accessory slot.
  // 渲染在左侧附属槽位中的可选视觉图标。
  icon?: React.ReactNode;
  // CSS class suffix controlling the iced-aligned fixed width.
  // 控制 iced 对齐固定宽度的 CSS 类后缀。
  className: string;
  // Available local options for this selector.
  // 当前选择器可用的本地选项。
  options: ComposerSelectOption[];
  // Current selected option value.
  // 当前选中的选项值。
  value: string;
  // Selection change handler.
  // 选项变更处理函数。
  onValueChange: (value: string) => void;
}) {
  const selectedOption = props.options.find((option) => option.value === props.value) ?? props.options[0];

  return (
    <Select onValueChange={props.onValueChange} value={props.value}>
      <SelectTrigger
        aria-label={props.ariaLabel}
        className={cn("composer-select-trigger", props.className)}
      >
        {props.icon ? <span className="composer-select-icon">{props.icon}</span> : null}
        <span className={cn("composer-select-copy", !props.icon && "centered")}>
          <span className="composer-select-label">{selectedOption?.label ?? props.fallbackLabel}</span>
          {selectedOption?.subtitle ? (
            <span className="composer-select-subtitle">{selectedOption.subtitle}</span>
          ) : null}
        </span>
      </SelectTrigger>
      <SelectContent align="start" className="composer-select-content" position="popper">
        {props.options.map((option) => (
          <SelectItem className="composer-select-item" key={option.value} value={option.value}>
            <span className="composer-select-item-copy">
              <span>{option.label}</span>
              {option.subtitle ? <small>{option.subtitle}</small> : null}
            </span>
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}

// composerShortcutHint returns a platform-specific keyboard hint for the composer placeholder.
// composerShortcutHint 返回输入区 placeholder 使用的平台化快捷键提示。
function composerShortcutHint(baseHint: string): string {
  const platformText = typeof navigator === "undefined"
    ? ""
    : `${navigator.platform} ${navigator.userAgent}`.toLowerCase();
  if (platformText.includes("mac")) {
    return baseHint;
  }
  return baseHint;
}

// EmptyConversationCenter renders the centered first-prompt state for a blank conversation.
// EmptyConversationCenter 渲染空白会话的居中首次输入状态。
function EmptyConversationCenter(props: { projectName: string }) {
  // t translates the empty conversation prompt while projectName remains user data.
  // t 翻译空会话引导语，projectName 仍保留为用户数据。
  const { t } = useI18n();
  // promptText inserts the selected project name into the localized empty prompt.
  // promptText 将当前项目名称插入本地化空会话提示。
  const promptText = t("console.emptyPrompt").replace("{projectName}", props.projectName);

  return (
    <section className="empty-conversation-center">
      <h2>{promptText}</h2>
      <ComposerPanel />
    </section>
  );
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
