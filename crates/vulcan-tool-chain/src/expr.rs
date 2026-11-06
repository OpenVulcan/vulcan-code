//! Expression language shared by step `if` conditions and `${{ }}` argument templates.
//! 步骤 `if` 条件与 `${{ }}` 参数模板共用的表达式语言。
//!
//! The grammar deliberately mirrors the small subset of GitHub Actions expressions that models
//! already know: status functions, `steps.<id>.<field>`, comparison, logic and a few functions.
//! 语法刻意对齐模型已熟悉的 GitHub Actions 表达式子集：状态函数、`steps.<id>.<field>`、比较、
//! 逻辑运算与少量函数。

use regex::Regex;
use serde_json::{Number, Value};

use crate::run::{KeptValues, MAX_KEPT_VALUE_BYTES, StepRecord};

/// One parsed expression node.
/// 一个已解析的表达式节点。
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Expr {
    /// String, number, boolean or null literal.
    /// 字符串、数字、布尔或 null 字面量。
    Literal(Value),
    /// `steps.<id>.<field>` reference to an earlier step.
    /// 对更早步骤的 `steps.<id>.<field>` 引用。
    StepField {
        /// Referenced step id.
        /// 被引用的步骤标识。
        step: String,
        /// Referenced field.
        /// 被引用的字段。
        field: StepField,
    },
    /// Built-in function call with arity already checked.
    /// 已检查参数个数的内置函数调用。
    Call {
        /// Called function.
        /// 被调用的函数。
        function: Function,
        /// Call arguments in order.
        /// 按顺序排列的调用参数。
        args: Vec<Expr>,
    },
    /// `.name` or `[index]` access on an object or array value.
    /// 对对象或数组值的 `.name` 或 `[index]` 访问。
    Member {
        /// Accessed value.
        /// 被访问的值。
        target: Box<Expr>,
        /// Field name or array index.
        /// 字段名或数组下标。
        key: MemberKey,
    },
    /// Logical negation.
    /// 逻辑非。
    Not(Box<Expr>),
    /// Short-circuit logical and.
    /// 短路逻辑与。
    And(Box<Expr>, Box<Expr>),
    /// Short-circuit logical or.
    /// 短路逻辑或。
    Or(Box<Expr>, Box<Expr>),
    /// Equality, or inequality when `negated`.
    /// 相等比较；`negated` 为真时为不等比较。
    Compare {
        /// Left operand.
        /// 左操作数。
        left: Box<Expr>,
        /// Right operand.
        /// 右操作数。
        right: Box<Expr>,
        /// True for `!=`.
        /// `!=` 时为真。
        negated: bool,
    },
}

/// Field readable through `steps.<id>.<field>`.
/// 可通过 `steps.<id>.<field>` 读取的字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StepField {
    /// `'success'`, `'failure'` or `'skipped'`.
    /// `'success'`、`'failure'` 或 `'skipped'`。
    Outcome,
    /// Trimmed output text the model would see.
    /// 去除首尾空白后的模型可见输出文本。
    Output,
    /// Exit code of command tools, null otherwise.
    /// 命令类工具的退出码，其余为 null。
    ExitCode,
    /// Raw standard output of command tools without trailing whitespace.
    /// 命令类工具去除尾部空白的原始标准输出。
    Stdout,
    /// Raw standard error of command tools without trailing whitespace.
    /// 命令类工具去除尾部空白的原始标准错误。
    Stderr,
}

impl StepField {
    /// Name used after `steps.<id>.`.
    /// `steps.<id>.` 之后使用的名称。
    fn name(self) -> &'static str {
        match self {
            Self::Outcome => "outcome",
            Self::Output => "output",
            Self::ExitCode => "exit_code",
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
        }
    }
}

/// Field names listed in reference errors.
/// 引用错误中列出的字段名。
const STEP_FIELD_NAMES: &str = "`.outcome`, `.output`, `.exit_code`, `.stdout` or `.stderr`";

/// Key used by member access.
/// 成员访问使用的键。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MemberKey {
    /// Object field name.
    /// 对象字段名。
    Name(String),
    /// Array index.
    /// 数组下标。
    Index(usize),
}

/// Built-in function.
/// 内置函数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Function {
    /// `success()`: no earlier step failed.
    /// `success()`：更早的步骤均未失败。
    Success,
    /// `failure()`: an earlier step failed.
    /// `failure()`：存在失败的更早步骤。
    Failure,
    /// `always()`: always true.
    /// `always()`：恒为真。
    Always,
    /// `contains(haystack, needle)`.
    /// `contains(haystack, needle)`。
    Contains,
    /// `startsWith(text, prefix)`.
    /// `startsWith(text, prefix)`。
    StartsWith,
    /// `match(text, regex)`: first capture group or whole match.
    /// `match(text, regex)`：第一个捕获组或整段匹配。
    Match,
    /// `test(text, regex)`: whether the regex matches.
    /// `test(text, regex)`：正则是否匹配。
    Test,
    /// `fromJSON(text)`.
    /// `fromJSON(text)`。
    FromJson,
}

impl Function {
    /// Resolves a function name and its required argument count.
    /// 解析函数名及其所需参数个数。
    fn lookup(name: &str) -> Option<(Self, usize)> {
        Some(match name {
            "success" => (Self::Success, 0),
            "failure" => (Self::Failure, 0),
            "always" => (Self::Always, 0),
            "contains" => (Self::Contains, 2),
            "startsWith" => (Self::StartsWith, 2),
            "match" => (Self::Match, 2),
            "test" => (Self::Test, 2),
            "fromJSON" => (Self::FromJson, 1),
            _ => return None,
        })
    }
}

/// Names listed in parse errors for unknown functions.
/// 未知函数解析错误中列出的名称。
const FUNCTION_NAMES: &str =
    "success(), failure(), always(), contains(), startsWith(), match(), test(), fromJSON()";

/// Lexical token.
/// 词法记号。
#[derive(Debug, Clone, PartialEq)]
enum Token {
    Ident(String),
    Str(String),
    Num(f64),
    LParen,
    RParen,
    LBracket,
    RBracket,
    Dot,
    Comma,
    Bang,
    AndAnd,
    OrOr,
    EqEq,
    NotEq,
}

/// Splits expression source into tokens.
/// 将表达式源码切分为记号。
fn tokenize(source: &str) -> Result<Vec<Token>, String> {
    let chars = source.chars().collect::<Vec<_>>();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        match ch {
            ' ' | '\t' | '\r' | '\n' => index += 1,
            '(' => push(&mut tokens, &mut index, Token::LParen),
            ')' => push(&mut tokens, &mut index, Token::RParen),
            '[' => push(&mut tokens, &mut index, Token::LBracket),
            ']' => push(&mut tokens, &mut index, Token::RBracket),
            '.' => push(&mut tokens, &mut index, Token::Dot),
            ',' => push(&mut tokens, &mut index, Token::Comma),
            '!' if chars.get(index + 1) == Some(&'=') => {
                tokens.push(Token::NotEq);
                index += 2;
            }
            '!' => push(&mut tokens, &mut index, Token::Bang),
            '=' if chars.get(index + 1) == Some(&'=') => {
                tokens.push(Token::EqEq);
                index += 2;
            }
            '&' if chars.get(index + 1) == Some(&'&') => {
                tokens.push(Token::AndAnd);
                index += 2;
            }
            '|' if chars.get(index + 1) == Some(&'|') => {
                tokens.push(Token::OrOr);
                index += 2;
            }
            '\'' => {
                // Single-quoted strings escape a quote by doubling it, as in GitHub Actions.
                // 单引号字符串与 GitHub Actions 一样以连写两个引号转义引号。
                let mut text = String::new();
                index += 1;
                loop {
                    match chars.get(index) {
                        None => {
                            return Err(String::from(
                                "a string literal is missing its closing quote",
                            ));
                        }
                        Some('\'') if chars.get(index + 1) == Some(&'\'') => {
                            text.push('\'');
                            index += 2;
                        }
                        Some('\'') => {
                            index += 1;
                            break;
                        }
                        Some(other) => {
                            text.push(*other);
                            index += 1;
                        }
                    }
                }
                tokens.push(Token::Str(text));
            }
            '"' => {
                return Err(String::from(
                    "string literals use single quotes, for example 'success'",
                ));
            }
            '-' | '0'..='9' => {
                let start = index;
                index += 1;
                while index < chars.len() && (chars[index].is_ascii_digit() || chars[index] == '.')
                {
                    index += 1;
                }
                let text = chars[start..index].iter().collect::<String>();
                let value = text
                    .parse::<f64>()
                    .map_err(|_| format!("`{text}` is not a valid number"))?;
                tokens.push(Token::Num(value));
            }
            ch if ch.is_ascii_alphabetic() || ch == '_' => {
                let start = index;
                while index < chars.len()
                    && (chars[index].is_ascii_alphanumeric()
                        || chars[index] == '_'
                        || chars[index] == '-')
                {
                    index += 1;
                }
                tokens.push(Token::Ident(chars[start..index].iter().collect()));
            }
            other => return Err(format!("unexpected character `{other}`")),
        }
    }
    Ok(tokens)
}

/// Pushes one single-character token and advances the cursor.
/// 推入一个单字符记号并前移游标。
fn push(tokens: &mut Vec<Token>, index: &mut usize, token: Token) {
    tokens.push(token);
    *index += 1;
}

/// Recursive-descent parser over a token list.
/// 基于记号列表的递归下降解析器。
struct Parser {
    tokens: Vec<Token>,
    position: usize,
}

impl Parser {
    /// Returns the next token without consuming it.
    /// 返回下一个记号但不消费。
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.position)
    }

    /// Consumes the next token.
    /// 消费下一个记号。
    fn next(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.position).cloned();
        self.position += 1;
        token
    }

    /// Consumes the next token when it equals `expected`.
    /// 下一个记号等于 `expected` 时消费它。
    fn eat(&mut self, expected: &Token) -> bool {
        if self.peek() == Some(expected) {
            self.position += 1;
            true
        } else {
            false
        }
    }

    /// Parses `or` precedence.
    /// 解析 `or` 优先级。
    fn parse_or(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_and()?;
        while self.eat(&Token::OrOr) {
            let right = self.parse_and()?;
            left = Expr::Or(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    /// Parses `and` precedence.
    /// 解析 `and` 优先级。
    fn parse_and(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_compare()?;
        while self.eat(&Token::AndAnd) {
            let right = self.parse_compare()?;
            left = Expr::And(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    /// Parses one optional comparison; `!` binds tighter, so `!a == b` is `(!a) == b`.
    /// 解析一个可选的比较；`!` 绑定更紧，因此 `!a == b` 是 `(!a) == b`。
    fn parse_compare(&mut self) -> Result<Expr, String> {
        let left = self.parse_unary()?;
        let negated = match self.peek() {
            Some(Token::EqEq) => false,
            Some(Token::NotEq) => true,
            _ => return Ok(left),
        };
        self.position += 1;
        let right = self.parse_unary()?;
        Ok(Expr::Compare {
            left: Box::new(left),
            right: Box::new(right),
            negated,
        })
    }

    /// Parses negation.
    /// 解析逻辑非。
    fn parse_unary(&mut self) -> Result<Expr, String> {
        if self.eat(&Token::Bang) {
            return Ok(Expr::Not(Box::new(self.parse_unary()?)));
        }
        self.parse_postfix()
    }

    /// Parses a primary expression followed by member accesses.
    /// 解析一个基础表达式及其后的成员访问。
    fn parse_postfix(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_primary()?;
        loop {
            if self.eat(&Token::Dot) {
                match self.next() {
                    Some(Token::Ident(name)) => {
                        expr = Expr::Member {
                            target: Box::new(expr),
                            key: MemberKey::Name(name),
                        };
                    }
                    _ => return Err(String::from("`.` must be followed by a field name")),
                }
            } else if self.eat(&Token::LBracket) {
                let key = match self.next() {
                    Some(Token::Num(value)) if value >= 0.0 && value.fract() == 0.0 => {
                        MemberKey::Index(value as usize)
                    }
                    Some(Token::Str(name)) => MemberKey::Name(name),
                    _ => {
                        return Err(String::from(
                            "`[` must contain a non-negative integer index or a quoted field name",
                        ));
                    }
                };
                if !self.eat(&Token::RBracket) {
                    return Err(String::from("`[` is missing its closing `]`"));
                }
                expr = Expr::Member {
                    target: Box::new(expr),
                    key,
                };
            } else {
                return Ok(expr);
            }
        }
    }

    /// Parses literals, step references, calls and parentheses.
    /// 解析字面量、步骤引用、函数调用与括号。
    fn parse_primary(&mut self) -> Result<Expr, String> {
        match self.next() {
            Some(Token::Str(text)) => Ok(Expr::Literal(Value::String(text))),
            Some(Token::Num(value)) => Ok(Expr::Literal(number_value(value))),
            Some(Token::LParen) => {
                let inner = self.parse_or()?;
                if !self.eat(&Token::RParen) {
                    return Err(String::from("`(` is missing its closing `)`"));
                }
                Ok(inner)
            }
            Some(Token::Ident(name)) => match name.as_str() {
                "true" => Ok(Expr::Literal(Value::Bool(true))),
                "false" => Ok(Expr::Literal(Value::Bool(false))),
                "null" => Ok(Expr::Literal(Value::Null)),
                "steps" => self.parse_step_field(),
                _ if self.peek() == Some(&Token::LParen) => self.parse_call(&name),
                _ => Err(format!(
                    "unknown name `{name}`; use steps.<id> followed by {STEP_FIELD_NAMES}, or a function: {FUNCTION_NAMES}"
                )),
            },
            Some(other) => Err(format!("unexpected {}", describe(&other))),
            None => Err(String::from("the expression ended too early")),
        }
    }

    /// Parses `.<id>.<field>` after `steps`.
    /// 解析 `steps` 之后的 `.<id>.<field>`。
    fn parse_step_field(&mut self) -> Result<Expr, String> {
        let step = match (self.next(), self.next()) {
            (Some(Token::Dot), Some(Token::Ident(step))) => step,
            _ => return Err(String::from("`steps` must be followed by `.<id>.<field>`")),
        };
        let field = match (self.next(), self.next()) {
            (Some(Token::Dot), Some(Token::Ident(field))) => field,
            _ => {
                return Err(format!(
                    "`steps.{step}` must be followed by {STEP_FIELD_NAMES}"
                ));
            }
        };
        let field = match field.as_str() {
            "outcome" => StepField::Outcome,
            "output" => StepField::Output,
            "exit_code" => StepField::ExitCode,
            "stdout" => StepField::Stdout,
            "stderr" => StepField::Stderr,
            other => {
                return Err(format!(
                    "`steps.{step}.{other}` is not available; use {STEP_FIELD_NAMES}"
                ));
            }
        };
        Ok(Expr::StepField { step, field })
    }

    /// Parses one function call whose name was already consumed.
    /// 解析一个函数名已被消费的函数调用。
    fn parse_call(&mut self, name: &str) -> Result<Expr, String> {
        let Some((function, arity)) = Function::lookup(name) else {
            return Err(format!(
                "unknown function `{name}()`; available: {FUNCTION_NAMES}"
            ));
        };
        self.position += 1;
        let mut args = Vec::new();
        if !self.eat(&Token::RParen) {
            loop {
                args.push(self.parse_or()?);
                if self.eat(&Token::RParen) {
                    break;
                }
                if !self.eat(&Token::Comma) {
                    return Err(format!("`{name}(` is missing `,` or `)`"));
                }
            }
        }
        if args.len() != arity {
            return Err(format!(
                "`{name}()` takes {arity} argument{}, got {}",
                if arity == 1 { "" } else { "s" },
                args.len()
            ));
        }
        if matches!(function, Function::Match | Function::Test)
            && let Some(Expr::Literal(Value::String(pattern))) = args.get(1)
        {
            Regex::new(pattern).map_err(|error| format!("invalid regex in {name}(): {error}"))?;
        }
        Ok(Expr::Call { function, args })
    }
}

/// Describes a token for error messages.
/// 为错误信息描述一个记号。
fn describe(token: &Token) -> String {
    match token {
        Token::Ident(name) => format!("`{name}`"),
        Token::Str(text) => format!("string '{text}'"),
        Token::Num(value) => format!("number {value}"),
        Token::LParen => String::from("`(`"),
        Token::RParen => String::from("`)`"),
        Token::LBracket => String::from("`[`"),
        Token::RBracket => String::from("`]`"),
        Token::Dot => String::from("`.`"),
        Token::Comma => String::from("`,`"),
        Token::Bang => String::from("`!`"),
        Token::AndAnd => String::from("`&&`"),
        Token::OrOr => String::from("`||`"),
        Token::EqEq => String::from("`==`"),
        Token::NotEq => String::from("`!=`"),
    }
}

/// Parses one complete expression.
/// 解析一个完整表达式。
pub(crate) fn parse_expression(source: &str) -> Result<Expr, String> {
    let tokens = tokenize(source)?;
    if tokens.is_empty() {
        return Err(String::from("the expression is empty"));
    }
    let mut parser = Parser {
        tokens,
        position: 0,
    };
    let expr = parser.parse_or()?;
    match parser.peek() {
        None => Ok(expr),
        Some(token) => Err(format!(
            "unexpected {} after the expression",
            describe(token)
        )),
    }
}

/// Collects every step id referenced by an expression.
/// 收集表达式引用的全部步骤标识。
pub(crate) fn referenced_steps<'a>(expr: &'a Expr, out: &mut Vec<&'a str>) {
    match expr {
        Expr::Literal(_) => {}
        Expr::StepField { step, .. } => out.push(step),
        Expr::Call { args, .. } => args.iter().for_each(|arg| referenced_steps(arg, out)),
        Expr::Member { target, .. } | Expr::Not(target) => referenced_steps(target, out),
        Expr::And(left, right) | Expr::Or(left, right) => {
            referenced_steps(left, out);
            referenced_steps(right, out);
        }
        Expr::Compare { left, right, .. } => {
            referenced_steps(left, out);
            referenced_steps(right, out);
        }
    }
}

/// Step results visible to expressions.
/// 表达式可见的步骤结果。
pub(crate) struct EvalContext<'a> {
    /// Records of the steps that came before the evaluating step.
    /// 正在求值步骤之前各步骤的记录。
    pub(crate) earlier: &'a [StepRecord],
}

impl EvalContext<'_> {
    /// Whether any earlier step failed without `continue_on_error`.
    /// 是否有未设置 `continue_on_error` 的更早步骤失败。
    pub(crate) fn any_failed(&self) -> bool {
        self.earlier.iter().any(StepRecord::counts_as_failure)
    }

    /// Finds an earlier step by id; validation guarantees it exists.
    /// 按标识查找更早步骤；校验已保证其存在。
    fn step(&self, id: &str) -> Result<&StepRecord, String> {
        self.earlier
            .iter()
            .find(|record| record.id.as_deref() == Some(id))
            .ok_or_else(|| format!("no earlier step has id `{id}`"))
    }
}

/// Evaluates an expression against earlier step results.
/// 基于更早步骤结果对表达式求值。
pub(crate) fn evaluate(expr: &Expr, context: &EvalContext<'_>) -> Result<Value, String> {
    match expr {
        Expr::Literal(value) => Ok(value.clone()),
        Expr::StepField { step, field } => {
            let record = context.step(step)?;
            match field {
                StepField::Outcome => {
                    Ok(Value::String(record.outcome.expression_name().to_string()))
                }
                _ if !record.outcome.ran() => Err(format!(
                    "step `{step}` did not run, so it has no `{}`",
                    field.name()
                )),
                StepField::Output => Ok(Value::String(record.output.trim().to_string())),
                StepField::ExitCode => Ok(record.exit_code.map_or(Value::Null, Value::from)),
                StepField::Stdout | StepField::Stderr => {
                    let streams = match &record.kept {
                        KeptValues::Kept { streams } => streams.as_ref(),
                        KeptValues::OverBudget => {
                            return Err(format!(
                                "the chain already keeps {MAX_KEPT_VALUE_BYTES} bytes of command streams, so `{}` of step `{step}` was not kept; use `.output`, or write the data to a file and read it",
                                field.name()
                            ));
                        }
                        KeptValues::None => None,
                    };
                    streams
                        .map(|(stdout, stderr)| {
                            let text = if *field == StepField::Stdout {
                                stdout
                            } else {
                                stderr
                            };
                            Value::String(text.trim_end().to_string())
                        })
                        .ok_or_else(|| {
                            format!(
                                "step `{step}` ({}) has no `{}`: only a command step that ran its command has one; use `.output`",
                                record.tool,
                                field.name()
                            )
                        })
                }
            }
        }
        Expr::Call { function, args } => call(*function, args, context),
        Expr::Member { target, key } => {
            let value = evaluate(target, context)?;
            match (key, &value) {
                (MemberKey::Name(name), Value::Object(map)) => map
                    .get(name)
                    .cloned()
                    .ok_or_else(|| format!("field `{name}` was not found")),
                (MemberKey::Index(index), Value::Array(items)) => {
                    items.get(*index).cloned().ok_or_else(|| {
                        format!(
                            "index {index} is out of range for an array of {} items",
                            items.len()
                        )
                    })
                }
                (MemberKey::Name(name), other) => Err(format!(
                    "cannot read field `{name}` of {}",
                    type_name(other)
                )),
                (MemberKey::Index(index), other) => {
                    Err(format!("cannot read index {index} of {}", type_name(other)))
                }
            }
        }
        Expr::Not(inner) => Ok(Value::Bool(!truthy(&evaluate(inner, context)?))),
        Expr::And(left, right) => {
            if !truthy(&evaluate(left, context)?) {
                return Ok(Value::Bool(false));
            }
            Ok(Value::Bool(truthy(&evaluate(right, context)?)))
        }
        Expr::Or(left, right) => {
            if truthy(&evaluate(left, context)?) {
                return Ok(Value::Bool(true));
            }
            Ok(Value::Bool(truthy(&evaluate(right, context)?)))
        }
        Expr::Compare {
            left,
            right,
            negated,
        } => {
            let equal = loose_equal(&evaluate(left, context)?, &evaluate(right, context)?);
            Ok(Value::Bool(equal != *negated))
        }
    }
}

/// Evaluates one built-in function call.
/// 对一个内置函数调用求值。
fn call(function: Function, args: &[Expr], context: &EvalContext<'_>) -> Result<Value, String> {
    let mut values = Vec::with_capacity(args.len());
    for arg in args {
        values.push(evaluate(arg, context)?);
    }
    match function {
        Function::Success => Ok(Value::Bool(!context.any_failed())),
        Function::Failure => Ok(Value::Bool(context.any_failed())),
        Function::Always => Ok(Value::Bool(true)),
        Function::Contains => Ok(Value::Bool(match &values[0] {
            Value::Array(items) => items.iter().any(|item| loose_equal(item, &values[1])),
            haystack => to_text(haystack).contains(&to_text(&values[1])),
        })),
        Function::StartsWith => Ok(Value::Bool(
            to_text(&values[0]).starts_with(&to_text(&values[1])),
        )),
        Function::Match => {
            let pattern = to_text(&values[1]);
            let regex = Regex::new(&pattern)
                .map_err(|error| format!("invalid regex in match(): {error}"))?;
            let text = to_text(&values[0]);
            let captures = regex
                .captures(&text)
                .ok_or_else(|| format!("match() found no match for '{pattern}'"))?;
            let matched = captures.get(1).or_else(|| captures.get(0));
            Ok(Value::String(
                matched.map_or("", |m| m.as_str()).to_string(),
            ))
        }
        Function::Test => {
            let regex = Regex::new(&to_text(&values[1]))
                .map_err(|error| format!("invalid regex in test(): {error}"))?;
            Ok(Value::Bool(regex.is_match(&to_text(&values[0]))))
        }
        Function::FromJson => serde_json::from_str(&to_text(&values[0]))
            .map_err(|error| format!("fromJSON() could not parse the text as JSON: {error}")),
    }
}

/// Truthiness used by `if`, `!`, `&&` and `||`.
/// `if`、`!`、`&&` 与 `||` 使用的真值规则。
pub(crate) fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Number(number) => number.as_f64().is_some_and(|n| n != 0.0),
        Value::String(text) => !text.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

/// Equality that compares numbers numerically, including numeric strings.
/// 按数值比较数字（包括数字字符串）的相等判断。
fn loose_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Number(a), Value::Number(b)) => a.as_f64() == b.as_f64(),
        (Value::Number(number), Value::String(text))
        | (Value::String(text), Value::Number(number)) => text
            .trim()
            .parse::<f64>()
            .is_ok_and(|parsed| number.as_f64() == Some(parsed)),
        _ => left == right,
    }
}

/// Converts a value to the text inserted into arguments or compared as text.
/// 将值转换为插入参数或按文本比较时使用的文本。
pub(crate) fn to_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        Value::Array(_) | Value::Object(_) => value.to_string(),
    }
}

/// Builds a JSON number, keeping integral values as integers.
/// 构建 JSON 数字，整数值保持为整数。
fn number_value(value: f64) -> Value {
    if value.fract() == 0.0 && value.abs() < 9.0e15 {
        Value::from(value as i64)
    } else {
        Number::from_f64(value).map_or(Value::Null, Value::Number)
    }
}

/// Names a value's type for error messages.
/// 为错误信息命名值的类型。
fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

#[cfg(test)]
mod precedence_tests {
    use super::{EvalContext, evaluate, parse_expression};
    use serde_json::{Value, json};

    /// Evaluates one expression with no earlier steps.
    /// 在没有更早步骤的情况下求值一个表达式。
    fn eval(source: &str) -> Value {
        let expr = parse_expression(source).expect("expression parses");
        evaluate(&expr, &EvalContext { earlier: &[] }).expect("expression evaluates")
    }

    /// Verifies `!` binds tighter than comparison and may start a comparison operand.
    /// 验证 `!` 比比较运算绑定更紧，且可作为比较操作数的开头。
    #[test]
    fn negation_binds_tighter_than_comparison() {
        // `(!'a') == 'b'` compares a boolean with a string; `!('a' == 'b')` would be true.
        // `(!'a') == 'b'` 比较布尔值与字符串；若解析为 `!('a' == 'b')` 则为真。
        assert_eq!(eval("!'a' == 'b'"), json!(false));
        assert_eq!(eval("true == !false"), json!(true));
        assert_eq!(eval("!(true == false)"), json!(true));
        assert_eq!(eval("true == true && !false"), json!(true));
    }
}
