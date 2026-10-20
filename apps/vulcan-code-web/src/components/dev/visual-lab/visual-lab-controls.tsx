import { Sparkles } from "lucide-react"
import type { MouseEvent } from "react"

import { PageRow, PageSection } from "@/components/page"
import { Badge } from "@/ui-kit/primitives/badge"
import { Button, IconButton } from "@/ui-kit/primitives/button"
import { Checkbox } from "@/ui-kit/primitives/checkbox"
import { Field, FieldDescription, FieldError, FieldLabel } from "@/ui-kit/primitives/field"
import { Input } from "@/ui-kit/primitives/input"
import { RadioGroup, RadioGroupItem } from "@/ui-kit/primitives/radio-group"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select"
import { Spinner } from "@/ui-kit/primitives/spinner"
import { StatusPill } from "@/components/shared/status"
import { Switch } from "@/ui-kit/primitives/switch"
import { Textarea } from "@/ui-kit/primitives/textarea"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/ui-kit/primitives/tooltip"

import { actionLabel, type VisualLabDemoProps } from "./visual-lab-shared"

// VisualLabActions renders the approved button hierarchy, sizes, icon buttons and status indicators.
// VisualLabActions 渲染允许的按钮层级、尺寸、图标按钮与状态指示。
// Parameters: props supplies copy and the local action recorder. Returns page sections of action samples.
// 参数：props 提供文案与本地操作记录器。返回操作样本的页面分区。
export function VisualLabActions({ text, onAction }: VisualLabDemoProps) {
  // record forwards a demo click to the shell status line.
  // record 将演示点击转发到外壳状态行。
  const record = (event: MouseEvent<HTMLElement>) => onAction(actionLabel(event))
  return (
    <>
      <PageSection
        title={text("操作层级", "Action hierarchy")}
        description={text("每个页面只有一个主要操作；危险操作只用于不可撤销的决定。", "Each page has one primary action; destructive actions are reserved for irreversible decisions.")}
      >
        <div className="visual-lab-row">
          <Button onClick={record}>{text("主要操作", "Primary")}</Button>
          <Button onClick={record} variant="secondary">{text("次要操作", "Secondary")}</Button>
          <Button onClick={record} variant="outline">{text("描边操作", "Outline")}</Button>
          <Button onClick={record} variant="ghost">{text("弱操作", "Ghost")}</Button>
          <Button onClick={record} variant="destructive">{text("危险操作", "Danger")}</Button>
          <Button disabled>{text("禁用", "Disabled")}</Button>
        </div>
      </PageSection>
      <PageSection
        title={text("尺寸", "Sizes")}
        description={text("工具栏与表格行内使用 sm；页面主操作使用默认尺寸。", "Toolbars and table rows use sm; primary page actions use the default size.")}
      >
        <div className="visual-lab-row">
          <Button onClick={record} size="xs">XS</Button>
          <Button onClick={record} size="sm">SM</Button>
          <Button onClick={record} size="default">MD</Button>
          <Button onClick={record} size="lg">LG</Button>
        </div>
        <div className="visual-lab-row">
          <IconButton onClick={record} aria-label={text("超小图标按钮", "Extra-small icon button")} size="xs"><Sparkles /></IconButton>
          <IconButton onClick={record} aria-label={text("小图标按钮", "Small icon button")} size="sm"><Sparkles /></IconButton>
          <IconButton onClick={record} aria-label={text("中图标按钮", "Medium icon button")} size="md"><Sparkles /></IconButton>
          <IconButton onClick={record} aria-label={text("大图标按钮", "Large icon button")} size="lg"><Sparkles /></IconButton>
          <IconButton onClick={record} aria-label={text("危险图标按钮", "Destructive icon button")} variant="destructive"><Sparkles /></IconButton>
          <IconButton aria-label={text("禁用图标按钮", "Disabled icon button")} disabled><Sparkles /></IconButton>
          <Tooltip>
            <TooltipTrigger asChild>
              <IconButton onClick={record} aria-label={text("提示示例", "Tooltip example")}><Sparkles /></IconButton>
            </TooltipTrigger>
            <TooltipContent>{text("统一提示层", "Unified tooltip surface")}</TooltipContent>
          </Tooltip>
        </div>
      </PageSection>
      <PageSection
        title={text("状态与标记", "Status and badges")}
        description={text("状态胶囊表达运行状态；徽标只承载计数或分类。", "Status pills express runtime state; badges carry only counts or categories.")}
      >
        <div className="visual-lab-row">
          <StatusPill label={text("就绪", "Ready")} status="ready" />
          <StatusPill label={text("等待", "Pending")} status="pending" />
          <StatusPill label={text("警告", "Warning")} status="warning" />
          <StatusPill label={text("错误", "Error")} status="error" />
        </div>
        <div className="visual-lab-row">
          <Badge>{text("默认", "Default")}</Badge>
          <Badge variant="secondary">{text("次要", "Secondary")}</Badge>
          <Badge variant="outline">{text("描边", "Outline")}</Badge>
          <Badge variant="destructive">{text("危险", "Danger")}</Badge>
          <Spinner aria-label={text("加载演示", "Loading demo")} />
        </div>
      </PageSection>
    </>
  )
}

// VisualLabForms renders settings rows and standalone fields with one density and state language.
// VisualLabForms 以统一密度与状态语言渲染设置行与独立字段。
// Parameters: props supplies copy. Returns page sections of uncontrolled form samples.
// 参数：props 提供文案。返回非受控表单样本的页面分区。
export function VisualLabForms({ text }: VisualLabDemoProps) {
  return (
    <>
      <PageSection
        layout="rows"
        title={text("设置行", "Settings rows")}
        description={text("设置页统一使用标签在左、控件在右的行；窄宽度时自动换行。", "Settings pages use rows with the label on the left and the control on the right; they wrap at narrow widths.")}
      >
        <PageRow label={text("开关", "Switch")} description={text("即时生效的布尔选项。", "A boolean option that applies immediately.")}>
          <Switch aria-label={text("开关", "Switch")} defaultChecked />
        </PageRow>
        <PageRow htmlFor="visual-lab-row-select" label={text("选择密度", "Select density")} description={text("固定选项使用共享 Select。", "Fixed options use the shared Select.")}>
          <Select defaultValue="balanced">
            <SelectTrigger id="visual-lab-row-select" aria-label={text("选择密度", "Select density")}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="compact">{text("紧凑", "Compact")}</SelectItem>
              <SelectItem value="balanced">{text("均衡", "Balanced")}</SelectItem>
              <SelectItem value="comfortable">{text("宽松", "Comfortable")}</SelectItem>
            </SelectContent>
          </Select>
        </PageRow>
        <PageRow htmlFor="visual-lab-row-input" label={text("超时（秒）", "Timeout (seconds)")}>
          <Input id="visual-lab-row-input" defaultValue="30" density="compact" inputMode="numeric" />
        </PageRow>
        <PageRow stacked label={text("模式", "Mode")} description={text("选项较长时使用堆叠行。", "Use a stacked row when choices are long.")}>
          <RadioGroup aria-label={text("模式", "Mode")} defaultValue="standard">
            <label className="visual-lab-choice-line"><RadioGroupItem value="standard" />{text("标准模式", "Standard mode")}</label>
            <label className="visual-lab-choice-line"><RadioGroupItem value="focused" />{text("聚焦模式", "Focused mode")}</label>
          </RadioGroup>
        </PageRow>
      </PageSection>
      <PageSection
        title={text("独立字段", "Standalone fields")}
        description={text("弹窗与编辑器内使用标签在上的字段；错误信息紧贴控件。", "Dialogs and editors use fields with the label on top; errors sit directly under the control.")}
      >
        <div className="visual-lab-stack">
          <Field>
            <FieldLabel htmlFor="visual-lab-default-input">{text("普通输入", "Default input")}</FieldLabel>
            <Input defaultValue={text("可编辑内容", "Editable value")} id="visual-lab-default-input" />
            <FieldDescription>{text("标签、说明和控件使用统一间距。", "Label, description and control use shared spacing.")}</FieldDescription>
          </Field>
          <div className="visual-lab-row visual-lab-control-sizes">
            <Input aria-label={text("紧凑输入", "Compact input")} defaultValue={text("紧凑", "Compact")} density="compact" />
            <Input aria-label={text("默认输入", "Default input")} defaultValue={text("默认", "Default")} />
            <Input aria-label={text("大型输入", "Large input")} defaultValue={text("大型", "Large")} density="large" />
          </div>
          <Field data-invalid="true">
            <FieldLabel htmlFor="visual-lab-invalid-input">{text("错误状态", "Invalid state")}</FieldLabel>
            <Input aria-invalid="true" aria-describedby="visual-lab-input-error" defaultValue={text("无效内容", "Invalid value")} id="visual-lab-invalid-input" />
            <FieldError id="visual-lab-input-error">{text("这里显示统一的字段错误信息。", "A unified field error appears here.")}</FieldError>
          </Field>
          <Field>
            <FieldLabel htmlFor="visual-lab-textarea">{text("多行输入", "Multiline input")}</FieldLabel>
            <Textarea defaultValue={text("用于检查字体、边框、内边距和换行。", "Checks typography, borders, padding and wrapping.")} id="visual-lab-textarea" />
          </Field>
          <label className="visual-lab-choice-line"><Checkbox defaultChecked />{text("复选框", "Checkbox")}</label>
        </div>
      </PageSection>
    </>
  )
}
