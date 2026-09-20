import { expect, it } from "vitest";
import { localBrowserAddress, localBrowserTabLabel } from "./ClientBrowserArea";

// Local addresses use HTTP while ordinary domains use HTTPS and privileged schemes stay blocked.
// 本机地址使用 HTTP，普通域名使用 HTTPS，且拒绝特权协议。
it("normalizes GUI-local browser addresses for development sites", () => {
  expect(localBrowserAddress("localhost:5173/app")).toBe("http://localhost:5173/app");
  expect(localBrowserAddress("127.0.0.1:61449/")).toBe("http://127.0.0.1:61449/");
  expect(localBrowserAddress("www.baidu.com")).toBe("https://www.baidu.com/");
  expect(() => localBrowserAddress("file:///private.txt")).toThrow();
});

// Long native page titles keep a bounded tab label and retain a distinct blank-page label.
// 较长的原生页面标题只占有限标签空间，空白页也有独立的标签名称。
it("bounds GUI-local tab titles", () => {
  expect(Array.from(localBrowserTabLabel("很长的浏览器标题".repeat(8), true))).toHaveLength(24);
  expect(localBrowserTabLabel("about:blank", true)).toBe("新标签页");
});
