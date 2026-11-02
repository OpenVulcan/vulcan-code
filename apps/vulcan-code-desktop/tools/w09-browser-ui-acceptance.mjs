// Verify that the real desktop WebUI can observe and drive a Core-owned browser page.
// 验证真实桌面 WebUI 能观察并驱动 Core 拥有的浏览器页面。
import assert from 'node:assert/strict';
import { execFile, spawn } from 'node:child_process';
import { randomBytes, randomUUID } from 'node:crypto';
import { createServer } from 'node:http';
import { cp, link, mkdir, mkdtemp, readFile, readdir, realpath, rm, symlink, unlink, writeFile } from 'node:fs/promises';
import { createConnection } from 'node:net';
import { homedir, tmpdir } from 'node:os';
import path from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { _electron as electron, chromium } from 'playwright-core';

// execFileAsync runs the Windows toast-history probe without shell interpolation of test paths.
// execFileAsync 不通过 Shell 插值测试路径来运行 Windows 通知历史探针。
const execFileAsync = promisify(execFile);

// readBrandedDebugToastTags observes only one executable-owned debug app's native toast IDs.
// readBrandedDebugToastTags 只观察一个可执行程序专属调试应用的原生通知 ID。
// appUserModelId selects the exact path-derived debug identity; the return value contains its tags.
// appUserModelId 选择精确的路径派生调试身份；返回值包含该身份的通知标签。
async function readBrandedDebugToastTags(appUserModelId) {
  const command = '$m=[Windows.UI.Notifications.ToastNotificationManager,Windows.UI.Notifications,ContentType=WindowsRuntime];'
    + '$m::History.GetHistory($env:VULCAN_W09_AUMID) | ForEach-Object { $_.Tag }';
  const { stdout } = await execFileAsync('powershell.exe', ['-NoProfile', '-Command', command], {
    env: { ...process.env, VULCAN_W09_AUMID: appUserModelId },
    timeout: 10_000,
  });
  return stdout.split(/\r?\n/).map((tag) => tag.trim()).filter(Boolean);
}

// desktop is the source desktop package containing the Electron runtime and GUI entry.
// desktop 是包含 Electron 运行时和 GUI 入口的源码桌面包。
const desktop = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
// repository is the source checkout containing the isolated-test-support service binary.
// repository 是包含隔离测试支持服务程序的源码工作区。
const repository = path.resolve(desktop, '../..');
// Explicit integration inputs bind this probe to the binaries selected by its caller.
// 显式集成输入将探针绑定到调用者选择的程序文件。
const selectedServiceExecutable = process.env.VULCAN_W09_SERVICE_EXE
  ? path.resolve(process.env.VULCAN_W09_SERVICE_EXE) : path.join(repository, 'target/debug/vulcan-code-service.exe');
const selectedDesktopRuntime = process.env.VULCAN_W09_DESKTOP_RUNTIME
  ? path.resolve(process.env.VULCAN_W09_DESKTOP_RUNTIME)
  : path.join(desktop, 'node_modules/electron/dist/electron.exe');
const selectedDesktopEntry = process.env.VULCAN_W09_DESKTOP_ENTRY
  ? path.resolve(process.env.VULCAN_W09_DESKTOP_ENTRY) : path.join(desktop, 'dist/gui/main.js');
// assetRoot may select a fresh isolated distribution so the test never observes stale output assets.
// assetRoot 可选择新的隔离发行目录，使测试绝不读取陈旧的 output 资源。
const assetRoot = process.env.VULCAN_W09_ASSET_ROOT
  ? path.resolve(process.env.VULCAN_W09_ASSET_ROOT) : path.join(repository, 'output');
// Device-scale forcing exercises Electron's high-DPI layout without changing the user's OS setting.
// 强制设备缩放可在不修改用户系统设置的情况下检验 Electron 高 DPI 布局。
const requestedDeviceScale = process.env.VULCAN_W09_DEVICE_SCALE;
assert.ok(!requestedDeviceScale || ['1.25', '1.5'].includes(requestedDeviceScale),
  'VULCAN_W09_DEVICE_SCALE must be 1.25 or 1.5.');
// electronScaleArgs applies the same native compositor scale to every GUI launched by this test.
// electronScaleArgs 对此测试启动的每个 GUI 施加相同的原生合成器缩放。
const electronScaleArgs = requestedDeviceScale ? [`--force-device-scale-factor=${requestedDeviceScale}`] : [];
if (process.env.VULCAN_W09_ASSET_ROOT) {
  const relative = path.relative(path.join(repository, 'target'), assetRoot);
  assert.ok(relative && !relative.startsWith('..') && !path.isAbsolute(relative),
    'VULCAN_W09_ASSET_ROOT must be inside the repository target directory.');
}
// root isolates all Core and GUI state from actual user data.
// root 将全部 Core 和 GUI 状态与真实用户数据隔离。
const cloneModelConfig = Boolean(process.env.VULCAN_W09_AI_CONNECTIONS_SOURCE);
const cloneCredentialDatabase = Boolean(process.env.VULCAN_W09_SQLITE_SOURCE);
assert.equal(cloneModelConfig, cloneCredentialDatabase,
  'Model configuration and credential SQLite backup must be supplied together.');
// manualG14 requires one real Debug model turn and a generated branded native toast identity.
// manualG14 要求真实 Debug 模型回合及本次生成的品牌化原生通知身份。
const manualG14 = process.env.VULCAN_W09_MANUAL_G14 === '1';
// webNotificationProbe verifies a real browser-owned completion notification and desktop isolation.
// webNotificationProbe 验证真实网页会话完成通知及与桌面通知的隔离。
const webNotificationProbe = process.env.VULCAN_W09_WEB_NOTIFICATION === '1';
assert.ok(!webNotificationProbe || cloneModelConfig,
  'Web notification acceptance requires the isolated configured-model test.');
// g14SimulateCom exercises the same native callback when physical notification input is unavailable.
// g14SimulateCom 在无法操作系统通知时检验同一原生回调，但不冒充物理点击。
const g14SimulateCom = process.env.VULCAN_W09_G14_SIMULATE_COM === '1';
assert.ok(!g14SimulateCom || manualG14,
  'G14 COM activation simulation requires the real Debug notification route.');
// g14ReloadBeforeActivation tests a delivered toast after the owning renderer reloads.
// g14ReloadBeforeActivation 测试所属渲染器重载后点击已投递的通知。
const g14ReloadBeforeActivation = process.env.VULCAN_W09_G14_RELOAD === '1';
assert.ok(!g14ReloadBeforeActivation || g14SimulateCom,
  'G14 renderer reload requires the isolated COM activation test.');
// A delayed COM probe distinguishes a fresh banner from an Action Center item after banner expiry.
// 延迟 COM 探针用于区分刚显示的横幅和横幅消失后仍留在通知中心的条目。
const g14ComDelayMs = Number(process.env.VULCAN_W09_G14_COM_DELAY_MS ?? '0');
assert.ok(Number.isInteger(g14ComDelayMs) && g14ComDelayMs >= 0 && g14ComDelayMs <= 30_000
  && (g14ComDelayMs === 0 || g14SimulateCom),
'G14 COM delay must be at most 30000 ms and requires COM simulation.');
if (manualG14) {
  assert.equal(process.platform, 'win32');
  assert.equal(cloneModelConfig, true);
  assert.equal(path.basename(selectedDesktopRuntime).toLowerCase(), 'vulcancodedesktopdebug.exe');
  assert.ok(selectedDesktopRuntime.startsWith(path.join(repository, 'target') + path.sep));
}
// modelOverride selects an existing account and declared model only inside this disposable Core.
// modelOverride 仅在本次可丢弃的 Core 中选择已有账号与已声明模型。
const modelOverride = {
  providerId: process.env.VULCAN_W09_MODEL_PROVIDER,
  modelId: process.env.VULCAN_W09_MODEL_ID,
  connectionId: process.env.VULCAN_W09_CONNECTION_ID,
};
// modelOverrideFieldCount rejects a partial selection that could silently use the wrong account.
// modelOverrideFieldCount 拒绝不完整的选择，防止静默使用错误账号。
const modelOverrideFieldCount = Object.values(modelOverride).filter(Boolean).length;
assert.ok(modelOverrideFieldCount === 0 || (modelOverrideFieldCount === 3 && cloneModelConfig),
  'An AI model override requires provider, model, connection and copied debug configuration.');
// captureProviderTools enables a disposable Core trace of the final DeepSeek wire request.
// captureProviderTools 只在可丢弃 Core 中追踪最终发给 DeepSeek 的请求工具定义。
const captureProviderTools = process.env.VULCAN_W09_CAPTURE_PROVIDER_TOOLS === '1';
assert.ok(!captureProviderTools || cloneModelConfig,
  'Provider tool capture requires the isolated configured-model acceptance.');
// Private AI acceptance data stays under the user profile instead of a shared temp volume.
// 私有 AI 验收数据保留在用户目录下，不放入共享临时卷。
const suppliedTestRoot = process.env.VULCAN_W09_TEST_ROOT;
const suppliedEvidenceDirectory = process.env.VULCAN_W09_EVIDENCE_DIR;
assert.equal(Boolean(suppliedTestRoot), Boolean(suppliedEvidenceDirectory),
  'An integration test root requires its explicit evidence directory.');
if (suppliedTestRoot) {
  assert.ok(path.isAbsolute(suppliedTestRoot) && path.isAbsolute(suppliedEvidenceDirectory),
    'Integration root and evidence directory must be absolute.');
  const relative = path.relative(suppliedEvidenceDirectory, suppliedTestRoot);
  assert.ok(relative && !relative.startsWith('..') && !path.isAbsolute(relative)
    && path.dirname(path.resolve(suppliedTestRoot)) === path.resolve(suppliedEvidenceDirectory)
    && path.basename(suppliedTestRoot).startsWith('vulcan-w09-ui-'),
  'Integration root must be a named direct test descendant of the evidence directory.');
}
const rootParent = suppliedTestRoot ? path.dirname(path.resolve(suppliedTestRoot))
  : cloneCredentialDatabase ? homedir() : tmpdir();
const root = suppliedTestRoot ? await realpath(suppliedTestRoot)
  : await mkdtemp(path.join(rootParent, cloneCredentialDatabase ? '.vulcan-w09-ui-' : 'vulcan-w09-ui-'));
if (cloneCredentialDatabase) {
  // The integration runner may supply a test root; credential rows still belong only below the
  // actual user profile, never inside a shared checkout or an arbitrary evidence directory.
  // 集成运行器可以指定测试根，但凭据行仍只能置于真实用户目录下，不能进入共享工作区或任意证据目录。
  const privateHome = await realpath(homedir());
  const relative = path.relative(privateHome, root);
  assert.ok(relative && !relative.startsWith('..') && !path.isAbsolute(relative),
    'AI acceptance root must remain inside the user profile.');
}
// selectedSecretReference narrows copied credential rows to the model selected for this test.
// selectedSecretReference 将复制的凭据行限定为本次测试所选模型的账号。
let selectedSecretReference = null;
// A supplied debug model configuration is copied only into this disposable test namespace.
// 显式提供的 debug 模型配置只会复制到本次可丢弃的测试命名空间。
if (process.env.VULCAN_W09_AI_CONNECTIONS_SOURCE) {
  await mkdir(path.join(root, 'user-vulcan/code'), { recursive: true });
  // isolatedConfigPath is never the user's authoritative connection file.
  // isolatedConfigPath 绝非用户的权威连接配置文件。
  const isolatedConfigPath = path.join(root, 'user-vulcan/code/ai-connections.toml');
  if (modelOverrideFieldCount === 3) {
    // A test-only model override keeps one exact existing connection. An unrelated expired OAuth
    // account must not stop this isolated Core before its selected DeepSeek model is tested.
    // 测试模型覆盖仅保留一个现有连接；无关的过期 OAuth 账号不能在 DeepSeek 测试前阻止隔离 Core 启动。
    for (const value of Object.values(modelOverride)) {
      assert.match(value, /^[A-Za-z0-9_-]+$/, 'Model override contains an invalid catalog identifier.');
    }
    // sourceConfigText is read only; sections are copied byte-for-byte from its generated TOML.
    // sourceConfigText 只读；连接表节从其生成的 TOML 中逐字节复制。
    const sourceConfigText = await readFile(process.env.VULCAN_W09_AI_CONNECTIONS_SOURCE, 'utf8');
    // sectionHeaders enumerates exact TOML tables without interpreting credential values.
    // sectionHeaders 枚举精确 TOML 表，不解释凭据内容。
    const sectionHeaders = [...sourceConfigText.matchAll(/^\[([^\]\r\n]+)\][ \t]*\r?$/gm)];
    // tableSections preserves each complete generated table body and its exact table path.
    // tableSections 保留每个生成表的完整正文及精确表路径。
    const tableSections = sectionHeaders.map((match, index) => ({
      name: match[1],
      text: sourceConfigText.slice(match.index, sectionHeaders[index + 1]?.index ?? sourceConfigText.length),
    }));
    // configurationTable owns the persisted internal schema version.
    // configurationTable 持有持久化内部 Schema 版本。
    const configurationTable = tableSections.find(section => section.name === 'configuration');
    assert.ok(configurationTable, 'Debug AI configuration has no configuration table.');
    // connectionTable is the precise account named by the test override.
    // connectionTable 是测试覆盖指定的精确账号。
    const connectionTableName = `configuration.connection_graph.connections.${modelOverride.connectionId}`;
    const connectionTable = tableSections.find(section => section.name === connectionTableName);
    assert.ok(connectionTable, 'Requested debug AI connection is absent.');
    assert.match(connectionTable.text,
      new RegExp(`^provider_id[ \\t]*=[ \\t]*"${modelOverride.providerId}"[ \\t]*$`, 'm'),
      'Requested connection belongs to a different provider.');
    // secretReferenceMatch identifies one existing credential row without exposing its value.
    // secretReferenceMatch 标识一条现有凭据行，不输出其值。
    const secretReferenceMatch = connectionTable.text.match(/^secret_reference[ \t]*=[ \t]*"([^"\r\n]+)"[ \t]*$/m);
    assert.ok(secretReferenceMatch, 'Requested connection has no credential reference.');
    selectedSecretReference = secretReferenceMatch[1];
    // rootSchema and configurationSchema are copied from the verified source's declared versions.
    // rootSchema 与 configurationSchema 复制已核验来源声明的版本。
    const rootSchema = sourceConfigText.match(/^schema_version[ \t]*=[ \t]*(\d+)[ \t]*$/m)?.[1];
    const configurationSchema = configurationTable.text.match(/^schema_version[ \t]*=[ \t]*(\d+)[ \t]*$/m)?.[1];
    assert.ok(rootSchema && configurationSchema, 'Debug AI configuration lacks declared schema versions.');
    // selectedTables include the base connection and its declared nested tables only.
    // selectedTables 仅包含目标连接主表及其声明的嵌套表。
    const selectedTables = tableSections.filter(section =>
      section.name === connectionTableName || section.name.startsWith(`${connectionTableName}.`));
    assert.ok(selectedTables.length >= 1);
    await writeFile(isolatedConfigPath,
      `schema_version = ${rootSchema}\n\n[configuration]\nschema_version = ${configurationSchema}\n\n`
      + `[configuration.active]\nprovider_id = "${modelOverride.providerId}"\n`
      + `model_id = "${modelOverride.modelId}"\nconnection_id = "${modelOverride.connectionId}"\n\n`
      + selectedTables.map(section => section.text).join(''));
  } else {
    await cp(process.env.VULCAN_W09_AI_CONNECTIONS_SOURCE, isolatedConfigPath);
  }
  // Copy only credential rows into a fresh database; cloning session indexes would refer to
  // metadata files that do not belong to this isolated test root.
  // 仅把凭据行复制到新数据库；克隆会话索引会引用不属于本隔离测试根的元数据文件。
  await mkdir(path.join(root, 'user-vulcan/code/sqlite'), { recursive: true });
  const source = new DatabaseSync(process.env.VULCAN_W09_SQLITE_SOURCE, { readOnly: true });
  const destination = new DatabaseSync(path.join(root, 'user-vulcan/code/sqlite/app-workspace.sqlite3'));
  try {
    const table = source.prepare("SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'ai_credential_secrets'").get();
    assert.ok(table?.sql, 'The debug credential table is unavailable.');
    destination.exec(table.sql);
    const rows = selectedSecretReference
      ? source.prepare('SELECT * FROM ai_credential_secrets WHERE secret_reference = ?').all(selectedSecretReference)
      : source.prepare('SELECT * FROM ai_credential_secrets').all();
    if (selectedSecretReference) assert.equal(rows.length, 1, 'Selected debug connection has no unique credential row.');
    if (rows.length > 0) {
      const columns = Object.keys(rows[0]);
      const insert = destination.prepare(`INSERT INTO ai_credential_secrets (${columns.join(',')}) VALUES (${columns.map(() => '?').join(',')})`);
      destination.exec('BEGIN');
      try {
        for (const row of rows) insert.run(...columns.map((column) => row[column]));
        destination.exec('COMMIT');
      } catch (error) {
        destination.exec('ROLLBACK');
        throw error;
      }
    }
  } finally {
    destination.close();
    source.close();
  }
}
// layoutRoot selects the current host build without changing the shared output package.
// layoutRoot 选用当前宿主构建而不改动共享的 output 包。
const layoutRoot = await mkdtemp(path.join(repository, 'target', 'w09-ui-layout-'));
await mkdir(path.join(layoutRoot, 'bin'), { recursive: true });
await mkdir(path.join(layoutRoot, 'browser'), { recursive: true });
await mkdir(path.join(layoutRoot, 'desktop'), { recursive: true });
await cp(path.join(repository, 'apps/vulcan-code-desktop/dist'), path.join(layoutRoot, 'compiled'), { recursive: true });
await mkdir(path.join(layoutRoot, 'compiled/node_modules'), { recursive: true });
await Promise.all([
  link(selectedServiceExecutable, path.join(layoutRoot, 'bin/vulcan-code-service.exe')),
  symlink(path.join(assetRoot, 'desktop/runtime'), path.join(layoutRoot, 'desktop/runtime'), 'junction'),
  symlink(path.join(assetRoot, 'browser/chromium'), path.join(layoutRoot, 'browser/chromium'), 'junction'),
  symlink(path.join(assetRoot, 'webui'), path.join(layoutRoot, 'webui'), 'junction'),
  symlink(await realpath(path.join(repository, 'apps/vulcan-code-desktop/node_modules/playwright-core')),
    path.join(layoutRoot, 'compiled/node_modules/playwright-core'), 'junction'),
  symlink(await realpath(path.join(repository, 'apps/vulcan-code-desktop/node_modules/ws')),
    path.join(layoutRoot, 'compiled/node_modules/ws'), 'junction'),
]);
// runtimeLayout binds the fresh host to the same explicit Electron, Chromium, and WebUI assets.
// runtimeLayout 将新宿主绑定到同一组明确的 Electron、Chromium 和 WebUI 资源。
const runtimeLayout = JSON.parse(await readFile(path.join(assetRoot, 'runtime-layout.json'), 'utf8'));
runtimeLayout.profile = 'debug';
runtimeLayout.browserHostEntry = 'compiled/chromium-host/main.js';
await writeFile(path.join(layoutRoot, 'runtime-layout.json'), JSON.stringify(runtimeLayout, null, 2));
// serviceExecutable is the service built with the test-support feature.
// serviceExecutable 是启用测试支持特性构建的服务程序。
const serviceExecutable = selectedServiceExecutable;
// diagnostic captures startup failure details without printing service credentials.
// diagnostic 捕获启动失败细节，但不打印服务凭据。
const diagnostic = [];
// inputOutcomes retains sanitized Core acknowledgements for diagnosing a missed real-page click.
// inputOutcomes 保留已清理的 Core 输入确认，供真实页面点击未命中时诊断。
const inputOutcomes = [];
// The UI lease value is retained in memory only for exact secret-leak auditing.
// 界面租约值仅在内存中保留，用于精确审计秘密是否泄漏。
let capturedUiLeaseSecret = null;
// debugNotificationId identifies only the opt-in branded debug toast created by this run.
// debugNotificationId 仅标识本轮可选的品牌化调试通知。
let debugNotificationId = null;
// notificationIdentity records the exact GUI identity used by this acceptance process.
// notificationIdentity 记录本次验收进程使用的精确 GUI 身份。
let notificationIdentity = null;
// debugNotificationIdentity expands a path-derived debug identity for Windows probes and cleanup.
// debugNotificationIdentity 为 Windows 探针与清理展开路径派生的调试身份。
let debugNotificationIdentity = null;
// g14NotificationId identifies the exact conversation notification for physical-click cleanup.
// g14NotificationId 标识物理点击验收的精确会话通知，供定向清理。
let g14NotificationId = null;
// g14PriorToastTags records this exact GUI identity's history before the model turn.
// g14PriorToastTags 记录模型回合开始前当前精确 GUI 身份的通知历史。
let g14PriorToastTags = null;
// g14Acceptance retains the real notification-to-conversation navigation result.
// g14Acceptance 保留真实通知到会话导航的验收结果。
let g14Acceptance = null;
// debugToastAcceptance retains only non-secret native identity evidence for the final result.
// debugToastAcceptance 仅为最终结果保留不含秘密的原生身份验收证据。
let debugToastAcceptance = null;
// rpcOwnerSessionId is a test-only routing field for service RPCs that accept optional client context.
// rpcOwnerSessionId 是接受可选客户端上下文的服务 RPC 使用的仅测试路由字段。
const rpcOwnerSessionId = 'w09-browser-ui-acceptance';
// clicks is incremented only when the sandboxed page receives a real pointer click.
// clicks 仅在沙箱页面收到真实指针点击时增加。
let clicks = 0;
// drags counts completed page drags with a held left button during pointer movement.
// drags 统计指针移动期间保持鼠标左键按下的已完成页面拖拽。
let drags = 0;
// wheelScrolls counts a real scroll event inside the page's scrollable fixture.
// wheelScrolls 统计页面滚动夹具中实际触发的滚轮滚动事件。
let wheelScrolls = 0;
// A real HttpOnly cookie must never appear in model history or service diagnostics.
// 真实 HttpOnly Cookie 不得出现在模型历史或服务诊断中。
const cookieSentinel = `w09-private-cookie-${randomUUID()}`;
// The delayed response stays open only until the real conversation Stop has settled.
// 延迟响应只保持到真实会话 Stop 完成，不把悬挂请求带出隔离夹具。
let stalledPageResponse = null;
let stalledPageRequestSeen = false;
// stalledPagePath uniquely identifies the current delayed navigation so a late retry from a
// stopped AI turn cannot satisfy the next turn's in-flight assertion.
// stalledPagePath 唯一标识当前延迟导航，避免已停止 AI 回合的迟到重试误满足下一回合的在途断言。
let stalledPagePath = `/w09-stall/${randomUUID()}`;
// fixture serves the deterministic browser page for UI input verification.
// fixture 提供用于界面输入验证的确定性浏览器页面。
const fixture = createServer((request, response) => {
  if (request.url?.startsWith('/w09-stall/')) {
    if (request.url !== stalledPagePath) {
      response.writeHead(410).end('stale navigation fixture');
      return;
    }
    stalledPageResponse = response;
    stalledPageRequestSeen = true;
    return;
  }
  if (request.url === '/w09-download') {
    const body = Buffer.from('W09 owner-scoped download fixture', 'utf8');
    response.writeHead(200, {
      'content-type': 'application/octet-stream',
      'content-disposition': 'attachment; filename="w09-owner-download.txt"',
      'content-length': body.length,
    });
    response.end(body);
    return;
  }
  if (request.url === '/clicked' && request.method === 'POST') {
    clicks += 1;
    response.writeHead(204).end();
    return;
  }
  if (request.url === '/dragged' && request.method === 'POST') {
    drags += 1;
    response.writeHead(204).end();
    return;
  }
  if (request.url === '/scrolled' && request.method === 'POST') {
    wheelScrolls += 1;
    response.writeHead(204).end();
    return;
  }
  response.writeHead(200, { 'content-type': 'text/html; charset=utf-8',
    'set-cookie': `w09_private=${cookieSentinel}; HttpOnly; SameSite=Strict` });
  response.end(`<!doctype html><meta charset="utf-8"><title>Vulcan AI browser fixture</title>
    <button style="position:absolute;left:0;top:0;width:300px;height:260px" onclick="fetch('/clicked',{method:'POST'})">UI click fixture</button>
    <a style="position:absolute;left:350px;top:0;width:120px;height:40px" href="/w09-download" download="w09-owner-download.txt">Download fixture</a>
    <div id="drag" style="position:absolute;left:600px;top:100px;width:180px;height:90px;background:#ada;touch-action:none">Drag fixture</div>
    <div id="scroll" style="position:absolute;left:600px;top:300px;width:180px;height:100px;overflow:auto"><div style="height:600px">Scroll fixture</div></div>
    <script>
      let dragStart = null;
      let movedWithButton = false;
      document.getElementById('drag').addEventListener('pointerdown', event => { dragStart = event.clientX; movedWithButton = false; });
      document.addEventListener('pointermove', event => {
        if (dragStart !== null && event.buttons === 1 && event.clientX - dragStart > 80) movedWithButton = true;
      });
      document.addEventListener('pointerup', () => {
        if (movedWithButton) fetch('/dragged', { method: 'POST' });
        dragStart = null;
        movedWithButton = false;
      });
      document.getElementById('scroll').addEventListener('scroll', event => {
        if (event.currentTarget.scrollTop > 20) fetch('/scrolled', { method: 'POST' });
      }, { once: true });
    </script>`);
});
await new Promise((resolve) => fixture.listen(0, '127.0.0.1', resolve));
// fixtureUrl is the page the user navigates to through the browser panel.
// fixtureUrl 是用户通过浏览器面板导航到的页面。
const fixtureUrl = `http://127.0.0.1:${fixture.address().port}/`;
// service owns browser sessions independently of the GUI lifetime.
// service 独立于 GUI 生命周期拥有浏览器会话。
const service = spawn(serviceExecutable, ['--headless', `--test-root=${root}`], {
  env: { ...process.env, VULCAN_RUNTIME_LAYOUT: path.join(layoutRoot, 'runtime-layout.json'),
    VULCAN_CODE_OUTPUT_ROOT: assetRoot },
  windowsHide: true,
  stdio: ['ignore', 'pipe', 'pipe'],
});
service.stdout.on('data', (chunk) => diagnostic.push(chunk.toString()));
service.stderr.on('data', (chunk) => diagnostic.push(chunk.toString()));
// gui is assigned after Electron finishes launching and closed in finally.
// gui 在 Electron 启动完成后赋值，并在 finally 中关闭。
let gui = null;
// webBrowser is an ordinary Chromium client, separate from Electron and the Core-owned host.
// webBrowser 是普通 Chromium 客户端，与 Electron 及 Core 所属宿主相互独立。
let webBrowser = null;
// normalSocket and slowSocket are closed before the isolated service is torn down.
// normalSocket 与 slowSocket 会在隔离服务关闭前释放。
let normalSocket = null;
let slowSocket = null;

// until retries a read-only condition with a bounded deadline.
// until 在有界期限内重试只读条件。
// observe returns the accepted result; timeout is the maximum wait in milliseconds.
// observe 返回已认可结果；timeout 是最长等待毫秒数。
async function until(observe, timeout = 60_000) {
  const deadline = Date.now() + timeout;
  for (;;) {
    if (service.exitCode !== null) throw new Error(`Core exited: ${service.exitCode}\n${diagnostic.join('')}`);
    try { return await observe(); } catch (error) {
      if (Date.now() >= deadline) throw error;
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
  }
}

// readProviderToolTraces reads committed trace slots for one exact disposable conversation.
// readProviderToolTraces 读取一条精确且可丢弃会话已经提交的追踪槽位。
// conversationId identifies the durable session and promptMarker optionally identifies one turn;
// returns final wire tools and a marker match without returning request text.
// conversationId 标识可靠会话，promptMarker 可选地标识一个回合；只返回最终线协议工具及
// 标记匹配结果，不返回请求正文。
async function readProviderToolTraces(conversationId, promptMarker = null) {
  // sessionRoot is the service-owned V1 session directory below this test-only data root.
  // sessionRoot 是当前测试数据根下由服务持有的 V1 会话目录。
  const sessionsRoot = path.join(root, 'user-vulcan/code/sessions');
  const matchingSessions = [];
  for (const project of await readdir(sessionsRoot, { withFileTypes: true })) {
    if (!project.isDirectory()) continue;
    for (const session of await readdir(path.join(sessionsRoot, project.name), { withFileTypes: true })) {
      if (session.isDirectory() && session.name.endsWith(`-${conversationId}`)) {
        matchingSessions.push(path.join(sessionsRoot, project.name, session.name));
      }
    }
  }
  assert.equal(matchingSessions.length, 1, 'Provider trace session directory is not unique.');
  const traceRoot = path.join(matchingSessions[0], 'temp/provider-api-trace');
  const slots = await readdir(traceRoot, { withFileTypes: true });
  const traces = [];
  for (const slot of slots) {
    if (!slot.isDirectory() || !/^slot-\d{2}$/.test(slot.name)) continue;
    const slotRoot = path.join(traceRoot, slot.name);
    const pointer = JSON.parse(await readFile(path.join(slotRoot, 'identity.json'), 'utf8'));
    assert.ok(pointer.payload_bank === 0 || pointer.payload_bank === 1);
    const input = JSON.parse(await readFile(path.join(slotRoot,
      `payload-${pointer.payload_bank}/input.jsonl`), 'utf8'));
    assert.equal(input.session_id, conversationId);
    assert.equal(input.route.protocol_route.protocol_profile_id, 'openai-responses.v1');
    assert.ok(Array.isArray(input.request.body.tools), 'Final Provider request omitted its tool array.');
    traces.push({ sequence: pointer.trace_sequence,
      promptMarkerPresent: promptMarker !== null
        && JSON.stringify(input.request.body).includes(promptMarker),
      tools: input.request.body.tools });
  }
  return traces.sort((left, right) => left.sequence - right.sequence);
}

try {
  // discovery is the exact isolated Core instance the GUI must use.
  // discovery 是 GUI 必须使用的精确隔离 Core 实例。
  const discovery = await until(async () => {
    const value = JSON.parse(await readFile(path.join(root, 'user-vulcan/code/webui-service.json'), 'utf8'));
    const response = await fetch(`${value.baseUrl}/v1/service/status`, {
      headers: { 'x-vulcan-service-token': value.serviceToken },
    });
    const status = await response.json();
    assert.equal(response.status, 200, `service status HTTP ${response.status}`);
    assert.equal(status.pid, service.pid, JSON.stringify(status));
    assert.equal(status.lifecycle, 'ready', JSON.stringify(status));
    return value;
  }, 120_000);
  // rpcResponse sends one authenticated service request and returns its complete wire envelope.
  // rpcResponse 发送一条带认证的服务请求，并返回完整线协议信封。
  // Parameters: method is the exact service RPC name and params carries only declared fields.
  // 参数：method 是精确服务 RPC 名称，params 仅携带已声明字段。
  // Returns: the parsed HTTP response envelope without hiding a service rejection.
  // 返回值：解析后的 HTTP 响应信封，不隐藏服务拒绝。
  // rpcResponse sends one authenticated service request for an explicit owner context.
  // rpcResponse 为明确的所有者上下文发送一条经过认证的服务请求。
  // Parameters: method is the exact service RPC name, params are typed fields, and ownerSessionId is trusted test context.
  // 参数：method 是精确服务 RPC 名称，params 是类型化字段，ownerSessionId 是可信测试上下文。
  // Returns: the parsed HTTP response envelope, including an explicit service rejection.
  // 返回值：解析后的 HTTP 响应信封，其中保留明确的服务拒绝。
  async function rpcResponse(method, params = {}, ownerSessionId = rpcOwnerSessionId) {
    const response = await fetch(`${discovery.baseUrl}/rpc`, {
      method: 'POST',
      headers: { 'content-type': 'application/json', 'x-vulcan-service-token': discovery.serviceToken },
      body: JSON.stringify({ method, params: { ownerSessionId, ...params } }),
    });
    const body = await response.json();
    assert.equal(response.status, 200, `${method}: HTTP ${response.status}`);
    return body;
  }
  // rpc sends one authenticated service request and requires an accepted result.
  // rpc 发送一条带认证的服务请求，并要求服务返回成功结果。
  // Parameters: method is the exact service RPC name and params carries its typed fields.
  // 参数：method 是精确服务 RPC 名称，params 携带其类型化字段。
  // Returns: the RPC result, or throws the complete non-secret error envelope.
  // 返回值：RPC 结果；失败时抛出完整的非敏感错误信封。
  async function rpc(method, params = {}, ownerSessionId = rpcOwnerSessionId) {
    const body = await rpcResponse(method, params, ownerSessionId);
    assert.equal(body.ok, true, `${method}: ${JSON.stringify(body)}`);
    return body.result;
  }
  // rpcExact preserves strict V1 request shapes that do not declare an ownerSessionId field.
  // rpcExact 保留未声明 ownerSessionId 字段的严格 V1 请求结构。
  // method and params are the exact service contract; returns the accepted result.
  // method 与 params 是精确服务契约；返回已接受的结果。
  async function rpcExact(method, params) {
    const response = await fetch(`${discovery.baseUrl}/rpc`, {
      method: 'POST',
      headers: { 'content-type': 'application/json', 'x-vulcan-service-token': discovery.serviceToken },
      body: JSON.stringify({ method, params }),
    });
    const body = await response.json();
    assert.equal(response.status, 200, `${method}: HTTP ${response.status}`);
    assert.equal(body.ok, true, `${method}: ${JSON.stringify(body.error)}`);
    return body.result;
  }
  // webSocketBytes converts one complete WebSocket message into bytes without assuming a runtime-specific data class.
  // webSocketBytes 将一条完整 WebSocket 消息转换为字节，不假定运行时使用某一种数据类。
  // Parameters: data is the message payload emitted by the WebSocket implementation.
  // 参数：data 是 WebSocket 实现发出的消息载荷。
  // Returns: the message bytes used to parse the Core frame envelope.
  // 返回值：用于解析 Core 帧信封的消息字节。
  async function webSocketBytes(data) {
    if (data instanceof ArrayBuffer) return Buffer.from(data);
    if (ArrayBuffer.isView(data)) return Buffer.from(data.buffer, data.byteOffset, data.byteLength);
    if (data && typeof data.arrayBuffer === 'function') return Buffer.from(await data.arrayBuffer());
    throw new Error('Core browser stream emitted an unsupported binary message.');
  }
  // browserStreamUrl builds the authenticated URL for one page-specific observer subscription.
  // browserStreamUrl 为一个按页面限定的观察订阅构建经过认证的 URL。
  // Parameters: ticket authorizes one stream, pageId selects the immutable page, and subscriptionId isolates the observer.
  // 参数：ticket 授权一条流，pageId 选择不可变页面，subscriptionId 隔离观察者。
  // Returns: the WebSocket URL accepted by the service browser stream route.
  // 返回值：服务浏览器流路由接受的 WebSocket URL。
  function browserStreamUrl(ticket, pageId, subscriptionId) {
    const value = new URL(`${discovery.baseUrl}/v1/browser/stream`);
    value.protocol = value.protocol === 'https:' ? 'wss:' : 'ws:';
    value.searchParams.set('ticket', ticket);
    value.searchParams.set('pageId', pageId);
    value.searchParams.set('subscriptionId', subscriptionId);
    value.searchParams.set('token', discovery.serviceToken);
    return value;
  }
  // openNormalObserver opens a real binary stream and records frame receipt and capture latency.
  // openNormalObserver 打开一条真实二进制流，并记录帧接收时间与捕获延迟。
  // Parameters: streamUrl is the one-use service URL and pageId filters frames to the selected page.
  // 参数：streamUrl 是一次性服务 URL，pageId 将帧限定到选定页面。
  // Returns: a live socket, frame samples, and the first-frame promise result.
  // 返回值：活动 socket、帧样本以及首帧 Promise 结果。
  async function openNormalObserver(streamUrl, pageId) {
    const frames = [];
    let firstFrameResolve;
    let firstFrameReject;
    let firstFrameSeen = false;
    const firstFrame = new Promise((resolve, reject) => {
      firstFrameResolve = resolve;
      firstFrameReject = reject;
    });
    const startedAt = Date.now();
    const socket = new WebSocket(streamUrl);
    socket.binaryType = 'arraybuffer';
    socket.addEventListener('error', () => {
      const failure = new Error('Normal browser observer stream failed.');
      if (!firstFrameSeen) firstFrameReject(failure);
    });
    socket.addEventListener('message', (event) => {
      void (async () => {
        if (typeof event.data === 'string') {
          const metadata = JSON.parse(event.data);
          if (metadata.kind === 'error' && !firstFrameSeen) {
            firstFrameReject(new Error(`Normal browser observer refused: ${metadata.message ?? 'unknown error'}`));
          }
          return;
        }
        const packet = await webSocketBytes(event.data);
        if (packet.length < 5) return;
        const headerLength = packet.readUInt32BE(0);
        if (headerLength === 0 || headerLength > packet.length - 4) return;
        const header = JSON.parse(packet.subarray(4, 4 + headerLength).toString('utf8'));
        if (header.kind !== 'frame' || header.pageId !== pageId) return;
        const receivedAt = Date.now();
        const capturedAt = Number(header.capturedAt);
        frames.push({
          receivedAt,
          sequence: String(header.sequence),
          latencyMs: Number.isFinite(capturedAt) ? receivedAt - capturedAt : null,
        });
        if (!firstFrameSeen) {
          firstFrameSeen = true;
          firstFrameResolve({ receivedAt, header });
        }
      })().catch((error) => {
        if (!firstFrameSeen) firstFrameReject(error);
      });
    });
    const timer = setTimeout(() => firstFrameReject(new Error('Timed out waiting for the normal observer first frame.')), 20_000);
    timer.unref?.();
    try {
      const first = await firstFrame;
      clearTimeout(timer);
      return { socket, frames, first, startedAt };
    } catch (error) {
      clearTimeout(timer);
      socket.close();
      throw error;
    }
  }
  // openPausedObserver performs a real WebSocket upgrade and then stops reading its TCP stream.
  // openPausedObserver 完成真实 WebSocket 升级后暂停读取 TCP 流。
  // Parameters: streamUrl is the authenticated page-specific stream URL.
  // 参数：streamUrl 是经过认证且限定页面的流 URL。
  // Returns: the paused TCP socket representing a deliberately slow observer.
  // 返回值：代表故意变慢观察者的暂停 TCP socket。
  async function openPausedObserver(streamUrl) {
    return new Promise((resolve, reject) => {
      const socket = createConnection({ host: streamUrl.hostname, port: Number(streamUrl.port || 80) });
      let response = '';
      let settled = false;
      const timer = setTimeout(() => fail(new Error('Timed out waiting for the slow observer WebSocket upgrade.')), 20_000);
      timer.unref?.();
      const fail = (error) => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        socket.destroy();
        reject(error);
      };
      socket.once('error', fail);
      socket.on('data', (chunk) => {
        response += chunk.toString('latin1');
        if (!response.includes('\r\n\r\n')) return;
        const statusLine = response.slice(0, response.indexOf('\r\n'));
        if (!statusLine.startsWith('HTTP/1.1 101')) {
          fail(new Error(`Slow observer upgrade refused: ${statusLine}`));
          return;
        }
        settled = true;
        clearTimeout(timer);
        socket.pause();
        resolve(socket);
      });
      socket.once('connect', () => {
        const key = randomBytes(16).toString('base64');
        socket.write(`GET ${streamUrl.pathname}${streamUrl.search} HTTP/1.1\r\nHost: ${streamUrl.host}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: ${key}\r\nSec-WebSocket-Version: 13\r\n\r\n`);
      });
    });
  }
  // percentile returns a stable latency summary from finite samples.
  // percentile 从有限样本返回稳定的延迟摘要。
  // Parameters: values contains non-negative latency samples and percentile is between 0 and 1.
  // 参数：values 包含非负延迟样本，percentile 取值范围为 0 到 1。
  // Returns: the nearest-rank sample, or null when no sample exists.
  // 返回值：最近秩样本；没有样本时返回 null。
  function percentile(values, percentileValue) {
    const sorted = values.filter(Number.isFinite).sort((left, right) => left - right);
    if (sorted.length === 0) return null;
    return sorted[Math.min(sorted.length - 1, Math.floor(percentileValue * sorted.length))];
  }
  // waitForCoreReady waits for the protected lifecycle endpoint before mutating catalog or conversation state.
  // waitForCoreReady 在修改目录或会话状态前等待受保护的生命周期端点。
  // Returns: the ready service status owned by this exact child process.
  // 返回值：由本测试子进程拥有的已就绪服务状态。
  const serviceStatus = await until(async () => {
    const response = await fetch(`${discovery.baseUrl}/v1/service/status`, {
      headers: { 'x-vulcan-service-token': discovery.serviceToken },
    });
    const status = await response.json();
    assert.equal(response.status, 200, `service status HTTP ${response.status}`);
    assert.equal(status.pid, service.pid, JSON.stringify(status));
    assert.equal(status.lifecycle, 'ready', JSON.stringify(status));
    return status;
  }, 120_000);
  if (modelOverrideFieldCount === 3) {
    // The service validates this exact provider/model/account against its loaded catalog; only
    // the isolated copied settings are changed, before any conversation is materialized.
    // 服务会按已加载目录验证精确供应商、模型和账号；仅在会话物化前修改隔离副本的设置。
    await rpcExact('service_select_public_ai_model', {
      request: { ...modelOverride, conversationId: null },
    });
  }
  // browserComponentStatus is separate from the later Playwright MCP environment check.
  // browserComponentStatus 与后续 Playwright MCP 环境检查相互独立。
  const browserComponentStatus = await rpcExact('browser.runtime.status', {});
  assert.equal(browserComponentStatus.ready, true,
    'The isolated Core must expose its standalone Chromium before enabling the plugin.');
  // browserPluginSource is the exact packaged generation loaded by this isolated service.
  // browserPluginSource 是此隔离服务实际加载的精确发行包代次。
  const browserPluginSource = path.join(assetRoot, 'plugins', 'vulcan-browser');
  // browserPluginPreview proves the real manifest and package fingerprint before enabling the system-supplied package.
  // browserPluginPreview 在启用系统分发包前证明真实清单与插件包指纹。
  const browserPluginPreview = await rpc('service_inspect_local_plugin', { sourcePath: browserPluginSource });
  // currentBrowserPluginPreview distinguishes a stale selected package from the current checkout.
  // currentBrowserPluginPreview 区分陈旧的选定发行包与当前检出源码。
  const currentBrowserPluginPreview = await rpc('service_inspect_local_plugin', {
    sourcePath: path.join(repository, 'plugins', 'vulcan-browser'),
  });
  assert.equal(browserPluginPreview.contentFingerprint, currentBrowserPluginPreview.contentFingerprint,
    'The selected asset root contains a stale browser plugin; synchronize a fresh isolated distribution before W09 acceptance.');
  assert.equal(browserPluginPreview.pluginId, 'vulcan-browser');
  assert.ok(browserPluginPreview.contentFingerprint);
  assert.ok(browserPluginPreview.uiMountPoints.includes('conversation.right_panel'));
  assert.equal(browserPluginPreview.installSecretRequirements.length, 0);
  // browserPluginEnvironmentReady checks the exact bundled generation before lifecycle enablement.
  // browserPluginEnvironmentReady 在生命周期启用前检查精确的内置插件代次。
  const browserPluginEnvironmentReady = await rpc('service_plugin_environment_ready', { pluginId: 'vulcan-browser' });
  // pluginInstallAcceptance records whether a fresh isolated profile exercised the async path.
  // pluginInstallAcceptance 记录新的隔离配置是否实际走过异步安装路径。
  let pluginInstallAcceptance = null;
  if (!browserPluginEnvironmentReady) {
    // Keep a real GUI connected while Core accepts the installation, then close it before waiting for completion.
    // 在 Core 接受安装任务时保持真实 GUI 连接，随后关闭 GUI，再等待任务完成。
    gui = await electron.launch({
      executablePath: selectedDesktopRuntime,
      args: [...electronScaleArgs, selectedDesktopEntry, `--service-executable=${serviceExecutable}`, `--webui-url=${discovery.baseUrl}`, `--test-root=${root}`, `--user-data-dir=${path.join(root, 'gui-install')}`],
      timeout: 60_000,
    });
    const installationGui = await until(async () => {
      const current = gui.windows()[0];
      assert.ok(current);
      await current.locator('.workspace-sidebar-feature-button').first().waitFor({ timeout: 1000 });
      return current;
    }).catch(async (error) => {
      // diagnostic reads only structural GUI state when the installation window never reaches its workbench.
      // diagnostic 仅在安装窗口迟迟未进入工作台时读取结构状态，不输出用户页面正文或凭据。
      // current is the isolated Electron page still present at the failed deadline.
      // current 是失败期限到达时仍存在的隔离 Electron 页面。
      const current = gui.windows()[0];
      if (current) {
        const diagnostic = await current.evaluate(() => ({
          protocol: location.protocol,
          host: location.host,
          pathname: location.pathname,
          readyState: document.readyState,
          hasBridge: Boolean(window.vulcanDesktop),
          hasRoot: Boolean(document.getElementById('root')),
          bodyLength: document.body?.innerText.length ?? 0,
          chromiumError: document.body?.innerText.match(/ERR_[A-Z_]+/)?.[0] ?? null,
          hasWorkspaceButton: Boolean(document.querySelector('.workspace-sidebar-feature-button')),
          hasConnectionError: /connection failed|连接失败|无法连接/i.test(document.body?.innerText ?? ''),
        })).catch(() => ({ rendererUnavailable: true }));
        if (diagnostic.protocol === 'data:') {
          // startupMessage is restricted to the isolated startup page and redacts long token-like values.
          // startupMessage 仅取隔离启动页，并遮蔽形似令牌的长值。
          const startupMessage = await current.evaluate(() => document.body?.innerText ?? '')
            .catch(() => '');
          diagnostic.startupMessage = startupMessage
            .replace(/[A-Za-z0-9_-]{40,}/g, '[redacted]')
            .slice(0, 600);
        }
        console.error(JSON.stringify({ installationGuiDiagnostic: diagnostic }));
      }
      throw error;
    });
    assert.equal((await installationGui.evaluate(() => window.vulcanDesktop.service.getConnection())).pid, service.pid);
    // Two independent RPC requests must coalesce onto one Core-owned installation task.
    // 两个独立 RPC 请求必须合并到同一个 Core 自有安装任务。
    const firstInstall = await rpc('service_start_plugin_environment_install', { pluginId: 'vulcan-browser' });
    const duplicateInstall = await rpc('service_start_plugin_environment_install', { pluginId: 'vulcan-browser' });
    assert.equal(duplicateInstall.taskId, firstInstall.taskId);
    const taskByPlugin = await rpc('service_plugin_environment_install_status_for_plugin',
      { pluginId: 'vulcan-browser' });
    assert.equal(taskByPlugin.taskId, firstInstall.taskId);
    const stateBeforeGuiClose = taskByPlugin.state;
    await gui.close();
    gui = null;
    const survivingService = await rpcExact('browser.runtime.status', {});
    assert.equal(survivingService.ready, true,
      'Closing the installation GUI must leave the Core and its browser runtime available.');
    const beforeInstallCatalog = await rpc('workbench.snapshot');
    const beforeInstallPlugin = beforeInstallCatalog.plugins.find((plugin) => plugin.id === 'vulcan-browser');
    assert.equal(beforeInstallPlugin?.enabled, false);
    const completedInstall = await until(async () => {
      const status = await rpc('service_plugin_environment_install_status', { taskId: firstInstall.taskId });
      assert.notEqual(status.state, 'failed', JSON.stringify(status));
      assert.equal(status.state, 'succeeded');
      return status;
    }, 120_000);
    const afterInstallCatalog = await rpc('workbench.snapshot');
    const afterInstallPlugin = afterInstallCatalog.plugins.find((plugin) => plugin.id === 'vulcan-browser');
    assert.equal(afterInstallPlugin?.enabled, false);
    pluginInstallAcceptance = { taskId: firstInstall.taskId, duplicateCoalesced: true,
      crossRequestStatus: true, stateBeforeGuiClose, state: completedInstall.state,
      guiClosedBeforeStatusWait: true, remainedDisabled: true };
  }
  assert.equal(await rpc('service_plugin_environment_ready', { pluginId: 'vulcan-browser' }), true);
  // privateEnvironment is the exact installed generation selected by the validated source fingerprint.
  // privateEnvironment 是由已校验源码指纹选出的精确已安装代次。
  const privateEnvironment = await realpath(path.join(root, 'user-vulcan/code/plugin-runtime',
    'vulcan-browser', '0.1.0', browserPluginPreview.contentFingerprint));
  // installedMcpPackage resolves pnpm's private package link before damaging the isolated fixture.
  // installedMcpPackage 在破坏隔离夹具前解析 pnpm 的私有包链接。
  const installedMcpPackage = await realpath(path.join(privateEnvironment,
    'node/node_modules/@playwright/mcp'));
  // installedMcpManifest identifies the declared executable instead of guessing its filename.
  // installedMcpManifest 通过已声明入口定位可执行脚本，不猜测文件名。
  const installedMcpManifest = JSON.parse(await readFile(path.join(installedMcpPackage, 'package.json'), 'utf8'));
  const installedMcpBin = await realpath(path.join(installedMcpPackage,
    installedMcpManifest.bin['playwright-mcp']));
  const binRelative = path.relative(privateEnvironment, installedMcpBin);
  assert.ok(binRelative && !binRelative.startsWith('..') && !path.isAbsolute(binRelative),
    'The isolated MCP bin must stay inside the disposable plugin environment.');
  await unlink(installedMcpBin);
  assert.equal(await rpc('service_plugin_environment_ready', { pluginId: 'vulcan-browser' }), false,
    'A marker and package directory cannot hide a missing MCP executable.');
  const damagedEnable = await rpcResponse('service_set_vulcan_plugin_enabled',
    { pluginId: 'vulcan-browser', enabled: true });
  assert.equal(damagedEnable.ok, false, JSON.stringify(damagedEnable));
  assert.match(JSON.stringify(damagedEnable.error), /执行环境尚未安装/);
  const damagedCatalog = await rpc('workbench.snapshot');
  assert.equal(damagedCatalog.plugins.find((plugin) => plugin.id === 'vulcan-browser')?.enabled, false);
  // A second explicit installation repairs the exact disabled generation and receives a fresh task id.
  // 第二次显式安装修复同一个已停用代次，并取得新的任务标识。
  const repairInstall = await rpc('service_start_plugin_environment_install', { pluginId: 'vulcan-browser' });
  if (pluginInstallAcceptance) assert.notEqual(repairInstall.taskId, pluginInstallAcceptance.taskId);
  const repairedInstall = await until(async () => {
    const status = await rpc('service_plugin_environment_install_status', { taskId: repairInstall.taskId });
    assert.notEqual(status.state, 'failed', JSON.stringify(status));
    assert.equal(status.state, 'succeeded');
    return status;
  }, 120_000);
  assert.equal(await rpc('service_plugin_environment_ready', { pluginId: 'vulcan-browser' }), true);
  if (pluginInstallAcceptance) pluginInstallAcceptance.damageRepair = {
    missingExecutableRejected: true, enableRefused: true, retryTaskId: repairInstall.taskId,
    retryState: repairedInstall.state,
  };
  // The synchronized distribution owns this plugin; the preview above validates the authoritative source package before enabling it.
  // 已同步的发行布局拥有该插件；上面的预览会在启用前校验权威源插件包。
  // Installing the same bundled identity into the isolated user root is rejected by the runtime by design.
  // 运行时按设计拒绝将同一内置身份再次安装到隔离用户根目录。
  // browserPluginCatalog waits for the enabled generation that the WebUI dynamic mount may load.
  // browserPluginCatalog 等待 WebUI 动态挂载可以加载的已启用插件代次。
  const browserPluginCatalog = await rpc('service_set_vulcan_plugin_enabled', { pluginId: 'vulcan-browser', enabled: true });
  const browserPlugin = browserPluginCatalog.plugins.find((plugin) => plugin.id === 'vulcan-browser');
  assert.ok(browserPlugin, 'Bundled browser plugin is absent from the service catalog.');
  assert.equal(browserPlugin.enabled, true, JSON.stringify(browserPlugin));
  assert.equal(browserPlugin.runtimeState, 'enabled', JSON.stringify(browserPlugin));
  assert.equal(browserPlugin.extension?.uiMounts.some((mount) => mount.point === 'conversationRightPanel'), true);
  assert.equal(browserPlugin.extension?.uiMounts.some((mount) => mount.point === 'skillsPluginDetail'
    && mount.id === 'browser-settings'), true);
  // The declarative settings layout binds declared plugin Actions; Core validates and persists the
  // complete configuration before a new Playwright MCP connection is started.
  // 声明式设置版面绑定已声明的插件 Action；Core 在启动新 Playwright MCP 连接前校验并持久化
  // 完整配置。
  const settingsMount = await rpc('service_load_plugin_ui_module', {
    pluginId: 'vulcan-browser', mountId: 'browser-settings',
  });
  assert.equal(settingsMount.kind, 'settings');
  assert.ok(settingsMount.source.includes('settings-update'));
  const browserSettings = await rpc('service_invoke_plugin_action', {
    pluginId: 'vulcan-browser', actionId: 'settings-get', arguments: {},
  });
  assert.equal(browserSettings.consoleLevel, 'warning');
  const invalidBrowserSettings = await rpcResponse('service_invoke_plugin_action', {
    pluginId: 'vulcan-browser', actionId: 'settings-update',
    arguments: { ...browserSettings, actionTimeoutMs: '99' },
  });
  assert.equal(invalidBrowserSettings.ok, false);
  const updatedBrowserSettings = await rpc('service_invoke_plugin_action', {
    pluginId: 'vulcan-browser', actionId: 'settings-update',
    arguments: { ...browserSettings, actionTimeoutMs: '7000', consoleLevel: 'info' },
  });
  assert.deepEqual(updatedBrowserSettings,
    { ...browserSettings, actionTimeoutMs: '7000', consoleLevel: 'info' });
  assert.deepEqual(await rpc('service_invoke_plugin_action', {
    pluginId: 'vulcan-browser', actionId: 'settings-get', arguments: {},
  }), updatedBrowserSettings);
  const pluginSettingsAcceptance = {
    mountedInPluginDetails: true, invalidConfigurationRejected: true,
    completeConfigurationPersisted: true, newConnectionConsoleLevel: updatedBrowserSettings.consoleLevel,
    newConnectionActionTimeoutMs: updatedBrowserSettings.actionTimeoutMs,
  };
  // initialWorkbench captures the durable conversation set before this acceptance creates its own owner.
  // initialWorkbench 捕获本验收创建所属会话前的可靠会话集合。
  const initialWorkbench = await rpc('workbench.snapshot');
  // project is the exact host-projected project selected for the new persistent conversation.
  // project 是新持久会话所选的宿主投影精确项目。
  const project = initialWorkbench.projects[0];
  assert.ok(project, 'The isolated service did not expose a project for conversation materialization.');
  // conversationSubmissionId is the stable browser-owned idempotency identity for this one fixture.
  // conversationSubmissionId 是本次夹具使用的稳定浏览器所有幂等身份。
  const conversationSubmissionId = `w09-browser-ui-${Date.now()}`;
  // conversationSubmitResponse preserves a possible post-materialization provider rejection for evidence.
  // conversationSubmitResponse 保留可能发生在会话物化后的 Provider 拒绝证据。
  const conversationSubmitResponse = await rpcResponse('service_submit_conversation_prompt', {
    clientSubmissionId: conversationSubmissionId,
    projectId: project.id,
    conversationId: null,
    draftSlotKey: `new:${project.id}`,
    prompt: 'W09 browser UI persistent conversation fixture',
    // A deliberately absent selection materializes a UI-only owner without starting an
    // unrelated model turn; the separate AI conversation below uses the real debug model.
    // 故意选择不存在的模型，只物化界面会话而不启动无关模型回合；下方独立 AI 会话仍使用真实 debug 模型。
    ...(cloneModelConfig ? { modelSelection: {
      providerId: 'w09-ui-fixture-unavailable',
      modelId: 'w09-ui-fixture-unavailable',
      connectionId: 'w09-ui-fixture-unavailable',
    } } : {}),
  });
  if (cloneModelConfig) assert.equal(conversationSubmitResponse.ok, false,
    'The UI-only fixture unexpectedly started a model turn.');
  // materializedWorkbench proves that the selected owner is durable even when an empty test profile cannot run a model.
  // materializedWorkbench 证明选定所有者已可靠持久化，即使空测试画像无法运行模型。
  const materializedWorkbench = await until(async () => {
    const snapshot = await rpc('workbench.snapshot');
    const created = snapshot.conversations.filter((conversation) => (
      !initialWorkbench.conversations.some((existing) => existing.id === conversation.id)
      && conversation.projectId === project.id
    ));
    assert.equal(created.length, 1, JSON.stringify(snapshot.conversations));
    return { snapshot, conversation: created[0] };
  }, 30_000);
  // acceptedConversationId is the persistent owner forwarded to the conversation-right-panel UI context.
  // acceptedConversationId 是传给会话右侧面板界面上下文的持久所有者。
  const acceptedConversationId = conversationSubmitResponse.ok
    ? conversationSubmitResponse.result?.conversationId
    : materializedWorkbench.conversation.id;
  assert.equal(typeof acceptedConversationId, 'string');
  assert.equal(materializedWorkbench.conversation.id, acceptedConversationId);
  // A persisted conversation must retain the enabled plugin in the workbench projection.
  // 持久化会话必须在工作台投影中保留已启用的插件。
  const workbenchBrowserPlugin = materializedWorkbench.snapshot.plugins.find((plugin) => plugin.id === 'vulcan-browser');
  assert.ok(workbenchBrowserPlugin, 'Workbench snapshot omitted the enabled browser plugin.');
  assert.equal(workbenchBrowserPlugin.enabled, true, JSON.stringify(workbenchBrowserPlugin));
  assert.equal(workbenchBrowserPlugin.runtimeState, 'enabled', JSON.stringify(workbenchBrowserPlugin));
  assert.equal(workbenchBrowserPlugin.extension?.uiMounts.some((mount) => mount.point === 'conversationRightPanel'), true);
  gui = await electron.launch({
    executablePath: selectedDesktopRuntime,
    args: [...electronScaleArgs, selectedDesktopEntry, `--service-executable=${serviceExecutable}`, `--webui-url=${discovery.baseUrl}`, `--test-root=${root}`, `--user-data-dir=${path.join(root, 'gui')}`],
    timeout: 60_000,
  });
  // Native notification identity must follow the executable profile even when both GUIs share a user.
  // 即使两种 GUI 属于同一用户，原生通知身份也必须随可执行文件配置隔离。
  notificationIdentity = await gui.evaluate(({ app }) => ({
    name: app.getName(), activatorClsid: app.toastActivatorCLSID,
  }));
  if (process.platform === 'win32') {
    const releaseGui = path.basename(selectedDesktopRuntime).toLowerCase() === 'vulcancodedesktop.exe';
    if (releaseGui) {
      assert.deepEqual(notificationIdentity, {
        name: 'Vulcan Code Desktop',
        activatorClsid: '{AB3EF79F-A068-4BA3-9B40-814138499202}',
      });
    } else {
      // debugSuffix binds shortcut, AUMID, and COM activation to this executable path.
      // debugSuffix 将快捷方式、AUMID 与 COM 激活绑定到当前可执行程序路径。
      const debugSuffix = notificationIdentity.name
        .match(/^Vulcan Code Desktop Debug ([0-9a-f]{12})$/)?.[1];
      assert.ok(debugSuffix);
      assert.match(notificationIdentity.activatorClsid.toUpperCase(),
        /^\{[0-9A-F]{8}-[0-9A-F]{4}-5[0-9A-F]{3}-[89AB][0-9A-F]{3}-[0-9A-F]{12}\}$/);
      debugNotificationIdentity = {
        name: notificationIdentity.name,
        appUserModelId: `ai.vulcan.code.desktop.debug.${debugSuffix}`,
        activatorClsid: notificationIdentity.activatorClsid.toUpperCase(),
      };
    }
  }
  // page is the actual desktop workbench renderer served by Core HTTP.
  // page 是由 Core HTTP 提供的实际桌面工作台渲染器。
  const page = await until(async () => {
    const current = gui.windows()[0];
    assert.ok(current);
    await current.locator('.workspace-sidebar-feature-button').first().waitFor({ timeout: 1000 });
    return current;
  });
  const initialDevicePixelRatio = await page.evaluate(() => window.devicePixelRatio);
  if (requestedDeviceScale) {
    assert.ok(Math.abs(initialDevicePixelRatio - Number(requestedDeviceScale)) < 0.01,
      JSON.stringify({ requestedDeviceScale, initialDevicePixelRatio }));
  }
  // The exact session-create response separates a host startup refusal from a later lifecycle close.
  // 精确的会话创建响应区分宿主启动拒绝与后续生命周期关闭。
  page.on('response', (response) => {
    const request = response.request();
    if (!response.url().endsWith('/rpc') || request.method() !== 'POST') return;
    const body = request.postDataJSON();
    if (body?.method === 'browser.session.create') {
      void response.json().then((result) => console.error('W09 browser.session.create:', JSON.stringify(result)));
    } else if (body?.method === 'browser.lease.acquire') {
      void response.json().then((result) => {
        if (result.ok && typeof result.result?.leaseSecret === 'string') {
          capturedUiLeaseSecret = result.result.leaseSecret;
        }
      });
    } else if (body?.method === 'browser.input.dispatch') {
      void response.json().then((result) => inputOutcomes.push({
        kind: body.params?.kind,
        type: body.params?.payload?.type,
        x: body.params?.payload?.x,
        y: body.params?.payload?.y,
        result: result.result,
        error: result.error,
      }));
    }
  });
  // connection proves the GUI selected the same service process as this fixture.
  // connection 证明 GUI 选择了本夹具的同一服务进程。
  const connection = await page.evaluate(() => window.vulcanDesktop.service.getConnection());
  assert.equal(connection.pid, service.pid);
  if (process.platform === 'win32'
    && path.basename(selectedDesktopRuntime).toLowerCase() === 'electron.exe') {
    // Generic electron.exe shares Electron.lnk with other developer projects. Native presentation
    // would adopt that shortcut's prior CLSID and could overwrite the release COM registration.
    // 通用 electron.exe 与其他开发项目共用 Electron.lnk；原生投递可能采用其中旧 CLSID 并覆盖正式版 COM 注册。
    const sourceNotificationBoundary = await page.evaluate(async () => {
      const bridge = window.vulcanDesktop.desktop;
      let refusal = '';
      try {
        await bridge.showNotification({ id: 'w09-source-identity-guard', title: 'Guard', body: 'No toast' });
      } catch (error) {
        refusal = String(error);
      }
      return {
        supported: bridge.nativeNotificationsAvailable,
        permission: await bridge.getNotificationPermission(),
        refusal,
      };
    });
    assert.equal(sourceNotificationBoundary.supported, false);
    assert.equal(sourceNotificationBoundary.permission, 'unsupported');
    assert.match(sourceNotificationBoundary.refusal, /Native notifications are not supported/);
    assert.equal((await gui.evaluate(({ app }) => app.toastActivatorCLSID)).toUpperCase(),
      debugNotificationIdentity.activatorClsid);
  }
  if (process.platform === 'win32' && process.env.VULCAN_W09_TEST_BRANDED_TOAST === '1') {
    // The opt-in probe is restricted to the generated debug package in this checkout; it must
    // never inspect or remove a real installed application's notification identity.
    // 可选探针仅允许使用本仓库生成的调试包，绝不检查或移除真实安装程序的通知身份。
    assert.equal(path.basename(selectedDesktopRuntime).toLowerCase(), 'vulcancodedesktopdebug.exe');
    assert.ok(path.resolve(selectedDesktopRuntime).startsWith(path.join(repository, 'target') + path.sep));
    debugNotificationId = `w09-debug-notification-${randomUUID()}`;
    const debugNotificationBoundary = await page.evaluate(async (id) => {
      const bridge = window.vulcanDesktop.desktop;
      const permission = await bridge.getNotificationPermission();
      await bridge.showNotification({ id, title: 'Vulcan Code Debug', body: 'Branded toast identity probe' });
      return { supported: bridge.nativeNotificationsAvailable, permission };
    }, debugNotificationId);
    assert.deepEqual(debugNotificationBoundary, { supported: true, permission: 'granted' });
    assert.ok(debugNotificationIdentity);
    assert.equal((await gui.evaluate(({ app }) => app.toastActivatorCLSID)).toUpperCase(),
      debugNotificationIdentity.activatorClsid);
    const observeDebugToast = '$m=[Windows.UI.Notifications.ToastNotificationManager,Windows.UI.Notifications,ContentType=WindowsRuntime];'
      + '$t=$m::History.GetHistory($env:VULCAN_W09_AUMID) | Where-Object { $_.Tag -eq $env:VULCAN_W09_TOAST_ID } | Select-Object -First 1;'
      + '$p=[Environment]::GetFolderPath("Programs");$f=Join-Path $p ($env:VULCAN_W09_APP_NAME + ".lnk");'
      + '$s=(New-Object -ComObject WScript.Shell).CreateShortcut($f);'
      + '$i=(New-Object -ComObject Shell.Application).NameSpace($p).ParseName($env:VULCAN_W09_APP_NAME + ".lnk");'
      + '$k="Registry::HKEY_CURRENT_USER\\Software\\Classes\\CLSID\\" + $env:VULCAN_W09_CLSID + "\\LocalServer32";'
      + '[pscustomobject]@{xml=if($t){$t.Content.GetXml()}else{$null};target=$s.TargetPath;'
      + 'aumid=$i.ExtendedProperty("System.AppUserModel.ID");'
      + 'clsid=$i.ExtendedProperty("System.AppUserModel.ToastActivatorCLSID");'
      + 'server=(Get-Item -LiteralPath $k).GetValue("")} | ConvertTo-Json -Compress';
    const debugToast = await until(async () => {
      const { stdout } = await execFileAsync('powershell.exe', ['-NoProfile', '-Command', observeDebugToast], {
        env: { ...process.env,
          VULCAN_W09_TOAST_ID: debugNotificationId,
          VULCAN_W09_AUMID: debugNotificationIdentity.appUserModelId,
          VULCAN_W09_APP_NAME: debugNotificationIdentity.name,
          VULCAN_W09_CLSID: debugNotificationIdentity.activatorClsid,
        }, timeout: 10_000,
      });
      const value = JSON.parse(stdout);
      assert.ok(value.xml?.includes(`tag=${debugNotificationId}`));
      return value;
    }, 20_000);
    assert.equal(debugToast.target.toLowerCase(), selectedDesktopRuntime.toLowerCase());
    assert.equal(debugToast.server.toLowerCase(), selectedDesktopRuntime.toLowerCase());
    assert.equal(debugToast.aumid, debugNotificationIdentity.appUserModelId);
    assert.equal(debugToast.clsid.toUpperCase(), debugNotificationIdentity.activatorClsid);
    debugToastAcceptance = {
      delivered: true, exactHistoryId: debugNotificationId,
      shortcutTargetMatches: true, serverTargetMatches: true,
      aumid: debugToast.aumid, clsid: debugToast.clsid,
    };
  }
  // conversationRow selects the durable conversation created through the service RPC before GUI launch.
  // conversationRow 选择 GUI 启动前通过服务 RPC 创建的可靠会话。
  const conversationRow = page.locator('.conversation-tree-button').filter({ hasText: 'W09 browser UI persistent conversation fixture' }).first();
  await conversationRow.waitFor({ timeout: 20_000 });
  await conversationRow.click();
  await until(async () => {
    assert.equal(await conversationRow.getAttribute('aria-current'), 'page');
    return true;
  }, 20_000);
  // openContextPanel exposes the conversation-scoped dynamic plugin mount through the real header action.
  // openContextPanel 通过真实标题栏动作暴露会话范围动态插件挂载。
  const contextPanelToggle = page.getByRole('button', { name: /打开右侧面板|关闭右侧面板|Open right panel|Close right panel/ });
  try {
    await contextPanelToggle.waitFor({ timeout: 20_000 });
    await contextPanelToggle.click();
    await until(async () => {
      assert.equal(await contextPanelToggle.getAttribute('aria-pressed'), 'true');
      return true;
    }, 8_000);
  } catch (error) {
    // Header diagnostics distinguish a missing chat action from a missing plugin mount.
    // 标题栏诊断区分缺少会话动作与缺少插件挂载。
    console.error(JSON.stringify({
      pageUrl: page.url(),
      buttons: await page.locator('button').evaluateAll((buttons) => buttons.map((button) => ({
        ariaLabel: button.getAttribute('aria-label'),
        text: button.textContent?.trim(),
        pressed: button.getAttribute('aria-pressed'),
      }))),
      bodyText: (await page.locator('body').innerText()).slice(-5000),
    }));
    throw error;
  }
  // The native browser is a fixed GUI capability, even before the AI browser plugin creates a Core session.
  // 原生浏览器是 GUI 的固定能力，即使 AI 浏览器插件尚未创建 Core 会话也应出现。
  await page.locator('.context-launcher-card').filter({ hasText: '内置浏览器' }).waitFor({ timeout: 10_000 });
  assert.equal(await page.locator('.context-launcher-card').filter({ hasText: 'AI 浏览器' }).count(), 0);
  // The Core browser belongs to AI; creating it must open this conversation's dedicated right tab.
  // Core 浏览器归 AI 所有；创建后必须打开此会话专属的右侧页签。
  const initialBrowser = await rpc('browser.session.create', { profileMode: 'ephemeral' }, acceptedConversationId);
  assert.equal(initialBrowser.status, 'running');
  // browserPanel proves the enabled manifest mount renders only inside the browser tab.
  // browserPanel 证明启用的插件挂载只在浏览器页签内部渲染。
  const browserPanel = page.locator('.context-tab-content.browser-tab-content [aria-label="AI 浏览器"], .context-tab-content.browser-tab-content [aria-label="AI browser"]').first();
  try {
    await browserPanel.waitFor({ timeout: 20_000 });
  } catch (error) {
    // Exact DOM and live service projection distinguish stale frontend state from module failure.
    // 精确 DOM 和实时服务投影用于区分前端旧状态与模块加载失败。
    const liveWorkbench = await rpc('workbench.snapshot');
    const liveBrowserPlugin = liveWorkbench.plugins.find((plugin) => plugin.id === 'vulcan-browser');
    console.error(JSON.stringify({
      liveBrowserPlugin: liveBrowserPlugin ? {
        enabled: liveBrowserPlugin.enabled,
        runtimeState: liveBrowserPlugin.runtimeState,
        uiMounts: liveBrowserPlugin.extension?.uiMounts,
      } : null,
      contextTabs: await page.locator('.context-dynamic-tab-strip').allInnerTexts(),
      alerts: await page.getByRole('alert').allInnerTexts(),
      statuses: await page.getByRole('status').allInnerTexts(),
      selectedConversationId: acceptedConversationId,
      bodyTail: (await page.locator('body').innerText()).slice(-3500),
    }));
    throw error;
  }
  // Native window gestures run only when their implementation changes and the caller opts in.
  // 仅窗口手势实现发生变化且调用者显式选择时才重跑原生窗口手势。
  let separatorStartWidth = null;
  let separatorEndWidth = null;
  let terminalSelectionAcceptance = null;
  if (process.env.VULCAN_W09_RUN_STATIC_GUI_GESTURES === '1') {
  // Drag the actual docked workbench separator and ensure the native top-level window stays put.
  // 拖动真实停靠工作台分隔条，并确认原生顶层窗口位置不变。
  // At the default 1440px window the responsive panel deliberately floats and has no separator.
  // 默认 1440 像素窗口下响应式面板有意浮动，没有分隔条；扩大实际窗口后检验停靠状态。
  const originalSeparatorTestBounds = await gui.evaluate(({ BrowserWindow }) =>
    BrowserWindow.getAllWindows()[0].getBounds());
  await gui.evaluate(({ BrowserWindow }) => {
    const window = BrowserWindow.getAllWindows()[0];
    window.setBounds({ ...window.getBounds(), width: 1900 });
  });
  const separator = page.locator('.workspace-context-resize-handle[role="separator"]');
  await separator.waitFor({ state: 'visible', timeout: 10_000 });
  const separatorBounds = await separator.boundingBox();
  assert.ok(separatorBounds);
  separatorStartWidth = Number(await separator.getAttribute('aria-valuenow'));
  const windowBeforeSeparator = await gui.evaluate(({ BrowserWindow }) =>
    BrowserWindow.getAllWindows()[0].getBounds());
  const separatorPointer = {
    x: separatorBounds.x + separatorBounds.width / 2,
    y: separatorBounds.y + separatorBounds.height / 2,
  };
  await page.mouse.move(separatorPointer.x, separatorPointer.y);
  await page.mouse.down();
  await page.mouse.move(separatorPointer.x + 65, separatorPointer.y, { steps: 5 });
  await page.mouse.up();
  separatorEndWidth = await until(async () => {
    const width = Number(await separator.getAttribute('aria-valuenow'));
    assert.ok(width <= separatorStartWidth - 35, JSON.stringify({ separatorStartWidth, width }));
    return width;
  }, 5_000);
  assert.deepEqual(await gui.evaluate(({ BrowserWindow }) =>
    BrowserWindow.getAllWindows()[0].getBounds()), windowBeforeSeparator);
  // Expand to the actual maximum to verify that resizing retains the default reading column and reasoning gutter.
  // 拉到实际最大值，验证拖拽保留默认阅读列与思考留白。
  await separator.press('End');
  // The wide window must permit more than the former 640px cap while retaining 922px of conversation space.
  // 宽窗口须突破旧的 640px 上限，同时保留 922px 会话空间。
  const expandedDockWidth = Number(await separator.getAttribute('aria-valuenow'));
  assert.ok(expandedDockWidth > 640, JSON.stringify({ expandedDockWidth }));
  await until(async () => {
    // panelWidth is the remaining conversation pane after the right panel reaches its real limit.
    // panelWidth 是右侧面板达到真实上限后剩余的会话区宽度。
    const panelWidth = await page.locator('.workspace-main-panel').first()
      .evaluate((element) => element.getBoundingClientRect().width);
    // readingWidth confirms the default 800px reading column did not reflow.
    // readingWidth 确认默认的 800px 阅读列没有发生重新排版。
    const readingWidth = await page.locator('.workspace-main-panel .conversation-scroll-content').first()
      .evaluate((element) => element.getBoundingClientRect().width);
    assert.ok(Math.abs(panelWidth - 922) <= 2 && Math.abs(readingWidth - 800) <= 1,
      JSON.stringify({ panelWidth, readingWidth, expandedDockWidth }));
    return readingWidth;
  }, 5_000);
  await gui.evaluate(({ BrowserWindow }, bounds) =>
    BrowserWindow.getAllWindows()[0].setBounds(bounds), originalSeparatorTestBounds);
  // Use a real project PTY so dragging selectable terminal text cannot masquerade as native window dragging.
  // 使用真实项目 PTY，验证终端文字拖选不会被误当作原生窗口拖动。
  const terminalCatalog = await rpc('service_list_project_terminals', { projectId: project.id });
  assert.ok(terminalCatalog.shells.length > 0, 'The project has no configured terminal shell.');
  const terminalCreated = await rpc('service_create_project_terminal', {
    request: { projectId: project.id, shellId: terminalCatalog.shells[0].id, rows: 24, cols: 80 },
  });
  const createdTerminal = terminalCreated.terminals.find(terminal =>
    !terminalCatalog.terminals.some(previous => previous.id === terminal.id));
  assert.ok(createdTerminal, 'The Core did not create the test terminal.');
  try {
    const terminalButton = page.locator('.project-terminal-launch-button');
    await terminalButton.click();
    const terminalViewport = page.locator('.terminal-viewport[role="application"]');
    await terminalViewport.waitFor({ state: 'visible', timeout: 10_000 });
    await terminalViewport.click();
    await page.keyboard.type('echo W11_TERMINAL_SELECTION_SAMPLE');
    await page.keyboard.press('Enter');
    await page.waitForTimeout(700);
    const screen = page.locator('.terminal-viewport .xterm-screen');
    const screenBounds = await screen.boundingBox();
    assert.ok(screenBounds);
    if (process.env.VULCAN_W09_TERMINAL_SCREENSHOT) {
      await screen.screenshot({ path: path.resolve(process.env.VULCAN_W09_TERMINAL_SCREENSHOT) });
    }
    const terminalDom = await screen.evaluate(element => ({
      text: element.innerText.slice(0, 200), html: element.innerHTML.slice(0, 400),
    }));
    const beforeTextDrag = await gui.evaluate(({ BrowserWindow }) =>
      BrowserWindow.getAllWindows()[0].getBounds());
    // Xterm renders its own selection rectangles instead of a DOM Selection.
    // Xterm 使用独立选区矩形，而不是浏览器原生 DOM Selection。
    await page.mouse.move(screenBounds.x + 10, screenBounds.y + 15);
    await page.mouse.down();
    await page.mouse.move(screenBounds.x + Math.min(300, screenBounds.width - 10),
      screenBounds.y + 15, { steps: 8 });
    await page.mouse.up();
    const selectionRects = await screen.evaluate(element => Array.from(
      element.querySelectorAll('.xterm-selection > div'), node => ({
        width: node.getBoundingClientRect().width,
        height: node.getBoundingClientRect().height,
      })));
    if (process.env.VULCAN_W09_TERMINAL_SCREENSHOT) {
      await screen.screenshot({ path: path.resolve(process.env.VULCAN_W09_TERMINAL_SCREENSHOT)
        .replace(/\.png$/i, '-selected.png') });
    }
    const terminalAppRegions = await page.evaluate(() => ({
      screen: getComputedStyle(document.querySelector('.xterm-screen')).webkitAppRegion,
      viewport: getComputedStyle(document.querySelector('.terminal-viewport')).webkitAppRegion,
    }));
    const afterTextDrag = await gui.evaluate(({ BrowserWindow }) =>
      BrowserWindow.getAllWindows()[0].getBounds());
    assert.deepEqual(afterTextDrag, beforeTextDrag);
    assert.ok(selectionRects.some(rect => rect.width > 100 && rect.height > 0),
      JSON.stringify({ selectionRects, screenBounds, terminalDom }));
    assert.deepEqual(terminalAppRegions, { screen: 'none', viewport: 'none' });
    terminalSelectionAcceptance = {
      selectionRects: selectionRects.length,
      nativeWindowUnchanged: true,
    };
    await terminalButton.click();
  } finally {
    await rpc('service_close_project_terminal', {
      projectId: project.id, terminalId: createdTerminal.id,
    });
  }
  }
  await browserPanel.locator(`[data-browser-session-id="${initialBrowser.sessionId}"]`).click();
  try {
    await browserPanel.getByRole('button', { name: /取得控制权|Take control/ }).waitFor({ timeout: 20_000 });
  } catch (error) {
    // A missing control action must report the browser panel error and Core session list.
    // 缺少控制动作时必须同时报告浏览器面板错误和 Core 会话列表。
    console.error(JSON.stringify({
      panelText: await browserPanel.innerText(),
      sessions: await rpc('browser.session.list', {}, acceptedConversationId),
    }));
    throw error;
  }
  await browserPanel.getByRole('button', { name: /取得控制权|Take control/ }).click();
  await browserPanel.getByRole('textbox', { name: /页面地址|Page URL/ }).fill(fixtureUrl);
  // go is enabled only after the UI has both a lease and a selected Core page.
  // go 仅在界面同时拥有租约和选定 Core 页面后才启用。
  const go = browserPanel.getByRole('button', { name: /前往|Go/ });
  try {
    await until(async () => { assert.equal(await go.isEnabled(), true); return true; }, 8_000);
  } catch (error) {
    console.error('Browser panel:', await browserPanel.innerText());
    console.error('Input outcomes:', JSON.stringify(inputOutcomes));
    throw error;
  }
  await go.click();
  // projection is the Core frame rendered by the WebUI, not a local BrowserView.
  // projection 是 WebUI 渲染的 Core 帧，而非本地 BrowserView。
  const projection = browserPanel.getByRole('img', { name: /Core 浏览器实时页面|Live Core browser page/ });
  await projection.locator('img').waitFor({ timeout: 20_000 });
  await until(async () => {
    const src = await projection.locator('img').getAttribute('src');
    assert.ok(src?.startsWith('blob:'), 'The WebUI must render a binary Core frame.');
    return true;
  }, 20_000);
  // browserBinding resolves the exact Core session and page currently owned by this persisted conversation.
  // browserBinding 解析当前持久会话实际拥有的精确 Core 会话与页面。
  const browserBinding = await until(async () => {
    const sessions = await rpc('browser.session.list', {}, acceptedConversationId);
    const runningSessions = sessions.filter((session) => session.status === 'running');
    assert.equal(runningSessions.length, 1, JSON.stringify(sessions));
    const session = runningSessions[0];
    const pages = await rpc('browser.pages.list', { sessionId: session.sessionId }, acceptedConversationId);
    assert.ok(pages.length >= 1, JSON.stringify({ session, pages }));
    const page = pages.find((candidate) => candidate.url === fixtureUrl);
    assert.ok(page, JSON.stringify({ session, pages, fixtureUrl }));
    return { session, page };
  }, 20_000);
  // normalTicket authorizes a second real observer while the WebUI remains the first normal client.
  // normalTicket 在 WebUI 作为第一个正常客户端时授权第二个真实观察者。
  const normalTicket = await rpc('browser.stream.ticket', { sessionId: browserBinding.session.sessionId }, acceptedConversationId);
  const normalStreamUrl = browserStreamUrl(normalTicket.ticket, browserBinding.page.pageId, `w09-normal-${randomUUID()}`);
  const normalObserver = await openNormalObserver(normalStreamUrl, browserBinding.page.pageId);
  normalSocket = normalObserver.socket;
  // baseline measures the normal observer before a deliberately paused observer is introduced.
  // baseline 测量引入故意暂停的观察者之前正常观察者的表现。
  await new Promise((resolve) => setTimeout(resolve, 2_000));
  const baselineStart = Date.now();
  await new Promise((resolve) => setTimeout(resolve, 4_000));
  const baselineEnd = Date.now();
  // slowTicket authorizes a separate subscription whose TCP receive side is paused after upgrade.
  // slowTicket 授权另一条订阅，并在升级后暂停其 TCP 接收端。
  const slowTicket = await rpc('browser.stream.ticket', { sessionId: browserBinding.session.sessionId }, acceptedConversationId);
  const slowStreamUrl = browserStreamUrl(slowTicket.ticket, browserBinding.page.pageId, `w09-slow-${randomUUID()}`);
  slowSocket = await openPausedObserver(slowStreamUrl);
  const slowStart = Date.now();
  await new Promise((resolve) => setTimeout(resolve, 6_000));
  const slowEnd = Date.now();
  const baselineFrames = normalObserver.frames.filter((frame) => frame.receivedAt >= baselineStart && frame.receivedAt < baselineEnd);
  const slowWindowFrames = normalObserver.frames.filter((frame) => frame.receivedAt >= slowStart && frame.receivedAt < slowEnd);
  const baselineFps = Number((baselineFrames.length / ((baselineEnd - baselineStart) / 1_000)).toFixed(1));
  const slowWindowFps = Number((slowWindowFrames.length / ((slowEnd - slowStart) / 1_000)).toFixed(1));
  const normalLatencies = normalObserver.frames.map((frame) => frame.latencyMs).filter(Number.isFinite);
  const slowLatencies = slowWindowFrames.map((frame) => frame.latencyMs).filter(Number.isFinite);
  // A relative ratio alone accepted the measured 15 fps regression; keep the 20 fps plan floor.
  // 仅按相对比例曾放过实测 15 帧回退；保留方案规定的每秒 20 帧绝对下限。
  const performanceTargetFps = Math.max(20, baselineFps * 0.7);
  const streamPerformance = {
    normalObserverFirstFrameMs: normalObserver.first.receivedAt - normalObserver.startedAt,
    normalObserverBaselineFrames: baselineFrames.length,
    normalObserverBaselineFps: baselineFps,
    normalObserverFramesWhileSlow: slowWindowFrames.length,
    normalObserverFpsWhileSlow: slowWindowFps,
    normalObserverFpsRatio: baselineFps > 0 ? Number((slowWindowFps / baselineFps).toFixed(2)) : null,
    normalObserverLatencyP50Ms: percentile(normalLatencies, 0.5),
    normalObserverLatencyP95Ms: percentile(normalLatencies, 0.95),
    normalObserverLatencyWhileSlowP95Ms: percentile(slowLatencies, 0.95),
    slowObserverPausedMs: slowEnd - slowStart,
    slowObserverBufferedBytes: slowSocket.readableLength,
    slowObserverUpgrade: true,
    normalObserverContinued: slowWindowFrames.length > 0,
    performanceTargetFps,
    performanceTargetMet: slowWindowFps >= performanceTargetFps,
  };
  assert.ok(baselineFrames.length > 0, JSON.stringify(streamPerformance));
  assert.ok(baselineFps >= 20, JSON.stringify(streamPerformance));
  // A slow subscriber must not starve the active normal observer; performance remains explicit evidence.
  // 慢订阅者不得饿死活动中的正常观察者；性能结果必须作为明确证据保留。
  assert.ok(streamPerformance.normalObserverContinued, JSON.stringify(streamPerformance));
  assert.ok(streamPerformance.performanceTargetMet, JSON.stringify(streamPerformance));
  normalSocket.close();
  normalSocket = null;
  slowSocket.destroy();
  slowSocket = null;
  // Click the rendered image's known content point, accounting for object-fit letterboxing.
  // 计入 object-fit 留白后，点击渲染图像中已知的页面内容点。
  await projection.scrollIntoViewIfNeeded();
  const clickPosition = await projection.evaluate(element => {
    // The WebUI maps pointer coordinates from getBoundingClientRect, which can differ from client size.
    // WebUI 用 getBoundingClientRect 映射指针坐标，其尺寸可能不同于 client 尺寸。
    const { width, height } = element.getBoundingClientRect();
    const scale = Math.min(width / 1280, height / 800);
    return {
      x: (width - 1280 * scale) / 2 + 100 * scale,
      y: (height - 800 * scale) / 2 + 100 * scale,
    };
  });
  await projection.click({ position: clickPosition });
  try {
    await until(() => { assert.equal(clicks, 1); return true; }, 10_000);
  } catch (error) {
    console.error('Browser panel:', await browserPanel.innerText());
    console.error('Click position:', JSON.stringify(clickPosition));
    console.error('Input outcomes:', JSON.stringify(inputOutcomes));
    throw error;
  }
  // pointerPositions map deterministic page CSS points through the projected image's letterboxing.
  // pointerPositions 将确定的页面 CSS 坐标穿过投影画面留白映射到界面坐标。
  const pointerPositions = await projection.evaluate(element => {
    // width and height measure the actual projection element shown by the desktop GUI.
    // width 和 height 测量桌面 GUI 实际显示的投影元素。
    const { width, height } = element.getBoundingClientRect();
    // scale matches the WebUI's object-fit coordinate conversion.
    // scale 与 WebUI 的 object-fit 坐标换算保持一致。
    const scale = Math.min(width / 1280, height / 800);
    // point converts one page CSS coordinate to a position inside the projection element.
    // point 将一个页面 CSS 坐标换算为投影元素内部的位置。
    const point = (x, y) => ({
      x: (width - 1280 * scale) / 2 + x * scale,
      y: (height - 800 * scale) / 2 + y * scale,
    });
    return { dragStart: point(620, 130), dragEnd: point(750, 130), scroll: point(650, 340) };
  });
  // projectionBounds convert Playwright mouse movement to the actual desktop WebUI viewport.
  // projectionBounds 将 Playwright 鼠标移动换算到真实桌面 WebUI 视口。
  const projectionBounds = await projection.boundingBox();
  assert.ok(projectionBounds);
  await page.mouse.move(projectionBounds.x + pointerPositions.dragStart.x,
    projectionBounds.y + pointerPositions.dragStart.y);
  await page.mouse.down();
  await page.mouse.move(projectionBounds.x + pointerPositions.dragEnd.x,
    projectionBounds.y + pointerPositions.dragEnd.y, { steps: 8 });
  await page.mouse.up();
  await until(() => { assert.equal(drags, 1); return true; }, 10_000);
  // wheelBounds are remeasured because focusing a drag target can scroll the workbench panel.
  // wheelBounds 需要重新测量，因为聚焦拖拽目标可能使工作台面板纵向滚动。
  const wheelBounds = await projection.boundingBox();
  assert.ok(wheelBounds);
  await page.mouse.move(wheelBounds.x + pointerPositions.scroll.x,
    wheelBounds.y + pointerPositions.scroll.y);
  await page.mouse.wheel(0, 180);
  try {
    await until(() => { assert.ok(wheelScrolls >= 1); return true; }, 10_000);
  } catch (error) {
    console.error('Wheel position:', JSON.stringify(pointerPositions.scroll));
    console.error('Input outcomes:', JSON.stringify(inputOutcomes));
    throw error;
  }
  // pointerInteractionAcceptance distinguishes tested browser gestures from skipped GUI window gestures.
  // pointerInteractionAcceptance 区分实际验收的浏览器手势与跳过的 GUI 窗口手势。
  const pointerInteractionAcceptance = { dragEvents: drags, wheelScrollEvents: wheelScrolls };
  // A real page download must remain retrievable only by its immutable conversation owner.
  // 真实网页下载只能由其不可变会话所有者取回。
  const downloadPosition = await projection.evaluate(element => {
    const { width, height } = element.getBoundingClientRect();
    const scale = Math.min(width / 1280, height / 800);
    return {
      x: (width - 1280 * scale) / 2 + 400 * scale,
      y: (height - 800 * scale) / 2 + 20 * scale,
    };
  });
  await projection.click({ position: downloadPosition });
  const downloadedFile = await until(async () => {
    const downloads = await rpc('browser.download.list',
      { sessionId: browserBinding.session.sessionId }, acceptedConversationId);
    const complete = downloads.find((download) => download.status === 'completed');
    assert.ok(complete, JSON.stringify(downloads));
    return complete;
  }, 20_000);
  const downloadUrl = `${discovery.baseUrl}/v1/browser/downloads/${encodeURIComponent(downloadedFile.downloadId)}/content`;
  const downloadHeaders = (ownerSessionId) => ({
    'x-vulcan-service-token': discovery.serviceToken,
    'x-vulcan-browser-owner-session-id': ownerSessionId,
  });
  const ownedDownload = await fetch(downloadUrl, { headers: downloadHeaders(acceptedConversationId) });
  assert.equal(ownedDownload.status, 200);
  assert.equal(await ownedDownload.text(), 'W09 owner-scoped download fixture');
  // The other conversation is created below; an unrelated owner value is already sufficient to
  // test the same Core resolver boundary, and the known second conversation is tested later.
  // 另一会话稍后创建；先用无关所有者值验证同一 Core 解析边界，随后再用已知第二会话复核。
  const foreignDownload = await fetch(downloadUrl, { headers: downloadHeaders('w09-foreign-owner') });
  const absentDownload = await fetch(`${discovery.baseUrl}/v1/browser/downloads/${randomUUID()}/content`,
    { headers: downloadHeaders('w09-foreign-owner') });
  assert.equal(foreignDownload.status, 404);
  assert.equal(absentDownload.status, foreignDownload.status);
  assert.equal(await foreignDownload.text(), await absentDownload.text());
  const crossOwnerDownloadAcceptance = {
    downloadId: downloadedFile.downloadId, ownerFetchedExactBytes: true,
    foreignResultIndistinguishableFromAbsent: true,
  };
  await page.screenshot({ path: path.join(root, 'browser-ui.png') });
  // Keep the first owner's Core browser alive for a two-conversation UI isolation check.
  // 保留首个归属会话的 Core 浏览器，以验证两个会话切换时的界面隔离。
  // The local tab is a separate Electron WebContentsView with no workbench preload or Core owner.
  // 本地标签是独立的 Electron WebContentsView，不带工作台预加载或 Core 归属。
  // pluginMount is the current conversation's right browser tab, opened by a conversation link.
  // pluginMount 是当前会话中由会话链接打开的右侧浏览器页签。
  const pluginMount = page.locator('.context-tab-content.browser-tab-content').first();
  try {
    await page.evaluate((url) => {
      const conversation = document.querySelector('.workspace-main-shell');
      if (!conversation) throw new Error('Conversation content is unavailable.');
      const link = document.createElement('a');
      link.href = url;
      link.textContent = 'W09 local browser link';
      link.dataset.w09LocalBrowserLink = 'true';
      conversation.append(link);
    }, fixtureUrl);
    await page.locator('[data-w09-local-browser-link="true"]').click();
    await pluginMount.getByRole('textbox', { name: /本地浏览地址|Local browser address/ }).waitFor({ timeout: 5_000 });
  } catch (error) {
    console.error('Local browser panel:', JSON.stringify({
      browserPanelText: await browserPanel.innerText().catch(() => '<unmounted>'),
      rightPanel: await pluginMount.innerText().catch(() => '<unmounted>'),
      nativeBridgeAvailable: await page.evaluate(() => typeof window.vulcanDesktop?.localBrowser?.setOwner === 'function'),
    }));
    throw error;
  }
  await pluginMount.getByRole('textbox', { name: /本地浏览地址|Local browser address/ }).fill(fixtureUrl);
  await pluginMount.getByRole('button', { name: /打开地址|Open address/ }).click();
  const localBrowser = await until(async () => {
    const snapshot = await page.evaluate(() => window.vulcanDesktop.localBrowser.snapshot());
    assert.equal(snapshot.tabs.length, 1, JSON.stringify(snapshot));
    assert.equal(snapshot.tabs[0].url, fixtureUrl);
    assert.equal(snapshot.activeId, snapshot.tabs[0].id);
    const native = await gui.evaluate(async ({ BrowserWindow }, url) => {
      const workbench = BrowserWindow.getAllWindows()[0];
      const view = workbench.contentView.children.find((child) => child.webContents?.getURL() === url);
      if (!view) return null;
      return {
        visible: view.getVisible(),
        bounds: view.getBounds(),
        preloadBridge: await view.webContents.executeJavaScript('typeof window.vulcanDesktop'),
      };
    }, fixtureUrl);
    assert.ok(native?.visible, JSON.stringify(native));
    assert.ok(native.bounds.width > 0 && native.bounds.height > 0, JSON.stringify(native));
    assert.equal(native.preloadBridge, 'undefined');
    return { snapshot, native };
  }, 20_000);
  // Zoom checks the real native temporary view against its visible DOM placeholder at three scales.
  // 缩放检查三个比例下真实原生临时视图与可见 DOM 占位区域是否对齐。
  const zoomChecks = [];
  for (const factor of [1, 1.25, 1.5]) {
    await gui.evaluate(({ BrowserWindow }, zoom) =>
      BrowserWindow.getAllWindows()[0].webContents.setZoomFactor(zoom), factor);
    const zoomCheck = await until(async () => {
      const geometry = await page.evaluate(() => {
        const address = document.querySelector('input[aria-label="本地浏览地址"], input[aria-label="Local browser address"]');
        const placeholder = address?.closest('form')?.nextElementSibling;
        const controls = document.querySelector('.window-controls');
        if (!placeholder || !controls) throw new Error('Local browser viewport or window controls are missing.');
        return { viewportWidth: window.innerWidth, viewportHeight: window.innerHeight,
          placeholder: placeholder.getBoundingClientRect().toJSON(),
          controls: controls.getBoundingClientRect().toJSON() };
      });
      const native = await gui.evaluate(({ BrowserWindow }, url) => {
        const workbench = BrowserWindow.getAllWindows()[0];
        const view = workbench.contentView.children.find((child) => child.webContents?.getURL() === url);
        return view ? { visible: view.getVisible(), bounds: view.getBounds(),
          content: workbench.getContentBounds() } : null;
      }, fixtureUrl);
      assert.ok(native?.visible, JSON.stringify({ factor, geometry, native }));
      const scaleX = native.content.width / geometry.viewportWidth;
      const scaleY = native.content.height / geometry.viewportHeight;
      const expected = { x: Math.round(geometry.placeholder.left * scaleX),
        y: Math.round(geometry.placeholder.top * scaleY),
        width: Math.round(geometry.placeholder.right * scaleX) - Math.round(geometry.placeholder.left * scaleX),
        height: Math.round(geometry.placeholder.bottom * scaleY) - Math.round(geometry.placeholder.top * scaleY) };
      for (const key of ['x', 'y', 'width', 'height']) {
        assert.ok(Math.abs(native.bounds[key] - expected[key]) <= 2,
          JSON.stringify({ factor, geometry, native, expected }));
      }
      const minimumUsableHeight = factor === 1.5 ? 120 : factor === 1.25 ? 140 : 200;
      assert.ok(native.bounds.height >= minimumUsableHeight,
        JSON.stringify({ factor, minimumUsableHeight, geometry, native }));
      assert.ok(geometry.controls.left >= 0 && geometry.controls.right <= geometry.viewportWidth + 1
        && geometry.controls.top >= 0 && geometry.controls.bottom <= geometry.viewportHeight + 1,
      JSON.stringify({ factor, geometry }));
      return { factor, nativeBounds: native.bounds, expected };
    }, 8_000);
    zoomChecks.push(zoomCheck);
  }
  await gui.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0].webContents.setZoomFactor(1));
  // A workbench modal must obscure the native page rather than being painted behind it.
  // 工作台模态框必须遮盖原生页面，而不能被原生页面绘制在其背后。
  await page.evaluate(() => {
    const modal = document.createElement('div');
    modal.id = 'w09-local-browser-modal';
    modal.setAttribute('role', 'dialog');
    modal.setAttribute('aria-modal', 'true');
    document.body.append(modal);
  });
  await until(async () => {
    const visible = await gui.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0]
      .contentView.children.some((view) => view.getVisible()));
    assert.equal(visible, false);
    return true;
  }, 8_000);
  await page.evaluate(() => document.getElementById('w09-local-browser-modal')?.remove());
  // localPageIsolation probes the temporary page itself, not the trusted workbench renderer.
  // localPageIsolation 在临时页面本身执行探针，而非受信任的工作台渲染进程。
  const localPageIsolation = await gui.evaluate(async ({ BrowserWindow }, url) => {
    const workbench = BrowserWindow.getAllWindows()[0];
    const view = workbench.contentView.children.find((child) => child.webContents?.getURL() === url);
    if (!view) throw new Error('Local browser view disappeared before the isolation probe.');
    return view.webContents.executeJavaScript(`({
      bridge: typeof window.vulcanDesktop,
      nodeRequire: typeof require,
      desktopCall: (() => { try { window.vulcanDesktop.desktop.minimize(); return 'called'; }
        catch { return 'denied'; } })()
    })`);
  }, fixtureUrl);
  assert.deepEqual(localPageIsolation, { bridge: 'undefined', nodeRequire: 'undefined', desktopCall: 'denied' });
  // A third-party non-HTTP popup must not create a workbench window or a local tab.
  // 第三方非 HTTP 弹窗不得创建工作台窗口或本地标签。
  await gui.evaluate(async ({ BrowserWindow }, url) => {
    const workbench = BrowserWindow.getAllWindows()[0];
    const view = workbench.contentView.children.find((child) => child.webContents?.getURL() === url);
    if (!view) throw new Error('Local browser view disappeared before the popup probe.');
    await view.webContents.executeJavaScript("window.open('file:///C:/Windows/win.ini', '_blank'); true", true);
  }, fixtureUrl);
  assert.equal((await page.evaluate(() => window.vulcanDesktop.localBrowser.snapshot())).tabs.length, 1);
  // The exact Core origin is forbidden even when an untrusted page requests an HTTP popup.
  // 即使不受信任页面请求 HTTP 弹窗，精确的 Core 来源仍被禁止。
  await gui.evaluate(async ({ BrowserWindow }, { sourceUrl, destinationUrl }) => {
    const workbench = BrowserWindow.getAllWindows()[0];
    const view = workbench.contentView.children.find((child) => child.webContents?.getURL() === sourceUrl);
    if (!view) throw new Error('Local browser view disappeared before the workbench popup probe.');
    await view.webContents.executeJavaScript(`window.open(${JSON.stringify(destinationUrl)}, '_blank'); true`, true);
  }, { sourceUrl: fixtureUrl, destinationUrl: discovery.baseUrl });
  const popupSnapshot = await page.evaluate(() => window.vulcanDesktop.localBrowser.snapshot());
  assert.equal(popupSnapshot.tabs.length, 1, JSON.stringify(popupSnapshot));
  assert.equal(popupSnapshot.tabs[0].url, fixtureUrl);
  // popupWindowState proves a denied popup did not become a privileged top-level window.
  // popupWindowState 证明被拒绝的弹窗没有变成带权限的顶层窗口。
  const popupWindowState = await gui.evaluate(({ BrowserWindow }) => ({
    count: BrowserWindow.getAllWindows().length,
    workbenchUrl: BrowserWindow.getAllWindows()[0].webContents.getURL(),
  }));
  assert.equal(popupWindowState.count, 1);
  assert.equal(popupWindowState.workbenchUrl, page.url());
  // A page-initiated navigation to the blocked Core origin must stay on the fixture page.
  // 页面自行导航到被禁止的 Core 来源时，必须保留在夹具页面。
  await gui.evaluate(async ({ BrowserWindow }, { sourceUrl, destinationUrl }) => {
    const workbench = BrowserWindow.getAllWindows()[0];
    const view = workbench.contentView.children.find((child) => child.webContents?.getURL() === sourceUrl);
    if (!view) throw new Error('Local browser view disappeared before navigation probe.');
    await view.webContents.executeJavaScript(`window.location.assign(${JSON.stringify(destinationUrl)}); true`, true)
      .catch(() => undefined);
  }, { sourceUrl: fixtureUrl, destinationUrl: discovery.baseUrl });
  await new Promise((resolve) => setTimeout(resolve, 300));
  const blockedNavigation = await page.evaluate(() => window.vulcanDesktop.localBrowser.snapshot());
  assert.equal(blockedNavigation.tabs.length, 1);
  assert.equal(blockedNavigation.tabs[0].url, fixtureUrl);
  const navigationWindowState = await gui.evaluate(({ BrowserWindow }) => ({
    count: BrowserWindow.getAllWindows().length,
    workbenchUrl: BrowserWindow.getAllWindows()[0].webContents.getURL(),
  }));
  assert.equal(navigationWindowState.count, 1);
  assert.equal(navigationWindowState.workbenchUrl, page.url());
  // The local and AI browsers are separate right tabs; selecting AI must retain its Core projection.
  // 本地与 AI 浏览器是独立的右侧页签；切回 AI 时仍须保留 Core 投影。
  await page.locator('.context-dynamic-tab-strip [role="tab"]').filter({ hasText: /AI 浏览器|AI Browser/ }).click();
  try {
    await browserPanel.waitFor({ timeout: 20_000 });
  } catch (error) {
    console.error('AI browser tab after leaving the local page:', JSON.stringify({
      rightToggle: await page.getByRole('button', { name: /打开右侧面板|关闭右侧面板|Open right panel|Close right panel/ })
        .getAttribute('aria-pressed').catch(() => null),
      contextTabs: await page.locator('.context-dynamic-tab-strip').allInnerTexts(),
      browserTabs: await page.locator('[data-browser-session-id]').evaluateAll((tabs) => tabs.map((tab) => tab.getAttribute('data-browser-session-id'))),
      bodyTail: (await page.locator('body').innerText()).slice(-2000),
    }));
    throw error;
  }
  await until(async () => {
    const running = (await rpc('browser.session.list', {}, acceptedConversationId))
      .filter((session) => session.status === 'running');
    assert.equal(running.length, 1, JSON.stringify(running));
    assert.equal(running[0].sessionId, browserBinding.session.sessionId);
    return true;
  }, 10_000);
  // A second browser client must discover the same Core browser without receiving GUI-local tabs.
  // 第二个网页客户端须发现同一个 Core 浏览器，同时不能获得 GUI 本地标签。
  const beforeOtherConversation = await rpc('workbench.snapshot');
  await rpcResponse('service_submit_conversation_prompt', {
    clientSubmissionId: `w09-other-owner-${Date.now()}`,
    projectId: project.id,
    conversationId: null,
    draftSlotKey: `new:${project.id}`,
    prompt: 'W09 browser owner isolation fixture',
    ...(cloneModelConfig ? { modelSelection: {
      providerId: 'w09-ui-fixture-unavailable', modelId: 'w09-ui-fixture-unavailable',
      connectionId: 'w09-ui-fixture-unavailable',
    } } : {}),
  });
  // otherConversationId identifies the durable owner with no Core browser.
  // otherConversationId 标识没有 Core 浏览器的持久化第二会话。
  const otherConversationId = await until(async () => {
    const snapshot = await rpc('workbench.snapshot');
    const created = snapshot.conversations.filter((conversation) =>
      !beforeOtherConversation.conversations.some((previous) => previous.id === conversation.id));
    assert.equal(created.length, 1, JSON.stringify(created));
    return created[0].id;
  }, 20_000);
  assert.deepEqual(await rpc('browser.session.list', {}, otherConversationId), []);
  // passiveWebBrowser is a second client; it is never the GUI's local-browser owner.
  // passiveWebBrowser 是第二个客户端，绝不持有 GUI 本地浏览器。
  const passiveWebBrowser = await chromium.launch({
    headless: true, executablePath: path.join(assetRoot, ...runtimeLayout.chromiumExecutable.split('/')),
  });
  let passiveWebAcceptance;
  try {
    const context = await passiveWebBrowser.newContext({ viewport: { width: 1280, height: 800 } });
    const webPage = await context.newPage();
    await webPage.goto(`${discovery.baseUrl}/?service=${encodeURIComponent(discovery.baseUrl)}`);
    await webPage.locator('.conversation-tree-button')
      .filter({ hasText: 'W09 browser UI persistent conversation fixture' }).first().click();
    const webPanel = webPage.locator('.context-tab-content.browser-tab-content [aria-label="AI 浏览器"], .context-tab-content.browser-tab-content [aria-label="AI browser"]').first();
    await webPanel.waitFor({ timeout: 25_000 });
    const webTab = webPanel.locator(`[data-browser-session-id="${browserBinding.session.sessionId}"]`);
    await webTab.waitFor({ timeout: 20_000 });
    assert.equal(await webTab.getAttribute('aria-selected'), 'false');
    assert.equal(await webPage.getByRole('button', { name: /内置浏览器|Local Browser/ }).count(), 0);
    await webTab.click();
    await webPanel.getByRole('img', { name: /Core 浏览器实时页面|Live Core browser page/ })
      .locator('img').waitFor({ timeout: 25_000 });
    // A second Core browser must appear in both clients while their selected tabs stay independent.
    // 第二个 Core 浏览器须同时出现在两个客户端，且各自选中的标签保持独立。
    // desktopFirstTab identifies the original Core browser in the GUI observer.
    // desktopFirstTab 标识 GUI 观察端的原始 Core 浏览器。
    const desktopFirstTab = browserPanel.locator(`[data-browser-session-id="${browserBinding.session.sessionId}"]`);
    await desktopFirstTab.click();
    // sharedSecond is a second Core-owned session created without involving a model.
    // sharedSecond 是无需模型参与而创建的第二个 Core 托管会话。
    const sharedSecond = await rpc('browser.session.create', { profileMode: 'ephemeral' }, acceptedConversationId);
    // desktopSecondTab and webSecondTab are independent client views of one Core identity.
    // desktopSecondTab 与 webSecondTab 是同一个 Core 身份在两端的独立视图。
    const desktopSecondTab = browserPanel.locator(`[data-browser-session-id="${sharedSecond.sessionId}"]`);
    const webSecondTab = webPanel.locator(`[data-browser-session-id="${sharedSecond.sessionId}"]`);
    await desktopSecondTab.waitFor({ timeout: 20_000 });
    await webSecondTab.waitFor({ timeout: 20_000 });
    await webSecondTab.click();
    assert.equal(await webSecondTab.getAttribute('aria-selected'), 'true');
    assert.equal(await desktopFirstTab.getAttribute('aria-selected'), 'true');
    await webPanel.getByRole('button', { name: /关闭浏览器|Close browser/ }).click();
    await until(async () => {
      const closed = (await rpc('browser.session.list', {}, acceptedConversationId))
        .find((session) => session.sessionId === sharedSecond.sessionId);
      assert.equal(closed?.status, 'closed', JSON.stringify(closed));
      return true;
    }, 20_000);
    await desktopSecondTab.waitFor({ state: 'detached', timeout: 20_000 });
    await webPage.locator('.conversation-tree-button')
      .filter({ hasText: 'W09 browser owner isolation fixture' }).first().click();
    await until(async () => {
      assert.equal(await webPage.locator(`[data-browser-session-id="${browserBinding.session.sessionId}"]`).count(), 0);
      return true;
    }, 20_000);
    passiveWebAcceptance = { sharedCoreBrowser: browserBinding.session.sessionId,
      defaultDisconnected: true, localBrowserAbsent: true, foreignConversationHidden: true,
      secondBrowserShared: sharedSecond.sessionId, independentTabSelection: true,
      webCloseObservedByDesktop: true };
  } finally {
    await passiveWebBrowser.close();
  }
  await conversationRow.click();
  // With copied debug credentials, ask the actual configured model to drive browser tools itself.
  // 当已复制 debug 凭据时，要求真实配置的模型亲自调用浏览器工具。
  let aiAcceptance = null;
  let aiTurnRetirementAcceptance = null;
  let aiCancellationAcceptance = null;
  let aiTakeoverAcceptance = null;
  let shellSecretRedactionAcceptance = null;
  let sensitiveDataAudit = null;
  let webAcceptance = null;
  // webNotificationAcceptance records the real Web Notification API result and owner isolation.
  // webNotificationAcceptance 记录真实网页通知接口结果与所有者隔离。
  let webNotificationAcceptance = null;
  let multiClientBrowserAcceptance = null;
  // providerToolTrace summarizes exact model-facing definitions without retaining prompt bytes.
  // providerToolTrace 只汇总模型实际收到的定义，不保留提示词字节。
  let providerToolTrace = null;
  // The selected AI browser remains available for plugin-disable authorization checks.
  // 保留已选 AI 浏览器，用于插件停用后的授权状态核验。
  let aiBrowserForDisable = null;
  let aiConversationForDisable = null;
  // crossOwnerProtectedMethodCount counts distinct Core browser RPCs with indistinguishable foreign-id rejection.
  // crossOwnerProtectedMethodCount 统计他人 ID 与不存在 ID 拒绝结果相同的 Core 浏览器 RPC 数量。
  let crossOwnerProtectedMethodCount = null;
  if (cloneModelConfig) {
    // initiatingNotificationClientId is the exact GUI event-socket owner used by the real model turn.
    // initiatingNotificationClientId 是真实模型回合使用的精确 GUI 事件套接字所有者。
    const initiatingNotificationClientId = await page.evaluate(() => window.sessionStorage.getItem(
      'vulcan.notifications.client-instance.v1',
    ));
    assert.match(initiatingNotificationClientId,
      /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
    // Enable notifications only in this isolated GUI before the real AI run settles.
    // 仅在此隔离 GUI 中启用通知，并确保发生在真实 AI 回合结束之前。
    if (manualG14) {
      await page.evaluate(() => window.localStorage.setItem(
        'vulcan.notification-preferences.local.v1',
        JSON.stringify({ version: 1, channels: { local: { enabled: true } },
          categories: { conversationCompleted: true, conversationFailed: true },
          privacy: { showConversationTitle: false } }),
      ));
      // Native history is the observable owner boundary; another event socket must not receive
      // this client-owned settlement merely to help the test discover its run identifier.
      // 原生通知历史是可观察的归属边界；测试不得为获取运行标识而让另一事件套接字
      // 接收此客户端所属的终态事件。
      assert.ok(debugNotificationIdentity);
      g14PriorToastTags = new Set(await readBrandedDebugToastTags(
        debugNotificationIdentity.appUserModelId,
      ));
    }
    if (captureProviderTools) {
      await rpc('service_save_provider_request_trace_settings', { request: { enabled: true } });
    }
    const aiPromptPrefix = 'This is an authorized local browser acceptance page:';
    const response = await rpc('service_submit_conversation_prompt', {
      clientSubmissionId: `vcn1:${initiatingNotificationClientId}:${randomUUID()}`,
      projectId: project.id,
      conversationId: null,
      draftSlotKey: `new:${project.id}`,
      prompt: `${aiPromptPrefix} ${fixtureUrl}. Use browser_open, then the Playwright browser_navigate and browser_snapshot tools. Click the button named "UI click fixture" with browser_click, take a screenshot with browser_take_screenshot, and report the page title. Leave the Core browser open so the user can review its live projection; do not call browser_close. Use browser tools directly; do not delegate or use shell/network tools.`,
    });
    assert.equal(typeof response.conversationId, 'string', JSON.stringify(response));
    const aiConversationId = response.conversationId;
    if (manualG14) {
      // The notification policy suppresses a completed turn visible in the focused GUI.
      // 通知策略会抑制前台 GUI 正在查看的会话完成提醒，因此在 AI 执行期间先最小化。
      await gui.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0].minimize());
      assert.equal(await gui.evaluate(({ BrowserWindow }) =>
        BrowserWindow.getAllWindows()[0].isMinimized()), true);
    }
    aiAcceptance = await until(async () => {
      const window = await rpcExact('service_session_event_hydration_v1', { sessionId: aiConversationId });
      const tools = window.events.filter((event) => event.event?.kind === 'tool_call')
        .map((event) => ({ name: event.event.tool_name, status: event.event.status }));
      const names = tools.map((tool) => tool.name);
      assert.ok(names.includes('browser_open'), JSON.stringify(tools));
      assert.ok(names.some((name) => name.includes('browser_navigate')), JSON.stringify(tools));
      assert.ok(names.some((name) => name.includes('browser_snapshot')), JSON.stringify(tools));
      assert.ok(names.some((name) => name.includes('browser_click')), JSON.stringify(tools));
      assert.ok(names.some((name) => name.includes('browser_take_screenshot')), JSON.stringify(tools));
      const screenshot = window.events.find((event) => event.event?.kind === 'tool_call'
        && event.event.tool_name.includes('browser_take_screenshot'))?.event;
      assert.ok(['completed', 'failed'].includes(screenshot?.status), JSON.stringify(tools));
      let screenshotImage = false;
      if (screenshot.status === 'completed') {
        // The image is saved as a generated resource; the model output only names its locator.
        // 图片保存为生成资源；模型输出只写明其定位符。
        assert.equal(typeof screenshot.output?.model_output, 'string');
        assert.match(screenshot.output.model_output, /attachment:\/\/\S+/);
        assert.ok(screenshot.generated_resources?.some((resource) => resource.kind === 'image'),
          JSON.stringify(screenshot.generated_resources));
        screenshotImage = true;
      }
      assert.ok(clicks >= 2, JSON.stringify({ clicks, tools }));
      return { conversationId: aiConversationId, tools, pageClicks: clicks - 1,
        screenshotImage,
        screenshotStatus: screenshot?.status,
        screenshotError: screenshot?.status === 'failed'
          ? String(JSON.stringify(screenshot.output)).slice(0, 1200) : null };
    }, 180_000);
    assert.equal(aiAcceptance.screenshotStatus, 'completed',
      JSON.stringify({ status: aiAcceptance.screenshotStatus, error: aiAcceptance.screenshotError }));
    assert.ok(!aiAcceptance.tools.some((tool) => tool.name.includes('browser_close')),
      'The model closed its browser before the required post-run projection check.');
    delete aiAcceptance.screenshotError;
    if (manualG14) {
      // The newly delivered native tag identifies this run without leaking its settlement to
      // another client. The exact GUI may have older notifications from earlier probes.
      // 新投递的原生标签在不向其他客户端泄漏终态事件的情况下标识本回合；
      // 同一 GUI 可能保留此前探针留下的旧通知。
      g14NotificationId = await until(async () => {
        const tags = await readBrandedDebugToastTags(debugNotificationIdentity.appUserModelId);
        const fresh = tags.filter((tag) => !g14PriorToastTags.has(tag)
          && /^notification:[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(tag));
        assert.equal(fresh.length, 1, JSON.stringify(fresh));
        return fresh[0];
      }, 60_000);
      const aiConversation = (await rpc('workbench.snapshot')).conversations.find(
        (conversation) => conversation.id === aiConversationId);
      assert.ok(aiConversation, 'The notified AI conversation disappeared from the workbench.');
      if (g14ReloadBeforeActivation) {
        // The toast stays in the native host while the Vite renderer is replaced.
        // Vite 渲染器更换期间，通知仍保留在原生宿主中。
        await gui.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0].restore());
        await page.reload({ waitUntil: 'domcontentloaded' });
        await conversationRow.waitFor({ timeout: 20_000 });
        await conversationRow.click();
        await until(async () => {
          // The row selection and transcript hydration are separate asynchronous steps.
          // 会话行选中与转录界面恢复是两个独立的异步步骤。
          const selected = await page.locator('[data-conversation-surface-id]').first()
            .getAttribute('data-conversation-surface-id');
          assert.equal(selected, acceptedConversationId);
          return true;
        }, 20_000);
        // A true reload must retain the submitting client identity, unlike a copied tab.
        // 真正重载必须保留提交回合的客户端身份，复制标签页则不能保留。
        const reloadedClientId = await page.evaluate(() => window.sessionStorage.getItem(
          'vulcan.notifications.client-instance.v1',
        ));
        assert.equal(reloadedClientId, initiatingNotificationClientId);
        await gui.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0].minimize());
      }
      // The conversation surface carries its exact Core identity; titles can collide.
      // 会话界面携带 Core 的精确身份；标题可能重复，不能作为导航验收依据。
      const selectedBeforeClick = await page.locator('[data-conversation-surface-id]').first()
        .getAttribute('data-conversation-surface-id');
      assert.notEqual(selectedBeforeClick, aiConversationId,
        'The target conversation was already selected before notification activation.');
      assert.equal(await gui.evaluate(({ BrowserWindow }) =>
        BrowserWindow.getAllWindows()[0].isMinimized()), true);
      // Capture each native-to-renderer hop without changing the production notification route.
      // 逐段记录原生到渲染端的传递，不修改生产通知路由。
      await gui.evaluate(({ app, BrowserWindow }) => {
        const window = BrowserWindow.getAllWindows()[0];
        const trace = { secondInstances: 0, sentIds: [] };
        globalThis.__w09G14HostTrace = trace;
        app.on('second-instance', () => { trace.secondInstances += 1; });
        const originalSend = window.webContents.send.bind(window.webContents);
        window.webContents.send = (channel, ...args) => {
          if (channel === 'vulcan:notification:activated') {
            trace.sentIds.push(args[0]?.notificationId ?? null);
          }
          return originalSend(channel, ...args);
        };
      });
      await page.evaluate(async () => {
        window.__w09G14RendererIds = [];
        await window.vulcanDesktop.desktop.subscribeNotificationActivation((activation) => {
          window.__w09G14RendererIds.push(activation.notificationId);
        });
      });
      console.log(`MANUAL_G14_READY id=${g14NotificationId} conversation=${aiConversationId}`);
      if (g14SimulateCom) {
        // The SDK callback tests route mechanics only; the result must retain a false physical-click flag.
        // SDK 回调只验证路由机制；验收结果必须明确保留“未物理点击”。
        if (g14ComDelayMs > 0) await new Promise((resolve) => setTimeout(resolve, g14ComDelayMs));
        await execFileAsync('rtk', [
          'proxy', 'pwsh', '-NoProfile', '-File',
          path.join(repository, 'apps/vulcan-code-desktop/tools/windows-toast-activate.ps1'),
          '-NotificationId', g14NotificationId, '-Profile', 'Debug',
          '-DebugExecutable', selectedDesktopRuntime,
        ], { timeout: 10_000 });
      }
      try {
        // Once Windows restores the window, ten seconds is sufficient for renderer navigation.
        // Windows 已还原窗口后，给渲染端十秒完成导航，再报告确定的路由失败。
        let restoredAt = null;
        const routeOutcome = await until(async () => {
          const minimized = await gui.evaluate(({ BrowserWindow }) =>
            BrowserWindow.getAllWindows()[0].isMinimized());
          assert.equal(minimized, false);
          if (restoredAt === null) restoredAt = Date.now();
          const selectedAfterClick = await page.locator('[data-conversation-surface-id]').first()
            .getAttribute('data-conversation-surface-id');
          if (selectedAfterClick !== aiConversationId) {
            if (Date.now() - restoredAt >= 10_000) {
              return { matched: false };
            }
            throw new Error('Waiting for the notification conversation route.');
          }
          const connection = await page.evaluate(() => window.vulcanDesktop.service.getConnection());
          assert.equal(connection.pid, service.pid);
          return { matched: true };
        }, g14SimulateCom ? 10_000 : 300_000);
        assert.equal(routeOutcome.matched, true, 'The restored GUI did not navigate to the notified conversation.');
      } catch (error) {
        // A real click may focus the GUI yet lose its navigation event at a later boundary.
        // 真实点击可能聚焦 GUI，却在后续边界丢失导航事件。
        const hostTrace = await gui.evaluate(() => globalThis.__w09G14HostTrace);
        const rendererIds = await page.evaluate(() => window.__w09G14RendererIds);
        const selected = await page.locator('[data-conversation-surface-id]').first()
          .getAttribute('data-conversation-surface-id');
        throw new Error(`G14 activation route failed: ${JSON.stringify({ hostTrace, rendererIds,
          selected, expected: aiConversationId })}`, { cause: error });
      }
      g14Acceptance = { physicalClickObserved: !g14SimulateCom,
        systemComActivationObserved: g14SimulateCom,
        rendererReloadedBeforeActivation: g14ReloadBeforeActivation,
        routedConversationId: aiConversationId, guiRestored: true,
        corePidUnchanged: true, notificationId: g14NotificationId };
      if (g14ReloadBeforeActivation
        && await contextPanelToggle.getAttribute('aria-pressed') !== 'true') {
        // Renderer reload resets the optional right panel; restore it for later browser UI checks.
        // 渲染器重载会重置可选右侧面板；后续浏览器界面检查前重新打开。
        await contextPanelToggle.click();
      }
    }
    if (captureProviderTools) {
      // Read only committed slots from this turn before later model turns can replace ring slots.
      // 在后续模型回合覆盖环槽之前，只读取本回合已提交的槽位。
      providerToolTrace = await until(async () => {
        const traces = await readProviderToolTraces(aiConversationId);
        assert.ok(traces.length >= 2, 'Expected repeated physical Provider requests.');
        const screenshotToolName = 'mcp_vulcan_browser_playwright_browser_take_screenshot';
        const screenshotTools = traces.map((trace) => trace.tools.find((tool) =>
          tool.name === screenshotToolName));
        assert.ok(screenshotTools.every(Boolean), 'A repeated Provider request lost the screenshot tool.');
        const descriptions = screenshotTools.map((tool) => tool.description);
        assert.ok(descriptions.every((description) => description === descriptions[0]),
          'Unchanged plugin generation produced different model-facing descriptions.');
        assert.ok(descriptions[0].includes('VulcanCode returns the screenshot as image content'),
          'The registration Hook did not reach the final Provider request.');
        assert.ok(descriptions[0].includes('Configured action timeout: 7000 ms.'),
          'The configured timeout did not reach the final Provider tool definition.');
        assert.ok(screenshotTools.every((tool) => tool.parameters?.properties?.filename?.description
          ?.includes('VulcanCode ignores it')),
        'The final Provider schema did not retain the registered filename contract.');
        assert.ok(traces.every((trace) => trace.tools.every((tool) =>
          tool.name !== 'mcp_vulcan_browser_playwright_browser_run_code_unsafe')),
        'The blocked unsafe MCP tool reached a final Provider request.');
        return { requestCount: traces.length, screenshotDefinitionStable: true,
          registrationHookReachedProvider: true, unsafeToolAbsent: true,
          initialActionTimeoutMs: '7000' };
      }, 15_000);
    }
    // Two ordinary browser clients must observe the same Core session without becoming its owner.
    // 两个普通浏览器客户端必须观察同一 Core 会话，且不能因此成为其所有者。
    const aiBrowser = (await rpc('browser.session.list', {}, aiConversationId))
      .find((session) => session.status === 'running');
    assert.ok(aiBrowser, 'The AI browser disappeared before cross-client observation.');
    // A finished logical model turn must retire its Playwright process and host grant while
    // preserving the Core browser for later user observation or a fresh AI turn.
    // 逻辑模型回合结束必须退休 Playwright 进程与宿主授权，同时保留 Core 浏览器供用户观察或下回合重连。
    aiTurnRetirementAcceptance = await until(async () => {
      const browser = await rpc('browser.session.snapshot', { sessionId: aiBrowser.sessionId }, aiConversationId);
      assert.equal(browser.status, 'running', JSON.stringify(browser));
      assert.equal(browser.automationState, 'detached', JSON.stringify(browser));
      assert.equal(browser.leaseState, 'free', JSON.stringify(browser));
      return { browserId: browser.sessionId, browserRetained: true,
        automationDetached: true, leaseReleased: true };
    }, 60_000);
    aiBrowserForDisable = aiBrowser;
    aiConversationForDisable = aiConversationId;
    const aiConversation = (await rpc('workbench.snapshot')).conversations
      .find((conversation) => conversation.id === aiConversationId);
    assert.ok(aiConversation, 'The AI conversation is missing from the Core workbench snapshot.');
    // Each selected conversation must see only its own Core browser, even with the other ID known.
    // 即使知道对方浏览器标识，每个选中会话也只能看到自己的 Core 浏览器。
    const firstOwnerSessions = await rpc('browser.session.list', {}, acceptedConversationId);
    const secondOwnerSessions = await rpc('browser.session.list', {}, aiConversationId);
    assert.ok(firstOwnerSessions.some((session) => session.sessionId === browserBinding.session.sessionId));
    assert.ok(!firstOwnerSessions.some((session) => session.sessionId === aiBrowser.sessionId));
    assert.ok(secondOwnerSessions.some((session) => session.sessionId === aiBrowser.sessionId));
    assert.ok(!secondOwnerSessions.some((session) => session.sessionId === browserBinding.session.sessionId));
    // protectedBrowserMethods cover read, mutation, control, stream, and file metadata routes.
    // protectedBrowserMethods 覆盖读取、变更、控制、画面流与文件元数据路由。
    const protectedBrowserMethods = [
      'browser.session.snapshot', 'browser.session.close', 'browser.session.navigate',
      'browser.pages.list', 'browser.page.create', 'browser.page.capture',
      'browser.lease.acquire', 'browser.input.dispatch', 'browser.stream.ticket',
      'browser.upload.list', 'browser.download.list', 'browser.dialog.list',
      'browser.fileChooser.list',
    ];
    for (const method of protectedBrowserMethods) {
      // foreign and absent must have indistinguishable errors before method-specific parsing.
      // foreign 与 absent 必须在方法专属解析前产生无法区分的错误。
      const foreign = await rpcResponse(method,
        { sessionId: browserBinding.session.sessionId }, aiConversationId);
      const absent = await rpcResponse(method,
        { sessionId: 'w09-nonexistent-browser' }, aiConversationId);
      assert.equal(foreign.ok, false, `${method}: ${JSON.stringify(foreign)}`);
      assert.equal(absent.ok, false, `${method}: ${JSON.stringify(absent)}`);
      assert.deepEqual(foreign.error, absent.error, `${method} leaked a foreign browser distinction.`);
    }
    crossOwnerProtectedMethodCount = protectedBrowserMethods.length;
    const knownForeignDownload = await fetch(downloadUrl,
      { headers: downloadHeaders(aiConversationId) });
    const knownAbsentDownload = await fetch(`${discovery.baseUrl}/v1/browser/downloads/${randomUUID()}/content`,
      { headers: downloadHeaders(aiConversationId) });
    assert.equal(knownForeignDownload.status, 404);
    assert.equal(knownAbsentDownload.status, knownForeignDownload.status);
    assert.equal(await knownForeignDownload.text(), await knownAbsentDownload.text());
    crossOwnerDownloadAcceptance.knownOtherConversationDenied = true;
    // Renderer zoom may leave the project sidebar collapsed after returning to 100%.
    // 渲染器缩放恢复至 100% 后，项目侧栏可能仍保持折叠；先用实际控件展开。
    const sidebarToggle = page.locator('.vcn-sidebar-trigger:visible').first();
    if ((await sidebarToggle.getAttribute('aria-expanded')) === 'false') await sidebarToggle.click();
    const aiConversationRow = page.locator('.conversation-tree-button')
      .filter({ hasText: aiConversation.title }).first();
    await aiConversationRow.waitFor({ timeout: 20_000 });
    await aiConversationRow.click();
    if (captureProviderTools) {
      const changedBrowserSettings = await rpc('service_invoke_plugin_action', {
        pluginId: 'vulcan-browser', actionId: 'settings-update',
        arguments: { ...updatedBrowserSettings, actionTimeoutMs: '9000' },
      });
      assert.equal(changedBrowserSettings.actionTimeoutMs, '9000');
    }
    const switchedPanel = page.locator('.context-tab-content.browser-tab-content [aria-label="AI 浏览器"], .context-tab-content.browser-tab-content [aria-label="AI browser"]').first();
    await until(async () => {
      const ids = await switchedPanel.locator('[data-browser-session-id]').evaluateAll((tabs) => tabs.map((tab) => tab.getAttribute('data-browser-session-id')));
      assert.ok(ids.includes(aiBrowser.sessionId), JSON.stringify(ids));
      assert.ok(!ids.includes(browserBinding.session.sessionId), JSON.stringify(ids));
      return true;
    }, 20_000);
    await conversationRow.click();
    await until(async () => {
      const ids = await switchedPanel.locator('[data-browser-session-id]').evaluateAll((tabs) => tabs.map((tab) => tab.getAttribute('data-browser-session-id')));
      assert.ok(ids.includes(browserBinding.session.sessionId), JSON.stringify(ids));
      assert.ok(!ids.includes(aiBrowser.sessionId), JSON.stringify(ids));
      return true;
    }, 20_000);
    // Leave the AI-owned conversation selected for compact clients without a visible tree.
    // 为没有可见会话树的紧凑客户端保留选中的 AI 会话。
    await aiConversationRow.click();
    webBrowser = await chromium.launch({
      headless: true,
      executablePath: path.join(assetRoot, ...runtimeLayout.chromiumExecutable.split('/')),
    });
    webAcceptance = [];
    for (const client of [{ name: 'web', width: 1280, height: 800, mobile: false },
      { name: 'mobile', width: 390, height: 844, mobile: true }]) {
      const browserContext = await webBrowser.newContext({
        viewport: { width: client.width, height: client.height },
        isMobile: client.mobile,
        hasTouch: client.mobile,
      });
      try {
        const webPage = await browserContext.newPage();
        await webPage.goto(`${discovery.baseUrl}/?service=${encodeURIComponent(discovery.baseUrl)}`);
        assert.equal(await webPage.locator('.window-controls').count(), 0,
          `${client.name} must not display desktop-only window controls.`);
        if (client.mobile) {
          // Compact layout hides the tree and opens the service-selected conversation directly.
          // 紧凑布局隐藏会话树，直接打开服务当前选中的会话。
          await webPage.getByText(aiPromptPrefix, { exact: false }).first()
            .waitFor({ timeout: 25_000 });
        } else {
          const conversation = webPage.locator('.conversation-tree-button')
            .filter({ hasText: aiConversation.title }).first();
          await conversation.waitFor({ timeout: 25_000 });
          await conversation.click();
        }
        const panel = webPage.locator('.context-tab-content.browser-tab-content [aria-label="AI 浏览器"], .context-tab-content.browser-tab-content [aria-label="AI browser"]').first();
        await panel.waitFor({ timeout: 25_000 });
        await panel.locator(`[data-browser-session-id="${aiBrowser.sessionId}"]`).click();
        try {
          await panel.getByRole('img', { name: /Core 浏览器实时页面|Live Core browser page/ })
            .waitFor({ timeout: 25_000 });
        } catch (error) {
          // projection and pages reveal whether the Core host or this observer lost the frame.
          // projection 与 pages 用于区分 Core 宿主丢帧和当前观察客户端丢帧。
          const projection = await rpc('browser.session.snapshot', { sessionId: aiBrowser.sessionId }, aiConversationId);
          const pages = await rpc('browser.pages.list', { sessionId: aiBrowser.sessionId }, aiConversationId);
          throw new Error(`${client.name} Core frame absent: ${JSON.stringify({ projection, pages, tools: aiAcceptance.tools, panel: await panel.innerText() })}`, { cause: error });
        }
        assert.equal(await panel.locator(`[data-browser-session-id="${aiBrowser.sessionId}"]`).count(), 1);
        assert.equal(await webPage.getByRole('button', { name: /内置浏览器|Local Browser/ }).count(), 0);
        webAcceptance.push({ client: client.name, coreBrowserId: aiBrowser.sessionId,
          projectionVisible: true, localBrowserUnavailable: true, desktopWindowControls: false });
      } finally {
        await browserContext.close();
      }
    }
    if (webNotificationProbe) {
      // This separate browser context owns its own permission and notification preference.
      // 此独立浏览器上下文拥有自己的权限和通知偏好。
      const notificationContext = await webBrowser.newContext({
        viewport: { width: 1280, height: 800 },
      });
      try {
        await notificationContext.grantPermissions(['notifications'], {
          origin: new URL(discovery.baseUrl).origin,
        });
        await notificationContext.addInitScript(() => {
          // The proxy delegates to Chromium and records permission requests and accepted toasts.
          // 代理委托 Chromium，并记录权限申请与已接受的通知。
          window.__w09WebNotificationCalls = [];
          window.__w09WebPermissionRequests = 0;
          window.Notification = new Proxy(window.Notification, {
            get(target, property) {
              if (property === 'requestPermission') {
                return (...args) => {
                  window.__w09WebPermissionRequests += 1;
                  return Reflect.apply(target.requestPermission, target, args);
                };
              }
              return Reflect.get(target, property, target);
            },
            construct(target, args) {
              const notification = Reflect.construct(target, args);
              window.__w09WebNotificationCalls.push({ title: args[0], tag: args[1]?.tag });
              return notification;
            },
          });
        });
        const notificationPage = await notificationContext.newPage();
        await notificationPage.goto(`${discovery.baseUrl}/?service=${encodeURIComponent(discovery.baseUrl)}`);
        await notificationPage.locator('.conversation-tree-button')
          .filter({ hasText: aiConversation.title }).first().waitFor({ timeout: 25_000 });
        // The actual WebUI switch must request permission from this browser, then show a test toast.
        // 必须由真实 WebUI 开关向当前浏览器申请权限，再发送测试通知。
        await notificationPage.locator('.sidebar-brand-settings-button').click();
        await notificationPage.locator('.settings-subarea-button')
          .filter({ hasText: /^通知$|^Notifications$/ }).click();
        const notificationSettings = notificationPage.locator('.notification-settings-page');
        await notificationSettings.getByRole('switch', {
          name: /本地系统通知|Local system notifications/,
        }).click();
        await until(async () => {
          const state = await notificationPage.evaluate(() => ({
            permissionRequests: window.__w09WebPermissionRequests,
            preference: JSON.parse(window.localStorage.getItem('vulcan.notification-preferences.local.v1')),
          }));
          assert.equal(state.permissionRequests, 1);
          assert.equal(state.preference.channels.local.enabled, true);
          return state;
        }, 10_000);
        await notificationSettings.getByRole('button', {
          name: /发送测试通知|Send test notification/,
        }).click();
        await until(async () => {
          const calls = await notificationPage.evaluate(() => window.__w09WebNotificationCalls);
          assert.ok(calls.some((call) => call.tag === 'notification-settings-test'),
            JSON.stringify(calls));
          return true;
        }, 10_000);
        const webNotificationClientId = await notificationPage.evaluate(() =>
          window.sessionStorage.getItem('vulcan.notifications.client-instance.v1'));
        assert.match(webNotificationClientId,
          /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
        assert.equal(await notificationPage.evaluate(() => window.Notification.permission), 'granted');
        const desktopTagsBefore = debugNotificationIdentity
          ? new Set(await readBrandedDebugToastTags(debugNotificationIdentity.appUserModelId)) : null;
        const notificationRun = await rpc('service_submit_conversation_prompt', {
          clientSubmissionId: `vcn1:${webNotificationClientId}:${randomUUID()}`,
          projectId: project.id,
          conversationId: null,
          draftSlotKey: `new:${project.id}`,
          prompt: 'Reply exactly OK. Do not call any tools.',
        });
        assert.equal(typeof notificationRun.conversationId, 'string');
        const browserNotification = await until(async () => {
          const calls = await notificationPage.evaluate(() => window.__w09WebNotificationCalls);
          const matching = calls.filter((call) =>
            /^notification:[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(call.tag));
          assert.equal(matching.length, 1, JSON.stringify(calls));
          return matching[0];
        }, 90_000);
        if (desktopTagsBefore) {
          const desktopTagsAfter = await readBrandedDebugToastTags(
            debugNotificationIdentity.appUserModelId);
          const foreignDesktopTags = desktopTagsAfter.filter((tag) =>
            !desktopTagsBefore.has(tag) && /^notification:/.test(tag));
          assert.deepEqual(foreignDesktopTags, [],
            'A browser-owned completion appeared under the desktop notification identity.');
        }
        webNotificationAcceptance = {
          ownerClientId: webNotificationClientId,
          conversationId: notificationRun.conversationId,
          nativeBrowserNotification: true,
          notificationTag: browserNotification.tag,
          permission: 'granted',
          permissionRequestedByWebUi: true,
          settingsTestNotificationDelivered: true,
          desktopNotificationAbsent: desktopTagsBefore !== null,
        };
      } finally {
        await notificationContext.close();
      }
    }
    // Two clients of one VulcanCode conversation must share the browser inventory while each
    // keeps its own tab selection. A close by the second client must update the first client.
    // 同一 VulcanCode 会话的两个客户端共享浏览器列表，但各自保留标签选择；第二端关闭后第一端必须更新。
    await conversationRow.click();
    await until(async () => {
      assert.equal(await conversationRow.getAttribute('aria-current'), 'page');
      return true;
    }, 10_000);
    await browserPanel.waitFor({ timeout: 20_000 });
    const createdAdditionalBrowser = await rpc('browser.session.create',
      { profileMode: 'ephemeral' }, acceptedConversationId);
    assert.equal(createdAdditionalBrowser.status, 'running');
    const secondOwnedBrowser = await until(async () => {
      const running = (await rpc('browser.session.list', {}, acceptedConversationId))
        .filter((session) => session.status === 'running');
      assert.equal(running.length, 2, JSON.stringify(running));
      const second = running.find((session) => session.sessionId !== browserBinding.session.sessionId);
      assert.ok(second, JSON.stringify(running));
      return second;
    }, 20_000);
    assert.equal(secondOwnedBrowser.sessionId, createdAdditionalBrowser.sessionId);
    await browserPanel.getByRole('button', { name: /刷新浏览器列表|Refresh browser list/ }).click();
    const desktopTabs = browserPanel.getByRole('tablist', { name: /活跃的 AI 浏览器|Active AI browsers/ });
    const firstDesktopTab = desktopTabs.locator(`[data-browser-session-id="${browserBinding.session.sessionId}"]`);
    const secondDesktopTab = desktopTabs.locator(`[data-browser-session-id="${secondOwnedBrowser.sessionId}"]`);
    await firstDesktopTab.waitFor({ timeout: 15_000 });
    await secondDesktopTab.waitFor({ timeout: 15_000 });
    await firstDesktopTab.click();
    assert.equal(await firstDesktopTab.getAttribute('aria-selected'), 'true');
    await secondDesktopTab.click();
    assert.equal(await secondDesktopTab.getAttribute('aria-selected'), 'true');
    const sharedClientContext = await webBrowser.newContext({ viewport: { width: 1280, height: 800 } });
    try {
      const sharedClientPage = await sharedClientContext.newPage();
      await sharedClientPage.goto(`${discovery.baseUrl}/?service=${encodeURIComponent(discovery.baseUrl)}`);
      await sharedClientPage.locator('.conversation-tree-button')
        .filter({ hasText: 'W09 browser UI persistent conversation fixture' }).first().click();
      const sharedPanel = sharedClientPage.locator('.context-tab-content.browser-tab-content [aria-label="AI 浏览器"], .context-tab-content.browser-tab-content [aria-label="AI browser"]').first();
      await sharedPanel.waitFor({ timeout: 20_000 });
      await sharedPanel.getByRole('button', { name: /刷新浏览器列表|Refresh browser list/ }).click();
      const sharedTabs = sharedPanel.getByRole('tablist', { name: /活跃的 AI 浏览器|Active AI browsers/ });
      const firstSharedTab = sharedTabs.locator(`[data-browser-session-id="${browserBinding.session.sessionId}"]`);
      const secondSharedTab = sharedTabs.locator(`[data-browser-session-id="${secondOwnedBrowser.sessionId}"]`);
      await firstSharedTab.waitFor({ timeout: 20_000 });
      await secondSharedTab.waitFor({ timeout: 20_000 });
      await firstSharedTab.click();
      assert.equal(await firstSharedTab.getAttribute('aria-selected'), 'true');
      await secondSharedTab.click();
      assert.equal(await secondSharedTab.getAttribute('aria-selected'), 'true');
      await sharedPanel.getByRole('button', { name: /关闭浏览器|Close browser/ }).click();
      await until(async () => {
        const closed = (await rpc('browser.session.list', {}, acceptedConversationId))
          .find((session) => session.sessionId === secondOwnedBrowser.sessionId);
        assert.equal(closed?.status, 'closed', JSON.stringify(closed));
        return true;
      }, 15_000);
      await browserPanel.getByRole('button', { name: /刷新浏览器列表|Refresh browser list/ }).click();
      await secondDesktopTab.waitFor({ state: 'detached', timeout: 15_000 });
      await firstDesktopTab.waitFor({ timeout: 15_000 });
      multiClientBrowserAcceptance = {
        ownerSessionId: acceptedConversationId, clients: ['desktop', 'web'],
        sharedOpenSessionCount: 2, independentTabSelection: true,
        secondClientClosedBrowser: secondOwnedBrowser.sessionId,
        firstClientObservedClose: true,
      };
    } finally {
      await sharedClientContext.close();
    }
    await aiConversationRow.click();
    // A second real model turn opens a separate Core browser. Its navigation is held by this
    // fixture until Stop completes, leaving the earlier reviewed browser available afterward.
    // 第二个真实模型回合打开独立 Core 浏览器；夹具在 Stop 完成前挂起其导航，
    // 使先前供用户查看的浏览器在取消后仍可用。
    const stalledUrl = new URL(stalledPagePath, fixtureUrl).href;
    const cancelSubmission = await rpc('service_submit_conversation_prompt', {
      clientSubmissionId: `w09-ai-stop-${Date.now()}`,
      projectId: project.id,
      conversationId: aiConversationId,
      draftSlotKey: `conversation:${aiConversationId}`,
      prompt: `W09 automated cancellation test: the test controller will intentionally stop this turn while browser navigation is pending. Use browser_open to create a separate Core browser, then use browser_navigate to ${stalledUrl}. Wait for navigation to finish before responding. Keep the earlier browser ${aiBrowser.sessionId} open. Use browser tools directly; do not delegate or use shell/network tools.`,
    });
    assert.equal(cancelSubmission.conversationId, aiConversationId);
    await until(async () => {
      assert.equal(stalledPageRequestSeen, true, 'AI has not reached the delayed page.');
      return true;
    }, 90_000);
    if (captureProviderTools) {
      const changedProviderRequests = await until(async () => {
        const traces = (await readProviderToolTraces(aiConversationId, stalledUrl))
          .filter((trace) => trace.promptMarkerPresent);
        assert.ok(traces.length > 0, 'No Provider request followed the plugin configuration change.');
        const screenshotName = 'mcp_vulcan_browser_playwright_browser_take_screenshot';
        const observedTimeouts = traces.map((trace) => ({ sequence: trace.sequence,
          timeout: trace.tools.find((tool) => tool.name === screenshotName)?.description
            ?.match(/Configured action timeout: ([0-9]+) ms\./)?.[1] ?? null }));
        assert.ok(observedTimeouts.every((trace) => trace.timeout === '9000'),
          `A later Provider request reused the stale registered timeout: ${JSON.stringify(observedTimeouts)}`);
        return traces.length;
      }, 15_000);
      providerToolTrace.configRevisionReachedProvider = true;
      providerToolTrace.afterConfigRequestCount = changedProviderRequests;
      providerToolTrace.updatedActionTimeoutMs = '9000';
    }
    const cancelledBrowser = (await rpc('browser.session.list', {}, aiConversationId))
      .find((session) => session.status === 'running' && session.sessionId !== aiBrowser.sessionId);
    assert.ok(cancelledBrowser, 'The in-flight navigation did not own a separate Core browser.');
    console.log(`W09_INTENTIONAL_STOP conversation=${aiConversationId} browser=${cancelledBrowser.sessionId}`);
    const stoppedAiTurn = await rpc('service_stop_conversation_run', {
      conversationId: aiConversationId, mode: 'main_only',
    });
    assert.equal(stoppedAiTurn.status, 'stopped', stoppedAiTurn.status);
    if (stalledPageResponse && !stalledPageResponse.destroyed) {
      stalledPageResponse.writeHead(200, { 'content-type': 'text/html; charset=utf-8' });
      stalledPageResponse.end('<!doctype html><title>Stopped navigation fixture</title>');
    }
    const retainedAfterStop = (await rpc('workbench.snapshot')).conversations
      .find((conversation) => conversation.id === aiConversationId);
    assert.ok(retainedAfterStop, 'Stopping the AI turn removed its VulcanCode conversation.');
    const browserAfterStop = await rpc('browser.session.snapshot',
      { sessionId: cancelledBrowser.sessionId }, aiConversationId);
    assert.equal(browserAfterStop.automationState, 'detached', JSON.stringify(browserAfterStop));
    assert.equal(browserAfterStop.leaseState, 'free', JSON.stringify(browserAfterStop));
    assert.ok(['running', 'closed'].includes(browserAfterStop.status), JSON.stringify(browserAfterStop));
    aiCancellationAcceptance = {
      conversationRetained: true, intentionalStop: true, inFlightNavigationObserved: true,
      browserId: cancelledBrowser.sessionId, browserStatus: browserAfterStop.status,
      automationDetached: true, leaseReleased: true,
    };
    // A separate model turn must be interrupted by a user's Core takeover while a real page
    // navigation is still in flight; unknown writes require closing the browser before reuse.
    // 独立模型回合的真实页面导航尚在途时，由用户从 Core 发起接管；
    // 未知写入必须先关闭浏览器，不能直接重用。
    stalledPageRequestSeen = false;
    stalledPageResponse = null;
    stalledPagePath = `/w09-stall/${randomUUID()}`;
    const takeoverStalledUrl = new URL(stalledPagePath, fixtureUrl).href;
    const takeoverSubmission = await rpc('service_submit_conversation_prompt', {
      clientSubmissionId: `w09-ai-takeover-${Date.now()}`,
      projectId: project.id,
      conversationId: null,
      draftSlotKey: `new:${project.id}`,
      prompt: `W09 automated takeover test: the test controller will intentionally interrupt this turn during browser navigation. Use browser_open to create a Core browser, then use browser_navigate to ${takeoverStalledUrl}. Wait for navigation to finish before responding. Do not use shell/network tools or delegate.`,
    });
    const takeoverConversationId = takeoverSubmission.conversationId;
    try {
      await until(async () => {
        assert.equal(stalledPageRequestSeen, true, 'The takeover model has not reached the delayed page.');
        return true;
      }, 90_000);
    } catch (error) {
      // A model may finish, refuse, or lose its browser before the fixture sees a request. Record
      // the bounded answer to this synthetic prompt and tool statuses to distinguish refusal
      // from host failure without printing provider requests or credential material.
      // 模型可能在夹具收到请求前完成、拒绝或失去浏览器。仅记录本合成提示的有限答复与工具状态，
      // 以便区分模型行为与宿主故障。
      const [history, browsers] = await Promise.allSettled([
        rpcExact('service_session_event_hydration_v1', { sessionId: takeoverConversationId }),
        rpc('browser.session.list', {}, takeoverConversationId),
      ]);
      const events = history.status === 'fulfilled' ? history.value.events : [];
      console.error('W09_TAKEOVER_WAIT_DIAGNOSTIC', JSON.stringify({
        conversationId: takeoverConversationId,
        openTurnId: history.status === 'fulfilled' ? history.value.open_turn_id : null,
        eventKinds: events.map((item) => item.event.kind).slice(-24),
        toolCalls: events.filter((item) => item.event.kind === 'tool_call')
          .map((item) => ({ name: item.event.tool_name, status: item.event.status })).slice(-12),
        assistantTexts: events.filter((item) => item.event.kind === 'assistant_text')
          .map((item) => ({ status: item.event.status, length: item.event.text.length,
            excerpt: item.event.text.slice(0, 750) })).slice(-5),
        displayNotes: events.filter((item) => item.event.kind === 'display_note')
          .map((item) => ({ reasonCode: item.event.reason_code,
            severity: item.event.severity })).slice(-5),
        browsers: browsers.status === 'fulfilled'
          ? browsers.value.map((browser) => ({ sessionId: browser.sessionId,
            status: browser.status, automationState: browser.automationState,
            currentUrl: browser.currentUrl })) : [],
        historyError: history.status === 'rejected' ? String(history.reason) : null,
        browserError: browsers.status === 'rejected' ? String(browsers.reason) : null,
      }));
      throw error;
    }
    const takeoverBrowser = (await rpc('browser.session.list', {}, takeoverConversationId))
      .find((session) => session.status === 'running');
    assert.ok(takeoverBrowser, 'The in-flight takeover lacks a Core browser.');
    assert.equal(takeoverBrowser.automationState, 'attached', JSON.stringify(takeoverBrowser));
    const takeoverAttempt = await rpcResponse('browser.lease.takeover', {
      sessionId: takeoverBrowser.sessionId, holder: 'w09-user-takeover',
    }, takeoverConversationId);
    assert.equal(takeoverAttempt.ok, false,
      'An unconfirmed AI navigation must not grant the user a fabricated lease.');
    const afterTakeover = await rpc('browser.session.snapshot',
      { sessionId: takeoverBrowser.sessionId }, takeoverConversationId);
    assert.equal(afterTakeover.status, 'closed', JSON.stringify(afterTakeover));
    assert.equal(afterTakeover.automationState, 'detached', JSON.stringify(afterTakeover));
    assert.equal(afterTakeover.leaseState, 'free', JSON.stringify(afterTakeover));
    const takeoverStop = await rpc('service_stop_conversation_run', {
      conversationId: takeoverConversationId, mode: 'main_only',
    });
    assert.equal(takeoverStop.status, 'stopped');
    if (stalledPageResponse && !stalledPageResponse.destroyed) {
      stalledPageResponse.writeHead(200, { 'content-type': 'text/html; charset=utf-8' });
      stalledPageResponse.end('<!doctype html><title>Takeover navigation fixture</title>');
    }
    assert.equal((await rpc('browser.session.snapshot',
      { sessionId: takeoverBrowser.sessionId }, takeoverConversationId)).status, 'closed');
    aiTakeoverAcceptance = {
      intentionalInterruption: true, inFlightNavigationObserved: true, userTakeoverAttempted: true,
      fabricatedLeaseDenied: true, uncertainBrowserClosed: true,
      automationDetached: true, staleNavigationCannotWrite: true,
    };
    // A fourth real model turn reads only this disposable Core's discovery file through the
    // ordinary shell tool, proving its result is filtered before model replay and persistence.
    // 第四个真实模型回合通过普通 shell 工具读取本次可丢弃 Core 的发现文件，
    // 验证结果在模型重放和持久化前已被过滤。
    const shellProbeSubmission = await rpc('service_submit_conversation_prompt', {
      clientSubmissionId: `w09-shell-secret-${Date.now()}`,
      projectId: project.id,
      conversationId: null,
      draftSlotKey: `new:${project.id}`,
      prompt: `Call shell_command once with this exact PowerShell command, without selecting fields or masking the command output: Get-Content -LiteralPath '${path.join(root, 'user-vulcan/code/webui-service.json')}' -Raw. Then report whether the tool result shows the serviceToken value or a redaction marker. Do not copy any credential into your final answer. Do not use browser tools or delegate.`,
    });
    const shellProbeConversationId = shellProbeSubmission.conversationId;
    const shellProbeHistory = await until(async () => {
      const history = await rpcExact('service_session_event_hydration_v1',
        { sessionId: shellProbeConversationId });
      const completedShellCalls = history.events.filter((entry) =>
        entry.event?.kind === 'tool_call' && entry.event.tool_name === 'shell_command'
        && entry.event.status === 'completed');
      assert.ok(completedShellCalls.length > 0, 'The model has not completed shell_command.');
      const shellOutcomes = completedShellCalls.map((entry) => {
        const output = JSON.stringify(entry.event.output ?? null);
        return { hasMarker: output.includes('[redacted]'),
          hasOriginalToken: output.includes(discovery.serviceToken), outputLength: output.length };
      });
      assert.ok(shellOutcomes.some((outcome) => outcome.hasMarker),
        `The shell tool did not return a redacted credential marker: ${JSON.stringify(shellOutcomes)}`);
      return history;
    }, 90_000);
    assert.ok(!JSON.stringify(shellProbeHistory).includes(discovery.serviceToken),
      'The shell probe retained the unredacted service token.');
    shellSecretRedactionAcceptance = {
      modelCalledShell: true, serviceTokenRedactedBeforeHistory: true,
      shellToolEvents: shellProbeHistory.events.filter((entry) =>
        entry.event?.kind === 'tool_call' && entry.event.tool_name === 'shell_command').length,
    };
    // Scan the complete retained histories and Core diagnostics using actual generated secrets.
    // 以本次实际生成的秘密扫描完整保留历史和 Core 诊断，而不是只匹配字段名样例。
    assert.ok(capturedUiLeaseSecret, 'The real UI control lease was not captured for auditing.');
    const histories = await Promise.all([acceptedConversationId, aiConversationId, takeoverConversationId, shellProbeConversationId].map((sessionId) =>
      rpcExact('service_session_event_hydration_v1', { sessionId })));
    assert.equal(histories[0].events.some((entry) => entry.event?.kind === 'tool_call'), false,
      'The UI-only fixture unexpectedly executed a model tool.');
    const historyText = JSON.stringify(histories);
    const diagnosticText = diagnostic.join('');
    for (const [label, secret] of [
      ['service token', discovery.serviceToken],
      ['control lease', capturedUiLeaseSecret],
      ['HttpOnly cookie', cookieSentinel],
    ]) {
      // Report only the object path containing a secret, never the secret or its surrounding value.
      // 仅报告含秘密的对象路径，绝不输出秘密本身或周边内容。
      const containingPaths = [];
      const pendingValues = histories.map((value, index) => ({ value, path: `history[${index}]` }));
      while (pendingValues.length > 0) {
        const { value, path: currentPath } = pendingValues.pop();
        if (typeof value === 'string' && value.includes(secret)) containingPaths.push(currentPath);
        else if (Array.isArray(value)) value.forEach((entry, index) =>
          pendingValues.push({ value: entry, path: `${currentPath}[${index}]` }));
        else if (value !== null && typeof value === 'object') Object.entries(value).forEach(([key, entry]) =>
          pendingValues.push({ value: entry, path: `${currentPath}.${key}` }));
      }
      // Name the producing tool without copying its model output into test diagnostics.
      // 标出产出工具，但不把其模型输出复制到测试诊断。
      const producingTools = histories.flatMap((history) => history.events
        .filter((entry) => typeof entry.event?.output?.model_output === 'string'
          && entry.event.output.model_output.includes(secret))
        .map((entry) => entry.event.tool_name));
      assert.ok(!historyText.includes(secret), `${label} leaked into retained model history at ${containingPaths.join(', ')}; tools=${JSON.stringify(producingTools)}.`);
      assert.ok(!diagnosticText.includes(secret), `${label} leaked into Core diagnostics.`);
    }
    assert.ok(!/ws:\/\/127\.0\.0\.1:\d+\/devtools\/browser\//.test(historyText),
      'A long-lived browser CDP endpoint leaked into model history.');
    sensitiveDataAudit = { historiesScanned: 4, coreDiagnosticsScanned: true,
      serviceTokenAbsent: true, controlLeaseAbsent: true, httpOnlyCookieAbsent: true,
      longLivedCdpCapabilityAbsent: true };
  }
  // Disabling the browser plugin must revoke its UI mount while Core-owned sessions survive.
  // 停用浏览器插件必须撤销界面挂载，同时保留由 Core 持有的既有浏览器会话。
  const aiBeforeDisable = aiBrowserForDisable
    ? await rpc('browser.session.snapshot', { sessionId: aiBrowserForDisable.sessionId }, aiConversationForDisable)
    : null;
  const disabledCatalog = await rpc('service_set_vulcan_plugin_enabled',
    { pluginId: 'vulcan-browser', enabled: false });
  const disabledPlugin = disabledCatalog.plugins.find((plugin) => plugin.id === 'vulcan-browser');
  assert.equal(disabledPlugin?.enabled, false);
  assert.equal(disabledPlugin?.runtimeState, 'disabled');
  await until(async () => {
    assert.equal(await page.locator('.context-tab-content.browser-tab-content [aria-label="AI 浏览器"], .context-tab-content.browser-tab-content [aria-label="AI browser"]').count(), 0);
    return true;
  }, 10_000);
  const firstOwnerAfterDisable = await rpc('browser.session.list', {}, acceptedConversationId);
  assert.ok(firstOwnerAfterDisable.some((session) => session.sessionId === browserBinding.session.sessionId
    && session.status === 'running'));
  const aiAfterDisable = aiBrowserForDisable
    ? await rpc('browser.session.snapshot', { sessionId: aiBrowserForDisable.sessionId }, aiConversationForDisable)
    : null;
  if (aiAfterDisable) {
    assert.equal(aiAfterDisable.automationState, 'detached', JSON.stringify(aiAfterDisable));
    assert.equal(aiAfterDisable.leaseState, 'free', JSON.stringify(aiAfterDisable));
    assert.equal(aiAfterDisable.status, 'running', JSON.stringify(aiAfterDisable));
  }
  const pluginDisableAcceptance = { uiMountRevoked: true, existingCoreSessionPreserved: true,
    aiAutomationBeforeDisable: aiBeforeDisable?.automationState ?? null,
    aiAutomationRevoked: aiAfterDisable?.automationState === 'detached',
    aiBrowserRetained: aiAfterDisable?.status === 'running' };
  // A copied package with a distinct id exercises the user-installed registry path without
  // changing the bundled browser plugin or the real user's plugin directory.
  // 使用独立 ID 的复制包验证用户安装来源，不改变内建浏览器插件或真实用户插件目录。
  const userPluginId = 'w09-user-browser-fixture';
  const userPluginSource = path.join(root, userPluginId);
  await cp(browserPluginSource, userPluginSource, { recursive: true });
  for (const relative of ['plugin.yaml', 'runtime/plugin.bundle.lua', 'ui/index.mjs']) {
    const sourceFile = path.join(userPluginSource, relative);
    await writeFile(sourceFile, (await readFile(sourceFile, 'utf8')).replaceAll('vulcan-browser', userPluginId));
  }
  const userPluginPreview = await rpc('service_inspect_local_plugin', { sourcePath: userPluginSource });
  assert.equal(userPluginPreview.pluginId, userPluginId);
  // Registry-backed installation requires explicit consent before the user package is published.
  // 依赖软件源的安装必须先取得明确授权，才能发布用户插件包。
  const unapprovedUserInstall = await rpcResponse('service_install_local_plugin', {
    sourcePath: userPluginSource,
    contentFingerprint: userPluginPreview.contentFingerprint,
    approvedPermissions: userPluginPreview.permissions,
    allowRegistryDependencies: false,
    initialSecrets: [],
  });
  assert.equal(unapprovedUserInstall.ok, false, JSON.stringify(unapprovedUserInstall));
  assert.equal((await rpc('workbench.snapshot')).plugins.some(plugin => plugin.id === userPluginId), false);
  const installedUserPluginCatalog = await rpc('service_install_local_plugin', {
    sourcePath: userPluginSource,
    contentFingerprint: userPluginPreview.contentFingerprint,
    approvedPermissions: userPluginPreview.permissions,
    allowRegistryDependencies: true,
    initialSecrets: [],
  });
  const installedUserPlugin = installedUserPluginCatalog.plugins.find(plugin => plugin.id === userPluginId);
  assert.equal(installedUserPlugin?.enabled, false);
  // Explicit package installation already prepares approved registry dependencies. Damage only
  // this disposable private environment to exercise the later one-click repair path.
  // 显式安装插件包时已准备获准的 registry 依赖；仅损坏本次可丢弃的私有环境以验证后续一键修复。
  assert.equal(await rpc('service_plugin_environment_ready', { pluginId: userPluginId }), true);
  const userPrivateEnvironment = await realpath(path.join(root, 'user-vulcan/code/plugin-runtime',
    userPluginId, '0.1.0', userPluginPreview.contentFingerprint));
  const userMcpPackage = await realpath(path.join(userPrivateEnvironment, 'node/node_modules/@playwright/mcp'));
  const userMcpManifest = JSON.parse(await readFile(path.join(userMcpPackage, 'package.json'), 'utf8'));
  const userMcpBin = await realpath(path.join(userMcpPackage, userMcpManifest.bin['playwright-mcp']));
  const userBinRelative = path.relative(userPrivateEnvironment, userMcpBin);
  assert.ok(userBinRelative && !userBinRelative.startsWith('..') && !path.isAbsolute(userBinRelative));
  await unlink(userMcpBin);
  assert.equal(await rpc('service_plugin_environment_ready', { pluginId: userPluginId }), false);
  const damagedUserEnable = await rpcResponse('service_set_vulcan_plugin_enabled',
    { pluginId: userPluginId, enabled: true });
  assert.equal(damagedUserEnable.ok, false, JSON.stringify(damagedUserEnable));
  const userInstall = await rpc('service_start_plugin_environment_install', { pluginId: userPluginId });
  const duplicateUserInstall = await rpc('service_start_plugin_environment_install', { pluginId: userPluginId });
  assert.equal(duplicateUserInstall.taskId, userInstall.taskId);
  const completedUserInstall = await until(async () => {
    const status = await rpc('service_plugin_environment_install_status', { taskId: userInstall.taskId });
    assert.notEqual(status.state, 'failed', JSON.stringify(status));
    assert.equal(status.state, 'succeeded');
    return status;
  }, 120_000);
  assert.equal(await rpc('service_plugin_environment_ready', { pluginId: userPluginId }), true);
  const afterUserInstallCatalog = await rpc('workbench.snapshot');
  assert.equal(afterUserInstallCatalog.plugins.find(plugin => plugin.id === userPluginId)?.enabled, false);
  // missingPackageRelative proves the package to be removed belongs only to this test plugin's
  // private generation and not to the shared Node runtime or another installed plugin.
  // missingPackageRelative 证明待删除的包仅属于本测试插件的私有代次，不属于共享 Node 运行时或其他插件。
  const missingPackageRelative = path.relative(userPrivateEnvironment, userMcpPackage);
  assert.ok(missingPackageRelative && !missingPackageRelative.startsWith('..')
    && !path.isAbsolute(missingPackageRelative));
  await rm(userMcpPackage, { recursive: true, force: true, maxRetries: 10, retryDelay: 500 });
  assert.equal(await rpc('service_plugin_environment_ready', { pluginId: userPluginId }), false);
  // missingPackageEnable checks that an intact marker cannot bypass the absent Node package.
  // missingPackageEnable 检查完整标记不能绕过缺失的 Node 包。
  const missingPackageEnable = await rpcResponse('service_set_vulcan_plugin_enabled',
    { pluginId: userPluginId, enabled: true });
  assert.equal(missingPackageEnable.ok, false, missingPackageEnable.error?.message);
  // packageRepair is the explicit Core task that replaces only this disabled private generation.
  // packageRepair 是显式 Core 任务，仅替换这个已停用的私有代次。
  const packageRepair = await rpc('service_start_plugin_environment_install', { pluginId: userPluginId });
  // completedPackageRepair is observed independently from the install request.
  // completedPackageRepair 由安装请求之外的独立查询观察得到。
  const completedPackageRepair = await until(async () => {
    const status = await rpc('service_plugin_environment_install_status', { taskId: packageRepair.taskId });
    assert.notEqual(status.state, 'failed', JSON.stringify(status));
    assert.equal(status.state, 'succeeded');
    return status;
  }, 120_000);
  assert.equal(await rpc('service_plugin_environment_ready', { pluginId: userPluginId }), true);
  const userPluginInstallAcceptance = {
    pluginId: userPluginId, taskId: userInstall.taskId,
    unapprovedInstallRejectedBeforePublication: true,
    missingExecutableRejected: true, enableRefused: true,
    duplicateCoalesced: true, state: completedUserInstall.state, remainedDisabled: true,
    missingNodePackageRejected: true, packageRepairTaskId: packageRepair.taskId,
    packageRepairState: completedPackageRepair.state,
  };
  // updateSource is a disposable candidate independent of the installed package generation.
  // updateSource 是独立于已安装插件包代次的一次性更新候选。
  const updateSource = path.join(root, 'update-candidate', userPluginId);
  await mkdir(path.dirname(updateSource), { recursive: true });
  await cp(userPluginSource, updateSource, { recursive: true });
  // updateManifest advances only the package version so the normal candidate stays functional.
  // updateManifest 仅推进插件包版本，使正常候选仍保持功能完整。
  const updateManifestPath = path.join(updateSource, 'plugin.yaml');
  const updateManifest = (await readFile(updateManifestPath, 'utf8'))
    .replace('version: 0.1.0', 'version: 0.1.1');
  assert.notEqual(updateManifest, await readFile(updateManifestPath, 'utf8'));
  await writeFile(updateManifestPath, updateManifest);
  // updatePreview binds every update request to the inspected bytes and approved permissions.
  // updatePreview 将每次更新请求绑定到检查过的字节和已批准权限。
  const updatePreview = await rpc('service_inspect_local_plugin', { sourcePath: updateSource });
  assert.equal(updatePreview.pluginId, userPluginId);
  // deniedUpdate proves registry dependencies never install without explicit update consent.
  // deniedUpdate 证明未明确授权时更新不会安装 registry 依赖。
  const deniedUpdate = await rpcResponse('service_update_local_plugin', {
    sourcePath: updateSource,
    contentFingerprint: updatePreview.contentFingerprint,
    approvedPermissions: updatePreview.permissions,
    allowRegistryDependencies: false,
    initialSecrets: [],
  });
  assert.equal(deniedUpdate.ok, false, JSON.stringify(deniedUpdate));
  // afterDeniedUpdate still projects the old disabled package and its ready environment.
  // afterDeniedUpdate 仍投影旧的停用插件包及其已就绪环境。
  const afterDeniedUpdate = await rpc('workbench.snapshot');
  assert.equal(afterDeniedUpdate.plugins.find(plugin => plugin.id === userPluginId)?.version, '0.1.0');
  assert.equal(await rpc('service_plugin_environment_ready', { pluginId: userPluginId }), true);
  // brokenUpdateSource remains syntactically valid but declares a nonexistent MCP package bin.
  // brokenUpdateSource 语法上仍然有效，但声明不存在的 MCP 包入口。
  const brokenUpdateSource = path.join(root, 'broken-update-candidate', userPluginId);
  await mkdir(path.dirname(brokenUpdateSource), { recursive: true });
  await cp(updateSource, brokenUpdateSource, { recursive: true });
  // brokenManifest reaches candidate environment verification after the old package is retained.
  // brokenManifest 会在保留旧插件包后进入候选环境验证。
  const brokenManifestPath = path.join(brokenUpdateSource, 'plugin.yaml');
  const brokenManifest = (await readFile(brokenManifestPath, 'utf8'))
    .replace('bin: playwright-mcp', 'bin: w09-missing-playwright-mcp-bin');
  assert.notEqual(brokenManifest, await readFile(brokenManifestPath, 'utf8'));
  await writeFile(brokenManifestPath, brokenManifest);
  // brokenPreview is inspected separately so the rollback test passes the exact fingerprint gate.
  // brokenPreview 单独检查，使回滚测试通过精确指纹关卡。
  const brokenPreview = await rpc('service_inspect_local_plugin', { sourcePath: brokenUpdateSource });
  // brokenUpdate must fail after staging and restore the old package instead of publishing it.
  // brokenUpdate 必须在暂存后失败并恢复旧包，不能发布损坏候选。
  const brokenUpdate = await rpcResponse('service_update_local_plugin', {
    sourcePath: brokenUpdateSource,
    contentFingerprint: brokenPreview.contentFingerprint,
    approvedPermissions: brokenPreview.permissions,
    allowRegistryDependencies: true,
    initialSecrets: [],
  });
  assert.equal(brokenUpdate.ok, false, brokenUpdate.error?.message);
  // afterBrokenUpdate verifies old package metadata, environment, and disabled state survived rollback.
  // afterBrokenUpdate 验证旧包元数据、环境与停用状态均在回滚后保留。
  const afterBrokenUpdate = await rpc('workbench.snapshot');
  assert.equal(afterBrokenUpdate.plugins.find(plugin => plugin.id === userPluginId)?.version, '0.1.0');
  assert.equal(afterBrokenUpdate.plugins.find(plugin => plugin.id === userPluginId)?.enabled, false);
  assert.equal(await rpc('service_plugin_environment_ready', { pluginId: userPluginId }), true);
  assert.equal(await realpath(userPrivateEnvironment), userPrivateEnvironment);
  // committedUpdate publishes the new generation only after its MCP environment validates.
  // committedUpdate 仅在新代次 MCP 环境验证通过后才发布更新。
  const committedUpdate = await rpc('service_update_local_plugin', {
    sourcePath: updateSource,
    contentFingerprint: updatePreview.contentFingerprint,
    approvedPermissions: updatePreview.permissions,
    allowRegistryDependencies: true,
    initialSecrets: [],
  });
  assert.equal(committedUpdate.plugins.find(plugin => plugin.id === userPluginId)?.version, '0.1.1');
  assert.equal(committedUpdate.plugins.find(plugin => plugin.id === userPluginId)?.enabled, false);
  assert.equal(await rpc('service_plugin_environment_ready', { pluginId: userPluginId }), true);
  // userPluginUpdateAcceptance records the three concrete update gates measured in one Core.
  // userPluginUpdateAcceptance 记录同一 Core 中测得的三个具体更新关卡。
  const userPluginUpdateAcceptance = {
    unapprovedRegistryRejected: true, brokenCandidateRolledBack: true,
    previousEnvironmentRetainedBeforeCommit: true, committedVersion: '0.1.1',
    remainedDisabled: true,
  };
  // Uninstall removes only the drained user plugin; the bundled browser runtime remains ready.
  // 卸载仅移除已排空的用户插件，内建浏览器运行环境仍保持就绪。
  const uninstalledUserCatalog = await rpc('service_uninstall_user_plugin', { pluginId: userPluginId });
  assert.equal(uninstalledUserCatalog.plugins.some(plugin => plugin.id === userPluginId), false);
  const afterUserUninstall = await rpc('workbench.snapshot');
  assert.equal(afterUserUninstall.plugins.some(plugin => plugin.id === userPluginId), false);
  assert.equal(await rpc('service_plugin_environment_ready', { pluginId: 'vulcan-browser' }), true);
  userPluginUpdateAcceptance.uninstalledAfterDrain = true;
  userPluginUpdateAcceptance.bundledEnvironmentUnaffected = true;
  // acceptanceResult makes a measured performance gap visible and causes the script to exit non-zero.
  // acceptanceResult 显式呈现实测性能缺口，并让脚本以非零状态退出。
  const acceptanceResult = streamPerformance.performanceTargetMet ? 'passed' : 'performance_gap';
  if (acceptanceResult === 'performance_gap') process.exitCode = 1;
  console.log(JSON.stringify({
    result: acceptanceResult,
    root,
    deviceScale: { requested: requestedDeviceScale ?? 'system', rendererPixelRatio: initialDevicePixelRatio },
    corePid: service.pid,
    serviceLifecycle: serviceStatus.lifecycle,
    conversationId: acceptedConversationId,
    pluginId: browserPlugin.id,
    notificationIdentity,
    debugToastAcceptance,
    g14Acceptance,
    workbenchSeparator: separatorStartWidth === null ? null : { beforeWidth: separatorStartWidth,
      afterWidth: separatorEndWidth, nativeWindowUnchanged: true },
    terminalSelectionAcceptance,
    pluginInstallAcceptance,
    userPluginInstallAcceptance,
    userPluginUpdateAcceptance,
    pluginDisableAcceptance,
    pluginMount: 'conversationRightPanel',
    browserFrame: true,
    pageClicks: clicks,
    pointerInteractionAcceptance,
    localBrowser: { tabs: localBrowser.snapshot.tabs.length, sandboxed: true,
      nativeBounds: localBrowser.native.bounds, modalOcclusion: true, zoomChecks,
      localPageIsolation, blockedWorkbenchPopup: true, blockedWorkbenchNavigation: true },
    guiIndependent: true,
    conversationSubmitAccepted: conversationSubmitResponse.ok,
    configuredModelRequestObserved: cloneModelConfig ? aiAcceptance !== null : null,
    selectedTestModel: modelOverrideFieldCount === 3
      ? { providerId: modelOverride.providerId, modelId: modelOverride.modelId } : null,
    aiAcceptance,
    providerToolTrace,
    pluginSettingsAcceptance,
    aiTurnRetirementAcceptance,
    aiCancellationAcceptance,
    aiTakeoverAcceptance,
    shellSecretRedactionAcceptance,
    sensitiveDataAudit,
    multiClientBrowserAcceptance,
    passiveWebAcceptance,
    crossOwnerProtectedMethodCount,
    crossOwnerDownloadAcceptance,
    webAcceptance,
    webNotificationAcceptance,
    streamPerformance,
  }));
} catch (error) {
  console.error(diagnostic.join(''));
  throw error;
} finally {
  if (webBrowser) await webBrowser.close().catch(() => {});
  if (gui) await gui.close().catch(() => {});
  if ((debugNotificationId || manualG14) && debugNotificationIdentity) {
    // Remove only this test's toast and the branded identity pointing to this generated executable.
    // 只清理本测试通知，以及指向本次生成程序的品牌化身份。
    const cleanupDebugToast = '$m=[Windows.UI.Notifications.ToastNotificationManager,Windows.UI.Notifications,ContentType=WindowsRuntime];'
      + 'if($env:VULCAN_W09_TOAST_ID){$m::History.Remove($env:VULCAN_W09_TOAST_ID,"Notifications",$env:VULCAN_W09_AUMID)};'
      + 'if($env:VULCAN_W09_G14_TOAST_ID){$m::History.Remove($env:VULCAN_W09_G14_TOAST_ID,"Notifications",$env:VULCAN_W09_AUMID)};'
      + '$p=Join-Path ([Environment]::GetFolderPath("Programs")) ($env:VULCAN_W09_APP_NAME + ".lnk");'
      + 'if(Test-Path -LiteralPath $p){$s=(New-Object -ComObject WScript.Shell).CreateShortcut($p);'
      + 'if($s.TargetPath -eq $env:VULCAN_W09_DEBUG_EXE){Remove-Item -LiteralPath $p}};'
      + '$k="Registry::HKEY_CURRENT_USER\\Software\\Classes\\CLSID\\" + $env:VULCAN_W09_CLSID;'
      + 'if(Test-Path -LiteralPath "$k\\LocalServer32"){$s=(Get-Item -LiteralPath "$k\\LocalServer32").GetValue("");'
      + 'if($s -eq $env:VULCAN_W09_DEBUG_EXE){Remove-Item -LiteralPath $k -Recurse}}';
    await execFileAsync('powershell.exe', ['-NoProfile', '-Command', cleanupDebugToast], {
      env: { ...process.env, VULCAN_W09_TOAST_ID: debugNotificationId ?? '',
        VULCAN_W09_G14_TOAST_ID: g14NotificationId ?? '',
        VULCAN_W09_DEBUG_EXE: selectedDesktopRuntime,
        VULCAN_W09_AUMID: debugNotificationIdentity.appUserModelId,
        VULCAN_W09_APP_NAME: debugNotificationIdentity.name,
        VULCAN_W09_CLSID: debugNotificationIdentity.activatorClsid,
      }, timeout: 10_000,
    });
  }
  if (normalSocket) normalSocket.close();
  if (slowSocket) slowSocket.destroy();
  if (stalledPageResponse && !stalledPageResponse.destroyed) {
    stalledPageResponse.writeHead(503).end('fixture cleanup');
  }
  service.kill();
  fixture.close();
  // Verify both generated roots, then detach junctions before recursive deletion.
  // 校验两个生成根目录，再解除联接后递归删除。
  const targetParent = await realpath(path.join(repository, 'target'));
  const tempParent = await realpath(rootParent);
  const realLayoutRoot = await realpath(layoutRoot);
  const realRoot = await realpath(root);
  assert.ok(realLayoutRoot.startsWith(targetParent + path.sep));
  assert.ok(realRoot.startsWith(tempParent + path.sep));
  await new Promise(resolve => {
    if (service.exitCode !== null) resolve();
    else { service.once('exit', resolve); setTimeout(resolve, 5_000); }
  });
  for (const relative of [
    'desktop/runtime', 'browser/chromium', 'webui',
    'compiled/node_modules/playwright-core', 'compiled/node_modules/ws',
  ]) await unlink(path.join(layoutRoot, relative));
  await rm(layoutRoot, { recursive: true, force: true, maxRetries: 10, retryDelay: 500 });
  await rm(root, { recursive: true, force: true, maxRetries: 20, retryDelay: 500 });
}
