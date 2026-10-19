import * as React from "react";
import { LoaderCircle } from "lucide-react";
import { Button } from "@/ui-kit/primitives/button";
import { InlineDisclosure } from "@/ui-kit/primitives/disclosure";
import { Input } from "@/ui-kit/primitives/input";
import { useI18n, type TranslationKey } from "@/i18n";
import { loadPublicAiSettings, resetPublicAiMediaDeliveryPolicy, savePublicAiMediaDeliveryPolicy } from "@/service/settings-actions-service";
import type { PublicAiMediaDeliverySettingsSnapshot, PublicAiSettingsSnapshot } from "@/types";
import { createMediaDeliveryDraft, MEDIA_DELIVERY_KINDS, MEDIA_DELIVERY_PURPOSES, MediaDeliveryDraftError, parseMediaDeliveryDraft, type MediaDeliveryKind, type MediaDeliveryPurpose, type MediaDeliveryThresholdField } from "./media-delivery-draft";

// PURPOSE_LABELS keeps main conversations and independent AI extensions visibly separate.
// PURPOSE_LABELS 在界面上明确分离主会话与独立 AI 扩展。
const PURPOSE_LABELS: Record<MediaDeliveryPurpose, TranslationKey> = {
  conversation: "settings.mediaDelivery.conversation",
  independent_extension: "settings.mediaDelivery.extension",
};
// KIND_LABELS names all four exact backend modalities, including generic files and PDFs.
// KIND_LABELS 命名全部四个精确后端模态，包括通用文件及 PDF。
const KIND_LABELS: Record<MediaDeliveryKind, TranslationKey> = {
  image: "settings.mediaDelivery.image", audio: "settings.mediaDelivery.audio",
  video: "settings.mediaDelivery.video", file: "settings.mediaDelivery.file",
};
// THRESHOLD_LABELS binds the two availability cases without inferring actual model support in the browser.
// THRESHOLD_LABELS 绑定两个可用性分支，不在浏览器推断实际模型支持。
const THRESHOLD_LABELS: Record<MediaDeliveryThresholdField, TranslationKey> = {
  with_upload_max_bytes: "settings.mediaDelivery.withUpload",
  without_upload_max_bytes: "settings.mediaDelivery.withoutUpload",
};

// MediaDeliveryPolicyCardProps supplies the authoritative snapshot and its parent publication callback.
// MediaDeliveryPolicyCardProps 提供权威快照及其父级发布回调。
export interface MediaDeliveryPolicyCardProps {
  // Complete backend settings and ceilings, never synthesized frontend defaults.
  // 完整后端设置及上限，绝不合成前端默认值。
  settings: PublicAiMediaDeliverySettingsSnapshot;
  // Publishes the exact returned full snapshot after save, reset or explicit reload.
  // 保存、重置或显式重载后发布精确返回的完整快照。
  onSaved: (snapshot: PublicAiSettingsSnapshot) => void;
}

// MediaDeliveryPolicyCard edits sixteen independent thresholds and two budgets using backend revision checks.
// MediaDeliveryPolicyCard 使用后端修订检查，编辑十六项独立阈值及两个预算。
// Props supply current server values; the returned form never uploads media or rewrites settings on mount.
// 属性提供当前服务器值；返回表单绝不上传媒体，也不在挂载时重写设置。
export function MediaDeliveryPolicyCard({ settings, onSaved }: MediaDeliveryPolicyCardProps) {
  // Translation follows the existing WebUI locale owner.
  // 翻译遵循既有 WebUI 语言所有者。
  const { t } = useI18n();
  // Baseline owns the revision the user actually edited, not whichever snapshot arrives most recently.
  // 基线持有用户实际编辑的修订，不盲目采用最近到达的任意快照。
  const [baseline, setBaseline] = React.useState(settings);
  // Draft strings preserve empty and invalid edits until explicit validation succeeds.
  // 草稿字符串保留空值与无效编辑，直至显式校验成功。
  const [draft, setDraft] = React.useState(() => createMediaDeliveryDraft(settings.policy));
  // Pending state and the synchronous guard prevent overlapping actions from this editor.
  // 等待状态与同步守卫防止本编辑器的动作重叠。
  const [pending, setPending] = React.useState(false);
  const busy = React.useRef(false);
  // Errors remain visible without replacing the user's draft with optimistic values.
  // 错误保持可见，不使用乐观值替换用户草稿。
  const [error, setError] = React.useState<string | null>(null);
  // Observed prop revisions distinguish external updates from local renders after editing or saving.
  // 已观察属性修订用于区分外部更新与编辑、保存后的本地渲染。
  const observedRevision = React.useRef(settings.revision);
  // Lifetime prevents a completed request from publishing into an unmounted settings view.
  // 生命周期防止已完成请求向卸载的设置视图发布。
  const mounted = React.useRef(true);
  React.useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; };
  }, []);
  // Dirty compares closed field sets, preserving each independent value exactly.
  // 脏状态比较封闭字段集合，精确保留每项独立值。
  const dirty = JSON.stringify(draft) !== JSON.stringify(createMediaDeliveryDraft(baseline.policy));
  // A changed external revision blocks mutation until the user explicitly reloads it.
  // 外部修订变化会阻止修改，直至用户显式重新载入。
  const conflict = settings.revision !== baseline.revision;
  React.useEffect(() => {
    if (observedRevision.current === settings.revision) return;
    observedRevision.current = settings.revision;
    if (!dirty && !pending) {
      setBaseline(settings);
      setDraft(createMediaDeliveryDraft(settings.policy));
    }
  }, [settings, dirty, pending]);
  // Validation retains the backend version and ceilings rather than duplicating policy defaults.
  // 校验保留后端版本及上限，不复制策略默认值。
  const validation = React.useMemo(() => {
    try { return { policy: parseMediaDeliveryDraft(draft, settings), error: null }; }
    catch (reason) {
      if (!(reason instanceof MediaDeliveryDraftError)) throw reason;
      return { policy: null, error: reason };
    }
  }, [draft, settings]);
  // Validation keys are closed so programmer or transport failures cannot become silent input defaults.
  // 校验键是封闭集合，程序或传输失败不能成为静默输入默认值。
  const validationKeys: Record<MediaDeliveryDraftError["code"], TranslationKey> = {
    number: "settings.mediaDelivery.invalidNumber", limit: "settings.mediaDelivery.invalidLimit", fraction: "settings.mediaDelivery.invalidFraction",
  };

  // updateThreshold changes exactly purpose/kind/field to value and returns no network side effect.
  // updateThreshold 仅将 purpose/kind/field 改为 value，不产生网络副作用。
  function updateThreshold(purpose: MediaDeliveryPurpose, kind: MediaDeliveryKind, field: MediaDeliveryThresholdField, value: string) {
    setDraft((previous) => ({ ...previous, [purpose]: { ...previous[purpose], [kind]: { ...previous[purpose][kind], [field]: value } } }));
  }

  // applyAction sends a validated save, revision-bound reset or explicit reload and adopts only the server result.
  // applyAction 发送已校验保存、绑定修订的重置或显式重载，且仅采用服务器结果。
  // action is selected only by user controls; failures preserve the current draft and expose an error.
  // action 仅由用户控件选择；失败时保留当前草稿并展示错误。
  async function applyAction(action: "save" | "reset" | "reload") {
    if (busy.current) return;
    if (action !== "reload" && conflict) { setError(t("settings.mediaDelivery.conflict")); return; }
    if (action === "save" && validation.policy === null) return;
    busy.current = true;
    setPending(true);
    setError(null);
    try {
      // The exact baseline fingerprint provides backend compare-and-set protection across clients.
      // 精确基线指纹为后端跨客户端比较并保存提供保护。
      const next = action === "reload" ? await loadPublicAiSettings()
        : action === "reset" ? await resetPublicAiMediaDeliveryPolicy(baseline.revision)
          : await savePublicAiMediaDeliveryPolicy(parseMediaDeliveryDraft(draft, settings), baseline.revision);
      if (!mounted.current) return;
      setBaseline(next.mediaDelivery);
      setDraft(createMediaDeliveryDraft(next.mediaDelivery.policy));
      observedRevision.current = next.mediaDelivery.revision;
      onSaved(next);
    } catch (reason) {
      if (mounted.current) {
        // Only the backend's explicit conflict code receives a localized conflict explanation.
        // 仅后端的明确冲突代码接收本地化冲突说明。
        const message = reason instanceof Error ? reason.message : String(reason);
        setError(message.includes("MEDIA_DELIVERY_POLICY_CHANGED:") ? t("settings.mediaDelivery.conflict") : message);
      }
    } finally {
      busy.current = false;
      if (mounted.current) setPending(false);
    }
  }

  return (
    <section className="min-w-0 space-y-4" aria-label={t("settings.mediaDelivery.title")}>
      <div className="space-y-1">
        <h3 className="text-sm font-semibold">{t("settings.mediaDelivery.title")}</h3>
        <p className="text-xs text-muted-foreground">{t("settings.mediaDelivery.description")}</p>
      </div>
      {/* Keep detailed delivery rules available without pushing the editable thresholds below a wall of text. */}
      {/* 保留完整投递规则，避免长篇说明将可编辑阈值推到首屏之外。 */}
      <InlineDisclosure className="media-delivery-rules" keepMounted label={t("settings.mediaDelivery.howItWorks")} openLabel={t("settings.mediaDelivery.hideExplanation")}>
        <ul className="grid list-disc gap-1.5 pl-4 text-xs leading-5 text-muted-foreground">
          <li>{t("settings.mediaDelivery.uploadRule")}</li>
          <li>{t("settings.mediaDelivery.historyRule")}</li>
          <li>{t("settings.mediaDelivery.failureRule")}</li>
          <li>{t("settings.mediaDelivery.rawMaximum")}: {settings.maxInlineRawBytes / 1024} KiB；{t("settings.mediaDelivery.totalMaximum")}: {settings.maxTotalEncodedBytes / 1024} KiB</li>
        </ul>
      </InlineDisclosure>
      {MEDIA_DELIVERY_PURPOSES.map((purpose) => (
        <fieldset className="min-w-0 space-y-3 border-t pt-3" disabled={pending} key={purpose}>
          <legend className="px-1 text-sm font-medium">{t(PURPOSE_LABELS[purpose])}</legend>
          <div className="grid grid-cols-[minmax(3.5rem,0.65fr)_minmax(5rem,1fr)_minmax(5rem,1fr)] items-center gap-2 text-xs">
            <span>{t("settings.mediaDelivery.kind")}</span>
            <span>{t(THRESHOLD_LABELS.with_upload_max_bytes)} (KiB)</span>
            <span>{t(THRESHOLD_LABELS.without_upload_max_bytes)} (KiB)</span>
            {MEDIA_DELIVERY_KINDS.map((kind) => (
              <React.Fragment key={kind}>
                <span>{t(KIND_LABELS[kind])}</span>
                {(["with_upload_max_bytes", "without_upload_max_bytes"] as const).map((field) => {
                  // Field identity binds accessibility, validation and the exact backend configuration slot.
                  // 字段身份绑定无障碍、校验与精确后端配置位置。
                  const id = `${purpose}-${kind}-${field}`;
                  return <Input key={field} id={id} type="text" inputMode="decimal" className="min-w-0 text-right tabular-nums"
                    aria-label={`${t(PURPOSE_LABELS[purpose])} / ${t(KIND_LABELS[kind])} / ${t(THRESHOLD_LABELS[field])} (KiB)`}
                    aria-invalid={validation.error?.field === id}
                    value={draft[purpose][kind][field]} onChange={(event) => updateThreshold(purpose, kind, field, event.currentTarget.value)} />;
                })}
              </React.Fragment>
            ))}
          </div>
          <label className="flex flex-wrap items-center justify-between gap-2 text-xs" htmlFor={`${purpose}-max_total_encoded_bytes`}>
            <span>{t("settings.mediaDelivery.aggregate")} (KiB)</span>
            <Input id={`${purpose}-max_total_encoded_bytes`} type="text" inputMode="decimal" className="w-36 text-right tabular-nums"
              aria-label={`${t(PURPOSE_LABELS[purpose])} / ${t("settings.mediaDelivery.aggregate")} (KiB)`}
              aria-invalid={validation.error?.field === `${purpose}-max_total_encoded_bytes`}
              value={draft[purpose].max_total_encoded_bytes}
              onChange={(event) => {
                // Capture the event value synchronously before React releases the event target.
                // 在 React 释放事件目标之前同步捕获事件值。
                const value = event.currentTarget.value;
                setDraft((previous) => ({ ...previous, [purpose]: { ...previous[purpose], max_total_encoded_bytes: value } }));
              }} />
          </label>
        </fieldset>
      ))}
      {validation.error ? <p role="alert" className="text-sm text-destructive">{t(validationKeys[validation.error.code])}</p> : null}
      {conflict || error ? <p role="alert" className="text-sm text-destructive">{error ?? t("settings.mediaDelivery.conflict")}</p> : null}
      <div className="flex flex-wrap gap-2">
        <Button type="button" disabled={pending || conflict || !dirty || validation.policy === null} onClick={() => void applyAction("save")}>
          {pending ? <LoaderCircle className="mr-2 size-4 animate-spin" aria-hidden="true" /> : null}{t("settings.mediaDelivery.save")}
        </Button>
        <Button type="button" variant="outline" disabled={pending || conflict} onClick={() => void applyAction("reset")}>{t("settings.mediaDelivery.reset")}</Button>
        <Button type="button" variant="ghost" disabled={pending} onClick={() => void applyAction("reload")}>{t("settings.mediaDelivery.reload")}</Button>
      </div>
    </section>
  );
}
