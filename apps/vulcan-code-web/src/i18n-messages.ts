// LocaleCode describes supported WebUI languages.
// LocaleCode 描述 WebUI 支持的语言。
export type LocaleCode = "zh-CN" | "en-US"

// TranslationKey describes text keys currently localized at the shell level.
// TranslationKey 描述当前在外壳层支持本地化的文本键。
export type TranslationKey =
  | "app.subtitle"
  | "area.chat"
  | "area.skills"
  | "area.memory"
  | "area.settings"
  | "area.chat.description"
  | "area.skills.description"
  | "area.memory.description"
  | "area.settings.description"
  | "feature.status.available"
  | "feature.status.preview"
  | "global.openSettings"
  | "global.toggleDark"
  | "global.toggleLight"
  | "global.language"
  | "global.online"
  | "global.preview"
  | "console.title"
  | "console.newConversation"
  | "console.emptyPrompt"
  | "conversation.loading.title"
  | "conversation.loading.locating"
  | "conversation.loading.index"
  | "conversation.loading.reading"
  | "conversation.loading.projecting"
  | "conversation.loading.done"
  | "conversation.loading.failed"
  | "conversation.history.loadMore"
  | "conversation.history.loading"
  | "conversation.action.copyFinal"
  | "conversation.action.copiedFinal"
  | "conversation.action.fork"
  | "conversation.action.forkTitleSuffix"
  | "conversation.action.forkFailed"
  | "conversation.action.forkSucceeded"
  | "conversation.forkSource.label"
  | "conversation.forkSource.missing"
  | "conversation.action.editPrompt"
  | "conversation.action.editPromptFailed"
  | "conversation.action.cancel"
  | "conversation.action.saveAndReplay"
  | "contextPanel.open"
  | "contextPanel.close"
  | "contextPanel.review"
  | "contextPanel.memory"
  | "contextPanel.files"
  | "contextPanel.dynamic"
  | "contextPanel.static"
  | "contextPanel.reviewEmpty"
  | "contextPanel.memoryEmpty"
  | "contextPanel.dynamicEmpty"
  | "contextPanel.staticEmpty"
  | "contextUsage.title"
  | "contextUsage.waiting"
  | "sidebar.console"
  | "sidebar.pinned"
  | "sidebar.projects"
  | "sidebar.addProject"
  | "sidebar.noProjects"
  | "sidebar.waitingWorkspace"
  | "sidebar.overview"
  | "sidebar.settingsCategories"
  | "composer.mode.execute"
  | "composer.mode.plan"
  | "composer.capability.extended"
  | "composer.capability.extended.subtitle"
  | "composer.capability.basic"
  | "composer.capability.basic.subtitle"
  | "composer.reasoning.off"
  | "composer.reasoning.low"
  | "composer.reasoning.medium"
  | "composer.reasoning.high"
  | "composer.reasoning.max"
  | "composer.reasoning.subtitle"
  | "composer.placeholder"
  | "composer.shortcutHint"
  | "composer.selectFallback"
  | "composer.aria.input"
  | "composer.aria.mode"
  | "composer.aria.model"
  | "composer.aria.capability"
  | "composer.aria.reasoning"
  | "composer.aria.send"
  | "composer.aria.stop"
  | "composer.modelPicker.filter"
  | "composer.modelPicker.noMatches"
  | "planArtifact.titleFallback"
  | "planArtifact.copy"
  | "planArtifact.copied"
  | "planArtifact.expand"
  | "planArtifact.collapse"
  | "planArtifact.confirmAria"
  | "planArtifact.noteAria"
  | "planArtifact.notePlaceholder"
  | "planArtifact.execute"
  | "planArtifact.executeWithNote"
  | "planArtifact.defer"
  | "planArtifact.suggest"
  | "runtime.memoryRecall"
  | "runtime.requesting"
  | "runtime.thinking"
  | "runtime.toolRunning"
  | "runtime.toolActivity"
  | "runtime.subagentRunning"
  | "runtime.thinkingOverlayHint"
  | "settings.general"
  | "settings.projects"
  | "settings.providers"
  | "settings.tools"
  | "settings.diagnostics"
  | "settings.general.description"
  | "settings.projects.description"
  | "settings.providers.description"
  | "settings.tools.description"
  | "settings.diagnostics.description"
  | "settings.group.models"
  | "settings.group.aiCapabilities"
  | "settings.modelProviders"
  | "settings.modelList"
  | "settings.aiVendors"
  | "settings.aiWebSearch"
  | "settings.aiUnderstandImage"
  | "settings.aiUnderstandVideo"
  | "settings.aiUnderstandAudio"
  | "settings.aiImageGeneration"
  | "settings.aiVideoGeneration"
  | "settings.aiSpeechSynthesis"
  | "settings.aiMusicGeneration"
  | "settings.aiMusicCover"
  | "settings.mcpServers"
  | "settings.runtimeDiagnostics"
  | "settings.promptComposer"
  | "settings.aboutLicenses"
  | "settings.archivedRestore"
  | "settings.modelProviders.description"
  | "settings.modelList.description"
  | "settings.aiVendors.description"
  | "settings.aiWebSearch.description"
  | "settings.aiUnderstandImage.description"
  | "settings.aiUnderstandVideo.description"
  | "settings.aiUnderstandAudio.description"
  | "settings.aiImageGeneration.description"
  | "settings.aiVideoGeneration.description"
  | "settings.aiSpeechSynthesis.description"
  | "settings.aiMusicGeneration.description"
  | "settings.aiMusicCover.description"
  | "settings.mcpServers.description"
  | "settings.runtimeDiagnostics.description"
  | "settings.promptComposer.description"
  | "settings.aboutLicenses.description"
  | "settings.archivedRestore.description"
  | "skills.action.refresh"
  | "skills.action.refreshing"
  | "skills.action.installLua"
  | "skills.action.syncSystem"
  | "skills.action.syncing"
  | "skills.action.install"
  | "skills.action.installing"
  | "skills.action.cancel"
  | "skills.action.details"
  | "skills.action.tools"
  | "skills.action.action"
  | "skills.action.enable"
  | "skills.action.disable"
  | "skills.action.update"
  | "skills.action.delete"
  | "skills.status.enabled"
  | "skills.status.stopped"
  | "skills.label.system"
  | "skills.label.user"
  | "skills.label.source"
  | "skills.label.explicit"
  | "skills.label.implicit"
  | "skills.label.closed"
  | "skills.label.currentStatus"
  | "skills.label.installLayer"
  | "skills.label.installManagement"
  | "skills.label.runtimeEntries"
  | "skills.label.runtimeTools"
  | "skills.label.managedInstall"
  | "skills.label.unmanagedInstall"
  | "skills.label.scope"
  | "skills.label.skillFile"
  | "skills.label.skillDir"
  | "skills.label.iconPath"
  | "skills.source.unknown"
  | "skills.sidebar.luaDetail"
  | "skills.sidebar.agentDetail"
  | "skills.header.luaDetail"
  | "skills.header.agentDetail"
  | "skills.lua.emptyTitle"
  | "skills.lua.emptyDetail"
  | "skills.lua.installTitle"
  | "skills.lua.installDescription"
  | "skills.lua.sourceType"
  | "skills.lua.installSource"
  | "skills.lua.placeholderGithub"
  | "skills.lua.placeholderUrl"
  | "skills.lua.detailsTitle"
  | "skills.lua.detailsDescription"
  | "skills.lua.toolsTitle"
  | "skills.lua.toolsDescription"
  | "skills.lua.toolsEmpty"
  | "skills.lua.toolModeAria"
  | "skills.lua.toolExplicit"
  | "skills.lua.toolImplicit"
  | "skills.lua.toolDisabled"
  | "skills.lua.syncing"
  | "skills.agent.emptyTitle"
  | "skills.agent.emptyDetail"
  | "skills.agent.detailsTitle"
  | "skills.agent.detailsDescription"

// translations stores shell-level localized labels.
// translations 保存外壳层本地化标签。
export const translations: Record<LocaleCode, Record<TranslationKey, string>> = {
  "zh-CN": {
    "app.subtitle": "Ai Coding",
    "area.chat": "工作台",
    "area.skills": "技能",
    "area.memory": "记忆",
    "area.settings": "设置",
    "area.chat.description": "项目会话、消息流与模型输入区",
    "area.skills.description": "LuaSkills、工具能力与技能包",
    "area.memory.description": "VMM 用户、项目与会话记忆",
    "area.settings.description": "项目、模型、工具、诊断与系统设置",
    "feature.status.available": "可用",
    "feature.status.preview": "预览",
    "global.openSettings": "打开设置",
    "global.toggleDark": "切换为暗色主题",
    "global.toggleLight": "切换为亮色主题",
    "global.language": "语言",
    "global.online": "在线",
    "global.preview": "预览",
    "console.title": "会话",
    "console.newConversation": "新会话",
    "console.emptyPrompt": "我们应该在 {projectName} 中构建什么？",
    "conversation.loading.title": "正在载入会话",
    "conversation.loading.locating": "正在定位会话",
    "conversation.loading.index": "正在读取消息索引",
    "conversation.loading.reading": "正在读取消息内容",
    "conversation.loading.projecting": "正在整理显示数据",
    "conversation.loading.done": "会话载入完成",
    "conversation.loading.failed": "会话正文载入失败",
    "conversation.history.loadMore": "加载更多历史",
    "conversation.history.loading": "正在加载历史",
    "conversation.action.copyFinal": "复制最终输出原文",
    "conversation.action.copiedFinal": "已复制最终输出原文",
    "conversation.action.fork": "分叉会话",
    "conversation.action.forkTitleSuffix": "（分叉）",
    "conversation.action.forkFailed": "分叉失败",
    "conversation.action.forkSucceeded": "已创建分叉会话",
    "conversation.forkSource.label": "从对话中派生",
    "conversation.forkSource.missing": "原会话已经不存在",
    "conversation.action.editPrompt": "编辑用户输入",
    "conversation.action.editPromptFailed": "编辑用户输入失败",
    "conversation.action.cancel": "取消",
    "conversation.action.saveAndReplay": "保存并重新执行",
    "contextPanel.open": "打开右侧面板",
    "contextPanel.close": "关闭右侧面板",
    "contextPanel.review": "审核",
    "contextPanel.memory": "记忆",
    "contextPanel.files": "文件",
    "contextPanel.dynamic": "动态",
    "contextPanel.static": "静态",
    "contextPanel.reviewEmpty": "Git Diff 审核面板待接入",
    "contextPanel.memoryEmpty": "记忆注入信息面板待接入",
    "contextPanel.dynamicEmpty": "当前会话没有有效的动态 TTL 注入。",
    "contextPanel.staticEmpty": "当前会话没有有效的静态上下文。",
    "contextUsage.title": "上下文占用",
    "contextUsage.waiting": "请求拆分：等待预算引擎计算",
    "sidebar.console": "工作台",
    "sidebar.pinned": "置顶",
    "sidebar.projects": "项目",
    "sidebar.addProject": "添加项目",
    "sidebar.noProjects": "暂无项目",
    "sidebar.waitingWorkspace": "等待 Workspace 快照载入",
    "sidebar.overview": "概览",
    "sidebar.settingsCategories": "设置分类",
    "composer.mode.execute": "构建",
    "composer.mode.plan": "计划",
    "composer.capability.extended": "扩展能力",
    "composer.capability.extended.subtitle": "能力扩展设置",
    "composer.capability.basic": "基础能力",
    "composer.capability.basic.subtitle": "仅文本与代码",
    "composer.reasoning.off": "关闭",
    "composer.reasoning.low": "低",
    "composer.reasoning.medium": "中",
    "composer.reasoning.high": "高",
    "composer.reasoning.max": "Max",
    "composer.reasoning.subtitle": "推理等级",
    "composer.placeholder": "输入当前会话要执行的任务指令（{shortcut}）",
    "composer.shortcutHint": "⏎ 发送，Shift + ⏎ 换行",
    "composer.selectFallback": "选择",
    "composer.aria.input": "当前会话输入",
    "composer.aria.mode": "选择构建模式",
    "composer.aria.model": "选择模型",
    "composer.aria.capability": "选择扩展能力",
    "composer.aria.reasoning": "选择推理等级",
    "composer.aria.send": "发送",
    "composer.aria.stop": "停止",
    "composer.modelPicker.filter": "输入模型名称、ID 或 Provider 过滤",
    "composer.modelPicker.noMatches": "当前过滤条件下没有匹配模型。",
    "planArtifact.titleFallback": "计划",
    "planArtifact.copy": "复制 Markdown",
    "planArtifact.copied": "已复制 Markdown",
    "planArtifact.expand": "展开计划",
    "planArtifact.collapse": "折叠计划",
    "planArtifact.confirmAria": "计划确认操作",
    "planArtifact.noteAria": "计划操作补充内容",
    "planArtifact.notePlaceholder": "补充内容（可选）",
    "planArtifact.execute": "执行计划",
    "planArtifact.executeWithNote": "补充信息并执行计划",
    "planArtifact.defer": "暂不执行",
    "planArtifact.suggest": "补充计划内容",
    "runtime.memoryRecall": "✨ 正在记忆召回",
    "runtime.requesting": "✨ 正在请求",
    "runtime.thinking": "✨ 正在思考",
    "runtime.toolRunning": "✨ 正在调用工具",
    "runtime.toolActivity": "工具调用中",
    "runtime.subagentRunning": "子代理工作中",
    "runtime.thinkingOverlayHint": "模型正在流式输出思考过程",
    "settings.general": "通用设置",
    "settings.projects": "项目",
    "settings.providers": "模型",
    "settings.tools": "工具",
    "settings.diagnostics": "诊断",
    "settings.general.description": "运行时、远程访问与界面设置",
    "settings.projects.description": "工作区项目列表与绑定状态",
    "settings.providers.description": "Provider、模型、推理与工具能力",
    "settings.tools.description": "工具调用、命令会话与输出",
    "settings.diagnostics.description": "启动日志、错误与运行时事件",
    "settings.group.models": "模型设置",
    "settings.group.aiCapabilities": "AI 能力",
    "settings.modelProviders": "模型供应商",
    "settings.modelList": "模型列表",
    "settings.aiVendors": "能力供应商",
    "settings.aiWebSearch": "网页搜索",
    "settings.aiUnderstandImage": "图片理解",
    "settings.aiUnderstandVideo": "视频理解",
    "settings.aiUnderstandAudio": "音频理解",
    "settings.aiImageGeneration": "图片生成",
    "settings.aiVideoGeneration": "视频生成",
    "settings.aiSpeechSynthesis": "语音合成",
    "settings.aiMusicGeneration": "音乐生成",
    "settings.aiMusicCover": "音乐翻唱",
    "settings.mcpServers": "MCP 服务器",
    "settings.runtimeDiagnostics": "运行诊断",
    "settings.promptComposer": "提示词组合",
    "settings.aboutLicenses": "关于与许可证",
    "settings.archivedRestore": "归档恢复",
    "settings.modelProviders.description": "对话模型供应商、当前模型和已启用模型状态。",
    "settings.modelList.description": "按供应商查看已启用模型，并核对当前工作台模型。",
    "settings.aiVendors.description": "多个语义 AI 工具共用的能力供应商与路由目标。",
    "settings.aiWebSearch.description": "网页搜索能力的启用状态与可用目标。",
    "settings.aiUnderstandImage.description": "图片理解能力的启用状态与可用目标。",
    "settings.aiUnderstandVideo.description": "视频理解能力入口与接入状态。",
    "settings.aiUnderstandAudio.description": "音频理解能力入口与接入状态。",
    "settings.aiImageGeneration.description": "图片生成能力的启用状态与可用目标。",
    "settings.aiVideoGeneration.description": "视频生成能力的启用状态与可用目标。",
    "settings.aiSpeechSynthesis.description": "语音合成能力的启用状态与可用目标。",
    "settings.aiMusicGeneration.description": "音乐生成能力的启用状态与可用目标。",
    "settings.aiMusicCover.description": "音乐翻唱能力的启用状态与可用目标。",
    "settings.mcpServers.description": "MCP Runtime、服务连接、工具、提示词与资源状态。",
    "settings.runtimeDiagnostics.description": "启动链路、运行时模块与诊断日志。",
    "settings.promptComposer.description": "输入区 slash 指令、附件能力与推理选择。",
    "settings.aboutLicenses.description": "应用版本、许可证入口与合规说明。",
    "settings.archivedRestore.description": "归档项目与会话恢复入口。",
    "skills.action.refresh": "刷新",
    "skills.action.refreshing": "刷新中",
    "skills.action.installLua": "安装 LuaSkills",
    "skills.action.syncSystem": "同步系统技能",
    "skills.action.syncing": "同步中",
    "skills.action.install": "安装",
    "skills.action.installing": "安装中",
    "skills.action.cancel": "取消",
    "skills.action.details": "详情",
    "skills.action.tools": "工具",
    "skills.action.action": "操作",
    "skills.action.enable": "启用",
    "skills.action.disable": "停用",
    "skills.action.update": "更新",
    "skills.action.delete": "删除",
    "skills.status.enabled": "已启用",
    "skills.status.stopped": "已停用",
    "skills.label.system": "系统",
    "skills.label.user": "用户",
    "skills.label.source": "来源",
    "skills.label.explicit": "显式",
    "skills.label.implicit": "隐式",
    "skills.label.closed": "关闭",
    "skills.label.currentStatus": "当前状态",
    "skills.label.installLayer": "安装层级",
    "skills.label.installManagement": "安装管理",
    "skills.label.runtimeEntries": "运行时入口",
    "skills.label.runtimeTools": "运行时工具",
    "skills.label.managedInstall": "受管安装",
    "skills.label.unmanagedInstall": "非受管安装",
    "skills.label.scope": "作用域",
    "skills.label.skillFile": "技能文件",
    "skills.label.skillDir": "技能目录",
    "skills.label.iconPath": "图标路径",
    "skills.source.unknown": "未记录来源",
    "skills.sidebar.luaDetail": "{count} 个技能包",
    "skills.sidebar.agentDetail": "{count} 个提示词技能",
    "skills.header.luaDetail": "总技能数 {total} · 显示工具数 {visible} · 隐式工具数 {implicit}",
    "skills.header.agentDetail": "总技能数 {total} · 已启用 {enabled} · 加载警告 {warnings}",
    "skills.lua.emptyTitle": "等待 LuaSkills 目录",
    "skills.lua.emptyDetail": "启动完成或刷新后会显示真实技能包。",
    "skills.lua.installTitle": "安装 LuaSkills",
    "skills.lua.installDescription": "从 GitHub 仓库或 URL 安装一个 Lua Skill 包。",
    "skills.lua.sourceType": "来源类型",
    "skills.lua.installSource": "安装来源",
    "skills.lua.placeholderGithub": "LuaSkills/vulcan-codekit",
    "skills.lua.placeholderUrl": "https://example.com/skill-source.yaml",
    "skills.lua.detailsTitle": "Lua Skill 详情",
    "skills.lua.detailsDescription": "查看 Lua Skill 包详情。",
    "skills.lua.toolsTitle": "LuaSkills 工具",
    "skills.lua.toolsDescription": "管理 Lua Skill 工具可见性。",
    "skills.lua.toolsEmpty": "当前技能尚未解析到可管理的运行时工具。",
    "skills.lua.toolModeAria": "{name} 工具启用模式",
    "skills.lua.toolExplicit": "显式启用",
    "skills.lua.toolImplicit": "隐式启用",
    "skills.lua.toolDisabled": "关闭工具",
    "skills.lua.syncing": "同步中",
    "skills.agent.emptyTitle": "提示词技能目录尚未载入",
    "skills.agent.emptyDetail": "点击刷新或同步系统技能后会扫描系统目录与用户目录下的 SKILL.md。",
    "skills.agent.detailsTitle": "Agent Skill 详情",
    "skills.agent.detailsDescription": "查看 Agent Skill 详情。",
  },
  "en-US": {
    "app.subtitle": "Ai Coding",
    "area.chat": "Workspace",
    "area.skills": "Skills",
    "area.memory": "Memory",
    "area.settings": "Settings",
    "area.chat.description": "Project sessions, message stream, and model input",
    "area.skills.description": "LuaSkills, tool capability, and skill packages",
    "area.memory.description": "VMM user, project, and conversation memory",
    "area.settings.description": "Projects, models, tools, diagnostics, and system settings",
    "feature.status.available": "Available",
    "feature.status.preview": "Preview",
    "global.openSettings": "Open settings",
    "global.toggleDark": "Switch to dark theme",
    "global.toggleLight": "Switch to light theme",
    "global.language": "Language",
    "global.online": "Online",
    "global.preview": "Preview",
    "console.title": "Conversation",
    "console.newConversation": "New conversation",
    "console.emptyPrompt": "What should we build in {projectName}?",
    "conversation.loading.title": "Loading conversation",
    "conversation.loading.locating": "Locating conversation",
    "conversation.loading.index": "Reading message index",
    "conversation.loading.reading": "Reading message content",
    "conversation.loading.projecting": "Preparing display data",
    "conversation.loading.done": "Conversation loaded",
    "conversation.loading.failed": "Conversation body failed to load",
    "conversation.history.loadMore": "Load more history",
    "conversation.history.loading": "Loading history",
    "conversation.action.copyFinal": "Copy final output source",
    "conversation.action.copiedFinal": "Copied final output source",
    "conversation.action.fork": "Fork conversation",
    "conversation.action.forkTitleSuffix": " (fork)",
    "conversation.action.forkFailed": "Fork failed",
    "conversation.action.forkSucceeded": "Forked conversation created",
    "conversation.forkSource.label": "Derived from conversation",
    "conversation.forkSource.missing": "The original conversation no longer exists",
    "conversation.action.editPrompt": "Edit user input",
    "conversation.action.editPromptFailed": "Edit user input failed",
    "conversation.action.cancel": "Cancel",
    "conversation.action.saveAndReplay": "Save and replay",
    "contextPanel.open": "Open right panel",
    "contextPanel.close": "Close right panel",
    "contextPanel.review": "Review",
    "contextPanel.memory": "Memory",
    "contextPanel.files": "Files",
    "contextPanel.dynamic": "Dynamic",
    "contextPanel.static": "Static",
    "contextPanel.reviewEmpty": "Git Diff review panel is pending",
    "contextPanel.memoryEmpty": "Memory injection panel is pending",
    "contextPanel.dynamicEmpty": "This conversation has no active dynamic TTL injection.",
    "contextPanel.staticEmpty": "This conversation has no active static context.",
    "contextUsage.title": "Context usage",
    "contextUsage.waiting": "Request split: waiting for budget engine",
    "sidebar.console": "Workspace",
    "sidebar.pinned": "Pinned",
    "sidebar.projects": "Projects",
    "sidebar.addProject": "Add project",
    "sidebar.noProjects": "No projects",
    "sidebar.waitingWorkspace": "Waiting for Workspace snapshot",
    "sidebar.overview": "Overview",
    "sidebar.settingsCategories": "Settings",
    "composer.mode.execute": "Build",
    "composer.mode.plan": "Plan",
    "composer.capability.extended": "Extended Capability",
    "composer.capability.extended.subtitle": "Capability settings",
    "composer.capability.basic": "Basic Capability",
    "composer.capability.basic.subtitle": "Text and code only",
    "composer.reasoning.off": "Off",
    "composer.reasoning.low": "Low",
    "composer.reasoning.medium": "Medium",
    "composer.reasoning.high": "High",
    "composer.reasoning.max": "Max",
    "composer.reasoning.subtitle": "Reasoning level",
    "composer.placeholder": "Enter task instruction for this conversation ({shortcut})",
    "composer.shortcutHint": "⏎ Send, Shift + ⏎ New line",
    "composer.selectFallback": "Select",
    "composer.aria.input": "Current conversation input",
    "composer.aria.mode": "Select build mode",
    "composer.aria.model": "Select model",
    "composer.aria.capability": "Select extended capability",
    "composer.aria.reasoning": "Select reasoning level",
    "composer.aria.send": "Send",
    "composer.aria.stop": "Stop",
    "composer.modelPicker.filter": "Filter by model name, id, or provider",
    "composer.modelPicker.noMatches": "No models match the current filter.",
    "planArtifact.titleFallback": "Plan",
    "planArtifact.copy": "Copy Markdown",
    "planArtifact.copied": "Copied Markdown",
    "planArtifact.expand": "Expand plan",
    "planArtifact.collapse": "Collapse plan",
    "planArtifact.confirmAria": "Plan confirmation actions",
    "planArtifact.noteAria": "Plan action note",
    "planArtifact.notePlaceholder": "Additional note (optional)",
    "planArtifact.execute": "Execute plan",
    "planArtifact.executeWithNote": "Add note and execute plan",
    "planArtifact.defer": "Not now",
    "planArtifact.suggest": "Send plan note",
    "runtime.memoryRecall": "✨ Recalling memory",
    "runtime.requesting": "✨ Requesting",
    "runtime.thinking": "✨ Thinking",
    "runtime.toolRunning": "✨ Running tools",
    "runtime.toolActivity": "Running tools",
    "runtime.subagentRunning": "Subagents working",
    "runtime.thinkingOverlayHint": "The model is streaming its thinking process",
    "settings.general": "General",
    "settings.projects": "Projects",
    "settings.providers": "Models",
    "settings.tools": "Tools",
    "settings.diagnostics": "Diagnostics",
    "settings.general.description": "Runtime, remote access, and interface settings",
    "settings.projects.description": "Workspace project list and bindings",
    "settings.providers.description": "Providers, models, reasoning, and tool capability",
    "settings.tools.description": "Tool calls, command sessions, and output",
    "settings.diagnostics.description": "Startup logs, errors, and runtime events",
    "settings.group.models": "Model settings",
    "settings.group.aiCapabilities": "AI capabilities",
    "settings.modelProviders": "Model providers",
    "settings.modelList": "Model list",
    "settings.aiVendors": "Capability vendors",
    "settings.aiWebSearch": "Web search",
    "settings.aiUnderstandImage": "Image understanding",
    "settings.aiUnderstandVideo": "Video understanding",
    "settings.aiUnderstandAudio": "Audio understanding",
    "settings.aiImageGeneration": "Image generation",
    "settings.aiVideoGeneration": "Video generation",
    "settings.aiSpeechSynthesis": "Speech synthesis",
    "settings.aiMusicGeneration": "Music generation",
    "settings.aiMusicCover": "Music cover",
    "settings.mcpServers": "MCP servers",
    "settings.runtimeDiagnostics": "Runtime diagnostics",
    "settings.promptComposer": "Prompt composer",
    "settings.aboutLicenses": "About and licenses",
    "settings.archivedRestore": "Archived restore",
    "settings.modelProviders.description": "Chat model providers, current model, and enabled model status.",
    "settings.modelList.description": "Review enabled models by provider and the active workbench model.",
    "settings.aiVendors.description": "Capability vendors and route targets shared by semantic AI tools.",
    "settings.aiWebSearch.description": "Web-search capability state and available targets.",
    "settings.aiUnderstandImage.description": "Image-understanding capability state and available targets.",
    "settings.aiUnderstandVideo.description": "Video-understanding capability entry and connection status.",
    "settings.aiUnderstandAudio.description": "Audio-understanding capability entry and connection status.",
    "settings.aiImageGeneration.description": "Image-generation capability state and available targets.",
    "settings.aiVideoGeneration.description": "Video-generation capability state and available targets.",
    "settings.aiSpeechSynthesis.description": "Speech-synthesis capability state and available targets.",
    "settings.aiMusicGeneration.description": "Music-generation capability state and available targets.",
    "settings.aiMusicCover.description": "Music-cover capability state and available targets.",
    "settings.mcpServers.description": "MCP runtime, server connections, tools, prompts, and resources.",
    "settings.runtimeDiagnostics.description": "Startup flow, runtime modules, and diagnostic logs.",
    "settings.promptComposer.description": "Composer slash commands, attachment capability, and reasoning selection.",
    "settings.aboutLicenses.description": "Application version, license entry points, and compliance notes.",
    "settings.archivedRestore.description": "Archived project and conversation restore entry.",
    "skills.action.refresh": "Refresh",
    "skills.action.refreshing": "Refreshing",
    "skills.action.installLua": "Install LuaSkills",
    "skills.action.syncSystem": "Sync System Skills",
    "skills.action.syncing": "Syncing",
    "skills.action.install": "Install",
    "skills.action.installing": "Installing",
    "skills.action.cancel": "Cancel",
    "skills.action.details": "Details",
    "skills.action.tools": "Tools",
    "skills.action.action": "Actions",
    "skills.action.enable": "Enable",
    "skills.action.disable": "Disable",
    "skills.action.update": "Update",
    "skills.action.delete": "Delete",
    "skills.status.enabled": "Enabled",
    "skills.status.stopped": "Disabled",
    "skills.label.system": "System",
    "skills.label.user": "User",
    "skills.label.source": "Source",
    "skills.label.explicit": "Explicit",
    "skills.label.implicit": "Implicit",
    "skills.label.closed": "Closed",
    "skills.label.currentStatus": "Current Status",
    "skills.label.installLayer": "Install Layer",
    "skills.label.installManagement": "Install Management",
    "skills.label.runtimeEntries": "Runtime Entries",
    "skills.label.runtimeTools": "Runtime Tools",
    "skills.label.managedInstall": "Managed Install",
    "skills.label.unmanagedInstall": "Unmanaged Install",
    "skills.label.scope": "Scope",
    "skills.label.skillFile": "Skill File",
    "skills.label.skillDir": "Skill Directory",
    "skills.label.iconPath": "Icon Path",
    "skills.source.unknown": "No source recorded",
    "skills.sidebar.luaDetail": "{count} skill packages",
    "skills.sidebar.agentDetail": "{count} prompt skills",
    "skills.header.luaDetail": "Total skills {total} · Visible tools {visible} · Implicit tools {implicit}",
    "skills.header.agentDetail": "Total skills {total} · Enabled {enabled} · Load warnings {warnings}",
    "skills.lua.emptyTitle": "Waiting for LuaSkills catalog",
    "skills.lua.emptyDetail": "Real skill packages appear after startup completes or refresh runs.",
    "skills.lua.installTitle": "Install LuaSkills",
    "skills.lua.installDescription": "Install a Lua Skill package from a GitHub repository or URL.",
    "skills.lua.sourceType": "Source Type",
    "skills.lua.installSource": "Install Source",
    "skills.lua.placeholderGithub": "LuaSkills/vulcan-codekit",
    "skills.lua.placeholderUrl": "https://example.com/skill-source.yaml",
    "skills.lua.detailsTitle": "Lua Skill Details",
    "skills.lua.detailsDescription": "View Lua Skill package details.",
    "skills.lua.toolsTitle": "LuaSkills Tools",
    "skills.lua.toolsDescription": "Manage Lua Skill tool visibility.",
    "skills.lua.toolsEmpty": "This skill has no manageable runtime tools yet.",
    "skills.lua.toolModeAria": "{name} tool enablement mode",
    "skills.lua.toolExplicit": "Explicit",
    "skills.lua.toolImplicit": "Implicit",
    "skills.lua.toolDisabled": "Disable",
    "skills.lua.syncing": "Syncing",
    "skills.agent.emptyTitle": "Prompt skill catalog is not loaded",
    "skills.agent.emptyDetail": "Refresh or sync system skills to scan SKILL.md files under system and user directories.",
    "skills.agent.detailsTitle": "Agent Skill Details",
    "skills.agent.detailsDescription": "View Agent Skill details.",
  },
}
