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
    { id: "service-process", label: "Core Service", status: "warning", message: "Browser preview mode: local Core Service is not connected" },
    { id: "luaskills", label: "LuaSkills", status: "pending", message: "Waiting for the desktop shell to provide live status" },
    { id: "vldb", label: "VLDB", status: "pending", message: "Waiting for the desktop shell to provide live status" },
    { id: "mcp", label: "MCP Runtime", status: "pending", message: "Waiting for the desktop shell to provide live status" },
    { id: "workspace", label: "Workspace", status: "ready", message: "Frontend layout preview is ready" },
  ],
  recentLogs: ["[webui] browser preview mode"],
};

// browserFallbackSlashCommands mirrors the service-projected command catalog for preview mode.
// browserFallbackSlashCommands 在预览模式中镜像 service 投影出的指令目录。
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
  revision: 0,
  appVersion: "0.1.0",
  modules: [
    { id: "core", label: "Core Service", status: "warning", message: "Browser preview mode" },
    { id: "luaskills", label: "LuaSkills", status: "pending", message: "Waiting for live status" },
    { id: "vldb", label: "VLDB", status: "pending", message: "Waiting for live status" },
    { id: "mcp", label: "MCP Runtime", status: "pending", message: "Waiting for live status" },
    { id: "workspace", label: "Workspace", status: "ready", message: "Frontend layout preview" },
  ],
  projects: [
    { id: "preview", name: "vulcan-code", path: "D:/openvulcan/vulcan-code", pinned: true },
    { id: "calc", name: "Abandoned Calculator", path: "D:/openvulcan/abandoned-calculator", pinned: true },
    { id: "test-project", name: "Test Project", path: "D:/openvulcan/test-project", pinned: false },
    { id: "luaskills-packages", name: "luaskills-packages", path: "D:/openvulcan/luaskills-packages", pinned: false },
    { id: "vulcan-workmem", name: "vulcan-workmem", path: "D:/openvulcan/vulcan-workmem", pinned: false },
  ],
  expandedProjectIds: ["preview", "calc", "test-project", "luaskills-packages", "vulcan-workmem"],
  conversations: [
    { id: "preview", title: "Tauri WebUI migration preview", status: "Completed", lastUpdatedLabel: "now", projectId: "preview" },
    { id: "preview-intro", title: "Introduce yourself", status: "Completed", lastUpdatedLabel: "now", projectId: "preview" },
    { id: "calc-curl", title: "Investigate missing lcurl.safe module", status: "Completed", lastUpdatedLabel: "3 days", projectId: "calc" },
    { id: "calc-iconv", title: "Check missing iconv support", status: "Completed", lastUpdatedLabel: "1 week", projectId: "calc" },
    { id: "test-tools", title: "Available tools", status: "Completed", lastUpdatedLabel: "now", projectId: "test-project", unread: true },
    { id: "test-playwright", title: "Add Playwright browser support", status: "Running", lastUpdatedLabel: "1 min", projectId: "test-project" },
    { id: "luaskills-rust", title: "Analyze the Rust plugin mechanism", status: "Completed", lastUpdatedLabel: "2 days", projectId: "luaskills-packages" },
    { id: "workmem-plan", title: "VMM pinned-project integration plan", status: "Completed", lastUpdatedLabel: "5 hours", projectId: "vulcan-workmem" },
  ],
  archivedProjects: [],
  archivedConversations: [],
  selectedConversationId: "preview",
  messages: [
    { id: "preview-user", conversationId: "preview", role: "user", body: "The new WebUI should fully cover the current interface." },
    {
      id: "preview-assistant",
      conversationId: "preview",
      role: "assistant",
      body: "This page previews the first layout pass for the workspace, settings, plugins, VMM, providers, tools, and diagnostics areas.\n\n:::tool-call tool_name=service_startup_snapshot tool_call_id=preview-service-startup status=preview\n## Tool Call\n\n`service_startup_snapshot`\n\n## Arguments\n\n```json\n{}\n```\n:::\n\n```diff\ndiff --git a/apps/vulcan-code-web/src/main.tsx b/apps/vulcan-code-web/src/main.tsx\n+ Add Markdown conversation rendering\n+ Support detail dialogs for special tool blocks\n```\n\nLive conversations will render tool calls, command output, and diff blocks in separate sections.",
    },
    {
      id: "preview-turn-stats",
      conversationId: "preview",
      role: "assistant",
      title: "Turn Stats · completed",
      recordKind: "turn-stats",
      body: "Status: completed\nProvider calls: 2\nTool calls: 3\nInput tokens: 45310\nOutput tokens: 1943\nReasoning tokens: 1182\nTotal tokens: 47253",
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
      title: "Tool Result · service_startup_snapshot",
      recordKind: "tool-result-output",
      body: "Ran 1 command\n\nCore Service is currently in browser-preview mode.",
      metadata: {
        tool_call_id: "preview-service-startup",
        tool_name: "service_startup_snapshot",
        tool_result_id: "tool-result-preview-service-startup",
      },
    },
  ],
  toolCalls: [{ id: "preview-tool", name: "service_startup_snapshot", status: "preview", detail: "Browser mode is using fallback data." }],
  featureAreas: [
    { id: "chat", label: "Workspace", description: "Project conversations, message stream, and model input", status: "Available" },
    { id: "skills", label: "Skills", description: "LuaSkills, tool capabilities, and skill packages", status: "Preview" },
    { id: "memory", label: "Memory", description: "VMM user, project, and conversation memory", status: "Preview" },
    { id: "settings", label: "Settings", description: "Project, model, tool, diagnostics, and system settings", status: "Preview" },
  ],
  composerSlashCommands: browserFallbackSlashCommands,
  pendingPrompts: [],
  pendingQuestion: null,
  luaSkills: {
    runtimeRoot: "D:/openvulcan/vulcan-code/output/lua_runtime",
    systemLuaLib: "D:/openvulcan/vulcan-code/output/lua_runtime/system_lua_lib",
    entryCount: 3,
    notice: "Browser preview mode is showing a sample Lua Skills catalog.",
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
          { canonicalName: "vulcan_codekit_rg", localName: "rg", description: "Search code and return structured context.", registrationState: "explicit", registrationLabel: "Explicit", registrationHelp: "The tool is exposed to the model as an independent function schema." },
          { canonicalName: "vulcan_codekit_patch", localName: "patch", description: "Replace a function selected by a structural selector.", registrationState: "implicit", registrationLabel: "Implicit", registrationHelp: "The tool remains available to the host but is not exposed as an independent schema." },
        ],
      },
    ],
  },
  agentSkills: {
    globalEnabled: true,
    systemRoot: "C:/Users/20000/.vulcan/skills/.system",
    userRoot: "C:/Users/20000/.vulcan/skills",
    notice: "Browser preview mode is showing a sample Agent Skills catalog.",
    errors: [],
    skills: [
      {
        name: "security-review",
        displayName: "Security Review",
        description: "Review JavaScript, Python, and Go code against security best practices.",
        scope: "System",
        enabled: true,
        skillFile: "C:/Users/20000/.vulcan/skills/.system/security-review/SKILL.md",
        baseDir: "C:/Users/20000/.vulcan/skills/.system/security-review",
        iconPath: "C:/Users/20000/.vulcan/skills/.system/skill-creator/assets/skill-creator-small.svg",
        body: "Use this skill when the user explicitly requests a security review.",
      },
    ],
  },
  settings: [
    { id: "runtime", label: "Runtime", description: "Core Service, LuaSkills, VLDB, and MCP", items: ["Startup mode", "Runtime directory", "MCP tool policy"] },
    { id: "providers", label: "Models & Providers", description: "Model providers, defaults, tool support, and reasoning configuration", items: ["Default provider", "Default model", "Tool calls", "Reasoning effort"] },
    { id: "remote", label: "Remote Access", description: "Future browser, mobile, and remote-client access to Core Service", items: ["Bind address", "Access token", "Remote switch", "Audit log"] },
  ],
  providers: [
    {
      id: "deepseek",
      name: "DeepSeek",
      status: "Preview",
      enabledModels: "2 enabled models",
      active: true,
      models: [
        { id: "deepseek-v4-pro", name: "DeepSeek V4 Pro", active: true },
        { id: "deepseek-v4", name: "DeepSeek V4" },
      ],
    },
    {
      id: "alibaba-cn",
      name: "Alibaba CN",
      status: "Preview",
      enabledModels: "1 enabled model",
      models: [{ id: "qwen3-coder", name: "Qwen3 Coder" }],
    },
    { id: "local", name: "Local Models", status: "Pending", enabledModels: "0", models: [] },
  ],
  reasoningSelection: {
    selectedEffort: "x-high",
    options: [
      { value: "off", label: "Off" },
      { value: "high", label: "High" },
      { value: "x-high", label: "Max" },
    ],
  },
  thinkingDisplayMode: "first_inline_with_popup",
  hostTools: {
    rtk: {
      enabled: true,
      routeMode: "selected",
      enabledRoutes: ["cargo", "find", "git", "go", "grep", "jest", "ls", "mypy", "npm", "npx", "pnpm", "pytest", "read", "ruff", "tree", "tsc", "vitest", "wc"],
      disabledRoutes: [],
      telemetryDisabled: true,
    },
  },
  aiCapabilities: [
    {
      kind: "web_search",
      label: "Web Search",
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
      label: "Image Understanding",
      available: false,
      enabled: false,
      native: false,
      bridged: false,
      targets: [],
    },
    {
      kind: "image_generation",
      label: "Image Generation",
      available: false,
      enabled: false,
      native: false,
      bridged: false,
      targets: [],
    },
  ],
  memories: [
    { id: "user-profile", title: "User Profile", category: "Profile", state: "Waiting for live VMM nodes" },
    { id: "project-memory", title: "Project Memory", category: "Project", state: "Waiting for live VMM nodes" },
    { id: "conversation-memory", title: "Conversation Memory", category: "Conversation", state: "Waiting for live VMM nodes" },
    { id: "conflict-review", title: "Conflict Review", category: "Review", state: "Waiting for live VMM conflict views" },
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
      profilePreview: "User preference: favors engineering closure.\nProject profile: WebUI now covers desktop features.",
    },
    sections: [
      { id: "user_manager", key: "USER MANAGER", label: "User Manager", description: "Resolve, precreate, and delete long-lived users." },
      { id: "project_manager", key: "PROJECT MANAGER", label: "Project Manager", description: "Resolve, precreate, delete, and migrate project paths." },
      { id: "profile_center", key: "PROFILE CENTER", label: "Profile Center", description: "Inspect active profile nodes and apply manual profile instructions." },
      { id: "profile_bundle", key: "PROFILE BUNDLE TEST", label: "Profile Bundle Test", description: "Read the complete profile bundle for the current user/project pair." },
      { id: "tools_debug", key: "TOOLS DEBUG", label: "Tools Debug", description: "Simulate AI-only tool search, detail expansion, and proactive writes." },
      { id: "memory_settings", key: "MEMORY SETTINGS", label: "Memory Settings", description: "Configure memory switches, TTL, profile refresh, and injection strategy." },
      { id: "grpc_transport", key: "GRPC TRANSPORT", label: "gRPC Transport", description: "Configure the global VMM connection and transport parameters." },
    ],
    notice: "Current project: Vulcan Code; diagnostics: ready; user/project: 20000/1001.",
  },
  commandSessions: [
    { id: "preview-command", name: "WebUI build", command: "pnpm --dir apps\\vulcan-code-web run build", status: "preview", preview: "Frontend preview command session" },
    { id: "preview-cargo", name: "Cargo check", command: "cargo check -p vulcan-code-tauri", status: "preview", preview: "Tauri desktop shell check entry" },
  ],
  logs: [
    { id: "preview-log", level: "info", source: "webui", message: "Browser preview mode loaded" },
    { id: "preview-service", level: "info", source: "service", message: "Tauri/WebUI is the current desktop entry" },
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
