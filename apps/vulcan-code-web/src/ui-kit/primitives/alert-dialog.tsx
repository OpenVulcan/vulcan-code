import * as React from "react";
import { composeHandlers, cx } from "../behavior/core";
import { Button, type ButtonProps } from "./button";
import {
  Dialog,
  DialogDescription,
  DialogTitle,
  DialogTrigger,
  ModalSurface,
  useModal,
  type DialogProps,
  type ModalSurfaceProps,
} from "./dialog";

// Alert dialogs interrupt for a decision: an outside press never closes them, and focus starts on Cancel
// so a stray Enter cannot confirm a destructive action.
// 警示对话框为一次决定而打断操作：外部按下不会关闭它，焦点从“取消”开始，避免误按 Enter 确认破坏性操作。

// AlertDialog provides the open state.
// AlertDialog 提供打开状态。
export function AlertDialog(props: DialogProps) {
  return <Dialog {...props} />;
}

// AlertDialogTrigger opens the alert dialog.
// AlertDialogTrigger 打开警示对话框。
export function AlertDialogTrigger(props: React.ComponentProps<typeof DialogTrigger>) {
  return <DialogTrigger data-slot="alert-dialog-trigger" {...props} />;
}

// AlertDialogPortal is kept for API compatibility.
// AlertDialogPortal 为兼容 API 而保留。
export function AlertDialogPortal({ children }: { children?: React.ReactNode }) {
  return <>{children}</>;
}

// AlertDialogOverlay is kept for API compatibility; the scrim belongs to the content layer.
// AlertDialogOverlay 为兼容 API 而保留；遮罩属于内容层。
export function AlertDialogOverlay(_props: React.ComponentProps<"div">) {
  return null;
}

// AlertDialogContentProps add the surface size and an extra class for the scrim layer.
// AlertDialogContentProps 增加表面尺寸以及遮罩层的附加类名。
export interface AlertDialogContentProps extends Omit<ModalSurfaceProps, "role" | "slot" | "dismissOnOutside"> {
  size?: "compact" | "sm" | "md";
  layerClassName?: string;
}

// cancelFirst focuses the Cancel action when present.
// cancelFirst 在存在“取消”操作时优先聚焦它。
function cancelFirst(scope: HTMLElement): HTMLElement | null {
  return scope.querySelector<HTMLElement>('[data-slot="alert-dialog-cancel"]:not([disabled])');
}

// AlertDialogContent renders the decision surface.
// AlertDialogContent 渲染决定表面。
export function AlertDialogContent({ className, layerClassName, size = "compact", ...props }: AlertDialogContentProps) {
  return (
    <ModalSurface
      role="alertdialog"
      slot="alert-dialog"
      dismissOnOutside={false}
      initialFocus={cancelFirst}
      scrimClassName={layerClassName}
      data-size={size}
      data-layout="form"
      className={cx("vk-dialog", className)}
      {...props}
    />
  );
}

// AlertDialogHeader stacks the media, title and description.
// AlertDialogHeader 堆叠图标、标题与说明。
export function AlertDialogHeader({ className, ...props }: React.ComponentProps<"div">) {
  return <div data-slot="alert-dialog-header" className={cx("vk-dialog-header", className)} {...props} />;
}

// AlertDialogFooter holds Cancel and the confirming action.
// AlertDialogFooter 承载“取消”与确认操作。
export function AlertDialogFooter({ className, ...props }: React.ComponentProps<"div">) {
  return <div data-slot="alert-dialog-footer" className={cx("vk-dialog-footer", className)} {...props} />;
}

// AlertDialogMedia holds an icon that signals the decision's weight.
// AlertDialogMedia 承载表明决定分量的图标。
export function AlertDialogMedia({ className, ...props }: React.ComponentProps<"div">) {
  return <div data-slot="alert-dialog-media" className={cx("vk-dialog-media", className)} {...props} />;
}

// AlertDialogTitle states the decision.
// AlertDialogTitle 陈述需要做出的决定。
export function AlertDialogTitle(props: React.ComponentProps<"h2">) {
  return <DialogTitle data-slot="alert-dialog-title" {...props} />;
}

// AlertDialogDescription explains the consequences.
// AlertDialogDescription 说明后果。
export function AlertDialogDescription(props: React.ComponentProps<"p">) {
  return <DialogDescription data-slot="alert-dialog-description" {...props} />;
}

// AlertDialogAction confirms and closes, unless its click handler prevented default.
// AlertDialogAction 确认并关闭，除非其点击处理器阻止了默认行为。
export function AlertDialogAction({ variant = "default", size = "default", onClick, ...props }: ButtonProps) {
  const modal = useModal();
  return (
    <Button
      data-slot="alert-dialog-action"
      variant={variant}
      size={size}
      onClick={composeHandlers(onClick, () => modal.setOpen(false))}
      {...props}
    />
  );
}

// AlertDialogCancel dismisses without acting.
// AlertDialogCancel 不执行操作直接关闭。
export function AlertDialogCancel({ variant = "outline", size = "default", onClick, ...props }: ButtonProps) {
  const modal = useModal();
  return (
    <Button
      data-slot="alert-dialog-cancel"
      variant={variant}
      size={size}
      onClick={composeHandlers(onClick, () => modal.setOpen(false))}
      {...props}
    />
  );
}
