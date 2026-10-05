import { useState, type CSSProperties } from "react"
import { Play } from "lucide-react"

import { PageSection } from "@/components/page"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"

import type { VisualLabDemoProps } from "./visual-lab-shared"

// SurfaceToken describes one semantic foundation token shown as a swatch.
// SurfaceToken 描述以色块展示的单个语义基础变量。
interface SurfaceToken {
  // label is the product-facing role of the token.
  // label 是变量面向产品的角色。
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

// MotionCurve describes one easing token paired with the duration it is normally used with.
// MotionCurve 描述一个缓动变量及其通常搭配的时长。
interface MotionCurve {
  // token is the easing custom property.
  // token 是缓动自定义属性。
  token: string
  // duration is the paired duration custom property.
  // duration 是搭配的时长自定义属性。
  duration: string
  // points are the cubic-bezier control points, used to draw the curve.
  // points 是 cubic-bezier 控制点，用于绘制曲线。
  points: [number, number, number, number]
  // zh / en explain when to use the curve.
  // zh / en 说明曲线的使用场景。
  zh: string
  en: string
}

// SURFACE_TOKENS is the stable semantic surface palette exposed for review.
// SURFACE_TOKENS 是用于验收的稳定语义表面色板。
const SURFACE_TOKENS: SurfaceToken[] = [
  { label: "Page", token: "--vcn-surface-page" },
  { label: "Panel", token: "--vcn-surface-panel" },
  { label: "Subtle", token: "--vcn-surface-subtle" },
  { label: "Floating", token: "--vcn-surface-floating" },
  { label: "Toolbar", token: "--vcn-surface-toolbar" },
  { label: "Border", token: "--vcn-border-default" },
]

// ELEVATION_STEPS lists the approved depth levels from page to modal.
// ELEVATION_STEPS 列出从页面到模态的允许深度层级。
const ELEVATION_STEPS: ElevationStep[] = [
  { token: "", zh: "页面 · 平面", en: "Page · flat" },
  { token: "--vcn-shadow-raised", zh: "卡片 · 静置", en: "Card · resting" },
  { token: "--vcn-shadow-hover", zh: "悬停 · 输入框", en: "Hover · composer" },
  { token: "--vcn-shadow-floating", zh: "菜单 · 提示", en: "Menus · toasts" },
  { token: "--vcn-shadow-modal", zh: "对话框 · 抽屉", en: "Dialogs · sheets" },
]

// MOTION_CURVES lists every easing token with its paired duration.
// MOTION_CURVES 列出每个缓动变量及其搭配时长。
const MOTION_CURVES: MotionCurve[] = [
  { token: "--vcn-ease-out", duration: "--vcn-duration-base", points: [0.22, 1, 0.36, 1], zh: "进入与响应：快速起步、柔和落定", en: "Entering and responding: quick start, soft landing" },
  { token: "--vcn-ease-spring", duration: "--vcn-duration-base", points: [0.34, 1.36, 0.64, 1], zh: "开关与勾选：轻微回弹", en: "Toggles and checks: slight overshoot" },
  { token: "--vcn-ease-in-out", duration: "--vcn-duration-slow", points: [0.65, 0, 0.35, 1], zh: "展开收起：对称的高度变化", en: "Expand and collapse: symmetric height change" },
  { token: "--vcn-ease-in", duration: "--vcn-duration-fast", points: [0.4, 0, 1, 1], zh: "退出：加速离开，不拖泥带水", en: "Exiting: accelerate away without lingering" },
]

// DURATION_STEPS lists the duration scale in milliseconds for the proportional bars.
// DURATION_STEPS 以毫秒列出时长刻度，用于按比例绘制条形。
const DURATION_STEPS: Array<[string, number]> = [
  ["--vcn-duration-instant", 80],
  ["--vcn-duration-fast", 120],
  ["--vcn-duration-base", 180],
  ["--vcn-duration-slow", 240],
  ["--vcn-duration-slower", 320],
]

// LONGEST_DURATION_MS scales the duration bars; derived from the scale so the longest bar is always full width.
// LONGEST_DURATION_MS 用于缩放时长条；由刻度派生，保证最长条始终占满宽度。
const LONGEST_DURATION_MS = Math.max(...DURATION_STEPS.map(([, ms]) => ms))

// RADIUS_STEPS lists the corner radius tokens used from small controls to the composer.
// RADIUS_STEPS 列出从小控件到输入框使用的圆角变量。
const RADIUS_STEPS: Array<[string, string]> = [
  ["--vcn-radius-sm", "var(--vcn-radius-sm)"],
  ["--vcn-radius-md", "var(--vcn-radius-md)"],
  ["--vcn-radius-lg", "var(--vcn-radius-lg)"],
  ["--vcn-radius-xl", "var(--vcn-radius-xl)"],
]

// curvePath draws a cubic-bezier easing as an SVG path inside a 48×48 box, y pointing up.
// curvePath 在 48×48 区域内把 cubic-bezier 缓动绘制为 SVG 路径，y 轴向上。
// Parameters: points are the four control-point values. Returns the SVG path data.
// 参数：points 为四个控制点数值。返回 SVG 路径数据。
function curvePath(points: MotionCurve["points"]): string {
  const [x1, y1, x2, y2] = points
  const size = 48
  return `M0 ${size} C ${x1 * size} ${size - y1 * size}, ${x2 * size} ${size - y2 * size}, ${size} 0`
}

// VisualLabFoundations renders surface, elevation, motion, material and radius tokens as page sections.
// VisualLabFoundations 以页面分区展示表面、层级、动效、材质与圆角变量。
// Parameters: props provides the locale-bound copy selector. Returns local-only samples; nothing here touches product state.
// 参数：props 提供已绑定语言的文案选择器。返回仅作用于本页的样本，不触及产品状态。
export function VisualLabFoundations({ text }: VisualLabDemoProps) {
  // run toggles the lane dots between both ends; replay increments to remount the stagger sample.
  // run 让轨道圆点在两端之间切换；replay 递增以重新挂载错峰样本。
  const [run, setRun] = useState(false)
  const [replay, setReplay] = useState(0)
  const [switchOn, setSwitchOn] = useState(true)
  const [checked, setChecked] = useState(true)

  return (
    <>
      <PageSection
        title={text("语义表面", "Semantic surfaces")}
        description={text("页面、面板、浮层、弱背景和工具栏的基础层级；组件只引用这些变量。", "Base hierarchy for pages, panels, floating surfaces, subtle regions and toolbars; components reference these tokens only.")}
      >
        <div className="visual-lab-token-grid">
          {SURFACE_TOKENS.map((item) => (
            <article key={item.token} className="visual-lab-token">
              <div className="visual-lab-token-swatch" style={{ "--visual-lab-token-color": `var(${item.token})` } as CSSProperties} />
              <div className="visual-lab-token-copy">
                <strong>{item.label}</strong>
                <code>{item.token}</code>
              </div>
            </article>
          ))}
        </div>
      </PageSection>

      <PageSection
        title={text("层级", "Elevation")}
        description={text("越靠近用户的层，阴影越深、越柔。深色模式额外叠加一道顶部高光，避免阴影在近黑背景上消失。", "Layers closer to the user cast deeper, softer shadows. Dark mode adds a faint top highlight so depth survives on near-black.")}
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
        title={text("动效曲线", "Motion curves")}
        description={text("每种变化只用一种曲线。下面的演示放慢了 4 倍，便于看清差别。", "Each kind of change uses one curve. The demo below is slowed 4× so the differences are visible.")}
        actions={(
          <Button type="button" variant="outline" size="sm" onClick={() => { setRun((value) => !value); setReplay((value) => value + 1) }}>
            <Play data-icon="inline-start" />
            {text("播放", "Play")}
          </Button>
        )}
      >
        <div className="visual-lang-curves">
          {MOTION_CURVES.map((curve) => (
            <div key={curve.token} className="visual-lang-curve">
              <svg className="visual-lang-curve-graph" viewBox="-4 -12 56 72" aria-hidden="true">
                <path className="visual-lang-curve-axis" d="M0 0 V48 H48" />
                <path className="visual-lang-curve-line" d={curvePath(curve.points)} />
              </svg>
              <div className="visual-lang-curve-copy">
                <strong>{text(curve.zh, curve.en)}</strong>
                <code>{curve.token} · {curve.duration}</code>
                <div className="visual-lang-lane">
                  <span
                    className="visual-lang-lane-dot"
                    data-run={run ? "true" : undefined}
                    style={{ transitionTimingFunction: `var(${curve.token})`, transitionDuration: `calc(var(${curve.duration}) * 4)` }}
                  />
                </div>
              </div>
            </div>
          ))}
        </div>
      </PageSection>

      <div className="visual-lang-split">
        <PageSection title={text("时长刻度", "Duration scale")} description={text("越小的元素越快；退出比进入快一档。", "Smaller elements move faster; exits run one step quicker than entrances.")}>
          <div className="visual-lang-durations">
            {DURATION_STEPS.map(([token, ms]) => (
              <div key={token} className="visual-lang-duration">
                <code>{token.replace("--vcn-duration-", "")}</code>
                <span className="visual-lang-duration-bar" style={{ width: `${(ms / LONGEST_DURATION_MS) * 100}%` }} />
                <span className="visual-lang-duration-value">{ms}ms</span>
              </div>
            ))}
          </div>
        </PageSection>
        <PageSection title={text("错峰进入", "Staggered entrance")} description={text("页面分区依次出现，第 5 个之后不再递增。", "Page sections arrive one after another, capped after the fifth.")}>
          <div key={replay} className="visual-lang-stagger">
            {[0, 1, 2, 3, 4].map((index) => <span key={index} style={{ animationDelay: `calc(var(--vcn-stagger) * ${index} * 4)` }} />)}
          </div>
        </PageSection>
      </div>

      <PageSection
        title={text("材质", "Materials")}
        description={text("主按钮带纵向渐变和顶部高光，悬停时泛起品牌色光晕；浮层在支持的浏览器上使用磨砂玻璃；聚焦时在描边外加一圈柔和光环。", "Primary buttons carry a vertical gradient and top highlight, glowing in the brand colour on hover. Overlays use frosted glass where supported. Focus adds a soft halo outside the ring.")}
      >
        <div className="visual-lang-materials">
          <div className="visual-lang-material">
            <div className="visual-lang-material-stage">
              <Button type="button">{text("主要操作", "Primary action")}</Button>
              <Button type="button" variant="outline">{text("次要", "Secondary")}</Button>
            </div>
            <code>--vcn-primary-gradient · --vcn-glow-primary</code>
          </div>
          <div className="visual-lang-material">
            <div className="visual-lang-material-stage visual-lang-glass-backdrop">
              <div className="visual-lang-glass">{text("磨砂浮层", "Frosted overlay")}</div>
            </div>
            <code>--vcn-surface-glass</code>
          </div>
          <div className="visual-lang-material">
            <div className="visual-lang-material-stage">
              <Input aria-label={text("聚焦光环示例", "Focus halo sample")} className="visual-lang-focus-sample" placeholder={text("点这里试试", "Click to focus")} />
            </div>
            <code>--vcn-focus-halo</code>
          </div>
          <div className="visual-lang-material">
            <div className="visual-lang-material-stage">
              <Switch aria-label={text("回弹开关示例", "Spring switch sample")} checked={switchOn} onCheckedChange={setSwitchOn} />
              <Checkbox aria-label={text("弹出勾选示例", "Pop-in checkbox sample")} checked={checked} onCheckedChange={(value) => setChecked(value === true)} />
            </div>
            <code>--vcn-ease-spring</code>
          </div>
        </div>
      </PageSection>

      <PageSection title={text("圆角", "Radius")} description={text("容器越大圆角越大；嵌套时内层圆角比外层小一档。", "Larger containers get larger radii; nested layers step one size down.")}>
        <div className="visual-lang-radii">
          {RADIUS_STEPS.map(([token, value]) => (
            <div key={token} className="visual-lang-radius">
              <span style={{ borderRadius: value }} />
              <code>{token}</code>
            </div>
          ))}
        </div>
      </PageSection>
    </>
  )
}
