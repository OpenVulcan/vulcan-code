// Export existing runtime version authorities to GitHub Actions without duplicating version literals.
// 向 GitHub Actions 导出现有运行时版本权威，不重复版本字面量。
import { appendFile, readFile } from 'node:fs/promises';

// versions is the same manifest used by Core's managed runtime installer.
// versions 是 Core 受管运行时安装器使用的同一清单。
const versions = JSON.parse(await readFile(new URL('../crates/vulcan-service-runtime/managed-runtimes.json', import.meta.url), 'utf8'));
if (!process.env.GITHUB_OUTPUT) throw new Error('GITHUB_OUTPUT is required.');
await appendFile(process.env.GITHUB_OUTPUT, `node=${versions.node_version}\npnpm=${versions.pnpm_version}\n`);
