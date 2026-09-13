import React from "react";
import {
  FileText,
  Maximize2,
  Minimize2,
  Minus,
  Plus,
  Scan,
  SquareIcon,
  X,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { useI18n } from "@/i18n";
import { cn } from "@/lib/utils";
import type {
  GeneratedMediaAttachment,
  GeneratedMediaImageOffset,
  GeneratedMediaImageSize,
} from "./generated-media-types";
import {
  GENERATED_MEDIA_MAX_ZOOM,
  GENERATED_MEDIA_MIN_ZOOM,
  GENERATED_MEDIA_ZOOM_STEP,
} from "./generated-media-types";
import {
  clampGeneratedMediaImageOffset,
  clampGeneratedMediaZoom,
  generatedMediaFitScale,
  generatedMediaHasImageOffset,
  generatedMediaIcon,
  generatedMediaZoomOffsetAroundPoint,
  useGeneratedMediaSource,
} from "./generated-media-utils";

// GeneratedMediaPlayerWindow renders a dialog viewer for generated media assets.
// GeneratedMediaPlayerWindow 为生成媒体资源渲染弹窗查看器。
export function GeneratedMediaPlayerWindow(props: {
  // attachment describes the generated media file to play.
  // attachment 描述需要播放或查看的生成媒体文件。
  attachment: GeneratedMediaAttachment;
  // onClose closes the media player dialog.
  // onClose 关闭媒体播放弹窗。
  onClose: () => void;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { locale, t } = useI18n();
  // sourceIdentity remounts one viewer when the selected media changes, isolating late load events.
  // sourceIdentity 在所选媒体变化时重挂载查看器，隔离迟到的加载事件。
  const sourceIdentity = `${props.attachment.kind}:${props.attachment.localPath}`;
  return <GeneratedMediaPlayerContent key={sourceIdentity} {...props} locale={locale} t={t} />;
}

// GeneratedMediaPlayerContent owns zoom and load state for one immutable media identity.
// GeneratedMediaPlayerContent 持有单一不可变媒体标识的缩放与加载状态。
// Parameters provide the selected attachment, close callback and current localized text resolver.
// 参数提供所选附件、关闭回调以及当前本地化文本解析器。
// Returns the bounded media dialog and its local recovery controls.
// 返回受视口约束的媒体弹窗及其局部恢复控件。
function GeneratedMediaPlayerContent(props: {
  // attachment is the selected local media descriptor.
  // attachment 是所选本地媒体描述。
  attachment: GeneratedMediaAttachment;
  // onClose dismisses this viewer.
  // onClose 关闭当前查看器。
  onClose: () => void;
  // locale selects visible failure and keyboard help text.
  // locale 选择可见失败提示及键盘帮助文本。
  locale: string;
  // t resolves existing media toolbar labels.
  // t 解析现有媒体工具栏文案。
  t: ReturnType<typeof useI18n>["t"];
}) {
  const { t } = props;
  // loadFailed retains a media decoding or loading failure until the user retries.
  // loadFailed 保留媒体解码或加载失败，直到用户重试。
  const [loadFailed, setLoadFailed] = React.useState(false);
  // loadAttempt remounts the actual browser media element without changing its authoritative URL.
  // loadAttempt 重挂载浏览器媒体元素，而不更改权威 URL。
  const [loadAttempt, setLoadAttempt] = React.useState(0);
  const imageStageRef = React.useRef<HTMLDivElement | null>(null);
  // imageStageNode tracks portal mounting so observation starts after the actual stage exists.
  // imageStageNode 跟踪门户挂载，确保实际舞台存在后才启动尺寸观测。
  const [imageStageNode, setImageStageNode] = React.useState<HTMLDivElement | null>(null);
  // bindImageStage receives the mounted viewport or null and synchronizes pointer and observer owners.
  // bindImageStage 接收已挂载视口或 null，并同步指针与观测器的所属节点。
  // Returns nothing; React invokes this stable callback whenever the portal stage changes.
  // 无返回值；门户舞台变化时由 React 调用此稳定回调。
  const bindImageStage = React.useCallback((node: HTMLDivElement | null) => {
    imageStageRef.current = node;
    setImageStageNode(node);
  }, []);
  const imageElementRef = React.useRef<HTMLImageElement | null>(null);
  const imageInitializedRef = React.useRef(false);
  const imageDragRef = React.useRef<{
    // Pointer id captured by the image stage during one drag.
    // 单次拖拽期间图片舞台捕获的指针标识。
    pointerId: number;
    // Pointer x coordinate at drag start.
    // 拖拽开始时的指针 x 坐标。
    startX: number;
    // Pointer y coordinate at drag start.
    // 拖拽开始时的指针 y 坐标。
    startY: number;
    // Image offset at drag start.
    // 拖拽开始时的图片偏移。
    startOffset: GeneratedMediaImageOffset;
  } | null>(null);
  const [imageScale, setImageScale] = React.useState(1);
  const [imageOffset, setImageOffset] = React.useState<GeneratedMediaImageOffset>({ x: 0, y: 0 });
  const [isImageDragging, setIsImageDragging] = React.useState(false);
  const [isMaximized, setIsMaximized] = React.useState(false);
  const [imageNaturalSize, setImageNaturalSize] = React.useState<GeneratedMediaImageSize | null>(null);
  const [imageViewportSize, setImageViewportSize] = React.useState<GeneratedMediaImageSize | null>(null);
  const source = useGeneratedMediaSource(props.attachment.localPath, props.attachment.mimeType);
  const MediaIcon = generatedMediaIcon(props.attachment.kind);
  const isImage = props.attachment.kind === "image";
  const imageFitScale = generatedMediaFitScale(imageViewportSize, imageNaturalSize) ?? 1;
  // measureImageViewportSize reads the live viewer bounds and stores usable image viewport dimensions.
  // measureImageViewportSize 读取实时查看器边界并保存可用的图片视口尺寸。
  const measureImageViewportSize = React.useCallback((): GeneratedMediaImageSize | null => {
    const node = imageStageRef.current;
    if (!node || !isImage) {
      return null;
    }
    // Layout dimensions exclude the dialog entrance transform, which otherwise freezes a smaller initial fit.
    // 布局尺寸排除弹窗入场变换，避免初次适配永久采用动画期间的偏小比例。
    const nextSize = { width: node.clientWidth, height: node.clientHeight };
    if (nextSize.width <= 0 || nextSize.height <= 0) {
      return null;
    }
    setImageViewportSize((current) => (
      current && Math.abs(current.width - nextSize.width) < 0.5 && Math.abs(current.height - nextSize.height) < 0.5
        ? current
        : nextSize
    ));
    return nextSize;
  }, [isImage]);
  const syncImageNaturalSize = React.useCallback((image: HTMLImageElement | null) => {
    if (!image || image.naturalWidth <= 0 || image.naturalHeight <= 0) {
      return;
    }
    setImageNaturalSize({ width: image.naturalWidth, height: image.naturalHeight });
  }, []);
  React.useLayoutEffect(() => {
    const node = imageStageRef.current;
    if (!node || !isImage) {
      return undefined;
    }
    measureImageViewportSize();
    const animationFrame = window.requestAnimationFrame(measureImageViewportSize);
    const observer = new ResizeObserver(measureImageViewportSize);
    observer.observe(node);
    return () => {
      window.cancelAnimationFrame(animationFrame);
      observer.disconnect();
    };
  }, [imageStageNode, isImage, isMaximized, measureImageViewportSize]);
  React.useEffect(() => {
    syncImageNaturalSize(imageElementRef.current);
  }, [syncImageNaturalSize, source]);
  React.useEffect(() => {
    if (imageInitializedRef.current || !imageNaturalSize || !imageViewportSize) {
      return;
    }
    imageInitializedRef.current = true;
    setImageScale(generatedMediaFitScale(imageViewportSize, imageNaturalSize) ?? 1);
    setImageOffset({ x: 0, y: 0 });
  }, [imageNaturalSize, imageViewportSize]);
  React.useEffect(() => {
    setImageOffset((offset) => clampGeneratedMediaImageOffset(offset, imageScale, imageViewportSize, imageNaturalSize));
  }, [imageNaturalSize, imageScale, imageViewportSize]);
  const applyImageScaleAroundCenter = React.useCallback((nextScale: number) => {
    const viewportSize = measureImageViewportSize() ?? imageViewportSize;
    if (!imageNaturalSize || !viewportSize) {
      return;
    }
    // The fit defines the lower bound; preserve the gesture direction when resizing raises that bound.
    // 适配比例定义缩放下限；窗口扩大抬高下限时，仍需保持用户操作方向。
    const boundedScale = clampGeneratedMediaZoom(nextScale, generatedMediaFitScale(viewportSize, imageNaturalSize));
    // clampedScale never enlarges the image in response to a shrinking action.
    // clampedScale 保证缩小操作绝不会放大图片。
    const clampedScale = nextScale < imageScale ? Math.min(imageScale, boundedScale) : boundedScale;
    setImageOffset((offset) => generatedMediaZoomOffsetAroundPoint(
      offset,
      imageScale,
      clampedScale,
      { x: 0, y: 0 },
      viewportSize,
      imageNaturalSize,
    ));
    setImageScale(clampedScale);
  }, [imageNaturalSize, imageScale, imageViewportSize, measureImageViewportSize]);
  const handleImageFit = React.useCallback(() => {
    const viewportSize = measureImageViewportSize() ?? imageViewportSize;
    const nextScale = generatedMediaFitScale(viewportSize, imageNaturalSize);
    if (nextScale === null) {
      return;
    }
    setImageScale(nextScale);
    setImageOffset({ x: 0, y: 0 });
  }, [imageNaturalSize, imageViewportSize, measureImageViewportSize]);
  const handleImageOriginalSize = React.useCallback(() => {
    const viewportSize = measureImageViewportSize() ?? imageViewportSize;
    if (!imageNaturalSize || !viewportSize) {
      return;
    }
    setImageScale(1);
    setImageOffset({ x: 0, y: 0 });
  }, [imageNaturalSize, imageViewportSize, measureImageViewportSize]);
  const zoomImageAtPoint = React.useCallback((deltaY: number, clientX: number, clientY: number, target: HTMLElement) => {
    const viewportSize = measureImageViewportSize() ?? imageViewportSize;
    if (!imageNaturalSize || !viewportSize) {
      return;
    }
    const rect = target.getBoundingClientRect();
    const anchor = {
      x: clientX - rect.left - rect.width / 2,
      y: clientY - rect.top - rect.height / 2,
    };
    // Wheel shrinking uses the toolbar's fit-aware lower bound, including after viewport enlargement.
    // 滚轮缩小使用工具栏的适配下限，并处理视口扩大后的下限变化。
    const boundedScale = clampGeneratedMediaZoom(imageScale * (deltaY < 0 ? 1.1 : 1 / 1.1), generatedMediaFitScale(viewportSize, imageNaturalSize));
    // nextScale preserves the shrinking direction even when the retained scale is below the new fit.
    // 即使保留比例低于新的适配比例，nextScale 也保持缩小方向。
    const nextScale = deltaY < 0 ? boundedScale : Math.min(imageScale, boundedScale);
    setImageOffset((offset) => generatedMediaZoomOffsetAroundPoint(
      offset,
      imageScale,
      nextScale,
      anchor,
      viewportSize,
      imageNaturalSize,
    ));
    setImageScale(nextScale);
  }, [imageNaturalSize, imageScale, imageViewportSize, measureImageViewportSize]);
  React.useEffect(() => {
    const node = imageStageRef.current;
    if (!node || !isImage) {
      return undefined;
    }
    const handleWheel = (event: WheelEvent) => {
      event.preventDefault();
      zoomImageAtPoint(event.deltaY, event.clientX, event.clientY, node);
    };
    node.addEventListener("wheel", handleWheel, { passive: false });
    return () => node.removeEventListener("wheel", handleWheel);
  }, [imageStageNode, isImage, zoomImageAtPoint]);
  const handleImagePointerDown = React.useCallback((event: React.PointerEvent<HTMLDivElement>) => {
    const viewportSize = measureImageViewportSize() ?? imageViewportSize;
    if (event.button !== 0 || !imageNaturalSize || !viewportSize) {
      return;
    }
    event.preventDefault();
    imageDragRef.current = {
      pointerId: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
      startOffset: imageOffset,
    };
    event.currentTarget.setPointerCapture(event.pointerId);
    setIsImageDragging(true);
  }, [imageNaturalSize, imageOffset, imageViewportSize, measureImageViewportSize]);
  const handleImagePointerMove = React.useCallback((event: React.PointerEvent<HTMLDivElement>) => {
    const drag = imageDragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) {
      return;
    }
    event.preventDefault();
    setImageOffset(clampGeneratedMediaImageOffset({
      x: drag.startOffset.x + event.clientX - drag.startX,
      y: drag.startOffset.y + event.clientY - drag.startY,
    }, imageScale, imageViewportSize, imageNaturalSize));
  }, [imageNaturalSize, imageScale, imageViewportSize]);
  const handleImagePointerEnd = React.useCallback((event: React.PointerEvent<HTMLDivElement>) => {
    const drag = imageDragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) {
      return;
    }
    imageDragRef.current = null;
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    setIsImageDragging(false);
  }, []);

  return (
    <Dialog open onOpenChange={(open) => !open && props.onClose()}>
      <DialogContent
        className={cn(
          "generated-media-player-window",
          isMaximized && "maximized",
        )}
        showCloseButton={false}
      >
        <header className="generated-media-player-header">
          <span className="generated-media-player-title">
            <MediaIcon size={18} />
            <DialogTitle title={props.attachment.fileName}>{props.attachment.fileName}</DialogTitle>
            <DialogDescription className="sr-only">{props.locale === "zh-CN" ? "查看生成的媒体文件，或使用工具栏调整显示。" : "View generated media and adjust its display using the toolbar."}</DialogDescription>
          </span>
          {isImage ? (
            <GeneratedMediaImageZoomControl
              canUseZoom={!loadFailed && imageNaturalSize !== null && imageViewportSize !== null}
              fitScale={imageFitScale}
              naturalSize={imageNaturalSize}
              offset={imageOffset}
              onFit={handleImageFit}
              onOriginalSize={handleImageOriginalSize}
              onZoomIn={() => applyImageScaleAroundCenter(imageScale + GENERATED_MEDIA_ZOOM_STEP)}
              onZoomOut={() => applyImageScaleAroundCenter(imageScale - GENERATED_MEDIA_ZOOM_STEP)}
              scale={imageScale}
            />
          ) : null}
          <span className="generated-media-player-actions">
            <Button
              aria-label={isMaximized ? t("mediaPlayer.restore") : t("mediaPlayer.maximize")}
              className="icon-button"
              onClick={() => setIsMaximized((value) => !value)}
              size="icon"
              type="button"
              variant="outline"
            >
              {isMaximized ? <Minimize2 size={17} /> : <Maximize2 size={17} />}
            </Button>
            <Button aria-label={t("mediaPlayer.close")} className="icon-button" onClick={props.onClose} size="icon" type="button" variant="outline">
              <X size={17} />
            </Button>
          </span>
        </header>
        <div className={cn("generated-media-player-body", isImage && "image")}>
          {loadFailed || !source ? (
            <div className="flex min-h-40 flex-col items-center justify-center gap-3 p-4 text-center" role="alert">
              <p className="text-sm text-muted-foreground">{props.locale === "zh-CN" ? "无法加载此媒体文件。" : "This media file could not be loaded."}</p>
              {source ? <Button variant="outline" onClick={() => { setLoadFailed(false); setLoadAttempt((attempt) => attempt + 1); }}>
                {props.locale === "zh-CN" ? "重试" : "Retry"}
              </Button> : null}
            </div>
          ) : isImage ? (
            <div
              className={cn("generated-media-image-viewer focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-ring", isImageDragging && "dragging")}
              aria-label={props.locale === "zh-CN" ? "图片查看器：方向键平移，Home 自适应窗口" : "Image viewer: arrow keys pan, Home fits the image"}
              tabIndex={0}
              role="region"
              onKeyDown={(event) => {
                if (event.altKey || event.ctrlKey || event.metaKey || !imageNaturalSize || !imageViewportSize) return;
                if (event.key === "Home") { event.preventDefault(); handleImageFit(); return; }
                if (!["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(event.key)) return;
                event.preventDefault();
                // Arrow movement uses the same clamped offsets as pointer panning.
                // 方向键移动与指针平移使用相同的边界限制。
                const step = event.shiftKey ? 100 : 40;
                setImageOffset((offset) => clampGeneratedMediaImageOffset({
                  x: offset.x + (event.key === "ArrowLeft" ? step : event.key === "ArrowRight" ? -step : 0),
                  y: offset.y + (event.key === "ArrowUp" ? step : event.key === "ArrowDown" ? -step : 0),
                }, imageScale, imageViewportSize, imageNaturalSize));
              }}
              onPointerCancel={handleImagePointerEnd}
              onPointerDown={handleImagePointerDown}
              onPointerMove={handleImagePointerMove}
              onPointerUp={handleImagePointerEnd}
              ref={bindImageStage}
            >
              {imageNaturalSize ? (
                <div
                  className="generated-media-image-canvas"
                  style={{
                    transform: `translate(calc(-50% + ${imageOffset.x}px), calc(-50% + ${imageOffset.y}px))`,
                  }}
                >
                  <img
                    key={loadAttempt}
                    alt={props.attachment.fileName}
                    className="generated-media-player-image"
                    draggable={false}
                    onLoad={(event) => syncImageNaturalSize(event.currentTarget)}
                    onError={() => setLoadFailed(true)}
                    ref={imageElementRef}
                    src={source}
                    style={{
                      height: imageNaturalSize.height,
                      transform: `scale(${imageScale})`,
                      width: imageNaturalSize.width,
                    }}
                  />
                </div>
              ) : (
                <img
                  key={loadAttempt}
                  alt={props.attachment.fileName}
                  className="generated-media-player-image loading"
                  draggable={false}
                  onLoad={(event) => syncImageNaturalSize(event.currentTarget)}
                  onError={() => setLoadFailed(true)}
                  ref={imageElementRef}
                  src={source}
                />
              )}
            </div>
          ) : props.attachment.kind === "video" ? (
            <video
              key={loadAttempt}
              aria-label={props.attachment.fileName}
              onError={() => setLoadFailed(true)}
              className="generated-media-player-video"
              controls
              preload="metadata"
              src={source}
            />
          ) : props.attachment.kind === "audio" ? (
            <audio
              key={loadAttempt}
              aria-label={props.attachment.fileName}
              onError={() => setLoadFailed(true)}
              className="generated-media-player-audio"
              controls
              preload="metadata"
              src={source}
            />
          ) : (
            <a className="generated-media-document-link" href={source} rel="noreferrer" target="_blank">
              <FileText size={18} />
              {props.attachment.fileName}
            </a>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}

// GeneratedMediaImageZoomControl renders the lightweight image zoom toolbar in the dialog header.
// GeneratedMediaImageZoomControl 在弹窗标题栏渲染轻量图片缩放工具条。
function GeneratedMediaImageZoomControl(props: {
  // Whether the image has enough measurements to accept zoom actions.
  // 图片是否已有足够尺寸信息来接受缩放操作。
  canUseZoom: boolean;
  // Fit-to-window scale relative to the image's original size.
  // 相对图片原始尺寸的自适应窗口缩放比例。
  fitScale: number;
  // naturalSize is the loaded image's original pixel size.
  // naturalSize 是已加载图片的原始像素尺寸。
  naturalSize: GeneratedMediaImageSize | null;
  // offset is the current pan offset.
  // offset 是当前拖拽平移偏移。
  offset: GeneratedMediaImageOffset;
  // Restores fit-to-window scale and centered position.
  // 恢复自适应窗口缩放与居中位置。
  onFit: () => void;
  // Restores original image size and centered position.
  // 恢复图片原始尺寸与居中位置。
  onOriginalSize: () => void;
  // Increases the image scale.
  // 放大图片。
  onZoomIn: () => void;
  // Decreases the image scale.
  // 缩小图片。
  onZoomOut: () => void;
  // Current scale relative to the image's original size.
  // 当前相对图片原始尺寸的缩放比例。
  scale: number;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  const hasOffset = generatedMediaHasImageOffset(props.offset);
  const percent = Math.round(props.scale * 100);
  const canUseZoom = props.canUseZoom && props.naturalSize !== null;
  // Disabled state and execution share the same fit-aware minimum.
  // 禁用状态与执行使用相同的适配下限。
  const canZoomOut = canUseZoom && props.scale > Math.min(GENERATED_MEDIA_MIN_ZOOM, props.fitScale) + 0.001;
  const canZoomIn = canUseZoom && props.scale < GENERATED_MEDIA_MAX_ZOOM - 0.001;
  const canFitWindow = canUseZoom && (Math.abs(props.scale - props.fitScale) > 0.001 || hasOffset);
  const canUseOriginalSize = canUseZoom && (Math.abs(props.scale - 1) > 0.001 || hasOffset);
  return (
    <span className="generated-media-image-zoom-control">
      <GeneratedMediaImageZoomIconButton
        disabled={!canFitWindow}
        icon={<Scan size={16} />}
        label={t("mediaPlayer.fitWindow")}
        onClick={props.onFit}
      />
      <GeneratedMediaImageZoomIconButton
        disabled={!canUseOriginalSize}
        icon={<SquareIcon size={15} />}
        label={t("mediaPlayer.originalSize")}
        onClick={props.onOriginalSize}
      />
      <GeneratedMediaImageZoomIconButton
        disabled={!canZoomOut}
        icon={<Minus size={16} />}
        label={t("mediaPlayer.zoomOut")}
        onClick={props.onZoomOut}
      />
      <strong>{canUseZoom ? `${percent}%` : "-"}</strong>
      <GeneratedMediaImageZoomIconButton
        disabled={!canZoomIn}
        icon={<Plus size={16} />}
        label={t("mediaPlayer.zoomIn")}
        onClick={props.onZoomIn}
      />
    </span>
  );
}

// GeneratedMediaImageZoomIconButton renders one icon-only image zoom toolbar action with a tooltip.
// GeneratedMediaImageZoomIconButton 渲染带提示的单个纯图标图片缩放工具按钮。
function GeneratedMediaImageZoomIconButton(props: {
  // Whether the action is currently unavailable.
  // 当前操作是否不可用。
  disabled: boolean;
  // Icon rendered inside the toolbar button.
  // 工具按钮内渲染的图标。
  icon: React.ReactNode;
  // Tooltip and accessibility label.
  // Tooltip 与无障碍标签。
  label: string;
  // Click handler for the toolbar action.
  // 工具按钮点击处理函数。
  onClick: () => void;
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span className="generated-media-image-zoom-trigger">
          <Button
            aria-label={props.label}
            className="icon-button"
            disabled={props.disabled}
            onClick={props.onClick}
            size="icon-sm"
            title={props.label}
            type="button"
            variant="ghost"
          >
            {props.icon}
          </Button>
        </span>
      </TooltipTrigger>
      <TooltipContent sideOffset={6}>{props.label}</TooltipContent>
    </Tooltip>
  );
}
