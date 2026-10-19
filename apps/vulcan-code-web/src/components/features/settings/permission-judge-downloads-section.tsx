import { Download, PackageCheck } from "lucide-react";
import { PageItem, PageNotice, PageSection } from "@/components/page";
import { Badge } from "@/ui-kit/primitives/badge";
import { Button } from "@/ui-kit/primitives/button";
import { Progress } from "@/ui-kit/primitives/progress";
import { formatMessage, type Translate, type TranslationKey } from "@/i18n";
import { formatBytes } from "@/lib/display-format";
import { DEVICE_KEYS, FORMAT_KEYS, label } from "@/components/features/settings/permission-judge-labels";
import type {
  PermissionJudgeKnownPackage,
  PermissionJudgeModelDownload,
  PermissionJudgeModelFormat,
} from "@/types";

// Labels of the download phases sent by the service; unknown phases render as themselves.
// 服务发送的下载阶段标签；未知阶段按原样显示。
const DOWNLOAD_PHASE_KEYS: Record<string, TranslationKey> = {
  manifest: "settings.permissionJudge.downloads.phase.manifest",
  downloading: "settings.permissionJudge.downloads.phase.downloading",
  verifying: "settings.permissionJudge.downloads.phase.verifying",
  installing: "settings.permissionJudge.downloads.phase.installing",
};

// PermissionJudgeDownloadsSection lists the published formats of every official package, marks the
// one recommended for the device and graphics card, and downloads one into the registry.
// PermissionJudgeDownloadsSection 列出每个官方模型包的已发布格式，标记适合当前设备与显卡的推荐格式，
// 并将其中一个下载到登记表。
export function PermissionJudgeDownloadsSection(props: {
  busy: string | null;
  download: PermissionJudgeModelDownload | null;
  knownPackages: PermissionJudgeKnownPackage[];
  t: Translate;
  onDownload: (packageId: string, format: PermissionJudgeModelFormat) => void;
}) {
  const { download, knownPackages, t } = props;
  const running = download?.running ?? false;
  const nameOf = (packageId: string, format: PermissionJudgeModelFormat) => {
    const displayName = knownPackages.find((known) => known.id === packageId)?.displayName ?? packageId;
    return `${displayName} · ${label(t, FORMAT_KEYS, format)}`;
  };
  const items = knownPackages.flatMap((known) => known.variants.map((variant) => ({ known, variant })));
  if (items.length === 0) return null;
  const progress = running && download && download.totalBytes > 0
    ? Math.round((download.completedBytes / download.totalBytes) * 100)
    : null;
  return <PageSection layout="rows" data-judge-downloads="" title={t("settings.permissionJudge.downloads.title")}
    description={t("settings.permissionJudge.downloads.hint")}>
    {items.map(({ known, variant }) => {
      const installed = variant.installedModelId !== null;
      const devices = variant.devices.map((device) => t(DEVICE_KEYS[device])).join(", ");
      const size = variant.downloadBytes === null
        ? t("settings.permissionJudge.downloads.unpublished")
        : formatMessage(t("settings.permissionJudge.downloads.size"), { size: formatBytes(variant.downloadBytes) });
      return <PageItem key={`${known.id}-${variant.format}`} data-judge-variant={`${known.id}:${variant.format}`}
        icon={installed ? <PackageCheck /> : <Download />} tone={variant.recommended ? "success" : "neutral"}
        title={nameOf(known.id, variant.format)}
        meta={<>
          {variant.recommended ? <Badge variant="secondary" data-judge-recommended="">{t("settings.permissionJudge.downloads.recommended")}</Badge> : null}
          {installed ? <Badge variant="outline">{t("settings.permissionJudge.downloads.installed")}</Badge> : null}
        </>}
        description={`${formatMessage(t("settings.permissionJudge.downloads.devices"), { devices })} · ${size}`}
        actions={installed ? undefined : <Button type="button" size="sm" variant={variant.recommended ? "default" : "outline"}
          disabled={variant.downloadBytes === null || running || props.busy !== null}
          onClick={() => props.onDownload(known.id, variant.format)}>
          <Download size={14} />{t("settings.permissionJudge.downloads.download")}
        </Button>} />;
    })}
    {running && download ? <PageNotice tone="info" data-judge-download-progress="">
      <p>{formatMessage(t("settings.permissionJudge.downloads.running"), {
        name: nameOf(download.packageId, download.format),
        phase: download.phase ? label(t, DOWNLOAD_PHASE_KEYS, download.phase) : "…",
        progress: `${formatBytes(download.completedBytes)} / ${formatBytes(download.totalBytes)}`,
      })}</p>
      {progress !== null ? <Progress className="mt-2" value={progress} /> : null}
    </PageNotice> : download?.error ? <PageNotice tone="error" details={download.error} detailsLabel={t("settings.errors.showErrorDetails")}>
      {t("settings.permissionJudge.downloads.failed")}
    </PageNotice> : download?.modelId ? <PageNotice tone="success">
      {formatMessage(t("settings.permissionJudge.downloads.done"), { name: nameOf(download.packageId, download.format) })}
    </PageNotice> : null}
  </PageSection>;
}
