// Enforce the predeclared browser budget against the same ordinary Markdown renderer.
// 对相同普通 Markdown 渲染器执行预先声明的浏览器预算。
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { realpath, writeFile } from 'node:fs/promises';
import { parseArgs } from 'node:util';
import { chromium } from 'playwright-core';

// Exact artifacts are explicit; no service discovery or real user data is used.
// 精确产物由调用方显式提供；不使用服务发现或真实用户数据。
const { values } = parseArgs({ options: { fixture: { type: 'string' }, bundle: { type: 'string' }, chromium: { type: 'string' }, report: { type: 'string' } } });
assert.ok(values.fixture && values.bundle && values.chromium && values.report);
const child = spawn(values.fixture, [values.bundle], { windowsHide: true, stdio: ['ignore', 'pipe', 'inherit'] });
const ready = new Promise((resolve, reject) => {
  const lines = createInterface({ input: child.stdout });
  child.once('error', reject); child.once('exit', () => reject(new Error('Synthetic fixture stopped')));
  lines.once('line', (line) => { lines.close(); resolve(JSON.parse(line)); });
});
let browser;
try {
  const { viewer } = await ready;
  browser = await chromium.launch({ executablePath: await realpath(values.chromium), headless: true });
  const page = await browser.newPage();
  let grants = 0;
  let errors = 0;
  page.on('request', (request) => { if (new URL(request.url()).pathname === '/rpc') grants += 1; });
  page.on('pageerror', () => { errors += 1; });
  await page.goto(`${viewer}/scripts/fixtures/html-answers.html?service=${encodeURIComponent(viewer)}&performance=1`);
  await page.locator('#performance-result').waitFor({ timeout: 60000 });
  const raw = JSON.parse(await page.locator('#performance-result').textContent());
  await page.getByRole('button', { name: '打开 HTML 回答', exact: true }).last().waitFor();
  const unopened = await page.getByRole('button', { name: '打开 HTML 回答', exact: true }).count();
  const frames = await page.locator('iframe').count();
  // Nearest-rank P95 and even-sample median follow the frozen 20-sample measurement contract.
  // 最近秩 P95 与偶数样本中位数遵循冻结的二十样本测量契约。
  const stats = (values) => { const sorted = [...values].sort((a, b) => a - b); return { medianMs: (sorted[9] + sorted[10]) / 2, p95Ms: sorted[18] }; };
  const baseline = stats(raw.baseline);
  const enabled = stats(raw.enabled);
  const passed = enabled.medianMs <= baseline.medianMs * 1.10 + 2 && enabled.p95Ms <= baseline.p95Ms * 1.10 + 2
    && unopened === 100 && frames === 0 && grants === 0 && errors === 0;
  const result = { browser: await browser.version(), ...raw, baseline, enabled, unopened, frames, grants, errors, passed };
  await writeFile(values.report, `${JSON.stringify(result, null, 2)}\n`);
  process.stdout.write(`${JSON.stringify(result)}\n`);
  assert.ok(passed, 'frozen rendering budget or inert-card behavior failed');
} finally {
  if (browser) await browser.close();
  child.kill();
}
