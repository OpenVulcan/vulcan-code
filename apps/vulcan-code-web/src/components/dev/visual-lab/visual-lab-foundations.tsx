import { useState, type CSSProperties } from "react"
import { Play } from "lucide-react"

import { PageSection } from "@/components/page"
import { Button } from "@/ui-kit/primitives/button"

import type { VisualLabDemoProps } from "./visual-lab-shared"

// SwatchToken describes one kit color token shown as a swatch.
// SwatchToken 描述以色块展示的单个套件颜色令牌。
interface SwatchToken {
  // label is the product-facing role of the token.
  // label 是令牌面向产品的角色。
  label: string
  // token is the CSS custom property name.
  // token 是 CSS 自定义属性名称。
  token: string
}

// ElevationStep describes one rung of the shadow ladder.
// ElevationStep 描述阴影阶梯中的一级。
interface ElevationStep {
  // token is the CSS custom property holding the shadow; empty for the flat baseline.
  // token 是保存阴影的 CSS 自定义属性；平面基线为空。
  token: string
  // zh / en name where the level is used.
  // zh / en 说明该层级的用途。
  zh: string
  en: string
}

// SURFACE_TOKENS is the ground-to-overlay surface ladder plus the interaction fills and lines drawn on it.
// SURFACE_TOKENS 是从底色到浮层的表面阶梯，以及其上的交互填充与线条。
const SURFACE_TOKENS: SwatchToken[] = [
  { label: "Ground", token: "--vk-ground" },
  { label: "Ground 2", token: "--vk-ground-2" },
  { label: "Panel", token: "--vk-panel" },
  { label: "Sunken", token: "--vk-sunken" },
  { label: "Raised", token: "--vk-raised" },
  { label: "Hover", token: "--vk-hover" },
  { label: "Select", token: "--vk-select" },
  { label: "Line", token: "--vk-line" },
  { label: "Line strong", token: "--vk-line-strong" },
]

// INK_TOKENS are the text tiers and the signal colors; ember is reserved for AI activity.
// INK_TOKENS 是文字层级与信号色；余烬色只用于表示 AI 正在工作。
const INK_TOKENS: SwatchToken[] = [
  { label: "Ink", token: "--vk-ink" },
  { label: "Ink 2", token: "--vk-ink-2" },
  { label: "Ink 3", token: "--vk-ink-3" },
  { label: "Signal", token: "--vk-signal" },
  { label: "Ember", token: "--vk-ember" },
  { label: "OK", token: "--vk-ok" },
  { label: "Warn", token: "--vk-warn" },
  { label: "Bad", token: "--vk-bad" },
  { label: "Info", token: "--vk-info" },
]

// ELEVATION_STEPS lists the approved depth levels from page to overlay.
// ELEVATION_STEPS 列出从页面到浮层的允许深度层级。
const ELEVATION_STEPS: ElevationStep[] = [
  { token: "", zh: "页面 · 平面", en: "Page · flat" },
  { token: "--vk-shadow-1", zh: "卡片 · 菜单", en: "Cards · menus" },
  { token: "--vk-shadow-2", zh: "对话框 · 浮窗", en: "Dialogs · floating windows" },
]

// DURATION_STEPS lists the kit duration tokens with their millisecond values for the proportional bars.
// DURATION_STEPS 列出套件时长令牌及其毫秒值，用于按比例绘制条形。
const DURATION_STEPS: Array<[string, number]> = [
  ["--vk-t-fast", 120],
  ["--vk-t-med", 180],
]

// LONGEST_DURATION_MS scales the duration bars; derived from the scale so the longest bar is always full width.
// LONGEST_DURATION_MS 用于缩放时长条；由刻度派生，保证最长条始终占满宽度。
const LONGEST_DURATION_MS = Math.max(...DURATION_STEPS.map(([, ms]) => ms))

// TYPE_STEPS lists the type scale from captions to display headings.
// TYPE_STEPS 列出从说明文字到展示标题的字号阶梯。
const TYPE_STEPS = ["--vk-fs-2xs", "--vk-fs-xs", "--vk-fs-sm", "--vk-fs-md", "--vk-fs-read", "--vk-fs-lg", "--vk-fs-xl", "--vk-fs-2xl"]

// EASE_POINTS are the control points of --vk-ease, drawn as the single motion curve.
// EASE_POINTS 是 --vk-ease 的控制点，作为唯一动效曲线绘制。
const EASE_POINTS: [number, number, number, number] = [0.2, 0.7, 0.2, 1]

// curvePath draws a cubic-bezier easing as an SVG path inside a 48×48 box, y pointing up.
// curvePath 在 48×48 区域内把 cubic-bezier 缓动绘制为 SVG 路径，y 轴向上。
// Parameters: points are the four control-point values. Returns the SVG path data.
// 参数：points 为四个控制点数值。返回 SVG 路径数据。
function curvePath(points: [number, number, number, number]): string {
  const [x1, y1, x2, y2] = points
  const size = 48
  return `M0 ${size} C ${x1 * size} ${size - y1 * size}, ${x2 * size} ${size - y2 * size}, ${size} 0`
}

// SwatchGrid renders one labelled swatch per token.
// SwatchGrid 为每个令牌渲染一个带标签的色块。
// Parameters: tokens are the swatches to show. Returns the grid.
// 参数：tokens 为要展示的色块。返回网格。
function SwatchGrid({ tokens }: { tokens: SwatchToken[] }) {
  return (
    <div className="visual-lab-token-grid">
      {tokens.map((item) => (
        <article key={item.token} className="visual-lab-token">
          <div className="visual-lab-token-swatch" style={{ "--visual-lab-token-color": `var(${item.token})` } as CSSProperties} />
          <div className="visual-lab-token-copy">
            <strong>{item.label}</strong>
            <code>{item.token}</code>
          </div>
        </article>
      ))}
    </div>
  )
}

// VisualLabFoundations renders the kit surface, ink, elevation, motion and type tokens as page sections.
// VisualLabFoundations 以页面分区展示套件的表面、文字、层级、动效与字号令牌。
// Parameters: props provides the locale-bound copy selector. Returns local-only samples; nothing here touches product state.
// 参数：props 提供已绑定语言的文案选择器。返回仅作用于本页的样本，不触及产品状态。
export function VisualLabFoundations({ text }: VisualLabDemoProps) {
  // run toggles the lane dot between both ends; replay increments to remount the stagger sample.
  // run 让轨道圆点在两端之间切换；replay 递增以重新挂载错峰样本。
  const [run, setRun] = useState(false)
  const [replay, setReplay] = useState(0)

  return (
    <>
      <PageSection
        title={text("表面与线条", "Surfaces and lines")}
        description={text("从底色到浮层的表面阶梯；组件只引用这些令牌，主题与皮肤只改令牌值。", "The surface ladder from ground to overlay; components reference these tokens only, and themes and skins change only their values.")}
      >
        <SwatchGrid tokens={SURFACE_TOKENS} />
      </PageSection>

      <PageSection
        title={text("文字与信号", "Ink and signals")}
        description={text("三级文字颜色加状态色；余烬色只表示 AI 正在工作，不作装饰。", "Three ink tiers plus status colors; ember only means the AI is working and is never decoration.")}
      >
        <SwatchGrid tokens={INK_TOKENS} />
      </PageSection>

      <PageSection
        title={text("层级", "Elevation")}
        description={text("只有两级阴影：贴近页面的卡片与菜单，以及浮在其上的对话框与浮窗。", "Only two shadow steps: cards and menus near the page, and dialogs and floating windows above them.")}
      >
        <div className="visual-lang-elevation">
          {ELEVATION_STEPS.map((step, index) => (
            <article key={step.zh} className="visual-lang-elevation-tile" style={{ boxShadow: step.token ? `var(${step.token})` : "none" }}>
              <span className="visual-lang-elevation-level">{index}</span>
              <strong>{text(step.zh, step.en)}</strong>
              <code>{step.token || "none"}</code>
            </article>
          ))}
        </div>
      </PageSection>

      <PageSection
        title={text("动效", "Motion")}
        description={text("所有变化共用一条缓出曲线。演示放慢了 4 倍。", "Every change shares one ease-out curve. The demo is slowed 4×.")}
        actions={(
          <Button type="button" variant="outline" size="sm" onClick={() => { setRun((value) => !value); setReplay((value) => value + 1) }}>
            <Play data-icon="inline-start" />
            {text("播放", "Play")}
          </Button>
        )}
      >
        <div className="visual-lang-curves">
          <div className="visual-lang-curve">
            <svg className="visual-lang-curve-graph" viewBox="-4 -12 56 72" aria-hidden="true">
              <path className="visual-lang-curve-axis" d="M0 0 V48 H48" />
              <path className="visual-lang-curve-line" d={curvePath(EASE_POINTS)} />
            </svg>
            <div className="visual-lang-curve-copy">
              <strong>{text("进入与响应：快速起步、柔和落定", "Entering and responding: quick start, soft landing")}</strong>
              <code>--vk-ease · --vk-t-med</code>
              <div className="visual-lang-lane">
                <span
                  className="visual-lang-lane-dot"
                  data-run={run ? "true" : undefined}
                  style={{ transitionTimingFunction: "var(--vk-ease)", transitionDuration: "calc(var(--vk-t-med) * 4)" }}
                />
              </div>
            </div>
          </div>
        </div>
      </PageSection>

      <div className="visual-lang-split">
        <PageSection title={text("时长刻度", "Duration scale")} description={text("小元素与悬停用快档，展开与进入用中档。", "Small elements and hovers use the fast step; expanding and entering use the medium step.")}>
          <div className="visual-lang-durations">
            {DURATION_STEPS.map(([token, ms]) => (
              <div key={token} className="visual-lang-duration">
                <code>{token.replace("--vk-t-", "")}</code>
                <span className="visual-lang-duration-bar" style={{ width: `${(ms / LONGEST_DURATION_MS) * 100}%` }} />
                <span className="visual-lang-duration-value">{ms}ms</span>
              </div>
            ))}
          </div>
        </PageSection>
        <PageSection title={text("错峰进入", "Staggered entrance")} description={text("页面分区依次出现，第 5 个之后不再递增。", "Page sections arrive one after another, capped after the fifth.")}>
          <div key={replay} className="visual-lang-stagger">
            {[0, 1, 2, 3, 4].map((index) => <span key={index} style={{ animationDelay: `calc(var(--vk-stagger) * ${index} * 4)` }} />)}
          </div>
        </PageSection>
      </div>

      <PageSection title={text("字号", "Type scale")} description={text("界面正文为 md，会话阅读正文为 read；其余档位用于说明、标题与展示数字。", "UI body text is md and conversation reading text is read; the other steps serve captions, headings and display figures.")}>
        <div className="visual-lang-durations">
          {TYPE_STEPS.map((token) => (
            <div key={token} className="visual-lang-duration">
              <code>{token.replace("--vk-fs-", "")}</code>
              <span style={{ fontSize: `var(${token})` }}>{text("火神代码 Vulcan", "Vulcan Code 火神")}</span>
            </div>
          ))}
        </div>
      </PageSection>
    </>
  )
}
