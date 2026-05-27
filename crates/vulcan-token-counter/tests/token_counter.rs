//! Integration tests for the standalone token budget counter crate.
//! 独立 Token 预算统计 crate 的集成测试。

use std::{fs, path::Path};

use tempfile::TempDir;
use vulcan_token_counter::{
    ExactTokenizerKey, OpenAiEncoding, ProxyTokenizerFamily, TokenCounter, TokenEstimatePrecision,
    TokenizerBackend, TokenizerTier, default_openai_resource_dir, route_model,
};

/// Verifies that OpenAI model names route to exact native OpenAI encodings.
/// 验证 OpenAI 模型名称会路由到精确原生 OpenAI 编码。
#[test]
fn routes_openai_models_to_exact_encodings() {
    assert_eq!(
        route_model("openai/gpt-5.5"),
        TokenizerTier::NativeOpenAi {
            encoding: OpenAiEncoding::O200kBase
        }
    );
    assert_eq!(
        route_model("openai/gpt-4.1"),
        TokenizerTier::NativeOpenAi {
            encoding: OpenAiEncoding::O200kBase
        }
    );
    assert_eq!(
        route_model("openai/o3"),
        TokenizerTier::NativeOpenAi {
            encoding: OpenAiEncoding::O200kBase
        }
    );
    assert_eq!(
        route_model("gpt-4-turbo"),
        TokenizerTier::NativeOpenAi {
            encoding: OpenAiEncoding::Cl100kBase
        }
    );
}

/// Verifies that mainstream non-OpenAI coding models route to lightweight proxy families.
/// 验证主流非 OpenAI 编码模型会路由到轻量代理族。
#[test]
fn routes_non_openai_models_to_proxy_families() {
    assert_eq!(
        route_model("Qwen/Qwen3.7-Coder"),
        TokenizerTier::ExactTokie {
            key: ExactTokenizerKey::QwenCoder,
            local_precision: TokenEstimatePrecision::Exact,
            local_multiplier: 1.0,
            fallback_family: ProxyTokenizerFamily::EasternMixed,
            fallback_encoding: OpenAiEncoding::O200kBase,
            fallback_multiplier: 1.08
        }
    );
    assert_eq!(
        route_model("deepseek-ai/DeepSeek-V4-Pro"),
        TokenizerTier::ExactTokie {
            key: ExactTokenizerKey::DeepSeek,
            local_precision: TokenEstimatePrecision::Exact,
            local_multiplier: 1.0,
            fallback_family: ProxyTokenizerFamily::EasternMixed,
            fallback_encoding: OpenAiEncoding::O200kBase,
            fallback_multiplier: 1.08
        }
    );
    assert_eq!(
        route_model("volcengine/doubao-seed-2.0-code"),
        TokenizerTier::BudgetProxy {
            family: ProxyTokenizerFamily::EasternMixed,
            proxy_encoding: OpenAiEncoding::O200kBase,
            multiplier: 1.08
        }
    );
    assert_eq!(
        route_model("moonshotai/Kimi-K2.6"),
        TokenizerTier::BudgetProxy {
            family: ProxyTokenizerFamily::EasternMixed,
            proxy_encoding: OpenAiEncoding::O200kBase,
            multiplier: 1.08
        }
    );
    assert_eq!(
        route_model("baidu/ERNIE-4.5-300B-A47B-PT"),
        TokenizerTier::ExactTokie {
            key: ExactTokenizerKey::Ernie,
            local_precision: TokenEstimatePrecision::Exact,
            local_multiplier: 1.0,
            fallback_family: ProxyTokenizerFamily::EasternMixed,
            fallback_encoding: OpenAiEncoding::O200kBase,
            fallback_multiplier: 1.08
        }
    );
    assert_eq!(
        route_model("meta-llama/Llama-4-Maverick"),
        TokenizerTier::ExactTokie {
            key: ExactTokenizerKey::Llama,
            local_precision: TokenEstimatePrecision::Exact,
            local_multiplier: 1.0,
            fallback_family: ProxyTokenizerFamily::WesternOpen,
            fallback_encoding: OpenAiEncoding::O200kBase,
            fallback_multiplier: 1.05
        }
    );
    assert_eq!(
        route_model("mistralai/Mistral-Large-3"),
        TokenizerTier::ExactTokie {
            key: ExactTokenizerKey::Mistral,
            local_precision: TokenEstimatePrecision::Exact,
            local_multiplier: 1.0,
            fallback_family: ProxyTokenizerFamily::WesternOpen,
            fallback_encoding: OpenAiEncoding::O200kBase,
            fallback_multiplier: 1.05
        }
    );
    assert_eq!(
        route_model("nvidia/NVIDIA-Nemotron-3-Super-120B-A12B-FP8"),
        TokenizerTier::ExactTokie {
            key: ExactTokenizerKey::Nemotron,
            local_precision: TokenEstimatePrecision::Exact,
            local_multiplier: 1.0,
            fallback_family: ProxyTokenizerFamily::WesternOpen,
            fallback_encoding: OpenAiEncoding::O200kBase,
            fallback_multiplier: 1.05
        }
    );
    assert_eq!(
        route_model("xai-org/grok-2"),
        TokenizerTier::BudgetProxy {
            family: ProxyTokenizerFamily::WesternOpen,
            proxy_encoding: OpenAiEncoding::O200kBase,
            multiplier: 1.05
        }
    );
    assert_eq!(
        route_model("baidu/other-model"),
        TokenizerTier::BudgetProxy {
            family: ProxyTokenizerFamily::EasternMixed,
            proxy_encoding: OpenAiEncoding::O200kBase,
            multiplier: 1.08
        }
    );
    assert_eq!(
        route_model("nvidia/other-model"),
        TokenizerTier::BudgetProxy {
            family: ProxyTokenizerFamily::WesternOpen,
            proxy_encoding: OpenAiEncoding::O200kBase,
            multiplier: 1.05
        }
    );
}

/// Verifies that closed or unknown model families use conservative proxy routes.
/// 验证闭源或未知模型族会使用保守代理路由。
#[test]
fn routes_closed_and_unknown_models_to_conservative_proxies() {
    assert_eq!(
        route_model("claude-sonnet-4.5"),
        TokenizerTier::BudgetProxy {
            family: ProxyTokenizerFamily::Claude,
            proxy_encoding: OpenAiEncoding::Cl100kBase,
            multiplier: 1.10
        }
    );
    assert_eq!(
        route_model("gemini-3-pro-preview"),
        TokenizerTier::ExactTokie {
            key: ExactTokenizerKey::Gemma,
            local_precision: TokenEstimatePrecision::Proxy,
            local_multiplier: 1.08,
            fallback_family: ProxyTokenizerFamily::Gemini,
            fallback_encoding: OpenAiEncoding::O200kBase,
            fallback_multiplier: 1.08
        }
    );
    assert_eq!(
        route_model("google/gemma-4-31b-it"),
        TokenizerTier::ExactTokie {
            key: ExactTokenizerKey::Gemma,
            local_precision: TokenEstimatePrecision::Exact,
            local_multiplier: 1.0,
            fallback_family: ProxyTokenizerFamily::Gemini,
            fallback_encoding: OpenAiEncoding::O200kBase,
            fallback_multiplier: 1.08
        }
    );
    assert_eq!(
        route_model("unknown-lab/future-model"),
        TokenizerTier::BudgetProxy {
            family: ProxyTokenizerFamily::Unknown,
            proxy_encoding: OpenAiEncoding::O200kBase,
            multiplier: 1.15
        }
    );
}

/// Verifies that construction validates resources without parsing tokenizer vocabularies.
/// 验证构造阶段只校验资源存在而不解析 tokenizer 词表。
#[test]
fn constructs_lazily_and_warms_up_on_demand() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    copy_openai_resources(
        temp_dir.path(),
        &["cl100k_base.tiktoken", "o200k_base.tiktoken"],
    );

    let counter =
        TokenCounter::from_resources_dir(temp_dir.path()).expect("resources should validate");
    assert!(!counter.is_encoding_loaded(OpenAiEncoding::O200kBase));
    assert!(!counter.is_encoding_loaded(OpenAiEncoding::Cl100kBase));

    counter
        .preload_model_tokenizer("deepseek-ai/DeepSeek-V4-Pro")
        .expect("proxy tokenizer should preload");
    assert!(counter.is_encoding_loaded(OpenAiEncoding::O200kBase));
    assert!(!counter.is_encoding_loaded(OpenAiEncoding::Cl100kBase));

    counter.warm_up().expect("both tokenizers should warm up");
    assert!(counter.is_encoding_loaded(OpenAiEncoding::O200kBase));
    assert!(counter.is_encoding_loaded(OpenAiEncoding::Cl100kBase));
}

/// Verifies that the experimental `tokie` backend can load bundled `.tkz` resources lazily.
/// 验证实验性的 `tokie` 后端可以惰性加载内置 `.tkz` 资源。
#[test]
fn constructs_tokie_backend_lazily_and_counts() {
    let counter = TokenCounter::new_with_backend(TokenizerBackend::Tokie)
        .expect("bundled tokie resources should validate");
    assert_eq!(counter.backend(), TokenizerBackend::Tokie);
    assert!(!counter.is_encoding_loaded(OpenAiEncoding::O200kBase));

    let estimate = counter
        .estimate("gpt-5-codex", "Hello Vulcan. 你好，世界。")
        .expect("tokie estimate should succeed");
    assert!(estimate.tokens > 0);
    assert_eq!(estimate.precision, TokenEstimatePrecision::Experimental);
    assert!(counter.is_encoding_loaded(OpenAiEncoding::O200kBase));
}

/// Verifies that bundled exact `.tkz` assets are used before proxy fallback.
/// 验证内置精确 `.tkz` 资产会优先于代理回退使用。
#[test]
fn uses_exact_tkz_assets_for_supported_model_families() {
    let counter = TokenCounter::new().expect("bundled resources should validate");
    assert!(counter.external_tokenizer_count() >= 13);

    let estimate = counter
        .estimate("Qwen/Qwen3.7-Coder", "fn main() { println!(\"你好\"); }")
        .expect("qwen exact estimate should succeed");

    assert!(estimate.tokens > 0);
    assert_eq!(estimate.precision, TokenEstimatePrecision::Exact);
    assert_eq!(
        estimate.routed_tier,
        TokenizerTier::ExactTokie {
            key: ExactTokenizerKey::QwenCoder,
            local_precision: TokenEstimatePrecision::Exact,
            local_multiplier: 1.0,
            fallback_family: ProxyTokenizerFamily::EasternMixed,
            fallback_encoding: OpenAiEncoding::O200kBase,
            fallback_multiplier: 1.08
        }
    );
}

/// Verifies that both backends can count the same file content without changing routing semantics.
/// 验证两个后端都能统计同一个文件内容，且不会改变路由语义。
#[test]
fn compares_backends_on_same_file_content() {
    let text = include_str!("../src/lib.rs");
    let wordchipper = TokenCounter::new_with_backend(TokenizerBackend::Wordchipper)
        .expect("wordchipper resources should validate");
    let tokie = TokenCounter::new_with_backend(TokenizerBackend::Tokie)
        .expect("tokie resources should validate");

    let wordchipper_estimate = wordchipper
        .estimate("gpt-5-codex", text)
        .expect("wordchipper estimate should succeed");
    let tokie_estimate = tokie
        .estimate("gpt-5-codex", text)
        .expect("tokie estimate should succeed");

    assert!(wordchipper_estimate.tokens > 0);
    assert!(tokie_estimate.tokens > 0);
    assert_eq!(wordchipper_estimate.routed_tier, tokie_estimate.routed_tier);
    assert_eq!(
        wordchipper_estimate.precision,
        TokenEstimatePrecision::Exact
    );
    assert_eq!(
        tokie_estimate.precision,
        TokenEstimatePrecision::Experimental
    );
}

/// Verifies that estimates are returned with exact, experimental, or proxy precision metadata.
/// 验证估算会返回精确、实验或代理精度元数据。
#[test]
fn estimates_tokens_with_expected_precision_metadata() {
    let counter = TokenCounter::new().expect("bundled resources should validate");

    let openai = counter
        .estimate("gpt-5-codex", "hello world")
        .expect("OpenAI estimate should succeed");
    assert!(openai.tokens > 0);
    assert_eq!(openai.precision, TokenEstimatePrecision::Exact);

    let doubao = counter
        .estimate("doubao-seed-2.0-pro", "你好，世界")
        .expect("Doubao proxy estimate should succeed");
    assert!(doubao.tokens > 0);
    assert_eq!(doubao.precision, TokenEstimatePrecision::Proxy);
    assert_eq!(
        doubao.routed_tier,
        TokenizerTier::BudgetProxy {
            family: ProxyTokenizerFamily::EasternMixed,
            proxy_encoding: OpenAiEncoding::O200kBase,
            multiplier: 1.08
        }
    );

    let gemini = counter
        .estimate("gemini-3-pro-preview", "hello world")
        .expect("Gemini adapter estimate should succeed");
    assert!(gemini.tokens > 0);
    assert_eq!(gemini.precision, TokenEstimatePrecision::Proxy);
}

/// Verifies that missing OpenAI resources are reported before any counting work.
/// 验证缺失 OpenAI 资源会在任何统计工作前被报告。
#[test]
fn rejects_missing_openai_resources() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let error = TokenCounter::from_resources_dir(temp_dir.path())
        .expect_err("missing resources should fail construction");
    assert!(
        error
            .to_string()
            .contains("missing OpenAI tokenizer resource")
    );
}

/// Copies bundled OpenAI resources into one temporary test asset directory.
/// 将内置 OpenAI 资源复制到一个临时测试资产目录。
fn copy_openai_resources(asset_root: &Path, file_names: &[&str]) {
    let source_root = default_openai_resource_dir();
    let destination_root = asset_root.join("openai");
    fs::create_dir_all(&destination_root).expect("openai destination should be created");

    for file_name in file_names {
        fs::copy(
            source_root.join(file_name),
            destination_root.join(file_name),
        )
        .expect("OpenAI resource should be copied");
    }
}
