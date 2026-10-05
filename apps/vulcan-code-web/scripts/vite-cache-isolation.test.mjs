// Verify actual Vite configuration boundaries without starting or stopping any user or audit server.
// 验证真实 Vite 配置边界，不启动或停止任何用户或验收服务。
import assert from "node:assert/strict";
import test from "node:test";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { resolveConfig } from "vite";

// appRoot fixes the production root independently of the invoking terminal's working directory.
// appRoot 固定正式应用根目录，不依赖调用终端的工作目录。
const appRoot = fileURLToPath(new URL("../", import.meta.url));

// Resolve real development and unit-test configs and assert optimizer output ownership is disjoint.
// 解析真实开发与单元测试配置，断言依赖预构建产物归属互不重叠。
// No arguments; returns after the Vite resolver proves both workbench cache directories are exclusive.
// 无参数；Vite 解析器证明两个工作台缓存目录均独占后返回。
test("workbench React cache is isolated from unit tests and the live acceptance server", async () => {
  // The production config has no root override, so supply the same root as its normal package invocation.
  // 正式配置没有覆盖根目录，因此传入正常包内调用所使用的同一根目录。
  const development = await resolveConfig({ root: appRoot, configFile: path.join(appRoot, "vite.config.ts"), mode: "development" }, "serve");
  const unitTests = await resolveConfig({ root: appRoot, configFile: path.join(appRoot, "vite.config.ts"), mode: "test" }, "serve");
  assert.notEqual(development.cacheDir, unitTests.cacheDir);
  // tests/vce-live needs a private backend receipt to load, so compare against the package-default cache it resolves to.
  // tests/vce-live 需要私有后端回执才能加载，因此与其解析得到的包级默认缓存位置比较。
  const acceptanceCacheDir = path.join(appRoot, "node_modules", ".vite");
  for (const workbench of [development, unitTests]) {
    // Both sibling and nested cache reuse are rejected because either can admit foreign optimizer files.
    // 同目录和嵌套缓存复用均拒绝，因为二者都可能引入外部依赖预构建文件。
    for (const relative of [path.relative(acceptanceCacheDir, workbench.cacheDir), path.relative(workbench.cacheDir, acceptanceCacheDir)]) {
      assert.ok(relative.startsWith(`..${path.sep}`), `tests/vce-live overlaps workbench cache: ${workbench.cacheDir}`);
    }
  }
});
