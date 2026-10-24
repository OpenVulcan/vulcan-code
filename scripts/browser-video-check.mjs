// Record a short video through a packaged browser runtime to prove its ffmpeg component works.
// 通过已打包的浏览器运行时录制一段短视频，以证明其 ffmpeg 组件可用。
// Usage: node browser-video-check.mjs <browser runtime root> <chromium executable>
// The caller sets PLAYWRIGHT_BROWSERS_PATH exactly as the browser host does.
// 调用方按浏览器宿主的方式设置 PLAYWRIGHT_BROWSERS_PATH。
import { mkdtemp, readdir, rm, stat } from 'node:fs/promises';
import { createRequire } from 'node:module';
import os from 'node:os';
import path from 'node:path';

const [runtimeRoot, chromiumExecutable] = process.argv.slice(2);
if (!runtimeRoot || !chromiumExecutable) throw new Error('Usage: browser-video-check.mjs <runtime root> <chromium executable>');
// playwright-core is the packaged copy, so its registry resolves the packaged components.
// playwright-core 是打包副本，因此其注册表解析的是随包组件。
const { chromium } = createRequire(path.join(runtimeRoot, 'package.json'))('playwright-core');
const videoDirectory = await mkdtemp(path.join(os.tmpdir(), 'vulcan-video-check-'));
try {
  const browser = await chromium.launch({ executablePath: chromiumExecutable, headless: true });
  try {
    const context = await browser.newContext({ recordVideo: { dir: videoDirectory } });
    const page = await context.newPage();
    await page.setContent('<h1>Vulcan video check</h1>');
    await page.waitForTimeout(500);
    // Closing the context finalizes the recording through ffmpeg.
    // 关闭上下文时由 ffmpeg 完成录像文件。
    await context.close();
  } finally {
    await browser.close();
  }
  const recordings = await readdir(videoDirectory);
  const sizes = await Promise.all(recordings.map(name => stat(path.join(videoDirectory, name)).then(item => item.size)));
  if (recordings.length !== 1 || sizes[0] === 0) {
    throw new Error(`Expected one non-empty recording, found ${recordings.length} file(s).`);
  }
} finally {
  await rm(videoDirectory, { recursive: true, force: true });
}
