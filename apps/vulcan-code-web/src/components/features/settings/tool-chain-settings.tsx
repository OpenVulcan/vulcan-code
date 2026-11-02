import * as React from "react";
import { SettingsErrorNotice } from "@/components/features/settings/settings-error-notice";
import type { SettingsActionProps } from "@/components/features/settings/settings-action-types";
import { useI18n } from "@/i18n";
import { errorMessage } from "@/lib/display-format";
import { Input } from "@/ui-kit/primitives/input";
import { Switch } from "@/ui-kit/primitives/switch";
import "@/ui-kit/styles/features/settings-forms.css";

// ToolChainSettingsSection switches the tool_chain tool for sessions without their own choice and sets its step limit.
// The service owns the accepted step range and rejects anything outside it.
// ToolChainSettingsSection 为未自行选择的会话开关 tool_chain 工具并设置其步骤上限。可接受的步骤范围由服务持有，超出即拒绝。
// Parameters: `actions` saves both values; `enabled` and `maxSteps` are the saved values.
// 参数：`actions` 保存两个值；`enabled` 与 `maxSteps` 是已保存的值。
export function ToolChainSettingsSection(props: { actions: SettingsActionProps; enabled: boolean; maxSteps: number | null }) {
  const { t } = useI18n();
  const [pending, setPending] = React.useState<{ enabled: boolean; maxSteps: number } | null>(null);
  const [error, setError] = React.useState("");
  const [stepsDraft, setStepsDraft] = React.useState(props.maxSteps === null ? "" : String(props.maxSteps));
  React.useEffect(() => { setStepsDraft(props.maxSteps === null ? "" : String(props.maxSteps)); }, [props.maxSteps]);

  // save commits both values and keeps the shown switch until the request settles.
  // save 提交两个值，并在请求结束前保持显示的开关。
  const save = async (enabled: boolean, maxSteps: number) => {
    if (pending) return;
    setPending({ enabled, maxSteps });
    setError("");
    try {
      await props.actions.onSaveToolChainSettings(enabled, maxSteps);
    } catch (cause) {
      setError(errorMessage(cause));
      setStepsDraft(props.maxSteps === null ? "" : String(props.maxSteps));
    } finally {
      setPending(null);
    }
  };

  // commitSteps saves an edited whole number; anything else returns to the saved value.
  // commitSteps 保存编辑后的整数；其他输入恢复为已保存的值。
  const commitSteps = () => {
    const next = Number(stepsDraft);
    if (props.maxSteps === null || !Number.isInteger(next) || next === props.maxSteps) {
      setStepsDraft(props.maxSteps === null ? "" : String(props.maxSteps));
      return;
    }
    void save(props.enabled, next);
  };

  const enabled = pending?.enabled ?? props.enabled;
  return <>
    <section className="vk-form-pref" data-tool-chain-settings>
      <div className="vk-form-pref-text">
        <strong>{t("settings.toolChain.title")}</strong>
        <p className="vk-form-note">{t("settings.toolChain.hint")}</p>
      </div>
      <Switch
        aria-label={t("settings.toolChain.title")}
        checked={enabled}
        disabled={pending !== null || props.maxSteps === null}
        onCheckedChange={(next) => { if (props.maxSteps !== null) void save(next, props.maxSteps); }}
      />
      {error ? <SettingsErrorNotice detail={error} message={t("settings.toolChain.saveFailed")} /> : null}
    </section>
    {enabled ? <label className="vk-form-pref">
      <span className="vk-form-pref-text">
        <strong>{t("settings.toolChain.maxSteps")}</strong>
        <small className="vk-form-note">{t("settings.toolChain.maxStepsHint")}</small>
      </span>
      <Input
        className="vk-form-w-sm"
        disabled={pending !== null || props.maxSteps === null}
        onBlur={commitSteps}
        onChange={(event) => setStepsDraft(event.currentTarget.value)}
        onKeyDown={(event) => { if (event.key === "Enter") commitSteps(); }}
        step={1}
        type="number"
        value={stepsDraft}
      />
    </label> : null}
  </>;
}
