//! Location of the bundled ONNX Runtime the local judge worker loads.
//! 本机判定 worker 加载的随包 ONNX Runtime 的定位。
//!
//! `scripts/install-locked-tools.mjs judge-runtime` installs the core library and the WebGPU plugin
//! from the build-input lock and writes `judge-runtime.json`; synchronization copies that directory
//! to `<output>/judge-runtime`. The manifest names both libraries, so no file name is guessed here.
//! `scripts/install-locked-tools.mjs judge-runtime` 按构建输入锁文件安装核心库与 WebGPU 插件并写入
//! `judge-runtime.json`；同步流程将该目录复制到 `<output>/judge-runtime`。清单给出两个库，此处不猜测文件名。

use std::collections::BTreeMap;
use std::io;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

/// Manifest written by the build installer; keep equal to `JUDGE_RUNTIME_MANIFEST` in that script.
/// 构建安装器写入的清单；须与该脚本中的 `JUDGE_RUNTIME_MANIFEST` 保持一致。
pub const RUNTIME_MANIFEST_FILE: &str = "judge-runtime.json";
/// The only runtime manifest format this build reads.
/// 本构建唯一读取的执行库清单格式。
pub const RUNTIME_MANIFEST_SCHEMA_VERSION: u32 = 1;

/// Manifest as written by the installer.
/// 安装器写入的清单。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeManifest {
    /// Manifest format.
    /// 清单格式。
    schema_version: u32,
    /// ONNX Runtime release of the core library.
    /// 核心库的 ONNX Runtime 发行版本。
    onnxruntime: String,
    /// Release of the WebGPU execution provider plugin.
    /// WebGPU 执行提供器插件的发行版本。
    webgpu_plugin: String,
    /// Core library relative to the runtime root.
    /// 相对执行库根目录的核心库。
    core_library: String,
    /// WebGPU plugin library relative to the runtime root.
    /// 相对执行库根目录的 WebGPU 插件库。
    webgpu_library: String,
    /// Digest of every installed file, keyed by relative path.
    /// 每个已安装文件的摘要，以相对路径为键。
    files: BTreeMap<String, String>,
}

/// Bundled runtime whose libraries exist on disk.
/// 库文件均已存在于磁盘上的随包执行库。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundledRuntime {
    /// ONNX Runtime release of the core library.
    /// 核心库的 ONNX Runtime 发行版本。
    pub onnxruntime_version: String,
    /// Release of the WebGPU execution provider plugin.
    /// WebGPU 执行提供器插件的发行版本。
    pub webgpu_plugin_version: String,
    /// Absolute path of the core library passed to the worker.
    /// 传给 worker 的核心库绝对路径。
    pub core_library: PathBuf,
    /// Absolute path of the WebGPU plugin passed to the worker for the WebGPU device.
    /// WebGPU 设备时传给 worker 的 WebGPU 插件绝对路径。
    pub webgpu_library: PathBuf,
}

/// Why the bundled runtime cannot be used.
/// 随包执行库无法使用的原因。
#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    /// The manifest could not be read.
    /// 清单无法读取。
    #[error("Judge runtime manifest `{path}` could not be read: {source}.")]
    Unreadable {
        /// Manifest path.
        /// 清单路径。
        path: PathBuf,
        /// I/O failure.
        /// I/O 失败。
        source: io::Error,
    },
    /// The manifest is not valid JSON of the expected shape.
    /// 清单不是预期形状的合法 JSON。
    #[error("Judge runtime manifest `{path}` is malformed: {source}.")]
    Malformed {
        /// Manifest path.
        /// 清单路径。
        path: PathBuf,
        /// Parser failure.
        /// 解析失败。
        source: serde_json::Error,
    },
    /// The manifest format is not the one this build reads.
    /// 清单格式不是本构建读取的格式。
    #[error(
        "Judge runtime manifest schema {0} is not supported (supported: {RUNTIME_MANIFEST_SCHEMA_VERSION})."
    )]
    UnsupportedSchema(u32),
    /// A library path escapes the runtime root or is not among the installed files.
    /// 库路径越出执行库根目录或不在已安装文件之列。
    #[error("Judge runtime library `{0}` is not an installed file of the runtime.")]
    UndeclaredLibrary(String),
    /// A declared library is absent.
    /// 声明的库不存在。
    #[error("Judge runtime library `{0}` is missing; run make sync or reinstall the application.")]
    MissingLibrary(PathBuf),
}

impl BundledRuntime {
    /// Reads the manifest below `root` and checks that both libraries exist.
    /// 读取 `root` 下的清单并检查两个库均存在。
    ///
    /// # Arguments
    /// 参数
    /// * `root` - Absolute runtime directory such as `<output>/judge-runtime`.
    /// * `root` - 绝对执行库目录，例如 `<output>/judge-runtime`。
    pub fn load(root: &Path) -> Result<Self, RuntimeError> {
        let path = root.join(RUNTIME_MANIFEST_FILE);
        let bytes = std::fs::read(&path).map_err(|source| RuntimeError::Unreadable {
            path: path.clone(),
            source,
        })?;
        let manifest: RuntimeManifest = serde_json::from_slice(&bytes)
            .map_err(|source| RuntimeError::Malformed { path, source })?;
        if manifest.schema_version != RUNTIME_MANIFEST_SCHEMA_VERSION {
            return Err(RuntimeError::UnsupportedSchema(manifest.schema_version));
        }
        let library = |relative: &str| -> Result<PathBuf, RuntimeError> {
            let contained = !relative.contains('\\')
                && Path::new(relative)
                    .components()
                    .all(|part| matches!(part, Component::Normal(_)));
            if !contained || !manifest.files.contains_key(relative) {
                return Err(RuntimeError::UndeclaredLibrary(relative.to_string()));
            }
            let path = root.join(relative);
            if !path.is_file() {
                return Err(RuntimeError::MissingLibrary(path));
            }
            Ok(path)
        };
        Ok(Self {
            core_library: library(&manifest.core_library)?,
            webgpu_library: library(&manifest.webgpu_library)?,
            onnxruntime_version: manifest.onnxruntime,
            webgpu_plugin_version: manifest.webgpu_plugin,
        })
    }

    /// Directories the worker needs on its library search path, so dependencies placed beside each
    /// library (such as the WebGPU shader compiler) resolve without system-wide installation.
    /// worker 库搜索路径所需的目录，使放在各库旁边的依赖（如 WebGPU 着色器编译器）无需系统级安装即可解析。
    pub fn library_directories(&self) -> Vec<PathBuf> {
        let mut directories: Vec<PathBuf> = [&self.core_library, &self.webgpu_library]
            .into_iter()
            .filter_map(|library| library.parent().map(Path::to_path_buf))
            .collect();
        directories.dedup();
        directories
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes a manifest and the named files below a fresh root.
    /// 在新根目录下写入清单与指定文件。
    fn runtime(manifest: serde_json::Value, files: &[&str]) -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        for file in files {
            let path = root.path().join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"library").unwrap();
        }
        std::fs::write(
            root.path().join(RUNTIME_MANIFEST_FILE),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        root
    }

    /// Builds an installer-shaped manifest for the two libraries.
    /// 为两个库构造安装器形状的清单。
    fn manifest(core: &str, webgpu: &str) -> serde_json::Value {
        serde_json::json!({
            "schema_version": 1,
            "onnxruntime": "1.30.0",
            "webgpu_plugin": "0.4.0",
            "core_library": core,
            "webgpu_library": webgpu,
            "files": { core: "00", webgpu: "00" },
        })
    }

    #[test]
    fn loads_declared_libraries_and_their_directories() {
        let root = runtime(
            manifest("onnxruntime/core.dll", "webgpu/plugin.dll"),
            &["onnxruntime/core.dll", "webgpu/plugin.dll"],
        );
        let loaded = BundledRuntime::load(root.path()).unwrap();
        assert_eq!(
            loaded.core_library,
            root.path().join("onnxruntime/core.dll")
        );
        assert_eq!(
            loaded.library_directories(),
            vec![root.path().join("onnxruntime"), root.path().join("webgpu")]
        );
    }

    #[test]
    fn refuses_missing_escaping_and_unknown_schema() {
        let missing = runtime(
            manifest("onnxruntime/core.dll", "webgpu/plugin.dll"),
            &["onnxruntime/core.dll"],
        );
        assert!(matches!(
            BundledRuntime::load(missing.path()),
            Err(RuntimeError::MissingLibrary(_))
        ));
        let escaping = runtime(manifest("../core.dll", "webgpu/plugin.dll"), &[]);
        assert!(matches!(
            BundledRuntime::load(escaping.path()),
            Err(RuntimeError::UndeclaredLibrary(_))
        ));
        let mut future = manifest("onnxruntime/core.dll", "webgpu/plugin.dll");
        future["schema_version"] = serde_json::json!(2);
        let future = runtime(future, &["onnxruntime/core.dll", "webgpu/plugin.dll"]);
        assert!(matches!(
            BundledRuntime::load(future.path()),
            Err(RuntimeError::UnsupportedSchema(2))
        ));
    }
}
