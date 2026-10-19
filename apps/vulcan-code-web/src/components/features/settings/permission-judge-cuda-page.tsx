import * as React from "react";
import { Cpu, Download, ExternalLink, FolderCheck, Library, MonitorCog } from "lucide-react";
import { toast } from "@/ui-kit/primitives/toast";
import { PageItem, PageNotice, PageRow, PageSection } from "@/components/page";
import { StatusPill } from "@/components/shared/status";
import { Badge } from "@/ui-kit/primitives/badge";
import { Button } from "@/ui-kit/primitives/button";
import { Input } from "@/ui-kit/primitives/input";
import { Progress } from "@/ui-kit/primitives/progress";
import { formatMessage, type Translate, type TranslationKey } from "@/i18n";
import { errorMessage, formatBytes } from "@/lib/display-format";
import { label } from "@/components/features/settings/permission-judge-labels";
import { openExternalUrl } from "@/service/project-actions-service";
import type {
  PermissionJudgeCudaComponent,
  PermissionJudgeCudaInstall,
  PermissionJudgeCudaSnapshot,
} from "@/types";
import "@/ui-kit/styles/features/settings-forms.css"

// Labels of stable codes sent by the service; unknown codes render as themselves.
// 服务发送的稳定代码对应的标签；未知代码按原样显示。
const INSTALL_PHASE_KEYS: Record<string, TranslationKey> = {
  downloading: "settings.permissionJudge.cuda.phase.downloading",
  verifying: "settings.permissionJudge.cuda.phase.verifying",
  extracting: "settings.permissionJudge.cuda.phase.extracting",
  completed: "settings.permissionJudge.cuda.phase.completed",
};
const SOURCE_KEYS: Record<string, TranslationKey> = {
  managed: "settings.permissionJudge.cuda.source.managed",
  toolkit: "settings.permissionJudge.cuda.source.toolkit",
  search_path: "settings.permissionJudge.cuda.source.searchPath",
};
const LINK_KEYS: Record<string, TranslationKey> = {
  cuda_toolkit: "settings.permissionJudge.cuda.link.toolkit",
  cuda_toolkit_archive: "settings.permissionJudge.cuda.link.toolkitArchive",
  nvidia_driver: "settings.permissionJudge.cuda.link.driver",
  cuda_redistributables: "settings.permissionJudge.cuda.link.redistributables",
  cuda_compatibility: "settings.permissionJudge.cuda.link.compatibility",
  onnxruntime_cuda_requirements: "settings.permissionJudge.cuda.link.onnxruntimeRequirements",
};
export const COMPONENT_KEYS: Record<PermissionJudgeCudaComponent, TranslationKey> = {
  onnxruntime: "settings.permissionJudge.cuda.onnxruntime",
  cuda_libraries: "settings.permissionJudge.cuda.libraries",
};
// Link offered next to a missing or outdated driver.
// 驱动缺失或过旧时一并提供的链接。
const DRIVER_LINK_ID = "nvidia_driver";

// CudaReadiness tells which of the three parts CUDA needs are in place.
// CudaReadiness 表示 CUDA 所需三个部分中哪些已就绪。
export interface CudaReadiness {
  driver: boolean;
  onnxruntime: boolean;
  libraries: boolean;
}

// cudaReadiness derives readiness from the snapshot: a driver new enough, an installed ONNX Runtime
// GPU build, and a complete library folder (the locked one, or a detected one when none is locked).
// cudaReadiness 从快照推导就绪状态：足够新的驱动、已安装的 ONNX Runtime GPU 版，以及完整的函式库
// 文件夹（已锁定的文件夹；未锁定时为探测到的文件夹）。
export function cudaReadiness(cuda: PermissionJudgeCudaSnapshot): CudaReadiness {
  return {
    driver: cuda.driver !== null && (cuda.minimumDriverMajor === null || cuda.driver.major >= cuda.minimumDriverMajor),
    onnxruntime: cuda.onnxruntimeDirectory !== null,
    libraries: cuda.lockedDirectory
      ? cuda.lockedDirectoryMissing.length === 0
      : cuda.detectedDirectories.length > 0,
  };
}

// cudaMissingLabels names the parts that are not ready, in setup order.
// cudaMissingLabels 按设置顺序列出未就绪部分的名称。
export function cudaMissingLabels(t: Translate, readiness: CudaReadiness): string[] {
  return [
    readiness.driver ? null : t("settings.permissionJudge.cuda.driver"),
    readiness.onnxruntime ? null : t(COMPONENT_KEYS.onnxruntime),
    readiness.libraries ? null : t(COMPONENT_KEYS.cuda_libraries),
  ].filter((name): name is string => name !== null);
}

/** CUDA environment page of the permission judge: the three parts CUDA needs with their install or
 * download actions, the library folder in use, and the official pages for manual downloads.
 * 权限判定的 CUDA 环境页：CUDA 所需的三个部分及其安装或下载操作、使用中的函式库文件夹，以及手动下载用的官方页面。
 * The folder choice saves at once through `onSelectDirectory`; installs report progress through `install`. */
export function PermissionJudgeCudaPage(props: {
  busy: string | null;
  cuda: PermissionJudgeCudaSnapshot;
  install: PermissionJudgeCudaInstall | null;
  // Folder saved for the judge; null follows the first detected folder.
  // 为判定保存的文件夹；null 表示跟随第一个探测到的文件夹。
  selectedDirectory: string | null;
  t: Translate;
  onInstall: (component: PermissionJudgeCudaComponent) => void;
  onSelectDirectory: (directory: string) => void;
}) {
  const { t, cuda, install } = props;
  const [custom, setCustom] = React.useState("");
  const open = (url: string) => void openExternalUrl(url).catch((cause) => {
    toast.error(t("settings.permissionJudge.cuda.openFailed"), { description: errorMessage(cause) });
  });
  if (!cuda.supported) {
    return <PageNotice tone="info">{t("settings.permissionJudge.cuda.unsupported")}</PageNotice>;
  }
  const readiness = cudaReadiness(cuda);
  const installing = install?.running === true;
  const driverLink = cuda.links.find((link) => link.id === DRIVER_LINK_ID);
  const archiveOf = (component: PermissionJudgeCudaComponent) => cuda.archives.find((archive) => archive.component === component);
  const pill = (ready: boolean) => <StatusPill status={ready ? "ready" : "warning"}
    label={t(ready ? "settings.permissionJudge.cuda.ready" : "settings.permissionJudge.cuda.missing")} />;
  // installButton offers one download-and-install, or a reinstall once the part is in place.
  // installButton 提供一次下载并安装；部分已就绪时改为重新安装。
  const installButton = (component: PermissionJudgeCudaComponent, ready: boolean) => {
    const archive = archiveOf(component);
    if (!archive) return null;
    return <Button type="button" size="sm" variant={ready ? "ghost" : "default"} data-cuda-install={component}
      disabled={props.busy !== null || installing} onClick={() => props.onInstall(component)}>
      <Download size={14} />
      {ready
        ? t("settings.permissionJudge.cuda.reinstall")
        : formatMessage(t("settings.permissionJudge.cuda.downloadInstall"), { size: formatBytes(archive.size) })}
    </Button>;
  };
  const capability = cuda.driver?.computeCapability
    ? ` · ${formatMessage(t("settings.permissionJudge.cuda.computeCapability"), { value: cuda.driver.computeCapability })}`
    : "";
  const driverValue = cuda.driver
    ? formatMessage(t("settings.permissionJudge.cuda.driverValue"), { version: cuda.driver.version, gpus: cuda.driver.gpus.join(", ") }) + capability
    : t("settings.permissionJudge.cuda.driverMissing");
  const librariesDirectory = cuda.lockedDirectory ?? cuda.detectedDirectories[0]?.directory ?? null;
  const progress = install?.completedBytes != null && install.totalBytes
    ? Math.round((install.completedBytes / install.totalBytes) * 100)
    : null;

  return <>
    <PageSection layout="rows" data-cuda-components="" title={t("settings.permissionJudge.cuda.components")}
      description={t("settings.permissionJudge.cuda.componentsHint")}>
      <PageItem icon={<MonitorCog />} tone={readiness.driver ? "neutral" : "attention"} title={t("settings.permissionJudge.cuda.driver")}
        meta={pill(readiness.driver)}
        description={cuda.minimumDriverMajor != null && cuda.cudaMajor != null
          ? `${driverValue} · ${formatMessage(t("settings.permissionJudge.cuda.driverMinimum"), { major: cuda.minimumDriverMajor, cuda: cuda.cudaMajor })}`
          : driverValue}
        actions={driverLink ? <Button type="button" size="sm" variant={readiness.driver ? "ghost" : "default"} onClick={() => open(driverLink.url)}>
          <ExternalLink size={14} />{t("settings.permissionJudge.cuda.getDriver")}
        </Button> : undefined} />
      <PageItem icon={<Cpu />} tone={readiness.onnxruntime ? "neutral" : "attention"} title={t(COMPONENT_KEYS.onnxruntime)}
        meta={pill(readiness.onnxruntime)}
        description={cuda.onnxruntimeDirectory ?? t("settings.permissionJudge.cuda.notInstalled")}
        actions={installButton("onnxruntime", readiness.onnxruntime)} />
      <PageItem icon={<Library />} tone={readiness.libraries ? "neutral" : "attention"} title={t(COMPONENT_KEYS.cuda_libraries)}
        meta={pill(readiness.libraries)}
        description={librariesDirectory ?? formatMessage(t("settings.permissionJudge.cuda.required"), { libraries: cuda.requiredLibraries.join(", ") })}
        actions={installButton("cuda_libraries", readiness.libraries)} />
      {installing && install ? <PageNotice tone="info" data-cuda-install-progress="">
        <p>{formatMessage(t("settings.permissionJudge.cuda.installing"), {
          component: t(COMPONENT_KEYS[install.component]),
          phase: install.phase ? label(t, INSTALL_PHASE_KEYS, install.phase) : "…",
          progress: install.completedBytes != null && install.totalBytes
            ? `${formatBytes(install.completedBytes)} / ${formatBytes(install.totalBytes)}`
            : "",
        })}</p>
        {progress !== null ? <Progress className="mt-2" value={progress} /> : null}
      </PageNotice> : install?.error ? <PageNotice tone="error" details={install.error} detailsLabel={t("settings.errors.showErrorDetails")}>
        {t("settings.permissionJudge.cuda.installFailed")}
      </PageNotice> : install ? <PageNotice tone="success">
        {formatMessage(t("settings.permissionJudge.cuda.installed"), { component: t(COMPONENT_KEYS[install.component]), directory: install.directory ?? "" })}
      </PageNotice> : null}
    </PageSection>

    <PageSection layout="rows" data-cuda-folder="" title={t("settings.permissionJudge.cuda.folder")}
      description={t("settings.permissionJudge.cuda.folderHint")}>
      {cuda.lockedDirectory && cuda.lockedDirectoryMissing.length > 0 ? <PageNotice tone="warning">
        {formatMessage(t("settings.permissionJudge.cuda.lockedMissing"), { libraries: cuda.lockedDirectoryMissing.join(", ") })}
      </PageNotice> : null}
      {cuda.detectedDirectories.map((detected) => {
        const inUse = props.selectedDirectory === detected.directory;
        return <PageItem key={detected.directory} icon={<FolderCheck />} tone={inUse ? "success" : "neutral"}
          title={label(t, SOURCE_KEYS, detected.source)} description={detected.directory}
          actions={inUse
            ? <Badge variant="secondary">{t("settings.permissionJudge.modelActive")}</Badge>
            : <Button type="button" size="sm" variant="outline" disabled={props.busy !== null}
              onClick={() => props.onSelectDirectory(detected.directory)}>
              {t("settings.permissionJudge.cuda.useDirectory")}
            </Button>} />;
      })}
      <PageRow stacked label={t("settings.permissionJudge.cuda.customFolder")}>
        <div className="flex w-full items-center gap-2">
          <Input aria-label={t("settings.permissionJudge.cuda.customFolder")} className="vk-form-fill"
            placeholder={props.selectedDirectory ?? ""} value={custom} onChange={(event) => setCustom(event.target.value)} />
          <Button type="button" size="sm" variant="outline" disabled={props.busy !== null || custom.trim() === ""}
            onClick={() => props.onSelectDirectory(custom.trim())}>
            {t("settings.permissionJudge.cuda.apply")}
          </Button>
        </div>
      </PageRow>
    </PageSection>

    {cuda.links.length > 0 ? <PageSection layout="rows" data-cuda-manual="" title={t("settings.permissionJudge.cuda.manual")}
      description={t("settings.permissionJudge.cuda.manualHint")}>
      {cuda.links.map((link) => (
        <PageItem key={link.id} icon={<ExternalLink />} title={label(t, LINK_KEYS, link.id)}
          actions={<Button type="button" size="sm" variant="ghost" onClick={() => open(link.url)}>
            {t("settings.permissionJudge.open")}
          </Button>} />
      ))}
    </PageSection> : null}
  </>;
}
