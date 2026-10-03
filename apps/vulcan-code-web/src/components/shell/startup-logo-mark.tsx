import { useEffect, useRef } from "react";
import type { RefObject } from "react";
import { VULCAN_STACKED_LOGO_ASPECT, VulcanStackedLogo } from "@/components/shell/vulcan-stacked-logo";

// STARTUP_LOGO_WIDTH is the single CSS-pixel width of the startup lockup; its height follows the master aspect ratio.
// STARTUP_LOGO_WIDTH 是启动组合标志唯一的 CSS 像素宽度；高度按母版宽高比推导。
const STARTUP_LOGO_WIDTH = 232;
// STARTUP_LOGO_HEIGHT derives from the width so the lockup, its canvas and its mask always share one box.
// STARTUP_LOGO_HEIGHT 由宽度推导，使组合标志、画布与蒙版始终共用同一尺寸。
const STARTUP_LOGO_HEIGHT = STARTUP_LOGO_WIDTH / VULCAN_STACKED_LOGO_ASPECT;

// StartupLogoMark renders the stacked brand lockup (symbol, then wordmark) with a Canvas-native scan highlight.
// StartupLogoMark 渲染竖版品牌组合（图标在上、字标在下），并叠加 Canvas 原生扫光高亮。
export function StartupLogoMark() {
  // shineCanvasRef stores the transparent canvas layer used for the moving logo highlight.
  // shineCanvasRef 保存用于绘制 Logo 动态高光的透明画布层。
  const shineCanvasRef = useRef<HTMLCanvasElement | null>(null);
  // logoRef points at the rendered lockup so the highlight mask uses its exact outlines.
  // logoRef 指向已渲染的组合标志，使高光蒙版使用其精确轮廓。
  const logoRef = useRef<SVGSVGElement | null>(null);
  useStartupLogoScan(shineCanvasRef, logoRef, STARTUP_LOGO_WIDTH, STARTUP_LOGO_HEIGHT);
  return (
    <div className="startup-logo-stage" aria-hidden="true" style={{ width: STARTUP_LOGO_WIDTH, height: STARTUP_LOGO_HEIGHT }}>
      <VulcanStackedLogo className="startup-logo-mark" ref={logoRef} />
      <canvas ref={shineCanvasRef} className="startup-logo-shine-canvas" />
    </div>
  );
}

// useStartupLogoScan paints a masked moving highlight into a transparent canvas.
// useStartupLogoScan 将被 Logo 轮廓裁剪的移动高光绘制到透明画布中。
// Parameters: canvasRef is the drawing host, logoRef the rendered lockup used as the mask, logoWidth and logoHeight the
// CSS pixel box; returns no value and owns cleanup of its animation and preference listener.
// 参数：canvasRef 是绘制宿主，logoRef 是用作蒙版的已渲染组合标志，logoWidth 与 logoHeight 是 CSS 像素尺寸；
// 不返回值，并负责动画及偏好监听清理。
function useStartupLogoScan(
  // Canvas reference that receives the animated highlight layer.
  // 接收动态高光层的画布引用。
  canvasRef: RefObject<HTMLCanvasElement | null>,
  // Rendered lockup whose outlines become the alpha mask for the highlight.
  // 其轮廓作为高光 alpha 蒙版的已渲染组合标志。
  logoRef: RefObject<SVGSVGElement | null>,
  // CSS pixel width of the logo canvas.
  // Logo 画布的 CSS 像素宽度。
  logoWidth: number,
  // CSS pixel height of the logo canvas.
  // Logo 画布的 CSS 像素高度。
  logoHeight: number,
) {
  useEffect(() => {
    // canvas is the visible transparent layer placed above the static logo.
    // canvas 是叠放在静态 Logo 之上的可见透明图层。
    const canvas = canvasRef.current;
    // logo is the inline lockup serialized once into the mask image.
    // logo 是只序列化一次为蒙版图像的内联组合标志。
    const logo = logoRef.current;
    if (!canvas || !logo) {
      return;
    }
    // context is required for drawing the highlight and applying the alpha mask.
    // context 用于绘制高光并应用 alpha 蒙版。
    const context = canvas.getContext("2d");
    if (!context) {
      return;
    }
    // pixelRatio keeps the canvas sharp on high-DPI WebView2 displays.
    // pixelRatio 确保画布在高 DPI WebView2 屏幕上保持清晰。
    const pixelRatio = Math.max(1, window.devicePixelRatio);
    canvas.width = Math.round(logoWidth * pixelRatio);
    canvas.height = Math.round(logoHeight * pixelRatio);
    canvas.style.width = `${logoWidth}px`;
    canvas.style.height = `${logoHeight}px`;
    context.setTransform(pixelRatio, 0, 0, pixelRatio, 0, 0);

    // maskCanvas stores the logo alpha once so each animation frame can clip the shine cheaply.
    // maskCanvas 只缓存一次 Logo alpha，便于每帧低成本裁剪扫光。
    const maskCanvas = document.createElement("canvas");
    maskCanvas.width = canvas.width;
    maskCanvas.height = canvas.height;
    // maskContext draws the logo mask at the same device-pixel ratio as the visible canvas.
    // maskContext 按可见画布相同的设备像素比绘制 Logo 蒙版。
    const maskContext = maskCanvas.getContext("2d");
    if (!maskContext) {
      return;
    }
    maskContext.setTransform(pixelRatio, 0, 0, pixelRatio, 0, 0);

    // logoImage rasterizes the serialized lockup once; only its alpha matters, so currentColor may resolve to black.
    // logoImage 只栅格化一次序列化的组合标志；只使用其 alpha，currentColor 解析为黑色亦无影响。
    const logoImage = new Image();
    logoImage.decoding = "async";
    logoImage.src = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(new XMLSerializer().serializeToString(logo))}`;

    // animationFrame keeps the current requestAnimationFrame handle for cleanup.
    // animationFrame 保存当前 requestAnimationFrame 句柄，便于清理。
    let animationFrame = 0;
    // startTimestamp anchors the animation to this component mount instead of the browser clock.
    // startTimestamp 将动画锚定到当前组件挂载时间，而不是浏览器全局时钟。
    let startTimestamp: number | null = null;
    // disposed prevents pending image or animation callbacks from drawing after unmount.
    // disposed 避免卸载后仍由待处理图片或动画回调继续绘制。
    let disposed = false;
    // reducedMotion preserves the static logo when the user's system disables decorative motion.
    // reducedMotion 在用户系统禁用装饰动画时保留静态 Logo。
    const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)");
    // maskReady prevents a preference change from drawing before the image has loaded.
    // maskReady 防止偏好变化在图片加载前触发绘制。
    let maskReady = false;

    // drawFrame renders one masked scan-light frame.
    // drawFrame 渲染一帧经过蒙版裁剪的扫光。
    // Parameters: timestamp is the animation frame clock; draws one masked frame and schedules the next permitted frame, returning no value.
    // 参数：timestamp 是动画帧时钟；绘制一帧蒙版内容并安排下一允许帧，不返回值。
    const drawFrame = (timestamp: number) => {
      if (disposed || reducedMotion.matches) {
        return;
      }
      if (startTimestamp === null) {
        startTimestamp = timestamp - 120;
      }
      context.clearRect(0, 0, logoWidth, logoHeight);
      // passMs controls the visible sweep duration.
      // passMs 控制可见扫光经过 Logo 的时长。
      const passMs = 1320;
      // cycleMs defines one single pass plus a short quiet tail so the shine does not feel doubled.
      // cycleMs 定义一次单条扫光和短暂停顿，避免产生扫两次的感觉。
      const cycleMs = 1900;
      // cycleElapsed is measured from this component's first frame to avoid a delayed initial phase.
      // cycleElapsed 从组件首帧开始计时，避免首帧落在延迟阶段。
      const cycleElapsed = (timestamp - startTimestamp) % cycleMs;
      if (cycleElapsed < passMs) {
        // progress is the normalized visible sweep position.
        // progress 是可见扫光阶段内的归一化位置。
        const progress = cycleElapsed / passMs;
        // scanX moves the highlight from the lockup's left outside edge to the right outside edge.
        // scanX 将高光从组合标志左侧外部移动到右侧外部。
        const scanX = -logoHeight * 0.4 + progress * (logoWidth + logoHeight * 0.8);
        context.save();
        context.translate(logoWidth / 2, logoHeight / 2);
        context.rotate((18 * Math.PI) / 180);
        context.translate(-logoWidth / 2, -logoHeight / 2);
        // gradient creates a bright center with soft transparent edges.
        // gradient 创建中间明亮、两侧柔和透明的扫光带。
        const gradient = context.createLinearGradient(scanX - 42, 0, scanX + 42, 0);
        gradient.addColorStop(0, "rgba(255, 255, 255, 0)");
        gradient.addColorStop(0.34, "rgba(255, 255, 255, 0.14)");
        gradient.addColorStop(0.5, "rgba(255, 255, 255, 0.84)");
        gradient.addColorStop(0.66, "rgba(224, 247, 255, 0.22)");
        gradient.addColorStop(1, "rgba(255, 255, 255, 0)");
        context.fillStyle = gradient;
        context.shadowColor = "rgba(56, 189, 248, 0.34)";
        context.shadowBlur = 12;
        context.fillRect(scanX - 46, -logoHeight * 0.65, 92, logoHeight * 2.3);
        context.restore();
        // destination-in clips every drawn pixel to the exact logo alpha, preventing side pollution.
        // destination-in 将所有已绘制像素裁剪到精确 Logo alpha，避免侧边污染块。
        context.globalCompositeOperation = "destination-in";
        context.drawImage(maskCanvas, 0, 0, logoWidth, logoHeight);
        context.globalCompositeOperation = "source-over";
      }
      animationFrame = window.requestAnimationFrame(drawFrame);
    };

    logoImage.onload = () => {
      if (disposed) {
        return;
      }
      maskContext.clearRect(0, 0, logoWidth, logoHeight);
      maskContext.drawImage(logoImage, 0, 0, logoWidth, logoHeight);
      maskReady = true;
      if (!reducedMotion.matches) animationFrame = window.requestAnimationFrame(drawFrame);
    };
    logoImage.onerror = () => {
      context.clearRect(0, 0, logoWidth, logoHeight);
    };
    // updateMotion stops immediately or resumes one animation loop after a system preference change.
    // updateMotion 在系统偏好变化后立即停止动画或恢复单一动画循环。
    // Parameters: none; reads the current reduced-motion preference. Returns no value after stopping or resuming this instance's sole animation loop.
    // 参数：无，读取当前减少动态效果偏好。停止或恢复本实例唯一动画循环后不返回值。
    const updateMotion = () => {
      window.cancelAnimationFrame(animationFrame);
      context.clearRect(0, 0, logoWidth, logoHeight);
      startTimestamp = null;
      if (!reducedMotion.matches && maskReady) animationFrame = window.requestAnimationFrame(drawFrame);
    };
    reducedMotion.addEventListener("change", updateMotion);

    return () => {
      disposed = true;
      reducedMotion.removeEventListener("change", updateMotion);
      logoImage.onload = null;
      logoImage.onerror = null;
      window.cancelAnimationFrame(animationFrame);
      context.clearRect(0, 0, logoWidth, logoHeight);
    };
  }, [canvasRef, logoHeight, logoRef, logoWidth]);
}
