import * as React from "react";
import { XIcon } from "lucide-react";
import { composeHandlers, cx, useComposedRefs, useControllableState, useStableId } from "../behavior/core";
import { useFocusScope } from "../behavior/focus-scope";
import { LayerDepth, LayerPortal, useDismissable, type DismissOptions } from "../behavior/layer";
import { Slot } from "../behavior/slot";
import { Button } from "./button";
import { useUiText } from "./ui-text";

// Modal dialogs: a frosted scrim over the page, a focus-trapped surface, Escape and outside press to close.
// Dialog, AlertDialog and Sheet share this engine and differ only in role, dismissal and placement.
// 模态对话框：页面上方的磨砂遮罩、限制焦点的表面、Esc 与外部按下关闭。Dialog、AlertDialog 与 Sheet 共用该引擎，
// 仅在角色、关闭方式与位置上不同。

// ModalContextValue is the open state and the label ids shared by the parts of one dialog.
// ModalContextValue 是同一对话框各部分共享的打开状态与标签 id。
interface ModalContextValue {
  open: boolean;
  setOpen: (open: boolean) => void;
  titleId: string;
  descriptionId: string;
  hasTitle: boolean;
  hasDescription: boolean;
  registerTitle: (present: boolean) => void;
  registerDescription: (present: boolean) => void;
  // triggerRef is the DialogTrigger, which gets focus back when the dialog closes.
  // triggerRef 是 DialogTrigger，对话框关闭时焦点回到它。
  triggerRef: React.RefObject<HTMLElement | null>;
}

const ModalContext = React.createContext<ModalContextValue | null>(null);

// useModal reads the enclosing dialog.
// useModal 读取外层对话框。
export function useModal(): ModalContextValue {
  const context = React.useContext(ModalContext);
  if (!context) throw new Error("Dialog parts must be rendered inside a Dialog.");
  return context;
}

// DialogProps follow the Radix dialog root.
// DialogProps 沿用 Radix 对话框根组件。
export interface DialogProps {
  open?: boolean;
  defaultOpen?: boolean;
  onOpenChange?: (open: boolean) => void;
  modal?: boolean;
  children?: React.ReactNode;
}

// Dialog provides the open state to its trigger and content.
// Dialog 向触发器与内容提供打开状态。
export function Dialog({ open: controlled, defaultOpen = false, onOpenChange, children }: DialogProps) {
  const [open, setOpen] = useControllableState(controlled, defaultOpen, onOpenChange);
  const titleId = useStableId("vk-dialog-title");
  const descriptionId = useStableId("vk-dialog-description");
  const [hasTitle, registerTitle] = React.useState(false);
  const [hasDescription, registerDescription] = React.useState(false);
  const triggerRef = React.useRef<HTMLElement | null>(null);
  const context = React.useMemo(
    () => ({ open, setOpen, titleId, descriptionId, hasTitle, hasDescription, registerTitle, registerDescription, triggerRef }),
    [open, setOpen, titleId, descriptionId, hasTitle, hasDescription],
  );
  return <ModalContext.Provider value={context}>{children}</ModalContext.Provider>;
}

// DialogTrigger opens the dialog.
// DialogTrigger 打开对话框。
export function DialogTrigger({ asChild, onClick, ref, ...props }: React.ComponentProps<"button"> & { asChild?: boolean }) {
  const modal = useModal();
  const composed = useComposedRefs(modal.triggerRef as React.Ref<HTMLButtonElement>, ref);
  const Comp = asChild ? Slot : "button";
  return (
    <Comp
      ref={composed as never}
      type={asChild ? undefined : "button"}
      aria-haspopup="dialog"
      aria-expanded={modal.open}
      data-state={modal.open ? "open" : "closed"}
      data-slot="dialog-trigger"
      onClick={composeHandlers(onClick as React.MouseEventHandler<HTMLElement> | undefined, () => modal.setOpen(!modal.open))}
      {...(props as React.HTMLAttributes<HTMLElement>)}
    />
  );
}

// DialogClose closes the dialog.
// DialogClose 关闭对话框。
export function DialogClose({ asChild, onClick, ...props }: React.ComponentProps<"button"> & { asChild?: boolean }) {
  const modal = useModal();
  const Comp = asChild ? Slot : "button";
  return (
    <Comp
      type={asChild ? undefined : "button"}
      data-slot="dialog-close"
      onClick={composeHandlers(onClick as React.MouseEventHandler<HTMLElement> | undefined, () => modal.setOpen(false))}
      {...(props as React.HTMLAttributes<HTMLElement>)}
    />
  );
}

// DialogPortal is kept for API compatibility; content always renders in its own layer.
// DialogPortal 为兼容 API 而保留；内容始终渲染在自己的层中。
export function DialogPortal({ children }: { children?: React.ReactNode }) {
  return <>{children}</>;
}

// DialogOverlay is kept for API compatibility; the scrim is part of the content layer.
// DialogOverlay 为兼容 API 而保留；遮罩属于内容层的一部分。
export function DialogOverlay(_props: React.ComponentProps<"div">) {
  return null;
}

// ModalSurfaceProps configure the shared modal surface.
// ModalSurfaceProps 配置共享的模态表面。
export interface ModalSurfaceProps extends Omit<React.ComponentProps<"div">, "role">, DismissOptions {
  role: "dialog" | "alertdialog";
  slot: string;
  scrimClassName?: string;
  // dismissOnOutside is false for alert dialogs, which close only through their actions or Escape.
  // 警示对话框的 dismissOnOutside 为 false，只能通过其操作或 Esc 关闭。
  dismissOnOutside?: boolean;
  onOpenAutoFocus?: (event: Event) => void;
  onCloseAutoFocus?: (event: Event) => void;
  initialFocus?: (scope: HTMLElement) => HTMLElement | null;
  forceMount?: boolean;
}

// ModalSurface renders the scrim and the focus-trapped dialog element while open.
// ModalSurface 在打开期间渲染遮罩与限制焦点的对话框元素。
export function ModalSurface({
  role,
  slot,
  scrimClassName,
  dismissOnOutside = true,
  onEscapeKeyDown,
  onPointerDownOutside,
  onFocusOutside,
  onInteractOutside,
  onOpenAutoFocus,
  onCloseAutoFocus,
  initialFocus,
  forceMount: _forceMount,
  ref,
  children,
  ...props
}: ModalSurfaceProps) {
  const modal = useModal();
  const element = React.useRef<HTMLDivElement | null>(null);
  const composed = useComposedRefs(element, ref);
  const layer = useDismissable(modal.open, element, () => modal.setOpen(false), {
    blocking: true,
    onEscapeKeyDown,
    onPointerDownOutside: (event) => {
      onPointerDownOutside?.(event);
      if (!dismissOnOutside) event.preventDefault();
    },
    onFocusOutside,
    onInteractOutside,
  });
  useFocusScope(modal.open, element, {
    trapped: true,
    onOpenAutoFocus,
    onCloseAutoFocus,
    initialFocus,
    returnFocus: () => modal.triggerRef.current,
  });
  if (!modal.open) return null;
  return (
    <LayerPortal>
      <div className={cx("vk-scrim", scrimClassName)} data-slot={`${slot}-overlay`} data-state="open">
        <div
          ref={composed}
          role={role}
          aria-modal="true"
          aria-labelledby={modal.hasTitle ? modal.titleId : undefined}
          aria-describedby={modal.hasDescription ? modal.descriptionId : undefined}
          data-slot={`${slot}-content`}
          data-state="open"
          tabIndex={-1}
          {...layer.handlers}
          {...props}
        >
          <LayerDepth value={layer.depth}>{children}</LayerDepth>
        </div>
      </div>
    </LayerPortal>
  );
}

// DialogContentSize is the surface width; wide reads long documents at content height, while viewer and fullscreen also fix the height.
// DialogContentSize 是表面宽度；wide 以内容高度阅读长文档，viewer 与 fullscreen 同时固定高度。
export type DialogContentSize = "compact" | "sm" | "md" | "lg" | "xl" | "wide" | "viewer" | "fullscreen";

// DialogContentLayout sets the padding owned by the surface: forms are padded, documents and viewers are not.
// DialogContentLayout 设置表面负责的内边距：表单有内边距，文档与查看器没有。
export type DialogContentLayout = "form" | "document" | "viewer";

// DialogContentScrollMode chooses which layer owns vertical overflow in bounded-height dialogs.
// DialogContentScrollMode 选择有界高度对话框中由哪一层负责垂直溢出。
export type DialogContentScrollMode = "body-scroll" | "content-scroll" | "fixed-footer";

// DialogContentProps add semantic size, layout and overflow to the surface.
// DialogContentProps 为表面增加语义尺寸、布局与溢出设置。
export interface DialogContentProps extends Omit<ModalSurfaceProps, "role" | "slot"> {
  showCloseButton?: boolean;
  size?: DialogContentSize;
  layout?: DialogContentLayout;
  scrollMode?: DialogContentScrollMode;
}

// DialogContent renders the standard modal surface with an optional top-right close action.
// DialogContent 渲染标准模态表面，可附带右上角关闭操作。
export function DialogContent({ className, children, showCloseButton = true, size = "compact", layout = "form", scrollMode, ...props }: DialogContentProps) {
  const text = useUiText();
  return (
    <ModalSurface
      role="dialog"
      slot="dialog"
      data-size={size}
      data-layout={layout}
      data-scroll-mode={scrollMode}
      className={cx("vk-dialog", className)}
      {...props}
    >
      {children}
      {showCloseButton ? (
        <DialogClose asChild>
          <Button variant="ghost" size="icon-sm" className="vk-dialog-x">
            <XIcon />
            <span className="vk-sr-only">{text.close}</span>
          </Button>
        </DialogClose>
      ) : null}
    </ModalSurface>
  );
}

// DialogHeader stacks the title and description.
// DialogHeader 堆叠标题与说明。
export function DialogHeader({ className, ...props }: React.ComponentProps<"div">) {
  return <div data-slot="dialog-header" className={cx("vk-dialog-header", className)} {...props} />;
}

// DialogFooter holds the actions, optionally followed by a standard close button.
// DialogFooter 承载操作，可在末尾附带标准关闭按钮。
export function DialogFooter({ className, showCloseButton = false, children, ...props }: React.ComponentProps<"div"> & { showCloseButton?: boolean }) {
  const text = useUiText();
  return (
    <div data-slot="dialog-footer" className={cx("vk-dialog-footer", className)} {...props}>
      {children}
      {showCloseButton ? (
        <DialogClose asChild>
          <Button variant="outline">{text.close}</Button>
        </DialogClose>
      ) : null}
    </div>
  );
}

// useRegisterLabel tells the dialog that a title or description is mounted, so aria references resolve.
// useRegisterLabel 告知对话框标题或说明已挂载，使 aria 引用可以解析。
function useRegisterLabel(register: (present: boolean) => void): void {
  React.useLayoutEffect(() => {
    register(true);
    return () => register(false);
  }, [register]);
}

// DialogTitle names the dialog.
// DialogTitle 为对话框命名。
export function DialogTitle({ className, ...props }: React.ComponentProps<"h2">) {
  const modal = useModal();
  useRegisterLabel(modal.registerTitle);
  return <h2 id={modal.titleId} data-slot="dialog-title" className={cx("vk-dialog-title", className)} {...props} />;
}

// DialogDescription describes the dialog's purpose.
// DialogDescription 说明对话框的用途。
export function DialogDescription({ className, ...props }: React.ComponentProps<"p">) {
  const modal = useModal();
  useRegisterLabel(modal.registerDescription);
  return <p id={modal.descriptionId} data-slot="dialog-description" className={cx("vk-dialog-description", className)} {...props} />;
}
