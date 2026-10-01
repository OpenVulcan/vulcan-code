// MemoryBrainIcon draws one whole brain in side view, with cerebellum and brainstem, on the lucide 24px stroke grid.
// lucide's brain icons split the brain into two hemispheres, which reads poorly at rail size.
// MemoryBrainIcon 在 lucide 24px 描边网格上绘制一个完整的侧视大脑，含小脑与脑干。
// lucide 的大脑图标将大脑分为左右两半，在活动栏尺寸下观感不佳。
export function MemoryBrainIcon({ className, size = 24 }: {
  // Optional class merged onto the svg, as with lucide icons.
  // 合并到 svg 上的可选类名，与 lucide 图标一致。
  className?: string;
  // Rendered width and height in pixels.
  // 渲染宽高（像素）。
  size?: number;
}) {
  return (
    <svg
      aria-hidden="true"
      className={className}
      fill="none"
      height={size}
      stroke="currentColor"
      strokeLinecap="round"
      strokeLinejoin="round"
      strokeWidth={2}
      viewBox="0 0 24 24"
      width={size}
    >
      <path d="M14 17H8a5 5 0 0 1-4.6-6.9A4.5 4.5 0 0 1 8 4.6a5 5 0 0 1 7.6-.4A5 5 0 0 1 21 9.5a4.5 4.5 0 0 1-2.5 6.3 2.6 2.6 0 0 1-4.5 1.2" />
      <path d="M12.5 17v3.5" />
      <path d="M8.5 9.5a3 3 0 0 1 3 3" />
      <path d="M14.5 7.5a2.5 2.5 0 0 1 2.5 2.5" />
      <path d="M14.5 13h-3" />
    </svg>
  );
}
