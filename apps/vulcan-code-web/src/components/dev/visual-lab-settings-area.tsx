import { Palette, Sparkles } from "lucide-react"
import { useState, type CSSProperties, type ReactNode } from "react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { Badge } from "@/components/ui/badge"
import { Button, IconButton } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { Checkbox } from "@/components/ui/checkbox"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty"
import { Field, FieldDescription, FieldError, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Progress } from "@/components/ui/progress"
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetFooter,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet"
import { Spinner } from "@/components/ui/spinner"
import { Switch } from "@/components/ui/switch"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { Textarea } from "@/components/ui/textarea"
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip"
import { useI18n } from "@/i18n"

// VisualLabSectionProps describes one independently reviewable component family.
// VisualLabSectionProps 描述一个可独立验收的组件分组。
interface VisualLabSectionProps {
  // Visible examples rendered inside the section.
  // 分组内部展示的可见示例。
  children: ReactNode
  // Short explanation of the component family's responsibility.
  // 对组件分组职责的简短说明。
  description: string
  // Visible section title.
  // 分组显示标题。
  title: string
}

// VisualToken describes one semantic foundation token shown in the lab.
// VisualToken 描述视觉实验室展示的单个语义基础变量。
interface VisualToken {
  // CSS color expression used by the preview swatch.
  // 预览色块使用的 CSS 颜色表达式。
  color: string
  // Product-facing semantic token label.
  // 面向产品语义的变量名称。
  label: string
  // CSS custom property name.
  // CSS 自定义属性名称。
  token: string
}

// VisualTokenSwatchProps identifies the semantic token rendered by one swatch.
// VisualTokenSwatchProps 标识单个色块所呈现的语义变量。
interface VisualTokenSwatchProps {
  // Semantic token rendered by this swatch.
  // 当前色块呈现的语义变量。
  token: VisualToken
}

// VISUAL_TOKENS is the first stable semantic surface palette exposed for review.
// VISUAL_TOKENS 是首批用于验收的稳定语义表面色板。
const VISUAL_TOKENS: VisualToken[] = [
  { label: "Page", token: "--vcn-surface-page", color: "var(--vcn-surface-page)" },
  { label: "Panel", token: "--vcn-surface-panel", color: "var(--vcn-surface-panel)" },
  { label: "Subtle", token: "--vcn-surface-subtle", color: "var(--vcn-surface-subtle)" },
  { label: "Floating", token: "--vcn-surface-floating", color: "var(--vcn-surface-floating)" },
  { label: "Toolbar", token: "--vcn-surface-toolbar", color: "var(--vcn-surface-toolbar)" },
  { label: "Border", token: "--vcn-border-default", color: "var(--vcn-border-default)" },
]

// visualLabText selects compact lab copy for the active locale.
// visualLabText 根据当前语言选择紧凑的实验室文案。
function visualLabText(locale: string, zh: string, en: string): string {
  return locale === "zh-CN" ? zh : en
}

// VisualLabSection renders one consistent review surface for a component family.
// VisualLabSection 为一个组件分组渲染统一的验收表面。
function VisualLabSection(props: VisualLabSectionProps) {
  return (
    <section className="visual-lab-section">
      <header className="visual-lab-section-heading">
        <h3>{props.title}</h3>
        <p>{props.description}</p>
      </header>
      {props.children}
    </section>
  )
}

// VisualTokenSwatch renders one semantic token and its resolved product role.
// VisualTokenSwatch 渲染一个语义变量及其对应的产品角色。
function VisualTokenSwatch(props: VisualTokenSwatchProps) {
  return (
    <article className="visual-lab-token">
      <div
        className="visual-lab-token-swatch"
        style={{ "--visual-lab-token-color": props.token.color } as CSSProperties}
      />
      <div className="visual-lab-token-copy">
        <strong>{props.token.label}</strong>
        <code>{props.token.token}</code>
      </div>
    </article>
  )
}

// VisualLabSettingsArea renders the first visible acceptance surface for the unified UI system.
// VisualLabSettingsArea 渲染统一 UI 体系首个可见的验收页面。
export function VisualLabSettingsArea() {
  // locale selects bilingual preview copy without introducing product state.
  // locale 仅用于选择双语预览文案，不引入产品状态。
  const { locale } = useI18n()
  // text binds the current locale to the compact copy selector.
  // text 将当前语言绑定到紧凑文案选择器。
  const text = (zh: string, en: string) => visualLabText(locale, zh, en)
  // dialogPreview identifies the modal surface currently opened from the fixed acceptance section.
  // dialogPreview 标识当前从固定验收区域打开的模态表面。
  const [dialogPreview, setDialogPreview] = useState<"dialog" | "alert" | "sheet" | null>(null)

  return (
    <div className="visual-lab">
      <header className="visual-lab-intro">
        <Badge variant="outline">
          <Palette />
          {text("统一视觉体系", "Unified visual system")}
        </Badge>
        <h2>{text("视觉实验室", "Visual Lab")}</h2>
        <p>
          {text(
            "这里只展示真实组件和统一语义变量。后续每次可见调整都在这里提供固定对照，不再要求验收没有发生的变化。",
            "This surface renders real components and semantic tokens. Every visible adjustment will provide a stable comparison here.",
          )}
        </p>
      </header>

      <div className="visual-lab-grid">
        <VisualLabSection
          description={text("页面、面板、浮层、弱背景和工具栏的基础层级。", "Base hierarchy for pages, panels, floating surfaces, subtle regions, and toolbars.")}
          title={text("语义表面", "Semantic surfaces")}
        >
          <div className="visual-lab-token-grid">
            {VISUAL_TOKENS.map((token) => <VisualTokenSwatch key={token.token} token={token} />)}
          </div>
        </VisualLabSection>

        <VisualLabSection
          description={text("普通操作只选择明确等级和尺寸，不在业务页面重新设计。", "Ordinary actions select an approved hierarchy and size without page-owned skinning.")}
          title={text("按钮与状态", "Actions and status")}
        >
          <div className="visual-lab-stack">
            <div className="visual-lab-row">
              <Button>{text("主要操作", "Primary")}</Button>
              <Button variant="secondary">{text("次要操作", "Secondary")}</Button>
              <Button variant="outline">{text("描边操作", "Outline")}</Button>
              <Button variant="ghost">{text("弱操作", "Ghost")}</Button>
              <Button variant="destructive">{text("危险操作", "Danger")}</Button>
            </div>
            <div className="visual-lab-row">
              <Button size="xs">XS</Button>
              <Button size="sm">SM</Button>
              <Button size="default">MD</Button>
              <Button size="lg">LG</Button>
              <Button disabled>{text("禁用", "Disabled")}</Button>
              <Tooltip>
                <TooltipTrigger asChild>
                  <IconButton aria-label={text("提示示例", "Tooltip example")}>
                    <Sparkles />
                  </IconButton>
                </TooltipTrigger>
                <TooltipContent>{text("统一提示层", "Unified tooltip surface")}</TooltipContent>
              </Tooltip>
            </div>
            <div className="visual-lab-row">
              <IconButton aria-label={text("超小图标按钮", "Extra-small icon button")} size="xs"><Sparkles /></IconButton>
              <IconButton aria-label={text("小图标按钮", "Small icon button")} size="sm"><Sparkles /></IconButton>
              <IconButton aria-label={text("中图标按钮", "Medium icon button")} size="md"><Sparkles /></IconButton>
              <IconButton aria-label={text("大图标按钮", "Large icon button")} size="lg"><Sparkles /></IconButton>
              <IconButton aria-label={text("危险图标按钮", "Destructive icon button")} variant="destructive"><Sparkles /></IconButton>
              <IconButton aria-label={text("禁用图标按钮", "Disabled icon button")} disabled><Sparkles /></IconButton>
            </div>
            <div className="visual-lab-row">
              <Badge>{text("默认", "Default")}</Badge>
              <Badge variant="secondary">{text("次要", "Secondary")}</Badge>
              <Badge variant="outline">{text("描边", "Outline")}</Badge>
              <Badge variant="destructive">{text("危险", "Danger")}</Badge>
              <Spinner />
            </div>
          </div>
        </VisualLabSection>

        <VisualLabSection
          description={text("输入、选择和布尔控件使用同一密度与状态语言。", "Inputs, selections, and boolean controls share one density and state language.")}
          title={text("表单控件", "Form controls")}
        >
          <div className="visual-lab-stack">
            <Field>
              <FieldLabel htmlFor="visual-lab-default-input">{text("普通输入", "Default input")}</FieldLabel>
              <Input defaultValue={text("可编辑内容", "Editable value")} id="visual-lab-default-input" />
              <FieldDescription>{text("标签、说明和控件使用统一间距。", "Label, description, and control use shared spacing.")}</FieldDescription>
            </Field>
            <div className="visual-lab-row visual-lab-control-sizes">
              <Input aria-label={text("紧凑输入", "Compact input")} defaultValue={text("紧凑", "Compact")} density="compact" />
              <Input aria-label={text("默认输入", "Default input")} defaultValue={text("默认", "Default")} />
              <Input aria-label={text("大型输入", "Large input")} defaultValue={text("大型", "Large")} density="large" />
            </div>
            <Field data-invalid="true">
              <FieldLabel htmlFor="visual-lab-invalid-input">{text("错误状态", "Invalid state")}</FieldLabel>
              <Input aria-invalid="true" defaultValue={text("无效内容", "Invalid value")} id="visual-lab-invalid-input" />
              <FieldError>{text("这里显示统一的字段错误信息。", "A unified field error appears here.")}</FieldError>
            </Field>
            <Field>
              <FieldLabel htmlFor="visual-lab-textarea">{text("多行输入", "Multiline input")}</FieldLabel>
              <Textarea defaultValue={text("用于检查字体、边框、内边距和换行。", "Checks typography, borders, padding, and wrapping.")} id="visual-lab-textarea" />
            </Field>
            <Select defaultValue="balanced">
              <SelectTrigger>
                <SelectValue placeholder={text("选择密度", "Select density")} />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="compact">{text("紧凑", "Compact")}</SelectItem>
                <SelectItem value="balanced">{text("均衡", "Balanced")}</SelectItem>
                <SelectItem value="comfortable">{text("宽松", "Comfortable")}</SelectItem>
              </SelectContent>
            </Select>
            <div className="visual-lab-row">
              <label className="visual-lab-choice-line">
                <Checkbox defaultChecked />
                {text("复选框", "Checkbox")}
              </label>
              <label className="visual-lab-choice-line">
                <Switch defaultChecked />
                {text("开关", "Switch")}
              </label>
            </div>
            <RadioGroup defaultValue="standard">
              <label className="visual-lab-choice-line">
                <RadioGroupItem value="standard" />
                {text("标准模式", "Standard mode")}
              </label>
              <label className="visual-lab-choice-line">
                <RadioGroupItem value="focused" />
                {text("聚焦模式", "Focused mode")}
              </label>
            </RadioGroup>
          </div>
        </VisualLabSection>

        <VisualLabSection
          description={text("普通内容表面统一使用相同的边框、圆角、间距和状态。", "Ordinary content surfaces share borders, radii, spacing, and status treatments.")}
          title={text("内容表面", "Content surfaces")}
        >
          <div className="visual-lab-card-grid">
            <Card>
              <CardHeader>
                <CardTitle>{text("标准卡片", "Standard card")}</CardTitle>
                <CardDescription>{text("普通信息层级", "Ordinary information hierarchy")}</CardDescription>
              </CardHeader>
              <CardContent className="visual-lab-caption">
                {text("业务组件负责内容，基础表面由统一体系提供。", "Product components own content while the unified system owns the base surface.")}
              </CardContent>
            </Card>
            <Card size="sm">
              <CardHeader>
                <CardTitle>{text("紧凑卡片", "Compact card")}</CardTitle>
                <CardDescription>{text("用于密集设置区域", "For dense settings surfaces")}</CardDescription>
              </CardHeader>
            </Card>
          </div>
          <Alert>
            <AlertTitle>{text("普通提示", "Informational alert")}</AlertTitle>
            <AlertDescription>{text("用于确认边框、正文和图标层级。", "Checks border, body copy, and icon hierarchy.")}</AlertDescription>
          </Alert>
          <Alert variant="destructive">
            <AlertTitle>{text("危险提示", "Destructive alert")}</AlertTitle>
            <AlertDescription>{text("危险语义必须保持清晰但不过度抢眼。", "Danger must remain clear without overwhelming the interface.")}</AlertDescription>
          </Alert>
        </VisualLabSection>

        <VisualLabSection
          description={text("普通弹窗、确认弹窗和侧边抽屉使用统一表面、尺寸与操作区。", "Dialogs, confirmations, and sheets share surface, sizing, and action-area contracts.")}
          title={text("模态表面", "Modal surfaces")}
        >
          <div className="visual-lab-row">
            <Button onClick={() => setDialogPreview("dialog")} variant="outline">{text("普通弹窗", "Dialog")}</Button>
            <Button onClick={() => setDialogPreview("alert")} variant="outline">{text("确认弹窗", "Confirmation")}</Button>
            <Button onClick={() => setDialogPreview("sheet")} variant="outline">{text("侧边抽屉", "Sheet")}</Button>
          </div>
          <Dialog open={dialogPreview === "dialog"} onOpenChange={(open) => setDialogPreview(open ? "dialog" : null)}>
            <DialogContent size="md">
              <DialogHeader>
                <DialogTitle>{text("标准表单弹窗", "Standard form dialog")}</DialogTitle>
                <DialogDescription>{text("检查标题、说明、内容间距、关闭按钮和底部操作区。", "Review title, description, spacing, close action, and footer.")}</DialogDescription>
              </DialogHeader>
              <Field>
                <FieldLabel htmlFor="visual-lab-dialog-input">{text("示例字段", "Example field")}</FieldLabel>
                <Input defaultValue={text("示例内容", "Example value")} id="visual-lab-dialog-input" />
              </Field>
              <DialogFooter>
                <Button onClick={() => setDialogPreview(null)} variant="outline">{text("取消", "Cancel")}</Button>
                <Button onClick={() => setDialogPreview(null)}>{text("保存", "Save")}</Button>
              </DialogFooter>
            </DialogContent>
          </Dialog>
          <AlertDialog open={dialogPreview === "alert"} onOpenChange={(open) => setDialogPreview(open ? "alert" : null)}>
            <AlertDialogContent>
              <AlertDialogHeader>
                <AlertDialogTitle>{text("确认当前操作？", "Confirm this action?")}</AlertDialogTitle>
                <AlertDialogDescription>{text("确认弹窗只承载需要明确决定的操作。", "Confirmations are reserved for explicit decisions.")}</AlertDialogDescription>
              </AlertDialogHeader>
              <AlertDialogFooter>
                <AlertDialogCancel>{text("取消", "Cancel")}</AlertDialogCancel>
                <AlertDialogAction>{text("确认", "Confirm")}</AlertDialogAction>
              </AlertDialogFooter>
            </AlertDialogContent>
          </AlertDialog>
          <Sheet open={dialogPreview === "sheet"} onOpenChange={(open) => setDialogPreview(open ? "sheet" : null)}>
            <SheetContent size="md">
              <SheetHeader>
                <SheetTitle>{text("标准侧边抽屉", "Standard side sheet")}</SheetTitle>
                <SheetDescription>{text("用于需要保留主界面上下文的辅助编辑。", "Used for auxiliary editing that preserves page context.")}</SheetDescription>
              </SheetHeader>
              <div className="px-4 text-sm text-muted-foreground">{text("抽屉内容区域", "Sheet content area")}</div>
              <SheetFooter>
                <Button onClick={() => setDialogPreview(null)}>{text("完成", "Done")}</Button>
              </SheetFooter>
            </SheetContent>
          </Sheet>
        </VisualLabSection>

        <VisualLabSection
          description={text("导航模式和进度状态用于后续统一前的固定对照。", "Navigation and progress states provide a stable reference for later unification.")}
          title={text("导航与反馈", "Navigation and feedback")}
        >
          <Tabs defaultValue="overview">
            <TabsList>
              <TabsTrigger value="overview">{text("概览", "Overview")}</TabsTrigger>
              <TabsTrigger value="details">{text("详情", "Details")}</TabsTrigger>
            </TabsList>
            <TabsContent value="overview">
              <div className="visual-lab-progress">
                <span className="visual-lab-caption">{text("处理中 64%", "Processing 64%")}</span>
                <Progress value={64} />
              </div>
            </TabsContent>
            <TabsContent value="details">
              <p className="visual-lab-caption">{text("详情页签内容。", "Details tab content.")}</p>
            </TabsContent>
          </Tabs>
          <Empty>
            <EmptyHeader>
              <EmptyMedia variant="icon"><Palette /></EmptyMedia>
              <EmptyTitle>{text("空状态标题", "Empty state title")}</EmptyTitle>
              <EmptyDescription>{text("空状态也必须来自统一视觉体系。", "Empty states also belong to the unified visual system.")}</EmptyDescription>
            </EmptyHeader>
          </Empty>
        </VisualLabSection>
      </div>
    </div>
  )
}
