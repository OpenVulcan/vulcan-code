import React from "react";
import { CircleAlert, RefreshCw } from "lucide-react";
import { I18nProvider } from "../../src/i18n";
import { Alert, AlertAction, AlertDescription, AlertTitle } from "../../src/components/ui/alert";
import { Badge } from "../../src/components/ui/badge";
import { Button } from "../../src/components/ui/button";
import { Card, CardAction, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from "../../src/components/ui/card";
import { Empty, EmptyContent, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "../../src/components/ui/empty";
import { Field, FieldContent, FieldDescription, FieldError, FieldGroup, FieldLabel, FieldLegend, FieldSeparator, FieldSet, FieldTitle } from "../../src/components/ui/field";
import { Input } from "../../src/components/ui/input";
import { Label } from "../../src/components/ui/label";
import { Progress } from "../../src/components/ui/progress";
import { ProgressRing } from "../../src/components/ui/progress-ring";
import { Separator } from "../../src/components/ui/separator";
import { Spinner } from "../../src/components/ui/spinner";
import { Surface } from "../../src/components/ui/surface";
import { Table, TableBody, TableCaption, TableCell, TableFooter, TableHead, TableHeader, TableRow } from "../../src/components/ui/table";
import { PanelTitle, StatusIcon, StatusPill } from "../../src/components/shared/status";
import { ProviderToolCapabilityIcon } from "../../src/components/shared/provider-tool-capability-icon";
import type { PublicAiProviderToolCapabilityKind } from "../../src/types";

// capabilities exercises every member of the production capability-to-icon contract.
// capabilities 覆盖生产能力图标契约的每个成员。
const capabilities: PublicAiProviderToolCapabilityKind[] = ["web_search", "web_extract", "image_understand", "image_generate", "image_edit", "video_understand", "video_generate", "video_edit", "video_extend", "audio_understand", "speech_synthesize", "speech_transcribe", "music_generate", "music_cover"];
// longIdentifier represents a provider or filesystem identifier, never a real host path.
// longIdentifier 表示供应商或文件系统标识，绝不是实际宿主路径。
const longIdentifier = "LongProviderIdentifier".repeat(12);

/** Mounts shared production primitives with explicit synthetic states and local-only actions.
 * 挂载共享生产基础组件，使用明确合成状态与仅本地动作。
 * No parameters; returns a responsive gallery without RPC, persistence, or model calls.
 * 无参数；返回响应式图集，不调用 RPC、持久化或模型。
 */
export function AuditSharedSurface() {
  // query selects only the documented fixture locale and long-content state.
  // query 仅选择已声明的夹具语言与长内容状态。
  const query = new URLSearchParams(window.location.search);
  // long switches realistic identifiers into compact surfaces without changing component CSS.
  // long 将实际形态的长标识放入紧凑表面，不改变组件 CSS。
  const long = query.get("content") === "long";
  // action records fixture interaction only and cannot change application data.
  // action 仅记录夹具交互，不能改变应用数据。
  const [action, setAction] = React.useState("尚未操作");
  // disabled demonstrates native fieldset propagation rather than copying disabled logic.
  // disabled 演示原生字段组禁用传递，不复刻禁用逻辑。
  const [disabled, setDisabled] = React.useState(false);
  return <I18nProvider initialLocale={query.get("locale") === "en-US" ? "en-US" : "zh-CN"}>
    <main style={{ padding: 16, height: "100dvh", overflow: "auto" }}>
      <p>共享组件合成验收 · 不保存、不调用模型</p>
      <p role="status">{action}</p>
      <div style={{ display: "grid", gap: 20, gridTemplateColumns: "repeat(auto-fit, minmax(min(340px, 100%), 1fr))", alignItems: "start" }}>
        <section aria-label="状态与提示" style={{ minWidth: 0, display: "grid", gap: 12 }}>
          <PanelTitle icon={<CircleAlert size={18} />} title="状态与提示" />
          <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
            {(["ready", "warning", "error", "pending"] as const).map((status) => <span key={status} style={{ display: "inline-flex", gap: 4, alignItems: "center" }}><StatusIcon status={status} /><StatusPill status={status} label={status} /></span>)}
          </div>
          <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
            {(["default", "secondary", "outline", "success", "warning", "destructive", "ghost", "link"] as const).map((variant) => <Badge key={variant} variant={variant}>{variant}</Badge>)}
            <Badge asChild variant="outline"><button onClick={() => setAction("已操作徽标")}>徽标动作</button></Badge>
          </div>
          {(["default", "warning", "destructive"] as const).map((variant) => <Alert key={variant} variant={variant}>
            <CircleAlert /><AlertTitle>{variant} 提示</AlertTitle><AlertDescription>{long ? longIdentifier : "简短说明，保留必要的处理动作。"}</AlertDescription>
            <AlertAction><Button aria-label={`${variant} 重新读取`} size="icon-sm" variant="ghost" onClick={() => setAction(`${variant} 已重读`)}><RefreshCw /></Button></AlertAction>
          </Alert>)}
        </section>
        <section aria-label="表单与分隔" style={{ minWidth: 0, display: "grid", gap: 12 }}>
          <Button variant="outline" onClick={() => setDisabled(!disabled)}>切换字段组禁用</Button>
          <FieldSet disabled={disabled}>
            <FieldLegend>关联字段</FieldLegend><FieldGroup>
              <Field><FieldLabel htmlFor="shared-name">名称</FieldLabel><Input id="shared-name" defaultValue="演示输入" /><FieldDescription>{long ? longIdentifier : "标签点击应聚焦输入。"}</FieldDescription></Field>
              <Field orientation="responsive"><FieldContent><FieldTitle>响应式说明</FieldTitle><FieldDescription>{long ? longIdentifier : "窄窗口应自然换行。"}</FieldDescription></FieldContent><Input aria-label="响应式输入" /></Field>
              <FieldSeparator>高级</FieldSeparator>
              <Field data-invalid="true"><FieldLabel htmlFor="shared-invalid">校验失败</FieldLabel><Input id="shared-invalid" aria-invalid="true" aria-describedby="shared-error" /><FieldError id="shared-error" errors={[{ message: "请填写模型名称" }, { message: "请填写模型名称" }, { message: "请配置账号" }, undefined]} /></Field>
            </FieldGroup>
          </FieldSet>
          <Label htmlFor="shared-standalone">独立标签</Label><Input id="shared-standalone" />
          <Separator decorative={false} />
          <div style={{ display: "flex", gap: 12, minHeight: 32 }}><span>左</span><Separator orientation="vertical" decorative={false} /><span>右</span></div>
        </section>
        <section aria-label="卡片与空状态" style={{ minWidth: 0, display: "grid", gap: 12 }}>
          <Card><CardHeader><CardTitle>{long ? longIdentifier : "标准卡片"}</CardTitle><CardDescription>次要信息</CardDescription><CardAction><Button size="sm" variant="outline" onClick={() => setAction("已刷新卡片")}>刷新</Button></CardAction></CardHeader><CardContent>{long ? longIdentifier : "主体内容"}</CardContent><CardFooter><Button onClick={() => setAction("已保存演示")}>保存演示</Button></CardFooter></Card>
          <Card size="sm" variant="selected"><CardHeader><CardTitle>紧凑选中卡片</CardTitle></CardHeader></Card>
          {(["default", "muted", "outlined", "selected"] as const).map((variant) => <Surface key={variant} variant={variant} style={{ padding: 12 }}>{variant}</Surface>)}
          <Surface asChild variant="interactive"><button style={{ padding: 12 }} onClick={() => setAction("已操作交互表面")}>交互表面</button></Surface>
          {(["muted", "outlined"] as const).map((variant) => <Empty key={variant} variant={variant}><EmptyHeader><EmptyMedia variant="icon"><CircleAlert /></EmptyMedia><EmptyTitle>没有匹配的条目</EmptyTitle><EmptyDescription>{long ? longIdentifier : "调整筛选后再查看。"}</EmptyDescription></EmptyHeader><EmptyContent><Button variant="outline" onClick={() => setAction("已重置演示筛选")}>重置筛选 {variant}</Button></EmptyContent></Empty>)}
        </section>
        <section aria-label="进度与图标" style={{ minWidth: 0, display: "grid", gap: 12 }}>
          {[null, 0, 32, 50].map((value) => <div key={String(value)}><span>{value === null ? "未知进度" : `${value}/50`}</span><Progress value={value} max={50} aria-label={`进度 ${String(value)}`} /></div>)}
          <div style={{ display: "flex", gap: 12 }}>{[null, 0, 0.5, 1].map((value) => <ProgressRing key={String(value)} value={value} size={24} aria-label={`圆环 ${String(value)}`} />)}<Spinner /><Spinner aria-label="读取模型目录" /></div>
          {capabilities.map((capability) => <div key={capability} style={{ display: "flex", gap: 8 }}><ProviderToolCapabilityIcon capability={capability} size={18} />{capability}</div>)}
        </section>
        <section aria-label="表格" style={{ minWidth: 0 }}>
          <Table><TableCaption>合成模型统计</TableCaption><TableHeader><TableRow><TableHead scope="col">模型</TableHead><TableHead scope="col">输入</TableHead><TableHead scope="col">输出</TableHead><TableHead scope="col">操作</TableHead></TableRow></TableHeader><TableBody>
            {["普通模型", long ? longIdentifier : "较长的模型名称", "空价格模型"].map((name, index) => <TableRow key={name} data-state={index === 0 ? "selected" : undefined}><TableCell>{name}</TableCell><TableCell>123,456</TableCell><TableCell>{index === 2 ? "-" : "42,123"}</TableCell><TableCell><Button size="sm" variant="outline" onClick={() => setAction(`已查看表格行 ${index}`)}>查看 {index + 1}</Button></TableCell></TableRow>)}
          </TableBody><TableFooter><TableRow><TableCell colSpan={4}>共 3 个模型</TableCell></TableRow></TableFooter></Table>
        </section>
      </div>
    </main>
  </I18nProvider>;
}
