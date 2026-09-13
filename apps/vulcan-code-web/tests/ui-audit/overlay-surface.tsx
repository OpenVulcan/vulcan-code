import React from "react";
import { I18nProvider } from "../../src/i18n";
import { Button } from "../../src/components/ui/button";
import { Input } from "../../src/components/ui/input";
import { Dialog, DialogClose, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from "../../src/components/ui/dialog";
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle, AlertDialogTrigger } from "../../src/components/ui/alert-dialog";
import { Sheet, SheetClose, SheetContent, SheetDescription, SheetFooter, SheetHeader, SheetTitle, SheetTrigger } from "../../src/components/ui/sheet";
import { Popover, PopoverContent, PopoverDescription, PopoverHeader, PopoverTitle, PopoverTrigger } from "../../src/components/ui/popover";
import { HoverCard, HoverCardContent, HoverCardTrigger } from "../../src/components/ui/hover-card";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "../../src/components/ui/tooltip";

/** Renders production overlays with local-only actions and explicit long-content scenarios.
 * 使用纯本地动作及明确长内容场景渲染生产浮层。
 * No parameters; returns the isolated overlay controls and their action status.
 * 无参数；返回隔离浮层控件及其操作状态。
 */
export function AuditOverlaySurface() {
  // query selects fixture content, not application preferences or business configuration.
  // query 选择夹具内容，不修改应用偏好或业务配置。
  const query = new URLSearchParams(window.location.search);
  // long expands a synthetic title and body to exercise actual containment.
  // long 扩展合成标题和正文以验证实际容纳边界。
  const long = query.get("content") === "long";
  // title is a deliberate unbroken name that can occur in model and file details.
  // title 是模型和文件详情可能出现的连续长名称样本。
  const title = long ? "LongModelOrFileIdentifier".repeat(12) : "示例详情";
  // action records only fixture callbacks.
  // action 仅记录夹具回调。
  const [action, setAction] = React.useState("尚未操作");
  // scrollMode uses the three declared production layout contracts.
  // scrollMode 使用三个已声明的生产布局契约。
  const [scrollMode, setScrollMode] = React.useState<"body-scroll" | "content-scroll" | "fixed-footer">("body-scroll");
  // side selects one declared sheet direction.
  // side 选择一种已声明的抽屉方向。
  const [side, setSide] = React.useState<"top" | "right" | "bottom" | "left">("right");
  // paragraphs updates only rendered sample content.
  // paragraphs 仅生成待显示的样本正文。
  const paragraphs = Array.from({ length: long ? 24 : 2 }, (_, index) => <p key={index}>第 {index + 1} 项说明。此处仅验证弹层正文与操作的可达性。</p>);
  return <I18nProvider><TooltipProvider delayDuration={0}><main className="h-dvh overflow-auto p-4">
    <h1>浮层合成验收 · 不保存，不调用模型</h1><output aria-live="polite">{action}</output>
    <div className="my-4 flex flex-wrap gap-3">
      <label>滚动模式<select value={scrollMode} onChange={event => setScrollMode(event.currentTarget.value as typeof scrollMode)}><option value="body-scroll">整体滚动</option><option value="content-scroll">正文滚动</option><option value="fixed-footer">固定操作栏</option></select></label>
      <label>抽屉方向<select value={side} onChange={event => setSide(event.currentTarget.value as typeof side)}>{(["top", "right", "bottom", "left"] as const).map(edge => <option key={edge} value={edge}>{edge}</option>)}</select></label>
    </div>
    <div className="flex flex-wrap gap-3">
      <Dialog><DialogTrigger asChild><Button>普通弹窗</Button></DialogTrigger>
        <DialogContent size="md" scrollMode={scrollMode} className={scrollMode === "body-scroll" ? undefined : "grid-rows-[auto_minmax(0,1fr)_auto]"}>
          <DialogHeader><DialogTitle>{title}</DialogTitle><DialogDescription>普通弹窗说明</DialogDescription></DialogHeader>
          <div data-audit-body className={scrollMode === "body-scroll" ? undefined : "min-h-0 overflow-auto"}><Input aria-label="弹窗字段" defaultValue="可编辑" />{paragraphs}<Button onClick={() => setAction("dialog:body")}>正文末尾操作</Button></div>
          <DialogFooter><DialogClose asChild><Button variant="outline">取消弹窗</Button></DialogClose><Button onClick={() => setAction("dialog:save")}>保存演示</Button></DialogFooter>
        </DialogContent>
      </Dialog>
      <AlertDialog><AlertDialogTrigger asChild><Button>确认弹窗</Button></AlertDialogTrigger><AlertDialogContent size="md"><AlertDialogHeader><AlertDialogTitle>{title}</AlertDialogTitle><AlertDialogDescription>{long ? title : "仅记录本地确认，不删除任何数据。"}</AlertDialogDescription></AlertDialogHeader><AlertDialogFooter><AlertDialogCancel>取消确认</AlertDialogCancel><AlertDialogAction onClick={() => setAction("alert:confirm")}>确认演示</AlertDialogAction></AlertDialogFooter></AlertDialogContent></AlertDialog>
      <Sheet><SheetTrigger asChild><Button>侧边抽屉</Button></SheetTrigger><SheetContent side={side} size="md"><SheetHeader><SheetTitle>{title}</SheetTitle><SheetDescription>抽屉详情说明</SheetDescription></SheetHeader><div className="px-4" data-audit-body>{paragraphs}</div><SheetFooter><SheetClose asChild><Button onClick={() => setAction("sheet:finish")}>完成抽屉</Button></SheetClose></SheetFooter></SheetContent></Sheet>
      <Popover><PopoverTrigger asChild><Button>信息浮层</Button></PopoverTrigger><PopoverContent width="lg"><PopoverHeader><PopoverTitle>{title}</PopoverTitle><PopoverDescription>浮层辅助说明</PopoverDescription></PopoverHeader><Input aria-label="浮层字段" /><Button onClick={() => setAction("popover:apply")}>应用演示</Button></PopoverContent></Popover>
      <HoverCard openDelay={0}><HoverCardTrigger asChild><Button variant="outline">悬停卡片</Button></HoverCardTrigger><HoverCardContent width="lg"><strong>{title}</strong><p>只读说明，焦点进入与悬停均可查看。</p></HoverCardContent></HoverCard>
      <Tooltip><TooltipTrigger asChild><Button variant="outline">辅助说明</Button></TooltipTrigger><TooltipContent>{title}</TooltipContent></Tooltip>
      <Button variant="ghost">外部焦点</Button>
    </div>
  </main></TooltipProvider></I18nProvider>;
}
