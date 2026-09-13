import React from "react";
import { toast } from "sonner";
import { I18nProvider } from "../../src/i18n";
import { Button } from "../../src/components/ui/button";
import { Combobox } from "../../src/components/ui/combobox";
import { Command, CommandDialog, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList, CommandSeparator, CommandShortcut } from "../../src/components/ui/command";
import { InputGroup, InputGroupAddon, InputGroupButton, InputGroupInput, InputGroupText, InputGroupTextarea } from "../../src/components/ui/input-group";
import { NavItem, TreeRow } from "../../src/components/ui/navigation-item";
import { ScrollArea, ScrollBar } from "../../src/components/ui/scroll-area";
import { Toaster } from "../../src/components/ui/sonner";

/** Renders searchable collection controls with local-only identities and actions.
 * 使用纯本地标识和操作渲染可搜索集合控件。
 * No parameters; returns picker, command, navigation, input and toast fixtures.
 * 无参数；返回选择、命令、导航、输入及提示夹具。
 */
export function AuditPickerSurface() {
  // long enables explicit unbroken sample text.
  // long 启用明确的连续样本文本。
  const long = new URLSearchParams(window.location.search).get("content") === "long";
  // label tests rendering only; stable identities stay independent from display names.
  // label 仅验证显示；稳定标识与显示名称保持独立。
  const label = long ? "LongModelIdentifier".repeat(14) : "普通模型";
  // value is a controlled identity with two deliberately equal display labels.
  // value 是受控标识，目录中刻意包含两个相同显示名称。
  const [value, setValue] = React.useState("first");
  // disabled tests the documented parent-owned availability contract.
  // disabled 验证声明的父层可用性契约。
  const [disabled, setDisabled] = React.useState(false);
  // commandOpen controls the exported command dialog only.
  // commandOpen 仅控制导出的命令弹窗。
  const [commandOpen, setCommandOpen] = React.useState(false);
  // action records local callbacks without transport.
  // action 记录本地回调，不访问传输层。
  const [action, setAction] = React.useState("尚未操作");
  // selected retains the explicit demonstration navigation state.
  // selected 保留明确的演示导航状态。
  const [selected, setSelected] = React.useState(0);
  // options retain exact, unique command identities under duplicate labels and keyword filtering.
  // options 在同名和关键字过滤时保留精确唯一命令标识。
  const options = [{ value: "first", label: "同名模型", keywords: ["alpha"] }, { value: "second", label: "同名模型", keywords: ["beta"], description: "第二个账号" }, { value: "disabled", label: "禁用模型", disabled: true }, { value: "long", label }];
  return <I18nProvider><main className="h-dvh overflow-auto p-4">
    <h1>搜索与组合控件 · 不保存，不调用模型</h1><output aria-live="polite">{action} · {value}</output>
    <div className="grid min-w-0 gap-4 lg:grid-cols-2">
      <section className="min-w-0 space-y-3"><h2>搜索选择</h2><Combobox ariaLabel="验收模型" options={options} value={value} onValueChange={setValue} placeholder="请选择模型" searchPlaceholder="搜索模型" emptyText="没有匹配项" disabled={disabled} />
        <Button onClick={() => setDisabled(!disabled)}>切换模型禁用</Button><Combobox ariaLabel="空目录" options={[]} placeholder="空目录" searchPlaceholder="搜索空目录" emptyText="没有可用模型" onValueChange={setValue} />
        <InputGroup><InputGroupInput aria-label="组合输入" defaultValue="名称" /><InputGroupAddon><InputGroupText>输入前缀</InputGroupText></InputGroupAddon><InputGroupAddon align="inline-end"><InputGroupButton onClick={() => setAction("input:action")}>执行</InputGroupButton></InputGroupAddon></InputGroup>
        <InputGroup><InputGroupTextarea aria-label="组合多行" defaultValue="多行正文" /><InputGroupAddon align="block-start"><InputGroupText>多行前缀</InputGroupText></InputGroupAddon><InputGroupAddon align="block-end"><InputGroupButton onClick={() => setAction("textarea:action")}>多行操作</InputGroupButton></InputGroupAddon></InputGroup>
        <div className="flex gap-2">{(["xs", "sm", "icon-xs", "icon-sm"] as const).map(size => <InputGroupButton key={size} size={size} aria-label={`组合按钮 ${size}`} onClick={() => setAction(`size:${size}`)}>＋</InputGroupButton>)}</div>
        <InputGroup><InputGroupInput aria-label="禁用组合" disabled defaultValue="不可修改" /><InputGroupAddon><InputGroupText>禁用前缀</InputGroupText></InputGroupAddon></InputGroup>
      </section>
      <section className="min-w-0 space-y-3"><h2>命令及导航</h2><Button onClick={() => setCommandOpen(true)}>命令弹窗</Button>
        <CommandDialog open={commandOpen} onOpenChange={setCommandOpen} title="命令测试" description="只记录本地动作" showCloseButton><Command><CommandInput aria-label="搜索命令" placeholder="搜索命令" /><CommandList><CommandEmpty>没有命令</CommandEmpty><CommandGroup heading="常用"><CommandItem value="open" onSelect={() => setAction("command:open")}>打开<CommandShortcut>Enter</CommandShortcut></CommandItem><CommandItem disabled value="disabled">不可用</CommandItem></CommandGroup><CommandSeparator /><CommandGroup heading="其他">{Array.from({ length: 30 }, (_, index) => <CommandItem key={index} value={`row-${index}`} onSelect={() => setAction(`command:${index}`)}>命令 {index + 1}</CommandItem>)}</CommandGroup></CommandList></Command></CommandDialog>
        <nav aria-label="演示导航" className="flex flex-col">{[0, 1, 2].map(index => <NavItem key={index} selected={selected === index} disabled={index === 2} onClick={() => setSelected(index)}>页面 {index + 1}</NavItem>)}</nav>
        <div>{[0, 1, 2].map(index => <TreeRow key={index} role="button" depth={index} selected={selected === index} aria-pressed={selected === index} onClick={() => setSelected(index)}>层级 {index + 1}</TreeRow>)}</div>
        <ScrollArea className="h-40 w-full rounded border" type="always"><div className="w-[800px]">{Array.from({ length: 30 }, (_, index) => <Button className="block" key={index} onClick={() => setAction(`scroll:${index}`)}>滚动行 {index + 1} {label}</Button>)}</div><ScrollBar orientation="horizontal" /></ScrollArea>
        <Button onClick={() => toast("演示通知", { description: label, duration: Infinity, action: { label: "查看", onClick: () => setAction("toast:action") }, cancel: { label: "取消", onClick: () => setAction("toast:cancel") } })}>显示通知</Button><Button onClick={() => toast.dismiss()}>清除演示通知</Button>
      </section>
    </div><Toaster closeButton />
  </main></I18nProvider>;
}
