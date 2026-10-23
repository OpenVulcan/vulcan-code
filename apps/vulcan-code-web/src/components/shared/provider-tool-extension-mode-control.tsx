import { SegmentedControl, SegmentedControlItem } from "@/ui-kit/primitives/choice-controls";
import { useI18n } from "@/i18n";
import type { PublicAiProviderExtensionMode } from "@/types";

/** Switches one model-extension capability between a single fixed model and multiple models.
 * 在单个固定模型与多个模型之间切换一项模型扩展能力。
 *
 * @param props current mode, its disabled state and the change handler of the owning surface.
 *              当前模式、禁用状态及所属界面的变更处理函数。
 * @returns one segmented control shared by the settings page and the session dialog.
 *          设置页与会话对话框共用的分段控件。 */
export function ProviderToolExtensionModeControl(props: {
  mode: PublicAiProviderExtensionMode;
  disabled: boolean;
  onModeChange: (mode: PublicAiProviderExtensionMode) => void;
}) {
  const { t } = useI18n();
  return (
    <SegmentedControl
      aria-label={t("settings.providerTools.extensionMode")}
      disabled={props.disabled}
      value={props.mode}
      onValueChange={(value) => props.onModeChange(value as PublicAiProviderExtensionMode)}
    >
      <SegmentedControlItem value="single">
        {t("settings.providerTools.extensionModeSingle")}
      </SegmentedControlItem>
      <SegmentedControlItem value="multi">
        {t("settings.providerTools.extensionModeMulti")}
      </SegmentedControlItem>
    </SegmentedControl>
  );
}

/** Renders the "not selected" choice that leaves a single-model capability without a tool.
 * 渲染使单模型能力不提供工具的“未选择”选项。
 *
 * @param props radio group shared with the model cards, selection state and its handler.
 *              与模型卡片共用的单选组、选中状态及处理函数。
 * @returns one labelled radio row. 一个带标签的单选行。 */
export function ProviderToolExtensionNoModelChoice(props: {
  radioGroup: string;
  checked: boolean;
  disabled: boolean;
  onSelect: () => void;
}) {
  const { t } = useI18n();
  return (
    <label className="vk-check-label">
      <input
        checked={props.checked}
        className="vk-radio"
        disabled={props.disabled}
        name={props.radioGroup}
        onChange={(event) => {
          if (event.currentTarget.checked) props.onSelect();
        }}
        type="radio"
      />
      <span>{t("settings.providerTools.extensionNotSelected")}</span>
    </label>
  );
}
