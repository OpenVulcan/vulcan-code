import * as React from "react"

// LocaleCode describes supported WebUI languages.
// LocaleCode 描述 WebUI 支持的语言。
export type LocaleCode = "zh-CN" | "en-US"

// TranslationKey describes text keys currently localized at the shell level.
// TranslationKey 描述当前在外壳层支持本地化的文本键。
export type TranslationKey =
  | "app.subtitle"
  | "area.chat"
  | "area.skills"
  | "area.automation"
  | "area.plugins"
  | "area.memory"
  | "area.settings"
  | "area.chat.description"
  | "area.skills.description"
  | "area.automation.description"
  | "area.plugins.description"
  | "area.memory.description"
  | "area.settings.description"
  | "feature.status.available"
  | "feature.status.preview"
  | "feature.status.planned"
  | "global.openSettings"
  | "global.toggleDark"
  | "global.toggleLight"
  | "global.language"
  | "global.online"
  | "global.preview"
  | "console.title"
  | "console.newConversation"
  | "console.emptyPrompt"
  | "contextPanel.open"
  | "contextPanel.close"
  | "contextPanel.review"
  | "contextPanel.memory"
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

// translations stores shell-level localized labels.
// translations 保存外壳层本地化标签。
const translations: Record<LocaleCode, Record<TranslationKey, string>> = {
  "zh-CN": {
    "app.subtitle": "Ai Coding",
    "area.chat": "工作台",
    "area.skills": "技能",
    "area.automation": "自动",
    "area.plugins": "插件",
    "area.memory": "记忆",
    "area.settings": "设置",
    "area.chat.description": "项目会话、消息流与模型输入区",
    "area.skills.description": "LuaSkills、工具能力与技能包",
    "area.automation.description": "自动化任务、工作流与后台执行",
    "area.plugins.description": "插件中心与 WebUI 控制面板",
    "area.memory.description": "VMM 用户、项目与会话记忆",
    "area.settings.description": "项目、模型、工具、诊断与系统设置",
    "feature.status.available": "可用",
    "feature.status.preview": "预览",
    "feature.status.planned": "规划中",
    "global.openSettings": "打开设置",
    "global.toggleDark": "切换为暗色主题",
    "global.toggleLight": "切换为亮色主题",
    "global.language": "语言",
    "global.online": "在线",
    "global.preview": "预览",
    "console.title": "会话",
    "console.newConversation": "新会话",
    "console.emptyPrompt": "我们应该在 {projectName} 中构建什么？",
    "contextPanel.open": "打开右侧面板",
    "contextPanel.close": "关闭右侧面板",
    "contextPanel.review": "审核",
    "contextPanel.memory": "记忆",
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
  },
  "en-US": {
    "app.subtitle": "Ai Coding",
    "area.chat": "Workspace",
    "area.skills": "Skills",
    "area.automation": "Automation",
    "area.plugins": "Plugins",
    "area.memory": "Memory",
    "area.settings": "Settings",
    "area.chat.description": "Project sessions, message stream, and model input",
    "area.skills.description": "LuaSkills, tool capability, and skill packages",
    "area.automation.description": "Automation tasks, workflows, and background execution",
    "area.plugins.description": "Plugin center and WebUI control panels",
    "area.memory.description": "VMM user, project, and conversation memory",
    "area.settings.description": "Projects, models, tools, diagnostics, and system settings",
    "feature.status.available": "Available",
    "feature.status.preview": "Preview",
    "feature.status.planned": "Planned",
    "global.openSettings": "Open settings",
    "global.toggleDark": "Switch to dark theme",
    "global.toggleLight": "Switch to light theme",
    "global.language": "Language",
    "global.online": "Online",
    "global.preview": "Preview",
    "console.title": "Conversation",
    "console.newConversation": "New conversation",
    "console.emptyPrompt": "What should we build in {projectName}?",
    "contextPanel.open": "Open right panel",
    "contextPanel.close": "Close right panel",
    "contextPanel.review": "Review",
    "contextPanel.memory": "Memory",
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
  },
}

// I18nContextValue exposes the active language and translation helpers.
// I18nContextValue 暴露当前语言与翻译辅助函数。
interface I18nContextValue {
  // Current language code.
  // 当前语言代码。
  locale: LocaleCode
  // Updates the current language code.
  // 更新当前语言代码。
  setLocale: (locale: LocaleCode) => void
  // Translates one stable text key.
  // 翻译单个稳定文本键。
  t: (key: TranslationKey) => string
}

// I18nContext stores the WebUI localization state.
// I18nContext 保存 WebUI 本地化状态。
const I18nContext = React.createContext<I18nContextValue | null>(null)

// initialLocale returns the persisted language or a browser-derived default.
// initialLocale 返回持久化语言或从浏览器推导出的默认语言。
function initialLocale(): LocaleCode {
  const stored = window.localStorage.getItem("vulcan-webui-locale")
  if (stored === "zh-CN" || stored === "en-US") {
    return stored
  }
  return navigator.language.toLowerCase().startsWith("zh") ? "zh-CN" : "en-US"
}

// I18nProvider owns the WebUI language state.
// I18nProvider 管理 WebUI 语言状态。
function I18nProvider(props: {
  // Child tree that consumes localized shell labels.
  // 消费本地化外壳标签的子树。
  children: React.ReactNode
}) {
  // locale stores the active WebUI language.
  // locale 保存当前 WebUI 语言。
  const [locale, setLocaleState] = React.useState<LocaleCode>(initialLocale)
  // setLocale persists and applies one selected language.
  // setLocale 持久化并应用选中的语言。
  const setLocale = React.useCallback((nextLocale: LocaleCode) => {
    window.localStorage.setItem("vulcan-webui-locale", nextLocale)
    setLocaleState(nextLocale)
  }, [])
  // t translates one stable key using the current language.
  // t 使用当前语言翻译单个稳定键。
  const t = React.useCallback((key: TranslationKey) => translations[locale][key] ?? translations["zh-CN"][key], [locale])
  // value memoizes the provider payload to avoid unnecessary shell rerenders.
  // value 缓存 provider 载荷，避免外壳不必要重渲染。
  const value = React.useMemo(() => ({ locale, setLocale, t }), [locale, setLocale, t])

  return <I18nContext.Provider value={value}>{props.children}</I18nContext.Provider>
}

// useI18n returns the current WebUI localization helpers.
// useI18n 返回当前 WebUI 本地化辅助函数。
function useI18n(): I18nContextValue {
  const context = React.useContext(I18nContext)
  if (!context) {
    throw new Error("useI18n must be used within I18nProvider")
  }
  return context
}

export { I18nProvider, useI18n }
