import * as React from "react";
import { Button } from "@/ui-kit/primitives/button";
import { Input } from "@/ui-kit/primitives/input";
import { useI18n, type TranslationKey } from "@/i18n";
import {
  formatCustomContextWindowInput,
  type CustomContextWindowInputError,
} from "@/lib/context-window-selection";

// CustomContextWindowEditor is the inline custom-window input shared by the composer and settings.
// CustomContextWindowEditor 是输入区与设置页共用的内联自定义窗口输入。
// Parameters: props carry the text, bounds, validation error and callbacks; returns a small form.
// 参数：props 提供文本、范围、校验错误与回调；返回一个小表单。
export function CustomContextWindowEditor(props: {
  // Current raw text.
  // 当前原始文本。
  value: string;
  // Backend-projected minimum and the model's own window.
  // 后端投影的下限与模型自身窗口。
  minTokens: number;
  maxTokens: number;
  // Last validation error, or null.
  // 最近一次校验错误，或为空。
  error: CustomContextWindowInputError | null;
  // Disables the editor while a save runs.
  // 保存执行期间禁用编辑器。
  disabled?: boolean;
  onChange: (value: string) => void;
  onSubmit: () => void;
  onCancel: () => void;
}) {
  const { t } = useI18n();
  const errorId = React.useId();
  const min = formatCustomContextWindowInput(props.minTokens);
  const max = formatCustomContextWindowInput(props.maxTokens);
  const errorText = props.error === null
    ? null
    : t(CUSTOM_CONTEXT_WINDOW_ERROR_KEYS[props.error]).replace("{min}", min).replace("{max}", max);
  return (
    <form
      className="vk-window-custom"
      onSubmit={(event) => { event.preventDefault(); props.onSubmit(); }}
      onKeyDown={(event) => {
        if (event.key !== "Escape") return;
        event.stopPropagation();
        props.onCancel();
      }}
    >
      <div className="vk-window-custom-row">
        <Input
          aria-describedby={errorText ? errorId : undefined}
          aria-invalid={errorText ? true : undefined}
          aria-label={t("contextWindowCustom.label")}
          autoFocus
          density="compact"
          disabled={props.disabled}
          inputMode="numeric"
          onChange={(event) => props.onChange(event.target.value)}
          placeholder={t("contextWindowCustom.placeholder")}
          value={props.value}
        />
        <Button disabled={props.disabled} size="sm" type="submit" variant="outline">
          {t("contextWindowCustom.apply")}
        </Button>
      </div>
      {errorText
        ? <p className="vk-window-custom-error" id={errorId} role="alert">{errorText}</p>
        : <p className="vk-stats-note">{t("contextWindowCustom.hint").replace("{min}", min).replace("{max}", max)}</p>}
    </form>
  );
}

// CUSTOM_CONTEXT_WINDOW_ERROR_KEYS maps each validation error to its catalog entry.
// CUSTOM_CONTEXT_WINDOW_ERROR_KEYS 将每种校验错误映射到目录条目。
const CUSTOM_CONTEXT_WINDOW_ERROR_KEYS: Record<CustomContextWindowInputError, TranslationKey> = {
  format: "contextWindowCustom.error.format",
  belowMinimum: "contextWindowCustom.error.belowMinimum",
  aboveModel: "contextWindowCustom.error.aboveModel",
};

