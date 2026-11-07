import * as React from "react";
import { CircleCheck, Eye, Send, ShieldCheck, Wrench } from "lucide-react";
import { Alert, AlertAction, AlertDescription, AlertTitle } from "@/ui-kit/primitives/alert";
import { Button } from "@/ui-kit/primitives/button";
import { Checkbox } from "@/ui-kit/primitives/checkbox";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/ui-kit/primitives/dialog";
import { Field, FieldLabel } from "@/ui-kit/primitives/field";
import { Input } from "@/ui-kit/primitives/input";
import { SegmentedControl, SegmentedControlItem } from "@/ui-kit/primitives/choice-controls";
import { Textarea } from "@/ui-kit/primitives/textarea";
import { OpenVulcanAccountContext } from "@/components/features/account/openvulcan-account-context";
import { signedInAccount } from "@/components/features/account/openvulcan-account-parts";
import { formatMessage, useI18n } from "@/i18n";
import { errorMessage, formatBytes } from "@/lib/display-format";
import type { SystemProviderError } from "@/components/conversation/conversation-system-error-card";
import { prepareFeedback, submitFeedback, type FeedbackDraft, type FeedbackErrorFacts, type FeedbackKind, type FeedbackPreview, type FeedbackReceipt } from "@/service/feedback-service";
import { FeedbackCenterContext, type FeedbackSeed } from "./feedback-center-context";
import SelfFixDialog from "./self-fix-dialog";
import "@/ui-kit/styles/features/feedback.css";

// FeedbackForm is what the user edits; the draft id survives edits so every retry reuses one idempotency key.
// FeedbackForm 是用户编辑的内容；草稿编号在编辑间保留，使每次重试复用同一幂等键。
interface FeedbackForm {
  kind: FeedbackKind;
  title: string;
  body: string;
  includeClientInfo: boolean;
  includeLog: boolean;
  allowContact: boolean;
  // Whether the error card's facts stay attached; only offered when the window was opened from one.
  // 错误卡事实是否保持附带；仅在从错误卡打开时提供。
  includeError: boolean;
}

// feedbackFactsOf returns the failure facts a report attaches; the service filters and bounds them before preview.
// feedbackFactsOf 返回报告附带的失败事实；服务端在预览前对其过滤并限制长度。
function feedbackFactsOf(error: SystemProviderError): FeedbackErrorFacts {
  const status = error.httpStatus === null ? Number.NaN : Number(error.httpStatus);
  return {
    category: error.category,
    subcategory: error.subcategory,
    stage: error.stage,
    httpStatus: Number.isInteger(status) ? status : null,
    providerId: error.provider,
    modelId: error.modelId,
    upstreamCode: error.upstreamCode,
    requestId: error.requestId,
    retryable: error.retryable,
    message: error.message,
  };
}

// initialForm derives the first form from the seed; logs are attached by default, contact is not. errorTitle is the
// localized title template of a report opened from an error card.
// initialForm 由初始内容得出首个表单；默认附带日志，不默认允许联系。errorTitle 是从错误卡打开时的本地化标题模板。
function initialForm(seed: FeedbackSeed, errorTitle: string): FeedbackForm {
  return {
    kind: seed.kind ?? (seed.systemError ? "bug" : "suggestion"),
    title: seed.title ?? (seed.systemError ? formatMessage(errorTitle, { category: seed.systemError.category }) : ""),
    body: seed.body ?? "",
    includeClientInfo: true,
    includeLog: true,
    allowContact: false,
    includeError: Boolean(seed.systemError),
  };
}

// draftOf builds the service draft from the seed and the form.
// draftOf 由初始内容与表单构建服务端草稿。
export function draftOf(seed: FeedbackSeed, form: FeedbackForm, draftId: string | null): FeedbackDraft {
  return {
    draftId,
    kind: form.kind,
    source: seed.source,
    title: form.title,
    body: form.body,
    allowContact: form.allowContact,
    includeClientInfo: form.includeClientInfo,
    includeLog: form.includeLog,
    error: form.includeError && seed.systemError ? feedbackFactsOf(seed.systemError) : null,
  };
}

// FeedbackDialog writes, previews and sends one report. Nothing leaves the device before the user has seen the
// exact filtered request and pressed Send; going back to edit discards the preview.
// FeedbackDialog 编写、预览并发送一份报告。用户看到精确的过滤后请求并按下发送之前，不会有任何内容离开设备；
// 返回编辑会丢弃预览。
export default function FeedbackDialog() {
  const center = React.useContext(FeedbackCenterContext);
  const account = React.useContext(OpenVulcanAccountContext);
  const { t } = useI18n();
  const seed = center?.feedbackSeed ?? null;
  const [form, setForm] = React.useState<FeedbackForm>(() => initialForm(seed ?? { source: "manual" }, t("feedback.errorTitle")));
  const [draftId, setDraftId] = React.useState<string | null>(null);
  const [preview, setPreview] = React.useState<FeedbackPreview | null>(null);
  const [receipt, setReceipt] = React.useState<FeedbackReceipt | null>(null);
  const [busy, setBusy] = React.useState(false);
  const [error, setError] = React.useState("");
  // selfFix shows the on-device fix window over this one; it is offered for bug reports, before or after sending.
  // selfFix 在本窗口之上显示本机修复窗口；它在发送前后都对问题报告提供。
  const [selfFix, setSelfFix] = React.useState(false);
  if (!center || !seed) return null;
  const signedIn = Boolean(account && signedInAccount(account));
  const ready = form.title.trim().length > 0 && form.body.trim().length > 0;

  const edit = (patch: Partial<FeedbackForm>) => setForm((current) => ({ ...current, ...patch }));
  const close = () => { if (!busy) center.closeFeedback(); };

  const showPreview = async () => {
    setBusy(true);
    setError("");
    try {
      const next = await prepareFeedback(draftOf(seed, form, draftId));
      setDraftId(next.draftId);
      setPreview(next);
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setBusy(false);
    }
  };

  const send = async () => {
    if (!preview) return;
    setBusy(true);
    setError("");
    try {
      setReceipt(await submitFeedback(preview.draftId, preview.digest));
      setPreview(null);
      void center.runtime.current?.refresh();
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setBusy(false);
    }
  };

  const selfFixButton = (
    <Button variant="ghost" className="vk-feedback-footer-start" disabled={busy || !ready} onClick={() => setSelfFix(true)}>
      <Wrench aria-hidden="true" />{t("feedback.selfFix.open")}
    </Button>
  );
  const hits = preview ? Object.entries(preview.redactionHits).filter(([, count]) => count > 0) : [];
  return (
    <Dialog open onOpenChange={(open) => { if (!open) close(); }}>
      <DialogContent size="lg" scrollMode="fixed-footer" className="vk-feedback-dialog">
        <DialogHeader>
          <DialogTitle>{t(receipt ? "feedback.sentTitle" : preview ? "feedback.previewTitle" : "feedback.title")}</DialogTitle>
          <DialogDescription>{t(receipt ? "feedback.sentDescription" : preview ? "feedback.previewDescription" : "feedback.description")}</DialogDescription>
        </DialogHeader>
        <div className="vk-feedback-body">
          {error ? <Alert variant="destructive"><AlertDescription>{error}</AlertDescription></Alert> : null}
          {receipt ? (
            <div className="vk-feedback-sent">
              <CircleCheck aria-hidden="true" />
              <p>{formatMessage(t("feedback.sentReceipt"), { id: receipt.feedbackId })}</p>
            </div>
          ) : preview ? (
            <>
              {!signedIn ? (
                <Alert variant="warning">
                  <AlertTitle>{t("feedback.signInTitle")}</AlertTitle>
                  <AlertDescription>{t("feedback.signInDescription")}</AlertDescription>
                  {account ? <AlertAction><Button size="sm" variant="outline" onClick={() => account.setOpen(true)}>{t("openvulcan.account.login")}</Button></AlertAction> : null}
                </Alert>
              ) : null}
              <div className="vk-feedback-facts">
                <span>{formatMessage(t("feedback.previewSize"), { size: formatBytes(preview.byteLength) })}</span>
                {form.includeLog ? <span>{formatMessage(t("feedback.previewLogLines"), { count: preview.logLineCount })}</span> : null}
                <span>{hits.length > 0 ? t("feedback.previewRedacted") : t("feedback.previewNothingRedacted")}</span>
                {hits.map(([category, count]) => <code key={category}>{category} ×{count}</code>)}
              </div>
              <pre className="vk-feedback-payload" aria-label={t("feedback.previewPayload")}><code>{preview.payload}</code></pre>
            </>
          ) : (
            <>
              <SegmentedControl aria-label={t("feedback.kind")} value={form.kind} onValueChange={(value) => edit({ kind: value as FeedbackKind })}>
                <SegmentedControlItem value="bug">{t("feedback.kind.bug")}</SegmentedControlItem>
                <SegmentedControlItem value="suggestion">{t("feedback.kind.suggestion")}</SegmentedControlItem>
              </SegmentedControl>
              <Field>
                <FieldLabel htmlFor="feedback-title">{t("feedback.titleLabel")}</FieldLabel>
                <Input id="feedback-title" value={form.title} disabled={busy} placeholder={t("feedback.titlePlaceholder")}
                  onChange={(event) => edit({ title: event.currentTarget.value })} />
              </Field>
              <Field>
                <FieldLabel htmlFor="feedback-body">{t("feedback.bodyLabel")}</FieldLabel>
                <Textarea id="feedback-body" density="large" value={form.body} disabled={busy}
                  placeholder={t(form.kind === "bug" ? "feedback.bodyPlaceholder.bug" : "feedback.bodyPlaceholder.suggestion")}
                  onChange={(event) => edit({ body: event.currentTarget.value })} />
              </Field>
              {seed.systemError ? (
                <label className="vk-check-label">
                  <Checkbox checked={form.includeError} disabled={busy} onCheckedChange={(checked) => edit({ includeError: checked === true })} />
                  <span>{formatMessage(t("feedback.includeError"), { category: seed.systemError.category })}</span>
                </label>
              ) : null}
              <label className="vk-check-label">
                <Checkbox checked={form.includeLog} disabled={busy} onCheckedChange={(checked) => edit({ includeLog: checked === true })} />
                <span>{t("feedback.includeLog")}</span>
              </label>
              <label className="vk-check-label">
                <Checkbox checked={form.includeClientInfo} disabled={busy} onCheckedChange={(checked) => edit({ includeClientInfo: checked === true })} />
                <span>{t("feedback.includeClientInfo")}</span>
              </label>
              <label className="vk-check-label">
                <Checkbox checked={form.allowContact} disabled={busy} onCheckedChange={(checked) => edit({ allowContact: checked === true })} />
                <span>{t("feedback.allowContact")}</span>
              </label>
              <p className="vk-feedback-privacy"><ShieldCheck aria-hidden="true" />{t("feedback.privacy")}</p>
            </>
          )}
        </div>
        <DialogFooter>
          {receipt ? (
            <>
              {form.kind === "bug" ? selfFixButton : null}
              <Button variant="outline" onClick={() => { center.closeFeedback(); center.openMessageCenter("feedback"); }}>{t("feedback.viewMine")}</Button>
              <Button onClick={() => center.closeFeedback()}>{t("feedback.done")}</Button>
            </>
          ) : preview ? (
            <>
              <Button variant="ghost" disabled={busy} onClick={() => { setPreview(null); setError(""); }}>{t("feedback.backToEdit")}</Button>
              <Button disabled={busy || !signedIn} onClick={() => void send()}><Send aria-hidden="true" />{t("feedback.send")}</Button>
            </>
          ) : (
            <>
              {form.kind === "bug" ? selfFixButton : null}
              <Button variant="ghost" disabled={busy} onClick={close}>{t("conversation.action.cancel")}</Button>
              <Button disabled={busy || !ready} onClick={() => void showPreview()}><Eye aria-hidden="true" />{t("feedback.preview")}</Button>
            </>
          )}
        </DialogFooter>
        {selfFix ? (
          <SelfFixDialog onClose={() => setSelfFix(false)} onStarted={() => center.closeFeedback()} problem={{
            title: form.title,
            body: form.body,
            error: draftOf(seed, form, null).error ?? null,
            feedbackId: receipt?.feedbackId ?? null,
          }} />
        ) : null}
      </DialogContent>
    </Dialog>
  );
}
