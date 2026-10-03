import { useId } from "react";

// WordmarkGlyph places one outlined glyph from the wordmark master at its horizontal offset.
// WordmarkGlyph 描述字标母版中的一个转曲字形及其水平偏移。
type WordmarkGlyph = {
  // Horizontal offset in master units, copied from the glyph's translate().
  // 母版单位下的水平偏移，取自字形的 translate()。
  x: number;
  // Outline path data in master units.
  // 母版单位下的轮廓路径数据。
  d: string;
};

// vulcanGlyphs are copied unchanged from the locked VulcanCode wordmark master (outlined, "Vulcan" 550 weight).
// vulcanGlyphs 原样取自锁定的 VulcanCode 纯文字字标转曲母版（「Vulcan」550 字重）。
const vulcanGlyphs: readonly WordmarkGlyph[] = [
  { x: 0, d: "M551.0 0.0H849.0L1376.5 1490.0H1117.5L844.0 678.5C806.5 561.5 761.0 413.5 703.0 221.0C643.5 413.5 597.0 561.5 559.0 678.5L277.0 1490.0H14.5Z" },
  { x: 1391.5, d: "M494.5 -21.0C637.5 -21.0 744.5 41.0 816.5 160.5V0.0H1044.0V1056.0H812.0V453.5C812.0 265.5 707.5 184.0 569.5 184.0C428.5 184.0 347.0 268.0 347.0 430.0V1056.0H115.5V397.0C115.5 123.5 275.5 -21.0 494.5 -21.0Z" },
  { x: 2551, d: "M347.0 1490.0H115.5V0.0H347.0Z" },
  { x: 3013.5, d: "M575.0 -24.0C834.5 -24.0 1024.5 145.0 1045.5 370.5H814.5C788.5 250.5 715.5 172.0 577.0 172.0C402.5 172.0 299.5 310.5 299.5 526.5C299.5 744.0 403.5 884.0 577.0 884.0C714.0 884.0 796.5 802.5 817.0 686.5H1045.5C1021.5 916.0 833.5 1080.0 575.0 1080.0C270.0 1080.0 65.0 852.0 65.0 526.5C65.0 202.0 269.5 -24.0 575.0 -24.0Z" },
  { x: 4117.5, d: "M437.5 -17.0C614.5 -17.0 702.0 60.0 749.0 149.5H753.0V0.0H981.0V722.0C981.0 939.5 820.0 1077.0 552.5 1077.0C282.5 1077.0 111.0 936.5 100.0 731.0H325.0C332.5 824.0 419.5 892.5 548.5 892.5C676.0 892.5 751.5 823.5 751.5 727.5V719.0C751.5 644.5 685.0 644.0 488.0 621.0C271.5 597.0 72.0 541.5 72.0 301.0C72.0 90.0 227.5 -17.0 437.5 -17.0ZM490.0 161.5C373.5 161.5 300.0 213.0 300.0 300.0C300.0 401.5 398.0 441.5 509.5 458.0C617.5 474.0 722.5 490.5 752.5 512.0V393.0C752.5 266.5 665.5 161.5 490.0 161.5Z" },
  { x: 5226.5, d: "M347.0 602.5C347.0 790.5 452.0 872.0 590.0 872.0C730.5 872.0 812.0 788.0 812.0 626.0V0.0H1044.0V659.0C1044.0 932.5 883.5 1077.0 664.5 1077.0C522.5 1077.0 415.0 1015.0 343.0 895.5V1056.0H115.5V0.0H347.0Z" },
];

// codeGlyphs are copied unchanged from the locked VulcanCode wordmark master (outlined, "Code" 450 weight).
// codeGlyphs 原样取自锁定的 VulcanCode 纯文字字标转曲母版（「Code」450 字重）。
const codeGlyphs: readonly WordmarkGlyph[] = [
  { x: 6368.12, d: "M777.5 -24.0C1096.0 -24.0 1346.0 173.5 1398.5 487.0H1194.5C1145.5 278.0 987.0 161.5 778.5 161.5C474.5 161.5 287.0 398.5 287.0 744.0C287.0 1091.5 474.5 1328.5 778.5 1328.5C988.5 1328.5 1147.5 1211.0 1193.5 1001.0H1398.5C1345.0 1315.5 1096.0 1514.0 777.5 1514.0C365.0 1514.0 85.5 1199.0 85.5 744.0C85.5 292.0 366.0 -24.0 777.5 -24.0Z" },
  { x: 7848.62, d: "M567.5 -24.0C865.0 -24.0 1065.0 202.5 1065.0 526.0C1065.0 851.5 865.0 1080.0 567.5 1080.0C270.0 1080.0 69.5 851.5 69.5 526.0C69.5 202.5 270.0 -24.0 567.5 -24.0ZM567.5 144.0C377.0 144.0 262.5 293.5 262.5 526.0C262.5 760.5 378.0 912.0 567.5 912.0C757.0 912.0 872.5 760.0 872.5 526.0C872.5 294.5 758.0 144.0 567.5 144.0Z" },
  { x: 8983.62, d: "M527.5 -20.5C665.0 -20.5 784.5 38.0 858.0 160.5H860.0V0.0H1046.5V1490.0H856.0V901.5H854.0C786.0 1019.0 664.0 1076.5 527.0 1076.5C253.0 1076.5 69.5 854.0 69.5 528.0C69.5 202.0 252.5 -20.5 527.5 -20.5ZM562.0 147.0C385.5 147.0 262.5 279.5 262.5 528.0C262.5 777.0 385.5 910.0 562.0 910.0C750.0 910.0 870.0 758.5 870.0 528.0C870.0 297.5 750.0 147.0 562.0 147.0Z" },
  { x: 10152.6, d: "M566.0 -24.0C800.5 -24.0 984.5 115.5 1024.0 318.5H837.5C806.5 213.5 711.5 141.0 567.5 141.0C375.0 141.0 265.0 276.5 257.5 475.0H1041.0V527.0C1041.0 854.0 851.0 1080.0 554.5 1080.0C270.5 1080.0 69.5 847.5 69.5 526.0C69.5 207.5 254.5 -24.0 566.0 -24.0ZM259.5 628.0C277.5 805.0 389.5 916.0 555.5 916.0C722.5 916.0 835.5 805.0 853.0 628.0Z" },
];

// VULCAN_WORDMARK_VIEW_BOX is the tight bounds of the locked wordmark master in its own units.
// VULCAN_WORDMARK_VIEW_BOX 是锁定字标母版在自身单位下的紧边界。
export const VULCAN_WORDMARK_VIEW_BOX = "0 0 11179.12 1538";

// VulcanWordmarkShapes renders the outlined wordmark without an outer <svg>, so lockups can place it.
// "Vulcan" follows the current text color and "Code" carries the same flame gradient as the symbol.
// VulcanWordmarkShapes 渲染不带外层 <svg> 的转曲字标，便于组合版式定位。
// 「Vulcan」跟随当前文字颜色，「Code」使用与图标火焰相同的渐变。
// Parameters: gradientId must be unique in the document; returns the defs and glyph groups in wordmark master units.
// 参数：gradientId 必须在文档内唯一；返回字标母版单位下的渐变定义与字形分组。
export function VulcanWordmarkShapes(props: {
  // Document-unique id for the "Code" gradient.
  // 「Code」渐变在文档内唯一的 id。
  gradientId: string;
}) {
  return (
    <>
      <defs>
        <linearGradient id={props.gradientId} x1="0%" x2="100%" y1="0%" y2="0%">
          <stop offset="0%" stopColor="#FF6527" />
          <stop offset="100%" stopColor="#F44E1C" />
        </linearGradient>
      </defs>
      {/* The master draws glyphs in a y-up font space; this transform is copied unchanged from it. */}
      {/* 母版在 y 轴向上的字体坐标系中绘制字形；此变换原样取自母版。 */}
      <g transform="translate(-14.5 1514) scale(1 -1)">
        <g fill="currentColor">
          {vulcanGlyphs.map((glyph) => <path d={glyph.d} key={glyph.x} transform={`translate(${glyph.x} 0)`} />)}
        </g>
        <g fill={`url(#${props.gradientId})`}>
          {codeGlyphs.map((glyph) => <path d={glyph.d} key={glyph.x} transform={`translate(${glyph.x} 0)`} />)}
        </g>
      </g>
    </>
  );
}

// VulcanWordmark renders the outlined brand wordmark inline so it matches the brand master in every theme.
// VulcanWordmark 以内联方式渲染转曲品牌字标，使其在各主题下都与品牌母版一致。
// Parameters: className sizes the wordmark (set a height; the width follows the master aspect ratio) and label, when set,
// exposes it as a named image; returns the SVG, decorative when no label is given.
// 参数：className 控制字标尺寸（设定高度，宽度按母版比例推导）；label 设置时将其暴露为具名图像；
// 返回 SVG，未提供 label 时为装饰性。
export function VulcanWordmark(props: {
  // CSS classes that decide the rendered box; the wordmark keeps its aspect ratio inside it.
  // 决定渲染盒尺寸的 CSS 类；字标在盒内保持原始比例。
  className?: string;
  // Accessible name announced in place of the outlined text; omit when a surrounding element already names the brand.
  // 代替转曲文字朗读的无障碍名称；外层元素已提供品牌名称时省略。
  label?: string;
}) {
  // gradientId stays unique per instance so several wordmarks never share one definition.
  // gradientId 按实例唯一，避免多个字标共用同一渐变定义。
  const gradientId = `vulcan-wordmark-code-${useId().replace(/:/g, "")}`;
  return (
    <svg
      aria-hidden={props.label ? undefined : true}
      aria-label={props.label}
      className={props.className}
      focusable="false"
      role={props.label ? "img" : undefined}
      viewBox={VULCAN_WORDMARK_VIEW_BOX}
    >
      <VulcanWordmarkShapes gradientId={gradientId} />
    </svg>
  );
}
