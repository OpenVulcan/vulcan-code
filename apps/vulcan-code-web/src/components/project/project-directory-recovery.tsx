import React from "react";
import { toast } from "@/ui-kit/primitives/toast";
import { useI18n } from "@/i18n";
import { browseProjectReplacement, changeProjectDirectory, checkProjectDirectory } from "@/service/project-actions-service";
import { hasServiceRuntime } from "@/service/webui-service-runtime";
import type { ProjectSummary, WorkbenchSnapshot } from "@/types";
import { Button } from "@/ui-kit/primitives/button";
import "./project-directory-recovery.css";

// Load the shared picker only when the user opens directory recovery.
// 仅在用户打开目录恢复时加载共用选择器。
const FolderPicker = React.lazy(() => import("./folder-picker-dialog")
  .then((module) => ({ default: module.ProjectFolderPickerDialog })));

// ProjectDirectoryRecoveryProps binds the check and replacement to one exact project.
// ProjectDirectoryRecoveryProps 将检查与更换绑定到一个精确项目。
interface ProjectDirectoryRecoveryProps {
  // Project identity and saved path as displayed by the current workspace snapshot.
  // 当前工作区快照展示的项目标识与保存路径。
  project: ProjectSummary;
  // Changing this value requests a fresh status after execution or a submission failure.
  // 执行状态变更或提交失败时改变此值以重新检查。
  revision: string;
  // Keep the replacement action available in project home when the directory is healthy.
  // 项目目录正常时仍在项目首页显示更换操作。
  showChange?: boolean;
  // Apply the refreshed snapshot returned by a successful replacement.
  // 应用更换成功后返回的刷新快照。
  onChanged: (snapshot: WorkbenchSnapshot) => void;
}

// ProjectDirectoryRecovery displays the saved path and performs user-driven recovery.
// ProjectDirectoryRecovery 展示保存路径并执行用户发起的恢复操作。
export function ProjectDirectoryRecovery({ project, revision, showChange, onChanged }: ProjectDirectoryRecoveryProps) {
  const { t } = useI18n();
  // Track only local health and dialog state; checking health never submits a conversation.
  // 仅跟踪本地可用性与对话框状态；检查状态不会提交会话。
  const [unavailable, setUnavailable] = React.useState(false);
  const [checking, setChecking] = React.useState(false);
  // The opening directory remains fixed while the picker is showing, even if health changes.
  // 即使可用性发生变化，选择器开启期间也保持其初始目录。
  const [picker, setPicker] = React.useState<{ initialPath: string | null } | null>(null);
  const checkSequence = React.useRef(0);

  // Read live state for this path, discarding checks superseded by navigation or replacement.
  // 读取此路径的实时状态，丢弃因导航或更换而过时的检查。
  const check = React.useCallback(async (reportFailure: boolean) => {
    if (!hasServiceRuntime()) return;
    const sequence = ++checkSequence.current;
    setChecking(true);
    try {
      const status = await checkProjectDirectory(project.id);
      if (sequence === checkSequence.current && status.path === project.path) setUnavailable(!status.available);
    } catch (error) {
      if (reportFailure && sequence === checkSequence.current) {
        toast.error(error instanceof Error ? error.message : String(error));
      }
    } finally {
      if (sequence === checkSequence.current) setChecking(false);
    }
  }, [project.id, project.path]);

  React.useEffect(() => {
    void check(false);
    // Window focus detects filesystem changes made outside the app.
    // 窗口重新聚焦时检测应用外的文件系统变动。
    const onFocus = () => { void check(false); };
    window.addEventListener("focus", onFocus);
    return () => {
      checkSequence.current += 1;
      window.removeEventListener("focus", onFocus);
    };
  }, [check, revision]);

  // Persist the replacement against the displayed old path before dismissing the picker.
  // 以界面显示的原路径为条件持久化更换，然后关闭选择器。
  const replace = async (path: string, expectedVolume?: string) => {
    onChanged(await changeProjectDirectory(project.id, project.path, path, expectedVolume));
    void check(false);
  };

  if (!unavailable && !showChange && !picker) return null;
  return (
    <section className="project-directory-recovery" data-unavailable={unavailable ? "" : undefined}>
      {unavailable && (
        <div role="alert">
          <div>{t("projectDirectoryRecovery.unavailable")}</div>
          <code>{project.path}</code>
        </div>
      )}
      <div className="project-directory-recovery-actions">
        {unavailable && (
          <Button disabled={checking} onClick={() => { void check(true); }} variant="outline">
            {t("projectDirectoryRecovery.recheck")}
          </Button>
        )}
        <Button onClick={() => setPicker({ initialPath: unavailable ? null : project.path })} variant="outline">
          {t("projectDirectoryRecovery.change")}
        </Button>
      </div>
      {picker && (
        <React.Suspense fallback={null}>
          <FolderPicker
            title={t("projectDirectoryRecovery.change")}
            confirmLabel={t("projectDirectoryRecovery.change")}
            initialPath={picker.initialPath}
            open
            onBrowse={browseProjectReplacement}
            onConfirm={replace}
            onOpenChange={(open) => { if (!open) setPicker(null); }}
          />
        </React.Suspense>
      )}
    </section>
  );
}
