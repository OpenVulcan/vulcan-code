/// <reference types="vite/client" />

// Declares imported SVG files as build-resolved asset URLs.
// 将导入的 SVG 文件声明为构建阶段解析的资源 URL。
declare module "*.svg" {
  const assetUrl: string
  export default assetUrl
}
