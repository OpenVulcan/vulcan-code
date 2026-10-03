import { stackedLogoLayout, VulcanStackedLogo, type StackedLogoProportions } from "@/components/shell/vulcan-stacked-logo";

// startupLogoProportions enlarge the symbol and open the gap beyond the compact master so the startup screen reads
// as a hero mark: the symbol is 3.4 wordmark heights tall and sits 0.85 wordmark heights above the wordmark.
// startupLogoProportions 相比紧凑母版放大图标并拉开间距，使启动页呈现主视觉标志：
// 图标高 3.4 个字标高度，并位于字标上方 0.85 个字标高度处。
const startupLogoProportions: StackedLogoProportions = { symbol: 3.4, gap: 0.85 };
// STARTUP_WORDMARK_HEIGHT is the single CSS-pixel scale of the startup lockup; every other size derives from it.
// STARTUP_WORDMARK_HEIGHT 是启动组合标志唯一的 CSS 像素尺度；其余尺寸均由其推导。
const STARTUP_WORDMARK_HEIGHT = 36;
// startupLogoLayout is the lockup box in wordmark heights, scaled once to CSS pixels below.
// startupLogoLayout 是以字标高度为单位的组合尺寸，下方统一换算为 CSS 像素。
const startupLogoLayout = stackedLogoLayout(startupLogoProportions);

// StartupLogoMark renders the stacked brand lockup (symbol, then wordmark) with the living-flame motion: the chevron
// stays fixed, the flame ignites from its root and the wordmark rises, then the flame keeps a quiet flicker and slowly
// shifting fire tones while startup is pending. All motion lives in boot-styles.css. The intro plays on mount, so the
// first load keeps a single persistent instance (see BootLoadingScreen) instead of remounting it per phase.
// StartupLogoMark 渲染竖版品牌组合（图标在上、字标在下）并叠加「活火」动效：折角保持固定，火焰自根部点燃、
// 字标上浮，之后在启动等待期间火焰保持轻微摇曳与缓慢变换的火色。全部动效定义在 boot-styles.css 中。入场在挂载时
// 播放，因此首次加载只保留一个常驻实例（见 BootLoadingScreen），而不是每个阶段重新挂载。
export function StartupLogoMark() {
  return (
    <div
      className="startup-logo-stage"
      aria-hidden="true"
      style={{ width: startupLogoLayout.width * STARTUP_WORDMARK_HEIGHT, height: startupLogoLayout.height * STARTUP_WORDMARK_HEIGHT }}
    >
      <VulcanStackedLogo className="startup-logo-mark" proportions={startupLogoProportions} />
    </div>
  );
}
