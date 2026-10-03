import { useId } from "react";
import { VULCAN_SYMBOL_ASPECT, VULCAN_SYMBOL_VIEW_BOX, VulcanSymbolShapes } from "@/components/shell/vulcan-symbol";
import { VULCAN_WORDMARK_ASPECT, VULCAN_WORDMARK_VIEW_BOX, VulcanWordmarkShapes } from "@/components/shell/vulcan-wordmark";

// StackedLogoProportions sizes a stacked lockup in multiples of the wordmark height.
// StackedLogoProportions 以字标高度为单位描述竖版组合的尺寸。
export type StackedLogoProportions = {
  // Symbol height divided by wordmark height.
  // 图标高度与字标高度之比。
  symbol: number;
  // Vertical gap between the symbol and the wordmark divided by wordmark height.
  // 图标与字标之间的垂直间距与字标高度之比。
  gap: number;
};

// VULCAN_STACKED_MASTER_PROPORTIONS are the locked stacked master values (S220 / G30).
// VULCAN_STACKED_MASTER_PROPORTIONS 是锁定竖版母版的取值（S220 / G30）。
export const VULCAN_STACKED_MASTER_PROPORTIONS: StackedLogoProportions = { symbol: 2.2, gap: 0.3 };

// symbolOpticalShift is the master's X+2 correction: the symbol moves right by 2.2% of the wordmark height.
// symbolOpticalShift 是母版的 X+2 光学修正：图标向右偏移字标高度的 2.2%。
const symbolOpticalShift = 0.022;

// stackedLogoLayout places the symbol and the wordmark in one box measured in wordmark heights.
// stackedLogoLayout 在以字标高度为单位的盒子中摆放图标与字标。
// Parameters: proportions chooses the symbol size and gap; returns the overall box and both placements.
// 参数：proportions 选择图标尺寸与间距；返回整体尺寸与两者的位置。
export function stackedLogoLayout(proportions: StackedLogoProportions) {
  // wordmarkWidth and symbolWidth follow the masters' own aspect ratios.
  // wordmarkWidth 与 symbolWidth 按各自母版宽高比推导。
  const wordmarkWidth = VULCAN_WORDMARK_ASPECT;
  const symbolWidth = proportions.symbol * VULCAN_SYMBOL_ASPECT;
  const width = Math.max(wordmarkWidth, symbolWidth);
  return {
    width,
    height: proportions.symbol + proportions.gap + 1,
    symbol: { x: (width - symbolWidth) / 2 + symbolOpticalShift, y: 0, width: symbolWidth, height: proportions.symbol },
    wordmark: { x: (width - wordmarkWidth) / 2, y: proportions.symbol + proportions.gap, width: wordmarkWidth, height: 1 },
  };
}

// VulcanStackedLogo renders the stacked brand lockup: the symbol on the first row and the wordmark on the next.
// Proportions default to the locked stacked master; only colors follow the theme through currentColor.
// VulcanStackedLogo 渲染竖版品牌组合：首行为图标，下一行为字标。
// 比例默认取锁定的竖版母版；仅颜色通过 currentColor 跟随主题。
// Parameters: className sizes the lockup and proportions overrides the master spacing; returns a decorative SVG.
// 参数：className 控制组合尺寸，proportions 覆盖母版间距；返回装饰性 SVG。
export function VulcanStackedLogo(props: {
  // CSS classes that decide the rendered box; the lockup keeps its aspect ratio inside it.
  // 决定渲染盒尺寸的 CSS 类；组合在盒内保持原始比例。
  className?: string;
  // Symbol size and gap in wordmark heights; defaults to the locked master.
  // 以字标高度为单位的图标尺寸与间距；默认取锁定母版。
  proportions?: StackedLogoProportions;
}) {
  // idBase keeps both gradients unique per instance.
  // idBase 使两个渐变按实例唯一。
  const idBase = useId().replace(/:/g, "");
  // layout places both parts in wordmark-height units, which double as the viewBox units.
  // layout 以字标高度为单位摆放两部分，该单位同时作为 viewBox 单位。
  const layout = stackedLogoLayout(props.proportions ?? VULCAN_STACKED_MASTER_PROPORTIONS);
  return (
    <svg aria-hidden="true" className={props.className} focusable="false" viewBox={`0 0 ${layout.width} ${layout.height}`}>
      {/* Rows sit in groups because CSS transforms apply to <g> reliably but not to nested <svg> viewports. */}
      {/* 两行各自包在分组中，因为 CSS 变换可稳定作用于 <g>，但不适用于嵌套 <svg> 视口。 */}
      <g className="vulcan-stacked-symbol">
        <svg {...layout.symbol} preserveAspectRatio="xMidYMid meet" viewBox={VULCAN_SYMBOL_VIEW_BOX}>
          <VulcanSymbolShapes gradientId={`vulcan-stacked-flame-${idBase}`} />
        </svg>
      </g>
      <g className="vulcan-stacked-wordmark">
        <svg {...layout.wordmark} preserveAspectRatio="xMidYMid meet" viewBox={VULCAN_WORDMARK_VIEW_BOX}>
          <VulcanWordmarkShapes gradientId={`vulcan-stacked-code-${idBase}`} />
        </svg>
      </g>
    </svg>
  );
}
