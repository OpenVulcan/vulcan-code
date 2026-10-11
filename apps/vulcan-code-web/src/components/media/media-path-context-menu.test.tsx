// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { displayedImageSource } from "./media-path-context-menu";

// trigger builds one detached menu trigger element from markup.
// trigger 根据标记构建一个游离的菜单触发元素。
function trigger(markup: string): HTMLElement {
  const container = document.createElement("div");
  container.innerHTML = markup;
  return container;
}

describe("displayedImageSource", () => {
  it("returns the image under the pointer", () => {
    const container = trigger('<figure><img src="http://127.0.0.1:9/asset/a.png"><figcaption>a</figcaption></figure>');
    expect(displayedImageSource(container.querySelector("img"), container)).toBe("http://127.0.0.1:9/asset/a.png");
  });

  it("uses the trigger's sole image when the pointer lands on its frame", () => {
    const container = trigger('<button><img src="http://127.0.0.1:9/asset/b.png"><span>b.png</span></button>');
    expect(displayedImageSource(container.querySelector("span"), container)).toBe("http://127.0.0.1:9/asset/b.png");
  });

  it("offers no image for video or audio triggers", () => {
    const video = trigger('<div><video src="http://127.0.0.1:9/asset/c.mp4"></video><img src="http://127.0.0.1:9/asset/poster.png"></div>');
    expect(displayedImageSource(video.querySelector("video"), video)).toBeNull();
    const audio = trigger('<div><audio src="http://127.0.0.1:9/asset/d.mp3"></audio></div>');
    expect(displayedImageSource(audio.querySelector("audio"), audio)).toBeNull();
  });
});
