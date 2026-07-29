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

/// Builds Alibaba Model Studio WebSearch targets for the two verified API regions.
/// 为两个已验证的阿里云百炼 API 区域构建 WebSearch 目标。
///
/// # Returns
/// 返回值
/// Two independently selectable service targets restricted to Model Studio API credentials.
/// 返回两个可独立选择且仅限 Model Studio API 凭据使用的服务目标。
fn alibaba_tool_targets() -> Vec<ProviderToolTarget> {
    ["model-studio-cn", "model-studio-singapore"]
        .into_iter()
        .map(alibaba_web_search_target)
        .collect()
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
            endpoint_path: String::from("/api/v1/mcps/WebSearch/mcp"),
            authentication: AuthenticationProjection::BearerAuthorization,
            adapter: ProviderToolAdapterKind::AlibabaModelStudioSearch,
        },
    }
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
    use crate::{ProviderToolCapabilityKind, ProviderToolOutputKind};

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
        let targets = built_in_provider_tool_targets("alibaba");
        assert_eq!(targets.len(), 2);
        assert!(targets.iter().all(|target| {
            target.contract.capability == ProviderToolCapabilityKind::WebSearch
                && target.contract.output.kind == ProviderToolOutputKind::Search
                && target.credential_profile_ids.len() == 1
                && target.credential_profile_ids.contains("api-key")
                && target.service_entry_id.starts_with("model-studio-")
        }));
        assert!(targets.iter().all(|target| {
            target.contract.input_schema["properties"]["query"].is_object()
                && target.contract.input_schema["properties"]["max_results"].is_object()
                && target.contract.input_schema["properties"]["include_domains"].is_null()
        }));
    }
}
