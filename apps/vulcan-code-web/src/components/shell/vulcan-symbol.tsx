import { useId } from "react";

// symbolBodyPath is copied unchanged from the locked OpenVulcan symbol master (chevron body).
// symbolBodyPath 原样取自锁定的 OpenVulcan 纯图标母版（折角主体）。
const symbolBodyPath = "M 2 0 L 84 0 C 88 0 91 1 95 5 L 153 76 C 159 83 159 95 153 102 L 95 173 C 91 177 88 178 84 178 L 2 178 L 78 89 Z";
// symbolFlamePath is copied unchanged from the locked OpenVulcan symbol master (flame).
// symbolFlamePath 原样取自锁定的 OpenVulcan 纯图标母版（火焰）。
const symbolFlamePath = "M 73 0 C 64 14 51 28 38 40 C 21 55 7 68 2 87 C -2 107 8 129 25 150 C 39 134 53 119 67 107 C 82 95 91 79 93 60 C 95 40 86 18 73 0 Z";

// VULCAN_SYMBOL_VIEW_BOX is the tight bounds of the locked symbol master in its own units.
// VULCAN_SYMBOL_VIEW_BOX 是锁定图标母版在自身单位下的紧边界。
export const VULCAN_SYMBOL_VIEW_BOX = "32 29 232 259";

// VulcanSymbolShapes renders the symbol body and flame without an outer <svg>, so lockups can place it.
// VulcanSymbolShapes 渲染不带外层 <svg> 的图标主体与火焰，便于组合版式定位。
// Parameters: gradientId must be unique in the document; returns the defs and both paths in symbol master units.
// 参数：gradientId 必须在文档内唯一；返回图标母版单位下的渐变定义与两条路径。
export function VulcanSymbolShapes(props: {
  // Document-unique id for the flame gradient.
  // 火焰渐变在文档内唯一的 id。
  gradientId: string;
}) {
  return (
    <>
      <defs>
        <linearGradient id={props.gradientId} x1="0%" x2="100%" y1="100%" y2="0%">
          <stop offset="0%" stopColor="#FF6527" />
          <stop offset="100%" stopColor="#F44E1C" />
        </linearGradient>
      </defs>
      <path d={symbolBodyPath} fill="currentColor" transform="translate(30 110)" />
      <path d={symbolFlamePath} fill={`url(#${props.gradientId})`} transform="translate(169 29)" />
    </>
  );
}

// VulcanSymbol renders the brand symbol inline so its body follows the current theme text color.
// VulcanSymbol 以内联方式渲染品牌图标，使主体跟随当前主题文字颜色。
// Parameters: className sizes the symbol through existing brand-logo rules; returns a decorative SVG.
// 参数：className 通过现有 brand-logo 规则控制尺寸；返回装饰性 SVG。
export function VulcanSymbol(props: {
  // CSS classes that decide the rendered box; the symbol keeps its aspect ratio inside it.
  // 决定渲染盒尺寸的 CSS 类；图标在盒内保持原始比例。
  className?: string;
}) {
  // gradientId stays unique per instance so the sidebar and header never share one definition.
  // gradientId 按实例唯一，避免侧栏与标题栏共用同一渐变定义。
  const gradientId = `vulcan-flame-${useId().replace(/:/g, "")}`;
  return (
    <svg aria-hidden="true" className={props.className} focusable="false" viewBox={VULCAN_SYMBOL_VIEW_BOX}>
      <VulcanSymbolShapes gradientId={gradientId} />
    </svg>
  );
}
