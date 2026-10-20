import * as React from "react"
import { FileCode2, LockKeyhole, Play, RefreshCw, RotateCcw, Save, ShieldAlert, UnlockKeyhole } from "lucide-react"

import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/ui-kit/primitives/alert-dialog"
import { Badge } from "@/ui-kit/primitives/badge"
import { InlineStreamPlaceholder, StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder"
import { Button } from "@/ui-kit/primitives/button"
import { PageTabs } from "@/components/workspace/workspace-directory-layout"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/ui-kit/primitives/card"
import { Input } from "@/ui-kit/primitives/input"
import { Label } from "@/ui-kit/primitives/label"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select"
import { Switch } from "@/ui-kit/primitives/switch"
import { Textarea } from "@/ui-kit/primitives/textarea"
import {
  highlightedDiffLineSpans,
  useDiffShikiTheme,
  useHighlightedCodeBlockRevision,
} from "@/diff/diff-highlight"
import { escapeHtml, tokenSpansToHtml } from "@/diff/diff-shiki-shared"
import { useI18n } from "@/i18n"
import "@/ui-kit/styles/features/settings-forms.css";
import type {
  VmmMemoryActionResult,
  VmmNativeNoiseTestResult,
  VmmNativePiiFormulaTestResult,
  VmmNativePiiTestResult,
  VmmNativeResourceCatalogSummary,
  VmmNativeResourceFileContent,
  VmmNativeResourceSettingsSummary,
} from "@/types"
import "@/ui-kit/styles/features/memory.css"

// ResourceGroup identifies one user-facing resource category.
// ResourceGroup 标识一个面向用户的资源分类。
type ResourceGroup = "prompt" | "pii" | "noise"

// VmmNativeResourcesPageProps binds the native-only resource editor to the shared action channel.
// VmmNativeResourcesPageProps 将仅原生资源编辑器绑定到共享动作通道。
interface VmmNativeResourcesPageProps {
  // Whether another memory operation currently owns the shared action channel.
  // 是否有其他记忆操作正在占用共享动作通道。
  busy: boolean
  // Optional initial category used by tests and future category-aware navigation.
  // 供测试及未来分类感知导航使用的可选初始分类。
  initialGroup?: ResourceGroup
  // Executes one native resource action through the host service.
  // 通过宿主服务执行一个原生资源动作。
  onRunAction: (
    action: string,
    payload?: Record<string, unknown>,
  ) => Promise<VmmMemoryActionResult | null>
}

// ResourceConfirmation identifies one explicit risky file operation awaiting confirmation.
// ResourceConfirmation 标识一个等待明确确认的高风险文件操作。
type ResourceConfirmation = "unlock" | "save" | "reset" | null

// VmmNativeResourcesPage renders categorized effective resources with guarded user overrides.
// VmmNativeResourcesPage 渲染分类后的当前生效资源，并保护用户覆盖操作。
export function VmmNativeResourcesPage({ busy, initialGroup = "prompt", onRunAction }: VmmNativeResourcesPageProps) {
  const { t } = useI18n()
  const [activeGroup, setActiveGroup] = React.useState<ResourceGroup>(initialGroup)
  const [catalog, setCatalog] = React.useState<VmmNativeResourceCatalogSummary | null>(null)
  const [settings, setSettings] = React.useState<VmmNativeResourceSettingsSummary | null>(null)
  const [selectedPath, setSelectedPath] = React.useState("")
  const [file, setFile] = React.useState<VmmNativeResourceFileContent | null>(null)
  // filePending preserves editor geometry while preventing edits to the previous file during a read.
  // filePending 在读取期间保持编辑器布局，同时禁止编辑上一个文件。
  const [filePending, setFilePending] = React.useState(false)
  // fileReadRevision lets an explicit refresh retry a failed read of the same selected resource.
  // fileReadRevision 允许显式刷新重试同一选中资源的失败读取。
  const [fileReadRevision, setFileReadRevision] = React.useState(0)
  const [editorText, setEditorText] = React.useState("")
  const [editUnlocked, setEditUnlocked] = React.useState(false)
  const [confirmation, setConfirmation] = React.useState<ResourceConfirmation>(null)
  const [piiInput, setPiiInput] = React.useState("")
  const [piiResult, setPiiResult] = React.useState<VmmNativePiiTestResult | null>(null)
  const [piiFormulaInput, setPiiFormulaInput] = React.useState("")
  const [piiFormulaPattern, setPiiFormulaPattern] = React.useState(String.raw`\b(\d+)\b`)
  const [piiFormulaCondition, setPiiFormulaCondition] = React.useState("len($1) == 4")
  const [piiFormulaReplacement, setPiiFormulaReplacement] = React.useState("[TEST_MASKED]")
  const [piiFormulaResult, setPiiFormulaResult] = React.useState<VmmNativePiiFormulaTestResult | null>(null)
  const [noiseInput, setNoiseInput] = React.useState("")
  const [noiseResult, setNoiseResult] = React.useState<VmmNativeNoiseTestResult | null>(null)
  const [error, setError] = React.useState<string | null>(null)

  // acceptCatalog publishes one authoritative catalog and its effective composition settings.
  // acceptCatalog 发布一份权威目录及其当前生效组合设置。
  const acceptCatalog = React.useCallback((next: VmmNativeResourceCatalogSummary) => {
    setCatalog(next)
    setSettings({ ...next.settings })
  }, [])

  // loadCatalog refreshes settings and exact physical override state.
  // loadCatalog 刷新设置与准确物理覆盖状态。
  const loadCatalog = React.useCallback(async () => {
    const result = await onRunAction("native_resource_catalog")
    if (result?.data.kind !== "native_resource_catalog") {
      setError(t("memory.resources.invalidResponse"))
      return
    }
    acceptCatalog(result.data.value)
    setError(null)
  }, [acceptCatalog, onRunAction, t])

  React.useEffect(() => {
    void loadCatalog()
  }, [loadCatalog])

  const visibleFiles = React.useMemo(
    () => catalog?.files.filter((entry) => entry.group === activeGroup) ?? [],
    [activeGroup, catalog],
  )
  // effectiveSelector reflects saved configuration, not the composition draft.
  // effectiveSelector 反映已保存配置，而非组合草稿。
  const effectiveSelector = activeGroup === "prompt" ? catalog?.settings.promptBundle : activeGroup === "pii" ? catalog?.settings.piiLanguage : catalog?.settings.noiseLanguage

  React.useEffect(() => {
    if (visibleFiles.some((entry) => entry.path === selectedPath)) return
    // selector comes from the saved catalog, independently of unsaved composition edits.
    // selector 来自已保存目录，独立于尚未保存的组合编辑。
    const selector = activeGroup === "prompt" ? catalog?.settings.promptBundle : activeGroup === "pii" ? catalog?.settings.piiLanguage : catalog?.settings.noiseLanguage
    const activeFile = visibleFiles.find((entry) => entry.selector === selector)
    setSelectedPath(activeFile?.path ?? visibleFiles[0]?.path ?? "")
  }, [activeGroup, catalog, selectedPath, visibleFiles])

  React.useEffect(() => {
    if (!selectedPath) {
      setFile(null)
      setFilePending(false)
      setEditorText("")
      setEditUnlocked(false)
      return
    }
    let active = true
    setFilePending(true)
    setEditUnlocked(false)
    setConfirmation(null)
    void onRunAction("native_resource_read", { path: selectedPath }).then((result) => {
      if (!active) return
      setFilePending(false)
      if (result?.data.kind !== "native_resource_file") {
        setFile(null)
        setEditorText("")
        setError(t("memory.resources.invalidResponse"))
        return
      }
      setFile(result.data.value)
      setEditorText(result.data.value.effectiveText)
      setError(null)
    })
    return () => {
      active = false
    }
  }, [fileReadRevision, onRunAction, selectedPath, t])

  // saveSettings validates and hot-activates one complete common-plus-locale composition.
  // saveSettings 校验并热更新一份完整的 common 加区域组合。
  const saveSettings = React.useCallback(async () => {
    if (!settings) return
    const result = await onRunAction("native_resource_settings_update", { ...settings })
    if (result?.data.kind !== "native_resource_catalog") {
      setError(t("memory.resources.invalidResponse"))
      return
    }
    acceptCatalog(result.data.value)
    setError(null)
  }, [acceptCatalog, onRunAction, settings, t])

  // saveFile writes the selected effective text only after the separate save confirmation.
  // saveFile 仅在单独保存确认后写入所选当前生效文本。
  const saveFile = React.useCallback(async () => {
    if (!file || !editUnlocked || editorText === file.effectiveText) return false
    const result = await onRunAction("native_resource_write", {
      path: file.path,
      content: editorText,
      confirmedPurpose: true,
      confirmedModification: true,
    })
    if (result?.data.kind !== "native_resource_file") {
      setError(t("memory.resources.invalidResponse"))
      return false
    }
    setFile(result.data.value)
    setEditorText(result.data.value.effectiveText)
    setEditUnlocked(false)
    setError(null)
    await loadCatalog()
    return true
  }, [editUnlocked, editorText, file, loadCatalog, onRunAction, t])

  // resetFile removes only the selected user override after explicit confirmation.
  // resetFile 在明确确认后仅移除所选用户覆盖。
  const resetFile = React.useCallback(async () => {
    if (!file) return false
    const result = await onRunAction("native_resource_reset", { path: file.path })
    if (result?.data.kind !== "native_resource_file") {
      setError(t("memory.resources.invalidResponse"))
      return false
    }
    setFile(result.data.value)
    setEditorText(result.data.value.effectiveText)
    setEditUnlocked(false)
    setError(null)
    await loadCatalog()
    return true
  }, [file, loadCatalog, onRunAction, t])

  // testPii executes the active common-plus-locale scrubber without invoking an LLM.
  // testPii 执行当前 common 加区域清洗器，且不调用 LLM。
  const testPii = React.useCallback(async () => {
    const result = await onRunAction("native_pii_test", { text: piiInput })
    if (result?.data.kind !== "native_pii_test") {
      setError(t("memory.resources.invalidResponse"))
      return
    }
    setPiiResult(result.data.value)
    setError(null)
  }, [onRunAction, piiInput, t])

  // testPiiFormula compiles one temporary regex, condition, and replacement in Core.
  // testPiiFormula 在 Core 中编译一条临时正则、条件与替换。
  const testPiiFormula = React.useCallback(async () => {
    const result = await onRunAction("native_pii_formula_test", {
      text: piiFormulaInput,
      pattern: piiFormulaPattern,
      condition: piiFormulaCondition,
      replacement: piiFormulaReplacement,
    })
    if (result?.data.kind !== "native_pii_formula_test") {
      setError(t("memory.resources.invalidResponse"))
      return
    }
    setPiiFormulaResult(result.data.value)
    setError(null)
  }, [onRunAction, piiFormulaCondition, piiFormulaInput, piiFormulaPattern, piiFormulaReplacement, t])

  // testNoise evaluates only the current user prompt through active deterministic rules.
  // testNoise 仅通过当前生效确定性规则评估当前用户提示词。
  const testNoise = React.useCallback(async () => {
    const result = await onRunAction("native_noise_test", { text: noiseInput })
    if (result?.data.kind !== "native_noise_test") {
      setError(t("memory.resources.invalidResponse"))
      return
    }
    setNoiseResult(result.data.value)
    setError(null)
  }, [noiseInput, onRunAction, t])

  const selectedSummary = visibleFiles.find((entry) => entry.path === selectedPath)
  const resourcePurpose = selectedSummary
    ? resourcePurposeText(selectedSummary.group, selectedSummary.fileName, t)
    : ""
  const changed = Boolean(file && editorText !== file.effectiveText)
  // group is the resource family whose panel is currently shown.
  // group 是当前显示面板的资源分组。
  const group = activeGroup

  return (
    <div className="vk-form-loose">
      {/* Keep short reads non-interactive without toggling every control's disabled opacity; the tabs share this guard
          so a short read cannot switch groups mid-load. */}
      {/* 短暂读取期间禁止交互，但不切换所有控件的禁用透明度；标签共用此保护，读取期间不能切换分组。 */}
      <fieldset className="vk-form-loose" disabled={busy} inert={filePending} aria-busy={filePending}>
        <PageTabs
          actions={<Button onClick={() => void loadCatalog().then(() => setFileReadRevision((revision) => revision + 1))} size="sm" type="button" variant="outline"><RefreshCw className="vk-icon" aria-hidden="true" />{t("memory.resources.refresh")}</Button>}
          ariaLabel={t("memory.resources.title")}
          onValueChange={setActiveGroup}
          options={([["prompt", "memory.resources.promptTab"], ["pii", "memory.resources.piiTab"], ["noise", "memory.resources.noiseTab"]] as const).map(([value, label]) => ({ label: t(label), value }))}
          value={activeGroup}
        />
      <div className="vk-form-well vk-form-xs"><span>{t("memory.resources.overrideDirectory")}: </span><span className="vk-mono vk-form-break">{catalog?.overrideRoot ?? <InlineStreamPlaceholder label={t("memory.resources.loading")} />}</span></div>
      {error ? <p className="vk-field-error">{error}</p> : null}
          {/* The key remounts the group's panel on switch, matching the previous tab panels' lifecycle.
              key 在切换时重新挂载分组面板，与原标签面板的生命周期一致。 */}
          <div className="vk-form-loose" key={group}>
            {catalog && settings ? <ResourceCompositionSettings busy={busy} catalog={catalog} group={group} onChange={setSettings} onSave={saveSettings} settings={settings} /> : null}
            <div className="vmm-resource-workspace">
            <ResourceFileSelect effectiveSelector={effectiveSelector} files={visibleFiles} onChange={setSelectedPath} value={selectedPath} />
            <Card className="vmm-resource-editor">
              <CardHeader><CardTitle className="vk-form-inline"><FileCode2 className="vk-icon" aria-hidden="true" />{t("memory.resources.fileEditor")}</CardTitle><CardDescription>{t("memory.resources.fileEditorDescription")}</CardDescription></CardHeader>
              <CardContent className="vk-form-loose">
                {selectedSummary ? <div className="vk-form-inline"><Badge variant="outline">{selectedSummary.selector}</Badge><Badge variant={selectedSummary.hasUserOverride ? "default" : "secondary"}>{selectedSummary.hasUserOverride ? t("memory.resources.userOverride") : t("memory.resources.builtIn")}</Badge><Badge variant="outline"><LockKeyhole className="vk-icon" aria-hidden="true" />{editUnlocked ? t("memory.resources.unlocked") : t("memory.resources.locked")}</Badge></div> : null}
                {file ? <><div className="vk-mem-warnbox"><div className="vk-form-inline vk-form-em"><ShieldAlert className="vk-icon" aria-hidden="true" />{file.highRisk ? t("memory.resources.highRisk") : t("memory.resources.guardedEdit")}</div><p className="vk-faint vk-form-after">{resourcePurpose}</p>{group === "prompt" ? <p className="vk-faint">{t("memory.resources.promptHowTo")}</p> : null}</div><div className="vk-form-stack"><Label>{t("memory.resources.currentEffective")}</Label><ResourceCodeEditor disabled={busy || !editUnlocked} language={resourceEditorLanguage(file.path)} onChange={setEditorText} value={editorText} /></div><div className="vk-form-chips">{!editUnlocked ? <Button disabled={busy} onClick={() => setConfirmation("unlock")} type="button" variant="outline"><UnlockKeyhole className="vk-icon" aria-hidden="true" />{t("memory.resources.unlockEditor")}</Button> : null}<Button disabled={busy || !editUnlocked || !changed || !editorText} onClick={() => setConfirmation("save")} type="button"><Save className="vk-icon" aria-hidden="true" />{t("memory.resources.saveFile")}</Button><Button disabled={busy || !file.hasUserOverride} onClick={() => setConfirmation("reset")} type="button" variant="outline"><RotateCcw className="vk-icon" aria-hidden="true" />{t("memory.resources.restoreBuiltIn")}</Button></div></> : <StreamPlaceholder label={t("memory.resources.loading")} shape="fields" count={3} />}
              </CardContent>
            </Card>
            </div>
            {group === "pii" ? <PiiTesters busy={busy} input={piiInput} formulaCondition={piiFormulaCondition} formulaInput={piiFormulaInput} formulaPattern={piiFormulaPattern} formulaReplacement={piiFormulaReplacement} formulaResult={piiFormulaResult} onFormulaConditionChange={setPiiFormulaCondition} onFormulaInputChange={setPiiFormulaInput} onFormulaPatternChange={setPiiFormulaPattern} onFormulaReplacementChange={setPiiFormulaReplacement} onInputChange={setPiiInput} onTest={testPii} onTestFormula={testPiiFormula} result={piiResult} /> : null}
            {group === "noise" ? <NoiseTester busy={busy} input={noiseInput} onInputChange={setNoiseInput} onTest={testNoise} result={noiseResult} /> : null}
          </div>
      </fieldset>

      <AlertDialog open={confirmation !== null} onOpenChange={(open) => !open && !busy && setConfirmation(null)}><AlertDialogContent onEscapeKeyDown={(event) => { if (busy) event.preventDefault() }}><AlertDialogHeader><AlertDialogTitle>{confirmationTitle(confirmation, t)}</AlertDialogTitle><AlertDialogDescription>{confirmationDescription(confirmation, resourcePurpose, t)}</AlertDialogDescription></AlertDialogHeader>{error ? <p role="alert" className="vk-field-error">{error}</p> : null}<AlertDialogFooter><AlertDialogCancel disabled={busy}>{t("memory.common.cancel")}</AlertDialogCancel><AlertDialogAction disabled={busy} onClick={(event) => {
        event.preventDefault()
        if (confirmation === "unlock") { setEditUnlocked(true); setConfirmation(null) }
        if (confirmation === "save") void saveFile().then((completed) => { if (completed) setConfirmation(null) })
        if (confirmation === "reset") void resetFile().then((completed) => { if (completed) setConfirmation(null) })
      }}>{confirmationAction(confirmation, t)}</AlertDialogAction></AlertDialogFooter></AlertDialogContent></AlertDialog>
    </div>
  )
}

// ResourceCompositionSettings groups the active category's catalog, draft controls and save callback in a compact card; busy disables saving.
// ResourceCompositionSettings 将当前分类的目录、草稿控件与保存回调组织为紧凑卡片；busy 禁用保存。
function ResourceCompositionSettings({ busy, catalog, group, onChange, onSave, settings }: { busy: boolean; catalog: VmmNativeResourceCatalogSummary; group: ResourceGroup; onChange: React.Dispatch<React.SetStateAction<VmmNativeResourceSettingsSummary | null>>; onSave: () => Promise<void>; settings: VmmNativeResourceSettingsSummary }) {
  // t resolves the current locale's resource-setting labels.
  // t 解析当前语言的资源设置文案。
  const { t } = useI18n()
  return (
    <Card>
      <CardContent className="vk-form-loose">
        <div className="vmm-resource-composition-toolbar">
          <div className="vmm-resource-composition-heading vk-form-tight">
            <CardTitle>{t("memory.resources.composition")}</CardTitle>
            <CardDescription>{t("memory.resources.compositionDescription")}</CardDescription>
          </div>
          <div className="vmm-resource-composition-actions">
            {group === "prompt" ? <ResourceSelect label={t("memory.resources.promptBundle")} onChange={(promptBundle) => onChange((current) => current ? { ...current, promptBundle } : current)} options={catalog.promptBundles} value={settings.promptBundle} /> : null}
            {group === "pii" ? <ResourceSelect label={t("memory.resources.piiLocale")} onChange={(piiLanguage) => onChange((current) => current ? { ...current, piiLanguage } : current)} options={catalog.piiLanguages} value={settings.piiLanguage} /> : null}
            {group === "noise" ? <ResourceSelect label={t("memory.resources.noiseLocale")} onChange={(noiseLanguage) => onChange((current) => current ? { ...current, noiseLanguage } : current)} options={catalog.noiseLanguages} value={settings.noiseLanguage} /> : null}
            <Button disabled={busy} onClick={() => void onSave()} type="button"><Save className="vk-icon" aria-hidden="true" />{t("memory.resources.saveComposition")}</Button>
          </div>
        </div>
        {group === "noise" ? (
          <div className="vmm-resource-noise-options vk-mem-ruled">
            <BooleanSetting checked={settings.noiseEnabled} label={t("memory.resources.noiseEnabled")} onChange={(noiseEnabled) => onChange((current) => current ? { ...current, noiseEnabled } : current)} />
            <BooleanSetting checked={settings.noiseSemanticEnabled} label={t("memory.resources.noiseSemanticEnabled")} onChange={(noiseSemanticEnabled) => onChange((current) => current ? { ...current, noiseSemanticEnabled } : current)} />
            <div className="vk-form-switch vk-form-framed">
              <Label htmlFor="vmm-noise-threshold">{t("memory.resources.noiseThreshold")}</Label>
              <Input className="vk-form-w-xs" id="vmm-noise-threshold" max="1" min="0.01" onChange={(event) => onChange((current) => current ? { ...current, noiseSemanticThreshold: Number(event.target.value) } : current)} step="0.01" type="number" value={settings.noiseSemanticThreshold} />
            </div>
          </div>
        ) : null}
      </CardContent>
    </Card>
  )
}

// ResourceFileSelect renders file names for only the active resource category.
// ResourceFileSelect 仅渲染当前资源分类中的文件名。
function ResourceFileSelect({ effectiveSelector, files, onChange, value }: { effectiveSelector: string | undefined; files: VmmNativeResourceCatalogSummary["files"]; onChange: (value: string) => void; value: string }) {
  const { t } = useI18n()
  // PII and Noise compose common rules with the saved locale, as defined by native resources.rs.
  // 按原生 resources.rs 契约，PII 与噪音将 common 规则与已保存区域组合。
  // groups distinguish this complete composition from alternatives without hiding either.
  // groups 区分这一完整组合与可选资源，同时保留两类文件入口。
  const groups = [
    { label: t("memory.workspace.effectiveFiles"), files: files.filter((entry) => entry.selector === effectiveSelector || (entry.group !== "prompt" && entry.selector === "common")) },
    { label: t("memory.workspace.otherFiles"), files: files.filter((entry) => entry.selector !== effectiveSelector && (entry.group === "prompt" || entry.selector !== "common")) },
  ]
  return <div className="vmm-resource-file-groups vk-form-loose">{groups.filter((group) => group.files.length > 0).map((group) => <section key={group.label} aria-label={group.label}><p className="vk-form-note vk-mem-mb2">{group.label}</p><div className="vmm-resource-files">{group.files.map((entry) => <button key={entry.path} type="button" aria-pressed={entry.path === value} onClick={() => onChange(entry.path)}><span className="vk-mem-line">{entry.path.split("/").at(-1)}</span><span className="vk-form-note">{entry.selector}</span></button>)}</div></section>)}</div>
}

// ResourceCodeEditorProps defines one syntax-rendered resource editor surface.
// ResourceCodeEditorProps 定义一个带语法渲染的资源编辑器界面。
interface ResourceCodeEditorProps {
  // Whether text mutation is locked while selection and scrolling remain available.
  // 文本修改是否锁定，同时仍允许选择与滚动。
  disabled: boolean
  // Canonical Shiki language selected from the authoritative resource path.
  // 根据权威资源路径选出的规范 Shiki 语言。
  language: "json" | "markdown"
  // Publishes the exact edited resource text.
  // 发布准确的已编辑资源文本。
  onChange: (value: string) => void
  // Current effective or edited resource text.
  // 当前生效或正在编辑的资源文本。
  value: string
}

// ResourceCodeEditor renders a guarded syntax-highlighted editor with synchronized line numbers.
// ResourceCodeEditor 渲染一个带同步行号的受保护语法高亮编辑器。
function ResourceCodeEditor({ disabled, language, onChange, value }: ResourceCodeEditorProps) {
  const { t } = useI18n()
  // previewRef targets the token content so native scrollbar gutters cannot clamp its offset.
  // previewRef 指向高亮内容，避免原生滚动条占位限制其偏移量。
  const previewRef = React.useRef<HTMLElement>(null)
  // gutterRef targets the line-number content independently of its fixed clipping surface.
  // gutterRef 指向行号内容，与固定的裁剪容器分离。
  const gutterRef = React.useRef<HTMLSpanElement>(null)
  const lines = React.useMemo(() => value.split(/\r\n|\r|\n/u), [value])
  const shikiTheme = useDiffShikiTheme()
  const highlightEnabled = lines.length <= 2_000 && value.length <= 262_144
  const highlightRevision = useHighlightedCodeBlockRevision(lines, language, highlightEnabled)
  const renderedLines = React.useMemo(
    () => lines.map((line) => {
      const spans = highlightEnabled ? highlightedDiffLineSpans(line, language) : null
      return spans ? tokenSpansToHtml(spans) : escapeHtml(line)
    }),
    [highlightEnabled, highlightRevision, language, lines, shikiTheme],
  )
  const lineNumbers = React.useMemo(
    () => Array.from({ length: Math.max(1, lines.length) }, (_, index) => index + 1).join("\n"),
    [lines.length],
  )

  // synchronizeScroll applies the textarea's exact offsets to both display layers and returns nothing.
  // synchronizeScroll 将输入框的准确偏移应用到两个显示层，无返回值。
  // Translate content instead of scrolling shorter ranges: native scrollbars reduce only the textarea's viewport.
  // 平移内容而非滚动较短的范围：原生滚动条仅缩小输入框的可视区域。
  const synchronizeScroll = React.useCallback((element: HTMLTextAreaElement) => {
    if (previewRef.current) {
      previewRef.current.style.transform = `translate(${-element.scrollLeft}px, ${-element.scrollTop}px)`
    }
    if (gutterRef.current) {
      gutterRef.current.style.transform = `translateY(${-element.scrollTop}px)`
    }
  }, [])

  return (
    <div className="vk-mem-code" data-language={language}>
      <div className="vk-mem-code-bar">
        <span className="vk-mono">{language}</span>
      </div>
      <div className="vk-mem-code-body">
        <pre aria-hidden="true" className="vk-mem-code-preview">
          <code ref={previewRef}>
            {renderedLines.map((line, index) => (
              <span
                dangerouslySetInnerHTML={{ __html: line || "&#8203;" }}
                key={`${index}:${line.length}`}
              />
            ))}
          </code>
        </pre>
        <pre aria-hidden="true" className="vk-mem-code-gutter"><span ref={gutterRef}>{lineNumbers}</span></pre>
        <textarea
          aria-label={t("memory.resources.fileEditor")}
          aria-readonly={disabled}
          className="vk-mem-code-input"
          onChange={(event) => {
            if (!disabled) onChange(event.target.value)
          }}
          onScroll={(event) => synchronizeScroll(event.currentTarget)}
          readOnly={disabled}
          spellCheck={false}
          value={value}
        />
      </div>
    </div>
  )
}

// resourceEditorLanguage maps one native resource path to its exact editor grammar.
// resourceEditorLanguage 将原生资源路径映射到准确的编辑器语法。
function resourceEditorLanguage(path: string): "json" | "markdown" {
  return path.toLowerCase().endsWith(".json") ? "json" : "markdown"
}

// PiiTesters renders active-bundle and ad-hoc formula validation inside the PII category.
// PiiTesters 在 PII 分类内渲染当前组合与临时公式验证。
function PiiTesters(props: { busy: boolean; input: string; result: VmmNativePiiTestResult | null; formulaInput: string; formulaPattern: string; formulaCondition: string; formulaReplacement: string; formulaResult: VmmNativePiiFormulaTestResult | null; onInputChange: (value: string) => void; onFormulaInputChange: (value: string) => void; onFormulaPatternChange: (value: string) => void; onFormulaConditionChange: (value: string) => void; onFormulaReplacementChange: (value: string) => void; onTest: () => Promise<void>; onTestFormula: () => Promise<void> }) {
  const { t } = useI18n()
  return <Card><CardHeader><CardTitle>{t("memory.resources.piiTester")}</CardTitle><CardDescription>{t("memory.resources.piiTesterDescription")}</CardDescription></CardHeader><CardContent className="vk-form-loose"><div className="vk-form-stack"><Label>{t("memory.resources.piiActiveBundleTest")}</Label><Textarea onChange={(event) => props.onInputChange(event.target.value)} value={props.input} /><Button disabled={props.busy || !props.input} onClick={() => void props.onTest()} type="button" variant="outline"><Play className="vk-icon" aria-hidden="true" />{t("memory.resources.runTest")}</Button>{props.result ? <div className="vk-form-well"><pre className="vk-form-prose">{props.result.outputText}</pre><p className="vk-form-note">{props.result.redactions.map((entry) => `${entry.ruleName} × ${entry.matchCount}`).join(", ") || t("memory.resources.noMatch")}</p></div> : null}</div><div className="vk-form-stack vk-mem-ruled"><div><Label>{t("memory.resources.piiFormulaTester")}</Label><p className="vk-form-note">{t("memory.resources.piiFormulaTesterDescription")}</p></div><div className="vk-form-stack"><Label>{t("memory.resources.piiSampleText")}</Label><Textarea onChange={(event) => props.onFormulaInputChange(event.target.value)} value={props.formulaInput} /></div><div className="vk-form-stack"><Label>{t("memory.resources.piiPattern")}</Label><Input className="vk-mono" onChange={(event) => props.onFormulaPatternChange(event.target.value)} value={props.formulaPattern} /></div><div className="vk-form-stack"><Label>{t("memory.resources.piiCondition")}</Label><Input className="vk-mono" onChange={(event) => props.onFormulaConditionChange(event.target.value)} value={props.formulaCondition} /></div><div className="vk-form-stack"><Label>{t("memory.resources.piiReplacement")}</Label><Input className="vk-mono" onChange={(event) => props.onFormulaReplacementChange(event.target.value)} value={props.formulaReplacement} /></div><Button disabled={props.busy || !props.formulaInput || !props.formulaPattern} onClick={() => void props.onTestFormula()} type="button" variant="outline"><Play className="vk-icon" aria-hidden="true" />{t("memory.resources.runTest")}</Button>{props.formulaResult ? <div className="vk-form-well vk-form-stack"><pre className="vk-form-prose">{props.formulaResult.outputText}</pre><p className="vk-form-note">{t("memory.resources.piiCandidates").replace("{count}", String(props.formulaResult.matches.length))}</p>{props.formulaResult.matches.map((entry, index) => <div className="vk-mem-record" key={`${entry.startByte}:${entry.endByte}:${index}`}><div className="vk-form-inline"><Badge variant={entry.conditionAccepted ? "default" : "secondary"}>{entry.conditionAccepted ? t("memory.resources.piiAccepted") : t("memory.resources.piiRejected")}</Badge><code>{entry.matchedText}</code><span className="vk-faint">{entry.startByte}..{entry.endByte}</span></div><p className="vk-form-codeline vk-faint vk-form-after">{t("memory.resources.piiCaptures")}: {entry.captures.map((capture, captureIndex) => `$${captureIndex}=${capture ?? "<none>"}`).join(" · ")}</p></div>)}</div> : null}</div></CardContent></Card>
}

// NoiseTester renders a user-prompt-only deterministic noise validation surface.
// NoiseTester 渲染仅针对用户提示词的确定性噪音验证界面。
function NoiseTester({ busy, input, onInputChange, onTest, result }: { busy: boolean; input: string; onInputChange: (value: string) => void; onTest: () => Promise<void>; result: VmmNativeNoiseTestResult | null }) {
  const { t } = useI18n()
  return <Card><CardHeader><CardTitle>{t("memory.resources.noiseTester")}</CardTitle><CardDescription>{t("memory.resources.noiseTesterDescription")}</CardDescription></CardHeader><CardContent className="vk-form-stack"><Label>{t("memory.resources.userPromptOnly")}</Label><Textarea onChange={(event) => onInputChange(event.target.value)} value={input} /><Button disabled={busy || !input} onClick={() => void onTest()} type="button" variant="outline"><Play className="vk-icon" aria-hidden="true" />{t("memory.resources.runTest")}</Button>{result ? <p className="vk-form-well">{result.isNoise ? `${t("memory.resources.noiseMatched")}: ${result.category ?? "-"}` : t("memory.resources.noMatch")}</p> : null}</CardContent></Card>
}

// ResourceSelect renders one closed composition selector.
// ResourceSelect 渲染一个封闭的组合选择器。
function ResourceSelect({ label, onChange, options, value }: { label: string; onChange: (value: string) => void; options: string[]; value: string }) {
  const inputId = React.useId()
  return <div className="vk-form-stack"><Label htmlFor={inputId}>{label}</Label><Select onValueChange={onChange} value={value}><SelectTrigger id={inputId}><SelectValue /></SelectTrigger><SelectContent>{options.map((option) => <SelectItem key={option} value={option}>{option}</SelectItem>)}</SelectContent></Select></div>
}

// BooleanSetting renders one explicit native resource-policy switch.
// BooleanSetting 渲染一个明确的原生资源策略开关。
function BooleanSetting({ checked, label, onChange }: { checked: boolean; label: string; onChange: (checked: boolean) => void }) {
  return <label className="vk-form-switch vk-form-framed"><span>{label}</span><Switch checked={checked} onCheckedChange={onChange} /></label>
}

// resourcePurposeText explains the semantic responsibility of one editable resource.
// resourcePurposeText 说明一份可编辑资源的语义职责。
function resourcePurposeText(group: ResourceGroup, fileName: string, t: ReturnType<typeof useI18n>["t"]): string {
  if (group === "pii") return t("memory.resources.piiPurpose")
  if (group === "noise") return t("memory.resources.noisePurpose")
  const purposeByFile: Record<string, string> = {
    "precheck_l1_main.md": t("memory.resources.promptPurpose.precheckL1"),
    "precheck_l2_main.md": t("memory.resources.promptPurpose.precheckL2"),
    "postaction_l1_main.md": t("memory.resources.promptPurpose.postactionL1"),
    "postaction_l2_main.md": t("memory.resources.promptPurpose.postactionL2"),
    "profile_instruction_main.md": t("memory.resources.promptPurpose.profileInstruction"),
  }
  return purposeByFile[fileName] ?? t("memory.resources.promptPurpose.unknown")
}

// confirmationTitle returns the exact title for one guarded file action.
// confirmationTitle 返回一个受保护文件动作的准确标题。
function confirmationTitle(action: ResourceConfirmation, t: ReturnType<typeof useI18n>["t"]): string {
  if (action === "save") return t("memory.resources.saveConfirmTitle")
  if (action === "reset") return t("memory.resources.resetConfirmTitle")
  return t("memory.resources.confirmPurposeTitle")
}

// confirmationDescription returns the exact risk explanation for one guarded file action.
// confirmationDescription 返回一个受保护文件动作的准确风险说明。
function confirmationDescription(action: ResourceConfirmation, purpose: string, t: ReturnType<typeof useI18n>["t"]): string {
  if (action === "save") return t("memory.resources.saveConfirmDescription")
  if (action === "reset") return t("memory.resources.resetConfirmDescription")
  return `${purpose} ${t("memory.resources.confirmModificationDescription")}`
}

// confirmationAction returns the localized affirmative action label.
// confirmationAction 返回本地化确认动作标签。
function confirmationAction(action: ResourceConfirmation, t: ReturnType<typeof useI18n>["t"]): string {
  if (action === "save") return t("memory.resources.saveConfirmAction")
  if (action === "reset") return t("memory.resources.resetConfirmAction")
  return t("memory.resources.unlockEditor")
}
