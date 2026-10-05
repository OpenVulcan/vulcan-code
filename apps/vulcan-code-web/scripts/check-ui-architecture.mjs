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
  ["components/composer/composer-panel.tsx#ComposerPanel", 727],
  ["components/conversation/chat-area.tsx#ChatArea", 496],
  ["components/conversation/conversation-workspace.tsx#ConversationWorkspace", 471],
  ["components/dev/visual-lab-settings-area.tsx#VisualLabSettingsArea", 469],
  ["components/features/browser/browser-area.tsx#BrowserArea", 753],
  ["components/features/code-context/code-context-area.tsx#CodeContextArea", 423],
  ["components/features/knowledge/knowledge-base-area.tsx#KnowledgeBaseArea", 478],
  ["components/features/memory/vmm-data-page.tsx#VmmDataPage", 472],
  ["components/features/memory/vmm-identity-project-page.tsx#VmmIdentityProjectPage", 411],
  ["components/features/memory/vmm-native-memory-records.tsx#VmmNativeMemoryRecords", 492],
  ["components/features/memory/vmm-profile-page.tsx#VmmProfilePage", 405],
  ["components/features/settings/custom-provider-models-dialog.tsx#CustomProviderModelsDialog", 553],
  ["components/features/settings/provider-list-settings.tsx#PublicAiConnectionDialog", 494],
  ["components/features/settings/provider-management-settings/custom-provider-dialog.tsx#CustomProviderDialog", 543],
  ["components/features/settings/provider-manifest-status.tsx#ProviderManifestStatusPanel", 498],
  ["components/features/settings/provider-model-settings.tsx#ModelListSettingsArea", 573],
  ["components/features/skills/lua-skill-config-dialog.tsx#LuaSkillConfigDialog", 507],
  ["components/features/skills/vulcan-plugins-panel.tsx#VulcanPluginsPanel", 746],
  ["components/project/project-settings-dialog.tsx#ProjectSettingsDialog", 613],
  ["components/project/terminal/project-floating-terminal-layer.tsx#ProjectFloatingTerminalLayer", 550],
  ["components/workspace/workbench-layout.tsx#WorkbenchLayout", 875],
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

// checkUiArchitecture validates boundaries that must not regress through ordinary component work.
// checkUiArchitecture 验证普通组件工作不得破坏的架构边界。
// Parameters: root is the source tree; componentCeilings defaults to the real-tree ratchet only when root is the real tree.
// 参数：root 是源码树；仅当 root 为真实源码树时 componentCeilings 才默认使用真实棘轮表。
export async function checkUiArchitecture(
  root = sourceRoot,
  componentCeilings = root === sourceRoot ? oversizedComponentCeilings : new Map(),
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
  return failures
}

// runUiArchitectureCheck prints every violation and fails the calling process.
// runUiArchitectureCheck 输出每个违规项并使调用进程失败。
async function runUiArchitectureCheck() {
  // failures contains the current exact architecture violations.
  // failures 包含当前精确的架构违规项。
  const failures = await checkUiArchitecture()
  if (failures.length > 0) {
    throw new Error(`WebUI architecture check failed:\n- ${failures.join("\n- ")}`)
  }
  process.stdout.write("WebUI architecture check passed.\n")
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await runUiArchitectureCheck()
}
