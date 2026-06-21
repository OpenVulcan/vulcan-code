import type {
  ComposerSlashCommandSummary,
  ConversationMessage,
  StartupSnapshot,
  WorkbenchSnapshot,
} from "@/types";

// BrowserFallbackStartup is used when the WebUI runs outside Tauri during frontend development.
// BrowserFallbackStartup 用于 WebUI 在 Tauri 外部进行前端开发时展示。
export const browserFallbackStartup: StartupSnapshot = {
  ready: true,
  phase: "browser-preview",
  steps: [
    { id: "service-process", label: "Core Service", status: "warning", message: "浏览器预览模式：未连接本地 Core Service" },
    { id: "luaskills", label: "LuaSkills", status: "pending", message: "等待桌面壳提供真实状态" },
    { id: "vldb", label: "VLDB", status: "pending", message: "等待桌面壳提供真实状态" },
    { id: "mcp", label: "MCP Runtime", status: "pending", message: "等待桌面壳提供真实状态" },
    { id: "workspace", label: "Workspace", status: "ready", message: "前端布局预览已就绪" },
  ],
  recentLogs: ["[webui] browser preview mode"],
};

// browserFallbackSlashCommands mirrors the service-projected iced command catalog for preview mode.
// browserFallbackSlashCommands 在预览模式中镜像 service 投影出的 iced 指令目录。
const browserFallbackSlashCommands: ComposerSlashCommandSummary[] = [
  {
    id: "model",
    command: "/model",
    descriptionZh: "选择当前模型",
    descriptionEn: "Select the current model",
  },
  {
    id: "compact",
    command: "/compact",
    descriptionZh: "压缩当前会话上下文",
    descriptionEn: "Compact the current conversation context",
  },
  {
    id: "reasoning",
    command: "/reasoning",
    descriptionZh: "选择当前思考级别",
    descriptionEn: "Select the current reasoning level",
  },
  {
    id: "plan",
    command: "/plan",
    descriptionZh: "切换当前会话计划模式",
    descriptionEn: "Toggle Plan mode for the current conversation",
  },
  {
    id: "skill",
    command: "/skill",
    descriptionZh: "插入提示词技能加载指令",
    descriptionEn: "Insert the prompt-skill loading instruction",
  },
  {
    id: "skills",
    command: "/skills",
    descriptionZh: "打开提示词技能面板",
    descriptionEn: "Open the prompt skills panel",
  },
];

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
  expandedProjectIds: ["preview", "calc", "test-project", "luaskills-packages", "vulcan-workmem"],
  conversations: [
    { id: "preview", title: "Tauri WebUI 迁移预览", status: "Completed", lastUpdatedLabel: "刚刚", projectId: "preview" },
    { id: "preview-intro", title: "介绍你自己", status: "Completed", lastUpdatedLabel: "刚刚", projectId: "preview" },
    { id: "calc-curl", title: "排查 lcurl.safe 模块缺失", status: "Completed", lastUpdatedLabel: "3 天", projectId: "calc" },
    { id: "calc-iconv", title: "检查是否缺少 iconv", status: "Completed", lastUpdatedLabel: "1 周", projectId: "calc" },
    { id: "test-tools", title: "你有哪些工具", status: "Completed", lastUpdatedLabel: "刚刚", projectId: "test-project", unread: true },
    { id: "test-playwright", title: "添加 Playwright 浏览器支持", status: "Running", lastUpdatedLabel: "1 分", projectId: "test-project" },
    { id: "luaskills-rust", title: "分析 Rust 插件机制", status: "Completed", lastUpdatedLabel: "2 天", projectId: "luaskills-packages" },
    { id: "workmem-plan", title: "VMM 置顶项目对接方案", status: "Completed", lastUpdatedLabel: "5 小时", projectId: "vulcan-workmem" },
  ],
  archivedProjects: [],
  archivedConversations: [],
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
    { id: "memory", label: "记忆", description: "VMM 用户、项目与会话记忆", status: "预览" },
    { id: "settings", label: "设置", description: "项目、模型、工具、诊断与系统设置", status: "预览" },
  ],
  composerSlashCommands: browserFallbackSlashCommands,
  pendingPrompts: [],
  pendingQuestion: null,
  luaSkills: {
    runtimeRoot: "D:/openvulcan/vulcan-code/output/lua_runtime",
    systemLuaLib: "D:/openvulcan/vulcan-code/output/lua_runtime/system_lua_lib",
    entryCount: 3,
    notice: "浏览器预览模式展示 Lua Skills 样例目录。",
    skills: [
      {
        skillId: "vulcan-codekit",
        displayName: "Vulcan CodeKit",
        version: "0.1.0",
        rootName: "ROOT",
        skillDir: "D:/openvulcan/vulcan-code/output/lua_runtime/skills/vulcan-codekit",
        enabled: true,
        managed: true,
        sourceType: "github",
        sourceLocator: "LuaSkills/vulcan-codekit",
        entryCount: 2,
        systemOwned: true,
        tools: [
          { canonicalName: "vulcan_codekit_rg", localName: "rg", description: "搜索代码并返回结构化上下文。", registrationState: "explicit", registrationLabel: "显式启用", registrationHelp: "工具会作为独立函数 Schema 暴露给模型。" },
          { canonicalName: "vulcan_codekit_patch", localName: "patch", description: "按结构选择器替换函数。", registrationState: "implicit", registrationLabel: "隐式启用", registrationHelp: "工具保留宿主调用能力，但不暴露独立 Schema。" },
        ],
      },
    ],
  },
  agentSkills: {
    globalEnabled: true,
    systemRoot: "C:/Users/20000/.vulcan/skills/.system",
    userRoot: "C:/Users/20000/.vulcan/skills",
    notice: "浏览器预览模式展示 Agent Skills 样例目录。",
    errors: [],
    skills: [
      {
        name: "security-review",
        displayName: "Security Review",
        description: "按安全最佳实践检查 JavaScript、Python 和 Go 代码。",
        scope: "系统",
        enabled: true,
        skillFile: "C:/Users/20000/.vulcan/skills/.system/security-review/SKILL.md",
        baseDir: "C:/Users/20000/.vulcan/skills/.system/security-review",
        iconPath: "C:/Users/20000/.vulcan/skills/.system/skill-creator/assets/skill-creator-small.svg",
        body: "Use this skill when the user explicitly requests a security review.",
      },
    ],
  },
  settings: [
    { id: "runtime", label: "运行时", description: "Core Service、LuaSkills、VLDB、MCP", items: ["启动模式", "运行时目录", "MCP 工具策略"] },
    { id: "providers", label: "模型与 Provider", description: "模型供应商、默认模型、工具能力与推理配置", items: ["默认 Provider", "默认模型", "工具调用", "推理强度"] },
    { id: "remote", label: "远程访问", description: "未来浏览器、手机、远程客户端访问 Core Service 的设置", items: ["监听地址", "访问令牌", "远程开关", "审计日志"] },
  ],
  providers: [
    {
      id: "deepseek",
      name: "DeepSeek",
      status: "预览",
      enabledModels: "2 个已启用模型",
      active: true,
      models: [
        { id: "deepseek-v4-pro", name: "DeepSeek V4 Pro", active: true },
        { id: "deepseek-v4", name: "DeepSeek V4" },
      ],
    },
    {
      id: "alibaba-cn",
      name: "Alibaba CN",
      status: "预览",
      enabledModels: "1 个已启用模型",
      models: [{ id: "qwen3-coder", name: "Qwen3 Coder" }],
    },
    { id: "local", name: "本地模型", status: "待接入", enabledModels: "0", models: [] },
  ],
  reasoningSelection: {
    selectedEffort: "x-high",
    options: [
      { value: "off", label: "关闭" },
      { value: "high", label: "高" },
      { value: "x-high", label: "Max" },
    ],
  },
  aiCapabilities: [
    {
      kind: "web_search",
      label: "网页搜索",
      available: true,
      enabled: true,
      native: false,
      bridged: true,
      targets: [
        {
          targetSelector: "builtin/web-search",
          vendorId: "builtin",
          vendorName: "Builtin",
          targetAlias: "web-search",
          targetName: "Web Search",
          modelId: "native",
          enabled: true,
        },
      ],
    },
    {
      kind: "understand_image",
      label: "图片理解",
      available: false,
      enabled: false,
      native: false,
      bridged: false,
      targets: [],
    },
    {
      kind: "image_generation",
      label: "图片生成",
      available: false,
      enabled: false,
      native: false,
      bridged: false,
      targets: [],
    },
  ],
  memories: [
    { id: "user-profile", title: "用户画像", category: "Profile", state: "等待真实 VMM 节点接入" },
    { id: "project-memory", title: "项目记忆", category: "Project", state: "等待真实 VMM 节点接入" },
    { id: "conversation-memory", title: "会话记忆", category: "Conversation", state: "等待真实 VMM 节点接入" },
    { id: "conflict-review", title: "冲突审计", category: "Review", state: "等待真实 VMM 冲突视图接入" },
  ],
  vmmMemory: {
    effective: {
      enabled: true,
      grpcTarget: "127.0.0.1:50051",
      grpcConfigured: true,
      userId: 20000,
      projectId: 1001,
      diagnostic: "ready",
      precheckDiagnostic: "ready",
      aiToolsEnabled: true,
      precheckEnabled: true,
      implicitMemoryTurns: 5,
      profileRefreshTurns: 3,
      profileNodeLimit: 24,
      injectionTag: "vmm_inject",
      injectionDescription: "VMM recalled memory",
    },
    globalDraft: {
      hasOverride: true,
      enabled: true,
      grpcTarget: "127.0.0.1:50051",
      grpcApiKey: "",
      userId: "20000",
      projectId: "1001",
      receiveTimeoutMs: "30000",
      handshakeTimeoutMs: "8000",
      maxReceiveMessageBytes: "",
      grpcKeepaliveTimeMs: "30000",
      grpcKeepaliveTimeoutMs: "10000",
      grpcKeepalivePermitWithoutCalls: true,
      implicitMemoryTurns: "5",
      profileRefreshTurns: "3",
      profileNodeLimit: "24",
      aiToolsEnabled: true,
      precheckEnabled: true,
      injectionTag: "vmm_inject",
      injectionDescription: "VMM recalled memory",
    },
    projectDraft: {
      hasOverride: false,
      enabled: true,
      grpcTarget: "127.0.0.1:50051",
      grpcApiKey: "",
      userId: "20000",
      projectId: "1001",
      receiveTimeoutMs: "30000",
      handshakeTimeoutMs: "8000",
      maxReceiveMessageBytes: "",
      grpcKeepaliveTimeMs: "30000",
      grpcKeepaliveTimeoutMs: "10000",
      grpcKeepalivePermitWithoutCalls: true,
      implicitMemoryTurns: "5",
      profileRefreshTurns: "3",
      profileNodeLimit: "24",
      aiToolsEnabled: true,
      precheckEnabled: true,
      injectionTag: "vmm_inject",
      injectionDescription: "VMM recalled memory",
    },
    projectScopes: [
      { projectId: "vulcan-workmem", name: "Vulcan Code", path: "D:/openvulcan/vulcan-code", hasOverride: false },
    ],
    selectedProjectId: "vulcan-workmem",
    selectedConversationId: "preview-session",
    runtime: {
      outboxCount: 0,
      profileSyncCount: 3,
      completedTurns: 1,
      updatedAt: "1780908377000",
      profilePreview: "用户偏好：偏向工程化闭环。\n项目画像：WebUI 正在迁移 iced 功能。",
    },
    sections: [
      { id: "user_manager", key: "USER MANAGER", label: "用户管理", description: "解析、创建预检和删除长期用户。" },
      { id: "project_manager", key: "PROJECT MANAGER", label: "项目管理", description: "解析、创建预检、删除和迁移项目路径。" },
      { id: "profile_center", key: "PROFILE CENTER", label: "画像中心", description: "查看 active profile nodes 并应用手工画像指令。" },
      { id: "profile_bundle", key: "PROFILE BUNDLE TEST", label: "画像 Bundle 测试", description: "读取当前 user/project 的完整画像 Bundle。" },
      { id: "tools_debug", key: "TOOLS DEBUG", label: "工具调试", description: "模拟 AI 专属工具检索、详情展开和主动写入。" },
      { id: "memory_settings", key: "MEMORY SETTINGS", label: "记忆设置", description: "配置记忆开关、TTL、画像刷新和注入策略。" },
      { id: "grpc_transport", key: "GRPC TRANSPORT", label: "gRPC 传输", description: "配置全局 VMM 连接和传输参数。" },
    ],
    notice: "当前项目：Vulcan Code；诊断：ready；用户/项目：20000/1001。",
  },
  commandSessions: [
    { id: "preview-command", name: "WebUI build", command: "pnpm --dir apps\\vulcan-code-web run build", status: "preview", preview: "前端预览命令会话" },
    { id: "preview-cargo", name: "Cargo check", command: "cargo check -p vulcan-code-tauri", status: "preview", preview: "Tauri 桌面壳检查入口" },
  ],
  logs: [
    { id: "preview-log", level: "info", source: "webui", message: "浏览器预览模式已加载" },
    { id: "preview-legacy", level: "info", source: "iced", message: "旧 iced GUI 保留为 reference 入口" },
  ],
};

// browserFallbackWorkbenchState stores mutable preview data for non-Tauri browser development.
// browserFallbackWorkbenchState 保存非 Tauri 浏览器开发模式下的可变预览数据。
let browserFallbackWorkbenchState: WorkbenchSnapshot = cloneWorkbenchSnapshot(browserFallbackWorkbench);

// cloneWorkbenchSnapshot returns a detached workbench snapshot for React state updates.
// cloneWorkbenchSnapshot 返回用于 React 状态更新的脱离引用工作台快照。
function cloneWorkbenchSnapshot(snapshot: WorkbenchSnapshot): WorkbenchSnapshot {
  return structuredClone(snapshot) as WorkbenchSnapshot;
}

// browserFallbackSnapshot returns the current browser-preview snapshot.
// browserFallbackSnapshot 返回当前浏览器预览快照。
export function browserFallbackSnapshot(): WorkbenchSnapshot {
  return cloneWorkbenchSnapshot(browserFallbackWorkbenchState);
}

// loadBrowserFallbackConversationMessages returns preview messages for one conversation.
// loadBrowserFallbackConversationMessages 返回单个会话的预览消息。
export function loadBrowserFallbackConversationMessages(sessionId: string): ConversationMessage[] {
  return browserFallbackWorkbenchState.messages.filter((message) => message.conversationId === sessionId);
}

// updateBrowserFallbackWorkbench mutates preview state and returns a fresh snapshot.
// updateBrowserFallbackWorkbench 修改预览状态并返回新的快照。
export function updateBrowserFallbackWorkbench(mutator: (snapshot: WorkbenchSnapshot) => void): WorkbenchSnapshot {
  mutator(browserFallbackWorkbenchState);
  browserFallbackWorkbenchState.luaSkills.entryCount = browserFallbackWorkbenchState.luaSkills.skills
    .reduce((total, skill) => total + (skill.enabled ? skill.entryCount : 0), 0);
  return browserFallbackSnapshot();
}
