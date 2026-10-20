import * as React from "react";
import { XIcon } from "lucide-react";
import { cx } from "../behavior/core";
import { Button } from "./button";
import { Dialog, DialogClose, DialogDescription, DialogTitle, DialogTrigger, ModalSurface, type DialogProps, type ModalSurfaceProps } from "./dialog";
import { useUiText } from "./ui-text";

// Sheets are dialogs that slide in from a screen edge for side tasks that keep the page in view.
// Sheet 是从屏幕边缘滑入的对话框，用于保持页面可见的旁路任务。

// Sheet provides the open state.
// Sheet 提供打开状态。
export function Sheet(props: DialogProps) {
  return <Dialog {...props} />;
}

// SheetTrigger opens the sheet.
// SheetTrigger 打开侧边面板。
export function SheetTrigger(props: React.ComponentProps<typeof DialogTrigger>) {
  return <DialogTrigger data-slot="sheet-trigger" {...props} />;
}

// SheetClose closes the sheet.
// SheetClose 关闭侧边面板。
export function SheetClose(props: React.ComponentProps<typeof DialogClose>) {
  return <DialogClose data-slot="sheet-close" {...props} />;
}

// SheetContentProps choose the edge and width of the sheet.
// SheetContentProps 选择侧边面板的边缘与宽度。
export interface SheetContentProps extends Omit<ModalSurfaceProps, "role" | "slot"> {
  side?: "top" | "right" | "bottom" | "left";
  showCloseButton?: boolean;
  size?: "sm" | "md" | "lg";
}

// SheetContent renders the edge-anchored surface.
// SheetContent 渲染贴靠边缘的表面。
export function SheetContent({ className, children, side = "right", showCloseButton = true, size = "sm", ...props }: SheetContentProps) {
  const text = useUiText();
  return (
    <ModalSurface
      role="dialog"
      slot="sheet"
      scrimClassName="vk-drawer-scrim"
      data-side={side}
      data-size={size}
      className={cx("vk-drawer", className)}
      {...props}
    >
      {children}
      {showCloseButton ? (
        <DialogClose asChild>
          <Button variant="ghost" size="icon-sm" className="vk-dialog-x" data-slot="sheet-close">
            <XIcon />
            <span className="vk-sr-only">{text.close}</span>
          </Button>
        </DialogClose>
      ) : null}
    </ModalSurface>
  );
}

// SheetHeader holds the title and description.
// SheetHeader 承载标题与说明。
export function SheetHeader({ className, ...props }: React.ComponentProps<"div">) {
  return <div data-slot="sheet-header" className={cx("vk-drawer-header", className)} {...props} />;
}

// SheetFooter holds the actions at the bottom.
// SheetFooter 在底部承载操作。
export function SheetFooter({ className, ...props }: React.ComponentProps<"div">) {
  return <div data-slot="sheet-footer" className={cx("vk-drawer-footer", className)} {...props} />;
}

// SheetTitle names the sheet.
// SheetTitle 为侧边面板命名。
export function SheetTitle(props: React.ComponentProps<"h2">) {
  return <DialogTitle data-slot="sheet-title" {...props} />;
}

// SheetDescription describes the sheet.
// SheetDescription 说明侧边面板。
export function SheetDescription(props: React.ComponentProps<"p">) {
  return <DialogDescription data-slot="sheet-description" {...props} />;
}
