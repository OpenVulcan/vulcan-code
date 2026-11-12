import * as React from "react"
import { OpenVulcanAccountContext } from "@/components/features/account/openvulcan-account-context"
import { useI18n, type TranslationKey } from "@/i18n"
import {
  loadCloudMemorySetup,
  type CloudMemoryChoice,
  type CloudMemoryDisclosure,
  type CloudMemoryPhase,
  type CloudMemorySetup,
} from "@/service/cloud-memory-service"
import { Alert, AlertDescription } from "@/ui-kit/primitives/alert"
import { Button } from "@/ui-kit/primitives/button"
import { Checkbox } from "@/ui-kit/primitives/checkbox"
import { Label } from "@/ui-kit/primitives/label"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select"

// Provides localized labels for all phases the site can disclose.
// 为站点可以披露的所有阶段提供本地化标签。
const PHASE_LABELS: Record<CloudMemoryPhase, TranslationKey> = {
  pcl1: "memory.cloud.pcl1",
  pcl2: "memory.cloud.pcl2",
  ptl1: "memory.cloud.ptl1",
  ptl2: "memory.cloud.ptl2",
  profile_instruction: "memory.cloud.profile",
  embedding: "memory.cloud.embedding",
  rerank: "memory.cloud.rerank",
}

// Defines the phases covered by a shared inference route.
// 定义共用推理路由所覆盖的阶段。
const GENERATION_PHASES: CloudMemoryPhase[] = ["pcl1", "pcl2", "ptl1", "ptl2", "profile_instruction"]

// Resolves the explicit or shared phase labels for one displayed provider.
// 为一条展示的供应商记录解析显式或共用的阶段标签。
function disclosurePhases(disclosure: CloudMemoryDisclosure): CloudMemoryPhase[] {
  if (disclosure.capability !== "inference") return [disclosure.capability]
  return disclosure.purposes.length > 0 ? disclosure.purposes : GENERATION_PHASES
}

// Displays why an existing selection needs attention, with no automatic replacement.
// 展示已保存选择为何需要处理，且不会自动替换。
function SelectionNotice({ setup }: { setup: CloudMemorySetup }) {
  const { t } = useI18n()
  if (!setup.selected || setup.status === "available") return null
  return <Alert variant={setup.status === "selection_changed" ? "destructive" : "default"}>
    <AlertDescription>
      {t(setup.status === "selection_changed" ? "memory.cloud.selectionChanged"
        : setup.status === "signed_out" ? "memory.cloud.signIn" : "memory.cloud.unavailable")}
    </AlertDescription>
  </Alert>
}

// Shows current cloud routing in Models and Services and refreshes when returning to the window.
// 在模型与服务中显示当前云端路由，并在返回窗口时刷新。
export function VmmCloudMemoryStatus({ onConfigure, revision, onCloudSelected }: {
  onConfigure: () => void
  revision: number
  onCloudSelected: (selected: boolean) => void
}) {
  const { t } = useI18n()
  const [setup, setSetup] = React.useState<CloudMemorySetup | null>(null)
  // A login or account change invalidates previously displayed cloud availability.
  // 登录或切换账号时，已展示的云端可用性失效。
  const account = React.useContext(OpenVulcanAccountContext)?.snapshot
  React.useEffect(() => {
    let active = true
    let pending = false
    const refresh = async () => {
      if (pending) return
      pending = true
      try {
        const next = await loadCloudMemorySetup()
        if (active) {
          setSetup(next)
          onCloudSelected(next.selected !== null)
        }
      } catch {
        // The picker provides a retryable error if the local service is still starting.
        // 本机服务仍在启动时，选择器会展示可重试的错误。
      } finally {
        pending = false
      }
    }
    void refresh()
    window.addEventListener("focus", refresh)
    return () => {
      active = false
      window.removeEventListener("focus", refresh)
    }
  }, [revision, onCloudSelected, account?.connected, account?.connectionId])

  return <section className="vk-form-box vk-form-stack" aria-label={t("memory.cloud.title")}>
    <div className="vmm-cloud-status-head">
      <div className="vk-form-stack">
        <strong>{t("memory.cloud.title")}</strong>
        <span className="vk-form-note">{setup?.selectedName ?? t("memory.cloud.hint")}</span>
      </div>
      <Button type="button" size="sm" variant="outline" onClick={onConfigure}>
        {t(setup?.status === "selection_changed" ? "memory.cloud.chooseAgain" : "memory.cloud.configure")}
      </Button>
    </div>
    {setup ? <SelectionNotice setup={setup} /> : null}
  </section>
}

// Collects explicit consent without saving anything until the parent completes setup.
// 收集明确同意，父流程完成设置之前不保存。
export function VmmCloudMemoryPicker({ choice, onChoice }: {
  choice: CloudMemoryChoice | null
  onChoice: (choice: CloudMemoryChoice | null) => void
}) {
  const { locale, t } = useI18n()
  // The shared account controller opens login and refreshes the picker once it completes.
  // 共享账号控制器打开登录，并在登录完成后刷新选择器。
  const account = React.useContext(OpenVulcanAccountContext)
  // Each control is initialized from the catalog that was actually shown.
  // 每项控件均从实际展示的目录初始化。
  const [setup, setSetup] = React.useState<CloudMemorySetup | null>(null)
  const [loading, setLoading] = React.useState(true)
  const [loadError, setLoadError] = React.useState(false)
  const [revision, setRevision] = React.useState(0)
  const [offerId, setOfferId] = React.useState("")
  const [piiRegion, setPiiRegion] = React.useState("")
  const [rerank, setRerank] = React.useState(false)
  const remembered = React.useRef(choice)
  const initialChoice = React.useRef(onChoice)

  React.useEffect(() => {
    let active = true
    setLoading(true)
    setLoadError(false)
    void loadCloudMemorySetup().then((next) => {
      if (!active) return
      setSetup(next)
      const previous = remembered.current
      const resume = previous && previous.accountContext === next.accountContext && next.catalog?.models.some((offer) =>
        offer.id === previous.modelId && offer.consent_revision === previous.consentRevision)
        ? previous : null
      const saved = next.status === "available" ? next.selected : null
      const modelId = resume?.modelId ?? saved?.modelId ?? (next.selected ? "" : next.catalog?.default_id ?? "")
      const model = next.catalog?.models.find(({ id }) => id === modelId)
      setOfferId(modelId)
      setPiiRegion(resume?.piiRegion ?? saved?.piiRegion ?? "")
      setRerank(resume?.rerank ?? saved?.rerank ?? Boolean(model?.providers.some(({ capability }) => capability === "rerank")))
      if (!resume) initialChoice.current(null)
    }).catch(() => {
      if (active) setLoadError(true)
    }).finally(() => {
      if (active) setLoading(false)
    })
    return () => { active = false }
  }, [revision, account?.snapshot?.connected, account?.snapshot?.connectionId])

  const offer = setup?.catalog?.models.find(({ id }) => id === offerId)
  const rerankAvailable = offer?.providers.some(({ capability }) => capability === "rerank") === true
  const languageNames = new Intl.DisplayNames([locale], { type: "language" })
  const retry = () => {
    remembered.current = null
    onChoice(null)
    setRevision((value) => value + 1)
  }
  const changeOffer = (id: string) => {
    setOfferId(id)
    setRerank(setup?.catalog?.models.find((item) => item.id === id)?.providers.some(({ capability }) => capability === "rerank") === true)
    onChoice(null)
  }

  if (loading) return <p role="status" className="vk-form-note">{t("ui.loading")}</p>
  if (loadError || !setup?.catalog || !setup.accountContext) return <div className="vk-form-stack">
    <p role="status">{t(setup?.status === "signed_out" ? "memory.cloud.signIn"
      : setup?.status === "selection_changed" ? "memory.cloud.selectionChanged" : "memory.cloud.unavailable")}</p>
    <div className="vk-form-bar">
      {setup?.status === "signed_out" && account ? <Button type="button" size="sm" onClick={() => account.setOpen(true)}>
        {t("openvulcan.account.login")}
      </Button> : null}
      <Button type="button" variant="outline" size="sm" onClick={retry}>{t("composer.capability.retry")}</Button>
    </div>
  </div>
  const accountContext = setup.accountContext
  const catalog = setup.catalog
  return <div className="vk-form-loose">
    <SelectionNotice setup={setup} />
    {catalog.models.length === 0 ? <div className="vk-form-stack">
      <p role="status">{t("memory.cloud.noOffers")}</p>
      <Button type="button" variant="outline" size="sm" onClick={retry}>{t("composer.capability.retry")}</Button>
    </div> : <>
      <p className="vk-form-note">{t("memory.cloud.disclosure")}</p>
      <div className="vk-form-grid">
        <div className="vk-form-stack">
          <Label id="memory-cloud-model-label">{t("memory.cloud.model")}</Label>
          <Select value={offerId} onValueChange={changeOffer}>
            <SelectTrigger aria-labelledby="memory-cloud-model-label"><SelectValue placeholder={t("memory.cloud.chooseModel")} /></SelectTrigger>
            <SelectContent>{catalog.models.map((model) =>
              <SelectItem key={model.id} value={model.id}>{model.name}{model.id === catalog.default_id ? ` · ${t("memory.cloud.default")}` : ""}</SelectItem>
            )}</SelectContent>
          </Select>
        </div>
        <div className="vk-form-stack">
          <Label id="memory-cloud-region-label">{t("memory.cloud.piiRegion")}</Label>
          <Select value={piiRegion} onValueChange={(value) => { setPiiRegion(value); onChoice(null) }}>
            <SelectTrigger aria-labelledby="memory-cloud-region-label"><SelectValue placeholder={t("memory.cloud.chooseRegion")} /></SelectTrigger>
            <SelectContent>{setup.piiRegions.map((region) =>
              <SelectItem key={region} value={region}>{languageNames.of(region) ?? region}</SelectItem>
            )}</SelectContent>
          </Select>
        </div>
      </div>
      {offer ? <>
        <div className="vk-form-stack">
          {offer.providers.map((provider) => {
            const phases = disclosurePhases(provider)
            return <div className="vk-form-box vk-form-stack vmm-cloud-disclosure" key={phases.join("-")}>
              <strong>{phases.map((phase) => t(PHASE_LABELS[phase])).join(" · ")}</strong>
              <span>{provider.provider} · {provider.model} · {provider.region}</span>
              <p className="vk-form-note">{provider.data_policy}</p>
              {provider.privacy_url ? <a href={provider.privacy_url} target="_blank" rel="noreferrer">{t("memory.cloud.policyLink")}</a> : null}
            </div>
          })}
        </div>
        {rerankAvailable ? <label className="vk-form-inline">
          <Checkbox checked={rerank} onCheckedChange={(value) => { setRerank(value === true); onChoice(null) }} />
          {t("memory.cloud.enableRerank")}
        </label> : null}
        <label className="vk-form-inline">
          <Checkbox checked={choice !== null} disabled={!piiRegion} onCheckedChange={(checked) => onChoice(checked === true ? {
            modelId: offer.id, consentRevision: offer.consent_revision, accountContext, piiRegion, rerank,
          } : null)} />
          {t("memory.cloud.consent")}
        </label>
      </> : null}
    </>}
  </div>
}
