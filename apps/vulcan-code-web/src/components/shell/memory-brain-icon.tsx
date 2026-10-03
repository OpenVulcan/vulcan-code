// MemoryBrainIcon draws the 🧠 emoji silhouette on the lucide 24px stroke grid: a whole brain seen from the side,
// frontal lobe to the left, lobed crown, a lateral fissure setting off the temporal lobe, cerebellum and brainstem.
// lucide's brain icons split the brain into two hemispheres, which reads poorly at rail and menu size.
// MemoryBrainIcon 在 lucide 24px 描边网格上绘制 🧠 表情的轮廓：侧视的完整大脑，额叶朝左，
// 顶部呈分叶状，外侧裂分出颞叶，并带有小脑与脑干。
// lucide 的大脑图标将大脑分为左右两半，在活动栏和菜单尺寸下观感不佳。
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
      {/* Cerebrum: five lobed arcs from the frontal base over the crown to the occipital pole, closed by the base. */}
      {/* 大脑：从额叶底部经顶部到枕极的五段分叶弧线，再由底缘闭合。 */}
      <path d="M8 16.2A3.6 3.6 0 0 1 3.4 10.7 3.8 3.8 0 0 1 8.4 5.1 3.5 3.5 0 0 1 14.6 4.6 3.6 3.6 0 0 1 20.2 8.7 3.3 3.3 0 0 1 19.4 14.3C16 15.7 12 16.5 8 16.2" />
      {/* Cerebellum hanging under the occipital lobe. */}
      {/* 悬于枕叶下方的小脑。 */}
      <path d="M14.4 15.8A2.7 2.7 0 1 0 19.4 14.3" />
      {/* Brainstem, angled back like the emoji. */}
      {/* 与表情一致、向后倾斜的脑干。 */}
      <path d="M11.8 16.4l.8 4.6" />
      {/* Lateral fissure and central sulcus. */}
      {/* 外侧裂与中央沟。 */}
      <path d="M4.6 13.5c2.6-1.8 6-2.2 9-1.2" />
      <path d="M14.6 4.6c-.5 1.6 0 3 1.2 3.9" />
    </svg>
  );
}
