// Reuse built and signed program files from a published release when their inputs did not change
// (plan section 6), so a release compiles only what changed and users download only what changed.
// 输入未变时复用已发布版本中已构建、已签名的程序文件（方案第六节），使发版只编译变化部分、用户只下载变化部分。
//
// A unit is one Cargo invocation of the shipped inventory (`rust-binary`, `rust-library`,
// `rust-isolated`) or, on Windows, the branded Electron executable (`desktop-executable`). After a
// unit is built, its record `fingerprints/<platform>/<unit>.json` names the version that ships it, the
// repository files rustc read for it (from Cargo's `.d` files) and the fingerprint over those files and
// everything else that shapes the bytes. A later release recomputes the fingerprint from the record's
// own file list; only an equal fingerprint reuses the files, and they are taken out of that version's
// signed manifest by `vulcan-release-tool restore`, never from the record. Everything else (the WebUI,
// Electron, runtimes, resources) is rebuilt and deduplicated by content in `pack --published`.
// 单元是分发清单中的一次 Cargo 调用（`rust-binary`、`rust-library`、`rust-isolated`），在 Windows 上另有带品牌的
// Electron 可执行文件（`desktop-executable`）。单元构建后，其记录 `fingerprints/<平台>/<单元>.json` 指明分发它的
// 版本、rustc 为其读取的仓库文件（取自 Cargo 的 `.d` 文件），以及覆盖这些文件和其他一切影响字节之因素的指纹。
// 后续发版按记录自带的文件清单重新计算指纹；只有指纹相等才复用，文件由 `vulcan-release-tool restore` 从该版本的
// 签名版本清单中取出，从不取自记录。其他内容（WebUI、Electron、运行时、资源）照常构建，并在
// `pack --published` 中按内容去重。
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { zstdDecompressSync } from 'node:zlib';
import { shippedCargoArgs, shippedFileName, shippedKindOf, shippedRustPackagesFor } from './shipped-rust-packages.mjs';

// RECIPE changes whenever the fingerprint below changes meaning, so every older record stops matching.
// RECIPE 在下方指纹含义变化时递增，使所有旧记录不再匹配。
export const RECIPE = 1;
// RECORD_SCHEMA is the format version of a reuse record.
// RECORD_SCHEMA 是复用记录的格式版本。
export const RECORD_SCHEMA = 1;
// RUST_KINDS lists the Cargo invocations of the shipped inventory, in build order.
// RUST_KINDS 按构建顺序列出分发清单中的 Cargo 调用。
const RUST_KINDS = ['binary', 'library', 'isolated'];
// ROOT_FILES shape every unit: workspace settings, toolchain, line endings and pinned build inputs.
// ROOT_FILES 影响每个单元：工作区设置、工具链、换行规则与锁定的构建输入。
const ROOT_FILES = [
  'Cargo.toml', '.cargo/config.toml', 'rust-toolchain.toml', '.gitattributes',
  'scripts/build-inputs.lock.json', 'scripts/shipped-rust-packages.json',
];
// FINGERPRINT_ENV matches the environment variables Cargo, rustc or C build scripts read for code generation.
// FINGERPRINT_ENV 匹配 Cargo、rustc 或 C 构建脚本在生成代码时读取的环境变量。
const FINGERPRINT_ENV = /^(RUSTFLAGS|CARGO_ENCODED_RUSTFLAGS|CARGO_INCREMENTAL|CARGO_PROFILE_.+|CARGO_TARGET_.+|CARGO_BUILD_.+|MACOSX_DEPLOYMENT_TARGET|CC|CXX|CFLAGS|CXXFLAGS|LDFLAGS|CL)$/;
const repository = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const NODE_PLATFORMS = { windows: 'win32', linux: 'linux', macos: 'darwin' };

// nodePlatformOf maps a release platform such as `macos-arm64` to its Node platform.
// nodePlatformOf 把 `macos-arm64` 这样的发布平台映射为 Node 平台。
export function nodePlatformOf(platform) {
  const nodePlatform = NODE_PLATFORMS[platform.split('-')[0]];
  if (!nodePlatform) throw new Error(`unknown platform ${platform}`);
  return nodePlatform;
}

// rustUnits returns the Cargo units shipped on a release platform with their files below `bin/`.
// rustUnits 返回某发布平台上分发的 Cargo 单元及其 `bin/` 下的文件。
export function rustUnits(platform) {
  const nodePlatform = nodePlatformOf(platform);
  const items = shippedRustPackagesFor(nodePlatform);
  return RUST_KINDS.flatMap(kind => {
    const members = items.filter(item => shippedKindOf(item) === kind);
    if (members.length === 0) return [];
    const files = members.map(item => shippedFileName(item, nodePlatform));
    return [{
      name: `rust-${kind}`,
      kind,
      packages: [...new Set(members.map(item => item.package))],
      cargo: [...shippedCargoArgs(kind, nodePlatform), '--locked', '--release'],
      files,
      // Cargo writes each artifact's dependency file beside it without the extension.
      // Cargo 在每个制品旁写出去掉扩展名的依赖文件。
      depFiles: files.map(file => `${file.replace(/\.(exe|dll|so|dylib)$/, '')}.d`),
    }];
  });
}

// parseDepFile returns the repository-relative inputs of one Cargo `.d` file; generated files below
// `target/` and files outside the repository (the toolchain, registry sources) are covered elsewhere.
// parseDepFile 返回一个 Cargo `.d` 文件中相对仓库的输入；`target/` 下的生成文件与仓库外的文件（工具链、
// 注册表源码）由其他部分覆盖。
export function parseDepFile(text, root = repository) {
  const line = text.split(/\r?\n/, 1)[0];
  const separator = line.indexOf(': ');
  if (separator < 0) throw new Error('the dependency file has no `target: inputs` line');
  const inputs = new Set();
  for (const escaped of line.slice(separator + 2).split(/(?<!\\) /)) {
    if (!escaped) continue;
    const absolute = escaped.replace(/\\ /g, ' ').replace(/^\\\\\?\\/, '');
    const relative = path.relative(root, absolute);
    if (!relative || relative.startsWith('..') || path.isAbsolute(relative)) continue;
    const portable = relative.split(path.sep).join('/');
    if (portable === 'target' || portable.startsWith('target/')) continue;
    inputs.add(portable);
  }
  return [...inputs].sort();
}

// canonical serializes a value with sorted object keys, so equal content always hashes equally.
// canonical 以排序后的对象键序列化值，使相同内容的哈希总是相同。
export function canonical(value) {
  if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`;
  if (value && typeof value === 'object') {
    return `{${Object.keys(value).sort().map(key => `${JSON.stringify(key)}:${canonical(value[key])}`).join(',')}}`;
  }
  return JSON.stringify(value);
}

const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');

// lockClosure lists every package a unit compiles, normal and build dependencies alike: registry and git
// packages by their immutable id, path packages by name, version and the hash of their manifest.
// lockClosure 列出单元编译的全部包（普通依赖与构建依赖）：注册表与 git 包取其不可变 ID，路径包取名称、版本及
// 清单哈希。
export function lockClosure(metadata, packageNames, readManifest) {
  const packages = new Map(metadata.packages.map(item => [item.id, item]));
  const nodes = new Map(metadata.resolve.nodes.map(node => [node.id, node]));
  const pending = packageNames.map(name => {
    const id = metadata.workspace_members.find(member => packages.get(member)?.name === name);
    if (!id) throw new Error(`${name} is not a workspace member`);
    return id;
  });
  const seen = new Set();
  while (pending.length > 0) {
    const id = pending.pop();
    if (seen.has(id)) continue;
    seen.add(id);
    for (const dependency of nodes.get(id)?.deps ?? []) {
      if (dependency.dep_kinds.some(kind => kind.kind !== 'dev')) pending.push(dependency.pkg);
    }
  }
  return [...seen].map(id => {
    const item = packages.get(id);
    if (item.source) return id;
    return `${item.name}@${item.version} ${sha256(readManifest(item.manifest_path))}`;
  }).sort();
}

// Tree holds the tracked files of the checkout; untracked build output never counts as a listing entry.
// Tree 保存检出中的受跟踪文件；未跟踪的构建产物从不计入目录清单。
export class Tree {
  constructor(root, tracked) {
    this.root = root;
    this.tracked = tracked;
    this.children = new Map();
    for (const file of tracked) {
      const parts = file.split('/');
      for (let depth = 0; depth < parts.length; depth += 1) {
        const directory = parts.slice(0, depth).join('/');
        if (!this.children.has(directory)) this.children.set(directory, new Set());
        this.children.get(directory).add(parts[depth]);
      }
    }
  }

  static of(root = repository) {
    const tracked = execFileSync('git', ['ls-files', '-z'], { cwd: root, encoding: 'utf8', maxBuffer: 1 << 28 })
      .split('\0').filter(Boolean);
    return new Tree(root, tracked);
  }

  // hash names the content of one input: a file's bytes, a directory's tracked files, or its absence.
  // hash 表示一个输入的内容：文件字节、目录内的受跟踪文件，或其不存在。
  hash(input) {
    const absolute = path.join(this.root, input);
    if (!existsSync(absolute)) return 'missing';
    if (!statSync(absolute).isDirectory()) return sha256(readFileSync(absolute));
    const files = this.tracked.filter(file => file.startsWith(`${input}/`)).sort();
    return sha256(canonical(files.map(file => [file, this.hash(file)])));
  }

  // listing names the tracked entries directly below a directory, so an added or removed source counts.
  // listing 列出目录下一层的受跟踪条目，使新增或删除的源文件也计入。
  listing(directory) {
    return [...(this.children.get(directory) ?? [])].sort();
  }
}

// buildContext gathers everything that shapes a unit's bytes besides the sources themselves.
// fingerprintInputs 收集除源文件本身外影响单元字节的一切因素。
export function buildContext(root = repository) {
  const rustc = execFileSync('rustc', ['-vV'], { cwd: root, encoding: 'utf8' });
  const host = /^host: (.+)$/m.exec(rustc)?.[1];
  if (!host) throw new Error('rustc -vV did not name the host');
  const metadata = JSON.parse(execFileSync('cargo', ['metadata', '--locked', '--format-version', '1', '--filter-platform', host], {
    cwd: root, encoding: 'utf8', maxBuffer: 1 << 30,
  }));
  const env = Object.fromEntries(Object.entries(process.env).filter(([name]) => FINGERPRINT_ENV.test(name)));
  return { rustc, metadata, env, tree: Tree.of(root) };
}

// fingerprint computes a unit's fingerprint for one input list.
// fingerprint 按给定输入清单计算单元指纹。
export function fingerprint(unit, platform, inputs, context) {
  const { tree } = context;
  const directories = [...new Set(inputs.map(input => path.posix.dirname(input)))].sort();
  return sha256(canonical({
    recipe: RECIPE,
    platform,
    unit: unit.name,
    cargo: unit.cargo,
    rustc: context.rustc,
    env: context.env,
    root: Object.fromEntries(ROOT_FILES.map(file => [file, tree.hash(file)])),
    lock: lockClosure(context.metadata, unit.packages, file => readFileSync(file)),
    inputs: inputs.map(input => [input, tree.hash(input)]),
    listings: directories.map(directory => [directory, tree.listing(directory === '.' ? '' : directory)]),
  }));
}

// recordKey is the object key of a unit's reuse record.
// recordKey 是单元复用记录的对象键。
export const recordKey = (platform, unit) => `fingerprints/${platform}/${unit}.json`;

// fetchObject reads one object below the origin; a missing object is null, any other failure throws.
// fetchObject 读取源站下的一个对象；对象不存在时为 null，其他失败一律抛出。
async function fetchObject(origin, key) {
  const url = `${origin.replace(/\/$/, '')}/${key}`;
  for (let attempt = 1; ; attempt += 1) {
    try {
      const response = await fetch(url, { cache: 'no-store' });
      if (response.status === 404) return null;
      if (!response.ok) throw new Error(`${url} answered ${response.status}`);
      return Buffer.from(await response.arrayBuffer());
    } catch (error) {
      if (attempt >= 3) throw error;
      await new Promise(resolve => setTimeout(resolve, 2000 * attempt));
    }
  }
}

// mirrorObject keeps one object in a local mirror in the object layout and returns its bytes.
// mirrorObject 以对象布局把一个对象保存到本地镜像并返回其字节。
async function mirrorObject(origin, mirror, key, { optional = false } = {}) {
  const target = path.join(mirror, key);
  if (existsSync(target)) return readFileSync(target);
  const bytes = await fetchObject(origin, key);
  if (!bytes) {
    if (optional) return null;
    throw new Error(`${key} is not published`);
  }
  mkdirSync(path.dirname(target), { recursive: true });
  writeFileSync(target, bytes);
  return bytes;
}

// readIndex decodes a component index object; its digest is checked again by the release tool.
// readIndex 解码组件索引对象；其摘要由发布工具再次校验。
export function readIndex(bytes) {
  return JSON.parse(zstdDecompressSync(bytes).toString('utf8'));
}

// mirrorRelease copies a version's manifest, its signature, every index and the packs holding `paths`.
// mirrorRelease 复制某版本的版本清单、签名、全部索引，以及存放 `paths` 的文件包。
async function mirrorRelease(origin, mirror, platform, version, paths) {
  const manifestKey = `releases/${version}/${platform}/manifest.json`;
  const manifest = JSON.parse((await mirrorObject(origin, mirror, manifestKey)).toString('utf8'));
  await mirrorObject(origin, mirror, `${manifestKey}.sig`);
  const wanted = new Set(paths);
  const packs = new Set();
  for (const reference of Object.values(manifest.components)) {
    const index = readIndex(await mirrorObject(origin, mirror, `indexes/${reference.index}.json.zst`));
    for (const entry of index.entries) {
      if (entry.type === 'file' && wanted.has(entry.path)) packs.add(entry.pack);
    }
  }
  for (const pack of packs) await mirrorObject(origin, mirror, `packs/${pack}.pack`);
}

// restore takes `paths` out of a published version into `out` through the release tool's signed checks.
// restore 经发布工具的签名校验，把 `paths` 从已发布版本取出到 `out`。
export async function restore({ origin, tool, mirror, platform, version, paths, out }) {
  await mirrorRelease(origin, mirror, platform, version, paths);
  execFileSync(tool, ['restore', '--mirror', mirror, '--platform', platform, '--version', version,
    ...paths.flatMap(file => ['--path', file]), '--out', out], { stdio: ['ignore', 'inherit', 'inherit'] });
}

// matchingRecord fetches a unit's record and returns it only when `current` reproduces its fingerprint.
// matchingRecord 读取单元记录，只在 `current` 重现其指纹时返回该记录。
async function matchingRecord(origin, platform, unit, current) {
  const bytes = await fetchObject(origin, recordKey(platform, unit));
  if (!bytes) {
    log(`${unit}: no record, building`);
    return null;
  }
  const record = JSON.parse(bytes.toString('utf8'));
  if (record.schema !== RECORD_SCHEMA || record.platform !== platform || record.unit !== unit) {
    log(`${unit}: the record has another format, building`);
    return null;
  }
  if (current(record) !== record.fingerprint) {
    log(`${unit}: inputs changed since ${record.version}, building`);
    return null;
  }
  log(`${unit}: unchanged since ${record.version}, reusing`);
  return record;
}

const log = message => process.stderr.write(`${message}\n`);

// readReport reads the reuse report of this build; a missing report reuses nothing.
// readReport 读取本次构建的复用报告；报告不存在时表示没有复用。
export function readReport(file) {
  if (!existsSync(file)) return { schema: RECORD_SCHEMA, units: {}, files: [] };
  return JSON.parse(readFileSync(file, 'utf8'));
}

function writeJson(file, value) {
  mkdirSync(path.dirname(file), { recursive: true });
  writeFileSync(file, `${JSON.stringify(value, null, 2)}\n`);
}

// lookup restores every unchanged Cargo unit into the Cargo profile directory, writes the report and
// prints the `VULCAN_PREBUILT_KINDS` line the build scripts read.
// lookup 把每个未变的 Cargo 单元还原到 Cargo 配置目录，写出报告，并输出构建脚本读取的 `VULCAN_PREBUILT_KINDS` 行。
async function lookup({ platform, origin, tool, work, target, report }) {
  const context = buildContext();
  const result = { schema: RECORD_SCHEMA, units: {}, files: [] };
  const kinds = [];
  for (const unit of rustUnits(platform)) {
    const record = await matchingRecord(origin, platform, unit.name,
      found => fingerprint(unit, platform, found.inputs, context));
    if (!record) continue;
    const staged = path.join(work, 'restored', unit.name);
    const paths = unit.files.map(file => `bin/${file}`);
    await restore({ origin, tool, mirror: path.join(work, 'mirror'), platform, version: record.version, paths, out: staged });
    mkdirSync(target, { recursive: true });
    for (const file of unit.files) copyFileSync(path.join(staged, 'bin', file), path.join(target, file));
    result.units[unit.name] = record.version;
    result.files.push(...paths);
    kinds.push(unit.kind);
  }
  writeJson(report, result);
  process.stdout.write(`VULCAN_PREBUILT_KINDS=${kinds.join(' ')}\n`);
}

// desktop reuses the signed Electron executable when its unsigned bytes equal the recorded ones, and
// otherwise writes its record; rcedit makes those bytes depend only on Electron, the icon and the names.
// desktop 在未签名字节与记录相同时复用已签名的 Electron 可执行文件，否则写出其记录；rcedit 使这些字节只取决于
// Electron、图标与名称。
async function desktop({ platform, origin, tool, work, tree, version, report, records }) {
  const layout = JSON.parse(readFileSync(path.join(tree, 'runtime-layout.json'), 'utf8'));
  const file = layout.desktopExecutable;
  const unsigned = sha256(readFileSync(path.join(tree, file)));
  const result = readReport(report);
  const unit = 'desktop-executable';
  const record = origin ? await matchingRecord(origin, platform, unit, () => unsigned) : null;
  if (record) {
    await restore({ origin, tool, mirror: path.join(work, 'mirror'), platform, version: record.version, paths: [file], out: tree });
    result.units[unit] = record.version;
    result.files.push(file);
  } else {
    writeJson(path.join(records, recordKey(platform, unit)),
      { schema: RECORD_SCHEMA, platform, unit, version, fingerprint: unsigned, inputs: [] });
  }
  writeJson(report, result);
}

// record writes the record of every Cargo unit this build compiled, from the `.d` files Cargo wrote.
// record 根据 Cargo 写出的 `.d` 文件，为本次构建编译的每个 Cargo 单元写出记录。
function record({ platform, version, target, report, records }) {
  const context = buildContext();
  const reused = readReport(report).units;
  for (const unit of rustUnits(platform)) {
    if (reused[unit.name]) continue;
    const inputs = [...new Set(unit.depFiles.flatMap(file => parseDepFile(readFileSync(path.join(target, file), 'utf8'))))].sort();
    writeJson(path.join(records, recordKey(platform, unit.name)), {
      schema: RECORD_SCHEMA, platform, unit: unit.name, version,
      fingerprint: fingerprint(unit, platform, inputs, context), inputs,
    });
    log(`${unit.name}: recorded ${inputs.length} inputs`);
  }
}

// published copies the indexes of the current preview release, which `pack --published` deduplicates
// against; before any preview release the directory stays empty.
// published 复制当前 preview 版本的索引，供 `pack --published` 去重；尚无 preview 版本时目录为空。
export async function published({ platform, origin, out }) {
  mkdirSync(out, { recursive: true });
  const pointer = await fetchObject(origin, `channels/preview/${platform}.json`);
  if (!pointer) {
    log(`no preview release for ${platform} yet`);
    return;
  }
  const { version } = JSON.parse(pointer.toString('utf8'));
  const manifest = await fetchObject(origin, `releases/${version}/${platform}/manifest.json`);
  if (!manifest) throw new Error(`the preview pointer names ${version}, which is not published`);
  for (const reference of Object.values(JSON.parse(manifest.toString('utf8')).components)) {
    const bytes = await fetchObject(origin, `indexes/${reference.index}.json.zst`);
    if (!bytes) throw new Error(`index ${reference.index} of ${version} is not published`);
    writeFileSync(path.join(out, `${reference.index}.json.zst`), bytes);
  }
  log(`deduplicating against ${version}`);
}

// packs downloads every pack the indexes below `objects` reference but this release did not write.
// packs 下载 `objects` 下索引引用、但本次发版未写出的每个文件包。
export async function packs({ objects, origin }) {
  const referenced = new Set();
  for (const name of readdirSync(path.join(objects, 'indexes'))) {
    for (const entry of readIndex(readFileSync(path.join(objects, 'indexes', name))).entries) {
      if (entry.type === 'file') referenced.add(entry.pack);
    }
  }
  let fetched = 0;
  for (const pack of referenced) {
    if (existsSync(path.join(objects, 'packs', `${pack}.pack`))) continue;
    await mirrorObject(origin, objects, `packs/${pack}.pack`);
    fetched += 1;
  }
  log(`fetched ${fetched} published packs`);
}

// options parses `--name value` pairs, requiring exactly the names given.
// options 解析 `--name value` 参数对，只接受并要求给定的名称。
function options(argv, required, optional = []) {
  const values = {};
  for (let index = 0; index < argv.length; index += 2) {
    const name = argv[index]?.replace(/^--/, '');
    if (!argv[index]?.startsWith('--') || ![...required, ...optional].includes(name) || argv[index + 1] === undefined) {
      throw new Error(`unexpected argument ${argv[index]}`);
    }
    values[name] = argv[index + 1];
  }
  for (const name of required) if (values[name] === undefined) throw new Error(`--${name} is required`);
  return values;
}

const COMMANDS = {
  lookup: [['platform', 'origin', 'tool', 'work', 'target', 'report'], [], lookup],
  desktop: [['platform', 'tool', 'work', 'tree', 'version', 'report', 'records'], ['origin'], desktop],
  record: [['platform', 'version', 'target', 'report', 'records'], [], record],
  published: [['platform', 'origin', 'out'], [], published],
  packs: [['objects', 'origin'], [], packs],
};

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [command, ...rest] = process.argv.slice(2);
  const entry = COMMANDS[command];
  if (!entry) {
    log(`usage: release-reuse.mjs ${Object.keys(COMMANDS).join('|')} --name value...`);
    process.exit(2);
  }
  const [required, optional, run] = entry;
  await run(options(rest, required, optional));
}
