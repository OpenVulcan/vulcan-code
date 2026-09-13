import React from "react";
import { Plus } from "lucide-react";
import { I18nProvider } from "../../src/i18n";
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from "../../src/components/ui/accordion";
import { Button, IconButton } from "../../src/components/ui/button";
import { Checkbox } from "../../src/components/ui/checkbox";
import { FilterChip, FilterChipGroup, SegmentedControl, SegmentedControlItem } from "../../src/components/ui/choice-controls";
import { Input } from "../../src/components/ui/input";
import { RadioGroup, RadioGroupItem } from "../../src/components/ui/radio-group";
import { Select, SelectContent, SelectGroup, SelectItem, SelectLabel, SelectSeparator, SelectTrigger, SelectValue } from "../../src/components/ui/select";
import { Switch } from "../../src/components/ui/switch";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "../../src/components/ui/tabs";
import { Textarea } from "../../src/components/ui/textarea";

// longLabel is a synthetic identifier used to exercise real selection and form layout limits.
// longLabel 是合成标识，用于验证真实选择和表单布局边界。
const longLabel = "LongProviderModelIdentifier".repeat(10);

/** Renders real selection primitives with local state and no service or filesystem actions.
 * 使用本地状态渲染真实选择基础组件，不进行服务或文件系统操作。
 * No parameters; returns the explicitly selected normal or long-content fixture.
 * 无参数；返回明确选择的普通或长内容夹具。
 */
export function AuditSelectionSurface() {
  // query selects the closed fixture states on the isolated origin.
  // query 选择隔离源中的封闭夹具状态。
  const query = new URLSearchParams(window.location.search);
  // long determines only sample text and never changes production component styles.
  // long 仅决定样本文字，不改变生产组件样式。
  const long = query.get("content") === "long";
  // action reports the exact last local interaction for browser assertions.
  // action 报告准确的最近本地交互，供浏览器断言使用。
  const [action, setAction] = React.useState("尚未操作");
  // checked exercises all three states declared by the Radix checkbox contract.
  // checked 覆盖 Radix 复选框契约声明的三个状态。
  const [checked, setChecked] = React.useState<boolean | "indeterminate">("indeterminate");
  // switched, radio, segmented, and chip are independent fixture-owned selections.
  // switched、radio、segmented 与 chip 是夹具独立持有的选择状态。
  const [switched, setSwitched] = React.useState(false);
  const [radio, setRadio] = React.useState("one");
  const [segmented, setSegmented] = React.useState("retired");
  const [chip, setChip] = React.useState(false);
  // expandedBody changes content height while the actual accordion stays mounted.
  // expandedBody 在真实折叠区保持挂载时改变正文高度。
  const [expandedBody, setExpandedBody] = React.useState(false);
  return <I18nProvider initialLocale={query.get("locale") === "en-US" ? "en-US" : "zh-CN"}>
    <main style={{ padding: 16, height: "100dvh", overflow: "auto" }}>
      <p>选择与输入合成验收 · 不保存、不调用模型</p><p role="status">{action}</p>
      <div style={{ display: "grid", gap: 20, gridTemplateColumns: "repeat(auto-fit, minmax(min(340px, 100%), 1fr))", alignItems: "start" }}>
        <section aria-label="布尔与单选" style={{ minWidth: 0, display: "grid", gap: 12 }}>
          <label style={{ display: "flex", gap: 12, alignItems: "center" }}><Checkbox checked={checked} onCheckedChange={(value) => { setChecked(value); setAction(`checkbox:${String(value)}`); }} />部分选择复选框</label>
          <label style={{ display: "flex", gap: 12, alignItems: "center" }}><Checkbox disabled checked />禁用复选框</label>
          <label style={{ display: "flex", gap: 12, alignItems: "center" }}><Switch checked={switched} onCheckedChange={(value) => { setSwitched(value); setAction(`switch:${String(value)}`); }} />通知开关</label>
          <label style={{ display: "flex", gap: 12, alignItems: "center" }}><Switch disabled checked />禁用开关</label>
          <RadioGroup aria-label="单选演示" value={radio} onValueChange={(value) => { setRadio(value); setAction(`radio:${value}`); }}>{["one", "disabled", "three"].map((value) => <label key={value} style={{ display: "flex", gap: 12, alignItems: "center" }}><RadioGroupItem value={value} disabled={value === "disabled"} />{value}</label>)}</RadioGroup>
          <SegmentedControl aria-label="失效项演示" value={segmented} onValueChange={(value) => { setSegmented(value); setAction(`segment:${value}`); }}><SegmentedControlItem value="retired" disabled>已失效</SegmentedControlItem><SegmentedControlItem value="default">默认</SegmentedControlItem><SegmentedControlItem value="extended">扩展</SegmentedControlItem></SegmentedControl>
          <FilterChipGroup aria-label="筛选演示"><FilterChip selected={chip} onClick={() => { setChip(!chip); setAction(`chip:${String(!chip)}`); }}>已启用</FilterChip><FilterChip disabled>不可用</FilterChip></FilterChipGroup>
        </section>
        <section aria-label="输入与动作" style={{ minWidth: 0, display: "grid", gap: 12 }}>
          {(["compact", "default", "large"] as const).map((density) => <React.Fragment key={density}><Input density={density} aria-label={`输入 ${density}`} defaultValue={long ? longLabel : "名称"} /><Textarea density={density} aria-label={`正文 ${density}`} defaultValue={long ? longLabel : "第一行\n第二行"} /></React.Fragment>)}
          <Input disabled aria-label="禁用输入" defaultValue="不可修改" /><Textarea readOnly aria-label="只读正文" defaultValue="只能选择复制" />
          <Input aria-invalid="true" aria-label="无效输入" defaultValue="无效值" />
          <div style={{ display: "flex", flexWrap: "wrap", gap: 8 }}>{(["default", "outline", "secondary", "ghost", "destructive", "link"] as const).map((variant) => <Button key={variant} variant={variant} onClick={() => setAction(`button:${variant}`)}>{variant}</Button>)}</div>
          <div style={{ display: "flex", flexWrap: "wrap", gap: 8 }}>{(["xs", "sm", "default", "lg"] as const).map((size) => <Button key={size} size={size} onClick={() => setAction(`size:${size}`)}>{size}</Button>)}{(["xs", "sm", "md", "lg"] as const).map((size) => <IconButton key={size} size={size} aria-label={`图标 ${size}`} onClick={() => setAction(`icon:${size}`)}><Plus /></IconButton>)}<Button disabled onClick={() => setAction("错误：禁用触发")}>禁用按钮</Button></div>
        </section>
        <section aria-label="选择目录" style={{ minWidth: 0, display: "grid", gap: 12 }}>
          <Select defaultValue="first" onValueChange={(value) => setAction(`select:${value}`)}><SelectTrigger aria-label="供应商目录"><SelectValue /></SelectTrigger><SelectContent><SelectGroup><SelectLabel>供应商</SelectLabel><SelectItem value="first">普通模型</SelectItem><SelectItem value="unavailable" disabled>已禁用模型</SelectItem><SelectSeparator /><SelectItem value="long">{long ? longLabel : "较长的模型名称"}</SelectItem>{Array.from({length: 30}, (_, index) => <SelectItem key={index} value={`model-${index}`}>模型 {index + 1}</SelectItem>)}</SelectGroup></SelectContent></Select>
          <Select disabled defaultValue="disabled"><SelectTrigger aria-label="禁用选择"><SelectValue /></SelectTrigger><SelectContent><SelectItem value="disabled">禁用选择</SelectItem></SelectContent></Select>
          <Select><SelectTrigger size="sm" aria-label="未配置选择"><SelectValue placeholder="尚未选择" /></SelectTrigger><SelectContent><SelectItem value="empty" disabled>暂无可用目标</SelectItem></SelectContent></Select>
          <Accordion type="single" collapsible><AccordionItem value="one"><AccordionTrigger>动态正文</AccordionTrigger><AccordionContent><Button variant="outline" onClick={() => setExpandedBody(!expandedBody)}>切换正文长度</Button>{Array.from({length: expandedBody ? 20 : 1}, (_, index) => <p key={index}>正文第 {index + 1} 行</p>)}<Button onClick={() => setAction("accordion:footer")}>正文末尾动作</Button></AccordionContent></AccordionItem><AccordionItem value="two"><AccordionTrigger>{long ? longLabel : "第二个折叠项"}</AccordionTrigger><AccordionContent>第二项正文</AccordionContent></AccordionItem><AccordionItem value="disabled" disabled><AccordionTrigger>禁用折叠项</AccordionTrigger><AccordionContent>不应该打开</AccordionContent></AccordionItem></Accordion>
        </section>
        <section aria-label="页签" style={{ minWidth: 0, display: "grid", gap: 16 }}>
          {(["horizontal", "vertical"] as const).map((orientation) => <Tabs key={orientation} orientation={orientation} defaultValue="one"><TabsList aria-label={`${orientation} 页签`} variant={orientation === "vertical" ? "line" : "default"}><TabsTrigger value="one">{long ? "长标题页面布局展示" : "概览"}</TabsTrigger><TabsTrigger value="disabled" disabled>禁用</TabsTrigger><TabsTrigger value="three">{long ? "另一项较长的中文页签标题" : "详情"}</TabsTrigger></TabsList><TabsContent value="one">{long ? longLabel : "第一项正文"}</TabsContent><TabsContent value="three"><Button onClick={() => setAction(`tabs:${orientation}`)}>页签动作 {orientation}</Button></TabsContent></Tabs>)}
        </section>
      </div>
    </main>
  </I18nProvider>;
}
