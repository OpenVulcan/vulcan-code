// Copies the shared language registry and the desktop catalogs beside the compiled GUI as `i18n/`.
// 将共享语言注册表与桌面目录作为 `i18n/` 复制到已编译 GUI 旁边。
//
// `crates/vulcan-i18n/catalogs` stays the only authority; `dist/i18n` and the packaged `app/i18n`
// are build outputs that `src/gui/desktop-text.ts` reads at startup.
// `crates/vulcan-i18n/catalogs` 仍是唯一权威；`dist/i18n` 与打包后的 `app/i18n` 是构建产物，
// 由 `src/gui/desktop-text.ts` 在启动时读取。
import { cp, mkdir, rm } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const source = fileURLToPath(new URL('../../../crates/vulcan-i18n/catalogs/', import.meta.url));

// Replaces `target` with the registry and the `desktop` surface.
// 用注册表与 `desktop` 分面替换 `target`。
export async function copyDesktopCatalogs(target) {
  await rm(target, { recursive: true, force: true });
  await mkdir(target, { recursive: true });
  await cp(path.join(source, 'locales.json'), path.join(target, 'locales.json'));
  await cp(path.join(source, 'desktop'), path.join(target, 'desktop'), { recursive: true });
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await copyDesktopCatalogs(fileURLToPath(new URL('../dist/i18n/', import.meta.url)));
}
