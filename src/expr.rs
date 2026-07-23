//! The little typed language keybind guards and command arguments are written
//! in.
//!
//! Four types — bool, grip, mask, multiplicity — and no arithmetic beyond
//! negation: an expression exists to *select* a twist, not to compute one.
//! Only grips are nullable, because `hovered_grip` is the one thing that can
//! be absent, so `??` is a grip fallback.
//!
//! ```text
//! expr     := or
//! or       := and ("||" and)*
//! and      := cmp ("&&" cmp)*
//! cmp      := coalesce (("==" | "!=") coalesce)?
//! coalesce := unary ("??" coalesce)?
//! unary    := ("!" | "-") unary | atom
//! atom     := "(" expr ")" | literal | identifier
//! literal  := "true" | "false" | "null" | integer
//!           | "{" [integer ("," integer)*] "}"
//!           | "R" | "L" | "U" | "D" | "F" | "B"
//! ```
//!
//! Identifiers name variables and are lowercase by convention; the six
//! uppercase side letters are grip literals instead.

use std::fmt;

use crate::puzzle_state::{LayerMask, Side};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Type {
    Bool,
    Grip,
    Mask,
    Multiplicity,
}
impl Type {
    pub const ALL: [Type; 4] = [Type::Bool, Type::Grip, Type::Mask, Type::Multiplicity];

    pub fn name(self) -> &'static str {
        match self {
            Type::Bool => "bool",
            Type::Grip => "grip",
            Type::Mask => "mask",
            Type::Multiplicity => "multiplicity",
        }
    }

    /// the value a variable gets when it's declared, or when its type changes.
    pub fn default_value(self) -> Value {
        match self {
            Type::Bool => Value::Bool(false),
            Type::Grip => Value::Grip(None),
            Type::Mask => Value::Mask(LayerMask::OUTER),
            Type::Multiplicity => Value::Multiplicity(1),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Value {
    Bool(bool),
    /// null when nothing is gripped — `hovered_grip` with the pointer off the
    /// twist gizmos. the only nullable type.
    Grip(Option<Side>),
    Mask(LayerMask),
    /// in 45 deg steps, like `Twist::multiplicity`: 1 is a 45 deg turn, 2 a
    /// quarter turn, and negative is the other way.
    Multiplicity(i8),
}
impl Value {
    pub fn ty(self) -> Type {
        match self {
            Value::Bool(_) => Type::Bool,
            Value::Grip(_) => Type::Grip,
            Value::Mask(_) => Type::Mask,
            Value::Multiplicity(_) => Type::Multiplicity,
        }
    }

    pub fn bool(self) -> Result<bool, String> {
        match self {
            Value::Bool(b) => Ok(b),
            other => Err(wrong_type(Type::Bool, other)),
        }
    }

    /// the grip, or `None` for null.
    pub fn grip(self) -> Result<Option<Side>, String> {
        match self {
            Value::Grip(grip) => Ok(grip),
            other => Err(wrong_type(Type::Grip, other)),
        }
    }

    pub fn mask(self) -> Result<LayerMask, String> {
        match self {
            Value::Mask(mask) => Ok(mask),
            other => Err(wrong_type(Type::Mask, other)),
        }
    }

    pub fn multiplicity(self) -> Result<i8, String> {
        match self {
            Value::Multiplicity(m) => Ok(m),
            other => Err(wrong_type(Type::Multiplicity, other)),
        }
    }
}
impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Bool(b) => write!(f, "{b}"),
            Value::Grip(None) => write!(f, "null"),
            Value::Grip(Some(side)) => write!(f, "{side:?}"),
            Value::Mask(mask) => write!(f, "{mask}"),
            Value::Multiplicity(m) => write!(f, "{m}"),
        }
    }
}

fn wrong_type(wanted: Type, got: Value) -> String {
    format!(
        "expected {}, got {} `{got}`",
        wanted.name(),
        got.ty().name()
    )
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Literal(Value),
    /// a variable, by name: builtin (`key_r`, `hovered_grip`) or user-declared.
    Var(String),
    Not(Box<Expr>),
    /// negation of a multiplicity, the only arithmetic there is.
    Neg(Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Eq(Box<Expr>, Box<Expr>),
    Ne(Box<Expr>, Box<Expr>),
    /// grip fallback: the left grip unless it's null.
    Coalesce(Box<Expr>, Box<Expr>),
}
impl Expr {
    pub fn parse(src: &str) -> Result<Self, String> {
        let tokens = lex(src)?;
        let mut parser = Parser { tokens, pos: 0 };
        let expr = parser.expr()?;
        match parser.peek() {
            None => Ok(expr),
            Some(token) => Err(format!("trailing `{token}`")),
        }
    }

    pub fn eval(&self, env: &dyn Env) -> Result<Value, String> {
        match self {
            Expr::Literal(value) => Ok(*value),
            Expr::Var(name) => env
                .get(name)
                .ok_or_else(|| format!("unknown variable `{name}`")),
            Expr::Not(a) => Ok(Value::Bool(!a.eval(env)?.bool()?)),
            Expr::Neg(a) => Ok(Value::Multiplicity(-a.eval(env)?.multiplicity()?)),
            // short-circuiting, so a guard can check a variable before using
            // it (`hovered_grip != null && ...`).
            Expr::And(a, b) => Ok(Value::Bool(a.eval(env)?.bool()? && b.eval(env)?.bool()?)),
            Expr::Or(a, b) => Ok(Value::Bool(a.eval(env)?.bool()? || b.eval(env)?.bool()?)),
            Expr::Eq(a, b) => Ok(Value::Bool(eq(a, b, env)?)),
            Expr::Ne(a, b) => Ok(Value::Bool(!eq(a, b, env)?)),
            Expr::Coalesce(a, b) => match a.eval(env)?.grip()? {
                Some(side) => Ok(Value::Grip(Some(side))),
                None => Ok(Value::Grip(b.eval(env)?.grip()?)),
            },
        }
    }
}

fn eq(a: &Expr, b: &Expr, env: &dyn Env) -> Result<bool, String> {
    let (a, b) = (a.eval(env)?, b.eval(env)?);
    if a.ty() != b.ty() {
        return Err(format!(
            "can't compare {} `{a}` with {} `{b}`",
            a.ty().name(),
            b.ty().name()
        ));
    }
    Ok(a == b)
}

/// where an expression reads its variables. implemented by the keybinds
/// component over the builtins plus the user-declared variables.
pub trait Env {
    fn get(&self, name: &str) -> Option<Value>;
}

/// An expression as the user typed it, kept next to its parse: the editor
/// shows the parse error inline, and the pass doesn't reparse every frame.
#[derive(Debug, Clone)]
pub struct ExprField {
    src: String,
    parsed: Result<Expr, String>,
}
impl ExprField {
    pub fn new(src: impl Into<String>) -> Self {
        let src = src.into();
        let parsed = Expr::parse(&src);
        Self { src, parsed }
    }

    pub fn src(&self) -> &str {
        &self.src
    }

    /// call after editing `src_mut`.
    pub fn reparse(&mut self) {
        self.parsed = Expr::parse(&self.src);
    }

    pub fn src_mut(&mut self) -> &mut String {
        &mut self.src
    }

    pub fn error(&self) -> Option<&str> {
        self.parsed.as_ref().err().map(String::as_str)
    }

    pub fn eval(&self, env: &dyn Env) -> Result<Value, String> {
        match &self.parsed {
            Ok(expr) => expr.eval(env),
            Err(e) => Err(e.clone()),
        }
    }

    pub fn eval_bool(&self, env: &dyn Env) -> Result<bool, String> {
        self.eval(env)?.bool()
    }
}

// ---- lexing ----

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Ident(String),
    Int(i64),
    LBrace,
    RBrace,
    LParen,
    RParen,
    Comma,
    Not,
    Minus,
    And,
    Or,
    Eq,
    Ne,
    Coalesce,
}
impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Token::Ident(name) => write!(f, "{name}"),
            Token::Int(n) => write!(f, "{n}"),
            Token::LBrace => write!(f, "{{"),
            Token::RBrace => write!(f, "}}"),
            Token::LParen => write!(f, "("),
            Token::RParen => write!(f, ")"),
            Token::Comma => write!(f, ","),
            Token::Not => write!(f, "!"),
            Token::Minus => write!(f, "-"),
            Token::And => write!(f, "&&"),
            Token::Or => write!(f, "||"),
            Token::Eq => write!(f, "=="),
            Token::Ne => write!(f, "!="),
            Token::Coalesce => write!(f, "??"),
        }
    }
}

fn lex(src: &str) -> Result<Vec<Token>, String> {
    let chars: Vec<char> = src.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        // two-character operators first, so `!=` doesn't lex as `!`.
        let pair = match (c, next) {
            ('&', Some('&')) => Some(Token::And),
            ('|', Some('|')) => Some(Token::Or),
            ('=', Some('=')) => Some(Token::Eq),
            ('!', Some('=')) => Some(Token::Ne),
            ('?', Some('?')) => Some(Token::Coalesce),
            _ => None,
        };
        if let Some(token) = pair {
            tokens.push(token);
            i += 2;
            continue;
        }
        let single = match c {
            '(' => Some(Token::LParen),
            ')' => Some(Token::RParen),
            '{' => Some(Token::LBrace),
            '}' => Some(Token::RBrace),
            ',' => Some(Token::Comma),
            '!' => Some(Token::Not),
            '-' => Some(Token::Minus),
            _ => None,
        };
        if let Some(token) = single {
            tokens.push(token);
            i += 1;
            continue;
        }
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            let n = text.parse().map_err(|_| format!("`{text}` is too big"))?;
            tokens.push(Token::Int(n));
        } else if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            tokens.push(Token::Ident(chars[start..i].iter().collect()));
        } else {
            return Err(format!("unexpected `{c}`"));
        }
    }
    Ok(tokens)
}

// ---- parsing ----

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}
impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    /// consume the next token if it's `token`.
    fn eat(&mut self, token: &Token) -> bool {
        let matched = self.peek() == Some(token);
        if matched {
            self.pos += 1;
        }
        matched
    }

    fn next(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.pos).cloned();
        self.pos += 1;
        token
    }

    fn expr(&mut self) -> Result<Expr, String> {
        self.or()
    }

    fn or(&mut self) -> Result<Expr, String> {
        let mut lhs = self.and()?;
        while self.eat(&Token::Or) {
            lhs = Expr::Or(Box::new(lhs), Box::new(self.and()?));
        }
        Ok(lhs)
    }

    fn and(&mut self) -> Result<Expr, String> {
        let mut lhs = self.cmp()?;
        while self.eat(&Token::And) {
            lhs = Expr::And(Box::new(lhs), Box::new(self.cmp()?));
        }
        Ok(lhs)
    }

    fn cmp(&mut self) -> Result<Expr, String> {
        let lhs = self.coalesce()?;
        // non-associative: `a == b == c` is a typo, not a chain.
        if self.eat(&Token::Eq) {
            Ok(Expr::Eq(Box::new(lhs), Box::new(self.coalesce()?)))
        } else if self.eat(&Token::Ne) {
            Ok(Expr::Ne(Box::new(lhs), Box::new(self.coalesce()?)))
        } else {
            Ok(lhs)
        }
    }

    fn coalesce(&mut self) -> Result<Expr, String> {
        let lhs = self.unary()?;
        if self.eat(&Token::Coalesce) {
            Ok(Expr::Coalesce(Box::new(lhs), Box::new(self.coalesce()?)))
        } else {
            Ok(lhs)
        }
    }

    fn unary(&mut self) -> Result<Expr, String> {
        if self.eat(&Token::Not) {
            Ok(Expr::Not(Box::new(self.unary()?)))
        } else if self.eat(&Token::Minus) {
            Ok(Expr::Neg(Box::new(self.unary()?)))
        } else {
            self.atom()
        }
    }

    fn atom(&mut self) -> Result<Expr, String> {
        match self.next() {
            None => Err("expected an expression".to_string()),
            Some(Token::LParen) => {
                let inner = self.expr()?;
                if self.eat(&Token::RParen) {
                    Ok(inner)
                } else {
                    Err("expected `)`".to_string())
                }
            }
            Some(Token::LBrace) => self.mask(),
            Some(Token::Int(n)) => {
                let m = i8::try_from(n).map_err(|_| format!("multiplicity `{n}` is too big"))?;
                Ok(Expr::Literal(Value::Multiplicity(m)))
            }
            Some(Token::Ident(name)) => Ok(match name.as_str() {
                "true" => Expr::Literal(Value::Bool(true)),
                "false" => Expr::Literal(Value::Bool(false)),
                "null" => Expr::Literal(Value::Grip(None)),
                "R" => Expr::Literal(Value::Grip(Some(Side::R))),
                "L" => Expr::Literal(Value::Grip(Some(Side::L))),
                "U" => Expr::Literal(Value::Grip(Some(Side::U))),
                "D" => Expr::Literal(Value::Grip(Some(Side::D))),
                "F" => Expr::Literal(Value::Grip(Some(Side::F))),
                "B" => Expr::Literal(Value::Grip(Some(Side::B))),
                _ => Expr::Var(name),
            }),
            Some(token) => Err(format!("unexpected `{token}`")),
        }
    }

    /// the `{` is already eaten.
    fn mask(&mut self) -> Result<Expr, String> {
        let mut mask = LayerMask::NONE;
        if self.eat(&Token::RBrace) {
            return Ok(Expr::Literal(Value::Mask(mask)));
        }
        loop {
            match self.next() {
                Some(Token::Int(n)) if n < LayerMask::N_LAYERS as i64 => mask.set(n as u8, true),
                Some(Token::Int(n)) => {
                    return Err(format!(
                        "layer `{n}`: the puzzle has layers 0..{}",
                        LayerMask::N_LAYERS
                    ));
                }
                _ => return Err("expected a layer number".to_string()),
            }
            if self.eat(&Token::RBrace) {
                return Ok(Expr::Literal(Value::Mask(mask)));
            }
            if !self.eat(&Token::Comma) {
                return Err("expected `,` or `}`".to_string());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    struct TestEnv(HashMap<&'static str, Value>);
    impl Env for TestEnv {
        fn get(&self, name: &str) -> Option<Value> {
            self.0.get(name).copied()
        }
    }

    fn env() -> TestEnv {
        TestEnv(HashMap::from([
            ("key_shift", Value::Bool(true)),
            ("key_r", Value::Bool(false)),
            ("hovered_grip", Value::Grip(None)),
            ("default_mask", Value::Mask(LayerMask::OUTER)),
            ("default_multiplicity", Value::Multiplicity(-1)),
        ]))
    }

    #[track_caller]
    fn eval(src: &str) -> Result<Value, String> {
        ExprField::new(src).eval(&env())
    }

    #[test]
    fn literals_and_variables() {
        assert_eq!(eval("true"), Ok(Value::Bool(true)));
        assert_eq!(eval("key_shift"), Ok(Value::Bool(true)));
        assert_eq!(eval("U"), Ok(Value::Grip(Some(Side::U))));
        assert_eq!(eval("null"), Ok(Value::Grip(None)));
        assert_eq!(eval("{0,1}"), Ok(Value::Mask(LayerMask(0b011))));
        assert_eq!(eval("{}"), Ok(Value::Mask(LayerMask::NONE)));
        assert_eq!(eval("-2"), Ok(Value::Multiplicity(-2)));
        assert!(eval("nope").is_err());
        assert!(eval("{3}").is_err());
    }

    #[test]
    fn operators_and_precedence() {
        // `&&` binds tighter than `||`.
        assert_eq!(eval("false && false || true"), Ok(Value::Bool(true)));
        assert_eq!(eval("false && (false || true)"), Ok(Value::Bool(false)));
        // comparison binds tighter than `&&`.
        assert_eq!(
            eval("key_shift && default_mask == {0}"),
            Ok(Value::Bool(true))
        );
        // `??` binds tighter than `==`.
        assert_eq!(eval("hovered_grip ?? F == F"), Ok(Value::Bool(true)));
        assert_eq!(eval("hovered_grip != null"), Ok(Value::Bool(false)));
        assert_eq!(eval("!key_shift"), Ok(Value::Bool(false)));
        assert_eq!(eval("-default_multiplicity"), Ok(Value::Multiplicity(1)));
    }

    #[test]
    fn type_errors_are_reported_not_coerced() {
        assert!(eval("key_shift && U").is_err());
        assert!(eval("!default_mask").is_err());
        // only grips are nullable, so `??` on anything else is a mistake.
        assert!(eval("default_mask ?? {1}").is_err());
        // comparing different types is a mistake too, not just false.
        assert!(eval("default_mask == 1").is_err());
    }

    #[test]
    fn and_short_circuits_past_an_error() {
        // the point of short-circuiting: guard a use behind a null check.
        assert_eq!(
            eval("hovered_grip != null && hovered_grip == U"),
            Ok(Value::Bool(false))
        );
        // ...and `false && <type error>` still evaluates, so the check has to
        // come first.
        assert_eq!(eval("false && U"), Ok(Value::Bool(false)));
    }

    #[test]
    fn parse_errors() {
        assert!(Expr::parse("key_shift &&").is_err());
        assert!(Expr::parse("(true").is_err());
        assert!(Expr::parse("true true").is_err());
        assert!(Expr::parse("true & false").is_err());
        assert!(Expr::parse("#").is_err());
    }
}
