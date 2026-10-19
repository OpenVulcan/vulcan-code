import { useRef, useState, type KeyboardEvent, type MouseEvent } from "react"

import { PageHint, PageSection } from "@/components/page"
import {
  AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle,
} from "@/ui-kit/primitives/alert-dialog"
import { Button } from "@/ui-kit/primitives/button"
import {
  ContextMenu, ContextMenuCheckboxItem, ContextMenuContent, ContextMenuItem, ContextMenuSeparator, ContextMenuShortcut,
  ContextMenuSub, ContextMenuSubContent, ContextMenuSubTrigger, ContextMenuTrigger,
} from "@/ui-kit/primitives/context-menu"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/ui-kit/primitives/dialog"
import {
  DropdownMenu, DropdownMenuCheckboxItem, DropdownMenuContent, DropdownMenuItem, DropdownMenuLabel, DropdownMenuRadioGroup, DropdownMenuRadioItem,
  DropdownMenuSeparator, DropdownMenuShortcut, DropdownMenuSub, DropdownMenuSubContent, DropdownMenuSubTrigger, DropdownMenuTrigger,
} from "@/ui-kit/primitives/dropdown-menu"
import { Field, FieldLabel } from "@/ui-kit/primitives/field"
import { HoverCard, HoverCardContent, HoverCardTrigger } from "@/ui-kit/primitives/hover-card"
import { Input } from "@/ui-kit/primitives/input"
import { Popover, PopoverContent, PopoverDescription, PopoverHeader, PopoverTitle, PopoverTrigger } from "@/ui-kit/primitives/popover"
import { Sheet, SheetContent, SheetDescription, SheetFooter, SheetHeader, SheetTitle } from "@/ui-kit/primitives/sheet"

import { actionLabel, type VisualLabDemoProps } from "./visual-lab-shared"

// ModalKind identifies the modal surface opened from the modal section.
// ModalKind 标识从模态分区打开的模态表面。
type ModalKind = "dialog" | "alert" | "sheet"

// VisualLabModals renders dialog, confirmation and sheet surfaces with focus returned to their launcher.
// VisualLabModals 渲染普通弹窗、确认弹窗与侧边抽屉，并把焦点还给启动按钮。
// Parameters: props supplies copy and the local action recorder. Returns one page section with controlled modals.
// 参数：props 提供文案与本地操作记录器。返回带受控模态的单个页面分区。
export function VisualLabModals({ text, onAction }: VisualLabDemoProps) {
  const [open, setOpen] = useState<ModalKind | null>(null)
  // launcher preserves the actual trigger for controlled modal focus restoration.
  // launcher 保留实际启动按钮，用于受控模态关闭后的焦点恢复。
  const launcher = useRef<HTMLButtonElement | null>(null)
  const show = (kind: ModalKind, event: MouseEvent<HTMLButtonElement>) => { launcher.current = event.currentTarget; setOpen(kind) }
  // restoreFocus prevents the primitive's default restoration and focuses the saved launcher.
  // restoreFocus 阻止原语默认恢复并聚焦已保存的启动按钮。
  const restoreFocus = (event: Event) => { event.preventDefault(); launcher.current?.focus() }
  const finish = (event: MouseEvent<HTMLElement>) => { onAction(actionLabel(event)); setOpen(null) }
  const openChange = (kind: ModalKind) => (next: boolean) => setOpen(next ? kind : null)
  return (
    <PageSection
      title={text("模态表面", "Modal surfaces")}
      description={text("弹窗承载短表单；确认弹窗只用于明确决定；抽屉用于保留页面上下文的辅助编辑。", "Dialogs hold short forms; confirmations are for explicit decisions; sheets keep page context for auxiliary editing.")}
    >
      <div className="visual-lab-row">
        <Button onClick={(event) => show("dialog", event)} variant="outline">{text("普通弹窗", "Dialog")}</Button>
        <Button onClick={(event) => show("alert", event)} variant="outline">{text("确认弹窗", "Confirmation")}</Button>
        <Button onClick={(event) => show("sheet", event)} variant="outline">{text("侧边抽屉", "Sheet")}</Button>
      </div>
      <Dialog open={open === "dialog"} onOpenChange={openChange("dialog")}>
        <DialogContent size="md" onCloseAutoFocus={restoreFocus}>
          <DialogHeader>
            <DialogTitle>{text("标准表单弹窗", "Standard form dialog")}</DialogTitle>
            <DialogDescription>{text("检查标题、说明、内容间距、关闭按钮和底部操作区。", "Review title, description, spacing, close action and footer.")}</DialogDescription>
          </DialogHeader>
          <Field>
            <FieldLabel htmlFor="visual-lab-dialog-input">{text("示例字段", "Example field")}</FieldLabel>
            <Input defaultValue={text("示例内容", "Example value")} id="visual-lab-dialog-input" />
          </Field>
          <DialogFooter>
            <Button onClick={() => setOpen(null)} variant="outline">{text("取消", "Cancel")}</Button>
            <Button onClick={finish}>{text("完成演示", "Finish demo")}</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      <AlertDialog open={open === "alert"} onOpenChange={openChange("alert")}>
        <AlertDialogContent onCloseAutoFocus={restoreFocus}>
          <AlertDialogHeader>
            <AlertDialogTitle>{text("确认当前操作？", "Confirm this action?")}</AlertDialogTitle>
            <AlertDialogDescription>{text("确认弹窗只承载需要明确决定的操作。", "Confirmations are reserved for explicit decisions.")}</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{text("取消", "Cancel")}</AlertDialogCancel>
            <AlertDialogAction onClick={(event) => onAction(actionLabel(event))}>{text("确认演示", "Confirm demo")}</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
      <Sheet open={open === "sheet"} onOpenChange={openChange("sheet")}>
        <SheetContent size="md" onCloseAutoFocus={restoreFocus}>
          <SheetHeader>
            <SheetTitle>{text("标准侧边抽屉", "Standard side sheet")}</SheetTitle>
            <SheetDescription>{text("用于需要保留主界面上下文的辅助编辑。", "Used for auxiliary editing that preserves page context.")}</SheetDescription>
          </SheetHeader>
          <PageHint className="visual-lab-sheet-body">{text("抽屉内容区域", "Sheet content area")}</PageHint>
          <SheetFooter>
            <Button onClick={finish}>{text("完成演示", "Finish demo")}</Button>
          </SheetFooter>
        </SheetContent>
      </Sheet>
    </PageSection>
  )
}

// VisualLabMenus renders dropdown, popover, hover card and context menu surfaces with controlled selections.
// VisualLabMenus 渲染下拉菜单、轻量浮层、悬停卡片与右键菜单，并采用受控选择。
// Parameters: props supplies copy and the local action recorder. Returns page sections of overlay samples.
// 参数：props 提供文案与本地操作记录器。返回浮层样本的页面分区。
export function VisualLabMenus({ text, onAction }: VisualLabDemoProps) {
  // Menu selections are controlled because Radix menu items do not own checked state.
  // 菜单选择采用受控状态，因为 Radix 菜单项不持有勾选状态。
  const [showActivity, setShowActivity] = useState(true)
  const [pinned, setPinned] = useState(true)
  const [density, setDensity] = useState("balanced")
  const select = (event: Event) => onAction(actionLabel(event))
  // openContextMenuKeyboard exposes the same native context-menu event to focused keyboard users.
  // openContextMenuKeyboard 向已聚焦的键盘用户提供同一个原生上下文菜单事件。
  const openContextMenuKeyboard = (event: KeyboardEvent<HTMLElement>) => {
    if (event.key !== "ContextMenu" && !(event.shiftKey && event.key === "F10")) return
    event.preventDefault()
    const rect = event.currentTarget.getBoundingClientRect()
    event.currentTarget.dispatchEvent(new window.MouseEvent("contextmenu", { bubbles: true, clientX: rect.left + 8, clientY: rect.top + 8 }))
  }
  return (
    <>
      <PageSection
        title={text("菜单与浮层", "Menus and overlays")}
        description={text("下拉菜单、轻量浮层与悬停卡片共享表面、密度、选中状态和碰撞翻转规则。", "Dropdowns, popovers and hover cards share surfaces, density, selection states and collision handling.")}
      >
        <div className="visual-lab-row">
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="outline">{text("打开下拉菜单", "Open dropdown")}</Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent width="md">
              <DropdownMenuLabel>{text("显示设置", "Display settings")}</DropdownMenuLabel>
              <DropdownMenuCheckboxItem checked={showActivity} onCheckedChange={(checked) => setShowActivity(checked === true)}>{text("显示活动状态", "Show activity")}</DropdownMenuCheckboxItem>
              <DropdownMenuRadioGroup value={density} onValueChange={setDensity}>
                <DropdownMenuRadioItem value="compact">{text("紧凑密度", "Compact density")}</DropdownMenuRadioItem>
                <DropdownMenuRadioItem value="balanced">{text("均衡密度", "Balanced density")}</DropdownMenuRadioItem>
              </DropdownMenuRadioGroup>
              <DropdownMenuSeparator />
              <DropdownMenuSub>
                <DropdownMenuSubTrigger>{text("更多操作", "More actions")}</DropdownMenuSubTrigger>
                <DropdownMenuSubContent>
                  <DropdownMenuItem onSelect={select}>{text("复制链接", "Copy link")}<DropdownMenuShortcut>Ctrl+C</DropdownMenuShortcut></DropdownMenuItem>
                  <DropdownMenuItem variant="destructive" onSelect={select}>{text("删除项目", "Delete project")}</DropdownMenuItem>
                </DropdownMenuSubContent>
              </DropdownMenuSub>
            </DropdownMenuContent>
          </DropdownMenu>
          <Popover>
            <PopoverTrigger asChild>
              <Button variant="outline">Popover</Button>
            </PopoverTrigger>
            <PopoverContent width="md">
              <PopoverHeader>
                <PopoverTitle>{text("轻量信息面板", "Lightweight panel")}</PopoverTitle>
                <PopoverDescription>{text("用于少量说明或紧凑操作，不承担完整弹窗职责。", "For concise guidance or compact actions, not full dialog workflows.")}</PopoverDescription>
              </PopoverHeader>
            </PopoverContent>
          </Popover>
          <HoverCard>
            <HoverCardTrigger asChild>
              <Button variant="ghost">HoverCard</Button>
            </HoverCardTrigger>
            <HoverCardContent width="md">
              <strong>{text("悬停详情", "Hover details")}</strong>
              <PageHint>{text("检查延迟、驻留、边界翻转和长文本换行。", "Review delay, persistence, collision flipping and long-text wrapping.")}</PageHint>
            </HoverCardContent>
          </HoverCard>
        </div>
      </PageSection>
      <PageSection
        title={text("右键菜单", "Context menu")}
        description={text("右键或 Shift+F10 打开；键盘用户与鼠标用户得到同一菜单。", "Opened by right-click or Shift+F10; keyboard and pointer users get the same menu.")}
      >
        <ContextMenu>
          <ContextMenuTrigger className="visual-lab-context-target" tabIndex={0} onKeyDown={openContextMenuKeyboard}>
            {text("在此区域点击右键，检查上下文菜单", "Right-click this area to review the context menu")}
          </ContextMenuTrigger>
          <ContextMenuContent width="md">
            <ContextMenuItem onSelect={select}>{text("打开", "Open")}<ContextMenuShortcut>Enter</ContextMenuShortcut></ContextMenuItem>
            <ContextMenuCheckboxItem checked={pinned} onCheckedChange={(checked) => setPinned(checked === true)}>{text("固定到侧栏", "Pin to sidebar")}</ContextMenuCheckboxItem>
            <ContextMenuSub>
              <ContextMenuSubTrigger>{text("移动到", "Move to")}</ContextMenuSubTrigger>
              <ContextMenuSubContent>
                <ContextMenuItem onSelect={select}>{text("工作区一", "Workspace one")}</ContextMenuItem>
                <ContextMenuItem onSelect={select}>{text("工作区二", "Workspace two")}</ContextMenuItem>
              </ContextMenuSubContent>
            </ContextMenuSub>
            <ContextMenuSeparator />
            <ContextMenuItem variant="destructive" onSelect={select}>{text("删除", "Delete")}</ContextMenuItem>
          </ContextMenuContent>
        </ContextMenu>
      </PageSection>
    </>
  )
}
