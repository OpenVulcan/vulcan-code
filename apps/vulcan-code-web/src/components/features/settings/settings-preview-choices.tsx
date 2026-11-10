import { useId, type ReactNode } from "react";
import { RadioGroupRoot, RadioItem } from "@/ui-kit/primitives/radio-group";
import "@/ui-kit/styles/features/settings-preview-choices.css";

// Each choice pairs the saved value with a visible label, an optional explanation and a decorative sample.
// 每项将保存值与可见标签、可选说明和装饰性示例配对。
export interface SettingsPreviewChoice<Value extends string> {
  value: Value;
  label: string;
  hint?: string;
  preview: ReactNode;
}

// Render a keyboard-accessible set of choice cards, using the given value and save callback.
// 使用传入的值和保存回调渲染一组支持键盘操作的选项卡片。
export function SettingsPreviewChoices<Value extends string>(props: {
  label: string;
  description?: string;
  options: readonly SettingsPreviewChoice<Value>[];
  value: Value;
  onSelect: (value: Value) => void;
  disabled?: boolean;
  action?: ReactNode;
  children?: ReactNode;
}) {
  // These IDs bind the group to the visible setting name and help text.
  // 这些 ID 将选项组关联到可见设置名称和说明。
  const labelId = useId();
  const descriptionId = useId();
  return <section className="settings-preview-field">
    <div className="settings-preview-heading">
      <div>
        <strong id={labelId}>{props.label}</strong>
        {props.description ? <p className="vk-form-note" id={descriptionId}>{props.description}</p> : null}
      </div>
      {props.action}
    </div>
    <RadioGroupRoot
      aria-labelledby={labelId}
      aria-describedby={props.description ? descriptionId : undefined}
      className="settings-preview-choices"
      data-pair={props.options.length === 2 ? "" : undefined}
      disabled={props.disabled}
      value={props.value}
      onValueChange={(value) => {
        // Accept only values rendered by this group, preserving the caller's value type.
        // 只接受当前组实际渲染的选项值，保留调用方的值类型。
        const choice = props.options.find((option) => option.value === value);
        if (choice) props.onSelect(choice.value);
      }}
    >
      {props.options.map((option) => (
        <RadioItem className="settings-preview-choice" key={option.value} value={option.value}>
          <span aria-hidden="true" className="settings-preview-art">{option.preview}</span>
          <span className="settings-preview-label">
            <strong>{option.label}</strong>
            <span aria-hidden="true" className="settings-preview-mark" />
          </span>
          {option.hint ? <small>{option.hint}</small> : null}
        </RadioItem>
      ))}
    </RadioGroupRoot>
    {props.children}
  </section>;
}
