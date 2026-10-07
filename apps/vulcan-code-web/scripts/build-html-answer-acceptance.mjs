// Build a separate local acceptance bundle; production entrypoints never include test fixtures.
// 构建独立本地验收包；正式入口绝不包含测试夹具。
import { build } from 'vite';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

// Paths resolve from this script, with all output restricted to the repository's ignored target area.
// 路径依据当前脚本解析，全部产物限制在仓库忽略的 target 区域。
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
await build({ root, configFile: path.join(root, 'vite.config.ts'), build: {
  outDir: path.resolve(root, '../../target/html-answer-card-bundle'), emptyOutDir: true,
  rollupOptions: { input: { htmlAnswerAcceptance: path.join(root, 'scripts/fixtures/html-answers.html') } },
} });
