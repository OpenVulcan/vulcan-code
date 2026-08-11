import { Palette, Sparkles } from "lucide-react"
import type { CSSProperties, ReactNode } from "react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { Checkbox } from "@/components/ui/checkbox"
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty"
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
                  <Button size="icon-sm" variant="ghost" aria-label={text("提示示例", "Tooltip example")}>
                    <Sparkles />
                  </Button>
                </TooltipTrigger>
                <TooltipContent>{text("统一提示层", "Unified tooltip surface")}</TooltipContent>
              </Tooltip>
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
            <label className="visual-lab-field">
              <span>{text("普通输入", "Default input")}</span>
              <Input defaultValue={text("可编辑内容", "Editable value")} />
            </label>
            <label className="visual-lab-field">
              <span>{text("多行输入", "Multiline input")}</span>
              <Textarea defaultValue={text("用于检查字体、边框、内边距和换行。", "Checks typography, borders, padding, and wrapping.")} />
            </label>
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
