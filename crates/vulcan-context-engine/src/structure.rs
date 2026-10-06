//! Extensible structural analysis with a deterministic recursive fallback.
//! 可扩展结构分析以及确定性的递归回退实现。

use crate::chunking::{ChunkerError, RecursiveChunker, RecursiveChunkerConfig};
use crate::identity::is_sha256_identity;
use crate::model::{Chunk, ChunkKind};
use crate::symbol::{SymbolOccurrence, extract_symbols_from_chunks, occurrence_matches_chunk};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};

/// Identify whether semantic AST windows or the recursive fallback produced a result.
/// 标识结果由语义 AST 窗口还是递归回退生成。
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum StructureMode {
    /// A language-aware analyzer produced semantic AST windows.
    /// 语言感知分析器生成了语义 AST 窗口。
    SemanticAst,
    /// The deterministic recursive source splitter produced the windows.
    /// 确定性递归源码切分器生成了窗口。
    RecursiveFallback,
}

/// Complete structural output consumed by indexing before any provider call.
/// 在任何供应商调用前由索引消费的完整结构输出。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructureAnalysis {
    /// Stable analyzer implementation identity.
    /// 稳定的分析器实现身份。
    pub analyzer_id: String,
    /// Compatibility version that must match the configured chunker version.
    /// 必须与配置切块器版本匹配的兼容版本。
    pub version: String,
    /// Structural mode that produced the result.
    /// 生成结果的结构模式。
    pub mode: StructureMode,
    /// Optional primary-analyzer diagnostic when recursive fallback was required.
    /// 需要递归回退时可选的主分析器诊断。
    pub fallback_reason: Option<String>,
    /// Source-preserving code chunks in deterministic order.
    /// 按确定性顺序排列且保留源码的代码块。
    pub chunks: Vec<Chunk>,
    /// Exact symbol occurrences derived from the same chunks.
    /// 从同一批代码块派生的精确符号出现位置。
    pub symbols: Vec<SymbolOccurrence>,
}

/// Errors returned by a host-independent structural analyzer.
/// 不依赖宿主的结构分析器返回的错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StructureError {
    /// Recursive fallback construction or execution failed.
    /// 递归回退构造或执行失败。
    Chunking(ChunkerError),
    /// An analyzer returned a compatibility version different from the VCE vector space.
    /// 分析器返回了与 VCE 向量空间不同的兼容版本。
    VersionMismatch {
        /// Analyzer version required by the configured vector space.
        /// 配置向量空间要求的分析器版本。
        expected: String,
        /// Analyzer version returned by the implementation.
        /// 实现实际返回的分析器版本。
        actual: String,
    },
    /// A language-aware implementation rejected the source with an explicit diagnostic.
    /// 语言感知实现使用明确诊断拒绝了源码。
    AnalysisFailed {
        /// Explicit language-aware analyzer diagnostic.
        /// 明确的语言感知分析器诊断。
        message: String,
    },
}

impl Display for StructureError {
    /// Format one deterministic structural-analysis diagnostic.
    /// 格式化一项确定性的结构分析诊断。
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Chunking(error) => write!(formatter, "structural fallback failed: {error}"),
            Self::VersionMismatch { expected, actual } => write!(
                formatter,
                "structure analyzer version mismatch: expected `{expected}`, received `{actual}`"
            ),
            Self::AnalysisFailed { message } => {
                write!(formatter, "structural analysis failed: {message}")
            }
        }
    }
}

impl Error for StructureError {}

/// Produce semantic chunks and symbols without depending on MCP, CBM, storage, or AI services.
/// 在不依赖 MCP、CBM、存储或 AI 服务的情况下生成语义代码块和符号。
pub trait VceStructureAnalyzer: Send + Sync + Debug {
    /// Return the stable analyzer identity used by management diagnostics.
    /// 返回管理诊断使用的稳定分析器身份。
    fn analyzer_id(&self) -> &str;

    /// Return the compatibility version included in the vector-space fingerprint.
    /// 返回包含在向量空间指纹中的兼容版本。
    fn version(&self) -> &str;

    /// Analyze one immutable source Blob before Embedding.
    /// 在向量化前分析一个不可变源码 Blob。
    fn analyze(
        &self,
        blob_name: &str,
        path: &str,
        content: &str,
    ) -> Result<StructureAnalysis, StructureError>;
}

/// Built-in analyzer that guarantees recursive fallback and exact lexical symbols.
/// 保证递归回退和精确词法符号的内置分析器。
#[derive(Debug, Clone)]
pub struct RecursiveStructureAnalyzer {
    /// Recursive source Chunker used by the fallback implementation.
    /// 回退实现使用的递归源码切块器。
    chunker: RecursiveChunker,
    /// Compatibility version supplied by the VCE configuration.
    /// 由 VCE 配置提供的兼容版本。
    version: String,
}

/// Compose one semantic analyzer with a deterministic recursive safety fallback.
/// 将一个语义分析器与确定性的递归安全回退组合起来。
#[derive(Debug)]
pub struct FallbackStructureAnalyzer {
    /// Preferred language-aware analyzer.
    /// 首选的语言感知分析器。
    primary: Box<dyn VceStructureAnalyzer>,
    /// Deterministic recursive analyzer used after any primary failure.
    /// 主分析器发生任何失败后使用的确定性递归分析器。
    fallback: RecursiveStructureAnalyzer,
    /// Compatibility version shared by both analyzers.
    /// 两个分析器共享的兼容版本。
    version: String,
}

impl FallbackStructureAnalyzer {
    /// Construct a semantic-first analyzer whose two branches share one compatibility version.
    /// 构造两个分支共享同一兼容版本的语义优先分析器。
    pub fn new(
        primary: Box<dyn VceStructureAnalyzer>,
        fallback: RecursiveStructureAnalyzer,
    ) -> Result<Self, StructureError> {
        if primary.analyzer_id().trim().is_empty() {
            return Err(StructureError::AnalysisFailed {
                message: "primary analyzer identity is empty".to_owned(),
            });
        }
        if primary.version() != fallback.version() {
            return Err(StructureError::VersionMismatch {
                expected: fallback.version().to_owned(),
                actual: primary.version().to_owned(),
            });
        }
        let version = fallback.version().to_owned();
        Ok(Self {
            primary,
            fallback,
            version,
        })
    }
}

impl VceStructureAnalyzer for FallbackStructureAnalyzer {
    /// Return the stable semantic-first composition identity.
    /// 返回稳定的语义优先组合身份。
    fn analyzer_id(&self) -> &str {
        "vce-semantic-with-recursive-fallback"
    }

    /// Return the shared structural compatibility version.
    /// 返回共享的结构兼容版本。
    fn version(&self) -> &str {
        &self.version
    }

    /// Use semantic output when valid, otherwise return deterministic recursive windows.
    /// 语义输出有效时使用它，否则返回确定性的递归窗口。
    fn analyze(
        &self,
        blob_name: &str,
        path: &str,
        content: &str,
    ) -> Result<StructureAnalysis, StructureError> {
        let primary = self
            .primary
            .analyze(blob_name, path, content)
            .and_then(|analysis| {
                validate_structure_analysis_identity(
                    &analysis,
                    &self.version,
                    self.primary.analyzer_id(),
                )?;
                validate_semantic_source_coverage(&analysis, content)?;
                Ok(analysis)
            });
        match primary {
            Ok(analysis) => Ok(analysis),
            Err(error) => {
                let mut analysis = self.fallback.analyze(blob_name, path, content)?;
                validate_structure_analysis_identity(
                    &analysis,
                    &self.version,
                    self.fallback.analyzer_id(),
                )?;
                validate_semantic_source_coverage(&analysis, content)?;
                analysis.fallback_reason = Some(error.to_string());
                Ok(analysis)
            }
        }
    }
}

impl RecursiveStructureAnalyzer {
    /// Construct the built-in analyzer from explicit Chunker and compatibility configuration.
    /// 根据明确的切块器和兼容配置构造内置分析器。
    pub fn new(
        config: RecursiveChunkerConfig,
        version: impl Into<String>,
    ) -> Result<Self, StructureError> {
        let version = version.into();
        if version.trim().is_empty() {
            return Err(StructureError::VersionMismatch {
                expected: "non-empty version".to_owned(),
                actual: version,
            });
        }
        Ok(Self {
            chunker: RecursiveChunker::new(config).map_err(StructureError::Chunking)?,
            version,
        })
    }
}

impl VceStructureAnalyzer for RecursiveStructureAnalyzer {
    /// Return the built-in analyzer identity.
    /// 返回内置分析器身份。
    fn analyzer_id(&self) -> &str {
        "vce-recursive-structure"
    }

    /// Return the configured compatibility version.
    /// 返回配置的兼容版本。
    fn version(&self) -> &str {
        &self.version
    }

    /// Produce recursive source windows and lexical definition occurrences.
    /// 生成递归源码窗口和词法定义出现位置。
    fn analyze(
        &self,
        blob_name: &str,
        path: &str,
        content: &str,
    ) -> Result<StructureAnalysis, StructureError> {
        let chunks = self
            .chunker
            .chunk(blob_name.to_owned(), path, content)
            .map_err(StructureError::Chunking)?;
        let symbols = extract_symbols_from_chunks(&chunks);
        Ok(StructureAnalysis {
            analyzer_id: self.analyzer_id().to_owned(),
            version: self.version.clone(),
            mode: StructureMode::RecursiveFallback,
            fallback_reason: None,
            chunks,
            symbols,
        })
    }
}

/// Validate a structural result before it can affect vector compatibility.
/// 在结构结果影响向量兼容性前对其进行校验。
pub fn validate_structure_analysis(
    analysis: &StructureAnalysis,
    expected_version: &str,
) -> Result<(), StructureError> {
    if expected_version.trim().is_empty() || analysis.version != expected_version {
        return Err(StructureError::VersionMismatch {
            expected: expected_version.to_owned(),
            actual: analysis.version.clone(),
        });
    }
    if analysis.analyzer_id.trim().is_empty() {
        return Err(StructureError::AnalysisFailed {
            message: "analyzer identity is empty".to_owned(),
        });
    }
    if analysis
        .fallback_reason
        .as_ref()
        .is_some_and(|reason| reason.trim().is_empty())
    {
        return Err(StructureError::AnalysisFailed {
            message: "fallback diagnostic is empty".to_owned(),
        });
    }
    if analysis.mode == StructureMode::SemanticAst && analysis.fallback_reason.is_some() {
        return Err(StructureError::AnalysisFailed {
            message: "semantic analysis cannot carry a fallback diagnostic".to_owned(),
        });
    }
    let expected_kind = match analysis.mode {
        StructureMode::SemanticAst => ChunkKind::Ast,
        StructureMode::RecursiveFallback => ChunkKind::Recursive,
    };
    let mut chunk_ids = BTreeSet::new();
    let mut previous_order = None::<(usize, usize, usize, String)>;
    let mut source_identity = None::<(String, String)>;
    for chunk in &analysis.chunks {
        let canonical = Chunk::new_with_start_char_offset(
            chunk.blob_name.clone(),
            chunk.start_char_offset,
            &chunk.path,
            chunk.content.clone(),
            chunk.range.start_line,
            chunk.range.end_line,
            chunk.kind,
        )
        .map_err(|error| StructureError::AnalysisFailed {
            message: format!("structural Chunk `{}` is invalid: {error}", chunk.chunk_id),
        })?;
        let order = (
            chunk.range.start_line,
            chunk.start_char_offset,
            chunk.range.end_line,
            chunk.chunk_id.clone(),
        );
        let identity = (chunk.blob_name.clone(), chunk.path.clone());
        if chunk.kind != expected_kind
            || !is_sha256_identity(&chunk.blob_name)
            || &canonical != chunk
            || !chunk_ids.insert(chunk.chunk_id.clone())
            || previous_order
                .as_ref()
                .is_some_and(|previous| previous >= &order)
            || source_identity
                .as_ref()
                .is_some_and(|expected| expected != &identity)
        {
            return Err(StructureError::AnalysisFailed {
                message: format!(
                    "structural Chunk `{}` has inconsistent source identity",
                    chunk.chunk_id
                ),
            });
        }
        previous_order = Some(order);
        source_identity.get_or_insert(identity);
    }
    let mut occurrence_ids = BTreeSet::new();
    // Resolve chunk ownership once rather than linearly rescanning all chunks for every symbol.
    // 一次构建代码块归属，避免每个符号都线性重扫全部代码块。
    let chunks_by_id = analysis
        .chunks
        .iter()
        .map(|chunk| (chunk.chunk_id.as_str(), chunk))
        .collect::<std::collections::BTreeMap<_, _>>();
    for symbol in &analysis.symbols {
        let Some(chunk) = chunks_by_id.get(symbol.chunk_id.as_str()) else {
            return Err(StructureError::AnalysisFailed {
                message: format!(
                    "symbol occurrence `{}` references an unknown Chunk",
                    symbol.occurrence_id
                ),
            });
        };
        if !occurrence_matches_chunk(symbol, chunk)
            || !occurrence_ids.insert(symbol.occurrence_id.clone())
        {
            return Err(StructureError::AnalysisFailed {
                message: format!(
                    "symbol occurrence `{}` is inconsistent or duplicated",
                    symbol.occurrence_id
                ),
            });
        }
    }
    Ok(())
}

/// Validate structural output against the exact analyzer selected by the engine.
/// 针对引擎选定的精确分析器校验结构化输出。
pub(crate) fn validate_structure_analysis_identity(
    analysis: &StructureAnalysis,
    expected_version: &str,
    expected_analyzer_id: &str,
) -> Result<(), StructureError> {
    validate_structure_analysis(analysis, expected_version)?;
    if analysis.analyzer_id != expected_analyzer_id {
        return Err(StructureError::AnalysisFailed {
            message: format!(
                "analyzer identity mismatch: expected `{expected_analyzer_id}`, received `{}`",
                analysis.analyzer_id
            ),
        });
    }
    Ok(())
}

/// Require retrievable source text to produce at least one structural Chunk.
/// 要求可检索源码文本至少产生一个结构化代码块。
fn validate_semantic_source_coverage(
    analysis: &StructureAnalysis,
    content: &str,
) -> Result<(), StructureError> {
    if content.chars().any(char::is_alphanumeric) && analysis.chunks.is_empty() {
        return Err(StructureError::AnalysisFailed {
            message: "semantic source produced no structural Chunks".to_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Return a canonical Blob identity for standalone analyzer tests.
    /// 为独立分析器测试返回规范 Blob 身份。
    fn test_blob() -> String {
        crate::identity::content_hash("structure-test-blob")
    }

    /// Semantic analyzer used to verify deterministic recursive fallback.
    /// 用于验证确定性递归回退的语义分析器。
    #[derive(Debug)]
    struct FailingAnalyzer;

    impl VceStructureAnalyzer for FailingAnalyzer {
        /// Return the fake primary analyzer identity.
        /// 返回伪主分析器身份。
        fn analyzer_id(&self) -> &str {
            "failing-semantic"
        }

        /// Return the compatibility version shared with the fallback.
        /// 返回与回退分析器共享的兼容版本。
        fn version(&self) -> &str {
            "chunk-v1"
        }

        /// Return an explicit semantic parsing failure.
        /// 返回明确的语义解析失败。
        fn analyze(
            &self,
            _blob_name: &str,
            _path: &str,
            _content: &str,
        ) -> Result<StructureAnalysis, StructureError> {
            Err(StructureError::AnalysisFailed {
                message: "unsupported syntax".to_owned(),
            })
        }
    }

    /// Semantic analyzer that incorrectly drops all retrievable source content.
    /// 错误丢弃全部可检索源码内容的语义分析器。
    #[derive(Debug)]
    struct EmptyAnalyzer;

    impl VceStructureAnalyzer for EmptyAnalyzer {
        /// Return the fake empty analyzer identity.
        /// 返回伪空分析器身份。
        fn analyzer_id(&self) -> &str {
            "empty-semantic"
        }

        /// Return the compatibility version shared with the fallback.
        /// 返回与回退分析器共享的兼容版本。
        fn version(&self) -> &str {
            "chunk-v1"
        }

        /// Return an invalid empty semantic result for non-empty source.
        /// 为非空源码返回非法的空语义结果。
        fn analyze(
            &self,
            _blob_name: &str,
            _path: &str,
            _content: &str,
        ) -> Result<StructureAnalysis, StructureError> {
            Ok(StructureAnalysis {
                analyzer_id: self.analyzer_id().to_owned(),
                version: self.version().to_owned(),
                mode: StructureMode::SemanticAst,
                fallback_reason: None,
                chunks: Vec::new(),
                symbols: Vec::new(),
            })
        }
    }

    /// Verify the built-in analyzer provides fallback chunks and exact definitions together.
    /// 验证内置分析器同时提供回退代码块和精确定义。
    #[test]
    fn recursive_analyzer_produces_chunks_and_symbols() {
        let analyzer =
            RecursiveStructureAnalyzer::new(RecursiveChunkerConfig::default(), "chunk-v1").unwrap();
        let analysis = analyzer
            .analyze(&test_blob(), "src/lib.rs", "pub fn run() {}")
            .unwrap();
        assert_eq!(analysis.mode, StructureMode::RecursiveFallback);
        assert_eq!(analysis.chunks.len(), 1);
        assert_eq!(analysis.symbols[0].identifier, "run");
    }

    /// Verify analyzer compatibility versions are validated explicitly.
    /// 验证分析器兼容版本会被显式校验。
    #[test]
    fn analysis_version_must_match_vector_space() {
        let analyzer =
            RecursiveStructureAnalyzer::new(RecursiveChunkerConfig::default(), "chunk-v1").unwrap();
        let analysis = analyzer
            .analyze(&test_blob(), "src/lib.rs", "pub fn run() {}")
            .unwrap();
        assert!(matches!(
            validate_structure_analysis(&analysis, "chunk-v2"),
            Err(StructureError::VersionMismatch { .. })
        ));
    }

    /// Verify a structural analyzer cannot forge exact symbol occurrence records.
    /// 验证结构分析器不能伪造精确符号出现记录。
    #[test]
    fn analysis_rejects_forged_symbol_occurrence() {
        let analyzer =
            RecursiveStructureAnalyzer::new(RecursiveChunkerConfig::default(), "chunk-v1").unwrap();
        let mut analysis = analyzer
            .analyze(&test_blob(), "src/lib.rs", "pub fn run() {}")
            .unwrap();
        analysis.symbols[0].occurrence_id = crate::identity::content_hash("forged");

        assert!(matches!(
            validate_structure_analysis(&analysis, "chunk-v1"),
            Err(StructureError::AnalysisFailed { .. })
        ));

        let chunk = &analysis.chunks[0];
        analysis.symbols[0] = SymbolOccurrence::from_chunk(
            chunk,
            "absent_identifier",
            crate::symbol::SymbolKind::Definition,
        );
        assert!(matches!(
            validate_structure_analysis(&analysis, "chunk-v1"),
            Err(StructureError::AnalysisFailed { .. })
        ));
    }

    /// Verify structural analyzers cannot inject synthetic path-index documents as source Chunks.
    /// 验证结构分析器不能把合成路径索引文档注入为源码块。
    #[test]
    fn analysis_rejects_path_document_chunks() {
        let analyzer =
            RecursiveStructureAnalyzer::new(RecursiveChunkerConfig::default(), "chunk-v1").unwrap();
        let mut analysis = analyzer
            .analyze(&test_blob(), "src/lib.rs", "pub fn run() {}")
            .unwrap();
        analysis.symbols.clear();
        analysis.chunks[0].kind = ChunkKind::Path;

        assert!(matches!(
            validate_structure_analysis(&analysis, "chunk-v1"),
            Err(StructureError::AnalysisFailed { .. })
        ));
    }

    /// Verify structural mode and source Chunk provenance cannot contradict each other.
    /// 验证结构模式与源码 Chunk 来源不能互相矛盾。
    #[test]
    fn analysis_mode_must_match_chunk_kind() {
        let analyzer =
            RecursiveStructureAnalyzer::new(RecursiveChunkerConfig::default(), "chunk-v1").unwrap();
        let mut analysis = analyzer
            .analyze(&test_blob(), "src/lib.rs", "pub fn run() {}")
            .unwrap();
        analysis.chunks[0].kind = ChunkKind::Ast;

        assert!(matches!(
            validate_structure_analysis(&analysis, "chunk-v1"),
            Err(StructureError::AnalysisFailed { .. })
        ));
    }

    /// Verify structural analysis output is ordered by canonical source location.
    /// 验证结构分析输出按规范源码位置排序。
    #[test]
    fn analysis_chunks_must_use_canonical_source_order() {
        let blob = test_blob();
        let first = Chunk::new(
            blob.clone(),
            "src/lib.rs",
            "fn first() {}\n",
            1,
            1,
            ChunkKind::Recursive,
        )
        .unwrap();
        let second = Chunk::new(
            blob,
            "src/lib.rs",
            "fn second() {}",
            2,
            2,
            ChunkKind::Recursive,
        )
        .unwrap();
        let analysis = StructureAnalysis {
            analyzer_id: "ordered-analyzer".to_owned(),
            version: "chunk-v1".to_owned(),
            mode: StructureMode::RecursiveFallback,
            fallback_reason: None,
            chunks: vec![second, first],
            symbols: Vec::new(),
        };

        assert!(matches!(
            validate_structure_analysis(&analysis, "chunk-v1"),
            Err(StructureError::AnalysisFailed { .. })
        ));
    }

    /// Verify an analyzer cannot return data under another implementation identity.
    /// 验证分析器不能以另一个实现身份返回数据。
    #[test]
    fn analysis_identity_must_match_selected_analyzer() {
        let analyzer =
            RecursiveStructureAnalyzer::new(RecursiveChunkerConfig::default(), "chunk-v1").unwrap();
        let mut analysis = analyzer
            .analyze(&test_blob(), "src/lib.rs", "pub fn run() {}")
            .unwrap();
        analysis.analyzer_id = "different-analyzer".to_owned();

        assert!(matches!(
            validate_structure_analysis_identity(&analysis, "chunk-v1", analyzer.analyzer_id()),
            Err(StructureError::AnalysisFailed { .. })
        ));
    }

    /// Verify semantic parser failures produce recursive windows with an observable reason.
    /// 验证语义解析失败会生成递归窗口并保留可观测原因。
    #[test]
    fn semantic_failure_uses_recursive_fallback() {
        let fallback =
            RecursiveStructureAnalyzer::new(RecursiveChunkerConfig::default(), "chunk-v1").unwrap();
        let analyzer = FallbackStructureAnalyzer::new(Box::new(FailingAnalyzer), fallback).unwrap();
        let analysis = analyzer
            .analyze(&test_blob(), "src/lib.rs", "pub fn run() {}")
            .unwrap();
        assert_eq!(analysis.mode, StructureMode::RecursiveFallback);
        assert_eq!(analysis.chunks.len(), 1);
        assert!(
            analysis
                .fallback_reason
                .as_deref()
                .is_some_and(|reason| reason.contains("unsupported syntax"))
        );
    }

    /// Verify empty semantic output for retrievable source also activates fallback.
    /// 验证可检索源码的空语义输出同样会触发回退。
    #[test]
    fn empty_semantic_output_uses_recursive_fallback() {
        let fallback =
            RecursiveStructureAnalyzer::new(RecursiveChunkerConfig::default(), "chunk-v1").unwrap();
        let analyzer = FallbackStructureAnalyzer::new(Box::new(EmptyAnalyzer), fallback).unwrap();
        let analysis = analyzer
            .analyze(&test_blob(), "src/lib.rs", "pub fn run() {}")
            .unwrap();

        assert_eq!(analysis.mode, StructureMode::RecursiveFallback);
        assert_eq!(analysis.chunks.len(), 1);
        assert!(
            analysis
                .fallback_reason
                .as_deref()
                .is_some_and(|reason| reason.contains("produced no structural Chunks"))
        );
    }
}
