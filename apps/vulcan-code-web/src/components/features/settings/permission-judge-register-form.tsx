import * as React from "react";
import { FolderOpen } from "lucide-react";
import { ProjectFolderPickerDialog } from "@/components/project/folder-picker-dialog";
import { PageHint, PageNotice } from "@/components/page";
import { Badge } from "@/ui-kit/primitives/badge";
import { Button } from "@/ui-kit/primitives/button";
import { Checkbox } from "@/ui-kit/primitives/checkbox";
import { Input } from "@/ui-kit/primitives/input";
import { Spinner } from "@/ui-kit/primitives/spinner";
import { formatMessage, useI18n, type TranslationKey } from "@/i18n";
import { errorMessage, formatBytes } from "@/lib/display-format";
import { inspectPermissionJudgePackage } from "@/service/permission-judge-service";
import { browseProjectDirectories } from "@/service/project-actions-service";
import type { PermissionJudgeKnownPackage, PermissionJudgePackageInspection } from "@/types";
import "@/ui-kit/styles/features/settings-forms.css"

// Labels of package check failure codes; unknown codes keep the code in a generic sentence.
// 模型包检查失败代码的标签；未知代码放入通用句子中保留。
const ISSUE_KEYS: Record<string, TranslationKey> = {
  invalid_path: "settings.permissionJudge.packageIssue.invalidPath",
  unreadable: "settings.permissionJudge.packageIssue.unreadable",
  malformed: "settings.permissionJudge.packageIssue.malformed",
  unsupported: "settings.permissionJudge.packageIssue.unsupported",
  missing_file: "settings.permissionJudge.packageIssue.missingFile",
  unsafe_entry: "settings.permissionJudge.packageIssue.unsafeEntry",
  size_mismatch: "settings.permissionJudge.packageIssue.sizeMismatch",
  digest_mismatch: "settings.permissionJudge.packageIssue.digestMismatch",
};

// PermissionJudgeRegisterRequest is what the form submits once its folder checked out.
// PermissionJudgeRegisterRequest 是文件夹检查通过后表单提交的内容。
export interface PermissionJudgeRegisterRequest {
  path: string;
  name: string;
  custom: boolean;
}

// Checked pairs one inspection with the exact folder it describes.
// Checked 将一次检查结果与其描述的准确文件夹配对。
interface Checked {
  path: string;
  inspection: PermissionJudgePackageInspection;
}

/** Registration form for a local model folder, shared by the settings page and the setup guide.
 * 本机模型文件夹登记表单，由设置页与初始化引导共用。
 * The folder is picked from the service host or typed, then fully checked before it can be added;
 * the trust checkbox appears only for a package that is not official.
 * 文件夹可从服务宿主选择或手动输入，完整检查通过后才能添加；只有非官方模型包才显示信任勾选。
 * `onRegister` runs the host's registration and calls `reset` once it succeeded. */
export function PermissionJudgeRegisterForm(props: {
  busy: string | null;
  knownPackages: PermissionJudgeKnownPackage[];
  onRegister: (request: PermissionJudgeRegisterRequest, reset: () => void) => Promise<void>;
}) {
  const { t } = useI18n();
  const [path, setPath] = React.useState("");
  const [name, setName] = React.useState("");
  const [custom, setCustom] = React.useState(false);
  const [pickerOpen, setPickerOpen] = React.useState(false);
  // checked is the latest finished check; checking names the folder whose check is running.
  // checked 是最近完成的检查；checking 指明正在检查的文件夹。
  const [checked, setChecked] = React.useState<Checked | null>(null);
  const [checking, setChecking] = React.useState<string | null>(null);
  const [checkError, setCheckError] = React.useState<string | null>(null);
  // sequence drops results of checks superseded by a newer one or by unmounting.
  // sequence 丢弃被更新检查或卸载取代的检查结果。
  const sequence = React.useRef(0);
  React.useEffect(() => () => { sequence.current += 1; }, []);

  const trimmed = path.trim();
  // Only a result for the folder now in the field counts; editing the field invalidates it.
  // 只有针对输入框中当前文件夹的结果才有效；修改输入即使其失效。
  const result = checked?.path === trimmed ? checked.inspection : null;
  const facts = result?.package ?? null;
  const official = facts?.knownPackageId
    ? props.knownPackages.find((known) => known.id === facts.knownPackageId)?.displayName ?? facts.knownPackageId
    : null;
  const canRegister = facts !== null && !facts.registered && (official !== null || custom)
    && checking === null && props.busy === null;

  // inspect checks one folder without registering it; the trust choice belongs to one folder.
  // inspect 检查一个文件夹但不登记；信任选择只属于单个文件夹。
  const inspect = async (target: string) => {
    const folder = target.trim();
    const current = ++sequence.current;
    setCheckError(null);
    setCustom(false);
    if (folder === "") {
      setChecked(null);
      setChecking(null);
      return;
    }
    setChecking(folder);
    try {
      const inspection = await inspectPermissionJudgePackage(folder);
      if (sequence.current === current) setChecked({ path: folder, inspection });
    } catch (cause) {
      if (sequence.current === current) setCheckError(errorMessage(cause));
    } finally {
      if (sequence.current === current) setChecking(null);
    }
  };
  // inspectTyped checks a typed folder once, unless that exact folder is checked or being checked.
  // inspectTyped 对手动输入的文件夹检查一次，除非该准确文件夹已检查或正在检查。
  const inspectTyped = () => {
    if (trimmed !== "" && checked?.path !== trimmed && checking !== trimmed) void inspect(trimmed);
  };
  const reset = () => {
    sequence.current += 1;
    setPath("");
    setName("");
    setCustom(false);
    setChecked(null);
    setChecking(null);
    setCheckError(null);
  };

  return <div className="vk-form-stack" data-judge-register="">
    <div className="vk-form-inline">
      <Input className="vk-form-fill" aria-label={t("settings.permissionJudge.registerPath")}
        placeholder={t("settings.permissionJudge.registerPath")} value={path}
        onBlur={inspectTyped}
        onChange={(event) => setPath(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            event.preventDefault();
            inspectTyped();
          }
        }} />
      <Button type="button" size="sm" variant="outline" data-judge-register-browse=""
        disabled={props.busy !== null} onClick={() => setPickerOpen(true)}>
        <FolderOpen size={14} />{t("settings.permissionJudge.registerBrowse")}
      </Button>
    </div>
    {checking !== null ? <PageHint className="vk-form-inline" data-judge-package-state="checking">
      <Spinner className="vk-icon" />{t("settings.permissionJudge.packageChecking")}
    </PageHint> : checkError !== null ? <PageNotice tone="error" data-judge-package-state="failed"
      details={checkError} detailsLabel={t("settings.errors.showErrorDetails")}>
      {t("settings.permissionJudge.packageCheckFailed")}
    </PageNotice> : result?.issue ? <PageNotice tone="error" data-judge-package-state="issue" data-judge-package-issue={result.issue.code}
      details={result.issue.detail} detailsLabel={t("settings.errors.showErrorDetails")}>
      {Object.hasOwn(ISSUE_KEYS, result.issue.code)
        ? t(ISSUE_KEYS[result.issue.code])
        : formatMessage(t("settings.permissionJudge.packageIssue.unknown"), { code: result.issue.code })}
    </PageNotice> : facts ? <PageNotice
      tone={facts.registered ? "info" : official ? "success" : "warning"}
      data-judge-package-state={facts.registered ? "registered" : official ? "official" : "custom"}
      title={<span className="inline-flex flex-wrap items-center gap-2">
        {t("settings.permissionJudge.packageValid")}
        <Badge variant="outline">{official
          ? formatMessage(t("settings.permissionJudge.modelOfficial"), { name: official })
          : t("settings.permissionJudge.modelCustom")}</Badge>
      </span>}>
      <p>{formatMessage(t("settings.permissionJudge.modelMeta"), {
        policy: facts.policyVersion, precision: facts.precision, size: formatBytes(facts.totalBytes),
      })}</p>
      {facts.registered
        ? <p>{t("settings.permissionJudge.packageRegistered")}</p>
        : official === null ? <p>{t("settings.permissionJudge.packageCustomHint")}</p> : null}
    </PageNotice> : null}
    <Input aria-label={t("settings.permissionJudge.registerName")}
      placeholder={facts ? `${t("settings.permissionJudge.registerName")} · ${facts.defaultName}` : t("settings.permissionJudge.registerName")}
      value={name} onChange={(event) => setName(event.target.value)} />
    {facts && official === null && !facts.registered ? <label className="vk-check-label">
      <Checkbox checked={custom} data-judge-register-custom="" onCheckedChange={(next) => setCustom(next === true)} />
      {t("settings.permissionJudge.registerCustom")}
    </label> : null}
    <div className="settings-card-actions">
      <Button type="button" size="sm" disabled={!canRegister}
        onClick={() => void props.onRegister({ path: trimmed, name: name.trim(), custom: official === null && custom }, reset)}>
        {t("settings.permissionJudge.registerSubmit")}
      </Button>
    </div>
    <ProjectFolderPickerDialog
      confirmLabel={t("settings.permissionJudge.registerPickConfirm")}
      initialPath={result && result.issue?.code !== "invalid_path" ? trimmed : null}
      open={pickerOpen}
      title={t("settings.permissionJudge.registerPickTitle")}
      onBrowse={browseProjectDirectories}
      onConfirm={async (picked) => {
        setPath(picked);
        void inspect(picked);
      }}
      onOpenChange={setPickerOpen}
    />
  </div>;
}
