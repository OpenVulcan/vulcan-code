import { useEffect, useRef } from "react";
import type { RefObject } from "react";

// StartupLogoMark renders the startup logo with a Canvas-native scan highlight.
// StartupLogoMark 渲染带有 Canvas 原生扫光高亮的启动 Logo。
export function StartupLogoMark() {
  // shineCanvasRef stores the transparent canvas layer used for the moving logo highlight.
  // shineCanvasRef 保存用于绘制 Logo 动态高光的透明画布层。
  const shineCanvasRef = useRef<HTMLCanvasElement | null>(null);
  useStartupLogoScan(shineCanvasRef, "/vulcan-logo-startup.png", 144);
  return (
    <div className="startup-logo-stage" aria-hidden="true">
      <img alt="" className="startup-logo-mark" src="/vulcan-logo-startup.png" />
      <canvas ref={shineCanvasRef} className="startup-logo-shine-canvas" />
    </div>
  );
}

// useStartupLogoScan paints a masked moving highlight into a transparent canvas.
// useStartupLogoScan 将被 Logo 透明通道裁剪的移动高光绘制到透明画布中。
function useStartupLogoScan(
  // Canvas reference that receives the animated highlight layer.
  // 接收动态高光层的画布引用。
  canvasRef: RefObject<HTMLCanvasElement | null>,
  // Logo source used as the alpha mask for the highlight.
  // 用作高光 alpha 蒙版的 Logo 资源地址。
  logoSrc: string,
  // CSS pixel size of the square logo canvas.
  // 正方形 Logo 画布的 CSS 像素尺寸。
  logoSize: number,
) {
  useEffect(() => {
    // canvas is the visible transparent layer placed above the static logo.
    // canvas 是叠放在静态 Logo 之上的可见透明图层。
    const canvas = canvasRef.current;
    if (!canvas) {
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
    canvas.width = Math.round(logoSize * pixelRatio);
    canvas.height = Math.round(logoSize * pixelRatio);
    canvas.style.width = `${logoSize}px`;
    canvas.style.height = `${logoSize}px`;
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

    // logoImage loads the public PNG once; WebView2 can draw same-origin bitmaps into canvas.
    // logoImage 只加载一次 public PNG；WebView2 可以将同源位图绘制到画布。
    const logoImage = new Image();
    logoImage.decoding = "async";
    logoImage.src = logoSrc;

    // animationFrame keeps the current requestAnimationFrame handle for cleanup.
    // animationFrame 保存当前 requestAnimationFrame 句柄，便于清理。
    let animationFrame = 0;
    // startTimestamp anchors the animation to this component mount instead of the browser clock.
    // startTimestamp 将动画锚定到当前组件挂载时间，而不是浏览器全局时钟。
    let startTimestamp: number | null = null;
    // disposed prevents pending image or animation callbacks from drawing after unmount.
    // disposed 避免卸载后仍由待处理图片或动画回调继续绘制。
    let disposed = false;

    // drawFrame renders one masked scan-light frame.
    // drawFrame 渲染一帧经过蒙版裁剪的扫光。
    const drawFrame = (timestamp: number) => {
      if (disposed) {
        return;
      }
      if (startTimestamp === null) {
        startTimestamp = timestamp - 120;
      }
      context.clearRect(0, 0, logoSize, logoSize);
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
        // scanX moves the highlight from the logo's left outside edge to the right outside edge.
        // scanX 将高光从 Logo 左侧外部移动到右侧外部。
        const scanX = -logoSize * 0.38 + progress * logoSize * 1.76;
        context.save();
        context.translate(logoSize / 2, logoSize / 2);
        context.rotate((18 * Math.PI) / 180);
        context.translate(-logoSize / 2, -logoSize / 2);
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
        context.fillRect(scanX - 46, -logoSize * 0.65, 92, logoSize * 2.3);
        context.restore();
        // destination-in clips every drawn pixel to the exact logo alpha, preventing side pollution.
        // destination-in 将所有已绘制像素裁剪到精确 Logo alpha，避免侧边污染块。
        context.globalCompositeOperation = "destination-in";
        context.drawImage(maskCanvas, 0, 0, logoSize, logoSize);
        context.globalCompositeOperation = "source-over";
      }
      animationFrame = window.requestAnimationFrame(drawFrame);
    };

    logoImage.onload = () => {
      if (disposed) {
        return;
      }
      maskContext.clearRect(0, 0, logoSize, logoSize);
      maskContext.drawImage(logoImage, 0, 0, logoSize, logoSize);
      animationFrame = window.requestAnimationFrame(drawFrame);
    };
    logoImage.onerror = () => {
      context.clearRect(0, 0, logoSize, logoSize);
    };

    return () => {
      disposed = true;
      window.cancelAnimationFrame(animationFrame);
      context.clearRect(0, 0, logoSize, logoSize);
    };
  }, [canvasRef, logoSize, logoSrc]);
}
