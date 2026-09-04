import * as React from "react"
import { FileCode2, Play, RefreshCw, RotateCcw, Save, ShieldAlert } from "lucide-react"

import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"
import { Textarea } from "@/components/ui/textarea"
import { useI18n } from "@/i18n"
import type {
  VmmMemoryActionResult,
  VmmNativeNoiseTestResult,
  VmmNativePiiFormulaTestResult,
  VmmNativePiiTestResult,
  VmmNativeResourceCatalogSummary,
  VmmNativeResourceFileContent,
  VmmNativeResourceSettingsSummary,
} from "@/types"

// VmmNativeResourcesPageProps binds the native-only resource editor to the shared action channel.
// VmmNativeResourcesPageProps 将仅原生资源编辑器绑定到共享动作通道。
interface VmmNativeResourcesPageProps {
  busy: boolean
  onRunAction: (
    action: string,
    payload?: Record<string, unknown>,
  ) => Promise<VmmMemoryActionResult | null>
}

// PromptConfirmationStage identifies the two explicit high-risk acknowledgements.
// PromptConfirmationStage 标识两次明确的高风险确认。
type PromptConfirmationStage = "purpose" | "modification" | null

// VmmNativeResourcesPage manages exact embedded files and user-owned overrides.
// VmmNativeResourcesPage 管理准确内置文件与用户自有覆盖。
export function VmmNativeResourcesPage({ busy, onRunAction }: VmmNativeResourcesPageProps) {
  const { t } = useI18n()
  const [catalog, setCatalog] = React.useState<VmmNativeResourceCatalogSummary | null>(null)
  const [settings, setSettings] = React.useState<VmmNativeResourceSettingsSummary | null>(null)
  const [selectedPath, setSelectedPath] = React.useState("")
  const [file, setFile] = React.useState<VmmNativeResourceFileContent | null>(null)
  const [editorText, setEditorText] = React.useState("")
  const [editUnlocked, setEditUnlocked] = React.useState(false)
  const [confirmationStage, setConfirmationStage] = React.useState<PromptConfirmationStage>(null)
  const [piiInput, setPiiInput] = React.useState("")
  const [piiResult, setPiiResult] = React.useState<VmmNativePiiTestResult | null>(null)
  // piiFormulaInput is the local sample for the non-persisting ad-hoc rule tester.
  // piiFormulaInput 是不持久化临时规则测试器使用的本地样本。
  const [piiFormulaInput, setPiiFormulaInput] = React.useState("")
  // piiFormulaPattern preserves the exact regex text submitted to the Core compiler.
  // piiFormulaPattern 保留提交给 Core 编译器的准确正则文本。
  const [piiFormulaPattern, setPiiFormulaPattern] = React.useState(String.raw`\b(\d+)\b`)
  // piiFormulaCondition preserves the exact optional bounded DSL expression.
  // piiFormulaCondition 保留准确的可选有界 DSL 表达式。
  const [piiFormulaCondition, setPiiFormulaCondition] = React.useState("len($1) == 4")
  // piiFormulaReplacement preserves the exact capture-aware replacement text.
  // piiFormulaReplacement 保留准确的捕获感知替换文本。
  const [piiFormulaReplacement, setPiiFormulaReplacement] = React.useState("[TEST_MASKED]")
  // piiFormulaResult contains the authoritative evaluator trace for the last local test.
  // piiFormulaResult 包含最近一次本地测试的权威求值器轨迹。
  const [piiFormulaResult, setPiiFormulaResult] = React.useState<VmmNativePiiFormulaTestResult | null>(null)
  const [noiseRole, setNoiseRole] = React.useState("user")
  const [noiseInput, setNoiseInput] = React.useState("")
  const [noiseResult, setNoiseResult] = React.useState<VmmNativeNoiseTestResult | null>(null)
  const [error, setError] = React.useState<string | null>(null)

  // acceptCatalog publishes one authoritative catalog and preserves a still-valid selection.
  // acceptCatalog 发布一份权威目录，并保留仍然有效的选择。
  const acceptCatalog = React.useCallback((next: VmmNativeResourceCatalogSummary) => {
    setCatalog(next)
    setSettings({ ...next.settings })
    setSelectedPath((current) => (
      next.files.some((entry) => entry.path === current)
        ? current
        : next.files[0]?.path ?? ""
    ))
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

  React.useEffect(() => {
    if (!selectedPath) {
      setFile(null)
      setEditorText("")
      return
    }
    let active = true
    void onRunAction("native_resource_read", { path: selectedPath }).then((result) => {
      if (!active) return
      if (result?.data.kind !== "native_resource_file") {
        setError(t("memory.resources.invalidResponse"))
        return
      }
      setFile(result.data.value)
      setEditorText(result.data.value.effectiveText)
      setEditUnlocked(!result.data.value.highRisk)
      setError(null)
    })
    return () => {
      active = false
    }
  }, [onRunAction, selectedPath, t])

  // selectResourceFile clears the previous file authority before loading another exact path.
  // selectResourceFile 在加载另一条准确路径前清除上一文件的操作权威。
  const selectResourceFile = React.useCallback((nextPath: string) => {
    if (nextPath === selectedPath) return
    setSelectedPath(nextPath)
    setFile(null)
    setEditorText("")
    setEditUnlocked(false)
    setConfirmationStage(null)
  }, [selectedPath])

  // saveSettings validates and activates one complete common-plus-locale composition.
  // saveSettings 校验并激活一份完整的 common 加区域组合。
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

  // saveFile persists only the selected exact path after any required two-step prompt confirmation.
  // saveFile 仅在完成所需两步提示词确认后持久化选中的准确路径。
  const saveFile = React.useCallback(async () => {
    if (!file || (file.highRisk && !editUnlocked)) return
    const result = await onRunAction("native_resource_write", {
      path: file.path,
      content: editorText,
      confirmedPurpose: file.highRisk && editUnlocked,
      confirmedModification: file.highRisk && editUnlocked,
    })
    if (result?.data.kind !== "native_resource_file") {
      setError(t("memory.resources.invalidResponse"))
      return
    }
    setFile(result.data.value)
    setEditorText(result.data.value.effectiveText)
    setError(null)
    await loadCatalog()
  }, [editUnlocked, editorText, file, loadCatalog, onRunAction, t])

  // resetFile restores built-in effective content for only the selected resource.
  // resetFile 仅为选中资源恢复内置生效内容。
  const resetFile = React.useCallback(async () => {
    if (!file) return
    const result = await onRunAction("native_resource_reset", { path: file.path })
    if (result?.data.kind !== "native_resource_file") {
      setError(t("memory.resources.invalidResponse"))
      return
    }
    setFile(result.data.value)
    setEditorText(result.data.value.effectiveText)
    setEditUnlocked(!result.data.value.highRisk)
    setError(null)
    await loadCatalog()
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

  // testPiiFormula compiles one temporary regex, condition, and replacement in the Core evaluator.
  // testPiiFormula 在 Core 求值器中编译一条临时正则、条件与替换。
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

  // testNoise executes deterministic regex rules; semantic prototypes remain in live intake.
  // testNoise 执行确定性正则规则；语义原型仍保留在实时录入链路。
  const testNoise = React.useCallback(async () => {
    const result = await onRunAction("native_noise_test", { role: noiseRole, text: noiseInput })
    if (result?.data.kind !== "native_noise_test") {
      setError(t("memory.resources.invalidResponse"))
      return
    }
    setNoiseResult(result.data.value)
    setError(null)
  }, [noiseInput, noiseRole, onRunAction, t])

  const selectedSummary = catalog?.files.find((entry) => entry.path === selectedPath)
  const promptPurpose = selectedSummary ? promptPurposeText(selectedSummary.fileName, t) : ""

  return (
    <div className="space-y-4">
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2"><FileCode2 className="size-4" aria-hidden="true" />{t("memory.resources.title")}</CardTitle>
          <CardDescription>{t("memory.resources.description")}</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="flex flex-wrap items-center justify-between gap-2 rounded-lg border bg-muted/20 px-3 py-2 text-xs">
            <span className="min-w-0 break-all font-mono text-muted-foreground">{catalog?.overrideRoot ?? t("memory.resources.loading")}</span>
            <Button disabled={busy} onClick={() => void loadCatalog()} size="sm" type="button" variant="outline"><RefreshCw className="size-3.5" aria-hidden="true" />{t("memory.resources.refresh")}</Button>
          </div>
          {error ? <p className="settings-error-text text-sm">{error}</p> : null}
          {catalog && settings ? (
            <div className="grid gap-4 md:grid-cols-3">
              <ResourceSelect label={t("memory.resources.promptBundle")} onChange={(promptBundle) => setSettings((current) => current ? { ...current, promptBundle } : current)} options={catalog.promptBundles} value={settings.promptBundle} />
              <ResourceSelect label={t("memory.resources.piiLocale")} onChange={(piiLanguage) => setSettings((current) => current ? { ...current, piiLanguage } : current)} options={catalog.piiLanguages} value={settings.piiLanguage} />
              <ResourceSelect label={t("memory.resources.noiseLocale")} onChange={(noiseLanguage) => setSettings((current) => current ? { ...current, noiseLanguage } : current)} options={catalog.noiseLanguages} value={settings.noiseLanguage} />
              <BooleanSetting checked={settings.noiseEnabled} label={t("memory.resources.noiseEnabled")} onChange={(noiseEnabled) => setSettings((current) => current ? { ...current, noiseEnabled } : current)} />
              <BooleanSetting checked={settings.noiseSemanticEnabled} label={t("memory.resources.noiseSemanticEnabled")} onChange={(noiseSemanticEnabled) => setSettings((current) => current ? { ...current, noiseSemanticEnabled } : current)} />
              <div className="space-y-2"><Label htmlFor="vmm-noise-threshold">{t("memory.resources.noiseThreshold")}</Label><Input id="vmm-noise-threshold" max="1" min="0.01" onChange={(event) => setSettings((current) => current ? { ...current, noiseSemanticThreshold: Number(event.target.value) } : current)} step="0.01" type="number" value={settings.noiseSemanticThreshold} /></div>
              <div className="md:col-span-3"><Button disabled={busy} onClick={() => void saveSettings()} type="button"><Save className="size-4" aria-hidden="true" />{t("memory.resources.saveComposition")}</Button></div>
            </div>
          ) : <p className="text-sm text-muted-foreground">{t("memory.resources.loading")}</p>}
        </CardContent>
      </Card>

      {catalog ? (
        <Card>
          <CardHeader>
            <CardTitle>{t("memory.resources.fileEditor")}</CardTitle>
            <CardDescription>{t("memory.resources.fileEditorDescription")}</CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <ResourceSelect label={t("memory.resources.file")} onChange={selectResourceFile} options={catalog.files.map((entry) => entry.path)} value={selectedPath} />
            {selectedSummary ? <div className="flex flex-wrap gap-2"><Badge variant="outline">{selectedSummary.group}</Badge><Badge variant="outline">{selectedSummary.selector}</Badge>{selectedSummary.hasUserOverride ? <Badge>{t("memory.resources.userOverride")}</Badge> : <Badge variant="secondary">{t("memory.resources.builtIn")}</Badge>}</div> : null}
            {file ? (
              <>
                {file.highRisk ? (
                  <div className="rounded-lg border border-amber-500/40 bg-amber-500/5 p-3 text-sm">
                    <div className="flex items-center gap-2 font-medium"><ShieldAlert className="size-4" aria-hidden="true" />{t("memory.resources.highRisk")}</div>
                    <p className="mt-2 text-muted-foreground">{promptPurpose}</p>
                    <p className="mt-1 text-muted-foreground">{t("memory.resources.promptHowTo")}</p>
                    {!editUnlocked ? <Button className="mt-3" disabled={busy} onClick={() => setConfirmationStage("purpose")} type="button" variant="outline">{t("memory.resources.beginPromptEdit")}</Button> : null}
                  </div>
                ) : null}
                <div className="grid gap-4 lg:grid-cols-2">
                  <div className="space-y-2"><Label>{t("memory.resources.builtInSource")}</Label><Textarea className="min-h-72 font-mono text-xs" readOnly value={file.builtInText} /></div>
                  <div className="space-y-2"><Label>{t("memory.resources.effectiveEditor")}</Label><Textarea className="min-h-72 font-mono text-xs" disabled={file.highRisk && !editUnlocked} onChange={(event) => setEditorText(event.target.value)} value={editorText} /></div>
                </div>
                <div className="flex flex-wrap gap-2">
                  <Button disabled={busy || (file.highRisk && !editUnlocked) || !editorText} onClick={() => void saveFile()} type="button"><Save className="size-4" aria-hidden="true" />{t("memory.resources.saveFile")}</Button>
                  <Button disabled={busy || !file.hasUserOverride} onClick={() => void resetFile()} type="button" variant="outline"><RotateCcw className="size-4" aria-hidden="true" />{t("memory.resources.restoreBuiltIn")}</Button>
                </div>
              </>
            ) : null}
          </CardContent>
        </Card>
      ) : null}

      <div className="grid gap-4 lg:grid-cols-2">
        <Card>
          <CardHeader><CardTitle>{t("memory.resources.piiTester")}</CardTitle><CardDescription>{t("memory.resources.piiTesterDescription")}</CardDescription></CardHeader>
          <CardContent className="space-y-5">
            <div className="space-y-3">
              <Label>{t("memory.resources.piiActiveBundleTest")}</Label>
              <Textarea onChange={(event) => setPiiInput(event.target.value)} value={piiInput} />
              <Button disabled={busy || !piiInput} onClick={() => void testPii()} type="button" variant="outline"><Play className="size-4" aria-hidden="true" />{t("memory.resources.runTest")}</Button>
              {piiResult ? <div className="rounded-lg border bg-muted/20 p-3 text-sm"><pre className="whitespace-pre-wrap break-words">{piiResult.outputText}</pre><p className="mt-2 text-xs text-muted-foreground">{piiResult.redactions.map((entry) => `${entry.ruleName} × ${entry.matchCount}`).join(", ") || t("memory.resources.noMatch")}</p></div> : null}
            </div>
            <div className="space-y-3 border-t pt-4">
              <div><Label>{t("memory.resources.piiFormulaTester")}</Label><p className="mt-1 text-xs text-muted-foreground">{t("memory.resources.piiFormulaTesterDescription")}</p></div>
              <div className="space-y-2"><Label htmlFor="vmm-pii-formula-sample">{t("memory.resources.piiSampleText")}</Label><Textarea id="vmm-pii-formula-sample" onChange={(event) => setPiiFormulaInput(event.target.value)} value={piiFormulaInput} /></div>
              <div className="space-y-2"><Label htmlFor="vmm-pii-formula-pattern">{t("memory.resources.piiPattern")}</Label><Input className="font-mono text-xs" id="vmm-pii-formula-pattern" onChange={(event) => setPiiFormulaPattern(event.target.value)} value={piiFormulaPattern} /></div>
              <div className="space-y-2"><Label htmlFor="vmm-pii-formula-condition">{t("memory.resources.piiCondition")}</Label><Input className="font-mono text-xs" id="vmm-pii-formula-condition" onChange={(event) => setPiiFormulaCondition(event.target.value)} value={piiFormulaCondition} /></div>
              <div className="space-y-2"><Label htmlFor="vmm-pii-formula-replacement">{t("memory.resources.piiReplacement")}</Label><Input className="font-mono text-xs" id="vmm-pii-formula-replacement" onChange={(event) => setPiiFormulaReplacement(event.target.value)} value={piiFormulaReplacement} /></div>
              <Button disabled={busy || !piiFormulaInput || !piiFormulaPattern} onClick={() => void testPiiFormula()} type="button" variant="outline"><Play className="size-4" aria-hidden="true" />{t("memory.resources.runTest")}</Button>
              {piiFormulaResult ? <div className="space-y-2 rounded-lg border bg-muted/20 p-3 text-sm"><pre className="whitespace-pre-wrap break-words">{piiFormulaResult.outputText}</pre><p className="text-xs text-muted-foreground">{t("memory.resources.piiCandidates").replace("{count}", String(piiFormulaResult.matches.length))}</p>{piiFormulaResult.matches.length > 0 ? <div className="space-y-2">{piiFormulaResult.matches.map((entry, index) => <div className="rounded border bg-background/70 p-2 text-xs" key={`${entry.startByte}:${entry.endByte}:${index}`}><div className="flex flex-wrap items-center gap-2"><Badge variant={entry.conditionAccepted ? "default" : "secondary"}>{entry.conditionAccepted ? t("memory.resources.piiAccepted") : t("memory.resources.piiRejected")}</Badge><code>{entry.matchedText}</code><span className="text-muted-foreground">{entry.startByte}..{entry.endByte}</span></div><p className="mt-2 break-all font-mono text-muted-foreground">{t("memory.resources.piiCaptures")}: {entry.captures.map((capture, captureIndex) => `$${captureIndex}=${capture ?? "<none>"}`).join(" · ")}</p></div>)}</div> : <p className="text-xs text-muted-foreground">{t("memory.resources.piiNoCandidates")}</p>}</div> : null}
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardHeader><CardTitle>{t("memory.resources.noiseTester")}</CardTitle><CardDescription>{t("memory.resources.noiseTesterDescription")}</CardDescription></CardHeader>
          <CardContent className="space-y-3"><ResourceSelect label={t("memory.resources.role")} onChange={setNoiseRole} options={["user", "assistant", "system", "tool"]} value={noiseRole} /><Textarea onChange={(event) => setNoiseInput(event.target.value)} value={noiseInput} /><Button disabled={busy || !noiseInput} onClick={() => void testNoise()} type="button" variant="outline"><Play className="size-4" aria-hidden="true" />{t("memory.resources.runTest")}</Button>{noiseResult ? <p className="rounded-lg border bg-muted/20 p-3 text-sm">{noiseResult.isNoise ? `${t("memory.resources.noiseMatched")}: ${noiseResult.category ?? "-"}` : t("memory.resources.noMatch")}</p> : null}</CardContent>
        </Card>
      </div>

      <AlertDialog open={confirmationStage !== null} onOpenChange={(open) => !open && setConfirmationStage(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{confirmationStage === "purpose" ? t("memory.resources.confirmPurposeTitle") : t("memory.resources.confirmModificationTitle")}</AlertDialogTitle>
            <AlertDialogDescription>{confirmationStage === "purpose" ? promptPurpose : t("memory.resources.confirmModificationDescription")}</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("memory.common.cancel")}</AlertDialogCancel>
            <AlertDialogAction onClick={(event) => {
              if (confirmationStage === "purpose") {
                event.preventDefault()
                setConfirmationStage("modification")
              } else {
                setConfirmationStage(null)
                setEditUnlocked(true)
              }
            }}>{confirmationStage === "purpose" ? t("memory.resources.confirmPurpose") : t("memory.resources.unlockEditor")}</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  )
}

// ResourceSelect renders one closed resource, locale, file, or role selector.
// ResourceSelect 渲染一个封闭的资源、区域、文件或角色选择器。
function ResourceSelect({ label, onChange, options, value }: {
  label: string
  onChange: (value: string) => void
  options: string[]
  value: string
}) {
  const inputId = React.useId()
  return <div className="space-y-2"><Label htmlFor={inputId}>{label}</Label><Select onValueChange={onChange} value={value}><SelectTrigger id={inputId}><SelectValue /></SelectTrigger><SelectContent>{options.map((option) => <SelectItem key={option} value={option}>{option}</SelectItem>)}</SelectContent></Select></div>
}

// BooleanSetting renders one explicit native resource-policy switch.
// BooleanSetting 渲染一个明确的原生资源策略开关。
function BooleanSetting({ checked, label, onChange }: {
  checked: boolean
  label: string
  onChange: (checked: boolean) => void
}) {
  return <label className="flex items-center justify-between gap-3 rounded-lg border px-3 py-2 text-sm"><span>{label}</span><Switch checked={checked} onCheckedChange={onChange} /></label>
}

// promptPurposeText explains the exact semantic stage owned by one editable prompt file.
// promptPurposeText 说明一份可编辑提示词文件所属的准确语义阶段。
function promptPurposeText(fileName: string, t: ReturnType<typeof useI18n>["t"]): string {
  const purposeByFile: Record<string, string> = {
    "precheck_l1_main.md": t("memory.resources.promptPurpose.precheckL1"),
    "precheck_l2_main.md": t("memory.resources.promptPurpose.precheckL2"),
    "postaction_l1_main.md": t("memory.resources.promptPurpose.postactionL1"),
    "postaction_l2_main.md": t("memory.resources.promptPurpose.postactionL2"),
    "profile_instruction_main.md": t("memory.resources.promptPurpose.profileInstruction"),
  }
  return purposeByFile[fileName] ?? t("memory.resources.promptPurpose.unknown")
}
