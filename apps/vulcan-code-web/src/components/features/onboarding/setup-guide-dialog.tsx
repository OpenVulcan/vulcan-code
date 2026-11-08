import * as React from "react";
import { ExternalLink } from "lucide-react";
import { toast } from "@/ui-kit/primitives/toast";
import { StatusPill } from "@/components/shared/status";
import { Badge } from "@/ui-kit/primitives/badge";
import { Button } from "@/ui-kit/primitives/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/ui-kit/primitives/dialog";
import { RadioGroup, RadioGroupItem } from "@/ui-kit/primitives/radio-group";
import { StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder";
import { OpenVulcanAccountContext } from "@/components/features/account/openvulcan-account-context";
import { PermissionJudgeApiSection } from "@/components/features/settings/permission-judge-api-section";
import {
  PermissionJudgeEdgeSection, edgeModelEnabled, type EdgeModelList,
} from "@/components/features/settings/permission-judge-edge-section";
import { DEVICE_KEYS, PHASE_KEYS, REASON_KEYS, label } from "@/components/features/settings/permission-judge-labels";
import { PermissionJudgeRegisterForm, type PermissionJudgeRegisterRequest } from "@/components/features/settings/permission-judge-register-form";
import { SettingsErrorNotice } from "@/components/features/settings/settings-error-notice";
import { formatMessage, useI18n, type TranslationKey } from "@/i18n";
import { errorMessage } from "@/lib/display-format";
import {
  activatePermissionJudgeModel,
  loadPermissionJudgeEdgeModels,
  loadPermissionJudgeSettings,
  registerPermissionJudgeModel,
  savePermissionJudgeSettings,
} from "@/service/permission-judge-service";
import type { PermissionJudgeSettingsSnapshot } from "@/types";
import "@/ui-kit/styles/features/settings-forms.css";

// Choice is one of the four permission judge answers the guide offers.
// Choice 是引导提供的四种权限判定答案之一。
type Choice = "local" | "cloud" | "api" | "off";

// CHOICES lists the answers in the order the plan presents them, with their copy keys.
// CHOICES 按计划呈现的顺序列出答案及其文案键。
const CHOICES: { value: Choice; title: TranslationKey; description: TranslationKey; confirm: TranslationKey }[] = [
  { value: "local", title: "setupGuide.local.title", description: "setupGuide.local.description", confirm: "setupGuide.local.confirm" },
  { value: "cloud", title: "setupGuide.cloud.title", description: "setupGuide.cloud.description", confirm: "setupGuide.cloud.confirm" },
  { value: "api", title: "setupGuide.api.title", description: "setupGuide.api.description", confirm: "setupGuide.api.confirm" },
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

// SetupGuideDialog walks through the first setup step, the permission judge: a local model, the
// OpenVulcan cloud judge, a judge API on the user's own provider or not now. Only a confirmed choice
// saves the system settings.
// SetupGuideDialog 引导第一步设定权限判定：本机模型、OpenVulcan 云端判定、用户自有供应商上的判定 API 或
// 暂不启用。只有确认的选择才会保存系统设置。
export default function SetupGuideDialog(props: SetupGuideDialogProps) {
  const { t } = useI18n();
  const [snapshot, setSnapshot] = React.useState<PermissionJudgeSettingsSnapshot | null>(null);
  const [loadError, setLoadError] = React.useState<string | null>(null);
  const [pickedChoice, setPickedChoice] = React.useState<Choice | null>(null);
  const [busy, setBusy] = React.useState<string | null>(null);
  const [failure, setFailure] = React.useState<Failure | null>(null);
  // apiConfigured tells whether an API decision route is saved, as reported by its section.
  // apiConfigured 表示是否已保存 API 判定路由，由其区块报告。
  const [apiConfigured, setApiConfigured] = React.useState(false);
  // signedIn tells whether the OpenVulcan account the cloud judge uses is signed in.
  // signedIn 表示云端判定所用的 OpenVulcan 账户是否已登录。
  const signedIn = React.useContext(OpenVulcanAccountContext)?.snapshot?.connected === true;
  // choice is the answer picked here; until then it is only the shown default, never a saved value:
  // the cloud judge while the account is signed in and no judge is configured, else the local model.
  // choice 是此处选择的答案；选择之前只是显示的默认项，绝不写入设置：账户已登录且尚未配置判定时为
  // 云端判定，否则为本机模型。
  const choice: Choice = pickedChoice ?? (signedIn && snapshot?.backend === null ? "cloud" : "local");
  // edgeModels is the cloud model list, read while the cloud answer is chosen and the account is
  // signed in; edgeRead drops answers of superseded reads. pickedEdgeModel is the model picked here.
  // edgeModels 是云端模型清单，在选中云端答案且账户已登录时读取；edgeRead 丢弃已被取代的读取结果。
  // pickedEdgeModel 是在此处选择的模型。
  const [edgeModels, setEdgeModels] = React.useState<EdgeModelList>({ phase: "signedOut" });
  const [pickedEdgeModel, setPickedEdgeModel] = React.useState<string | null>(null);
  const edgeRead = React.useRef(0);
  const loadEdgeModels = React.useCallback(async () => {
    const read = ++edgeRead.current;
    setEdgeModels({ phase: "loading" });
    try {
      const list = await loadPermissionJudgeEdgeModels();
      if (read === edgeRead.current) setEdgeModels({ phase: "ready", models: list.models.map((model) => model.modelId) });
    } catch (cause) {
      if (read === edgeRead.current) setEdgeModels({ phase: "failed", error: errorMessage(cause) });
    }
  }, []);
  const cloudShown = props.open && choice === "cloud";
  React.useEffect(() => {
    if (signedIn && cloudShown) {
      void loadEdgeModels();
    } else {
      edgeRead.current += 1;
      setEdgeModels({ phase: "signedOut" });
    }
  }, [signedIn, cloudShown, loadEdgeModels]);

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
  // The cloud model is the one picked here, else the saved one, else the first the server enables.
  // 云端模型依次为此处选择的、已保存的、服务端启用的第一个。
  const edgeModelId = pickedEdgeModel ?? snapshot?.edgeModelId
    ?? (edgeModels.phase === "ready" ? edgeModels.models[0] ?? null : null);
  const ready = choice === "local" ? activeModel !== null
    : choice === "cloud" ? signedIn && edgeModelEnabled(edgeModels, edgeModelId)
      : choice === "api" ? apiConfigured : true;
  // confirm saves the chosen backend and keeps the other saved fields; not now writes nothing.
  // confirm 保存所选后端并保留其他已保存字段；暂不启用不写入任何内容。
  const confirm = () => run("confirm", "setupGuide.saveFailed", async () => {
    if (!snapshot) return;
    if (choice === "off") {
      props.onOpenChange(false);
      return;
    }
    await savePermissionJudgeSettings({
      backend: choice,
      edgeModelId: choice === "cloud" ? edgeModelId : snapshot.edgeModelId,
      device: snapshot.device ?? snapshot.recommendedDevice,
      cudaLibraryDirectory: snapshot.cuda.lockedDirectory,
      llmMayAllowBlocked: snapshot.llmMayAllowBlocked,
      recordDecisions: snapshot.recordDecisions,
      approvalTimeoutSeconds: snapshot.approvalTimeoutSeconds,
    });
    toast.success(t("settings.permissionJudge.saved"));
    props.onOpenChange(false);
  });

  return <Dialog open={props.open} onOpenChange={(open) => { if (busy === null) props.onOpenChange(open); }}>
    <DialogContent size="lg" scrollMode="fixed-footer">
      <DialogHeader>
        <DialogTitle>{t("setupGuide.title")}</DialogTitle>
        <DialogDescription>{t("setupGuide.judge.intro")}</DialogDescription>
      </DialogHeader>
      <div className="vk-dialog-body vk-form-loose">
        {!snapshot
          ? loadError
            ? <SettingsErrorNotice detail={loadError} message={t("settings.permissionJudge.loadFailed")} />
            : <StreamPlaceholder label={t("ui.loading")} shape="rows" count={3} />
          : <>
            <RadioGroup className="vk-form-choices" value={choice} onValueChange={(value) => setPickedChoice(value as Choice)} disabled={busy !== null}>
              {CHOICES.map((option) => (
                <label key={option.value} className="vk-form-choice" data-setup-choice={option.value}>
                  <RadioGroupItem value={option.value} />
                  <span>
                    <strong>{t(option.title)}</strong>
                    <small>{t(option.description)}</small>
                  </span>
                </label>
              ))}
            </RadioGroup>
            {choice === "local"
              ? <LocalModelStep busy={busy} run={run} snapshot={snapshot} onSnapshot={setSnapshot} />
              : null}
            {choice === "cloud"
              ? <PermissionJudgeEdgeSection edgeModels={edgeModels} edgeModelId={edgeModelId} disabled={busy !== null}
                t={t} onEdgeModelChange={setPickedEdgeModel} onRetry={() => void loadEdgeModels()} />
              : null}
            {choice === "api" ? <div className="vk-form-stack">
              <PermissionJudgeApiSection onConfiguredChange={setApiConfigured} />
              <div className="vk-form-inline">
                <Button type="button" size="sm" variant="ghost" onClick={props.onOpenProviderSettings}>
                  <ExternalLink size={14} />{t("setupGuide.api.providers")}
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
  return <section className="vk-form-stack" data-setup-step="local">
    <p className="vk-form-text">{formatMessage(t("setupGuide.local.devices"), { devices })}</p>
    {snapshot.recommendedDevice ? <p className="vk-form-note">
      {formatMessage(t("settings.permissionJudge.recommended"), { device: t(DEVICE_KEYS[snapshot.recommendedDevice]) })}
    </p> : null}
    {snapshot.models.length === 0
      ? <p className="vk-form-note">{t("setupGuide.local.howTo")}</p>
      : snapshot.models.map((model) => {
        const active = model.id === snapshot.activeModelId;
        return <div className="vk-form-inline" key={model.id}>
          <strong className="vk-form-strong">{model.name}</strong>
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
