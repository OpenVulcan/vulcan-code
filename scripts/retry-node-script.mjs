// Run one Node download script again while it fails, waiting RETRY_DELAYS_MS between attempts: GitHub release
// downloads answer 504 in bursts that can last several minutes. The last failure's exit status is returned.
// 在一个 Node 下载脚本失败时重新运行，两次之间按 RETRY_DELAYS_MS 等待：GitHub 发布下载会成串地返回 504，可持续
// 数分钟。返回最后一次失败的退出码。
//
//   node scripts/retry-node-script.mjs <script> [arguments...]
import { spawnSync } from 'node:child_process';
import { RETRY_DELAYS_MS } from './locked-download.mjs';

const [script, ...args] = process.argv.slice(2);
if (!script) throw new Error('Usage: retry-node-script <script> [arguments...]');
for (let attempt = 0; ; attempt += 1) {
  const result = spawnSync(process.execPath, [script, ...args], { stdio: 'inherit' });
  if (result.status === 0) process.exit(0);
  if (attempt >= RETRY_DELAYS_MS.length) process.exit(result.status ?? 1);
  console.warn(`${script} failed; running it again in ${RETRY_DELAYS_MS[attempt] / 1000} s.`);
  await new Promise(resolve => setTimeout(resolve, RETRY_DELAYS_MS[attempt]));
}
