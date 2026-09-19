"""Verify a pinned Browser Use session against one Core-granted browser-level CDP socket.
验证固定版本 Browser Use 会话能否接入一条 Core 授权浏览器级 CDP 套接字。
"""

import asyncio
import json
import logging
import os


async def main() -> None:
    """Navigate and observe one local fixture without starting another browser process.
    导航并观察一个本地夹具，不启动第二个浏览器进程。

    Parameters: none; exact endpoint and page URL come from the managed probe process.
    参数：无；精确端点与页面地址由受管理探针进程提供。
    Returns: none; writes a path-free verification result to standard output.
    返回：无；向标准输出写入不含路径的验证结果。
    """
    logging.disable(logging.CRITICAL)
    from browser_use import BrowserSession

    endpoint = os.environ["VULCAN_BROWSER_CDP_URL"]
    fixture_url = os.environ["VULCAN_BROWSER_TEST_URL"]
    browser = BrowserSession(cdp_url=endpoint, is_local=False)
    try:
        await browser.start()
        await browser.navigate_to(fixture_url)
        state = await browser.get_browser_state_summary()
        print(json.dumps({
            "result": "passed",
            "url": state.url,
            "title": state.title,
            "dom": "Browser Use fixture" in state.dom_state.llm_representation(),
        }))
    finally:
        await browser.stop()


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except Exception as error:
        secret_endpoint = os.environ.get("VULCAN_BROWSER_CDP_URL", "")
        message = str(error).replace(secret_endpoint, "<redacted>") if secret_endpoint else str(error)
        print(json.dumps({"result": "failed", "errorType": type(error).__name__, "message": message}))
        raise SystemExit(1) from None
