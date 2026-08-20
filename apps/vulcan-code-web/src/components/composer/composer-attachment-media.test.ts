import { describe, expect, it } from "vitest";

import { inferComposerAttachmentKindFromFile } from "./composer-attachment-media";

// browserFile builds the minimum browser File contract used by attachment-kind inference.
// browserFile 构建附件类型推断使用的最小浏览器 File 契约。
// Parameters: name is the display file name and type is optional browser MIME metadata.
// 参数：name 是展示文件名，type 是可选的浏览器 MIME 元数据。
// Returns: a test-only File-shaped value without allocating browser-owned bytes.
// 返回值：不分配浏览器托管字节的测试专用 File 形状值。
function browserFile(name: string, type = ""): File {
  return { name, type } as File;
}

describe("composer attachment media", () => {
  // The full host video extension registry remains selectable even when a browser omits MIME metadata.
  // 当浏览器省略 MIME 元数据时，完整宿主视频扩展注册表仍然可选择。
  it("recognizes every registered video extension without MIME metadata", () => {
    // extension iterates one exact host-registered video suffix at a time.
    // extension 每次迭代一个宿主精确注册的视频后缀。
    for (const extension of ["mp4", "mpeg", "mpg", "mov", "m4v", "mkv", "webm", "avi", "flv", "wmv", "3gp", "3gpp", "3g2"]) {
      expect(inferComposerAttachmentKindFromFile(browserFile(`clip.${extension}`))).toBe("video");
    }
  });

  // The full host audio extension registry remains selectable even when a browser omits MIME metadata.
  // 当浏览器省略 MIME 元数据时，完整宿主音频扩展注册表仍然可选择。
  it("recognizes every registered audio extension without MIME metadata", () => {
    // extension iterates one exact host-registered audio suffix at a time.
    // extension 每次迭代一个宿主精确注册的音频后缀。
    for (const extension of ["mp3", "wav", "m4a", "aac", "flac", "ogg", "oga", "opus", "pcm", "ulaw"]) {
      expect(inferComposerAttachmentKindFromFile(browserFile(`voice.${extension}`))).toBe("audio");
    }
  });

  // MIME metadata still admits a media file whose display name has no useful extension.
  // MIME 元数据仍会准入展示名称不含有效扩展名的媒体文件。
  it("uses browser MIME metadata before extension fallback", () => {
    expect(inferComposerAttachmentKindFromFile(browserFile("recording", "audio/opus"))).toBe("audio");
    expect(inferComposerAttachmentKindFromFile(browserFile("capture", "video/3gpp2"))).toBe("video");
  });
});
