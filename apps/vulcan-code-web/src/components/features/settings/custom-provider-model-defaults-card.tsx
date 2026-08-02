import * as React from "react"
import { LoaderCircle, Settings2 } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Checkbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
import { useI18n } from "@/i18n"
import { savePublicAiCustomProviderModelDefaults } from "@/service/settings-actions-service"
import type {
  PublicAiCustomProviderModelDefaults,
  PublicAiResourceKind,
  PublicAiSettingsSnapshot,
} from "@/types"

// CustomProviderModelDefaultsCardProps binds the provider fallback editor to one snapshot update.
// CustomProviderModelDefaultsCardProps 将供应商回退编辑器绑定到一次快照更新。
interface CustomProviderModelDefaultsCardProps {
  defaults: PublicAiCustomProviderModelDefaults
  onSaved: (snapshot: PublicAiSettingsSnapshot) => void
  providerId: string
}

// CUSTOM_DEFAULT_RESOURCES is the closed non-text resource set supported by the common contract.
// CUSTOM_DEFAULT_RESOURCES 是公共契约支持的封闭非文本资源集合。
const CUSTOM_DEFAULT_RESOURCES: PublicAiResourceKind[] = ["image", "audio", "video", "file"]

// CustomProviderModelDefaultsCard edits the safe fallback used only for undeclared model fields.
// CustomProviderModelDefaultsCard 编辑仅用于未声明模型字段的安全回退值。
export function CustomProviderModelDefaultsCard(props: CustomProviderModelDefaultsCardProps) {
  const { t } = useI18n()
  const [contextLimit, setContextLimit] = React.useState(props.defaults.context_limit)
  const [outputLimit, setOutputLimit] = React.useState(props.defaults.output_limit)
  const [inputResources, setInputResources] = React.useState(props.defaults.input_resources ?? [])
  const [outputResources, setOutputResources] = React.useState(props.defaults.output_resources ?? [])
  const [editing, setEditing] = React.useState(false)
  const [saving, setSaving] = React.useState(false)
  const [error, setError] = React.useState<string | null>(null)

  React.useEffect(() => {
    if (editing) {
      return
    }
    setContextLimit(props.defaults.context_limit)
    setOutputLimit(props.defaults.output_limit)
    setInputResources(props.defaults.input_resources ?? [])
    setOutputResources(props.defaults.output_resources ?? [])
  }, [editing, props.defaults])

  // toggleEditing restores the published snapshot when the user cancels local edits.
  // toggleEditing 在用户取消本地编辑时恢复已发布快照。
  const toggleEditing = React.useCallback(() => {
    if (editing) {
      setContextLimit(props.defaults.context_limit)
      setOutputLimit(props.defaults.output_limit)
      setInputResources(props.defaults.input_resources ?? [])
      setOutputResources(props.defaults.output_resources ?? [])
      setError(null)
    }
    setEditing((current) => !current)
  }, [editing, props.defaults])

  // save validates the direct numeric contract before writing provider-wide fallback differences.
  // save 在写入供应商级回退差异前校验直接数值契约。
  const save = React.useCallback(async () => {
    if (!Number.isInteger(contextLimit) || contextLimit <= 0) {
      setError(t("settings.customModels.defaultsContextInvalid"))
      return
    }
    if (!Number.isInteger(outputLimit) || outputLimit <= 0 || outputLimit > contextLimit) {
      setError(t("settings.customModels.defaultsOutputInvalid"))
      return
    }
    setSaving(true)
    setError(null)
    try {
      const snapshot = await savePublicAiCustomProviderModelDefaults({
        providerId: props.providerId,
        defaults: {
          context_limit: contextLimit,
          output_limit: outputLimit,
          input_resources: inputResources,
          output_resources: outputResources,
        },
      })
      props.onSaved(snapshot)
      setEditing(false)
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setSaving(false)
    }
  }, [contextLimit, inputResources, outputLimit, outputResources, props, t])

  return (
    <Card>
      <CardHeader className="gap-2 p-4">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="min-w-0">
            <CardTitle className="text-sm">{t("settings.customModels.defaultsTitle")}</CardTitle>
            <CardDescription>{t("settings.customModels.defaultsDescription")}</CardDescription>
          </div>
          <Button onClick={toggleEditing} size="sm" type="button" variant="outline">
            <Settings2 className="mr-2 size-4" />
            {editing ? t("memory.common.cancel") : t("settings.customModels.defaultsEdit")}
          </Button>
        </div>
        {!editing ? (
          <div className="flex flex-wrap gap-2">
            <Badge variant="secondary">{t("settings.customModels.contextSummary").replace("{count}", contextLimit.toLocaleString())}</Badge>
            <Badge variant="secondary">{t("settings.customModels.outputSummary").replace("{count}", outputLimit.toLocaleString())}</Badge>
            <Badge variant="outline">{t("settings.customModels.textInputOutput")}</Badge>
          </div>
        ) : null}
      </CardHeader>
      {editing ? (
        <CardContent className="space-y-4 border-t p-4">
          <div className="grid gap-3 sm:grid-cols-2">
            <DirectNumberInput label={t("settings.customModels.contextLimit")} onChange={setContextLimit} value={contextLimit} />
            <DirectNumberInput label={t("settings.customModels.outputLimit")} onChange={setOutputLimit} value={outputLimit} />
          </div>
          <p className="text-xs text-muted-foreground">
            {t("settings.customModels.outputRecommendation").replace(
              "{count}",
              Math.min(32_768, Math.floor(contextLimit / 4)).toLocaleString(),
            )}
          </p>
          <div className="grid gap-3 sm:grid-cols-2">
            <DefaultResourceEditor label={t("settings.customModels.input")} onChange={setInputResources} value={inputResources} />
            <DefaultResourceEditor label={t("settings.customModels.output")} onChange={setOutputResources} value={outputResources} />
          </div>
          {error ? <p className="text-sm text-destructive">{error}</p> : null}
          <div className="flex justify-end">
            <Button disabled={saving} onClick={() => void save()} type="button">
              {saving ? <LoaderCircle className="mr-2 size-4 animate-spin" /> : null}
              {t("settings.customModels.defaultsSave")}
            </Button>
          </div>
        </CardContent>
      ) : null}
    </Card>
  )
}

// DirectNumberInput renders one locale-friendly integer field without browser steppers.
// DirectNumberInput 渲染一个不带浏览器步进器且适合本地化的整数输入框。
function DirectNumberInput(props: { label: string; onChange: (value: number) => void; value: number }) {
  return (
    <label className="grid gap-2 text-sm font-medium">
      {props.label}
      <Input
        inputMode="numeric"
        onChange={(event) => props.onChange(Number.parseInt(event.target.value.replace(/\D/g, "") || "0", 10))}
        value={props.value || ""}
      />
    </label>
  )
}

// DefaultResourceEditor edits one complete provider-default resource set.
// DefaultResourceEditor 编辑一个完整的供应商默认资源集合。
function DefaultResourceEditor(props: {
  label: string
  onChange: (resources: PublicAiResourceKind[]) => void
  value: PublicAiResourceKind[]
}) {
  const { t } = useI18n()
  return (
    <fieldset className="rounded-lg border p-3">
      <legend className="px-1 text-xs font-medium">{props.label}</legend>
      <div className="grid grid-cols-2 gap-2">
        {CUSTOM_DEFAULT_RESOURCES.map((resource) => (
          <label className="flex items-center gap-2 text-xs" key={resource}>
            <Checkbox
              checked={props.value.includes(resource)}
              onCheckedChange={(checked) => props.onChange(
                checked
                  ? [...props.value, resource]
                  : props.value.filter((candidate) => candidate !== resource),
              )}
            />
            {t(`settings.models.modality.${resource}`)}
          </label>
        ))}
      </div>
    </fieldset>
  )
}
