// Preloaded (`node --require`) into Playwright build processes on macOS x64. Playwright chooses arm64 browsers
// whenever a CPU model names Apple, even for an x64 process under Rosetta, so an x64 release would ship an arm64
// Chromium. The CPU models are reported without that name; Playwright's own platform logic then picks the x64
// build. On an Intel Mac no model names Apple and nothing changes.
// 以 `node --require` 预加载到 macOS x64 上的 Playwright 构建进程。只要 CPU 型号含 Apple，Playwright 就选择 arm64
// 浏览器，即使是 Rosetta 下的 x64 进程，导致 x64 发行物带上 arm64 Chromium。这里报告的 CPU 型号去掉该名称，
// 由 Playwright 自身的平台逻辑选择 x64 版本。Intel Mac 上没有含 Apple 的型号，不产生任何变化。
const os = require('node:os');

const cpus = os.cpus;
os.cpus = () => cpus().map(cpu => ({ ...cpu, model: cpu.model.replaceAll('Apple', 'Rosetta') }));
