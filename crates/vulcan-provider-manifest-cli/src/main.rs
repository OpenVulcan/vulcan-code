//! Provider-manifest command-line tool (plan §R10).
//! 供应商清单命令行工具（方案 §R10）。
//!
//! Subcommands: validate, validate-all, compile, inspect, diff, build-index, sign-index,
//! verify-index, fixture-check, dry-run-activate. The CLI never embeds a production private key, never
//! publishes automatically, and never bypasses the Bundled trust ceiling.
//! 子命令：validate、validate-all、compile、inspect、diff、build-index、sign-index、verify-index、fixture-check、
//! dry-run-activate。CLI 不内置生产私钥、不自动发布、不绕过内置信任上限。
//!
//! Exit codes: 0 success, 1 operational failure, 2 usage error.
//! 退出码：0 成功、1 操作失败、2 用法错误。

use std::io::Read;
use std::path::{Path, PathBuf};
use vulcan_provider_adapters::ManifestOwnership;
use vulcan_provider_config::manifest_sync::{
    RemoteCoreModelsEntry, RemoteProviderEntry, TrustedSigningKey, build_index_document,
    sign_index_document, verify_and_parse_index, verify_provider_bytes,
};

/// Source-tree core model list text, read once and shared by every compile in this process.
/// 源码树核心模型清单文本，每个进程读取一次，供本进程全部编译共享。
fn source_core_models_text() -> Result<&'static str, String> {
    static TEXT: std::sync::OnceLock<Result<String, String>> = std::sync::OnceLock::new();
    TEXT.get_or_init(|| {
        let path = source_core_models_path();
        std::fs::read_to_string(&path).map_err(|error| {
            format!(
                "core model list `{}` could not be read: {error}",
                path.display()
            )
        })
    })
    .as_deref()
    .map_err(Clone::clone)
}

/// Returns the source-tree core model list path next to the bundled index.
/// 返回与内置 Index 同目录的源码树核心模型清单路径。
fn source_core_models_path() -> PathBuf {
    vulcan_runtime_assets::source_bundled_provider_manifest_index_path()
        .with_file_name(vulcan_provider_adapters::provider_manifest::CORE_MODELS_RELATIVE_URL)
}

/// Compiles the source-tree core model list the CLI resolves every reference against.
/// 编译 CLI 解析全部引用所用的源码树核心模型清单。
///
/// The CLI reads the core file directly instead of through the bundled index, so `build-index`
/// can pin a core list the current index does not pin yet.
/// CLI 直接读取核心文件而不经过内置 Index，使 `build-index` 能固定当前 Index 尚未固定的核心清单。
fn source_core_models()
-> Result<&'static vulcan_provider_adapters::provider_manifest::CoreModelCatalog, String> {
    static CORE: std::sync::OnceLock<
        Result<vulcan_provider_adapters::provider_manifest::CoreModelCatalog, String>,
    > = std::sync::OnceLock::new();
    CORE.get_or_init(|| {
        vulcan_provider_adapters::provider_manifest::compile_core_models_text(
            source_core_models_text()?,
        )
        .map_err(|error| format!("core model list is invalid: {error}"))
    })
    .as_ref()
    .map_err(Clone::clone)
}

/// Compiles one system manifest against the source-tree core model list.
/// 以源码树核心模型清单编译一份系统清单。
fn compile_system_manifest(
    text: &str,
) -> Result<
    vulcan_provider_adapters::ResolvedProviderManifest,
    vulcan_provider_adapters::provider_manifest::ManifestValidationError,
> {
    let core = source_core_models().map_err(|error| {
        vulcan_provider_adapters::provider_manifest::ManifestValidationError::new(
            "core_models_unavailable",
            "$",
            error,
        )
    })?;
    vulcan_provider_adapters::compile_manifest_text_with_core(
        text,
        ManifestOwnership::System,
        vulcan_provider_adapters::provider_manifest::ManifestSourceLayer::Bundled,
        Some(core),
    )
}

/// Usage failure exit code.
/// 用法失败退出码。
const EXIT_USAGE: i32 = 2;
/// Operational failure exit code.
/// 操作失败退出码。
const EXIT_FAILURE: i32 = 1;

/// Program entry point; every subcommand returns a stable exit code.
/// 程序入口；每个子命令返回稳定退出码。
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let exit_code = match dispatch(&args) {
        Ok(()) => 0,
        Err(CliError::Usage(message)) => {
            eprintln!("usage error: {message}");
            EXIT_USAGE
        }
        Err(CliError::Operation(message)) => {
            eprintln!("error: {message}");
            EXIT_FAILURE
        }
    };
    std::process::exit(exit_code);
}

/// One controlled CLI failure.
/// 一次受控 CLI 失败。
#[derive(Debug)]
enum CliError {
    /// Invalid command-line usage.
    /// 无效命令行用法。
    Usage(String),
    /// Valid usage but the operation failed.
    /// 用法正确但操作失败。
    Operation(String),
}

impl From<String> for CliError {
    fn from(value: String) -> Self {
        CliError::Operation(value)
    }
}

/// Dispatches one subcommand with its arguments.
/// 分派一个子命令及其参数。
fn dispatch(args: &[String]) -> Result<(), CliError> {
    let Some(command) = args.first() else {
        return Err(CliError::Usage(
            "missing subcommand; expected one of: validate, validate-all, compile, inspect, diff, build-index, sign-index, verify-index, fixture-check, dry-run-activate"
                .to_string(),
        ));
    };
    if args[1..]
        .iter()
        .any(|argument| matches!(argument.as_str(), "--help" | "-h"))
    {
        return print_subcommand_help(command);
    }
    match command.as_str() {
        "validate" => cmd_validate(&args[1..]),
        "validate-all" => cmd_validate_all(&args[1..]),
        "compile" => cmd_compile(&args[1..]),
        "inspect" => cmd_inspect(&args[1..]),
        "diff" => cmd_diff(&args[1..]),
        "build-index" => cmd_build_index(&args[1..]),
        "sign-index" => cmd_sign_index(&args[1..]),
        "verify-index" => cmd_verify_index(&args[1..]),
        "fixture-check" => cmd_fixture_check(&args[1..]),
        "dry-run-activate" => cmd_dry_run_activate(&args[1..]),
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        other => Err(CliError::Usage(format!(
            "unknown subcommand `{other}`; run `provider-manifest help`"
        ))),
    }
}

/// Prints help for one known subcommand without entering its file or network path.
/// 打印一个已知子命令的帮助，并且不进入其文件或网络执行路径。
fn print_subcommand_help(command: &str) -> Result<(), CliError> {
    let usage = match command {
        "validate" => "validate <file|dir>",
        "validate-all" => "validate-all",
        "compile" => "compile <file>",
        "inspect" => "inspect <file>",
        "diff" => "diff <old> <new>",
        "build-index" => {
            "build-index <dir> [--revision N] [--key-id ID] [--issued-ms T] [--expires-ms T] [--relative-prefix PREFIX] [--out <path>]"
        }
        "sign-index" => {
            "sign-index <index-file> --key-file <path> [--out <path>] [--public-key-out <path>]"
        }
        "verify-index" => {
            "verify-index <index-file> --signature <hex> --public-key <hex> [--key-id ID]"
        }
        "fixture-check" => "fixture-check",
        "dry-run-activate" => "dry-run-activate <file> [--store <dir>]",
        other => {
            return Err(CliError::Usage(format!(
                "unknown subcommand `{other}`; run `provider-manifest help`"
            )));
        }
    };
    println!("usage: provider-manifest {usage}");
    Ok(())
}

/// Prints the CLI usage summary.
/// 打印 CLI 用法摘要。
fn print_help() {
    println!(
        "provider-manifest subcommands:\n  \
         validate <file|dir>\n  \
         validate-all\n  \
         compile <file>\n  \
         inspect <file>\n  \
         diff <old> <new>\n  \
         build-index <dir> [--revision N] [--key-id ID] [--issued-ms T] [--expires-ms T] [--relative-prefix PREFIX] [--out <path>]\n  \
         sign-index <index-file> --key-file <path> [--out <path>] [--public-key-out <path>]\n  \
         verify-index <index-file> --signature <hex> --public-key <hex> [--key-id ID]\n  \
         fixture-check\n  \
         dry-run-activate <file> [--store <dir>]\n"
    );
}

/// Validates every source-tree bundled manifest through the production compiler.
/// 通过生产编译器校验源码树中的每一份内置清单。
fn cmd_validate_all(args: &[String]) -> Result<(), CliError> {
    if !args.is_empty() {
        return Err(CliError::Usage(String::from(
            "validate-all does not accept arguments",
        )));
    }
    // indexedProviderCount proves the committed package index binds the same complete source
    // generation that schema and compiler validation inspect below.
    // indexedProviderCount 证明已提交的打包 Index 绑定下方 Schema 与编译器校验所检查的
    // 同一完整源码代际。
    let indexed_provider_count =
        vulcan_provider_adapters::verify_source_bundled_manifest_integrity_index()
            .map_err(CliError::Operation)?;
    println!("validate-all: bundled integrity index binds {indexed_provider_count} provider(s)");
    let core_model_count = vulcan_provider_adapters::verify_source_bundled_core_models()
        .map_err(CliError::Operation)?;
    println!("validate-all: bundled integrity index binds {core_model_count} core model(s)");
    let directory = vulcan_runtime_assets::source_bundled_provider_manifests_directory();
    validate_manifest_json_schema(&directory)?;
    validate_core_models_json_schema()?;
    cmd_validate(&[directory.to_string_lossy().into_owned()])
}

/// Validates the committed schema itself and every bundled manifest against Draft 2020-12.
/// 校验已提交 Schema 自身，并按 Draft 2020-12 校验每份内置 Manifest。
///
/// # Parameters
/// 参数
/// `directory` is the exact source-tree bundled manifest directory.
/// `directory` 是精确的源码树内置 Manifest 目录。
///
/// # Returns
/// 返回值
/// Success only when the schema is a valid Draft 2020-12 schema and every document conforms.
/// 仅当 Schema 是有效 Draft 2020-12 Schema 且每份文档均符合时返回成功。
fn validate_manifest_json_schema(directory: &Path) -> Result<(), CliError> {
    let schema_path = vulcan_runtime_assets::source_provider_manifest_schema_directory()
        .join("provider-manifest-v2.schema.json");
    let schema_text = read_bounded(&schema_path)?;
    let schema = parse_bounded_json(&schema_text, &format!("schema `{}`", schema_path.display()))?;
    jsonschema::draft202012::meta::validate(&schema).map_err(|error| {
        CliError::Operation(format!(
            "schema `{}` is not a valid Draft 2020-12 schema: {error}",
            schema_path.display()
        ))
    })?;
    let validator = jsonschema::draft202012::new(&schema).map_err(|error| {
        CliError::Operation(format!(
            "schema `{}` could not be compiled: {error}",
            schema_path.display()
        ))
    })?;
    let mut failures = 0_u32;
    for path in manifest_files(directory)? {
        let text = read_bounded(&path)?;
        let document: serde_json::Value =
            match parse_bounded_json(&text, &format!("manifest `{}`", path.display())) {
                Ok(document) => document,
                Err(error) => {
                    failures += 1;
                    eprintln!("{}: {error}", path.display());
                    continue;
                }
            };
        for error in validator.iter_errors(&document) {
            failures += 1;
            eprintln!(
                "{}{}: JSON Schema violation at {}: {}",
                path.display(),
                error.instance_path(),
                error.schema_path(),
                error
            );
        }
    }
    if failures > 0 {
        return Err(CliError::Operation(format!(
            "{failures} static JSON Schema violation(s) found."
        )));
    }
    Ok(())
}

/// Validates the source-tree core model list against its Draft 2020-12 schema.
/// 按 Draft 2020-12 Schema 校验源码树核心模型清单。
fn validate_core_models_json_schema() -> Result<(), CliError> {
    let schema_path = vulcan_runtime_assets::source_provider_manifest_schema_directory()
        .join("core-models-v1.schema.json");
    let schema_text = read_bounded(&schema_path)?;
    let schema = parse_bounded_json(&schema_text, &format!("schema `{}`", schema_path.display()))?;
    jsonschema::draft202012::meta::validate(&schema).map_err(|error| {
        CliError::Operation(format!(
            "schema `{}` is not a valid Draft 2020-12 schema: {error}",
            schema_path.display()
        ))
    })?;
    let validator = jsonschema::draft202012::new(&schema).map_err(|error| {
        CliError::Operation(format!(
            "schema `{}` could not be compiled: {error}",
            schema_path.display()
        ))
    })?;
    let path = source_core_models_path();
    let text = read_bounded(&path)?;
    let document = parse_bounded_json(&text, &format!("core model list `{}`", path.display()))?;
    let mut failures = 0_u32;
    for error in validator.iter_errors(&document) {
        failures += 1;
        eprintln!(
            "{}{}: JSON Schema violation at {}: {}",
            path.display(),
            error.instance_path(),
            error.schema_path(),
            error
        );
    }
    if failures > 0 {
        return Err(CliError::Operation(format!(
            "{failures} core model list JSON Schema violation(s) found."
        )));
    }
    Ok(())
}

/// Prints one validation error from a compile attempt.
/// 打印一次编译尝试的单个校验错误。
fn print_compile_error(
    path: &Path,
    error: &vulcan_provider_adapters::provider_manifest::ManifestValidationError,
) {
    eprintln!("{}: {}", path.display(), error);
}

/// Validates one manifest file or every manifest in a directory.
/// 校验一个清单文件或目录中的全部清单。
fn cmd_validate(args: &[String]) -> Result<(), CliError> {
    let target = required_path(args, 0, "validate <file|dir>")?;
    let mut failures = 0_u32;
    for path in manifest_files(&target)? {
        let text = read_bounded(&path)?;
        match compile_system_manifest(&text) {
            Ok(resolved) => println!(
                "{}: valid (revision {}, provider {})",
                path.display(),
                resolved.revision,
                resolved.descriptor.id
            ),
            Err(error) => {
                failures += 1;
                print_compile_error(&path, &error);
            }
        }
    }
    if failures > 0 {
        return Err(CliError::Operation(format!(
            "{failures} manifest(s) failed validation."
        )));
    }
    Ok(())
}

/// Compiles one manifest and prints its revision, provider id, and rule/route counts.
/// 编译一份清单并打印其修订号、供应商 ID 与规则/路由计数。
fn cmd_compile(args: &[String]) -> Result<(), CliError> {
    let path = required_path(args, 0, "compile <file>")?;
    let text = read_bounded(&path)?;
    let resolved =
        compile_system_manifest(&text).map_err(|error| CliError::Operation(error.to_string()))?;
    println!("provider: {}", resolved.descriptor.id);
    println!("revision: {}", resolved.revision);
    println!(
        "service entries: {}",
        resolved.descriptor.service_entries.len()
    );
    println!("model rules: {}", resolved.rule_instances.len());
    println!(
        "tool targets: {}",
        resolved
            .tool_targets
            .iter()
            .map(|target| target.target_id.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!(
        "tool target template instances: {}",
        resolved
            .tool_target_template_instances
            .values()
            .map(|instance| format!(
                "{}<={}[{}:{}]",
                instance.target_id,
                instance.template_id,
                instance.service_entry_id,
                instance.model_id
            ))
            .collect::<Vec<_>>()
            .join(", ")
    );
    Ok(())
}

/// Prints compiled routing, profile, operation, upload, tool, and provenance facts.
/// 打印编译后的路由、Profile、operation、upload、tool 与 provenance 事实。
fn cmd_inspect(args: &[String]) -> Result<(), CliError> {
    let path = required_path(args, 0, "inspect <file>")?;
    let text = read_bounded(&path)?;
    let resolved =
        compile_system_manifest(&text).map_err(|error| CliError::Operation(error.to_string()))?;
    println!("provider: {}", resolved.descriptor.id);
    println!("revision: {}", resolved.revision);
    println!(
        "source: {:?} ({:?})",
        resolved.snapshot_provenance.source_layer, resolved.snapshot_provenance.provenance_status
    );
    for entry in &resolved.descriptor.service_entries {
        println!("service {}: {}", entry.id, entry.display_name);
        for route in &entry.protocol_routes {
            println!(
                "  route {} -> {:?}",
                route.protocol_profile_id, route.endpoint_path
            );
        }
    }
    for rule in &resolved.rule_instances {
        println!(
            "rule {} [{}]: matcher={:?}:{}, protocols={:?}, selected={:?}, enabled={}, parameters={:?}, constraints={}, usage={:?}, request={:?}, response={:?}, operations={:?}",
            rule.rule_id,
            rule.service_entry_id,
            rule.matcher.kind,
            rule.matcher.value,
            rule.supported_protocols,
            rule.selected_protocol,
            rule.default_enabled,
            rule.model_parameters
                .iter()
                .map(|parameter| parameter.id.as_str())
                .collect::<Vec<_>>(),
            rule.input_resource_constraints.len(),
            rule.usage_profile_refs,
            rule.request_profile_refs,
            rule.response_profile_refs,
            rule.operation_refs
        );
        for (field, provenance) in &rule.field_provenance {
            println!(
                "  field {field}: {:?} rule={:?} revision={}",
                provenance.source, provenance.rule_id, provenance.manifest_revision
            );
            for overridden in &provenance.overridden_sources {
                println!(
                    "    overrides: {:?} rule={:?} revision={}",
                    overridden.source, overridden.rule_id, overridden.manifest_revision
                );
            }
        }
        for (field, provenance) in &rule.capability_provenance {
            println!(
                "  capability {field}: {:?} rule={:?} revision={}",
                provenance.source, provenance.rule_id, provenance.manifest_revision
            );
            for overridden in &provenance.overridden_sources {
                println!(
                    "    overrides: {:?} rule={:?} revision={}",
                    overridden.source, overridden.rule_id, overridden.manifest_revision
                );
            }
        }
    }
    for operation in &resolved.operations {
        println!(
            "operation {}: targets={:?}, uploads={:?}, async={}",
            operation.operation_id,
            operation.tool_target_refs,
            operation.upload_channel_refs,
            operation.async_execution.is_some()
        );
    }
    for target in &resolved.tool_targets {
        println!(
            "tool target {}: {:?}/{:?}",
            target.target_id, target.execution.execution_kind, target.execution.adapter
        );
    }
    for instance in resolved.tool_target_template_instances.values() {
        println!(
            "tool target template instance {}: template={}, service={}, model={} ({})",
            instance.target_id,
            instance.template_id,
            instance.service_entry_id,
            instance.model_id,
            instance.model_display_name
        );
    }
    for (key, provenance) in &resolved.declaration_provenance {
        println!(
            "declaration {key}: {:?} revision={} sha256={:?}",
            provenance.source_layer, provenance.manifest_revision, provenance.sha256
        );
    }
    Ok(())
}

/// Prints a bounded structural diff between two manifests.
/// 打印两份清单之间的有界结构差异。
fn cmd_diff(args: &[String]) -> Result<(), CliError> {
    let old_path = required_path(args, 0, "diff <old> <new>")?;
    let new_path = required_path(args, 1, "diff <old> <new>")?;
    let old_text = read_bounded(&old_path)?;
    let new_text = read_bounded(&new_path)?;
    let old = parse_bounded_json(&old_text, "old manifest")?;
    let new = parse_bounded_json(&new_text, "new manifest")?;

    fn field(value: &serde_json::Value, key: &str) -> Option<String> {
        value
            .get(key)
            .map(|value| value.to_string())
            .filter(|text| text != "null")
    }

    let old_revision = field(&old, "revision").unwrap_or_default();
    let new_revision = field(&new, "revision").unwrap_or_default();
    if old_revision != new_revision {
        println!("revision: {old_revision} -> {new_revision}");
    }
    let old_engine = field(&old, "engine_contract").unwrap_or_default();
    let new_engine = field(&new, "engine_contract").unwrap_or_default();
    if old_engine != new_engine {
        println!("engine_contract: changed");
    }
    for key in ["model_defaults", "provenance"] {
        if old.get(key) != new.get(key) {
            println!("{key}: changed");
        }
    }
    for (array, identity) in [
        ("services", "service_entry_id"),
        ("model_rules", "rule_id"),
        ("usage_profiles", "profile_id"),
        ("request_profiles", "profile_id"),
        ("response_profiles", "profile_id"),
        ("operations", "operation_id"),
        ("tool_targets", "target_id"),
        ("tool_target_templates", "template_id"),
        ("upload_channels", "upload_channel_id"),
    ] {
        print_named_array_diff(&old, &new, array, identity)?;
    }
    print_template_binding_diff(&old, &new)?;
    Ok(())
}

/// Prints deterministic changes to data-only template bindings keyed by exact service and model.
/// 按精确服务入口与模型打印纯数据模板绑定的确定性变化。
///
/// # Parameters
/// 参数
/// `old` and `new` are parsed manifest documents whose binding arrays are compared.
/// `old` 与 `new` 是待比较绑定数组的已解析清单文档。
///
/// # Returns
/// 返回值
/// Success after printing the bounded diff, or a controlled shape/duplicate diagnostic.
/// 打印有界差异后返回成功，结构错误或重复身份则返回受控诊断。
fn print_template_binding_diff(
    old: &serde_json::Value,
    new: &serde_json::Value,
) -> Result<(), CliError> {
    const ARRAY_NAME: &str = "tool_target_template_bindings";
    let collect = |document: &serde_json::Value| {
        let mut values = std::collections::BTreeMap::new();
        for value in document
            .get(ARRAY_NAME)
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
        {
            let service_entry_id = value
                .get("service_entry_id")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| {
                    CliError::Operation(format!(
                        "array `{ARRAY_NAME}` contains an item without `service_entry_id`"
                    ))
                })?;
            let model_id = value
                .get("model_id")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| {
                    CliError::Operation(format!(
                        "array `{ARRAY_NAME}` contains an item without `model_id`"
                    ))
                })?;
            let identity = (service_entry_id.to_string(), model_id.to_string());
            if values.insert(identity.clone(), value.clone()).is_some() {
                return Err(CliError::Operation(format!(
                    "array `{ARRAY_NAME}` contains duplicate identity `{service_entry_id}/{model_id}`"
                )));
            }
        }
        Ok::<_, CliError>(values)
    };
    let old_values = collect(old)?;
    let new_values = collect(new)?;
    for (service_entry_id, model_id) in old_values.keys() {
        if !new_values.contains_key(&(service_entry_id.clone(), model_id.clone())) {
            println!("{ARRAY_NAME}: removed {service_entry_id}/{model_id}");
        }
    }
    for ((service_entry_id, model_id), value) in &new_values {
        match old_values.get(&(service_entry_id.clone(), model_id.clone())) {
            None => println!("{ARRAY_NAME}: added {service_entry_id}/{model_id}"),
            Some(old_value) if old_value != value => {
                println!("{ARRAY_NAME}: changed {service_entry_id}/{model_id}")
            }
            Some(_) => {}
        }
    }
    Ok(())
}

/// Prints deterministic added, removed, and changed identities for one manifest array.
/// 为一个 Manifest 数组打印确定性的新增、删除与变化标识。
fn print_named_array_diff(
    old: &serde_json::Value,
    new: &serde_json::Value,
    array_name: &str,
    identity_field: &str,
) -> Result<(), CliError> {
    let collect = |document: &serde_json::Value| {
        let mut values = std::collections::BTreeMap::new();
        for value in document
            .get(array_name)
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
        {
            let identity = value
                .get(identity_field)
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| {
                    CliError::Operation(format!(
                        "array `{array_name}` contains an item without `{identity_field}`"
                    ))
                })?;
            if values.insert(identity.to_string(), value.clone()).is_some() {
                return Err(CliError::Operation(format!(
                    "array `{array_name}` contains duplicate identity `{identity}`"
                )));
            }
        }
        Ok::<_, CliError>(values)
    };
    let old_values = collect(old)?;
    let new_values = collect(new)?;
    for identity in old_values.keys() {
        if !new_values.contains_key(identity) {
            println!("{array_name}: removed {identity}");
        }
    }
    for (identity, new_value) in &new_values {
        match old_values.get(identity) {
            None => println!("{array_name}: added {identity}"),
            Some(old_value) if old_value != new_value => {
                println!("{array_name}: changed {identity}")
            }
            Some(_) => {}
        }
    }
    Ok(())
}

/// Builds a signed-ready index document from a manifest directory.
/// 从清单目录构建一份待签名 Index 文档。
fn cmd_build_index(args: &[String]) -> Result<(), CliError> {
    let directory = required_path(args, 0, "build-index <dir>")?;
    // relativePrefix is absent for root-relative remote URLs and explicitly set to `manifests`
    // when regenerating the packaged bundled integrity index.
    // relativePrefix 在远端根相对 URL 场景下缺省；重新生成打包内置完整性 Index 时显式设为
    // `manifests`。
    let relative_prefix = flag_value(args, "--relative-prefix");
    if let Some(prefix) = relative_prefix
        && (prefix.is_empty()
            || prefix.len() > 1024
            || prefix.starts_with('/')
            || prefix.ends_with('/')
            || prefix.contains('\\')
            || prefix.split('/').any(|segment| {
                segment.is_empty()
                    || matches!(segment, "." | "..")
                    || !segment.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')
                    })
            }))
    {
        return Err(CliError::Usage(String::from(
            "--relative-prefix must be a safe slash-separated relative path",
        )));
    }
    let mut entries = Vec::new();
    for path in manifest_files(&directory)? {
        let text = read_bounded(&path)?;
        let resolved = compile_system_manifest(&text)
            .map_err(|error| CliError::Operation(format!("{}: {error}", path.display())))?;
        let provider_id = resolved.descriptor.id.clone();
        let sha256 = vulcan_provider_config::manifest_store::ManifestStoreLayout::sha256_hex(
            text.as_bytes(),
        );
        // relativeUrl preserves the signed remote default while supporting the exact packaged
        // `manifests/<provider>.json` inventory layout through an explicit CLI flag.
        // relativeUrl 保留签名远端默认值，同时通过显式 CLI 旗标支持精确的打包
        // `manifests/<provider>.json` 清单布局。
        let relative_url = relative_prefix.map_or_else(
            || format!("/{provider_id}.json"),
            |prefix| format!("{prefix}/{provider_id}.json"),
        );
        entries.push(RemoteProviderEntry {
            provider_id: provider_id.clone(),
            revision: resolved.revision,
            relative_url,
            sha256,
            bytes: text.len() as u64,
            minimum_engine_contract: u64::from(resolved.engine_contract.minimum_manifest_engine),
        });
    }
    entries.sort_by(|left, right| left.provider_id.cmp(&right.provider_id));
    let revision = flag_value(args, "--revision").unwrap_or("1");
    let key_id = flag_value(args, "--key-id").unwrap_or("publish-key-1");
    // A malformed timestamp is a usage error; only an omitted flag selects its default, so a typo
    // never publishes an index with silently wrong validity bounds.
    // 时间戳格式错误属于用法错误；只有省略参数时才使用默认值，避免笔误静默发布有效期错误的索引。
    let now_ms = match flag_value(args, "--issued-ms") {
        Some(value) => value
            .parse::<i64>()
            .map_err(|_| CliError::Usage("--issued-ms must be an integer".into()))?,
        None => current_unix_millis(),
    };
    let expires_ms = match flag_value(args, "--expires-ms") {
        Some(value) => value
            .parse::<i64>()
            .map_err(|_| CliError::Usage("--expires-ms must be an integer".into()))?,
        None => now_ms.saturating_add(24 * 60 * 60 * 1_000),
    };
    let revision: u64 = revision
        .parse()
        .map_err(|_| CliError::Usage("--revision must be a positive integer".into()))?;
    // coreModels pins the exact source core list; the packaged index keeps it next to itself and
    // a remote index serves it from the origin root.
    // coreModels 固定精确的源码核心清单；打包 Index 将其放在自身旁边，远端 Index 从源根目录提供。
    let core = source_core_models().map_err(CliError::Operation)?;
    let core_text = source_core_models_text().map_err(CliError::Operation)?;
    let core_file = vulcan_provider_adapters::provider_manifest::CORE_MODELS_RELATIVE_URL;
    let core_models = RemoteCoreModelsEntry {
        revision: core.revision(),
        relative_url: if relative_prefix.is_some() {
            String::from(core_file)
        } else {
            format!("/{core_file}")
        },
        sha256: vulcan_provider_config::manifest_store::ManifestStoreLayout::sha256_hex(
            core_text.as_bytes(),
        ),
        bytes: core_text.len() as u64,
        minimum_engine_contract: 1,
    };
    let index_bytes =
        build_index_document(entries, core_models, revision, key_id, now_ms, expires_ms);
    let index_text = String::from_utf8(index_bytes)
        .map_err(|_| String::from("index serialization produced invalid UTF-8"))?;
    if let Some(out) = flag_path(args, "--out") {
        std::fs::write(&out, format!("{index_text}\n")).map_err(|error| {
            format!(
                "index output `{}` could not be written: {error}",
                out.display()
            )
        })?;
        println!(
            "index written to {}: revision {revision}, key {key_id}, issued {now_ms}, expires {expires_ms}",
            out.display()
        );
    } else {
        println!(
            "index built: revision {revision}, key {key_id}, issued {now_ms}, expires {expires_ms}"
        );
        println!("{index_text}");
    }
    Ok(())
}

/// Signs one index document with an explicit key file and prints the detached signature hex.
/// 用显式密钥文件签名一份 Index 文档并打印分离签名十六进制。
fn cmd_sign_index(args: &[String]) -> Result<(), CliError> {
    let index_path = required_path(args, 0, "sign-index <index-file> --key-file <path>")?;
    let key_path = flag_path(args, "--key-file")
        .ok_or_else(|| CliError::Usage("sign-index requires --key-file <path>".into()))?;
    let index_bytes = read_bounded(&index_path)?;
    let key_hex = read_bounded_utf8(&key_path, 256, "signing key file")?;
    let key_hex = key_hex.trim();
    let key_bytes = decode_hex(key_hex).ok_or_else(|| {
        CliError::Usage("key file must contain exactly 64 hex characters (32 bytes)".into())
    })?;
    let seed: [u8; 32] = key_bytes.try_into().map_err(|_| {
        CliError::Usage("key file must contain exactly 32 bytes (64 hex characters)".into())
    })?;
    let signature = sign_index_document(index_bytes.as_bytes(), &seed)?;
    let signature_hex = signature
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if let Some(out) = flag_path(args, "--out") {
        std::fs::write(&out, format!("{signature_hex}\n")).map_err(|error| {
            format!(
                "signature output `{}` could not be written: {error}",
                out.display()
            )
        })?;
        println!("signature written to {}", out.display());
    } else {
        println!("{signature_hex}");
    }
    if let Some(out) = flag_path(args, "--public-key-out") {
        let public_key_hex = vulcan_provider_config::manifest_sync::public_key_for_seed(&seed)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        std::fs::write(&out, format!("{public_key_hex}\n")).map_err(|error| {
            format!(
                "public-key output `{}` could not be written: {error}",
                out.display()
            )
        })?;
        println!("public key written to {}", out.display());
    }
    Ok(())
}

/// Verifies one signed index document against an explicit public key.
/// 用显式公钥校验一份签名 Index 文档。
fn cmd_verify_index(args: &[String]) -> Result<(), CliError> {
    let index_path = required_path(
        args,
        0,
        "verify-index <index-file> --signature <hex> --public-key <hex>",
    )?;
    let signature_hex = flag_value(args, "--signature")
        .ok_or_else(|| CliError::Usage("verify-index requires --signature <hex>".into()))?;
    let public_key_hex = flag_value(args, "--public-key")
        .ok_or_else(|| CliError::Usage("verify-index requires --public-key <hex>".into()))?;
    let key_id = flag_value(args, "--key-id").unwrap_or("publish-key-1");
    let index_bytes = read_bounded(&index_path)?;
    let signature = decode_hex(signature_hex.trim()).ok_or_else(|| {
        CliError::Usage("--signature must be 128 hex characters (64 bytes)".into())
    })?;
    let public_key = decode_hex(public_key_hex.trim()).ok_or_else(|| {
        CliError::Usage("--public-key must be 64 hex characters (32 bytes)".into())
    })?;
    let public_key: [u8; 32] = public_key.try_into().map_err(|_| {
        CliError::Usage("--public-key must be exactly 32 bytes (64 hex characters)".into())
    })?;
    let trusted = vec![TrustedSigningKey {
        key_id: key_id.to_string(),
        public_key,
    }];
    let now_ms = current_unix_millis();
    let index = verify_and_parse_index(index_bytes.as_bytes(), &signature, &trusted, now_ms)?;
    println!(
        "index valid: revision {}, {} providers, expires {}",
        index.index_revision,
        index.providers.len(),
        index.expires_at_unix_ms
    );
    Ok(())
}

/// Runs the full bundled fixture validation over every source manifest.
/// 对全部源码内置清单执行完整 fixture 校验。
fn cmd_fixture_check(args: &[String]) -> Result<(), CliError> {
    if !args.is_empty() {
        return Err(CliError::Usage(String::from(
            "fixture-check does not accept arguments",
        )));
    }
    let directory = vulcan_runtime_assets::source_bundled_provider_manifests_directory();
    let files = manifest_files(&directory)?;
    if files.is_empty() {
        return Err(CliError::Operation(format!(
            "no bundled manifests found under {}",
            directory.display()
        )));
    }
    let mut failures = 0_u32;
    for path in &files {
        let text = read_bounded(path)?;
        match compile_system_manifest(&text) {
            Ok(resolved) => println!(
                "{}: ok (revision {})",
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("?"),
                resolved.revision
            ),
            Err(error) => {
                failures += 1;
                print_compile_error(path, &error);
            }
        }
    }
    println!(
        "fixture-check: {} bundled manifest(s), {failures} failed",
        files.len()
    );
    if failures > 0 {
        return Err(CliError::Operation(format!(
            "{failures} fixture(s) failed."
        )));
    }
    let fixtures_directory = vulcan_runtime_assets::source_provider_manifest_fixtures_directory();
    let coverage =
        vulcan_provider_adapters::provider_manifest_fixture::audit_provider_manifest_fixture_coverage(
            &fixtures_directory,
            &directory,
        )?;
    println!(
        "fixture-check: {} provider(s), {} production target(s), adapters {:?}",
        coverage.provider_count, coverage.target_count, coverage.adapter_counts
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("fixture runtime could not start: {error}"))?;
    // fixtureSuite is pinned before first poll so the expanded media execution future never occupies the CLI main stack.
    // fixtureSuite 在首次轮询前固定到堆上，避免扩展后的媒体执行Future占用CLI主线程栈。
    let fixture_suite = Box::pin(async {
        vulcan_provider_adapters::provider_manifest_fixture::execute_provider_manifest_fixture_suite(
            &fixtures_directory,
            &directory,
        )
        .await
    });
    let executable = runtime.block_on(fixture_suite)?;
    for result in &executable {
        println!(
            "{}: executed (output {}, {} HTTP interaction(s))",
            result.fixture_id, result.output_kind, result.interaction_count
        );
    }
    println!(
        "fixture-check: {} executable fixture(s) passed",
        executable.len()
    );
    Ok(())
}

/// Dry-runs one activation: verifies bytes, builds the index pin, and compiles without
/// committing to a runtime catalog (plan §R10 dry-run-activate).
/// 试运行一次激活：校验字节、构建 Index 固定值并编译，而不提交到运行时目录
/// （方案 §R10 dry-run-activate）。
fn cmd_dry_run_activate(args: &[String]) -> Result<(), CliError> {
    let path = required_path(args, 0, "dry-run-activate <file>")?;
    let text = read_bounded(&path)?;
    let resolved =
        compile_system_manifest(&text).map_err(|error| CliError::Operation(error.to_string()))?;
    // The candidate would be pinned by an index with these exact facts; verifying the bytes
    // proves activation eligibility without touching any store or runtime.
    // 候选将由 Index 以这些精确事实固定；校验字节可证明激活资格而不触碰任何存储或运行时。
    let entry = RemoteProviderEntry {
        provider_id: resolved.descriptor.id.clone(),
        revision: resolved.revision,
        relative_url: String::new(),
        sha256: vulcan_provider_config::manifest_store::ManifestStoreLayout::sha256_hex(
            text.as_bytes(),
        ),
        bytes: text.len() as u64,
        minimum_engine_contract: u64::from(resolved.engine_contract.minimum_manifest_engine),
    };
    verify_provider_bytes(&entry, text.as_bytes())
        .map_err(|error| format!("dry-run activation failed: {error}"))?;
    println!(
        "dry-run: provider `{}` revision {} would activate",
        resolved.descriptor.id, resolved.revision
    );
    Ok(())
}

/// Returns the exact path argument at one index.
/// 返回指定索引处的精确路径参数。
fn required_path(args: &[String], index: usize, usage: &str) -> Result<PathBuf, CliError> {
    args.get(index)
        .map(PathBuf::from)
        .ok_or_else(|| CliError::Usage(format!("missing argument; usage: {usage}")))
}

/// Returns a flag value by name.
/// 按名称返回一个旗标值。
fn flag_value<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|arg| arg == name)
        .and_then(|index| args.get(index + 1))
        .map(|value| value.as_str())
}

/// Returns a flag path by name.
/// 按名称返回一个旗标路径。
fn flag_path(args: &[String], name: &str) -> Option<PathBuf> {
    flag_value(args, name).map(PathBuf::from)
}

/// Lists every JSON manifest file under a path (file or directory).
/// 列出一个路径（文件或目录）下的全部 JSON 清单文件。
fn manifest_files(target: &Path) -> Result<Vec<PathBuf>, CliError> {
    /// Maximum filesystem entries inspected by one CLI directory operation.
    /// 单次 CLI 目录操作允许检查的文件系统条目最大数量。
    const MAX_DIRECTORY_ENTRIES: usize = 4_096;
    /// Maximum manifest documents accepted by one CLI directory operation.
    /// 单次 CLI 目录操作允许接受的 Manifest 文档最大数量。
    const MAX_MANIFEST_FILES: usize = 256;
    let mut files = Vec::new();
    if target.is_file() {
        files.push(target.to_path_buf());
        return Ok(files);
    }
    if target.is_dir() {
        let entries = std::fs::read_dir(target).map_err(|error| {
            format!(
                "directory `{}` could not be read: {error}",
                target.display()
            )
        })?;
        // scanned_entries bounds ignored files as well as selected JSON documents.
        // scanned_entries 同时限制被忽略文件与选中的 JSON 文档。
        let mut scanned_entries = 0usize;
        for entry in entries {
            scanned_entries = scanned_entries.saturating_add(1);
            if scanned_entries > MAX_DIRECTORY_ENTRIES {
                return Err(format!(
                    "directory `{}` exceeds the {MAX_DIRECTORY_ENTRIES}-entry scan limit",
                    target.display()
                )
                .into());
            }
            let entry =
                entry.map_err(|error| format!("directory entry could not be read: {error}"))?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
                if files.len() >= MAX_MANIFEST_FILES {
                    return Err(format!(
                        "directory `{}` exceeds the {MAX_MANIFEST_FILES}-manifest limit",
                        target.display()
                    )
                    .into());
                }
                files.push(path);
            }
        }
    } else {
        return Err(CliError::Usage(format!(
            "path `{}` is neither a file nor a directory",
            target.display()
        )));
    }
    files.sort();
    Ok(files)
}

/// Reads one bounded manifest file.
/// 读取一份有界清单文件。
fn read_bounded(path: &Path) -> Result<String, CliError> {
    // Reuse the single manifest byte bound instead of restating it: this CLI must never accept a
    // manifest that the runtime validator would reject.
    // 复用唯一的清单字节上限而不是重述它：本 CLI 绝不能接受运行时校验会拒绝的清单。
    read_bounded_utf8(
        path,
        vulcan_provider_adapters::MAX_MANIFEST_BYTES as u64,
        "manifest file",
    )
}

/// Reads one regular UTF-8 file without crossing its post-open byte boundary.
/// 在不越过打开后字节边界的前提下读取一份普通 UTF-8 文件。
///
/// # Parameters
/// 参数
/// `path` is exact, `max_bytes` is the hard cap, and `kind` scopes diagnostics.
/// `path` 为精确路径，`max_bytes` 为硬上限，`kind` 限定诊断范围。
///
/// # Returns
/// 返回值
/// Exact UTF-8 text or a controlled type, size, I/O, replacement, or encoding failure.
/// 返回精确 UTF-8 文本，或受控的类型、大小、I/O、替换或编码失败。
fn read_bounded_utf8(path: &Path, max_bytes: u64, kind: &str) -> Result<String, CliError> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| format!("{kind} `{}` could not be stat-ed: {error}", path.display()))?;
    if !metadata.file_type().is_file() {
        return Err(format!("{kind} `{}` is not a regular file", path.display()).into());
    }
    if metadata.len() > max_bytes {
        return Err(format!(
            "{kind} `{}` exceeds the {max_bytes} byte bound",
            path.display()
        )
        .into());
    }
    let file = std::fs::File::open(path)
        .map_err(|error| format!("{kind} `{}` could not be opened: {error}", path.display()))?;
    let mut limited = file.take(max_bytes.saturating_add(1));
    let mut bytes = Vec::new();
    limited
        .read_to_end(&mut bytes)
        .map_err(|error| format!("{kind} `{}` could not be read: {error}", path.display()))?;
    if bytes.len() as u64 > max_bytes {
        return Err(format!(
            "{kind} `{}` changed while reading and exceeds the {max_bytes} byte bound",
            path.display()
        )
        .into());
    }
    String::from_utf8(bytes)
        .map_err(|error| format!("{kind} `{}` is not valid UTF-8: {error}", path.display()).into())
}

/// Parses one already byte-bounded JSON document under the shared depth and node limits.
/// 在共享深度与节点数量限制下解析一份已经受字节约束的 JSON 文档。
///
/// # Parameters
/// 参数
/// `text` is exact UTF-8 input and `label` scopes the controlled diagnostic.
/// `text` 是精确 UTF-8 输入，`label` 限定受控诊断范围。
///
/// # Returns
/// 返回值
/// A bounded JSON tree or a syntax, depth, or node-count failure.
/// 返回有界 JSON 树，或语法、深度、节点数量失败。
fn parse_bounded_json(text: &str, label: &str) -> Result<serde_json::Value, String> {
    let value =
        serde_json::from_str(text).map_err(|error| format!("{label} is invalid JSON: {error}"))?;
    vulcan_provider_adapters::provider_manifest::validate_runtime_json_bounds(&value, label)?;
    Ok(value)
}

/// Decodes a lowercase hex string into bytes.
/// 把十六进制字符串解码为字节。
fn decode_hex(value: &str) -> Option<Vec<u8>> {
    if !value.len().is_multiple_of(2) || !value.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return None;
    }
    (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).ok())
        .collect()
}

/// Returns the current unix milliseconds, or zero when the clock is unavailable.
/// 返回当前 Unix 毫秒；时钟不可用时返回零。
fn current_unix_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies hex decoding accepts lowercase and rejects malformed input.
    /// 验证十六进制解码接受小写并拒绝畸形输入。
    #[test]
    fn hex_decode_round_trip() {
        assert_eq!(decode_hex("00ff10"), Some(vec![0x00, 0xff, 0x10]));
        assert_eq!(decode_hex("0"), None);
        assert_eq!(decode_hex("zz"), None);
    }

    /// Verifies a valid bundled manifest compiles through the CLI validation path.
    /// 验证有效内置清单经 CLI 校验路径可编译。
    #[test]
    fn bundled_manifests_compile_for_cli() {
        let directory = vulcan_runtime_assets::source_bundled_provider_manifests_directory();
        let files = manifest_files(&directory).expect("manifest files");
        assert!(!files.is_empty(), "bundled manifests must exist");
        for path in files {
            let text = read_bounded(&path).expect("readable manifest");
            compile_system_manifest(&text)
                .unwrap_or_else(|error| panic!("{} must compile: {error}", path.display()));
        }
    }

    /// Verifies subcommand help never treats the help flag as an input path.
    /// 验证子命令帮助绝不会把帮助标志当作输入路径。
    #[test]
    fn subcommand_help_bypasses_execution_paths() {
        for command in ["inspect", "diff"] {
            dispatch(&[command.to_string(), "--help".to_string()])
                .unwrap_or_else(|error| panic!("{command} help must succeed: {error:?}"));
        }
    }

    /// Verifies sign-index and verify-index round trip with a deterministic key.
    /// 验证 sign-index 与 verify-index 使用确定性密钥往返。
    #[test]
    fn sign_and_verify_index_round_trip() {
        let directory = tempfile::tempdir().expect("temp dir");
        let manifest_path = directory.path().join("tavily.json");
        let text = std::fs::read_to_string(
            vulcan_runtime_assets::source_bundled_provider_manifests_directory()
                .join("tavily.json"),
        )
        .expect("bundled tavily manifest");
        std::fs::write(&manifest_path, &text).expect("write manifest");
        let resolved = compile_system_manifest(&text).expect("compile");
        let entry = RemoteProviderEntry {
            provider_id: resolved.descriptor.id.clone(),
            revision: resolved.revision,
            relative_url: String::from("/tavily.json"),
            sha256: vulcan_provider_config::manifest_store::ManifestStoreLayout::sha256_hex(
                text.as_bytes(),
            ),
            bytes: text.len() as u64,
            minimum_engine_contract: 1,
        };
        let now_ms = current_unix_millis();
        let index_bytes = build_index_document(
            vec![entry],
            vulcan_provider_config::manifest_sync::installed_core_models_entry(String::from(
                "/core-models.json",
            ))
            .expect("installed core model list"),
            1,
            "cli-test-key",
            now_ms,
            now_ms.saturating_add(60_000),
        );
        let mut seed = [0_u8; 32];
        seed[..8].copy_from_slice(b"cli-test");
        let signature = sign_index_document(&index_bytes, &seed).expect("sign");
        let public_key = vulcan_provider_config::manifest_sync::public_key_for_seed(&seed);
        let verified = verify_and_parse_index(
            &index_bytes,
            &signature,
            &[TrustedSigningKey {
                key_id: String::from("cli-test-key"),
                public_key,
            }],
            now_ms,
        )
        .expect("verify");
        assert_eq!(verified.providers.len(), 1);
        assert_eq!(verified.signing_key_id, "cli-test-key");
    }
}
