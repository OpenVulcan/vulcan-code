//! Explicit authoring commands use the same manifest and package validators as production.
//! 显式编写命令使用与生产相同的清单及插件包校验器。

use std::ffi::OsString;
use std::path::Path;
use vulcan_plugin_runtime::manifest_authoring::{
    migrate_manifest, sync_schemas, validate_package, write_new_manifest,
};

/// Run the requested authoring command and exit nonzero with an English diagnostic on failure.
/// 执行所请求的编写命令；失败时输出英文诊断并以非零状态退出。
fn main() {
    if let Err(error) = run(std::env::args_os().skip(1).collect()) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

/// Parse exact positional args, preserving native filesystem strings; never select commands by fallback.
/// 解析精确位置参数 args，保留原生文件系统字符串；绝不回退选择命令。
/// Returns success only after full validation, exact artifact checking or publication of a new manifest.
/// 仅在完整校验、精确产物检查或新清单发布完成后返回成功。
fn run(args: Vec<OsString>) -> Result<(), String> {
    match args.as_slice() {
        [command, directory] if command == "schema" || command == "schema-check" => {
            // Schema writes are explicit and restricted to the canonical generated artifact names.
            // Schema 写入是显式操作，且限定于规范生成产物名称。
            sync_schemas(Path::new(directory), command == "schema-check")
        }
        [command, package] if command == "validate" => {
            // Production package inspection checks identities, relative resources, schemas and dependency declarations.
            // 生产插件包检查校验身份、相对资源、Schema 及依赖声明。
            let descriptor = validate_package(Path::new(package))?;
            println!("Valid package: {}", descriptor.id);
            Ok(())
        }
        [command, input, output, rest @ ..] if command == "migrate" && rest.len() <= 1 => {
            // Input lives below the package's real directory name; it is read but never written.
            // 输入位于插件包真实目录名之下；只读取，绝不写入。
            let input = Path::new(input).canonicalize().map_err(|error| error.to_string())?;
            let package = input.parent().ok_or_else(|| "Manifest has no package directory.".to_string())?;
            let package_id = package.file_name().and_then(|name| name.to_str())
                .ok_or_else(|| "Package directory identity is not valid UTF-8.".to_string())?;
            let source = std::fs::read(&input).map_err(|error| error.to_string())?;
            let requirements = rest.first().map(std::fs::read).transpose().map_err(|error| error.to_string())?;
            let migrated = migrate_manifest(&source, package_id, requirements.as_deref())?;
            write_new_manifest(Path::new(output), &migrated)?;
            println!("Created manifest: {}", Path::new(output).display());
            Ok(())
        }
        _ => Err("Usage: vulcan-plugin-manifest schema|schema-check <directory> | validate <package-directory> | migrate <source-manifest> <new-output> [runtime-requirements-file]".into()),
    }
}
