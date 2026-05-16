#![allow(non_snake_case)]

use super::diagnostic::SdslvDiagnostic;
use super::token::{SdslvSpan, SdslvToken, SdslvTokenKind};

pub fn LexSource(source: &str) -> Result<Vec<SdslvToken>, Vec<SdslvDiagnostic>> {
    let mut l = Lexer::New(source);
    l.LexAll();
    if l.Diagnostics.is_empty() {
        Ok(l.Tokens)
    } else {
        Err(l.Diagnostics)
    }
}

struct Lexer<'a> {
    S: &'a [u8],
    I: usize,
    Line: usize,
    Col: usize,
    Tokens: Vec<SdslvToken>,
    Diagnostics: Vec<SdslvDiagnostic>,
}
impl<'a> Lexer<'a> {
    fn New(source: &'a str) -> Self {
        Self {
            S: source.as_bytes(),
            I: 0,
            Line: 1,
            Col: 1,
            Tokens: vec![],
            Diagnostics: vec![],
        }
    }
    fn LexAll(&mut self) {
        while self.I < self.S.len() {
            let c = self.S[self.I] as char;
            if c.is_whitespace() {
                self.bump(c);
                continue;
            }
            if c == '/' && self.peek(1) == Some('/') {
                while self.I < self.S.len() && self.S[self.I] as char != '\n' {
                    self.bump(self.S[self.I] as char);
                }
                continue;
            }
            if c == '/' && self.peek(1) == Some('*') {
                self.bump('/');
                self.bump('*');
                while self.I + 1 < self.S.len()
                    && !(self.S[self.I] as char == '*' && self.S[self.I + 1] as char == '/')
                {
                    self.bump(self.S[self.I] as char);
                }
                if self.I + 1 < self.S.len() {
                    self.bump('*');
                    self.bump('/');
                }
                continue;
            }
            let st = self.span_start();
            if c.is_ascii_alphabetic() || c == '_' {
                let text = self.take_while(|x| x.is_ascii_alphanumeric() || x == '_');
                self.push(self.keyword_or_ident(&text), st);
                continue;
            }
            if c.is_ascii_digit() {
                let num = self.take_while(|x| x.is_ascii_digit());
                if self.peek(0) == Some('.')
                    && self.peek(1).map(|x| x.is_ascii_digit()).unwrap_or(false)
                {
                    self.bump('.');
                    let frac = self.take_while(|x| x.is_ascii_digit());
                    self.push(SdslvTokenKind::FloatLiteral(format!("{num}.{frac}")), st);
                } else {
                    self.push(SdslvTokenKind::IntegerLiteral(num), st);
                }
                continue;
            }
            if c == '"' {
                self.bump('"');
                let mut text = String::new();
                while self.I < self.S.len() {
                    let ch = self.S[self.I] as char;
                    if ch == '"' {
                        self.bump('"');
                        break;
                    }
                    if ch == '\n' {
                        self.Diagnostics.push(SdslvDiagnostic::New(
                            "unterminated string literal",
                            self.span(st, self.I),
                        ));
                        break;
                    }
                    text.push(ch);
                    self.bump(ch);
                }
                if self.I >= self.S.len() && self.peek(0).is_none() {
                    self.Diagnostics.push(SdslvDiagnostic::New(
                        "unterminated string literal",
                        self.span(st, self.I),
                    ));
                }
                self.push(SdslvTokenKind::StringLiteral(text), st);
                continue;
            }
            if c == '=' && self.peek(1) == Some('=') {
                self.bump('=');
                self.bump('=');
                self.push(SdslvTokenKind::DoubleEquals, st);
                continue;
            }
            if c == '!' && self.peek(1) == Some('=') {
                self.bump('!');
                self.bump('=');
                self.push(SdslvTokenKind::BangEquals, st);
                continue;
            }
            if c == '<' && self.peek(1) == Some('=') {
                self.bump('<');
                self.bump('=');
                self.push(SdslvTokenKind::LeftAngleEquals, st);
                continue;
            }
            if c == '>' && self.peek(1) == Some('=') {
                self.bump('>');
                self.bump('=');
                self.push(SdslvTokenKind::RightAngleEquals, st);
                continue;
            }
            if c == '-' && self.peek(1) == Some('>') {
                self.bump('-');
                self.bump('>');
                self.push(SdslvTokenKind::Arrow, st);
                continue;
            }
            let kind = match c {
                '{' => Some(SdslvTokenKind::LeftBrace),
                '}' => Some(SdslvTokenKind::RightBrace),
                '(' => Some(SdslvTokenKind::LeftParen),
                ')' => Some(SdslvTokenKind::RightParen),
                '[' => Some(SdslvTokenKind::LeftBracket),
                ']' => Some(SdslvTokenKind::RightBracket),
                '<' => Some(SdslvTokenKind::LeftAngle),
                '>' => Some(SdslvTokenKind::RightAngle),
                ':' => Some(SdslvTokenKind::Colon),
                ';' => Some(SdslvTokenKind::Semicolon),
                ',' => Some(SdslvTokenKind::Comma),
                '.' => Some(SdslvTokenKind::Dot),
                '@' => Some(SdslvTokenKind::At),
                '=' => Some(SdslvTokenKind::Equals),
                '+' => Some(SdslvTokenKind::Plus),
                '-' => Some(SdslvTokenKind::Minus),
                '*' => Some(SdslvTokenKind::Star),
                '/' => Some(SdslvTokenKind::Slash),
                _ => None,
            };
            if let Some(k) = kind {
                self.bump(c);
                self.push(k, st);
            } else {
                self.Diagnostics.push(SdslvDiagnostic::New(
                    &format!("invalid character '{c}'"),
                    self.span(st, self.I + 1),
                ));
                self.bump(c);
            }
        }
    }
    fn keyword_or_ident(&self, t: &str) -> SdslvTokenKind {
        match t {
            "namespace" => SdslvTokenKind::KeywordNamespace,
            "use" => SdslvTokenKind::KeywordUse,
            "type" => SdslvTokenKind::KeywordType,
            "stream" => SdslvTokenKind::KeywordStream,
            "interface" => SdslvTokenKind::KeywordInterface,
            "shader" => SdslvTokenKind::KeywordShader,
            "material" => SdslvTokenKind::KeywordMaterial,
            "stage" => SdslvTokenKind::KeywordStage,
            "fn" => SdslvTokenKind::KeywordFn,
            "implements" => SdslvTokenKind::KeywordImplements,
            "where" => SdslvTokenKind::KeywordWhere,
            "override" => SdslvTokenKind::KeywordOverride,
            "compile" => SdslvTokenKind::KeywordCompile,
            "flow" => SdslvTokenKind::KeywordFlow,
            "state" => SdslvTokenKind::KeywordState,
            "when" => SdslvTokenKind::KeywordWhen,
            "case" => SdslvTokenKind::KeywordCase,
            "else" => SdslvTokenKind::KeywordElse,
            "goto" => SdslvTokenKind::KeywordGoto,
            "let" => SdslvTokenKind::KeywordLet,
            "return" => SdslvTokenKind::KeywordReturn,
            _ => SdslvTokenKind::Identifier(t.to_string()),
        }
    }
    fn span_start(&self) -> (usize, usize, usize) {
        (self.I, self.Line, self.Col)
    }
    fn span(&self, s: (usize, usize, usize), e: usize) -> SdslvSpan {
        SdslvSpan {
            Start: s.0,
            End: e,
            Line: s.1,
            Column: s.2,
        }
    }
    fn push(&mut self, kind: SdslvTokenKind, s: (usize, usize, usize)) {
        self.Tokens.push(SdslvToken {
            Kind: kind,
            Span: self.span(s, self.I),
        });
    }
    fn peek(&self, o: usize) -> Option<char> {
        self.S.get(self.I + o).map(|b| *b as char)
    }
    fn take_while<F: Fn(char) -> bool>(&mut self, f: F) -> String {
        let mut out = String::new();
        while self.I < self.S.len() && f(self.S[self.I] as char) {
            let c = self.S[self.I] as char;
            out.push(c);
            self.bump(c);
        }
        out
    }
    fn bump(&mut self, c: char) {
        self.I += 1;
        if c == '\n' {
            self.Line += 1;
            self.Col = 1;
        } else {
            self.Col += 1;
        }
    }
}
