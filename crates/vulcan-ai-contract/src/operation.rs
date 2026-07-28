//! Public AI operation identities.
//! 公共 AI 操作身份。

use serde::{Deserialize, Serialize};

/// Identifies one provider-neutral AI operation.
/// 标识一个供应商无关的 AI 操作。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiOperation {
    /// Produces one conversational model response.
    /// 生成一次对话模型响应。
    ConversationRespond,
    /// Understands one or more images.
    /// 理解一张或多张图片。
    ImageUnderstand,
    /// Generates one or more images.
    /// 生成一张或多张图片。
    ImageGenerate,
    /// Edits one or more images.
    /// 编辑一张或多张图片。
    ImageEdit,
    /// Understands one or more videos.
    /// 理解一个或多个视频。
    VideoUnderstand,
    /// Generates one video.
    /// 生成一个视频。
    VideoGenerate,
    /// Edits an existing video.
    /// 编辑已有视频。
    VideoEdit,
    /// Extends an existing video.
    /// 延长已有视频。
    VideoExtend,
    /// Understands audio content.
    /// 理解音频内容。
    AudioUnderstand,
    /// Synthesizes speech audio.
    /// 合成语音音频。
    SpeechSynthesize,
    /// Transcribes speech audio.
    /// 转录语音音频。
    SpeechTranscribe,
    /// Clones one voice from a reference.
    /// 从参考音频克隆声音。
    VoiceClone,
    /// Designs one voice from semantic controls.
    /// 根据语义控制设计声音。
    VoiceDesign,
    /// Generates music audio.
    /// 生成音乐音频。
    MusicGenerate,
    /// Produces a cover version of existing music.
    /// 生成已有音乐的翻唱版本。
    MusicCover,
    /// Generates song lyrics.
    /// 生成歌词。
    LyricsGenerate,
    /// Produces vector embeddings.
    /// 生成向量嵌入。
    EmbeddingCreate,
    /// Reranks candidate documents.
    /// 对候选文档进行重排。
    RerankDocuments,
    /// Searches remote content.
    /// 搜索远程内容。
    WebSearch,
    /// Extracts structured content.
    /// 抽取结构化内容。
    WebExtract,
}
