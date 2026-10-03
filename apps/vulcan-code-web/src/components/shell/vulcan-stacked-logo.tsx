import { forwardRef, useId } from "react";
import { VULCAN_SYMBOL_VIEW_BOX, VulcanSymbolShapes } from "@/components/shell/vulcan-symbol";
import { VULCAN_WORDMARK_VIEW_BOX, VulcanWordmarkShapes } from "@/components/shell/vulcan-wordmark";

// VULCAN_STACKED_LOGO_ASPECT is the width / height ratio of the locked stacked master (S220 / G30 / X+2).
// VULCAN_STACKED_LOGO_ASPECT 是锁定竖版母版（S220 / G30 / X+2）的宽高比。
export const VULCAN_STACKED_LOGO_ASPECT = 7268.6086 / 3500;

// VulcanStackedLogo renders the stacked brand lockup: the symbol on the first row and the wordmark on the next.
// Placement is copied unchanged from the locked stacked master; only colors follow the theme through currentColor.
// VulcanStackedLogo 渲染竖版品牌组合：首行为图标，下一行为字标。
// 摆放坐标原样取自锁定的竖版母版；仅颜色通过 currentColor 跟随主题。
// Parameters: className sizes the lockup; returns a decorative SVG whose element is exposed through ref.
// 参数：className 控制组合尺寸；返回装饰性 SVG，并通过 ref 暴露其元素。
export const VulcanStackedLogo = forwardRef<SVGSVGElement, {
  // CSS classes that decide the rendered box; the lockup keeps its aspect ratio inside it.
  // 决定渲染盒尺寸的 CSS 类；组合在盒内保持原始比例。
  className?: string;
}>(function VulcanStackedLogo(props, ref) {
  // idBase keeps both gradients unique per instance.
  // idBase 使两个渐变按实例唯一。
  const idBase = useId().replace(/:/g, "");
  return (
    <svg aria-hidden="true" className={props.className} focusable="false" ref={ref} viewBox="0 0 7268.6086 3500">
      <svg height="2200" preserveAspectRatio="xMidYMid meet" viewBox={VULCAN_SYMBOL_VIEW_BOX} width="1970.6564" x="2670.9541" y="0">
        <VulcanSymbolShapes gradientId={`vulcan-stacked-flame-${idBase}`} />
      </svg>
      <svg height="1000" preserveAspectRatio="xMidYMid meet" viewBox={VULCAN_WORDMARK_VIEW_BOX} width="7268.6086" x="0" y="2500">
        <VulcanWordmarkShapes gradientId={`vulcan-stacked-code-${idBase}`} />
      </svg>
    </svg>
  );
});
