import * as React from "react";
import { cx } from "../behavior/core";
import { Label } from "./label";
import { Separator } from "./separator";

// FieldSet groups related fields under one legend.
// FieldSet 将相关字段归入同一图例之下。
export function FieldSet({ className, ...props }: React.ComponentProps<"fieldset">) {
  return <fieldset data-slot="field-set" className={cx("vk-fieldset", className)} {...props} />;
}

// FieldLegend titles a field set; the label variant reads like a field label.
// FieldLegend 为字段组命名；label 变体呈现为字段标签样式。
export function FieldLegend({ className, variant = "legend", ...props }: React.ComponentProps<"legend"> & { variant?: "legend" | "label" }) {
  return <legend data-slot="field-legend" data-variant={variant} className={cx("vk-field-legend", className)} {...props} />;
}

// FieldGroup stacks fields with the shared vertical rhythm.
// FieldGroup 以共享纵向节奏堆叠字段。
export function FieldGroup({ className, ...props }: React.ComponentProps<"div">) {
  return <div data-slot="field-group" className={cx("vk-field-group", className)} {...props} />;
}

// FieldOrientation places the label above, beside, or responsively relative to the control.
// FieldOrientation 将标签放在控件上方、旁边，或按宽度响应式放置。
export type FieldOrientation = "vertical" | "horizontal" | "responsive";

// Field is one label, control, description and error unit.
// Field 是由标签、控件、说明与错误组成的单元。
export function Field({ className, orientation = "vertical", ...props }: React.ComponentProps<"div"> & { orientation?: FieldOrientation | null }) {
  return (
    <div
      role="group"
      data-slot="field"
      data-orientation={orientation ?? "vertical"}
      className={cx("vk-field", className)}
      {...props}
    />
  );
}

// FieldContent stacks a label with its description beside a horizontal control.
// FieldContent 在横向控件旁堆叠标签与说明。
export function FieldContent({ className, ...props }: React.ComponentProps<"div">) {
  return <div data-slot="field-content" className={cx("vk-field-content", className)} {...props} />;
}

// FieldLabel labels the field's control.
// FieldLabel 标注字段控件。
export function FieldLabel({ className, ...props }: React.ComponentProps<typeof Label>) {
  return <Label data-slot="field-label" className={cx("vk-field-label", className)} {...props} />;
}

// FieldTitle is a label-styled heading for fields without a single control.
// FieldTitle 是没有单一控件的字段所用的标签样式标题。
export function FieldTitle({ className, ...props }: React.ComponentProps<"div">) {
  return <div data-slot="field-label" className={cx("vk-field-label", className)} {...props} />;
}

// FieldDescription explains the field.
// FieldDescription 说明字段用途。
export function FieldDescription({ className, ...props }: React.ComponentProps<"p">) {
  return <p data-slot="field-description" className={cx("vk-field-hint", className)} {...props} />;
}

// FieldSeparator divides field groups, optionally with a centered caption.
// FieldSeparator 分隔字段组，可附居中说明。
export function FieldSeparator({ children, className, ...props }: React.ComponentProps<"div">) {
  return (
    <div data-slot="field-separator" data-content={!!children} className={cx("vk-field-separator", className)} {...props}>
      <Separator />
      {children ? <span data-slot="field-separator-content">{children}</span> : null}
    </div>
  );
}

// FieldError shows the field's errors; duplicate messages are collapsed and nothing renders without one.
// FieldError 显示字段错误；重复消息会合并，没有错误时不渲染。
export function FieldError({
  className,
  children,
  errors,
  ...props
}: React.ComponentProps<"div"> & { errors?: Array<{ message?: string } | undefined> }) {
  const content = React.useMemo(() => {
    if (children) return children;
    if (!errors?.length) return null;
    const unique = [...new Map(errors.map((error) => [error?.message, error])).values()];
    if (unique.length === 1) return unique[0]?.message;
    return <ul>{unique.map((error, index) => (error?.message ? <li key={index}>{error.message}</li> : null))}</ul>;
  }, [children, errors]);
  if (!content) return null;
  return (
    <div role="alert" data-slot="field-error" className={cx("vk-field-error", className)} {...props}>
      {content}
    </div>
  );
}
