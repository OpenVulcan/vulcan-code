use vulcan_tools_manager::{
    IncrementalJsonEvent, IncrementalJsonPathSegment, IncrementalJsonScalar,
};

/// Root-level consumption state for one JSON document.
/// 单个 JSON 文档的根级消费状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RootPhase {
    /// The document has not started its single root value.
    /// 文档尚未开始其唯一根值。
    Value,
    /// The root value has started and no second value is permitted.
    /// 根值已开始，不允许出现第二个值。
    Complete,
}

/// Grammar state for one open JSON object.
/// 单个已打开 JSON 对象的语法状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ObjectPhase {
    /// The object accepts its first key or an immediate close.
    /// 对象接受第一个键或立即闭合。
    FirstKeyOrEnd,
    /// The object requires a key after a comma.
    /// 对象在逗号后必须接收键。
    Key,
    /// The object requires a colon after a completed key.
    /// 对象在完成键后必须接收冒号。
    Colon,
    /// The object requires the value belonging to its pending key.
    /// 对象必须接收属于待处理键的值。
    Value,
    /// The object requires a comma or close after a value.
    /// 对象在值之后必须接收逗号或闭合符。
    CommaOrEnd,
}

/// Grammar state for one open JSON array.
/// 单个已打开 JSON 数组的语法状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArrayPhase {
    /// The array accepts its first value or an immediate close.
    /// 数组接受第一个值或立即闭合。
    FirstValueOrEnd,
    /// The array requires another value after a comma.
    /// 数组在逗号后必须接收另一个值。
    Value,
    /// The array requires a comma or close after a value.
    /// 数组在值之后必须接收逗号或闭合符。
    CommaOrEnd,
}

/// One open structured JSON container and its canonical path.
/// 一个已打开的结构化 JSON 容器及其规范路径。
#[derive(Debug)]
enum ContainerFrame {
    /// Object frame with its pending decoded member key.
    /// 带有待处理已解码成员键的对象帧。
    Object {
        /// Canonical path of the object value.
        /// 对象值的规范路径。
        path: Vec<IncrementalJsonPathSegment>,
        /// Current object grammar phase.
        /// 当前对象语法阶段。
        phase: ObjectPhase,
        /// Fully decoded key waiting for its value.
        /// 等待对应值的完整解码键。
        pending_key: Option<String>,
    },
    /// Array frame with the next zero-based item index.
    /// 带有下一个从零开始元素索引的数组帧。
    Array {
        /// Canonical path of the array value.
        /// 数组值的规范路径。
        path: Vec<IncrementalJsonPathSegment>,
        /// Current array grammar phase.
        /// 当前数组语法阶段。
        phase: ArrayPhase,
        /// Index assigned to the next value.
        /// 分配给下一个值的索引。
        next_index: usize,
    },
}

/// Semantic role of one active JSON string token.
/// 单个活动 JSON 字符串 Token 的语义角色。
#[derive(Debug)]
enum StringRole {
    /// Object key owned by the object at the supplied path.
    /// 由给定路径对象拥有的对象键。
    ObjectKey {
        /// Canonical path of the owning object.
        /// 所属对象的规范路径。
        object_path: Vec<IncrementalJsonPathSegment>,
        /// Full decoded key retained until the closing quote.
        /// 保留到结束引号的完整解码键。
        decoded_key: String,
    },
    /// String value located at the supplied canonical path.
    /// 位于给定规范路径的字符串值。
    Value {
        /// Canonical path of the string value.
        /// 字符串值的规范路径。
        path: Vec<IncrementalJsonPathSegment>,
        /// Decoded content accumulated only for the current input fragment.
        /// 仅为当前输入碎片累计的已解码内容。
        pending_content: String,
    },
}

/// Escape-decoding state retained across provider fragments.
/// 跨 Provider 碎片保留的转义解码状态。
#[derive(Debug)]
enum StringEscapeState {
    /// Normal decoded string content.
    /// 普通已解码字符串内容。
    Normal,
    /// A backslash was received and the escape designator is pending.
    /// 已收到反斜杠，正在等待转义指示符。
    AfterBackslash,
    /// A four-digit Unicode escape is being accumulated.
    /// 正在累计四位 Unicode 转义。
    Unicode {
        /// Hexadecimal digits received so far.
        /// 当前已接收的十六进制数字。
        digits: String,
    },
    /// A high surrogate requires the backslash that starts its low surrogate.
    /// 高代理项正在等待低代理项开头的反斜杠。
    LowSurrogateBackslash {
        /// Parsed high-surrogate code unit.
        /// 已解析的高代理代码单元。
        high: u16,
    },
    /// A high surrogate has received the next backslash and requires `u`.
    /// 高代理项已收到下一个反斜杠，正在等待 `u`。
    LowSurrogateU {
        /// Parsed high-surrogate code unit.
        /// 已解析的高代理代码单元。
        high: u16,
    },
    /// The four hexadecimal digits of a low surrogate are being accumulated.
    /// 正在累计低代理项的四位十六进制数字。
    LowSurrogateUnicode {
        /// Parsed high-surrogate code unit.
        /// 已解析的高代理代码单元。
        high: u16,
        /// Low-surrogate hexadecimal digits received so far.
        /// 当前已接收的低代理项十六进制数字。
        digits: String,
    },
}

/// One active lexical token whose delimiter may arrive in a later fragment.
/// 一个活动词法 Token，其分隔符可能在后续碎片到达。
#[derive(Debug)]
enum ActiveToken {
    /// JSON string with persistent escape state.
    /// 带有持久转义状态的 JSON 字符串。
    String {
        /// Semantic role of the string.
        /// 字符串的语义角色。
        role: StringRole,
        /// Current cross-fragment escape state.
        /// 当前跨碎片转义状态。
        escape: StringEscapeState,
    },
    /// JSON number retained as exact source text until validation.
    /// 在校验前以精确源文本保留的 JSON 数字。
    Number {
        /// Canonical path of the number value.
        /// 数字值的规范路径。
        path: Vec<IncrementalJsonPathSegment>,
        /// Exact number source received so far.
        /// 当前已接收的精确数字源文本。
        source: String,
    },
    /// JSON `true`, `false`, or `null` token.
    /// JSON `true`、`false` 或 `null` Token。
    Literal {
        /// Canonical path of the literal value.
        /// 字面量值的规范路径。
        path: Vec<IncrementalJsonPathSegment>,
        /// Required exact literal source.
        /// 必需的精确字面量源文本。
        expected: &'static str,
        /// Number of expected characters already consumed.
        /// 已消费的预期字符数量。
        consumed: usize,
    },
}

/// Strict incremental lexer for one streamed JSON tool-argument document.
/// 单个流式 JSON 工具参数文档使用的严格增量词法器。
#[derive(Debug)]
pub(super) struct IncrementalJsonLexer {
    /// Root-document grammar state.
    /// 根文档语法状态。
    root_phase: RootPhase,
    /// Open object and array frames.
    /// 已打开的对象与数组帧。
    containers: Vec<ContainerFrame>,
    /// Active scalar or string token.
    /// 活动标量或字符串 Token。
    token: Option<ActiveToken>,
    /// First permanent lexical failure, retained to prevent state revival.
    /// 首个永久词法失败，用于防止状态复活。
    failure: Option<String>,
}

impl Default for IncrementalJsonLexer {
    /// Builds an empty lexer awaiting exactly one JSON root value.
    /// 构建一个等待唯一 JSON 根值的空词法器。
    ///
    /// # Returns
    /// 返回值
    /// A fresh strict incremental lexer.
    /// 返回一个全新的严格增量词法器。
    fn default() -> Self {
        Self {
            root_phase: RootPhase::Value,
            containers: Vec::new(),
            token: None,
            failure: None,
        }
    }
}

impl IncrementalJsonLexer {
    /// Consumes one ordered raw JSON fragment and emits completed semantic events.
    /// 消费一段有序原始 JSON 碎片并发出已完成的语义事件。
    ///
    /// # Parameters
    /// 参数
    /// `fragment` is the next exact UTF-8 text fragment from the provider adapter.
    /// `fragment` 是 Provider 适配器提供的下一段精确 UTF-8 文本碎片。
    ///
    /// # Returns
    /// 返回值
    /// Ordered semantic events or the first permanent English lexical diagnostic.
    /// 返回有序语义事件或首个永久英文词法诊断。
    pub(super) fn push(&mut self, fragment: &str) -> Result<Vec<IncrementalJsonEvent>, String> {
        if let Some(failure) = &self.failure {
            return Err(failure.clone());
        }
        // events contains only completed semantic boundaries and decoded value fragments.
        // events 仅包含已完成的语义边界和已解码值碎片。
        let mut events = Vec::new();
        // characters allows the current delimiter to be reprocessed after scalar completion.
        // characters 允许在标量完成后重新处理当前分隔符。
        let characters = fragment.chars().collect::<Vec<_>>();
        // index advances only when the current character has been fully consumed.
        // index 仅在当前字符被完全消费后推进。
        let mut index = 0;
        while index < characters.len() {
            let character = characters[index];
            let consumed = match self.consume_character(character, &mut events) {
                Ok(consumed) => consumed,
                Err(error) => {
                    self.failure = Some(error.clone());
                    return Err(error);
                }
            };
            if consumed {
                index += 1;
            }
        }
        self.flush_string_content(&mut events);
        Ok(events)
    }

    /// Confirms that the streamed document ended at one complete legal JSON boundary.
    /// 确认流式文档在一个完整合法 JSON 边界结束。
    ///
    /// # Returns
    /// 返回值
    /// Any final scalar event, or an English incomplete/invalid-input diagnostic.
    /// 返回最终标量事件，或英文不完整/无效输入诊断。
    pub(super) fn finish(&mut self) -> Result<Vec<IncrementalJsonEvent>, String> {
        if let Some(failure) = &self.failure {
            return Err(failure.clone());
        }
        // events receives a number or literal whose delimiter is the end of input.
        // events 接收以输入结尾作为分隔符的数字或字面量。
        let mut events = Vec::new();
        match self.token.take() {
            Some(ActiveToken::Number { path, source }) => {
                events.push(number_event(path, source)?);
            }
            Some(ActiveToken::Literal {
                path,
                expected,
                consumed,
            }) if consumed == expected.len() => {
                events.push(literal_event(path, expected));
            }
            Some(token) => {
                self.token = Some(token);
                return Err(String::from(
                    "Tool request arguments ended inside an incomplete JSON token.",
                ));
            }
            None => {}
        }
        if !self.containers.is_empty() {
            return Err(String::from(
                "Tool request arguments ended before all JSON containers were closed.",
            ));
        }
        if self.root_phase != RootPhase::Complete {
            return Err(String::from(
                "Tool request arguments did not contain one complete JSON value.",
            ));
        }
        Ok(events)
    }

    /// Consumes or deliberately reprocesses one character according to current token state.
    /// 根据当前 Token 状态消费或有意重新处理一个字符。
    fn consume_character(
        &mut self,
        character: char,
        events: &mut Vec<IncrementalJsonEvent>,
    ) -> Result<bool, String> {
        match self.token.take() {
            Some(ActiveToken::String { role, escape }) => {
                self.consume_string_character(character, role, escape, events)?;
                Ok(true)
            }
            Some(ActiveToken::Number { path, mut source }) => {
                if number_character_is_possible(character) {
                    source.push(character);
                    self.token = Some(ActiveToken::Number { path, source });
                    return Ok(true);
                }
                if json_value_delimiter(character) {
                    events.push(number_event(path, source)?);
                    return Ok(false);
                }
                Err(format!(
                    "Invalid character `{character}` after JSON number in tool request arguments."
                ))
            }
            Some(ActiveToken::Literal {
                path,
                expected,
                mut consumed,
            }) => {
                if consumed < expected.len() {
                    let expected_character = expected.as_bytes()[consumed] as char;
                    if character != expected_character {
                        return Err(format!(
                            "Invalid JSON literal in tool request arguments; expected `{expected}`."
                        ));
                    }
                    consumed += 1;
                    self.token = Some(ActiveToken::Literal {
                        path,
                        expected,
                        consumed,
                    });
                    return Ok(true);
                }
                if json_value_delimiter(character) {
                    events.push(literal_event(path, expected));
                    return Ok(false);
                }
                Err(format!(
                    "Invalid character `{character}` after JSON literal in tool request arguments."
                ))
            }
            None => self.consume_structural_character(character, events),
        }
    }

    /// Consumes one character while no scalar or string token is active.
    /// 在没有活动标量或字符串 Token 时消费一个字符。
    fn consume_structural_character(
        &mut self,
        character: char,
        events: &mut Vec<IncrementalJsonEvent>,
    ) -> Result<bool, String> {
        if character.is_whitespace() {
            return Ok(true);
        }
        if let Some(container) = self.containers.last_mut() {
            match container {
                ContainerFrame::Object {
                    path,
                    phase,
                    pending_key,
                } => match *phase {
                    ObjectPhase::FirstKeyOrEnd if character == '}' => {
                        let path = path.clone();
                        self.containers.pop();
                        events.push(IncrementalJsonEvent::ObjectEnded { path });
                        return Ok(true);
                    }
                    ObjectPhase::FirstKeyOrEnd | ObjectPhase::Key if character == '"' => {
                        self.token = Some(ActiveToken::String {
                            role: StringRole::ObjectKey {
                                object_path: path.clone(),
                                decoded_key: String::new(),
                            },
                            escape: StringEscapeState::Normal,
                        });
                        return Ok(true);
                    }
                    ObjectPhase::Colon if character == ':' => {
                        *phase = ObjectPhase::Value;
                        return Ok(true);
                    }
                    ObjectPhase::CommaOrEnd if character == ',' => {
                        *phase = ObjectPhase::Key;
                        return Ok(true);
                    }
                    ObjectPhase::CommaOrEnd if character == '}' => {
                        let path = path.clone();
                        self.containers.pop();
                        events.push(IncrementalJsonEvent::ObjectEnded { path });
                        return Ok(true);
                    }
                    ObjectPhase::Value => {
                        let value_path = path_with_key(
                            path,
                            pending_key.as_ref().ok_or_else(|| {
                                String::from("JSON object value is missing its decoded key.")
                            })?,
                        );
                        *phase = ObjectPhase::CommaOrEnd;
                        return self.start_value(character, value_path, events);
                    }
                    _ => {}
                },
                ContainerFrame::Array {
                    path,
                    phase,
                    next_index,
                } => match *phase {
                    ArrayPhase::FirstValueOrEnd if character == ']' => {
                        let path = path.clone();
                        self.containers.pop();
                        events.push(IncrementalJsonEvent::ArrayEnded { path });
                        return Ok(true);
                    }
                    ArrayPhase::FirstValueOrEnd | ArrayPhase::Value => {
                        let value_path = path_with_index(path, *next_index);
                        *next_index = next_index.saturating_add(1);
                        *phase = ArrayPhase::CommaOrEnd;
                        return self.start_value(character, value_path, events);
                    }
                    ArrayPhase::CommaOrEnd if character == ',' => {
                        *phase = ArrayPhase::Value;
                        return Ok(true);
                    }
                    ArrayPhase::CommaOrEnd if character == ']' => {
                        let path = path.clone();
                        self.containers.pop();
                        events.push(IncrementalJsonEvent::ArrayEnded { path });
                        return Ok(true);
                    }
                    _ => {}
                },
            }
            return Err(format!(
                "Unexpected character `{character}` in streamed JSON tool request arguments."
            ));
        }
        match self.root_phase {
            RootPhase::Value => {
                self.root_phase = RootPhase::Complete;
                self.start_value(character, Vec::new(), events)
            }
            RootPhase::Complete => Err(String::from(
                "Tool request arguments contain more than one JSON root value.",
            )),
        }
    }

    /// Starts one value token or structured container at its already-determined path.
    /// 在已确定路径启动一个值 Token 或结构化容器。
    fn start_value(
        &mut self,
        character: char,
        path: Vec<IncrementalJsonPathSegment>,
        events: &mut Vec<IncrementalJsonEvent>,
    ) -> Result<bool, String> {
        match character {
            '{' => {
                events.push(IncrementalJsonEvent::ObjectStarted { path: path.clone() });
                self.containers.push(ContainerFrame::Object {
                    path,
                    phase: ObjectPhase::FirstKeyOrEnd,
                    pending_key: None,
                });
            }
            '[' => {
                events.push(IncrementalJsonEvent::ArrayStarted { path: path.clone() });
                self.containers.push(ContainerFrame::Array {
                    path,
                    phase: ArrayPhase::FirstValueOrEnd,
                    next_index: 0,
                });
            }
            '"' => {
                events.push(IncrementalJsonEvent::StringStarted { path: path.clone() });
                self.token = Some(ActiveToken::String {
                    role: StringRole::Value {
                        path,
                        pending_content: String::new(),
                    },
                    escape: StringEscapeState::Normal,
                });
            }
            '-' | '0'..='9' => {
                self.token = Some(ActiveToken::Number {
                    path,
                    source: character.to_string(),
                });
            }
            't' => {
                self.token = Some(ActiveToken::Literal {
                    path,
                    expected: "true",
                    consumed: 1,
                });
            }
            'f' => {
                self.token = Some(ActiveToken::Literal {
                    path,
                    expected: "false",
                    consumed: 1,
                });
            }
            'n' => {
                self.token = Some(ActiveToken::Literal {
                    path,
                    expected: "null",
                    consumed: 1,
                });
            }
            _ => {
                return Err(format!(
                    "Unexpected value-start character `{character}` in tool request arguments."
                ));
            }
        }
        Ok(true)
    }

    /// Decodes one character of an active JSON string and preserves cross-fragment escape state.
    /// 解码活动 JSON 字符串的一个字符并保留跨碎片转义状态。
    fn consume_string_character(
        &mut self,
        character: char,
        mut role: StringRole,
        escape: StringEscapeState,
        events: &mut Vec<IncrementalJsonEvent>,
    ) -> Result<(), String> {
        match escape {
            StringEscapeState::Normal => match character {
                '"' => self.finish_string(role, events),
                '\\' => {
                    self.token = Some(ActiveToken::String {
                        role,
                        escape: StringEscapeState::AfterBackslash,
                    });
                    Ok(())
                }
                character if character <= '\u{001F}' => Err(String::from(
                    "Unescaped control character in streamed JSON string.",
                )),
                character => {
                    append_decoded_character(&mut role, character);
                    self.token = Some(ActiveToken::String {
                        role,
                        escape: StringEscapeState::Normal,
                    });
                    Ok(())
                }
            },
            StringEscapeState::AfterBackslash => {
                if character == 'u' {
                    self.token = Some(ActiveToken::String {
                        role,
                        escape: StringEscapeState::Unicode {
                            digits: String::new(),
                        },
                    });
                    return Ok(());
                }
                let decoded = match character {
                    '"' => '"',
                    '\\' => '\\',
                    '/' => '/',
                    'b' => '\u{0008}',
                    'f' => '\u{000C}',
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    _ => {
                        return Err(format!(
                            "Invalid JSON escape `\\{character}` in tool request arguments."
                        ));
                    }
                };
                append_decoded_character(&mut role, decoded);
                self.token = Some(ActiveToken::String {
                    role,
                    escape: StringEscapeState::Normal,
                });
                Ok(())
            }
            StringEscapeState::Unicode { mut digits } => {
                push_hex_digit(&mut digits, character)?;
                if digits.len() < 4 {
                    self.token = Some(ActiveToken::String {
                        role,
                        escape: StringEscapeState::Unicode { digits },
                    });
                    return Ok(());
                }
                let code_unit = parse_hex_code_unit(&digits)?;
                if (0xD800..=0xDBFF).contains(&code_unit) {
                    self.token = Some(ActiveToken::String {
                        role,
                        escape: StringEscapeState::LowSurrogateBackslash { high: code_unit },
                    });
                    return Ok(());
                }
                if (0xDC00..=0xDFFF).contains(&code_unit) {
                    return Err(String::from(
                        "JSON Unicode escape contains an unpaired low surrogate.",
                    ));
                }
                let decoded = char::from_u32(u32::from(code_unit)).ok_or_else(|| {
                    String::from("JSON Unicode escape is not a valid scalar value.")
                })?;
                append_decoded_character(&mut role, decoded);
                self.token = Some(ActiveToken::String {
                    role,
                    escape: StringEscapeState::Normal,
                });
                Ok(())
            }
            StringEscapeState::LowSurrogateBackslash { high } => {
                if character != '\\' {
                    return Err(String::from(
                        "JSON high surrogate is not followed by a low-surrogate escape.",
                    ));
                }
                self.token = Some(ActiveToken::String {
                    role,
                    escape: StringEscapeState::LowSurrogateU { high },
                });
                Ok(())
            }
            StringEscapeState::LowSurrogateU { high } => {
                if character != 'u' {
                    return Err(String::from(
                        "JSON high surrogate is not followed by a low-surrogate Unicode escape.",
                    ));
                }
                self.token = Some(ActiveToken::String {
                    role,
                    escape: StringEscapeState::LowSurrogateUnicode {
                        high,
                        digits: String::new(),
                    },
                });
                Ok(())
            }
            StringEscapeState::LowSurrogateUnicode { high, mut digits } => {
                push_hex_digit(&mut digits, character)?;
                if digits.len() < 4 {
                    self.token = Some(ActiveToken::String {
                        role,
                        escape: StringEscapeState::LowSurrogateUnicode { high, digits },
                    });
                    return Ok(());
                }
                let low = parse_hex_code_unit(&digits)?;
                if !(0xDC00..=0xDFFF).contains(&low) {
                    return Err(String::from(
                        "JSON high surrogate is followed by an invalid low surrogate.",
                    ));
                }
                let scalar =
                    0x10000 + ((u32::from(high) - 0xD800) << 10) + (u32::from(low) - 0xDC00);
                let decoded = char::from_u32(scalar).ok_or_else(|| {
                    String::from("JSON surrogate pair is not a valid Unicode scalar value.")
                })?;
                append_decoded_character(&mut role, decoded);
                self.token = Some(ActiveToken::String {
                    role,
                    escape: StringEscapeState::Normal,
                });
                Ok(())
            }
        }
    }

    /// Completes one key or value string and updates the owning grammar frame.
    /// 完成一个键或值字符串并更新所属语法帧。
    fn finish_string(
        &mut self,
        mut role: StringRole,
        events: &mut Vec<IncrementalJsonEvent>,
    ) -> Result<(), String> {
        match &mut role {
            StringRole::ObjectKey {
                object_path,
                decoded_key,
            } => {
                let Some(ContainerFrame::Object {
                    path,
                    phase,
                    pending_key,
                }) = self.containers.last_mut()
                else {
                    return Err(String::from(
                        "Completed JSON object key has no owning object frame.",
                    ));
                };
                if path != object_path
                    || !matches!(*phase, ObjectPhase::FirstKeyOrEnd | ObjectPhase::Key)
                {
                    return Err(String::from(
                        "Completed JSON object key does not match the active object frame.",
                    ));
                }
                *pending_key = Some(decoded_key.clone());
                *phase = ObjectPhase::Colon;
                events.push(IncrementalJsonEvent::ObjectKey {
                    object_path: object_path.clone(),
                    key: decoded_key.clone(),
                });
            }
            StringRole::Value {
                path,
                pending_content,
            } => {
                if !pending_content.is_empty() {
                    events.push(IncrementalJsonEvent::StringContent {
                        path: path.clone(),
                        content: std::mem::take(pending_content),
                    });
                }
                events.push(IncrementalJsonEvent::StringEnded { path: path.clone() });
            }
        }
        Ok(())
    }

    /// Flushes decoded value content at the end of the current provider fragment.
    /// 在当前 Provider 碎片结束时刷新已解码值内容。
    fn flush_string_content(&mut self, events: &mut Vec<IncrementalJsonEvent>) {
        let Some(ActiveToken::String {
            role:
                StringRole::Value {
                    path,
                    pending_content,
                },
            ..
        }) = self.token.as_mut()
        else {
            return;
        };
        if pending_content.is_empty() {
            return;
        }
        events.push(IncrementalJsonEvent::StringContent {
            path: path.clone(),
            content: std::mem::take(pending_content),
        });
    }
}

/// Appends one decoded character to a key buffer or current value fragment.
/// 向键缓冲或当前值碎片追加一个已解码字符。
fn append_decoded_character(role: &mut StringRole, character: char) {
    match role {
        StringRole::ObjectKey { decoded_key, .. } => decoded_key.push(character),
        StringRole::Value {
            pending_content, ..
        } => pending_content.push(character),
    }
}

/// Extends one object path with its exact decoded member key.
/// 使用精确解码成员键扩展对象路径。
fn path_with_key(
    path: &[IncrementalJsonPathSegment],
    key: &str,
) -> Vec<IncrementalJsonPathSegment> {
    let mut result = path.to_vec();
    result.push(IncrementalJsonPathSegment::Key(key.to_string()));
    result
}

/// Extends one array path with its exact zero-based item index.
/// 使用精确的从零开始元素索引扩展数组路径。
fn path_with_index(
    path: &[IncrementalJsonPathSegment],
    index: usize,
) -> Vec<IncrementalJsonPathSegment> {
    let mut result = path.to_vec();
    result.push(IncrementalJsonPathSegment::Index(index));
    result
}

/// Returns whether one character can still belong to a JSON number token.
/// 返回单个字符是否仍可能属于 JSON 数字 Token。
fn number_character_is_possible(character: char) -> bool {
    character.is_ascii_digit() || matches!(character, '-' | '+' | '.' | 'e' | 'E')
}

/// Returns whether one character may delimit a completed JSON scalar value.
/// 返回单个字符是否可以分隔已完成的 JSON 标量值。
fn json_value_delimiter(character: char) -> bool {
    character.is_whitespace() || matches!(character, ',' | ']' | '}')
}

/// Validates an exact JSON number source and builds its semantic completion event.
/// 校验精确 JSON 数字源文本并构建其语义完成事件。
fn number_event(
    path: Vec<IncrementalJsonPathSegment>,
    source: String,
) -> Result<IncrementalJsonEvent, String> {
    let parsed = serde_json::from_str::<serde_json::Value>(&source).map_err(|error| {
        format!("Invalid JSON number in streamed tool request arguments: {error}")
    })?;
    if !parsed.is_number() {
        return Err(String::from(
            "Streamed tool request scalar was expected to be a JSON number.",
        ));
    }
    Ok(IncrementalJsonEvent::ScalarCompleted {
        path,
        value: IncrementalJsonScalar::Number(source),
    })
}

/// Builds the semantic completion event for one already-validated literal token.
/// 为一个已校验的字面量 Token 构建语义完成事件。
fn literal_event(
    path: Vec<IncrementalJsonPathSegment>,
    expected: &'static str,
) -> IncrementalJsonEvent {
    let value = match expected {
        "true" => IncrementalJsonScalar::Boolean(true),
        "false" => IncrementalJsonScalar::Boolean(false),
        "null" => IncrementalJsonScalar::Null,
        _ => unreachable!("literal token is created only for true, false, or null"),
    };
    IncrementalJsonEvent::ScalarCompleted { path, value }
}

/// Appends one hexadecimal Unicode-escape digit after strict validation.
/// 在严格校验后追加一个十六进制 Unicode 转义数字。
fn push_hex_digit(digits: &mut String, character: char) -> Result<(), String> {
    if !character.is_ascii_hexdigit() {
        return Err(format!(
            "Invalid hexadecimal digit `{character}` in JSON Unicode escape."
        ));
    }
    digits.push(character);
    Ok(())
}

/// Parses exactly four validated Unicode-escape hexadecimal digits.
/// 解析恰好四个已校验的 Unicode 转义十六进制数字。
fn parse_hex_code_unit(digits: &str) -> Result<u16, String> {
    u16::from_str_radix(digits, 16)
        .map_err(|error| format!("Invalid JSON Unicode escape code unit: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies nested paths, scalar boundaries, escapes, and surrogate pairs across fragments.
    /// 验证跨碎片的嵌套路径、标量边界、转义与代理对。
    #[test]
    fn streamed_json_emits_decoded_semantic_events() {
        // lexer retains grammatical and escape state across every intentionally split fragment.
        // lexer 在每个有意切分的碎片之间保留语法与转义状态。
        let mut lexer = IncrementalJsonLexer::default();
        // events accumulates the complete semantic stream for focused assertions.
        // events 为聚焦断言累计完整语义流。
        let mut events = Vec::new();
        for fragment in [
            r#"{"pa"#,
            r#"th":"C:\\te"#,
            r#"st\uD83D"#,
            r#"\uDE80.txt","nested":[tr"#,
            r#"ue,null,-1.2e3]}"#,
        ] {
            events.extend(lexer.push(fragment).expect("fragment should lex"));
        }
        events.extend(lexer.finish().expect("document should finish"));

        // decodedContent proves ordinary escapes and a split surrogate pair were decoded once.
        // decodedContent 证明普通转义与拆分代理对仅被解码一次。
        let decoded_content = events
            .iter()
            .filter_map(|event| match event {
                IncrementalJsonEvent::StringContent { path, content }
                    if path == &[IncrementalJsonPathSegment::Key(String::from("path"))] =>
                {
                    Some(content.as_str())
                }
                _ => None,
            })
            .collect::<String>();
        assert_eq!(decoded_content, "C:\\test🚀.txt");
        assert!(events.iter().any(|event| matches!(
            event,
            IncrementalJsonEvent::ScalarCompleted {
                path,
                value: IncrementalJsonScalar::Boolean(true),
            } if path == &[
                IncrementalJsonPathSegment::Key(String::from("nested")),
                IncrementalJsonPathSegment::Index(0),
            ]
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            IncrementalJsonEvent::ScalarCompleted {
                value: IncrementalJsonScalar::Number(source),
                ..
            } if source == "-1.2e3"
        )));
    }

    /// Verifies incomplete containers and escapes never become a successful final document.
    /// 验证不完整容器与转义绝不会成为成功的最终文档。
    #[test]
    fn incomplete_json_is_rejected_at_finish() {
        // containerLexer ends with one unclosed object.
        // containerLexer 以一个未闭合对象结束。
        let mut container_lexer = IncrementalJsonLexer::default();
        container_lexer
            .push(r#"{"value":1"#)
            .expect("prefix should remain lexically valid");
        assert!(container_lexer.finish().is_err());

        // escapeLexer ends between a high and low surrogate.
        // escapeLexer 在高代理与低代理之间结束。
        let mut escape_lexer = IncrementalJsonLexer::default();
        escape_lexer
            .push(r#""\uD83D"#)
            .expect("high-surrogate prefix should remain pending");
        assert!(escape_lexer.finish().is_err());
    }

    /// Verifies trailing commas are rejected instead of being treated as incomplete valid JSON.
    /// 验证尾随逗号会被拒绝，而不是被当作不完整合法 JSON。
    #[test]
    fn trailing_comma_is_rejected() {
        // lexer must reject the object close because the post-comma phase requires another key.
        // lexer 必须拒绝对象闭合，因为逗号后阶段要求另一个键。
        let mut lexer = IncrementalJsonLexer::default();
        assert!(lexer.push(r#"{"value":1,}"#).is_err());
    }
}
