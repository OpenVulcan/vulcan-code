//! Compiles the bounded VMM PII condition language into an immutable typed expression tree.
//! 将有界 VMM PII 条件语言编译为不可变强类型表达式树。

use regex::Captures;
use vulcan_memorymesh_core_contracts::{CoreError, ErrorCode, ErrorStage};

/// Matches VMM's maximum lowered opcode count; logical operations cost a jump and a pop.
/// 对齐 VMM 最大降级操作码数量；逻辑操作占用一次跳转与一次弹栈。
const MAX_STEPS: usize = 64;

/// Holds a validated condition without evaluating arbitrary code or runtime JSON.
/// 保存已验证条件，不执行任意代码或运行时 JSON。
#[derive(Clone)]
pub struct Condition {
    /// An absent expression denotes unconditional redaction.
    /// 缺失表达式表示无条件脱敏。
    root: Option<Expression>,
}

impl Condition {
    /// Parses the exact builtin/operator vocabulary and validates every capture, including skipped branches.
    /// 解析精确内建函数与操作符词汇，验证所有捕获，包括会被短路的分支。
    pub fn parse(source: Option<&str>, capture_count: usize) -> Result<Self, CoreError> {
        let source = source.unwrap_or_default().trim();
        if source.is_empty() {
            return Ok(Self { root: None });
        }
        let mut parser = Parser {
            source,
            cursor: 0,
            capture_count,
            steps: 0,
        };
        let root = parser.expression(1, 0)?;
        parser.whitespace();
        if parser.cursor != source.len() {
            return Err(condition_error("unexpected trailing condition syntax"));
        }
        Ok(Self { root: Some(root) })
    }

    /// Returns false only for a successfully evaluated false condition; evaluation faults still mask PII.
    /// 仅在条件成功求值为假时返回假；求值故障仍执行 PII 脱敏。
    pub fn evaluate(&self, captures: &Captures<'_>) -> bool {
        self.root
            .as_ref()
            .is_none_or(|root| !matches!(root.evaluate(captures), Some(Value::Boolean(false))))
    }
}

/// Represents only the source language's literal, capture, call, and binary forms.
/// 仅表示来源语言的字面值、捕获、调用及二元形式。
#[derive(Clone)]
enum Expression {
    /// Reads a regex capture whose existence was validated at compile time.
    /// 读取在编译期验证存在的正则捕获。
    Capture(usize),
    /// Carries a nonnegative source integer in a platform-independent signed 64-bit domain.
    /// 在跨平台有符号 64 位域中携带来源非负整数。
    Integer(i64),
    /// Carries a nonempty source array used by the weighted checksum function.
    /// 携带加权校验函数使用的非空来源数组。
    Array(Vec<i64>),
    /// Calls one closed builtin with a validated argument count.
    /// 使用已验证参数数量调用一个封闭内建函数。
    Call(Function, Vec<Self>),
    /// Preserves source precedence and real short-circuit evaluation.
    /// 保留来源优先级及真正的短路求值。
    Binary(Operator, Box<Self>, Box<Self>),
}

/// Holds borrowed match/configuration data so per-match evaluation does not copy sensitive strings.
/// 保存借用的匹配或配置数据，使逐次求值不复制敏感字符串。
enum Value<'a> {
    /// Borrows one matched capture.
    /// 借用一个命中捕获。
    Text(&'a str),
    /// Carries an integer result.
    /// 携带整数结果。
    Integer(i64),
    /// Carries a Boolean decision.
    /// 携带布尔决策。
    Boolean(bool),
    /// Borrows configured weights.
    /// 借用已配置权重。
    Array(&'a [i64]),
}

impl Expression {
    /// Propagates a missing capture, bad numeric input, or wrong type to the root masking policy.
    /// 将缺失捕获、非法数值输入或错误类型传播到根脱敏策略。
    fn evaluate<'a>(&'a self, captures: &'a Captures<'_>) -> Option<Value<'a>> {
        match self {
            Self::Capture(index) => Some(Value::Text(captures.get(*index)?.as_str())),
            Self::Integer(value) => Some(Value::Integer(*value)),
            Self::Array(values) => Some(Value::Array(values)),
            Self::Call(function, arguments) => {
                let values = arguments
                    .iter()
                    .map(|argument| argument.evaluate(captures))
                    .collect::<Option<Vec<_>>>()?;
                function.evaluate(&values)
            }
            Self::Binary(operator, left, right) => {
                let left = left.evaluate(captures)?;
                // Do not evaluate the other side after a decisive Boolean, as it may contain bad runtime captures.
                // 决定性的布尔值之后不求值另一侧，因为另一侧可能包含运行时无效捕获。
                match (operator, &left) {
                    (Operator::And, Value::Boolean(false)) => return Some(Value::Boolean(false)),
                    (Operator::Or, Value::Boolean(true)) => return Some(Value::Boolean(true)),
                    (Operator::And | Operator::Or, Value::Boolean(_)) => {}
                    (Operator::And | Operator::Or, _) => return None,
                    _ => {}
                }
                operator.evaluate(left, right.evaluate(captures)?)
            }
        }
    }
}

/// Enumerates the source evaluator's five builtin functions.
/// 枚举来源求值器的五个内建函数。
#[derive(Clone, Copy)]
enum Function {
    /// Computes byte length, without trimming the capture.
    /// 计算字节长度，不修剪捕获。
    Length,
    /// Validates the trimmed digit checksum.
    /// 验证修剪后的数字校验和。
    Luhn,
    /// Validates standard padded Base64 without decoding or retaining its content.
    /// 验证标准带填充 Base64，不解码或保留其内容。
    Base64,
    /// Calculates a positional weighted digit sum.
    /// 计算按位置加权的数字和。
    WeightSum,
    /// Maps a Chinese ID checksum character to its remainder.
    /// 将中国身份证校验字符映射为余数。
    ChineseCheck,
}

impl Function {
    /// Resolves only verified source function names and fixed arities.
    /// 仅解析已核实的来源函数名和固定参数数量。
    fn parse(name: &str) -> Result<(Self, usize), CoreError> {
        match name {
            "len" => Ok((Self::Length, 1)),
            "is_luhn" => Ok((Self::Luhn, 1)),
            "is_base64" => Ok((Self::Base64, 1)),
            "weight_sum" => Ok((Self::WeightSum, 2)),
            "cn_check" => Ok((Self::ChineseCheck, 1)),
            _ => Err(condition_error("unknown PII condition function")),
        }
    }

    /// Executes a builtin with strict source runtime types; errors request masking, not permission.
    /// 使用严格来源运行时类型执行内建函数；错误请求脱敏，而非放行。
    fn evaluate<'a>(self, values: &[Value<'a>]) -> Option<Value<'a>> {
        match (self, values) {
            (Self::Length, [Value::Text(text)]) => {
                Some(Value::Integer(i64::try_from(text.len()).ok()?))
            }
            (Self::Luhn, [Value::Text(text)]) => Some(Value::Boolean(is_luhn(text.trim()))),
            (Self::Base64, [Value::Text(text)]) => Some(Value::Boolean(is_base64(text))),
            (Self::WeightSum, [Value::Text(text), Value::Array(weights)]) => {
                let text = text.trim();
                if text.is_empty()
                    || text.len() != weights.len()
                    || !text.bytes().all(|byte| byte.is_ascii_digit())
                {
                    return None;
                }
                // Go's signed integer operations wrap; make the frozen 64-bit arithmetic explicit.
                // Go 有符号整数运算回绕；显式冻结为 64 位运算。
                let sum = text
                    .bytes()
                    .zip(*weights)
                    .fold(0_i64, |sum, (digit, weight)| {
                        sum.wrapping_add(i64::from(digit - b'0').wrapping_mul(*weight))
                    });
                Some(Value::Integer(sum))
            }
            (Self::ChineseCheck, [Value::Text(text)]) if text.len() == 1 => {
                let value = b"10X98765432"
                    .iter()
                    .position(|byte| *byte == text.as_bytes()[0].to_ascii_uppercase())?;
                Some(Value::Integer(i64::try_from(value).ok()?))
            }
            _ => None,
        }
    }
}

/// Defines exactly the source language's binary operators.
/// 精确定义来源语言的二元操作符。
#[derive(Clone, Copy)]
enum Operator {
    /// Short-circuits Boolean disjunction.
    /// 对布尔或进行短路。
    Or,
    /// Short-circuits Boolean conjunction.
    /// 对布尔与进行短路。
    And,
    /// Compares values of the same supported type.
    /// 比较相同受支持类型的值。
    Equal,
    /// Negates same-type equality.
    /// 对相同类型相等取反。
    NotEqual,
    /// Compares signed integer order.
    /// 比较有符号整数顺序。
    Greater,
    /// Includes equal signed integer values.
    /// 包含相等的有符号整数值。
    GreaterEqual,
    /// Compares reversed signed integer order.
    /// 比较反向有符号整数顺序。
    Less,
    /// Includes equality in reversed order.
    /// 在反向顺序中包含相等。
    LessEqual,
    /// Computes signed integer remainder.
    /// 计算有符号整数余数。
    Modulo,
}

impl Operator {
    /// Reads the longest operator first and returns its source precedence and byte width.
    /// 优先读取最长操作符，并返回来源优先级与字节宽度。
    fn read(source: &str) -> Option<(Self, u8, usize)> {
        for (text, operator, precedence) in [
            ("||", Self::Or, 1),
            ("&&", Self::And, 2),
            ("==", Self::Equal, 3),
            ("!=", Self::NotEqual, 3),
            (">=", Self::GreaterEqual, 3),
            ("<=", Self::LessEqual, 3),
            (">", Self::Greater, 3),
            ("<", Self::Less, 3),
            ("%", Self::Modulo, 4),
        ] {
            if source.starts_with(text) {
                return Some((operator, precedence, text.len()));
            }
        }
        None
    }

    /// Evaluates strict typed operations and preserves masking on division by zero or type mismatch.
    /// 求值严格类型操作，并在除零或类型不匹配时保留脱敏行为。
    fn evaluate<'a>(self, left: Value<'a>, right: Value<'a>) -> Option<Value<'a>> {
        match (self, left, right) {
            (Self::And | Self::Or, Value::Boolean(_), Value::Boolean(value)) => {
                Some(Value::Boolean(value))
            }
            (Self::Equal | Self::NotEqual, left, right) => {
                let equal = match (left, right) {
                    (Value::Integer(left), Value::Integer(right)) => left == right,
                    (Value::Boolean(left), Value::Boolean(right)) => left == right,
                    (Value::Text(left), Value::Text(right)) => left == right,
                    _ => return None,
                };
                Some(Value::Boolean(equal != matches!(self, Self::NotEqual)))
            }
            (operator, Value::Integer(left), Value::Integer(right)) => match operator {
                Self::Greater => Some(Value::Boolean(left > right)),
                Self::GreaterEqual => Some(Value::Boolean(left >= right)),
                Self::Less => Some(Value::Boolean(left < right)),
                Self::LessEqual => Some(Value::Boolean(left <= right)),
                Self::Modulo if right != 0 => Some(Value::Integer(left.wrapping_rem(right))),
                _ => None,
            },
            _ => None,
        }
    }
}

/// Parses directly from UTF-8 source using ASCII grammar and a bounded recursion/operation budget.
/// 使用 ASCII 语法及有界递归与操作预算，直接解析 UTF-8 来源。
struct Parser<'a> {
    /// Borrows the already trimmed resource expression.
    /// 借用已修剪的资源表达式。
    source: &'a str,
    /// Points to the next ASCII grammar byte.
    /// 指向下一个 ASCII 语法字节。
    cursor: usize,
    /// Includes capture zero, matching regex's capture table.
    /// 包含捕获零，对齐正则捕获表。
    capture_count: usize,
    /// Counts equivalent VMM opcodes before any match can execute.
    /// 在任何匹配执行前统计等价 VMM 操作码。
    steps: usize,
}

impl Parser<'_> {
    /// Skips only the source grammar's four internal whitespace bytes.
    /// 仅跳过来源语法内部的四种空白字节。
    fn whitespace(&mut self) {
        while self
            .source
            .as_bytes()
            .get(self.cursor)
            .is_some_and(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
        {
            self.cursor += 1;
        }
    }

    /// Consumes one exact delimiter after source whitespace.
    /// 在来源空白之后消费一个精确分隔符。
    fn consume(&mut self, byte: u8) -> bool {
        self.whitespace();
        if self.source.as_bytes().get(self.cursor) == Some(&byte) {
            self.cursor += 1;
            true
        } else {
            false
        }
    }

    /// Rejects missing delimiters instead of guessing argument boundaries.
    /// 拒绝缺失分隔符，而非猜测参数边界。
    fn require(&mut self, byte: u8) -> Result<(), CoreError> {
        if self.consume(byte) {
            Ok(())
        } else {
            Err(condition_error("missing PII condition delimiter"))
        }
    }

    /// Charges the same lowered-instruction budget before constructing further expression nodes.
    /// 在构造更多表达式节点前计入相同的降级指令预算。
    fn charge(&mut self, count: usize) -> Result<(), CoreError> {
        self.steps += count;
        if self.steps > MAX_STEPS {
            Err(condition_error("PII condition exceeds 64 operations"))
        } else {
            Ok(())
        }
    }

    /// Parses left-associative binary operations with the source precedence table.
    /// 使用来源优先级表解析左结合二元操作。
    fn expression(&mut self, minimum: u8, depth: usize) -> Result<Expression, CoreError> {
        if depth >= MAX_STEPS {
            return Err(condition_error("PII condition exceeds 64 nesting levels"));
        }
        let mut left = self.primary(depth + 1)?;
        loop {
            self.whitespace();
            let Some((operator, precedence, width)) = Operator::read(&self.source[self.cursor..])
            else {
                break;
            };
            if precedence < minimum {
                break;
            }
            self.cursor += width;
            self.charge(if matches!(operator, Operator::And | Operator::Or) {
                2
            } else {
                1
            })?;
            let right = self.expression(precedence + 1, depth + 1)?;
            left = Expression::Binary(operator, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    /// Parses a bounded literal/capture/call or explicit parenthesized expression.
    /// 解析有界字面值、捕获、调用或显式括号表达式。
    fn primary(&mut self, depth: usize) -> Result<Expression, CoreError> {
        self.whitespace();
        let byte = *self
            .source
            .as_bytes()
            .get(self.cursor)
            .ok_or_else(|| condition_error("missing PII condition operand"))?;
        if self.consume(b'(') {
            let expression = self.expression(1, depth)?;
            self.require(b')')?;
            return Ok(expression);
        }
        self.charge(1)?;
        match byte {
            b'$' => {
                self.cursor += 1;
                let index = usize::try_from(self.integer()?)
                    .map_err(|_| condition_error("PII capture index overflow"))?;
                if index >= self.capture_count {
                    return Err(condition_error(
                        "PII condition references an undefined capture",
                    ));
                }
                Ok(Expression::Capture(index))
            }
            b'0'..=b'9' => Ok(Expression::Integer(self.integer()?)),
            b'[' => self.array(),
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => self.call(depth),
            _ => Err(condition_error("unexpected PII condition syntax")),
        }
    }

    /// Reads one nonnegative decimal into the frozen signed 64-bit domain.
    /// 将一个非负十进制数读入冻结的有符号 64 位域。
    fn integer(&mut self) -> Result<i64, CoreError> {
        let start = self.cursor;
        while self
            .source
            .as_bytes()
            .get(self.cursor)
            .is_some_and(u8::is_ascii_digit)
        {
            self.cursor += 1;
        }
        self.source[start..self.cursor]
            .parse()
            .map_err(|_| condition_error("invalid or overflowing PII integer"))
    }

    /// Reads nonempty integer arrays, preserving VMM's accepted trailing comma.
    /// 读取非空整数数组，保留 VMM 接受的尾逗号。
    fn array(&mut self) -> Result<Expression, CoreError> {
        self.require(b'[')?;
        let mut values = Vec::new();
        loop {
            if self.consume(b']') {
                break;
            }
            values.push(self.integer()?);
            if self.consume(b']') {
                break;
            }
            self.require(b',')?;
        }
        if values.is_empty() {
            return Err(condition_error("PII condition array must not be empty"));
        }
        Ok(Expression::Array(values))
    }

    /// Resolves builtin arity and recursively parses each argument without evaluating it.
    /// 解析内建函数参数数量并递归解析每个参数，不执行求值。
    fn call(&mut self, depth: usize) -> Result<Expression, CoreError> {
        let start = self.cursor;
        while self
            .source
            .as_bytes()
            .get(self.cursor)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
        {
            self.cursor += 1;
        }
        let (function, arity) = Function::parse(&self.source[start..self.cursor])?;
        self.require(b'(')?;
        let mut arguments = Vec::with_capacity(arity);
        for index in 0..arity {
            if index > 0 {
                self.require(b',')?;
            }
            arguments.push(self.expression(1, depth)?);
        }
        self.require(b')')?;
        Ok(Expression::Call(function, arguments))
    }
}

/// Validates ASCII digits with the same right-to-left Luhn parity as VMM.
/// 使用与 VMM 相同的从右到左 Luhn 奇偶规则验证 ASCII 数字。
fn is_luhn(text: &str) -> bool {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return false;
    }
    let sum = text
        .bytes()
        .rev()
        .enumerate()
        .map(|(index, byte)| {
            let mut digit = u64::from(byte - b'0');
            if index % 2 == 1 {
                digit *= 2;
                if digit > 9 {
                    digit -= 9;
                }
            }
            digit
        })
        .sum::<u64>();
    sum % 10 == 0
}

/// Validates the standard Base64 alphabet, quartet length, and terminal padding accepted by Go.
/// 验证 Go 接受的标准 Base64 字母表、四元组长度及末尾填充。
fn is_base64(text: &str) -> bool {
    if text.is_empty() || text.len() % 4 != 0 {
        return false;
    }
    let unpadded = text.trim_end_matches('=');
    let padding = text.len() - unpadded.len();
    padding <= 2
        && unpadded
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/'))
}

/// Emits a configuration error without copying potentially sensitive rule or match text.
/// 发出配置错误，不复制潜在敏感规则或匹配文本。
fn condition_error(message: &str) -> CoreError {
    CoreError::new(
        ErrorCode::InvalidConfiguration,
        ErrorStage::Configuration,
        message,
    )
}
