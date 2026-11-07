import * as React from "react";
import { FolderOpen, Wrench } from "lucide-react";
import { Alert, AlertDescription } from "@/ui-kit/primitives/alert";
import { Button } from "@/ui-kit/primitives/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/ui-kit/primitives/dialog";
import { Field, FieldDescription, FieldLabel } from "@/ui-kit/primitives/field";
import { Input } from "@/ui-kit/primitives/input";
import { ProjectFolderPickerDialog } from "@/components/project/folder-picker-dialog";
import { requestNewConversationWithText } from "@/components/composer/composer-external-commands";
import { formatMessage, useI18n } from "@/i18n";
import { errorMessage } from "@/lib/display-format";
import { browseProjectDirectories, prepareSelfFixWorkspace, type SelfFixWorkspace } from "@/service/project-actions-service";
import type { FeedbackErrorFacts } from "@/service/feedback-service";

// SELF_FIX_SKILL is the bundled on-demand skill the fix conversation switches on.
// SELF_FIX_SKILL 是修复会话开启的随包按需技能。
const SELF_FIX_SKILL = "vulcan-self-fix";

// LAST_PARENT_KEY remembers, in this browser only, where the last source directory was placed, so the picker opens there.
// LAST_PARENT_KEY 仅在本浏览器中记住上次源码目录所在位置，使选择器在那里打开。
const LAST_PARENT_KEY = "vulcan.selfFix.lastParent";

// SelfFixProblem is what the fix conversation starts from.
// SelfFixProblem 是修复会话的起点。
export interface SelfFixProblem {
  title: string;
  body: string;
  error: FeedbackErrorFacts | null;
  // Present once the report was sent, so the pull request can be linked to it.
  // 报告发送后存在，使拉取请求可以关联到它。
  feedbackId: string | null;
}

// readLastParent returns the remembered parent directory, or null when storage is unavailable.
// readLastParent 返回记住的上级目录；存储不可用时返回 null。
function readLastParent(): string | null {
  try {
    return window.localStorage.getItem(LAST_PARENT_KEY);
  } catch {
    return null;
  }
}

// errorSummary lists the structured failure facts without the raw message, which may carry private text; the
// conversation reads the local log for the rest.
// errorSummary 列出结构化失败事实而不含原始消息（其中可能有隐私文本）；其余内容由会话读取本地日志获得。
function errorSummary(error: FeedbackErrorFacts): string {
  const facts: Array<[string, string | number | null | undefined]> = [
    ["category", error.category], ["subcategory", error.subcategory], ["stage", error.stage],
    ["http", error.httpStatus], ["provider", error.providerId], ["model", error.modelId],
    ["code", error.upstreamCode], ["request", error.requestId],
  ];
  return facts.filter(([, value]) => value !== null && value !== undefined && value !== "").map(([key, value]) => `${key}=${value}`).join(", ");
}

// SelfFixDialog explains the on-device fix, asks where the source goes, prepares that directory and opens a new
// conversation draft there with the self-fix skill on. Nothing runs until the user sends that draft, and every command
// of the conversation then goes through the user's permission mode.
// SelfFixDialog 说明本机修复，询问源码放在哪里，准备该目录并在其中打开开启了自修复技能的新会话草稿。用户发送该草稿
// 之前不运行任何操作，之后会话的每条命令都经过用户的权限模式。
export default function SelfFixDialog(props: { problem: SelfFixProblem; onClose: () => void; onStarted: () => void }) {
  const { t } = useI18n();
  const [parent, setParent] = React.useState<string | null>(null);
  const [folderName, setFolderName] = React.useState("vulcan-code");
  const [picking, setPicking] = React.useState(false);
  const [busy, setBusy] = React.useState(false);
  const [error, setError] = React.useState("");
  const ready = Boolean(parent) && folderName.trim().length > 0 && !busy;

  // promptOf writes the draft the user reviews and sends; it names the problem, the version and the directory.
  // promptOf 编写由用户检查并发送的草稿；其中写明问题、版本与目录。
  const promptOf = (workspace: SelfFixWorkspace) => [
    formatMessage(t("feedback.selfFix.prompt"), {
      title: props.problem.title.trim(),
      body: props.problem.body.trim(),
      version: workspace.installedVersion,
      path: workspace.path,
    }),
    props.problem.error ? formatMessage(t("feedback.selfFix.promptError"), { summary: errorSummary(props.problem.error) }) : null,
    props.problem.feedbackId ? formatMessage(t("feedback.selfFix.promptFeedback"), { id: props.problem.feedbackId }) : null,
    workspace.existingCheckout ? t("feedback.selfFix.promptCheckout") : null,
  ].filter(Boolean).join("\n\n");

  const start = async () => {
    if (!parent) return;
    setBusy(true);
    setError("");
    try {
      const workspace = await prepareSelfFixWorkspace(parent, folderName.trim());
      try {
        window.localStorage.setItem(LAST_PARENT_KEY, parent);
      } catch {
        // Remembering the location is a convenience only.
        // 记住位置只是一项便利。
      }
      requestNewConversationWithText(promptOf(workspace), {
        projectId: workspace.projectId,
        promptSkillOverrides: { [SELF_FIX_SKILL]: "enabled" },
      });
      props.onStarted();
    } catch (cause) {
      setError(errorMessage(cause));
      setBusy(false);
    }
  };

  return (
    <Dialog open onOpenChange={(open) => { if (!open && !busy) props.onClose(); }}>
      <DialogContent size="md" className="vk-feedback-dialog">
        <DialogHeader>
          <DialogTitle>{t("feedback.selfFix.title")}</DialogTitle>
          <DialogDescription>{t("feedback.selfFix.description")}</DialogDescription>
        </DialogHeader>
        <div className="vk-feedback-body">
          {error ? <Alert variant="destructive"><AlertDescription>{error}</AlertDescription></Alert> : null}
          <ol className="vk-self-fix-steps">
            <li>{t("feedback.selfFix.stepSource")}</li>
            <li>{t("feedback.selfFix.stepBuild")}</li>
            <li>{t("feedback.selfFix.stepFix")}</li>
            <li>{t("feedback.selfFix.stepPullRequest")}</li>
          </ol>
          <p className="vk-feedback-privacy"><Wrench aria-hidden="true" />{t("feedback.selfFix.requirements")}</p>
          <Field>
            <FieldLabel>{t("feedback.selfFix.parentLabel")}</FieldLabel>
            <div className="vk-self-fix-parent">
              <code>{parent ?? t("feedback.selfFix.parentNone")}</code>
              <Button size="sm" variant="outline" disabled={busy} onClick={() => setPicking(true)}>
                <FolderOpen aria-hidden="true" />{t("feedback.selfFix.choose")}
              </Button>
            </div>
          </Field>
          <Field>
            <FieldLabel htmlFor="self-fix-folder">{t("feedback.selfFix.folderLabel")}</FieldLabel>
            <Input id="self-fix-folder" value={folderName} disabled={busy} onChange={(event) => setFolderName(event.currentTarget.value)} />
            <FieldDescription>{t("feedback.selfFix.folderHint")}</FieldDescription>
          </Field>
        </div>
        <DialogFooter>
          <Button variant="ghost" disabled={busy} onClick={props.onClose}>{t("conversation.action.cancel")}</Button>
          <Button disabled={!ready} onClick={() => void start()}><Wrench aria-hidden="true" />{t("feedback.selfFix.start")}</Button>
        </DialogFooter>
        {picking ? (
          <ProjectFolderPickerDialog open title={t("feedback.selfFix.pickTitle")} confirmLabel={t("feedback.selfFix.pickConfirm")}
            initialPath={parent ?? readLastParent()} onBrowse={browseProjectDirectories}
            onOpenChange={setPicking} onConfirm={async (path) => { setParent(path); setPicking(false); }} />
        ) : null}
      </DialogContent>
    </Dialog>
  );
}
