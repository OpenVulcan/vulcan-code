import { readdir, readFile } from "node:fs/promises"
import { dirname, extname, join, relative, resolve } from "node:path"
import { fileURLToPath } from "node:url"

// scriptDirectory is the stable location used to resolve package sources.
// scriptDirectory 是用于解析包源码的稳定位置。
const scriptDirectory = dirname(fileURLToPath(import.meta.url))
// sourceRoot is the WebUI source tree checked by architectural guards.
// sourceRoot 是架构守卫检查的 WebUI 源码树。
const sourceRoot = resolve(scriptDirectory, "../src")
// sourceExtensions contains the source formats relevant to UI architecture.
// sourceExtensions 包含与 UI 架构相关的源码格式。
const sourceExtensions = new Set([".ts", ".tsx", ".css"])
// transcriptScrollOwners render the conversation transcript whose scroll position only the shared scroll controller may write.
// transcriptScrollOwners 渲染会话转录，其滚动位置只能由共享滚动控制器写入。
const transcriptScrollOwners = new Set([
  "components/conversation/chat-area.tsx",
  "components/conversation/conversation-workspace.tsx",
])
// componentCodeLineLimit is the single maximum number of code lines one component function may span.
// componentCodeLineLimit 是单个组件函数可占用代码行数的唯一上限。
const componentCodeLineLimit = 400
// oversizedComponentCeilings ratchets components that already exceeded the limit: each may shrink but never grow,
// and its entry must be removed once the component fits the limit. Keys are "<src-relative path>#<Component>".
// oversizedComponentCeilings 对已超限组件做棘轮约束：只能缩小不能增长，组件回到上限内后必须删除对应条目。
// 键格式为 "<src 相对路径>#<组件名>"。
const oversizedComponentCeilings = new Map([
  ["app/workbench-app.tsx#WorkbenchApp", 627],
  ["components/composer/composer-panel.tsx#ComposerPanel", 708],
  ["components/conversation/chat-area.tsx#ChatArea", 496],
  ["components/conversation/conversation-workspace.tsx#ConversationWorkspace", 471],
  ["components/features/browser/browser-area.tsx#BrowserArea", 753],
  ["components/features/memory/vmm-data-page.tsx#VmmDataPage", 472],
  ["components/features/memory/vmm-identity-project-page.tsx#VmmIdentityProjectPage", 411],
  ["components/features/memory/vmm-native-memory-records.tsx#VmmNativeMemoryRecords", 492],
  ["components/features/memory/vmm-profile-page.tsx#VmmProfilePage", 405],
  ["components/features/settings/custom-provider-models-dialog.tsx#CustomProviderModelsDialog", 553],
  ["components/features/settings/provider-list-settings.tsx#PublicAiConnectionDialog", 494],
  ["components/features/settings/provider-management-settings/custom-provider-dialog.tsx#CustomProviderDialog", 543],
  ["components/features/settings/provider-manifest-status.tsx#ProviderManifestStatusPanel", 498],
  ["components/features/settings/provider-model-settings.tsx#ProviderModelCatalog", 533],
  ["components/features/skills/lua-skill-config-dialog.tsx#LuaSkillConfigDialog", 507],
  ["components/features/skills/vulcan-plugins-panel.tsx#VulcanPluginsPanel", 699],
  ["components/project/project-settings-dialog.tsx#ProjectSettingsDialog", 613],
  ["components/project/terminal/project-floating-terminal-layer.tsx#ProjectFloatingTerminalLayer", 550],
  // Workbench orchestration has an explicit larger budget to allow routine shell integrations.
  // 工作台编排层采用明确的较大预算，为常规壳层集成保留空间。
  ["components/workspace/workbench-layout.tsx#WorkbenchLayout", 1500],
  ["components/workspace/workspace-review-panel-host.tsx#WorkspaceReviewPanelHost", 589],
  ["components/workspace/workspace-shell.tsx#WorkspaceShellContent", 781],
  ["diff/DiffReviewPanel/panel.tsx#DiffReviewPanel", 523],
])

// componentFunctionCodeLines measures every top-level component function in one TSX source.
// componentFunctionCodeLines 测量一个 TSX 源码中的每个顶层组件函数。
// Parameters: source is the exact file text. Returns name and code-line count pairs; blank and comment-only lines are not code.
// 参数：source 是精确文件文本。返回名称与代码行数对；空行与纯注释行不计为代码。
export function componentFunctionCodeLines(source) {
  // lines is the file split independently of its line terminator.
  // lines 是与换行符无关的文件分行结果。
  const lines = source.split(/\r?\n/)
  // components accumulates measured component functions.
  // components 累积已测量的组件函数。
  const components = []
  for (let start = 0; start < lines.length; start += 1) {
    // declaration matches PascalCase function declarations and memo/forwardRef-wrapped named functions.
    // declaration 匹配 PascalCase 函数声明以及 memo/forwardRef 包装的具名函数。
    const declaration = /^(?:export\s+)?(?:default\s+)?(?:function\s+([A-Z]\w*)\s*[(<]|const\s+([A-Z]\w*)\s*=\s*(?:React\.)?(?:memo|forwardRef)\(\s*function\b)/.exec(lines[start])
    if (!declaration) continue
    // end is the column-zero closing line of the top-level declaration.
    // end 是顶层声明位于首列的结束行。
    let end = start + 1
    while (end < lines.length && !/^}\)*;?\s*$/.test(lines[end])) end += 1
    // codeLines excludes blank lines, line comments, block-comment lines and JSX comments.
    // codeLines 排除空行、行注释、块注释行与 JSX 注释。
    let codeLines = 0
    for (let index = start; index <= end && index < lines.length; index += 1) {
      const line = lines[index].trim()
      if (line && !/^(?:\/\/|\/\*|\*|\{\/\*.*\*\/\}$)/.test(line)) codeLines += 1
    }
    components.push({ name: declaration[1] ?? declaration[2], codeLines })
    start = end
  }
  return components
}

// collectSourceFiles recursively returns relevant source files in stable order.
// collectSourceFiles 递归返回稳定排序的相关源码文件。
async function collectSourceFiles(directory) {
  // entries contains the exact directory children.
  // entries 包含精确的目录子项。
  const entries = await readdir(directory, { withFileTypes: true })
  // files accumulates relevant descendants.
  // files 累积相关后代文件。
  const files = []
  for (const entry of entries) {
    // path is the exact current child path.
    // path 是精确的当前子路径。
    const path = join(directory, entry.name)
    if (entry.isDirectory()) {
      files.push(...await collectSourceFiles(path))
    } else if (sourceExtensions.has(extname(entry.name))) {
      files.push(path)
    }
  }
  return files.sort()
}

// feedbackShellPattern matches class names of hand-built empty-state and notice shells; page kit classes are exempt.
// feedbackShellPattern 匹配手写空状态与提示外壳的类名；页面套件类名不受限制。
const feedbackShellPattern = /^(?!page-)[a-z0-9-]+-(?:empty-state|empty|notice)$/

// legacyFeedbackShells is the ratchet of bespoke feedback shells that predate the page kit; it may only shrink.
// New pages use PageEmptyState and PageNotice; inline widgets that truly cannot must justify a new entry in review.
// legacyFeedbackShells 是页面套件之前遗留的自定义反馈外壳棘轮表，只能缩减。
// 新页面使用 PageEmptyState 与 PageNotice；确实无法使用的行内控件须在评审中说明理由后再新增条目。
const legacyFeedbackShells = new Set([
  "composer-capability-dialog-empty", "composer-capability-summary-empty", "context-injection-delete-notice",
  "conversation-session-commit-empty", "diff-empty-state", "diff-file-tree-empty", "file-change-empty", "knowledge-empty",
  "markdown-tool-empty", "model-account-card-empty", "model-picker-empty", "project-folder-picker-empty",
  "project-folder-picker-notice", "project-terminal-empty", "session-review-repository-notice", "settings-archive-empty",
  "settings-error-notice", "subagent-orchestration-empty", "system-error-reconciliation-notice", "workspace-files-panel-notice",
])

// checkUiArchitecture validates boundaries that must not regress through ordinary component work.
// checkUiArchitecture 验证普通组件工作不得破坏的架构边界。
// Parameters: root is the source tree; componentCeilings and feedbackShells default to the real-tree ratchets only when root is the real tree.
// 参数：root 是源码树；仅当 root 为真实源码树时 componentCeilings 与 feedbackShells 才默认使用真实棘轮表。
export async function checkUiArchitecture(
  root = sourceRoot,
  componentCeilings = root === sourceRoot ? oversizedComponentCeilings : new Map(),
  feedbackShells = root === sourceRoot ? legacyFeedbackShells : new Set(),
) {
  // files is the exact source inventory under review.
  // files 是当前评审的精确源码清单。
  const files = await collectSourceFiles(root)
  // failures accumulates file-specific actionable violations.
  // failures 累积具体到文件的可操作违规项。
  const failures = []
  // measuredCeilings records ratchet entries whose component still exceeds the limit.
  // measuredCeilings 记录其组件仍超限的棘轮条目。
  const measuredCeilings = new Set()
  // seenFeedbackShells records ratchet entries still referenced by a component.
  // seenFeedbackShells 记录仍被组件引用的棘轮条目。
  const seenFeedbackShells = new Set()
  for (const path of files) {
    // name is the package-relative path used in diagnostics.
    // name 是诊断使用的包相对路径。
    const name = relative(root, path).replace(/\\/g, "/")
    // source is the exact current file text.
    // source 是精确的当前文件文本。
    const source = await readFile(path, "utf8")
    if (/\<select(?:\s|>)/.test(source)) {
      failures.push(`${name}: native <select> is forbidden; use the shared Select contract`)
    }
    if (/components\/ui\/(item|resizable|skeleton)/.test(source)) {
      failures.push(`${name}: imports a removed unused UI component`)
    }
    // ownsDeferredIconCatalog identifies the two exact source-isolated catalog chunks.
    // ownsDeferredIconCatalog 标识两个按来源隔离的精确延迟图标目录分块。
    const ownsDeferredIconCatalog = /^components\/project\/project-icon-catalog-(material|skill)\.ts$/.test(name)
    if (!ownsDeferredIconCatalog && /@iconify-json\/(material-icon-theme|skill-icons)/.test(source)) {
      failures.push(`${name}: full icon catalogs are only allowed in their source-isolated deferred catalog`)
    }
    if (!name.includes(".test.") && /focus-visible:ring-3|aria-invalid:ring-3/.test(source)) {
      failures.push(`${name}: three-pixel control focus rings are forbidden`)
    }
    if (transcriptScrollOwners.has(name) && /\.scrollTop\s*=(?!=)|\.scroll(?:To|By)\(/.test(source)) {
      failures.push(`${name}: transcript scroll writes belong to chat-area/conversation-scroll-controller`)
    }
    if (name.includes(".test.") && (
      /\.tsx?\?raw["'`]/.test(source)
      || (/\breadFile(?:Sync)?\(|import\.meta\.glob\(/.test(source) && /["'`][^"'`\n]*\.tsx?["'`]/.test(source))
    )) {
      failures.push(`${name}: tests must not read TypeScript source text; assert rendered behavior or exported contracts instead`)
    }
    if (name.endsWith(".tsx") && !name.includes(".test.")) {
      // shells lists this file's bespoke feedback shell classes, reported once each. The ui-kit owns the base
      // empty and notice primitives that the page kit builds on, so its own classes are not bespoke.
      // shells 列出本文件的自定义反馈外壳类名，每个只报告一次。ui-kit 拥有页面套件所依赖的基础空状态与提示原语，
      // 因此其自身类名不属于自定义外壳。
      const shells = name.startsWith("ui-kit/") ? [] : [...new Set(moduleClassNames(source).filter((token) => feedbackShellPattern.test(token)))]
      for (const shell of shells) {
        if (feedbackShells.has(shell)) seenFeedbackShells.add(shell)
        else failures.push(`${name}: bespoke feedback shell "${shell}"; use PageEmptyState or PageNotice from @/components/page`)
      }
      for (const component of componentFunctionCodeLines(source)) {
        // key identifies the component in the ratchet table.
        // key 在棘轮表中标识该组件。
        const key = `${name}#${component.name}`
        // ceiling is the ratchet maximum for an already oversized component.
        // ceiling 是已超限组件的棘轮上限。
        const ceiling = componentCeilings.get(key)
        if (component.codeLines <= componentCodeLineLimit) continue
        if (ceiling === undefined) {
          failures.push(`${key}: component function has ${component.codeLines} code lines (limit ${componentCodeLineLimit}); extract hooks or child components`)
          continue
        }
        measuredCeilings.add(key)
        if (component.codeLines > ceiling) {
          failures.push(`${key}: oversized component grew to ${component.codeLines} code lines (ratchet ceiling ${ceiling}); extract hooks or child components before adding more`)
        }
      }
    }
  }
  for (const key of componentCeilings.keys()) {
    if (!measuredCeilings.has(key)) {
      failures.push(`${key}: component no longer exceeds ${componentCodeLineLimit} code lines or was renamed; remove its oversizedComponentCeilings entry`)
    }
  }
  for (const shell of feedbackShells) {
    if (!seenFeedbackShells.has(shell)) {
      failures.push(`${shell}: feedback shell is no longer used; remove its legacyFeedbackShells entry`)
    }
  }
  return failures
}

// pluginsRoot holds the bundled plugin packages whose UI modules follow the plugin style contract.
// pluginsRoot 保存其 UI 模块遵循插件样式契约的内置插件包。
const pluginsRoot = resolve(scriptDirectory, "../../../plugins")
// pluginStyleVocabulary is the published stylesheet whose classes plugin UI modules may name.
// pluginStyleVocabulary 是插件 UI 模块可以引用其类名的已发布样式表。
const pluginStyleVocabulary = resolve(sourceRoot, "styles/plugin-host-components.css")
// pluginStyleRules are the custom-style limits of docs/contracts/plugin-ui-authoring.md; each pattern runs on non-test UI modules.
// pluginStyleRules 是 docs/contracts/plugin-ui-authoring.md 中的自定义样式限制；每条规则作用于非测试 UI 模块。
const pluginStyleRules = [
  [/createElement\(\s*["'`]style["'`]|\bh\(\s*["'`]style["'`]|<style[\s>]|\bCSSStyleSheet\b|adoptedStyleSheets/, "style elements and stylesheet injection are forbidden"],
  [/\bdocument\.|\binnerHTML\b|\bouterHTML\b|insertAdjacentHTML|dangerouslySetInnerHTML/, "direct DOM access is forbidden; render through sdk.React and sdk.components"],
  [/["'`](?:[^"'`\n]*\s)?#[0-9a-fA-F]{3,8}\b|\b(?:rgba?|hsla?|oklch|oklab|lab|lch|hwb)\(/, "color literals are forbidden; use host theme tokens such as var(--primary)"],
  [/\bposition\s*:\s*["'`](?:fixed|sticky)|\bzIndex\b|z-index/, "fixed or sticky positioning and z-index are reserved for the host shell"],
  [/\bfontSize\s*:\s*(?:\d|["'`][^"'`]*px)/, "pixel font sizes are forbidden; use host typography tokens such as var(--vcn-font-size-sm)"],
]

// stylesheetClassNames returns every class selector declared in one stylesheet.
// stylesheetClassNames 返回一个样式表中声明的全部类选择器。
// Parameters: css is the exact stylesheet text. Returns the class names without their leading dot.
// 参数：css 是精确样式表文本。返回不含前导点号的类名。
export function stylesheetClassNames(css) {
  // selectors is the stylesheet with comments removed so commented class names do not count.
  // selectors 是移除注释后的样式表，使注释中的类名不被计入。
  const selectors = css.replace(/\/\*[\s\S]*?\*\//g, "")
  return new Set([...selectors.matchAll(/\.(-?[_a-zA-Z][\w-]*)/g)].map((match) => match[1]))
}

// moduleClassNames returns every class token written in a className property of one plugin UI module.
// moduleClassNames 返回一个插件 UI 模块 className 属性中写出的全部类名标记。
// Parameters: source is the exact module text. Returns literal tokens; template interpolations are skipped.
// 参数：source 是精确模块文本。返回字面量标记；模板插值部分被跳过。
export function moduleClassNames(source) {
  // tokens accumulates class tokens in source order.
  // tokens 按源码顺序累积类名标记。
  const tokens = []
  for (const match of source.matchAll(/className\s*[:=]\s*\{?((?:\$\{[^}]*\}|[^,}\n])+)/g)) {
    for (const literal of match[1].matchAll(/"([^"]*)"|'([^']*)'|`([^`]*)`/g)) {
      const value = (literal[1] ?? literal[2] ?? literal[3]).replace(/\$\{[^}]*\}/g, " ")
      tokens.push(...value.split(/\s+/).filter(Boolean))
    }
  }
  return tokens
}

// checkPluginUiStyle enforces the plugin custom-style contract on every bundled plugin UI module.
// checkPluginUiStyle 对每个内置插件 UI 模块执行插件自定义样式契约。
// Parameters: root is the plugins directory; vocabularyPath is the published plugin stylesheet.
// 参数：root 是插件目录；vocabularyPath 是已发布的插件样式表。
// Returns actionable violations named by plugin-relative path.
// 返回以插件相对路径命名的可操作违规项。
export async function checkPluginUiStyle(root = pluginsRoot, vocabularyPath = pluginStyleVocabulary) {
  // vocabulary is the exact set of class names plugins may reference.
  // vocabulary 是插件可以引用的精确类名集合。
  const vocabulary = stylesheetClassNames(await readFile(vocabularyPath, "utf8"))
  // failures accumulates violations across all plugins.
  // failures 累积全部插件的违规项。
  const failures = []
  for (const plugin of (await readdir(root, { withFileTypes: true })).filter((entry) => entry.isDirectory()).map((entry) => entry.name).sort()) {
    // uiDirectory is the conventional location of one plugin's mounted modules.
    // uiDirectory 是单个插件挂载模块的约定位置。
    const uiDirectory = join(root, plugin, "ui")
    // modules lists the plugin's runtime UI modules; tests may stub freely.
    // modules 列出插件的运行时 UI 模块；测试可以自由打桩。
    const modules = await readdir(uiDirectory).catch(() => [])
    for (const file of modules.filter((name) => name.endsWith(".mjs") && !name.endsWith(".test.mjs")).sort()) {
      // name is the diagnostic path relative to the plugins directory.
      // name 是相对插件目录的诊断路径。
      const name = `${plugin}/ui/${file}`
      // source is the module text with comments removed so prose cannot trigger a rule.
      // source 是移除注释后的模块文本，使说明文字不会触发规则。
      const source = (await readFile(join(uiDirectory, file), "utf8")).replace(/\/\*[\s\S]*?\*\/|(^|[^:"'`])\/\/.*$/gm, "$1")
      for (const [pattern, message] of pluginStyleRules) {
        if (pattern.test(source)) failures.push(`${name}: ${message}`)
      }
      // unknown lists class tokens outside the published vocabulary, reported once each.
      // unknown 列出不在已发布词汇表中的类名标记，每个只报告一次。
      const unknown = [...new Set(moduleClassNames(source).filter((token) => !vocabulary.has(token)))]
      if (unknown.length > 0) {
        failures.push(`${name}: class names ${unknown.join(", ")} are not defined in styles/plugin-host-components.css; use page kit components or add a plugin-scoped rule there`)
      }
    }
  }
  return failures
}

// runUiArchitectureCheck prints every violation and fails the calling process.
// runUiArchitectureCheck 输出每个违规项并使调用进程失败。
async function runUiArchitectureCheck() {
  // failures contains the current exact architecture and plugin style violations.
  // failures 包含当前精确的架构与插件样式违规项。
  const failures = [...await checkUiArchitecture(), ...await checkPluginUiStyle()]
  if (failures.length > 0) {
    throw new Error(`WebUI architecture check failed:\n- ${failures.join("\n- ")}`)
  }
  process.stdout.write("WebUI architecture check passed.\n")
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await runUiArchitectureCheck()
}
