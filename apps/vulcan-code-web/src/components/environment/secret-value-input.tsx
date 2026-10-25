import React from "react";
import { Eye, EyeOff } from "lucide-react";
import { useI18n } from "@/i18n";
import { InputGroup, InputGroupAddon, InputGroupButton, InputGroupInput } from "@/ui-kit/primitives/input-group";

// SecretValueInputProps describe one value field that stays masked until the user reveals it.
// SecretValueInputProps 描述一个在用户显示前保持遮蔽的值输入框。
interface SecretValueInputProps extends Omit<React.ComponentProps<"input">, "type" | "onChange" | "value"> {
  value: string;
  onValueChange: (value: string) => void;
}

// SecretValueInput masks its value by default; the eye button shows or hides only this field.
// SecretValueInput 默认遮蔽其值；眼睛按钮只显示或隐藏本输入框。
export function SecretValueInput({ value, onValueChange, disabled, ...props }: SecretValueInputProps) {
  const { t } = useI18n();
  // revealed is per field, so showing one value never shows the others.
  // revealed 按输入框保存，显示一个值不会显示其他值。
  const [revealed, setRevealed] = React.useState(false);
  const label = revealed ? t("environment.value.hide") : t("environment.value.show");
  return (
    <InputGroup data-revealed={revealed ? "true" : undefined}>
      <InputGroupInput
        {...props}
        autoComplete="off"
        disabled={disabled}
        onChange={(event) => onValueChange(event.target.value)}
        spellCheck={false}
        type={revealed ? "text" : "password"}
        value={value}
      />
      <InputGroupAddon align="inline-end">
        <InputGroupButton
          aria-label={label}
          aria-pressed={revealed}
          data-reveal-toggle
          disabled={disabled}
          onClick={() => setRevealed((current) => !current)}
          size="icon-xs"
          title={label}
        >
          {revealed ? <EyeOff aria-hidden="true" /> : <Eye aria-hidden="true" />}
        </InputGroupButton>
      </InputGroupAddon>
    </InputGroup>
  );
}
