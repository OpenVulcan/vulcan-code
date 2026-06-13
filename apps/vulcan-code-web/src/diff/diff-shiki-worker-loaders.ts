import type { LanguageInput } from "shiki/core";

// ShikiLanguageModule describes one dynamically imported Shiki language module.
// ShikiLanguageModule 描述单个动态导入的 Shiki 语言模块。
type ShikiLanguageModule = { default: LanguageInput };

// ShikiLanguageLoader loads one Shiki language registration only when needed.
// ShikiLanguageLoader 仅在需要时加载单个 Shiki 语言注册信息。
type ShikiLanguageLoader = () => Promise<ShikiLanguageModule>;

// DIFF_SHIKI_LANGUAGE_LOADERS is the worker-only language registration loader map.
// DIFF_SHIKI_LANGUAGE_LOADERS 是仅供 Worker 使用的语言注册加载器表。
const DIFF_SHIKI_LANGUAGE_LOADERS: Record<string, ShikiLanguageLoader> = {
  bash: () => import("shiki/langs/bash.mjs"),
  bat: () => import("shiki/langs/bat.mjs"),
  c: () => import("shiki/langs/c.mjs"),
  clojure: () => import("shiki/langs/clojure.mjs"),
  cmake: () => import("shiki/langs/cmake.mjs"),
  cpp: () => import("shiki/langs/cpp.mjs"),
  csharp: () => import("shiki/langs/csharp.mjs"),
  css: () => import("shiki/langs/css.mjs"),
  dart: () => import("shiki/langs/dart.mjs"),
  diff: () => import("shiki/langs/diff.mjs"),
  docker: () => import("shiki/langs/docker.mjs"),
  dotenv: () => import("shiki/langs/dotenv.mjs"),
  elixir: () => import("shiki/langs/elixir.mjs"),
  fsharp: () => import("shiki/langs/fsharp.mjs"),
  go: () => import("shiki/langs/go.mjs"),
  graphql: () => import("shiki/langs/graphql.mjs"),
  haskell: () => import("shiki/langs/haskell.mjs"),
  html: () => import("shiki/langs/html.mjs"),
  ini: () => import("shiki/langs/ini.mjs"),
  java: () => import("shiki/langs/java.mjs"),
  javascript: () => import("shiki/langs/javascript.mjs"),
  json: () => import("shiki/langs/json.mjs"),
  jsonc: () => import("shiki/langs/jsonc.mjs"),
  jsx: () => import("shiki/langs/jsx.mjs"),
  kotlin: () => import("shiki/langs/kotlin.mjs"),
  less: () => import("shiki/langs/less.mjs"),
  lua: () => import("shiki/langs/lua.mjs"),
  make: () => import("shiki/langs/make.mjs"),
  markdown: () => import("shiki/langs/markdown.mjs"),
  mdx: () => import("shiki/langs/mdx.mjs"),
  "objective-c": () => import("shiki/langs/objective-c.mjs"),
  perl: () => import("shiki/langs/perl.mjs"),
  php: () => import("shiki/langs/php.mjs"),
  powershell: () => import("shiki/langs/powershell.mjs"),
  python: () => import("shiki/langs/python.mjs"),
  ruby: () => import("shiki/langs/ruby.mjs"),
  rust: () => import("shiki/langs/rust.mjs"),
  sass: () => import("shiki/langs/sass.mjs"),
  scala: () => import("shiki/langs/scala.mjs"),
  scss: () => import("shiki/langs/scss.mjs"),
  sql: () => import("shiki/langs/sql.mjs"),
  svelte: () => import("shiki/langs/svelte.mjs"),
  swift: () => import("shiki/langs/swift.mjs"),
  toml: () => import("shiki/langs/toml.mjs"),
  tsx: () => import("shiki/langs/tsx.mjs"),
  typescript: () => import("shiki/langs/typescript.mjs"),
  vue: () => import("shiki/langs/vue.mjs"),
  xml: () => import("shiki/langs/xml.mjs"),
  yaml: () => import("shiki/langs/yaml.mjs"),
};

// loadShikiLanguageRegistration imports one allowlisted language registration.
// loadShikiLanguageRegistration 导入一个白名单内的语言注册信息。
export async function loadShikiLanguageRegistration(language: string): Promise<LanguageInput> {
  const loader = DIFF_SHIKI_LANGUAGE_LOADERS[language];
  if (!loader) {
    throw new Error(`Unsupported Shiki language: ${language}`);
  }
  const module = await loader();
  return module.default;
}
