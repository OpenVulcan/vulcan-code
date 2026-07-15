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
  const { t } = useI18n();
  const imageStageRef = React.useRef<HTMLDivElement | null>(null);
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
    const rect = node.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) {
      return null;
    }
    const nextSize = { width: rect.width, height: rect.height };
    setImageViewportSize((current) => (
      current && Math.abs(current.width - nextSize.width) < 0.5 && Math.abs(current.height - nextSize.height) < 0.5
        ? current
        : nextSize
    ));
    return nextSize;
  }, [isImage]);
  React.useEffect(() => {
    imageInitializedRef.current = false;
    imageDragRef.current = null;
    imageElementRef.current = null;
    setImageScale(1);
    setImageOffset({ x: 0, y: 0 });
    setIsImageDragging(false);
    setIsMaximized(false);
    setImageNaturalSize(null);
    setImageViewportSize(null);
  }, [source]);
  React.useEffect(() => {
    if (!isImage || !source) {
      return undefined;
    }
    let cancelled = false;
    const image = new Image();
    image.onload = () => {
      if (!cancelled) {
        setImageNaturalSize({ width: image.naturalWidth, height: image.naturalHeight });
      }
    };
    image.src = source;
    return () => {
      cancelled = true;
    };
  }, [isImage, source]);
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
  }, [isImage, isMaximized, measureImageViewportSize]);
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
    const clampedScale = clampGeneratedMediaZoom(nextScale);
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
    const nextScale = clampGeneratedMediaZoom(imageScale * (deltaY < 0 ? 1.1 : 1 / 1.1));
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
  }, [isImage, zoomImageAtPoint]);
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
          </span>
          <span className="generated-media-player-actions">
            {isImage ? (
              <GeneratedMediaImageZoomControl
                canUseZoom={imageNaturalSize !== null && imageViewportSize !== null}
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
          {isImage ? (
            <div
              className={cn("generated-media-image-viewer", isImageDragging && "dragging")}
              onPointerCancel={handleImagePointerEnd}
              onPointerDown={handleImagePointerDown}
              onPointerMove={handleImagePointerMove}
              onPointerUp={handleImagePointerEnd}
              ref={imageStageRef}
            >
              {imageNaturalSize ? (
                <div
                  className="generated-media-image-canvas"
                  style={{
                    transform: `translate(calc(-50% + ${imageOffset.x}px), calc(-50% + ${imageOffset.y}px))`,
                  }}
                >
                  <img
                    alt={props.attachment.fileName}
                    className="generated-media-player-image"
                    draggable={false}
                    onLoad={(event) => syncImageNaturalSize(event.currentTarget)}
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
                  alt={props.attachment.fileName}
                  className="generated-media-player-image loading"
                  draggable={false}
                  onLoad={(event) => syncImageNaturalSize(event.currentTarget)}
                  ref={imageElementRef}
                  src={source}
                />
              )}
            </div>
          ) : props.attachment.kind === "video" ? (
            <video
              className="generated-media-player-video"
              controls
              preload="metadata"
              src={source}
            />
          ) : props.attachment.kind === "audio" ? (
            <audio
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
  const canZoomOut = canUseZoom && props.scale > GENERATED_MEDIA_MIN_ZOOM + 0.001;
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
      <strong>{percent}%</strong>
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
