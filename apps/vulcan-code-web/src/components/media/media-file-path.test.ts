import { describe, expect, it } from "vitest";
import { encodeBase64UrlUtf8 } from "@/resources/localMedia";
import { resourceId } from "@/service/conversation-event-window-store";
import { mediaFileLocator } from "./media-file-path";

describe("mediaFileLocator", () => {
  it("maps a session resource content URL to its owning session and resource", () => {
    expect(mediaFileLocator("http://127.0.0.1:9/v1/session-resources/abc-1/42?token=t")).toEqual({
      kind: "resource", sessionId: "abc-1", resourceId: resourceId(42),
    });
  });

  it("rejects a session resource URL whose resource id is not a number", () => {
    expect(mediaFileLocator("http://127.0.0.1:9/v1/session-resources/abc-1/x42")).toBeNull();
  });

  it("decodes an encoded local media route back to its host path", () => {
    const path = "D:\\shots\\畫面.png";
    expect(mediaFileLocator(`vulcan-media://local/asset/${encodeBase64UrlUtf8(path)}`)).toEqual({ kind: "path", path });
  });

  it("keeps a plain host path and gives remote or inline sources no file", () => {
    expect(mediaFileLocator("/tmp/clip.mp4")).toEqual({ kind: "path", path: "/tmp/clip.mp4" });
    expect(mediaFileLocator("https://example.com/a.png")).toBeNull();
    expect(mediaFileLocator("data:image/png;base64,AAAA")).toBeNull();
    expect(mediaFileLocator("  ")).toBeNull();
  });
});
