import * as React from "react";
import { ExternalLink } from "lucide-react";
import { toast } from "sonner";
import { StatusPill } from "@/components/shared/status";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import { StreamPlaceholder } from "@/components/ui/stream-placeholder";
import { PermissionJudgeCloudSection } from "@/components/features/settings/permission-judge-cloud-section";
import { DEVICE_KEYS, PHASE_KEYS, REASON_KEYS, label } from "@/components/features/settings/permission-judge-labels";
import { PermissionJudgeRegisterForm, type PermissionJudgeRegisterRequest } from "@/components/features/settings/permission-judge-register-form";
import { SettingsErrorNotice } from "@/components/features/settings/settings-error-notice";
import { formatMessage, useI18n, type TranslationKey } from "@/i18n";
import { errorMessage } from "@/lib/display-format";
import {
  activatePermissionJudgeModel,
  loadPermissionJudgeSettings,
  registerPermissionJudgeModel,
  savePermissionJudgeSettings,
} from "@/service/permission-judge-service";
import type { JudgeBackend, JudgeState, PermissionJudgeSettingsSnapshot } from "@/types";

// Choice is one of the three permission judge answers the guide offers.
// Choice 是引导提供的三种权限判定答案之一。
type Choice = "local" | "cloud" | "off";

// CHOICES lists the answers in the order the plan presents them, with their copy keys.
// CHOICES 按计划呈现的顺序列出答案及其文案键。
const CHOICES: { value: Choice; title: TranslationKey; description: TranslationKey; confirm: TranslationKey }[] = [
  { value: "local", title: "setupGuide.local.title", description: "setupGuide.local.description", confirm: "setupGuide.local.confirm" },
  { value: "cloud", title: "setupGuide.cloud.title", description: "setupGuide.cloud.description", confirm: "setupGuide.cloud.confirm" },
  { value: "off", title: "setupGuide.off.title", description: "setupGuide.off.description", confirm: "setupGuide.off.confirm" },
];

// Failure is one failed action: a readable outcome plus the raw service error.
// Failure 是一次失败的操作：可读结果加原始服务错误。
interface Failure {
  message: string;
  detail: string;
}

// SetupGuideDialogProps controls the re-entrant setup guide.
// SetupGuideDialogProps 控制可重入的初始化引导。
export interface SetupGuideDialogProps {
  open: boolean;
  // Closing never writes settings; the judge stays unset until a choice is confirmed.
  // 关闭绝不写入设置；确认选择之前判定保持未设定。
  onOpenChange: (open: boolean) => void;
  // Opens the provider settings so the user can add a TypeSafe or OpenRouter account.
  // 打开供应商设置，供用户添加 TypeSafe 或 OpenRouter 账号。
  onOpenProviderSettings: () => void;
}

// SetupGuideDialog walks through the first setup step, the permission judge: a local model, a
// cloud judge or not now. Only a confirmed choice saves the system settings.
// SetupGuideDialog 引导第一步设定权限判定：本机模型、云端判定或暂不启用。只有确认的选择才会保存系统设置。
export default function SetupGuideDialog(props: SetupGuideDialogProps) {
  const { t } = useI18n();
  const [snapshot, setSnapshot] = React.useState<PermissionJudgeSettingsSnapshot | null>(null);
  const [loadError, setLoadError] = React.useState<string | null>(null);
  const [choice, setChoice] = React.useState<Choice>("local");
  const [busy, setBusy] = React.useState<string | null>(null);
  const [failure, setFailure] = React.useState<Failure | null>(null);
  // cloudConfigured tells whether a cloud decision route is saved, as reported by its section.
  // cloudConfigured 表示是否已保存云端判定路由，由其区块报告。
  const [cloudConfigured, setCloudConfigured] = React.useState(false);

  // Every opening re-reads the settings, so re-entering shows the current state.
  // 每次打开都重新读取设置，使重新进入时显示当前状态。
  React.useEffect(() => {
    if (!props.open) return;
    let cancelled = false;
    setFailure(null);
    void loadPermissionJudgeSettings().then(
      (next) => { if (!cancelled) { setSnapshot(next); setLoadError(null); } },
      (cause: unknown) => { if (!cancelled) setLoadError(errorMessage(cause)); },
    );
    return () => { cancelled = true; };
  }, [props.open]);

  // run executes one action, keeping the failure visible until the next action.
  // run 执行一个操作，失败信息保持显示直到下一次操作。
  const run = async (name: string, failureKey: TranslationKey, action: () => Promise<void>) => {
    if (busy) return;
    setBusy(name);
    setFailure(null);
    try {
      await action();
    } catch (cause) {
      setFailure({ message: t(failureKey), detail: errorMessage(cause) });
    } finally {
      setBusy(null);
    }
  };

  const activeModel = snapshot?.models.find((model) => model.id === snapshot.activeModelId) ?? null;
  const ready = choice === "local" ? activeModel !== null : choice === "cloud" ? cloudConfigured : true;
  // confirm saves the one explicit choice; the other saved fields stay as they are.
  // confirm 保存这一个显式选择；其他已保存字段保持不变。
  const confirm = () => run("confirm", "setupGuide.saveFailed", async () => {
    if (!snapshot) return;
    const state: JudgeState = choice === "off" ? "off" : "enabled";
    const backend: JudgeBackend | null = choice === "off" ? snapshot.backend : choice;
    await savePermissionJudgeSettings({
      state,
      backend,
      device: snapshot.device ?? snapshot.recommendedDevice,
      cudaLibraryDirectory: snapshot.cuda.lockedDirectory,
      llmMayAllowBlocked: snapshot.llmMayAllowBlocked,
      recordDecisions: snapshot.recordDecisions,
    });
    toast.success(t("settings.permissionJudge.saved"));
    props.onOpenChange(false);
  });

  return <Dialog open={props.open} onOpenChange={(open) => { if (busy === null) props.onOpenChange(open); }}>
    <DialogContent size="lg" scrollMode="fixed-footer" className="grid-rows-[auto_minmax(0,1fr)_auto]">
      <DialogHeader>
        <DialogTitle>{t("setupGuide.title")}</DialogTitle>
        <DialogDescription>{t("setupGuide.judge.intro")}</DialogDescription>
      </DialogHeader>
      <div className="grid min-h-0 gap-4 overflow-y-auto">
        {!snapshot
          ? loadError
            ? <SettingsErrorNotice detail={loadError} message={t("settings.permissionJudge.loadFailed")} />
            : <StreamPlaceholder label={t("ui.loading")} shape="rows" count={3} />
          : <>
            <RadioGroup value={choice} onValueChange={(value) => setChoice(value as Choice)} disabled={busy !== null}>
              {CHOICES.map((option) => (
                <label key={option.value} className="flex items-start gap-3 rounded-md border p-3" data-setup-choice={option.value}>
                  <RadioGroupItem value={option.value} className="mt-1" />
                  <span className="grid gap-1">
                    <span className="font-medium">{t(option.title)}</span>
                    <span className="text-sm text-muted-foreground">{t(option.description)}</span>
                  </span>
                </label>
              ))}
            </RadioGroup>
            {choice === "local"
              ? <LocalModelStep busy={busy} run={run} snapshot={snapshot} onSnapshot={setSnapshot} />
              : null}
            {choice === "cloud" ? <div className="grid gap-2">
              <PermissionJudgeCloudSection onConfiguredChange={setCloudConfigured} />
              <div className="settings-card-actions">
                <Button type="button" size="sm" variant="ghost" onClick={props.onOpenProviderSettings}>
                  <ExternalLink size={14} />{t("setupGuide.cloud.providers")}
                </Button>
              </div>
            </div> : null}
            {failure ? <SettingsErrorNotice detail={failure.detail} message={failure.message} /> : null}
          </>}
      </div>
      <DialogFooter>
        <Button type="button" variant="ghost" disabled={busy !== null} onClick={() => props.onOpenChange(false)}>
          {t("setupGuide.skip")}
        </Button>
        <Button type="button" disabled={!snapshot || !ready || busy !== null} onClick={() => void confirm()}>
          {t(CHOICES.find((option) => option.value === choice)!.confirm)}
        </Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>;
}

// LocalModelStep shows the detected devices and the registered models, registers a model folder
// and verifies and warms it up in place.
// LocalModelStep 显示探测到的设备与已登记模型，登记模型文件夹，并就地验证与预热。
function LocalModelStep(props: {
  busy: string | null;
  run: (name: string, failureKey: TranslationKey, action: () => Promise<void>) => Promise<void>;
  snapshot: PermissionJudgeSettingsSnapshot;
  onSnapshot: (snapshot: PermissionJudgeSettingsSnapshot) => void;
}) {
  const { t } = useI18n();
  const { snapshot } = props;
  const worker = snapshot.status.worker;
  const pill = worker.phase === "ready" ? "ready" : worker.phase === "failed" ? "error" : worker.phase === "warming" ? "warning" : "pending";
  const devices = snapshot.devices.map((device) => device.unavailableReason
    ? `${t(DEVICE_KEYS[device.device])} · ${label(t, REASON_KEYS, device.unavailableReason)}`
    : t(DEVICE_KEYS[device.device])).join(", ");
  // register adds the checked folder and, while none is in use, makes it the model in use, which
  // verifies and warms it up.
  // register 登记已检查的文件夹；尚无使用中模型时将其设为使用中模型，同时完成验证与预热。
  const register = (request: PermissionJudgeRegisterRequest, reset: () => void) =>
    props.run("register", "settings.permissionJudge.registerFailed", async () => {
      const registered = await registerPermissionJudgeModel(request.path, request.name, request.custom);
      const added = registered.models.find((model) => !snapshot.models.some((known) => known.id === model.id));
      props.onSnapshot(registered);
      reset();
      if (added && registered.activeModelId === null) props.onSnapshot(await activatePermissionJudgeModel(added.id));
    });
  return <section className="grid gap-2" data-setup-step="local">
    <p className="text-sm">{formatMessage(t("setupGuide.local.devices"), { devices })}</p>
    {snapshot.recommendedDevice ? <p className="text-sm text-muted-foreground">
      {formatMessage(t("settings.permissionJudge.recommended"), { device: t(DEVICE_KEYS[snapshot.recommendedDevice]) })}
    </p> : null}
    {snapshot.models.length === 0
      ? <p className="text-sm text-muted-foreground">{t("setupGuide.local.howTo")}</p>
      : snapshot.models.map((model) => {
        const active = model.id === snapshot.activeModelId;
        return <div className="flex flex-wrap items-center gap-2" key={model.id}>
          <span className="font-medium">{model.name}</span>
          {active ? <>
            <Badge variant="secondary">{t("settings.permissionJudge.modelActive")}</Badge>
            <StatusPill label={label(t, PHASE_KEYS, worker.phase)} status={pill} />
          </> : <Button type="button" size="sm" variant="outline" disabled={props.busy !== null}
            onClick={() => void props.run(`activate-${model.id}`, "settings.permissionJudge.activateFailed",
              async () => props.onSnapshot(await activatePermissionJudgeModel(model.id)))}>
            {t("settings.permissionJudge.activate")}
          </Button>}
        </div>;
      })}
    {worker.phase === "failed" && worker.error ? <SettingsErrorNotice detail={worker.error} message={t("settings.permissionJudge.status.loadFailed")} /> : null}
    <PermissionJudgeRegisterForm busy={props.busy} knownPackages={snapshot.knownPackages} onRegister={register} />
  </section>;
}
