//! Code-owned provider tool targets and exact model-native schemas.
//! 代码持有的供应商工具目标与精确模型原生 Schema。

use crate::{
    AuthenticationProjection, ProviderToolAdapterKind, ProviderToolCapabilityKind,
    ProviderToolContract, ProviderToolExecutionContract, ProviderToolOutputContract,
    ProviderToolOutputKind, ProviderToolTarget,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// Returns every verified tool target owned by one built-in provider.
/// 返回一个内置供应商持有的全部已验证工具目标。
///
/// # Parameters
/// 参数
/// `provider_id` is the exact built-in provider identifier.
/// `provider_id` 是精确的内置供应商标识。
///
/// # Returns
/// 返回值
/// Model-specific targets whose schemas remain independent from one another.
/// 返回 Schema 彼此独立的模型专属目标。
pub fn built_in_provider_tool_targets(provider_id: &str) -> Vec<ProviderToolTarget> {
    match provider_id {
        "alibaba" => alibaba_tool_targets(),
        "minimax" => minimax_tool_targets(),
        "tavily" => tavily_tool_targets(),
        _ => Vec::new(),
    }
}

/// Builds every reviewed Alibaba native tool target without merging product contracts.
/// 构建全部已审核阿里云原生工具目标且不合并产品契约。
///
/// # Returns
/// 返回值
/// Independently selectable product, region, model, and operation targets.
/// 返回可独立选择的产品、区域、模型与操作目标。
fn alibaba_tool_targets() -> Vec<ProviderToolTarget> {
    let mut targets = Vec::new();
    for service_entry_id in ["model-studio-cn", "model-studio-singapore"] {
        targets.push(alibaba_web_search_target(service_entry_id));
        targets.push(alibaba_qwen_image_target(
            service_entry_id,
            "api-key",
            "qwen-image-2.0-pro",
            false,
        ));
        targets.push(alibaba_qwen_image_target(
            service_entry_id,
            "api-key",
            "qwen-image-2.0-pro",
            true,
        ));
        let cosy_voice_models: &[&str] = if service_entry_id == "model-studio-cn" {
            &[
                "cosyvoice-v2",
                "cosyvoice-v3-flash",
                "cosyvoice-v3-plus",
                "cosyvoice-v3.5-flash",
                "cosyvoice-v3.5-plus",
            ]
        } else {
            &["cosyvoice-v3-flash", "cosyvoice-v3-plus"]
        };
        for model_id in cosy_voice_models {
            targets.push(alibaba_cosyvoice_target(service_entry_id, model_id));
        }
        for model_id in ["qwen3-tts-flash", "qwen3-tts-instruct-flash"] {
            targets.push(alibaba_qwen_tts_target(service_entry_id, model_id));
        }
        targets.push(alibaba_qwen_asr_target(service_entry_id, "qwen3-asr-flash"));
        for model_id in ["qwen3.5-omni-plus", "qwen3.5-omni-flash"] {
            targets.push(alibaba_qwen_omni_media_target(service_entry_id, model_id));
        }
        if service_entry_id == "model-studio-cn" {
            targets.push(alibaba_fun_asr_target(service_entry_id));
            for model_id in ["wan2.7-t2v", "wan2.7-i2v"] {
                targets.push(alibaba_wan_video_target(service_entry_id, model_id));
            }
            for model_id in ["fun-music-v1", "fun-music-preview"] {
                targets.push(alibaba_fun_music_target(service_entry_id, model_id));
            }
            for model_id in [
                "happyhorse-1.1-t2v",
                "happyhorse-1.1-i2v",
                "happyhorse-1.1-r2v",
            ] {
                targets.push(alibaba_happyhorse_video_target(
                    service_entry_id,
                    "api-key",
                    model_id,
                    false,
                ));
            }
            targets.push(alibaba_happyhorse_video_target(
                service_entry_id,
                "api-key",
                "happyhorse-1.0-video-edit",
                true,
            ));
        }
    }
    for service_entry_id in ["token-plan-team-cn", "token-plan-team-global"] {
        for model_id in ["qwen-image-2.0", "qwen-image-2.0-pro"] {
            targets.push(alibaba_qwen_image_target(
                service_entry_id,
                "token-plan-api-key",
                model_id,
                false,
            ));
        }
    }
    for service_entry_id in [
        "token-plan-personal-cn",
        "token-plan-team-cn",
        "token-plan-team-global",
    ] {
        for model_id in ["wan2.7-image", "wan2.7-image-pro"] {
            targets.push(alibaba_wan_image_target(service_entry_id, model_id));
        }
    }
    for service_entry_id in ["token-plan-personal-cn", "token-plan-team-cn"] {
        for model_id in ["happyhorse-1.1-t2v", "happyhorse-1.1-i2v"] {
            targets.push(alibaba_happyhorse_video_target(
                service_entry_id,
                "token-plan-api-key",
                model_id,
                false,
            ));
        }
    }
    targets.push(alibaba_happyhorse_video_target(
        "token-plan-personal-cn",
        "token-plan-api-key",
        "happyhorse-1.1-r2v",
        false,
    ));
    targets
}

/// Returns the exact native API origin for one reviewed Alibaba service entry.
/// 返回一个已审核阿里云服务入口的精确原生 API 源地址。
///
/// # Parameters
/// 参数
/// `service_entry_id` is one closed built-in regional service identifier.
/// `service_entry_id` 是一个封闭内置区域服务标识。
///
/// # Returns
/// 返回值
/// The code-owned origin used by native Model Studio actions.
/// 返回原生 Model Studio 动作使用的代码持有源地址。
fn alibaba_service_origin(service_entry_id: &str) -> &'static str {
    match service_entry_id {
        "model-studio-cn" => "https://dashscope.aliyuncs.com",
        "model-studio-singapore" => "https://dashscope-intl.aliyuncs.com",
        "token-plan-personal-cn" | "token-plan-team-cn" => {
            "https://token-plan.cn-beijing.maas.aliyuncs.com"
        }
        "token-plan-team-global" => "https://token-plan.ap-southeast-1.maas.aliyuncs.com",
        _ => unreachable!("Alibaba tool targets only use reviewed service entries"),
    }
}

/// Builds one exact Alibaba Model Studio MCP WebSearch target.
/// 构建一个精确的阿里云百炼 Model Studio MCP WebSearch 目标。
///
/// # Parameters
/// 参数
/// `service_entry_id` fixes the exact regional Model Studio origin.
/// `service_entry_id` 固定精确的区域 Model Studio 服务入口。
///
/// # Returns
/// 返回值
/// A query-and-count schema backed by the reviewed streamable HTTP MCP flow.
/// 返回由已审核的 Streamable HTTP MCP 流程支持的查询与数量 Schema。
fn alibaba_web_search_target(service_entry_id: &str) -> ProviderToolTarget {
    ProviderToolTarget {
        target_id: format!("{service_entry_id}.web-search"),
        provider_id: String::from("alibaba"),
        service_entry_id: service_entry_id.to_string(),
        credential_profile_ids: BTreeSet::from([String::from("api-key")]),
        upstream_target_id: String::from("bailian_web_search"),
        display_name: String::from("Alibaba Model Studio WebSearch"),
        contract: ProviderToolContract {
            tool_id: String::from("vulcan_web_search"),
            description: String::from(
                "Search the public web with the selected Alibaba Model Studio account and return ordered structured results.",
            ),
            capability: ProviderToolCapabilityKind::WebSearch,
            input_schema: json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "query": {
                        "type": "string",
                        "minLength": 1,
                        "description": "The exact web search query."
                    },
                    "max_results": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 20,
                        "description": "Optional maximum number of ordered results."
                    }
                },
                "required": ["query"]
            }),
            output: ProviderToolOutputContract {
                kind: ProviderToolOutputKind::Search,
                source_path: String::from("result.content[].text.pages"),
                normalized_result_path: String::from("search"),
                resource_kind: None,
                mime_types: Vec::new(),
            },
        },
        execution: ProviderToolExecutionContract {
            endpoint_base_url: Some(alibaba_service_origin(service_entry_id).to_string()),
            endpoint_path: String::from("/api/v1/mcps/WebSearch/mcp"),
            authentication: AuthenticationProjection::BearerAuthorization,
            adapter: ProviderToolAdapterKind::AlibabaModelStudioSearch,
        },
    }
}

/// Builds one exact synchronous Qwen image generation or editing target.
/// 构建一个精确同步 Qwen 图片生成或编辑目标。
///
/// # Parameters
/// 参数
/// `service_entry_id`, `credential_profile_id`, and `model_id` freeze the product route while
/// `editing` selects the distinct native schema.
/// `service_entry_id`、`credential_profile_id` 与 `model_id` 冻结产品路由，
/// `editing` 选择不同原生 Schema。
///
/// # Returns
/// 返回值
/// A model-specific target backed by the reviewed MultiModalConversation endpoint.
/// 返回由已审核 MultiModalConversation 端点支持的模型专属目标。
fn alibaba_qwen_image_target(
    service_entry_id: &str,
    credential_profile_id: &str,
    model_id: &str,
    editing: bool,
) -> ProviderToolTarget {
    let (operation_id, display_suffix, capability, tool_id, description, input_schema, adapter) =
        if editing {
            (
                "edit",
                "Edit",
                ProviderToolCapabilityKind::ImageEdit,
                "vulcan_image_edit",
                "Edit one to three local images with the selected Qwen Image model and return generated PNG resources.",
                json!({
                    "$schema": "https://json-schema.org/draft/2020-12/schema",
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "instruction": {
                            "type": "string",
                            "minLength": 1,
                            "description": "The exact image editing instruction."
                        },
                        "source_paths": {
                            "type": "array",
                            "minItems": 1,
                            "maxItems": 3,
                            "items": {"type": "string", "minLength": 1},
                            "description": "One to three local image paths in provider order."
                        },
                        "negative_prompt": {
                            "type": "string",
                            "maxLength": 500,
                            "description": "Optional excluded visual content."
                        },
                        "count": {
                            "type": "integer",
                            "minimum": 1,
                            "maximum": 6,
                            "default": 1
                        },
                        "seed": {
                            "type": "integer",
                            "minimum": 0,
                            "maximum": 2147483647
                        },
                        "prompt_extend": {"type": "boolean"},
                        "watermark": {"type": "boolean"}
                    },
                    "required": ["instruction", "source_paths"]
                }),
                ProviderToolAdapterKind::AlibabaQwenImageEdit,
            )
        } else {
            (
                "generate",
                "Generate",
                ProviderToolCapabilityKind::ImageGenerate,
                "vulcan_image_generate",
                "Generate PNG images with the selected Qwen Image model using its exact native controls.",
                json!({
                    "$schema": "https://json-schema.org/draft/2020-12/schema",
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "prompt": {
                            "type": "string",
                            "minLength": 1,
                            "description": "The exact image generation prompt."
                        },
                        "negative_prompt": {
                            "type": "string",
                            "maxLength": 500,
                            "description": "Optional excluded visual content."
                        },
                        "count": {
                            "type": "integer",
                            "minimum": 1,
                            "maximum": 6,
                            "default": 1
                        },
                        "width": {"type": "integer", "minimum": 1},
                        "height": {"type": "integer", "minimum": 1},
                        "seed": {
                            "type": "integer",
                            "minimum": 0,
                            "maximum": 2147483647
                        },
                        "prompt_extend": {"type": "boolean"},
                        "watermark": {"type": "boolean"}
                    },
                    "dependentRequired": {
                        "width": ["height"],
                        "height": ["width"]
                    },
                    "required": ["prompt"]
                }),
                ProviderToolAdapterKind::AlibabaQwenImageGenerate,
            )
        };
    ProviderToolTarget {
        target_id: format!("{service_entry_id}.{model_id}.image-{operation_id}"),
        provider_id: String::from("alibaba"),
        service_entry_id: service_entry_id.to_string(),
        credential_profile_ids: BTreeSet::from([credential_profile_id.to_string()]),
        upstream_target_id: model_id.to_string(),
        display_name: format!("{model_id} {display_suffix}"),
        contract: ProviderToolContract {
            tool_id: String::from(tool_id),
            description: String::from(description),
            capability,
            input_schema,
            output: ProviderToolOutputContract {
                kind: ProviderToolOutputKind::Resources,
                source_path: String::from("output.choices[].message.content[].image"),
                normalized_result_path: String::from("resources"),
                resource_kind: Some(String::from("image")),
                mime_types: vec![String::from("image/png")],
            },
        },
        execution: ProviderToolExecutionContract {
            endpoint_base_url: Some(alibaba_service_origin(service_entry_id).to_string()),
            endpoint_path: String::from("/api/v1/services/aigc/multimodal-generation/generation"),
            authentication: AuthenticationProjection::BearerAuthorization,
            adapter,
        },
    }
}

/// Builds one exact synchronous Wan 2.7 image generation target.
/// 构建一个精确同步 Wan 2.7 图片生成目标。
///
/// # Parameters
/// 参数
/// `service_entry_id` fixes the Token Plan product and `model_id` selects the documented
/// standard or Pro resolution contract.
/// `service_entry_id` 固定 Token Plan 产品，`model_id` 选择文档明确的标准或 Pro 分辨率合同。
///
/// # Returns
/// 返回值
/// A reference-aware generation target with one exact model schema.
/// 返回具有精确模型 Schema 且支持参考图的生成目标。
fn alibaba_wan_image_target(service_entry_id: &str, model_id: &str) -> ProviderToolTarget {
    let resolutions = if model_id == "wan2.7-image-pro" {
        json!(["1k", "2k", "4k"])
    } else {
        json!(["1k", "2k"])
    };
    ProviderToolTarget {
        target_id: format!("{service_entry_id}.{model_id}.image-generate"),
        provider_id: String::from("alibaba"),
        service_entry_id: service_entry_id.to_string(),
        credential_profile_ids: BTreeSet::from([String::from("token-plan-api-key")]),
        upstream_target_id: model_id.to_string(),
        display_name: format!("{model_id} Generate"),
        contract: ProviderToolContract {
            tool_id: String::from("vulcan_image_generate"),
            description: String::from(
                "Generate PNG images with the selected Wan 2.7 Token Plan model and optional ordered local reference images.",
            ),
            capability: ProviderToolCapabilityKind::ImageGenerate,
            input_schema: json!({
                "$schema": "https://json-schema.org/draft/2020-12/schema",
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "prompt": {
                        "type": "string",
                        "minLength": 1,
                        "maxLength": 5000,
                        "description": "The exact image generation prompt."
                    },
                    "reference_paths": {
                        "type": "array",
                        "maxItems": 9,
                        "items": {"type": "string", "minLength": 1},
                        "description": "Optional local reference image paths in provider order."
                    },
                    "negative_prompt": {"type": "string"},
                    "resolution": {
                        "type": "string",
                        "enum": resolutions,
                        "default": "2k"
                    },
                    "count": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 4,
                        "default": 1
                    },
                    "seed": {
                        "type": "integer",
                        "minimum": 0,
                        "maximum": 2147483647
                    },
                    "prompt_extend": {"type": "boolean"},
                    "watermark": {"type": "boolean"}
                },
                "required": ["prompt"]
            }),
            output: ProviderToolOutputContract {
                kind: ProviderToolOutputKind::Resources,
                source_path: String::from("output.choices[].message.content[].image"),
                normalized_result_path: String::from("resources"),
                resource_kind: Some(String::from("image")),
                mime_types: vec![String::from("image/png")],
            },
        },
        execution: ProviderToolExecutionContract {
            endpoint_base_url: Some(alibaba_service_origin(service_entry_id).to_string()),
            endpoint_path: String::from("/api/v1/services/aigc/multimodal-generation/generation"),
            authentication: AuthenticationProjection::BearerAuthorization,
            adapter: ProviderToolAdapterKind::AlibabaWanImageGenerate,
        },
    }
}

/// Builds one exact synchronous CosyVoice synthesis target.
/// 构建一个精确同步 CosyVoice 合成目标。
///
/// # Parameters
/// 参数
/// `service_entry_id` fixes the region and `model_id` fixes the reviewed voice model.
/// `service_entry_id` 固定区域，`model_id` 固定已审核语音模型。
///
/// # Returns
/// 返回值
/// A model-native synthesis schema with the complete verified control set.
/// 返回包含完整已验证控制集的模型原生合成 Schema。
fn alibaba_cosyvoice_target(service_entry_id: &str, model_id: &str) -> ProviderToolTarget {
    ProviderToolTarget {
        target_id: format!("{service_entry_id}.{model_id}.speech-synthesize"),
        provider_id: String::from("alibaba"),
        service_entry_id: service_entry_id.to_string(),
        credential_profile_ids: BTreeSet::from([String::from("api-key")]),
        upstream_target_id: model_id.to_string(),
        display_name: format!("{model_id} Speech"),
        contract: ProviderToolContract {
            tool_id: String::from("vulcan_speech_synthesize"),
            description: String::from(
                "Synthesize speech with the selected CosyVoice model using its exact native voice and audio controls.",
            ),
            capability: ProviderToolCapabilityKind::SpeechSynthesize,
            input_schema: json!({
                "$schema": "https://json-schema.org/draft/2020-12/schema",
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "text": {"type": "string", "minLength": 1},
                    "voice_id": {"type": "string", "minLength": 1},
                    "output_format": {
                        "type": "string",
                        "enum": ["mp3", "pcm", "wav", "opus"],
                        "default": "mp3"
                    },
                    "sample_rate": {"type": "integer", "minimum": 1},
                    "volume": {"type": "number", "minimum": 0, "maximum": 100},
                    "speed": {"type": "number", "minimum": 0.5, "maximum": 2.0},
                    "pitch": {"type": "number", "minimum": 0.5, "maximum": 2.0},
                    "seed": {"type": "integer", "minimum": 0, "maximum": 65535},
                    "language": {"type": "string", "minLength": 1},
                    "style": {"type": "string"},
                    "enable_ssml": {"type": "boolean"}
                },
                "required": ["text", "voice_id"]
            }),
            output: ProviderToolOutputContract {
                kind: ProviderToolOutputKind::Resources,
                source_path: String::from("output.audio.url"),
                normalized_result_path: String::from("resources"),
                resource_kind: Some(String::from("audio")),
                mime_types: vec![
                    String::from("audio/mpeg"),
                    String::from("audio/pcm"),
                    String::from("audio/wav"),
                    String::from("audio/opus"),
                ],
            },
        },
        execution: ProviderToolExecutionContract {
            endpoint_base_url: Some(alibaba_service_origin(service_entry_id).to_string()),
            endpoint_path: String::from("/api/v1/services/audio/tts/SpeechSynthesizer"),
            authentication: AuthenticationProjection::BearerAuthorization,
            adapter: ProviderToolAdapterKind::AlibabaCosyVoiceSynthesize,
        },
    }
}

/// Builds one exact synchronous Qwen3-TTS target.
/// 构建一个精确同步 Qwen3-TTS 目标。
///
/// # Parameters
/// 参数
/// `service_entry_id` fixes the region and `model_id` selects plain or instruct synthesis.
/// `service_entry_id` 固定区域，`model_id` 选择普通或指令式合成。
///
/// # Returns
/// 返回值
/// A WAV-only model schema with model-specific instruction availability.
/// 返回仅输出 WAV 且具有模型专属指令可用性的 Schema。
fn alibaba_qwen_tts_target(service_entry_id: &str, model_id: &str) -> ProviderToolTarget {
    let mut properties = serde_json::Map::from_iter([
        (
            String::from("text"),
            json!({"type": "string", "minLength": 1, "maxLength": 600}),
        ),
        (
            String::from("voice_id"),
            json!({"type": "string", "minLength": 1}),
        ),
        (
            String::from("language"),
            json!({
                "type": "string",
                "enum": [
                    "Auto", "Chinese", "English", "German", "Italian", "Portuguese",
                    "Spanish", "Japanese", "Korean", "French", "Russian"
                ]
            }),
        ),
    ]);
    if model_id == "qwen3-tts-instruct-flash" {
        properties.insert(
            String::from("style"),
            json!({
                "type": "string",
                "minLength": 1,
                "description": "Natural-language delivery instruction for the instruct model."
            }),
        );
    }
    ProviderToolTarget {
        target_id: format!("{service_entry_id}.{model_id}.speech-synthesize"),
        provider_id: String::from("alibaba"),
        service_entry_id: service_entry_id.to_string(),
        credential_profile_ids: BTreeSet::from([String::from("api-key")]),
        upstream_target_id: model_id.to_string(),
        display_name: format!("{model_id} Speech"),
        contract: ProviderToolContract {
            tool_id: String::from("vulcan_speech_synthesize"),
            description: String::from(
                "Synthesize a WAV resource with the selected Qwen3-TTS model and its exact preset-voice contract.",
            ),
            capability: ProviderToolCapabilityKind::SpeechSynthesize,
            input_schema: Value::Object(serde_json::Map::from_iter([
                (
                    String::from("$schema"),
                    Value::String(String::from("https://json-schema.org/draft/2020-12/schema")),
                ),
                (String::from("type"), Value::String(String::from("object"))),
                (String::from("additionalProperties"), Value::Bool(false)),
                (String::from("properties"), Value::Object(properties)),
                (String::from("required"), json!(["text", "voice_id"])),
            ])),
            output: ProviderToolOutputContract {
                kind: ProviderToolOutputKind::Resources,
                source_path: String::from("output.audio.url"),
                normalized_result_path: String::from("resources"),
                resource_kind: Some(String::from("audio")),
                mime_types: vec![String::from("audio/wav")],
            },
        },
        execution: ProviderToolExecutionContract {
            endpoint_base_url: Some(alibaba_service_origin(service_entry_id).to_string()),
            endpoint_path: String::from("/api/v1/services/aigc/multimodal-generation/generation"),
            authentication: AuthenticationProjection::BearerAuthorization,
            adapter: ProviderToolAdapterKind::AlibabaQwenSpeechSynthesize,
        },
    }
}

/// Builds one exact synchronous Qwen3-ASR target.
/// 构建一个精确同步 Qwen3-ASR 目标。
///
/// # Parameters
/// 参数
/// `service_entry_id` fixes the region and `model_id` fixes the reviewed ASR model.
/// `service_entry_id` 固定区域，`model_id` 固定已审核 ASR 模型。
///
/// # Returns
/// 返回值
/// A local-audio-path schema normalized to transcript text.
/// 返回归一化为转录文本的本地音频路径 Schema。
fn alibaba_qwen_asr_target(service_entry_id: &str, model_id: &str) -> ProviderToolTarget {
    ProviderToolTarget {
        target_id: format!("{service_entry_id}.{model_id}.speech-transcribe"),
        provider_id: String::from("alibaba"),
        service_entry_id: service_entry_id.to_string(),
        credential_profile_ids: BTreeSet::from([String::from("api-key")]),
        upstream_target_id: model_id.to_string(),
        display_name: format!("{model_id} Transcribe"),
        contract: ProviderToolContract {
            tool_id: String::from("vulcan_speech_transcribe"),
            description: String::from(
                "Transcribe one local audio file with the selected synchronous Qwen3-ASR model.",
            ),
            capability: ProviderToolCapabilityKind::SpeechTranscribe,
            input_schema: json!({
                "$schema": "https://json-schema.org/draft/2020-12/schema",
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "audio_path": {
                        "type": "string",
                        "minLength": 1,
                        "description": "Local audio file path."
                    },
                    "language": {
                        "type": "string",
                        "enum": [
                            "zh", "yue", "en", "ja", "de", "ko", "ru", "fr", "pt",
                            "ar", "it", "es", "hi", "id", "th", "tr", "uk", "vi",
                            "cs", "da", "fil", "fi", "is", "ms", "no", "pl", "sv"
                        ]
                    }
                },
                "required": ["audio_path"]
            }),
            output: ProviderToolOutputContract {
                kind: ProviderToolOutputKind::Text,
                source_path: String::from("output.choices[0].message.content[0].text"),
                normalized_result_path: String::from("text"),
                resource_kind: None,
                mime_types: Vec::new(),
            },
        },
        execution: ProviderToolExecutionContract {
            endpoint_base_url: Some(alibaba_service_origin(service_entry_id).to_string()),
            endpoint_path: String::from("/api/v1/services/aigc/multimodal-generation/generation"),
            authentication: AuthenticationProjection::BearerAuthorization,
            adapter: ProviderToolAdapterKind::AlibabaQwenSpeechTranscribe,
        },
    }
}

/// Builds one exact Qwen3.5 Omni media-analysis target.
/// 构建一个精确的 Qwen3.5 Omni 媒体分析目标。
///
/// # Parameters
/// 参数
/// `service_entry_id` fixes the regional Model Studio origin and `model_id` selects the
/// reviewed non-realtime Omni model.
/// `service_entry_id` 固定区域 Model Studio 源地址，`model_id` 选择已审核的非实时 Omni 模型。
///
/// # Returns
/// 返回值
/// A closed local-media schema with Router-owned analysis task semantics.
/// 返回具有 Router 持有分析任务语义的封闭本地媒体 Schema。
fn alibaba_qwen_omni_media_target(service_entry_id: &str, model_id: &str) -> ProviderToolTarget {
    ProviderToolTarget {
        target_id: format!("{service_entry_id}.{model_id}.media-analyze"),
        provider_id: String::from("alibaba"),
        service_entry_id: service_entry_id.to_string(),
        credential_profile_ids: BTreeSet::from([String::from("api-key")]),
        upstream_target_id: model_id.to_string(),
        display_name: format!("{model_id} Media Analysis"),
        contract: ProviderToolContract {
            tool_id: String::from("vulcan_media_analyze"),
            description: String::from(
                "Analyze ordered local image or audio files with the selected Qwen3.5 Omni model. Use a task-specific instruction only for question_answer or extract.",
            ),
            capability: ProviderToolCapabilityKind::MediaAnalyze,
            input_schema: json!({
                "$schema": "https://json-schema.org/draft/2020-12/schema",
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "task": {
                        "type": "string",
                        "enum": ["describe", "summarize", "question_answer", "extract"]
                    },
                    "media_paths": {
                        "type": "array",
                        "minItems": 1,
                        "maxItems": 16,
                        "items": {
                            "type": "string",
                            "minLength": 1,
                            "description": "Local image or audio file path."
                        }
                    },
                    "instruction": {
                        "type": "string",
                        "minLength": 1
                    }
                },
                "required": ["task", "media_paths"],
                "allOf": [{
                    "if": {
                        "properties": {
                            "task": {"enum": ["question_answer", "extract"]}
                        },
                        "required": ["task"]
                    },
                    "then": {"required": ["instruction"]}
                }]
            }),
            output: ProviderToolOutputContract {
                kind: ProviderToolOutputKind::Text,
                source_path: String::from("choices[].delta.content"),
                normalized_result_path: String::from("text"),
                resource_kind: None,
                mime_types: Vec::new(),
            },
        },
        execution: ProviderToolExecutionContract {
            endpoint_base_url: Some(alibaba_service_origin(service_entry_id).to_string()),
            endpoint_path: String::from("/compatible-mode/v1/chat/completions"),
            authentication: AuthenticationProjection::BearerAuthorization,
            adapter: ProviderToolAdapterKind::AlibabaQwenOmniMediaAnalyze,
        },
    }
}

/// Builds the exact asynchronous Fun-ASR transcription target.
/// 构建精确的异步 Fun-ASR 转录目标。
///
/// # Parameters
/// 参数
/// `service_entry_id` is the verified Model Studio China entry.
/// `service_entry_id` 是已验证的 Model Studio 中国站入口。
///
/// # Returns
/// 返回值
/// A bounded local audio/video batch schema normalized to ordered transcript text.
/// 返回归一化为有序转录文本的有界本地音视频批次 Schema。
fn alibaba_fun_asr_target(service_entry_id: &str) -> ProviderToolTarget {
    ProviderToolTarget {
        target_id: format!("{service_entry_id}.fun-asr.speech-transcribe"),
        provider_id: String::from("alibaba"),
        service_entry_id: service_entry_id.to_string(),
        credential_profile_ids: BTreeSet::from([String::from("api-key")]),
        upstream_target_id: String::from("fun-asr"),
        display_name: String::from("Fun-ASR"),
        contract: ProviderToolContract {
            tool_id: String::from("vulcan_speech_transcribe"),
            description: String::from(
                "Transcribe one through one hundred local audio or video files with Fun-ASR and wait for provider completion.",
            ),
            capability: ProviderToolCapabilityKind::SpeechTranscribe,
            input_schema: json!({
                "$schema": "https://json-schema.org/draft/2020-12/schema",
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "source_paths": {
                        "type": "array",
                        "minItems": 1,
                        "maxItems": 100,
                        "items": {
                            "type": "string",
                            "minLength": 1,
                            "description": "Local audio or video file path."
                        }
                    },
                    "language": {
                        "type": "string",
                        "enum": [
                            "zh", "en", "ja", "ko", "vi", "th", "id", "ms", "tl",
                            "hi", "ar", "fr", "de", "es", "pt", "ru", "it", "nl",
                            "sv", "da", "fi", "no", "el", "pl", "cs", "hu", "ro",
                            "bg", "hr", "sk"
                        ]
                    },
                    "channel_ids": {
                        "type": "array",
                        "minItems": 1,
                        "uniqueItems": true,
                        "items": {"type": "integer", "minimum": 0}
                    },
                    "diarization": {"type": "boolean"},
                    "speaker_count": {"type": "integer", "minimum": 1},
                    "vocabulary_id": {"type": "string", "minLength": 1}
                },
                "required": ["source_paths"],
                "dependentSchemas": {
                    "speaker_count": {
                        "properties": {"diarization": {"const": true}},
                        "required": ["diarization"]
                    }
                }
            }),
            output: ProviderToolOutputContract {
                kind: ProviderToolOutputKind::Text,
                source_path: String::from("output.results[].transcription_url"),
                normalized_result_path: String::from("text"),
                resource_kind: None,
                mime_types: Vec::new(),
            },
        },
        execution: ProviderToolExecutionContract {
            endpoint_base_url: Some(alibaba_service_origin(service_entry_id).to_string()),
            endpoint_path: String::from("/api/v1/services/audio/asr/transcription"),
            authentication: AuthenticationProjection::BearerAuthorization,
            adapter: ProviderToolAdapterKind::AlibabaFunAsrTranscribe,
        },
    }
}

/// Builds one exact asynchronous HappyHorse generation or edit target.
/// 构建一个精确的异步 HappyHorse 生成或编辑目标。
///
/// # Parameters
/// 参数
/// Product, credential profile, model, and operation are frozen independently.
/// 产品、凭据配置档、模型与操作被独立冻结。
///
/// # Returns
/// 返回值
/// A model-specific local-resource schema with no cross-model input union.
/// 返回不含跨模型输入联合的模型专属本地资源 Schema。
fn alibaba_happyhorse_video_target(
    service_entry_id: &str,
    credential_profile_id: &str,
    model_id: &str,
    editing: bool,
) -> ProviderToolTarget {
    let (tool_id, capability, description, input_schema, adapter) = if editing {
        (
            "video_edit",
            ProviderToolCapabilityKind::VideoEdit,
            "Edit one local MP4 or MOV video with optional local image references and wait for provider completion.",
            alibaba_happyhorse_edit_schema(),
            ProviderToolAdapterKind::AlibabaHappyHorseVideoEdit,
        )
    } else {
        (
            "video_generate",
            ProviderToolCapabilityKind::VideoGenerate,
            "Generate one MP4 video with the selected HappyHorse model and wait for provider completion.",
            alibaba_happyhorse_generate_schema(model_id),
            ProviderToolAdapterKind::AlibabaHappyHorseVideoGenerate,
        )
    };
    ProviderToolTarget {
        target_id: format!("{service_entry_id}.{model_id}"),
        provider_id: String::from("alibaba"),
        service_entry_id: service_entry_id.to_string(),
        credential_profile_ids: BTreeSet::from([credential_profile_id.to_string()]),
        upstream_target_id: model_id.to_string(),
        display_name: match model_id {
            "happyhorse-1.1-t2v" => String::from("HappyHorse 1.1 Text to Video"),
            "happyhorse-1.1-i2v" => String::from("HappyHorse 1.1 Image to Video"),
            "happyhorse-1.1-r2v" => String::from("HappyHorse 1.1 Reference to Video"),
            "happyhorse-1.0-video-edit" => String::from("HappyHorse 1.0 Video Edit"),
            _ => unreachable!("closed HappyHorse target registry"),
        },
        contract: ProviderToolContract {
            tool_id: String::from(tool_id),
            description: String::from(description),
            capability,
            input_schema,
            output: ProviderToolOutputContract {
                kind: ProviderToolOutputKind::Resources,
                source_path: String::from("output.video_url|output.results[0].url"),
                normalized_result_path: String::from("resources"),
                resource_kind: Some(String::from("video")),
                mime_types: vec![String::from("video/mp4")],
            },
        },
        execution: ProviderToolExecutionContract {
            endpoint_base_url: Some(alibaba_service_origin(service_entry_id).to_string()),
            endpoint_path: String::from("/api/v1/services/aigc/video-generation/video-synthesis"),
            authentication: AuthenticationProjection::BearerAuthorization,
            adapter,
        },
    }
}

/// Builds the common HappyHorse parameter properties shared by generation and editing.
/// 构建生成与编辑共享的 HappyHorse 通用参数属性。
///
/// # Returns
/// 返回值
/// Exact JSON Schema property definitions copied from the verified wire contract.
/// 返回从已验证 Wire 合同复制的精确 JSON Schema 属性定义。
fn alibaba_happyhorse_common_properties() -> serde_json::Map<String, Value> {
    serde_json::Map::from_iter([
        (String::from("negative_prompt"), json!({"type": "string"})),
        (
            String::from("resolution"),
            json!({"type": "string", "enum": ["720P", "1080P"]}),
        ),
        (
            String::from("aspect_ratio"),
            json!({"type": "string", "enum": ["16:9", "9:16", "1:1", "4:3", "3:4"]}),
        ),
        (
            String::from("seed"),
            json!({"type": "integer", "minimum": 0}),
        ),
        (String::from("prompt_extend"), json!({"type": "boolean"})),
        (String::from("watermark"), json!({"type": "boolean"})),
    ])
}

/// Builds one model-specific HappyHorse generation schema.
/// 构建一个模型专属 HappyHorse 生成 Schema。
///
/// # Parameters
/// 参数
/// `model_id` selects text-only, first-frame, or reference-media input.
/// `model_id` 选择纯文本、首帧或参考媒体输入。
///
/// # Returns
/// 返回值
/// A closed JSON Schema for only that model.
/// 返回仅适用于该模型的封闭 JSON Schema。
fn alibaba_happyhorse_generate_schema(model_id: &str) -> Value {
    let mut properties = alibaba_happyhorse_common_properties();
    properties.insert(
        String::from("prompt"),
        json!({"type": "string", "minLength": 1}),
    );
    properties.insert(
        String::from("duration_seconds"),
        json!({"type": "integer", "minimum": 1}),
    );
    let mut required = vec![Value::String(String::from("prompt"))];
    match model_id {
        "happyhorse-1.1-t2v" => {}
        "happyhorse-1.1-i2v" => {
            properties.insert(
                String::from("first_frame_path"),
                json!({"type": "string", "minLength": 1, "description": "Local JPEG, PNG, or WebP first-frame path."}),
            );
            required.push(Value::String(String::from("first_frame_path")));
        }
        "happyhorse-1.1-r2v" => {
            properties.insert(
                String::from("references"),
                json!({
                    "type": "array",
                    "minItems": 1,
                    "maxItems": 9,
                    "items": {
                        "type": "object",
                        "additionalProperties": false,
                        "properties": {
                            "path": {"type": "string", "minLength": 1},
                            "kind": {"type": "string", "enum": ["image", "video"]},
                            "voice_path": {"type": "string", "minLength": 1}
                        },
                        "required": ["path", "kind"]
                    }
                }),
            );
            required.push(Value::String(String::from("references")));
        }
        _ => unreachable!("closed HappyHorse generation model registry"),
    }
    Value::Object(serde_json::Map::from_iter([
        (
            String::from("$schema"),
            Value::String(String::from("https://json-schema.org/draft/2020-12/schema")),
        ),
        (String::from("type"), Value::String(String::from("object"))),
        (String::from("additionalProperties"), Value::Bool(false)),
        (String::from("properties"), Value::Object(properties)),
        (String::from("required"), Value::Array(required)),
    ]))
}

/// Builds the exact HappyHorse video-edit schema.
/// 构建精确的 HappyHorse 视频编辑 Schema。
///
/// # Returns
/// 返回值
/// A local source-video schema with up to four image references.
/// 返回包含本地来源视频及最多四张参考图片的 Schema。
fn alibaba_happyhorse_edit_schema() -> Value {
    let mut properties = alibaba_happyhorse_common_properties();
    properties.insert(
        String::from("source_video_path"),
        json!({"type": "string", "minLength": 1}),
    );
    properties.insert(String::from("instruction"), json!({"type": "string"}));
    properties.insert(
        String::from("duration_seconds"),
        json!({"type": "integer", "minimum": 2, "maximum": 10}),
    );
    properties.insert(
        String::from("audio_mode"),
        json!({"type": "string", "enum": ["auto", "origin"]}),
    );
    properties.insert(
        String::from("reference_image_paths"),
        json!({
            "type": "array",
            "maxItems": 4,
            "items": {"type": "string", "minLength": 1}
        }),
    );
    Value::Object(serde_json::Map::from_iter([
        (
            String::from("$schema"),
            Value::String(String::from("https://json-schema.org/draft/2020-12/schema")),
        ),
        (String::from("type"), Value::String(String::from("object"))),
        (String::from("additionalProperties"), Value::Bool(false)),
        (String::from("properties"), Value::Object(properties)),
        (String::from("required"), json!(["source_video_path"])),
    ]))
}

/// Builds one exact Wan 2.7 text-to-video or image-to-video target.
/// 构建一个精确的 Wan 2.7 文生视频或图生视频目标。
///
/// # Parameters
/// 参数
/// `service_entry_id` fixes the reviewed Beijing Model Studio entry and `model_id`
/// selects the exact Wan carrier.
/// `service_entry_id` 固定已审核的百炼北京入口，`model_id` 选择精确 Wan 载体。
///
/// # Returns
/// 返回值
/// A model-specific asynchronous MP4 generation target.
/// 返回模型专属的异步 MP4 生成目标。
fn alibaba_wan_video_target(service_entry_id: &str, model_id: &str) -> ProviderToolTarget {
    let input_schema = alibaba_wan_video_schema(model_id);
    ProviderToolTarget {
        target_id: format!("{service_entry_id}.{model_id}"),
        provider_id: String::from("alibaba"),
        service_entry_id: service_entry_id.to_string(),
        credential_profile_ids: BTreeSet::from([String::from("api-key")]),
        upstream_target_id: model_id.to_string(),
        display_name: match model_id {
            "wan2.7-t2v" => String::from("Wan 2.7 Text to Video"),
            "wan2.7-i2v" => String::from("Wan 2.7 Image to Video"),
            _ => unreachable!("closed Wan 2.7 target registry"),
        },
        contract: ProviderToolContract {
            tool_id: String::from("video_generate"),
            description: match model_id {
                "wan2.7-t2v" => String::from(
                    "Generate one MP4 video from a text prompt with Wan 2.7 and wait for provider completion.",
                ),
                "wan2.7-i2v" => String::from(
                    "Generate one MP4 video from a required local first frame and optional local last frame or driving audio with Wan 2.7.",
                ),
                _ => unreachable!("closed Wan 2.7 target registry"),
            },
            capability: ProviderToolCapabilityKind::VideoGenerate,
            input_schema,
            output: ProviderToolOutputContract {
                kind: ProviderToolOutputKind::Resources,
                source_path: String::from("output.video_url|output.results[0].url"),
                normalized_result_path: String::from("resources"),
                resource_kind: Some(String::from("video")),
                mime_types: vec![String::from("video/mp4")],
            },
        },
        execution: ProviderToolExecutionContract {
            endpoint_base_url: Some(alibaba_service_origin(service_entry_id).to_string()),
            endpoint_path: String::from("/api/v1/services/aigc/video-generation/video-synthesis"),
            authentication: AuthenticationProjection::BearerAuthorization,
            adapter: ProviderToolAdapterKind::AlibabaWanVideoGenerate,
        },
    }
}

/// Builds the exact model-specific Wan 2.7 input schema.
/// 构建精确的模型专属 Wan 2.7 输入 Schema。
///
/// # Parameters
/// 参数
/// `model_id` selects text-only or first-frame input semantics.
/// `model_id` 选择纯文本或首帧输入语义。
///
/// # Returns
/// 返回值
/// A closed JSON Schema containing only documented Wan 2.7 fields.
/// 返回仅包含文档明确 Wan 2.7 字段的封闭 JSON Schema。
fn alibaba_wan_video_schema(model_id: &str) -> Value {
    let mut properties = serde_json::Map::from_iter([
        (
            String::from("prompt"),
            json!({"type": "string", "minLength": 1}),
        ),
        (String::from("negative_prompt"), json!({"type": "string"})),
        (
            String::from("resolution"),
            json!({"type": "string", "enum": ["720P", "1080P"]}),
        ),
        (
            String::from("duration_seconds"),
            json!({"type": "integer", "minimum": 2, "maximum": 15}),
        ),
        (String::from("prompt_extend"), json!({"type": "boolean"})),
        (String::from("watermark"), json!({"type": "boolean"})),
        (
            String::from("seed"),
            json!({"type": "integer", "minimum": 0, "maximum": 2147483647_i64}),
        ),
    ]);
    let required = match model_id {
        "wan2.7-t2v" => {
            properties.insert(
                String::from("aspect_ratio"),
                json!({"type": "string", "enum": ["16:9", "9:16", "1:1", "4:3", "3:4"]}),
            );
            properties.insert(
                String::from("audio_path"),
                json!({"type": "string", "minLength": 1, "description": "Optional local MP3 or WAV audio-track path."}),
            );
            json!(["prompt"])
        }
        "wan2.7-i2v" => {
            properties.insert(
                String::from("first_frame_path"),
                json!({"type": "string", "minLength": 1, "description": "Required local JPEG, PNG, or WebP first-frame path."}),
            );
            properties.insert(
                String::from("last_frame_path"),
                json!({"type": "string", "minLength": 1, "description": "Optional local JPEG, PNG, or WebP last-frame path."}),
            );
            properties.insert(
                String::from("audio_path"),
                json!({"type": "string", "minLength": 1, "description": "Optional local MP3 or WAV driving-audio path."}),
            );
            json!(["first_frame_path"])
        }
        _ => unreachable!("closed Wan 2.7 target registry"),
    };
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "additionalProperties": false,
        "properties": properties,
        "required": required
    })
}

/// Builds one exact Beijing Fun-Music generation target.
/// 构建一个精确的北京 Fun-Music 生成目标。
///
/// # Parameters
/// 参数
/// `service_entry_id` fixes the reviewed Beijing Model Studio entry and `model_id`
/// selects the model-specific conditional fields.
/// `service_entry_id` 固定已审核的百炼北京入口，`model_id` 选择模型专属条件字段。
///
/// # Returns
/// 返回值
/// A synchronous audio-resource target with MP3 and WAV output.
/// 返回支持 MP3 与 WAV 输出的同步音频资源目标。
fn alibaba_fun_music_target(service_entry_id: &str, model_id: &str) -> ProviderToolTarget {
    ProviderToolTarget {
        target_id: format!("{service_entry_id}.{model_id}"),
        provider_id: String::from("alibaba"),
        service_entry_id: service_entry_id.to_string(),
        credential_profile_ids: BTreeSet::from([String::from("api-key")]),
        upstream_target_id: model_id.to_string(),
        display_name: match model_id {
            "fun-music-v1" => String::from("Fun-Music V1"),
            "fun-music-preview" => String::from("Fun-Music Preview"),
            _ => unreachable!("closed Fun-Music target registry"),
        },
        contract: ProviderToolContract {
            tool_id: String::from("music_generate"),
            description: String::from(
                "Generate one complete song or instrumental audio resource with the selected Fun-Music model.",
            ),
            capability: ProviderToolCapabilityKind::MusicGenerate,
            input_schema: alibaba_fun_music_schema(model_id),
            output: ProviderToolOutputContract {
                kind: ProviderToolOutputKind::Resources,
                source_path: String::from("output.audio.url"),
                normalized_result_path: String::from("resources"),
                resource_kind: Some(String::from("audio")),
                mime_types: vec![String::from("audio/mpeg"), String::from("audio/wav")],
            },
        },
        execution: ProviderToolExecutionContract {
            endpoint_base_url: Some(alibaba_service_origin(service_entry_id).to_string()),
            endpoint_path: String::from("/api/v1/services/audio/music/generation"),
            authentication: AuthenticationProjection::BearerAuthorization,
            adapter: ProviderToolAdapterKind::AlibabaFunMusicGenerate,
        },
    }
}

/// Builds the exact model-specific Fun-Music input schema.
/// 构建精确的模型专属 Fun-Music 输入 Schema。
///
/// # Parameters
/// 参数
/// `model_id` selects whether vocal gender is supported and how prompt requirements apply.
/// `model_id` 选择是否支持人声性别以及提示词必填规则。
///
/// # Returns
/// 返回值
/// A closed non-streaming schema matching the official Fun-Music API.
/// 返回匹配官方 Fun-Music 非流式 API 的封闭 Schema。
fn alibaba_fun_music_schema(model_id: &str) -> Value {
    let mut properties = serde_json::Map::from_iter([
        (
            String::from("prompt"),
            json!({"type": "string", "minLength": 1, "maxLength": 2000}),
        ),
        (
            String::from("lyrics"),
            json!({"type": "string", "minLength": 5, "maxLength": 2000}),
        ),
        (String::from("is_instrumental"), json!({"type": "boolean"})),
        (
            String::from("format"),
            json!({"type": "string", "enum": ["mp3", "wav"]}),
        ),
        (
            String::from("enable_aigc_watermark"),
            json!({"type": "boolean"}),
        ),
    ]);
    let mut schema = serde_json::Map::from_iter([
        (
            String::from("$schema"),
            Value::String(String::from("https://json-schema.org/draft/2020-12/schema")),
        ),
        (String::from("type"), Value::String(String::from("object"))),
        (String::from("additionalProperties"), Value::Bool(false)),
    ]);
    match model_id {
        "fun-music-v1" => {
            properties.insert(
                String::from("gender"),
                json!({"type": "string", "enum": ["male", "female"]}),
            );
            schema.insert(
                String::from("anyOf"),
                json!([{"required": ["prompt"]}, {"required": ["lyrics"]}]),
            );
        }
        "fun-music-preview" => {
            schema.insert(String::from("required"), json!(["prompt"]));
        }
        _ => unreachable!("closed Fun-Music target registry"),
    }
    schema.insert(String::from("properties"), Value::Object(properties));
    Value::Object(schema)
}

/// Builds the exact Tavily Search and Extract service targets.
/// 构建精确的 Tavily Search 与 Extract 服务目标。
///
/// # Returns
/// 返回值
/// Two independently selectable provider-native tool contracts sharing one API credential.
/// 返回两个可独立选择且共享一个 API 凭据的供应商原生工具契约。
fn tavily_tool_targets() -> Vec<ProviderToolTarget> {
    vec![tavily_search_target(), tavily_extract_target()]
}

/// Builds the reviewed Tavily structured-search target.
/// 构建经过审核的 Tavily 结构化搜索目标。
///
/// # Returns
/// 返回值
/// An exact schema for the result-only search surface normalized into search-card data.
/// 返回结果型搜索界面的精确 Schema，并归一化为搜索卡片数据。
fn tavily_search_target() -> ProviderToolTarget {
    ProviderToolTarget {
        target_id: String::from("public-api.search"),
        provider_id: String::from("tavily"),
        service_entry_id: String::from("public-api"),
        credential_profile_ids: BTreeSet::from([String::from("api-key")]),
        upstream_target_id: String::from("search"),
        display_name: String::from("Tavily Search"),
        contract: ProviderToolContract {
            tool_id: String::from("vulcan_web_search"),
            description: String::from(
                "Search the public web with the selected Tavily account and return provider-backed structured results.",
            ),
            capability: ProviderToolCapabilityKind::WebSearch,
            input_schema: json!({
                "$schema": "https://json-schema.org/draft/2020-12/schema",
                "type": "object",
                "description": "Tavily structured web search arguments.",
                "additionalProperties": false,
                "properties": {
                    "query": {
                        "type": "string",
                        "minLength": 1,
                        "description": "The exact web search query."
                    },
                    "max_results": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 20,
                        "default": 5,
                        "description": "Maximum number of ordered results."
                    },
                    "include_domains": {
                        "type": "array",
                        "maxItems": 300,
                        "uniqueItems": true,
                        "items": {"type": "string", "minLength": 1},
                        "description": "Optional exact domain allowlist."
                    },
                    "exclude_domains": {
                        "type": "array",
                        "maxItems": 150,
                        "uniqueItems": true,
                        "items": {"type": "string", "minLength": 1},
                        "description": "Optional exact domain blocklist."
                    }
                },
                "required": ["query"]
            }),
            output: ProviderToolOutputContract {
                kind: ProviderToolOutputKind::Search,
                source_path: String::from("results"),
                normalized_result_path: String::from("search"),
                resource_kind: None,
                mime_types: Vec::new(),
            },
        },
        execution: ProviderToolExecutionContract {
            endpoint_base_url: None,
            endpoint_path: String::from("/search"),
            authentication: AuthenticationProjection::BearerAuthorization,
            adapter: ProviderToolAdapterKind::TavilySearch,
        },
    }
}

/// Builds the reviewed Tavily structured-extraction target.
/// 构建经过审核的 Tavily 结构化网页提取目标。
///
/// # Returns
/// 返回值
/// An exact extraction schema retaining query dependencies and provider limits.
/// 返回保留查询依赖关系与供应商限制的精确提取 Schema。
fn tavily_extract_target() -> ProviderToolTarget {
    ProviderToolTarget {
        target_id: String::from("public-api.extract"),
        provider_id: String::from("tavily"),
        service_entry_id: String::from("public-api"),
        credential_profile_ids: BTreeSet::from([String::from("api-key")]),
        upstream_target_id: String::from("extract"),
        display_name: String::from("Tavily Extract"),
        contract: ProviderToolContract {
            tool_id: String::from("vulcan_web_extract"),
            description: String::from(
                "Extract structured content from one or more HTTPS pages with the selected Tavily account.",
            ),
            capability: ProviderToolCapabilityKind::WebExtract,
            input_schema: json!({
                "$schema": "https://json-schema.org/draft/2020-12/schema",
                "type": "object",
                "description": "Tavily web extraction arguments.",
                "additionalProperties": false,
                "properties": {
                    "urls": {
                        "type": "array",
                        "minItems": 1,
                        "maxItems": 20,
                        "uniqueItems": true,
                        "items": {
                            "type": "string",
                            "format": "uri",
                            "pattern": "^https://"
                        },
                        "description": "One to twenty credential-free HTTPS URLs."
                    },
                    "query": {
                        "type": "string",
                        "minLength": 1,
                        "description": "Optional query used to retain relevance-ranked chunks."
                    },
                    "chunks_per_source": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 5,
                        "default": 3,
                        "description": "Relevant chunks per source; valid only when query is present."
                    },
                    "extract_depth": {
                        "type": "string",
                        "enum": ["basic", "advanced"],
                        "default": "basic",
                        "description": "Provider extraction depth."
                    },
                    "format": {
                        "type": "string",
                        "enum": ["markdown", "text"],
                        "default": "markdown",
                        "description": "Extracted content format."
                    },
                    "include_images": {
                        "type": "boolean",
                        "default": false,
                        "description": "Request extracted image URLs."
                    },
                    "include_favicon": {
                        "type": "boolean",
                        "default": false,
                        "description": "Request each page favicon URL."
                    },
                    "timeout_seconds": {
                        "type": "number",
                        "minimum": 1,
                        "maximum": 60,
                        "description": "Provider extraction timeout in seconds."
                    }
                },
                "required": ["urls"],
                "dependentRequired": {
                    "chunks_per_source": ["query"]
                }
            }),
            output: ProviderToolOutputContract {
                kind: ProviderToolOutputKind::Extract,
                source_path: String::from("results"),
                normalized_result_path: String::from("extract"),
                resource_kind: None,
                mime_types: Vec::new(),
            },
        },
        execution: ProviderToolExecutionContract {
            endpoint_base_url: None,
            endpoint_path: String::from("/extract"),
            authentication: AuthenticationProjection::BearerAuthorization,
            adapter: ProviderToolAdapterKind::TavilyExtract,
        },
    }
}

/// Builds MiniMax image targets for both exact regional service entries.
/// 为两个精确区域服务入口构建 MiniMax 图片目标。
///
/// # Returns
/// 返回值
/// Four independently selectable regional and model-specific targets.
/// 返回四个可独立选择的区域与模型专属目标。
fn minimax_tool_targets() -> Vec<ProviderToolTarget> {
    ["global", "cn"]
        .into_iter()
        .flat_map(|service_entry_id| {
            let mut targets = vec![
                minimax_vision_target(service_entry_id),
                minimax_image_target(service_entry_id, "image-01", false),
                minimax_image_target(service_entry_id, "image-01-live", true),
                minimax_search_target(service_entry_id),
            ];
            targets.extend([
                minimax_speech_target(service_entry_id, "speech-2.8-hd"),
                minimax_speech_target(service_entry_id, "speech-2.8-turbo"),
                minimax_speech_target(service_entry_id, "speech-2.6"),
                minimax_speech_target(service_entry_id, "speech-02"),
            ]);
            targets.extend([
                minimax_music_target(service_entry_id, "music-3.0"),
                minimax_music_target(service_entry_id, "music-2.6"),
                minimax_music_target(service_entry_id, "music-2.6-free"),
                minimax_music_target(service_entry_id, "music-2.5+"),
                minimax_music_target(service_entry_id, "music-2.5"),
            ]);
            targets.extend([
                minimax_video_target(service_entry_id, "MiniMax-Hailuo-2.3"),
                minimax_video_target(service_entry_id, "MiniMax-Hailuo-2.3-Fast"),
                minimax_video_target(service_entry_id, "MiniMax-Hailuo-02"),
                minimax_video_target(service_entry_id, "S2V-01"),
            ]);
            targets
        })
        .collect()
}

/// Builds one exact MiniMax asynchronous video-generation target.
/// 构建一个精确的 MiniMax 异步视频生成目标。
///
/// # Parameters
/// 参数
/// `service_entry_id` fixes the region and `model_id` fixes the model-specific input mode.
/// `service_entry_id` 固定区域，`model_id` 固定模型专属输入模式。
///
/// # Returns
/// 返回值
/// One model-specific target with no cross-model schema union.
/// 返回一个不包含跨模型 Schema 联合的模型专属目标。
fn minimax_video_target(service_entry_id: &str, model_id: &str) -> ProviderToolTarget {
    ProviderToolTarget {
        target_id: format!("{service_entry_id}.{model_id}"),
        provider_id: String::from("minimax"),
        service_entry_id: service_entry_id.to_string(),
        credential_profile_ids: BTreeSet::from([
            String::from("payg-api-key"),
            String::from("token-plan-key"),
            String::from("token-plan-oauth"),
        ]),
        upstream_target_id: model_id.to_string(),
        display_name: match model_id {
            "MiniMax-Hailuo-2.3" => String::from("MiniMax Hailuo 2.3"),
            "MiniMax-Hailuo-2.3-Fast" => String::from("MiniMax Hailuo 2.3 Fast"),
            "MiniMax-Hailuo-02" => String::from("MiniMax Hailuo 02"),
            "S2V-01" => String::from("MiniMax S2V 01"),
            _ => unreachable!("closed MiniMax video target registry"),
        },
        contract: ProviderToolContract {
            tool_id: String::from("video_generate"),
            description: String::from(
                "Generate one MP4 video with the selected MiniMax video model and wait for provider completion.",
            ),
            capability: ProviderToolCapabilityKind::VideoGenerate,
            input_schema: minimax_video_input_schema(model_id),
            output: ProviderToolOutputContract {
                kind: ProviderToolOutputKind::Resources,
                source_path: String::from("file.download_url"),
                normalized_result_path: String::from("resources"),
                resource_kind: Some(String::from("video")),
                mime_types: vec![String::from("video/mp4")],
            },
        },
        execution: ProviderToolExecutionContract {
            endpoint_base_url: None,
            endpoint_path: String::from("/v1/video_generation"),
            authentication: AuthenticationProjection::BearerAuthorization,
            adapter: ProviderToolAdapterKind::MiniMaxVideoGenerate,
        },
    }
}

/// Builds one exact model-specific MiniMax video input schema.
/// 构建一个精确的模型专属 MiniMax 视频输入 Schema。
///
/// # Parameters
/// 参数
/// `model_id` selects the documented text, frame, or subject-reference mode.
/// `model_id` 选择文档声明的文本、帧或主体参考模式。
///
/// # Returns
/// 返回值
/// A closed schema containing only fields accepted by that model.
/// 返回一个仅包含该模型接受字段的封闭 Schema。
fn minimax_video_input_schema(model_id: &str) -> Value {
    let resolutions = if matches!(model_id, "MiniMax-Hailuo-02" | "S2V-01") {
        json!(["512P", "768P", "1080P"])
    } else {
        json!(["768P", "1080P"])
    };
    let mut properties = serde_json::Map::from_iter([
        (
            String::from("prompt"),
            json!({
                "type": "string",
                "minLength": 1,
                "maxLength": 2000,
                "description": "Motion and scene description."
            }),
        ),
        (
            String::from("duration_seconds"),
            json!({
                "type": "integer",
                "enum": [6, 10],
                "default": 6
            }),
        ),
        (
            String::from("resolution"),
            json!({
                "type": "string",
                "enum": resolutions
            }),
        ),
        (
            String::from("prompt_extend"),
            json!({
                "type": "boolean",
                "description": "Enable provider-native prompt optimization."
            }),
        ),
    ]);
    let mut required = vec![Value::String(String::from("prompt"))];
    match model_id {
        "MiniMax-Hailuo-2.3" => {
            properties.insert(
                String::from("first_frame_path"),
                json!({
                    "type": "string",
                    "minLength": 1,
                    "description": "Optional local JPEG, PNG, or WebP first-frame path."
                }),
            );
        }
        "MiniMax-Hailuo-2.3-Fast" => {
            properties.insert(
                String::from("first_frame_path"),
                json!({
                    "type": "string",
                    "minLength": 1,
                    "description": "Required local JPEG, PNG, or WebP first-frame path."
                }),
            );
            required.push(Value::String(String::from("first_frame_path")));
        }
        "MiniMax-Hailuo-02" => {
            properties.insert(
                String::from("first_frame_path"),
                json!({
                    "type": "string",
                    "minLength": 1,
                    "description": "Optional local JPEG, PNG, or WebP first-frame path."
                }),
            );
            properties.insert(
                String::from("last_frame_path"),
                json!({
                    "type": "string",
                    "minLength": 1,
                    "description": "Optional local last-frame path; requires first_frame_path."
                }),
            );
        }
        "S2V-01" => {
            properties.insert(
                String::from("subject_reference_path"),
                json!({
                    "type": "string",
                    "minLength": 1,
                    "description": "Required local JPEG, PNG, or WebP character reference path."
                }),
            );
            required.push(Value::String(String::from("subject_reference_path")));
        }
        _ => unreachable!("closed MiniMax video schema registry"),
    }
    let mut schema = serde_json::Map::from_iter([
        (String::from("type"), Value::String(String::from("object"))),
        (String::from("additionalProperties"), Value::Bool(false)),
        (String::from("properties"), Value::Object(properties)),
        (String::from("required"), Value::Array(required)),
    ]);
    if model_id == "MiniMax-Hailuo-02" {
        schema.insert(
            String::from("dependentRequired"),
            json!({"last_frame_path": ["first_frame_path"]}),
        );
    }
    Value::Object(schema)
}

/// Builds one exact MiniMax synchronous music-generation target.
/// 构建一个精确的 MiniMax 同步音乐生成目标。
///
/// # Parameters
/// 参数
/// `service_entry_id` fixes the region and `model_id` fixes the upstream music model.
/// `service_entry_id` 固定区域，`model_id` 固定上游音乐模型。
///
/// # Returns
/// 返回值
/// One model-specific target with the reviewed native music schema.
/// 返回一个带有已审核原生音乐 Schema 的模型专属目标。
fn minimax_music_target(service_entry_id: &str, model_id: &str) -> ProviderToolTarget {
    ProviderToolTarget {
        target_id: format!("{service_entry_id}.{model_id}"),
        provider_id: String::from("minimax"),
        service_entry_id: service_entry_id.to_string(),
        credential_profile_ids: BTreeSet::from([
            String::from("payg-api-key"),
            String::from("token-plan-key"),
            String::from("token-plan-oauth"),
        ]),
        upstream_target_id: model_id.to_string(),
        display_name: match model_id {
            "music-3.0" => String::from("MiniMax Music 3.0"),
            "music-2.6" => String::from("MiniMax Music 2.6"),
            "music-2.6-free" => String::from("MiniMax Music 2.6 Free"),
            "music-2.5+" => String::from("MiniMax Music 2.5 Plus"),
            "music-2.5" => String::from("MiniMax Music 2.5"),
            _ => unreachable!("closed MiniMax music target registry"),
        },
        contract: ProviderToolContract {
            tool_id: String::from("music_generate"),
            description: String::from(
                "Generate one music audio resource with the selected MiniMax music model.",
            ),
            capability: ProviderToolCapabilityKind::MusicGenerate,
            input_schema: minimax_music_input_schema(),
            output: ProviderToolOutputContract {
                kind: ProviderToolOutputKind::Resources,
                source_path: String::from("data.audio_url"),
                normalized_result_path: String::from("resources"),
                resource_kind: Some(String::from("audio")),
                mime_types: vec![
                    String::from("audio/mpeg"),
                    String::from("audio/wav"),
                    String::from("audio/L16"),
                ],
            },
        },
        execution: ProviderToolExecutionContract {
            endpoint_base_url: None,
            endpoint_path: String::from("/v1/music_generation"),
            authentication: AuthenticationProjection::BearerAuthorization,
            adapter: ProviderToolAdapterKind::MiniMaxMusicGenerate,
        },
    }
}

/// Builds the exact AI-visible schema shared by verified synchronous MiniMax music models.
/// 构建已验证同步 MiniMax 音乐模型共享的精确 AI 可见 Schema。
///
/// # Returns
/// 返回值
/// Closed prompt, lyrics, intent, encoding, seed, and watermark controls.
/// 返回封闭的提示词、歌词、意图、编码、种子与水印控制。
fn minimax_music_input_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "prompt": {
                "type": "string",
                "maxLength": 2000,
                "description": "Style, mood, and scenario guidance."
            },
            "lyrics": {
                "type": "string",
                "maxLength": 3500,
                "description": "Exact lyrics; omit for instrumental or optimized lyric generation."
            },
            "instrumental": {
                "type": "boolean",
                "default": false,
                "description": "Generate instrumental music without lyrics."
            },
            "lyrics_optimizer": {
                "type": "boolean",
                "default": false,
                "description": "Let MiniMax generate optimized lyrics from the prompt."
            },
            "sample_rate": {
                "type": "integer",
                "minimum": 1,
                "default": 44100
            },
            "bitrate": {
                "type": "integer",
                "minimum": 1,
                "default": 256000
            },
            "channels": {
                "type": "integer",
                "minimum": 1,
                "maximum": 2
            },
            "seed": {
                "type": "integer"
            },
            "watermark": {
                "type": "boolean",
                "default": false
            },
            "output_format": {
                "type": "string",
                "enum": ["mp3", "wav", "pcm"],
                "default": "mp3"
            }
        }
    })
}

/// Builds one exact MiniMax synchronous speech-model target.
/// 构建一个精确的 MiniMax 同步语音模型目标。
///
/// # Parameters
/// 参数
/// `service_entry_id` fixes the region and `model_id` fixes the upstream speech model.
/// `service_entry_id` 固定区域，`model_id` 固定上游语音模型。
///
/// # Returns
/// 返回值
/// One model-specific target with the reviewed synchronous T2A schema.
/// 返回一个带有已审核同步 T2A Schema 的模型专属目标。
fn minimax_speech_target(service_entry_id: &str, model_id: &str) -> ProviderToolTarget {
    ProviderToolTarget {
        target_id: format!("{service_entry_id}.{model_id}"),
        provider_id: String::from("minimax"),
        service_entry_id: service_entry_id.to_string(),
        credential_profile_ids: BTreeSet::from([
            String::from("payg-api-key"),
            String::from("token-plan-key"),
            String::from("token-plan-oauth"),
        ]),
        upstream_target_id: model_id.to_string(),
        display_name: match model_id {
            "speech-2.8-hd" => String::from("MiniMax Speech 2.8 HD"),
            "speech-2.8-turbo" => String::from("MiniMax Speech 2.8 Turbo"),
            "speech-2.6" => String::from("MiniMax Speech 2.6"),
            "speech-02" => String::from("MiniMax Speech 02"),
            _ => unreachable!("closed MiniMax speech target registry"),
        },
        contract: ProviderToolContract {
            tool_id: String::from("speech_synthesize"),
            description: String::from(
                "Synthesize one bounded speech audio resource with the selected MiniMax speech model.",
            ),
            capability: ProviderToolCapabilityKind::SpeechSynthesize,
            input_schema: minimax_speech_input_schema(),
            output: ProviderToolOutputContract {
                kind: ProviderToolOutputKind::Resources,
                source_path: String::from("data.audio"),
                normalized_result_path: String::from("resources"),
                resource_kind: Some(String::from("audio")),
                mime_types: vec![
                    String::from("audio/mpeg"),
                    String::from("audio/wav"),
                    String::from("audio/pcm"),
                    String::from("audio/flac"),
                    String::from("audio/basic"),
                    String::from("audio/opus"),
                    String::from("application/json"),
                ],
            },
        },
        execution: ProviderToolExecutionContract {
            endpoint_base_url: None,
            endpoint_path: String::from("/v1/t2a_v2"),
            authentication: AuthenticationProjection::BearerAuthorization,
            adapter: ProviderToolAdapterKind::MiniMaxSpeechSynthesize,
        },
    }
}

/// Builds the exact AI-visible schema shared by verified synchronous MiniMax speech models.
/// 构建已验证同步 MiniMax 语音模型共享的精确 AI 可见 Schema。
///
/// # Returns
/// 返回值
/// Closed voice, language, encoding, subtitle, and pronunciation controls.
/// 返回封闭的声音、语言、编码、字幕与发音控制。
fn minimax_speech_input_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "text": {
                "type": "string",
                "minLength": 1,
                "maxLength": 9999,
                "description": "Text to synthesize."
            },
            "voice_id": {
                "type": "string",
                "minLength": 1,
                "description": "Built-in or account-owned MiniMax voice identifier."
            },
            "language": {
                "type": "string",
                "enum": [
                    "Chinese", "Chinese,Yue", "English", "Arabic", "Russian", "Spanish",
                    "French", "Portuguese", "German", "Turkish", "Dutch", "Ukrainian",
                    "Vietnamese", "Indonesian", "Japanese", "Italian", "Korean", "Thai",
                    "Polish", "Romanian", "Greek", "Czech", "Finnish", "Hindi",
                    "Bulgarian", "Danish", "Hebrew", "Malay", "Persian", "Slovak",
                    "Swedish", "Croatian", "Filipino", "Hungarian", "Norwegian",
                    "Slovenian", "Catalan", "Nynorsk", "Tamil", "Afrikaans", "auto"
                ],
                "description": "Optional MiniMax language_boost value."
            },
            "speed": {
                "type": "number",
                "minimum": 0.5,
                "maximum": 2.0,
                "default": 1.0
            },
            "volume": {
                "type": "number",
                "exclusiveMinimum": 0,
                "maximum": 10.0,
                "default": 1.0
            },
            "pitch": {
                "type": "integer",
                "minimum": -12,
                "maximum": 12,
                "default": 0
            },
            "sample_rate": {
                "type": "integer",
                "enum": [8000, 16000, 22050, 24000, 32000, 44100],
                "default": 32000
            },
            "bitrate": {
                "type": "integer",
                "enum": [32000, 64000, 128000, 256000],
                "description": "MP3 bitrate; omit for non-MP3 output."
            },
            "channels": {
                "type": "integer",
                "enum": [1, 2],
                "default": 1
            },
            "output_format": {
                "type": "string",
                "enum": ["mp3", "pcm", "flac", "wav", "pcmu_raw", "pcmu_wav", "opus"],
                "default": "mp3"
            },
            "timestamps": {
                "type": "boolean",
                "default": false,
                "description": "Request a subtitle timing JSON resource."
            },
            "pronunciations": {
                "type": "array",
                "items": {"type": "string", "minLength": 1},
                "description": "Ordered MiniMax pronunciation_dict tone entries."
            }
        },
        "required": ["text", "voice_id"]
    })
}

/// Builds one exact MiniMax Coding Plan image-understanding target.
/// 构建一个精确的 MiniMax Coding Plan 图片理解目标。
///
/// # Parameters
/// 参数
/// `service_entry_id` fixes the regional Coding Plan origin.
/// `service_entry_id` 固定区域 Coding Plan 服务入口。
///
/// # Returns
/// 返回值
/// A local-image and prompt contract restricted to Token Plan credentials.
/// 返回仅限 Token Plan 凭据使用的本地图片与提示词契约。
fn minimax_vision_target(service_entry_id: &str) -> ProviderToolTarget {
    ProviderToolTarget {
        target_id: format!("{service_entry_id}:coding-plan-vlm"),
        provider_id: String::from("minimax"),
        service_entry_id: service_entry_id.to_string(),
        credential_profile_ids: BTreeSet::from([
            String::from("token-plan-key"),
            String::from("token-plan-oauth"),
        ]),
        upstream_target_id: String::from("coding-plan-vlm"),
        display_name: String::from("MiniMax Coding Plan Image Understanding"),
        contract: ProviderToolContract {
            tool_id: String::from("image_understand"),
            description: String::from(
                "Analyze one local JPEG, PNG, or WebP image with the selected MiniMax Coding Plan account.",
            ),
            capability: ProviderToolCapabilityKind::MediaAnalyze,
            input_schema: json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "image_path": {
                        "type": "string",
                        "minLength": 1,
                        "description": "Local path to one JPEG, PNG, or WebP image."
                    },
                    "prompt": {
                        "type": "string",
                        "minLength": 1,
                        "description": "Exact analysis instruction for the image."
                    }
                },
                "required": ["image_path", "prompt"]
            }),
            output: ProviderToolOutputContract {
                kind: ProviderToolOutputKind::Text,
                source_path: String::from("content"),
                normalized_result_path: String::from("text"),
                resource_kind: None,
                mime_types: Vec::new(),
            },
        },
        execution: ProviderToolExecutionContract {
            endpoint_base_url: None,
            endpoint_path: String::from("/v1/coding_plan/vlm"),
            authentication: AuthenticationProjection::BearerAuthorization,
            adapter: ProviderToolAdapterKind::MiniMaxCodingPlanVision,
        },
    }
}

/// Builds one exact MiniMax Coding Plan search target.
/// 构建一个精确的 MiniMax Coding Plan 搜索目标。
///
/// # Parameters
/// 参数
/// `service_entry_id` fixes the regional MiniMax origin.
/// `service_entry_id` 固定 MiniMax 区域入口。
///
/// # Returns
/// 返回值
/// A single-query native contract restricted to Token Plan credentials.
/// 返回仅限 Token Plan 凭据使用的单查询原生契约。
fn minimax_search_target(service_entry_id: &str) -> ProviderToolTarget {
    ProviderToolTarget {
        target_id: format!("{service_entry_id}:coding-plan-search"),
        provider_id: String::from("minimax"),
        service_entry_id: service_entry_id.to_string(),
        credential_profile_ids: BTreeSet::from([
            String::from("token-plan-key"),
            String::from("token-plan-oauth"),
        ]),
        upstream_target_id: String::from("coding-plan-search"),
        display_name: String::from("MiniMax Coding Plan Search"),
        contract: ProviderToolContract {
            tool_id: String::from("vulcan_web_search"),
            description: String::from(
                "Search the public web with the selected MiniMax Coding Plan account and return ordered structured results.",
            ),
            capability: ProviderToolCapabilityKind::WebSearch,
            input_schema: json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "query": {
                        "type": "string",
                        "minLength": 1,
                        "description": "The exact web search query."
                    },
                    "max_results": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 10,
                        "description": "Maximum number of ordered results to return."
                    }
                },
                "required": ["query"]
            }),
            output: ProviderToolOutputContract {
                kind: ProviderToolOutputKind::Search,
                source_path: String::from("organic"),
                normalized_result_path: String::from("search"),
                resource_kind: None,
                mime_types: Vec::new(),
            },
        },
        execution: ProviderToolExecutionContract {
            endpoint_base_url: None,
            endpoint_path: String::from("/v1/coding_plan/search"),
            authentication: AuthenticationProjection::BearerAuthorization,
            adapter: ProviderToolAdapterKind::MiniMaxCodingPlanSearch,
        },
    }
}

/// Builds one exact MiniMax image-generation target.
/// 构建一个精确的 MiniMax 图片生成目标。
///
/// # Parameters
/// 参数
/// `service_entry_id` fixes the region, `model_id` fixes the upstream model, and `live`
/// selects the documented model-specific parameter set.
/// `service_entry_id` 固定区域，`model_id` 固定上游模型，`live`
/// 选择文档声明的模型专属参数集合。
///
/// # Returns
/// 返回值
/// A target with an unmerged native input schema and unified resource output contract.
/// 返回包含未合并原生输入 Schema 与统一资源输出契约的目标。
fn minimax_image_target(service_entry_id: &str, model_id: &str, live: bool) -> ProviderToolTarget {
    ProviderToolTarget {
        target_id: format!("{service_entry_id}.{model_id}"),
        provider_id: String::from("minimax"),
        service_entry_id: service_entry_id.to_string(),
        credential_profile_ids: BTreeSet::from([
            String::from("payg-api-key"),
            String::from("token-plan-key"),
            String::from("token-plan-oauth"),
        ]),
        upstream_target_id: model_id.to_string(),
        display_name: if live {
            String::from("MiniMax Image 01 Live")
        } else {
            String::from("MiniMax Image 01")
        },
        contract: ProviderToolContract {
            tool_id: String::from("image_generate"),
            description: String::from(
                "Generate one or more images with the selected MiniMax model and optional local character references.",
            ),
            capability: ProviderToolCapabilityKind::ImageGenerate,
            input_schema: minimax_image_input_schema(live),
            output: ProviderToolOutputContract {
                kind: ProviderToolOutputKind::Resources,
                source_path: String::from("data.image_urls"),
                normalized_result_path: String::from("resources"),
                resource_kind: Some(String::from("image")),
                mime_types: vec![String::from("image/jpeg")],
            },
        },
        execution: ProviderToolExecutionContract {
            endpoint_base_url: None,
            endpoint_path: String::from("/v1/image_generation"),
            authentication: AuthenticationProjection::BearerAuthorization,
            adapter: ProviderToolAdapterKind::MiniMaxImageGenerate,
        },
    }
}

/// Builds the exact AI-visible schema for one MiniMax image model.
/// 为一个 MiniMax 图片模型构建精确的 AI 可见 Schema。
///
/// # Parameters
/// 参数
/// `live` selects `image-01-live`; `false` selects `image-01`.
/// `live` 为真时选择 `image-01-live`，否则选择 `image-01`。
///
/// # Returns
/// 返回值
/// A closed JSON Schema containing only parameters accepted by that model.
/// 返回只包含该模型接受参数的封闭 JSON Schema。
fn minimax_image_input_schema(live: bool) -> Value {
    let mut properties = serde_json::Map::from_iter([
        (
            String::from("prompt"),
            json!({
                "type": "string",
                "description": "Image description. Required and limited to 1500 Unicode characters.",
                "minLength": 1,
                "maxLength": 1500
            }),
        ),
        (
            String::from("subject_references"),
            json!({
                "type": "array",
                "description": "Optional local JPG or PNG paths containing front-facing character references.",
                "items": {
                    "type": "string",
                    "format": "vulcan-local-path",
                    "description": "Local character-reference image path."
                }
            }),
        ),
        (
            String::from("aspect_ratio"),
            json!({
                "type": "string",
                "description": "Provider-supported output aspect ratio.",
                "enum": minimax_image_aspect_ratios(live)
            }),
        ),
        (
            String::from("count"),
            json!({
                "type": "integer",
                "description": "Number of images to generate.",
                "minimum": 1,
                "maximum": 9,
                "default": 1
            }),
        ),
        (
            String::from("seed"),
            json!({
                "type": "integer",
                "description": "Optional provider seed for similar reproducible outputs."
            }),
        ),
        (
            String::from("prompt_optimizer"),
            json!({
                "type": "boolean",
                "description": "Enable MiniMax prompt optimization.",
                "default": false
            }),
        ),
        (
            String::from("aigc_watermark"),
            json!({
                "type": "boolean",
                "description": "Add the provider AIGC watermark.",
                "default": false
            }),
        ),
    ]);
    let mut all_of = Vec::new();
    if live {
        properties.insert(
            String::from("style_type"),
            json!({
                "type": "string",
                "description": "MiniMax image-01-live style type.",
                "enum": ["漫画", "元气", "中世纪", "水彩"]
            }),
        );
        properties.insert(
            String::from("style_weight"),
            json!({
                "type": "number",
                "description": "MiniMax image-01-live style weight in the interval (0, 1].",
                "exclusiveMinimum": 0,
                "maximum": 1,
                "default": 0.8
            }),
        );
        all_of.push(json!({
            "if": {"required": ["style_weight"]},
            "then": {"required": ["style_type"]}
        }));
    } else {
        for name in ["width", "height"] {
            properties.insert(
                String::from(name),
                json!({
                    "type": "integer",
                    "description": format!(
                        "Custom {name} for image-01; requires the other dimension and excludes aspect_ratio."
                    ),
                    "minimum": 512,
                    "maximum": 2048,
                    "multipleOf": 8
                }),
            );
        }
        all_of.extend([
            json!({
                "if": {"required": ["width"]},
                "then": {
                    "required": ["height"],
                    "not": {"required": ["aspect_ratio"]}
                }
            }),
            json!({
                "if": {"required": ["height"]},
                "then": {
                    "required": ["width"],
                    "not": {"required": ["aspect_ratio"]}
                }
            }),
            json!({
                "if": {"required": ["aspect_ratio"]},
                "then": {
                    "not": {
                        "anyOf": [
                            {"required": ["width"]},
                            {"required": ["height"]}
                        ]
                    }
                }
            }),
        ]);
    }
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "description": "MiniMax model-specific image generation tool arguments.",
        "properties": properties,
        "required": ["prompt"],
        "additionalProperties": false,
        "allOf": all_of
    })
}

/// Returns the exact aspect-ratio vocabulary for one MiniMax image model.
/// 返回一个 MiniMax 图片模型的精确宽高比词汇表。
///
/// # Parameters
/// 参数
/// `live` selects the live model vocabulary.
/// `live` 选择 Live 模型词汇表。
///
/// # Returns
/// 返回值
/// The provider-documented model-specific values.
/// 返回供应商文档声明的模型专属值。
fn minimax_image_aspect_ratios(live: bool) -> Vec<&'static str> {
    let mut ratios = vec!["1:1", "16:9", "4:3", "3:2", "2:3", "3:4", "9:16"];
    if !live {
        ratios.push("21:9");
    }
    ratios
}

#[cfg(test)]
mod tests {
    use super::built_in_provider_tool_targets;
    use crate::{ProviderToolAdapterKind, ProviderToolCapabilityKind, ProviderToolOutputKind};

    /// Verifies MiniMax models keep separate schemas and a shared normalized output kind.
    /// 验证 MiniMax 模型保持独立 Schema 并共享归一化输出类别。
    #[test]
    fn minimax_image_models_keep_independent_schemas() {
        let targets = built_in_provider_tool_targets("minimax");
        assert_eq!(targets.len(), 34);
        assert_eq!(
            targets
                .iter()
                .filter(|target| {
                    target.contract.capability == ProviderToolCapabilityKind::WebSearch
                })
                .count(),
            2
        );
        assert_eq!(
            targets
                .iter()
                .filter(|target| {
                    target.contract.capability == ProviderToolCapabilityKind::SpeechSynthesize
                })
                .count(),
            8
        );
        assert_eq!(
            targets
                .iter()
                .filter(|target| {
                    target.contract.capability == ProviderToolCapabilityKind::MusicGenerate
                })
                .count(),
            10
        );
        assert_eq!(
            targets
                .iter()
                .filter(|target| {
                    target.contract.capability == ProviderToolCapabilityKind::VideoGenerate
                })
                .count(),
            8
        );
        assert_eq!(
            targets
                .iter()
                .filter(|target| {
                    target.contract.capability == ProviderToolCapabilityKind::MediaAnalyze
                })
                .count(),
            2
        );
        let standard = targets
            .iter()
            .find(|target| target.target_id == "cn.image-01")
            .expect("China image-01 target");
        let live = targets
            .iter()
            .find(|target| target.target_id == "cn.image-01-live")
            .expect("China image-01-live target");
        assert_eq!(
            standard.contract.capability,
            ProviderToolCapabilityKind::ImageGenerate
        );
        assert_eq!(
            standard.contract.output.kind,
            ProviderToolOutputKind::Resources
        );
        assert!(standard.contract.input_schema["properties"]["width"].is_object());
        assert!(standard.contract.input_schema["properties"]["style_type"].is_null());
        assert!(live.contract.input_schema["properties"]["width"].is_null());
        assert!(live.contract.input_schema["properties"]["style_type"].is_object());
    }

    /// Verifies Tavily publishes separate native schemas and unified output kinds.
    /// 验证 Tavily 发布彼此独立的原生 Schema 与统一输出类别。
    #[test]
    fn tavily_search_and_extract_keep_independent_contracts() {
        let targets = built_in_provider_tool_targets("tavily");
        assert_eq!(targets.len(), 2);
        let search = targets
            .iter()
            .find(|target| target.target_id == "public-api.search")
            .expect("Tavily search target");
        let extract = targets
            .iter()
            .find(|target| target.target_id == "public-api.extract")
            .expect("Tavily extract target");
        assert_eq!(
            search.contract.capability,
            ProviderToolCapabilityKind::WebSearch
        );
        assert_eq!(search.contract.output.kind, ProviderToolOutputKind::Search);
        assert!(search.contract.input_schema["properties"]["query"].is_object());
        assert!(search.contract.input_schema["properties"]["urls"].is_null());
        assert_eq!(
            extract.contract.capability,
            ProviderToolCapabilityKind::WebExtract
        );
        assert_eq!(
            extract.contract.output.kind,
            ProviderToolOutputKind::Extract
        );
        assert!(extract.contract.input_schema["properties"]["urls"].is_object());
        assert!(extract.contract.input_schema["properties"]["query"].is_object());
        assert!(extract.contract.input_schema["properties"]["max_results"].is_null());
    }

    /// Verifies Alibaba exposes WebSearch only through Model Studio API entries.
    /// 验证阿里云仅通过 Model Studio API 入口暴露 WebSearch。
    #[test]
    fn alibaba_search_targets_exclude_token_plan_entries() {
        let targets = built_in_provider_tool_targets("alibaba")
            .into_iter()
            .filter(|target| target.contract.capability == ProviderToolCapabilityKind::WebSearch)
            .collect::<Vec<_>>();
        assert_eq!(targets.len(), 2);
        assert!(targets.iter().all(|target| {
            target.contract.output.kind == ProviderToolOutputKind::Search
                && target.credential_profile_ids.len() == 1
                && target.credential_profile_ids.contains("api-key")
                && target.service_entry_id.starts_with("model-studio-")
                && target
                    .execution
                    .endpoint_base_url
                    .as_deref()
                    .is_some_and(|base_url| !base_url.contains("compatible-mode"))
        }));
        assert!(targets.iter().all(|target| {
            target.contract.input_schema["properties"]["query"].is_object()
                && target.contract.input_schema["properties"]["max_results"].is_object()
                && target.contract.input_schema["properties"]["include_domains"].is_null()
        }));
    }

    /// Verifies Alibaba image targets preserve product, model, operation, and credential boundaries.
    /// 验证阿里云图片目标保留产品、模型、操作与凭据边界。
    #[test]
    fn alibaba_image_targets_preserve_exact_product_matrix() {
        let targets = built_in_provider_tool_targets("alibaba");
        let image_targets = targets
            .iter()
            .filter(|target| {
                matches!(
                    target.contract.capability,
                    ProviderToolCapabilityKind::ImageGenerate
                        | ProviderToolCapabilityKind::ImageEdit
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(image_targets.len(), 14);
        assert_eq!(
            image_targets
                .iter()
                .filter(|target| {
                    target.execution.adapter == ProviderToolAdapterKind::AlibabaQwenImageEdit
                })
                .count(),
            2
        );
        assert_eq!(
            image_targets
                .iter()
                .filter(|target| {
                    target.execution.adapter == ProviderToolAdapterKind::AlibabaWanImageGenerate
                })
                .count(),
            6
        );
        assert!(image_targets.iter().all(|target| {
            target.contract.output.kind == ProviderToolOutputKind::Resources
                && target
                    .execution
                    .endpoint_base_url
                    .as_deref()
                    .is_some_and(|base_url| !base_url.contains("compatible-mode"))
        }));
        assert!(
            image_targets
                .iter()
                .filter(|target| { target.service_entry_id.starts_with("token-plan-") })
                .all(|target| target.credential_profile_ids.contains("token-plan-api-key"))
        );
    }

    /// Verifies Alibaba Wan video and Fun-Music stay restricted to reviewed Beijing contracts.
    /// 验证阿里云 Wan 视频与 Fun-Music 始终限制在已审核的北京合同内。
    #[test]
    fn alibaba_wan_video_and_music_preserve_exact_product_matrix() {
        let targets = built_in_provider_tool_targets("alibaba");
        let wan_video = targets
            .iter()
            .filter(|target| {
                target.execution.adapter == ProviderToolAdapterKind::AlibabaWanVideoGenerate
            })
            .collect::<Vec<_>>();
        assert_eq!(wan_video.len(), 2);
        assert!(wan_video.iter().all(|target| {
            target.service_entry_id == "model-studio-cn"
                && target.credential_profile_ids.contains("api-key")
                && target.contract.output.kind == ProviderToolOutputKind::Resources
        }));
        let music = targets
            .iter()
            .filter(|target| {
                target.execution.adapter == ProviderToolAdapterKind::AlibabaFunMusicGenerate
            })
            .collect::<Vec<_>>();
        assert_eq!(music.len(), 2);
        assert!(music.iter().all(|target| {
            target.service_entry_id == "model-studio-cn"
                && target.contract.capability == ProviderToolCapabilityKind::MusicGenerate
        }));
        let team_reference_video = targets.iter().find(|target| {
            target.service_entry_id == "token-plan-team-cn"
                && target.upstream_target_id == "happyhorse-1.1-r2v"
        });
        assert!(
            team_reference_video.is_none(),
            "Token Plan Team must not advertise a reference-video schema that requires an unverified upload carrier."
        );
    }
}
