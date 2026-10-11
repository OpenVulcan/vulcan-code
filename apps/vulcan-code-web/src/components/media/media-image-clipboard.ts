/** The only image type the asynchronous clipboard accepts across Chromium-based clients.
 * Chromium 系客户端的异步剪贴板唯一接受的图片类型。 */
const CLIPBOARD_IMAGE_TYPE = "image/png";

/** Copies one displayed image to the clipboard as PNG.
 * 把一张展示中的图片以 PNG 形式复制到剪贴板。
 *
 * The clipboard write starts synchronously with a pending PNG, so the user's menu activation is
 * still valid while the image bytes are fetched and converted.
 * 剪贴板写入以待定 PNG 同步发起，因此在获取并转换图片字节期间，用户的菜单激活仍然有效。
 *
 * @param source the URL the image element currently displays.
 *               图片元素当前展示的 URL。
 * @returns resolves after the clipboard accepts the image; rejects when the client cannot write images
 *          or the image cannot be fetched or decoded.
 *          剪贴板接受图片后完成；客户端无法写入图片，或图片无法获取或解码时拒绝。 */
export async function copyImageToClipboard(source: string): Promise<void> {
  if (typeof navigator.clipboard?.write !== "function" || typeof ClipboardItem === "undefined") {
    throw new Error("Image clipboard writing is unavailable in this client.");
  }
  await navigator.clipboard.write([
    new ClipboardItem({ [CLIPBOARD_IMAGE_TYPE]: fetchImageAsPng(source) }),
  ]);
}

/** Fetches one image and returns it as a PNG blob, converting other formats through a canvas.
 * 获取一张图片并以 PNG blob 返回，其他格式经画布转换。 */
async function fetchImageAsPng(source: string): Promise<Blob> {
  const response = await fetch(source);
  if (!response.ok) {
    throw new Error(`Image request failed with HTTP ${response.status}.`);
  }
  const image = await response.blob();
  if (image.type === CLIPBOARD_IMAGE_TYPE) {
    return image;
  }
  const bitmap = await createImageBitmap(image);
  try {
    const canvas = document.createElement("canvas");
    canvas.width = bitmap.width;
    canvas.height = bitmap.height;
    const context = canvas.getContext("2d");
    if (!context) {
      throw new Error("A 2D canvas is unavailable for image conversion.");
    }
    context.drawImage(bitmap, 0, 0);
    return await new Promise<Blob>((resolve, reject) => {
      canvas.toBlob(
        (png) => (png ? resolve(png) : reject(new Error("The image could not be encoded as PNG."))),
        CLIPBOARD_IMAGE_TYPE,
      );
    });
  } finally {
    bitmap.close();
  }
}
