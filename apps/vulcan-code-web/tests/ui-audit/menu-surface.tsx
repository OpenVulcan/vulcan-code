import React from "react";
import { Home, Plus } from "lucide-react";
import { I18nProvider } from "../../src/i18n";
import { Button } from "../../src/components/ui/button";
import { TooltipProvider } from "../../src/components/ui/tooltip";
import * as Dropdown from "../../src/components/ui/dropdown-menu";
import * as Context from "../../src/components/ui/context-menu";
import { Sidebar, SidebarContent, SidebarFooter, SidebarGroup, SidebarGroupLabel, SidebarHeader, SidebarInset, SidebarMenu, SidebarMenuAction, SidebarMenuButton, SidebarMenuItem, SidebarProvider, SidebarTrigger, useSidebarNavigation } from "../../src/components/ui/sidebar";

/** Renders exact sidebar destination callbacks inside the owning provider.
 * 在所属提供者内渲染精确的侧栏目标回调。
 * Parameters: onAction records the selected local identity. Returns: navigation, actions and a fixed footer.
 * 参数：onAction 记录选中的本地标识。返回值：导航、操作及固定底栏。
 */
function AuditSidebarEntries({ onAction }: { onAction: (value: string) => void }) {
  // navigate closes only a mobile drawer and preserves the destination identity.
  // navigate 仅关闭手机抽屉并保留目标标识。
  const navigate = useSidebarNavigation();
  return <><SidebarHeader>导航演示</SidebarHeader><SidebarContent><SidebarGroup><SidebarGroupLabel>常用页面</SidebarGroupLabel><SidebarMenu>{Array.from({ length: 35 }, (_, index) => <SidebarMenuItem key={index}><SidebarMenuButton active={index === 0} tooltip={`页面 ${index + 1}`} disabled={index === 1} onClick={() => navigate(onAction)(`navigation:${index}`)}><Home /><span>页面 {index + 1}</span></SidebarMenuButton><SidebarMenuAction showOnHover aria-label={`操作 ${index + 1}`} onClick={() => onAction(`row-action:${index}`)}><Plus /></SidebarMenuAction></SidebarMenuItem>)}</SidebarMenu></SidebarGroup></SidebarContent><SidebarFooter>固定导航底栏</SidebarFooter></>;
}

/** Renders real menu primitives and responsive navigation without external actions.
 * 渲染真实菜单基础组件与响应式导航，不进行外部操作。
 * No parameters; returns the isolated menu and sidebar fixture.
 * 无参数；返回隔离菜单及侧栏夹具。
 */
export function AuditMenuSurface() {
  // long selects explicit long menu text and a larger submenu directory.
  // long 选择明确的长菜单文本与较大的子目录。
  const long = new URLSearchParams(window.location.search).get("content") === "long";
  // label is a synthetic unbroken name, independent of every selection identity.
  // label 是合成连续名称，与所有选择标识独立。
  const label = long ? "LongMenuIdentifier".repeat(12) : "普通操作";
  // action, checked and radio own only local fixture state.
  // action、checked 和 radio 仅持有本地夹具状态。
  const [action, setAction] = React.useState("尚未操作");
  const [checked, setChecked] = React.useState<boolean | "indeterminate">("indeterminate");
  const [radio, setRadio] = React.useState("one");
  return <I18nProvider><TooltipProvider delayDuration={0}><SidebarProvider className="h-dvh"><Sidebar><AuditSidebarEntries onAction={setAction} /></Sidebar><SidebarInset className="min-w-0 overflow-auto p-4"><header><SidebarTrigger /><h1>菜单与导航合成验收</h1></header><output aria-live="polite">{action} · {String(checked)} · {radio}</output><div className="my-4 flex flex-wrap gap-3">
    <Dropdown.DropdownMenu><Dropdown.DropdownMenuTrigger asChild><Button>下拉菜单</Button></Dropdown.DropdownMenuTrigger><Dropdown.DropdownMenuContent width="md"><Dropdown.DropdownMenuLabel>显示设置</Dropdown.DropdownMenuLabel><Dropdown.DropdownMenuGroup><Dropdown.DropdownMenuItem onSelect={() => setAction("dropdown:ordinary")}>{label}<Dropdown.DropdownMenuShortcut>Enter</Dropdown.DropdownMenuShortcut></Dropdown.DropdownMenuItem><Dropdown.DropdownMenuItem disabled>禁用操作</Dropdown.DropdownMenuItem><Dropdown.DropdownMenuCheckboxItem checked={checked} onCheckedChange={setChecked} onSelect={event => event.preventDefault()}>部分选择</Dropdown.DropdownMenuCheckboxItem></Dropdown.DropdownMenuGroup><Dropdown.DropdownMenuRadioGroup value={radio} onValueChange={setRadio}><Dropdown.DropdownMenuRadioItem value="one">选项一</Dropdown.DropdownMenuRadioItem><Dropdown.DropdownMenuRadioItem value="two">选项二</Dropdown.DropdownMenuRadioItem></Dropdown.DropdownMenuRadioGroup><Dropdown.DropdownMenuSeparator /><Dropdown.DropdownMenuSub><Dropdown.DropdownMenuSubTrigger>更多目录</Dropdown.DropdownMenuSubTrigger><Dropdown.DropdownMenuSubContent>{Array.from({ length: long ? 60 : 3 }, (_, index) => <Dropdown.DropdownMenuItem key={index} onSelect={() => setAction(`dropdown:sub:${index}`)}>目录 {index + 1}</Dropdown.DropdownMenuItem>)}</Dropdown.DropdownMenuSubContent></Dropdown.DropdownMenuSub><Dropdown.DropdownMenuItem variant="destructive" onSelect={() => setAction("dropdown:danger")}>危险演示</Dropdown.DropdownMenuItem></Dropdown.DropdownMenuContent></Dropdown.DropdownMenu>
    <Context.ContextMenu><Context.ContextMenuTrigger className="rounded border p-4" tabIndex={0}>右键菜单区域</Context.ContextMenuTrigger><Context.ContextMenuContent width="md"><Context.ContextMenuLabel>显示设置</Context.ContextMenuLabel><Context.ContextMenuGroup><Context.ContextMenuItem onSelect={() => setAction("context:ordinary")}>{label}<Context.ContextMenuShortcut>Enter</Context.ContextMenuShortcut></Context.ContextMenuItem><Context.ContextMenuItem disabled>禁用操作</Context.ContextMenuItem><Context.ContextMenuCheckboxItem checked={checked} onCheckedChange={setChecked} onSelect={event => event.preventDefault()}>部分选择</Context.ContextMenuCheckboxItem></Context.ContextMenuGroup><Context.ContextMenuRadioGroup value={radio} onValueChange={setRadio}><Context.ContextMenuRadioItem value="one">选项一</Context.ContextMenuRadioItem><Context.ContextMenuRadioItem value="two">选项二</Context.ContextMenuRadioItem></Context.ContextMenuRadioGroup><Context.ContextMenuSeparator /><Context.ContextMenuSub><Context.ContextMenuSubTrigger>更多目录</Context.ContextMenuSubTrigger><Context.ContextMenuSubContent>{Array.from({ length: long ? 60 : 3 }, (_, index) => <Context.ContextMenuItem key={index} onSelect={() => setAction(`context:sub:${index}`)}>目录 {index + 1}</Context.ContextMenuItem>)}</Context.ContextMenuSubContent></Context.ContextMenuSub><Context.ContextMenuItem variant="destructive" onSelect={() => setAction("context:danger")}>危险演示</Context.ContextMenuItem></Context.ContextMenuContent></Context.ContextMenu>
    <Button variant="outline">外部焦点</Button>
  </div></SidebarInset></SidebarProvider></TooltipProvider></I18nProvider>;
}
