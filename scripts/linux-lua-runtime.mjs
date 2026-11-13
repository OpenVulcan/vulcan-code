// Verify the dynamic Lua API and glibc requirements of the final Linux release files.
// 校验 Linux 发行文件的动态 Lua API 与 glibc 版本要求。
import { execFileSync } from 'node:child_process';
import { readdirSync } from 'node:fs';
import path from 'node:path';

// readElf runs the requested readelf query for an ELF path and returns the full report.
// readElf 对指定 ELF 路径执行所需的 readelf 查询并返回完整报告。
function readElf(option, file) {
  return execFileSync('readelf', ['--wide', option, file], {
    encoding: 'utf8',
    env: { ...process.env, LC_ALL: 'C' },
    maxBuffer: 16 * 1024 * 1024,
  });
}

// luaSymbols extracts the undefined imports or defined exports from an ELF dynamic symbol table.
// luaSymbols 从 ELF 动态符号表提取未定义导入或已定义导出。
// file is the ELF path and imported selects undefined symbols. Returns the unique Lua names.
// file 是 ELF 路径，imported 选择未定义符号。返回去重后的 Lua 符号名。
function luaSymbols(file, imported) {
  // The dynamic table alone determines what dlopen can resolve from Core.
  // 只有动态符号表决定 dlopen 可以从 Core 解析哪些符号。
  const rows = readElf('--dyn-syms', file);
  return new Set([...rows.matchAll(/^\s*\d+:\s+\S+\s+\S+\s+\S+\s+(?:GLOBAL|WEAK)\s+DEFAULT\s+(\S+)\s+(lua(?:L|JIT|open)?_\w+)(?=\s|@|$)/gm)]
    .filter((row) => (row[1] === 'UND') === imported)
    .map((row) => row[2]));
}

// verifyLinuxLuaRuntime checks the release Core and every shipped Lua shared object.
// verifyLinuxLuaRuntime 校验发行版 Core 和每个随包 Lua 共享对象。
// runtime is lua_runtime, core is the executable and ceiling is the supported glibc version. Throws on incompatibility.
// runtime 是 lua_runtime 目录，core 是可执行文件，ceiling 是支持的 glibc 版本。发现不兼容时抛出错误。
export function verifyLinuxLuaRuntime(runtime, core, ceiling) {
  if (!/^\d+\.\d+$/.test(ceiling)) throw new Error(`Invalid Linux release glibc baseline: ${ceiling}`);
  // Limit and paths are taken from the release job and the packaged layout.
  // 版本上限和路径取自发布作业与随包目录布局。
  const [major, minor] = ceiling.split('.').map(Number);
  const modules = path.join(runtime, 'lua_packages', 'lib', 'lua');
  const libs = path.join(runtime, 'libs');
  // Include versioned shared objects and nested Lua modules.
  // 包含带版本号的共享对象和嵌套的 Lua 模块。
  const moduleFiles = readdirSync(modules, { recursive: true, withFileTypes: true })
    .filter((file) => file.isFile() && file.name.endsWith('.so'))
    .map((file) => path.join(file.parentPath, file.name));
  if (!moduleFiles.includes(path.join(modules, '_openssl.so'))) {
    throw new Error(`Missing bundled Linux Lua OpenSSL module in ${modules}`);
  }
  // Only read real library files. Version aliases point at those files in the same directory.
  // 只读取实际库文件。版本别名指向同一目录中的实际文件。
  const libraryFiles = readdirSync(libs, { withFileTypes: true })
    .filter((file) => file.isFile() && /\.so(?:\.\d+)*$/.test(file.name))
    .map((file) => path.join(libs, file.name));
  const errors = [];
  for (const file of [...moduleFiles, ...libraryFiles, core]) {
    // Inspect ELF version requirements only, without interpreting debug strings or exported version definitions.
    // 只检查 ELF 的版本需求，不把调试字符串或导出版本定义当作依赖。
    const needs = readElf('--version-info', file).split('Version needs section')[1] ?? '';
    for (const version of needs.matchAll(/Name: GLIBC_(\d+)\.(\d+)\b/g)) {
      if (Number(version[1]) > major || (Number(version[1]) === major && Number(version[2]) > minor)) {
        errors.push(`${path.relative(runtime, file)} requires glibc ${version[1]}.${version[2]} (baseline ${ceiling})`);
      }
    }
    if (file === core) continue;
    // Every Lua shared object uses a loader-relative path to the same packaged library directory.
    // 每个 Lua 共享对象都以自身为基准指向同一个随包库目录。
    const relativeLibs = path.relative(path.dirname(file), libs);
    const expected = relativeLibs ? `$ORIGIN/${relativeLibs}` : '$ORIGIN';
    const dynamic = readElf('--dynamic', file);
    const runpath = [...dynamic.matchAll(/\((?:RPATH|RUNPATH)\).*\[([^\]]*)\]/g)];
    if (runpath.length !== 1 || runpath[0][1] !== expected) {
      errors.push(`${path.relative(runtime, file)} must use library search path ${expected}`);
    }
  }
  // List the specific module and symbol names to diagnose linker or package changes.
  // 列出具体模块和符号名称，便于诊断链接器或依赖包变更。
  const exported = luaSymbols(core, false);
  for (const file of moduleFiles) {
    const missing = [...luaSymbols(file, true)].filter((symbol) => !exported.has(symbol)).sort();
    if (missing.length) errors.push(`${path.relative(runtime, file)} cannot resolve Lua symbols from Core: ${missing.join(', ')}`);
  }
  if (errors.length) throw new Error(`Linux Lua runtime verification failed:\n${errors.join('\n')}`);
  console.log(`Verified ${moduleFiles.length} Linux Lua modules and ${libraryFiles.length} libraries against glibc ${ceiling}.`);
}
