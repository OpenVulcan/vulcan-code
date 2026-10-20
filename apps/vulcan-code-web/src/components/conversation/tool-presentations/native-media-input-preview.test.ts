import { describe, expect, it } from "vitest";
import { nativeMediaInputPreview } from "./native-media-input-preview";
import { v1ModelResult } from "./v1-tool-domain-presentation";

describe("native media input display", () => {
  it("shows the receipt without copying media bytes or private URLs into the result card", () => {
    // The persisted host envelope retains transport-only data beside its safe display text.
    // 持久宿主信封在安全展示文字旁保留仅供传输的数据。
    const output = JSON.stringify({ $vulcan_native_media_input: 1, text: "Read media: photo.png", resource: { url: "data:image/png;base64,cHJpdmF0ZQ==" } });
    expect(nativeMediaInputPreview(output)).toBe("Read media: photo.png");
    expect(v1ModelResult({ model_output: output })).toBe("Read media: photo.png");
  });
  it("preserves ordinary output and rejects unsupported marked versions", () => {
    expect(nativeMediaInputPreview("ordinary text")).toBeUndefined();
    expect(nativeMediaInputPreview({ text: "ordinary JSON" })).toBeUndefined();
    const rejected = nativeMediaInputPreview({ $vulcan_native_media_input: 2, text: "private result" });
    expect(rejected).toEqual(expect.any(String));
    expect(rejected).not.toContain("private result");
  });
});
