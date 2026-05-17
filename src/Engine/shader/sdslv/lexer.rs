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
                self.Bump(c);
                continue;
            }
            if c == '/' && self.Peek(1) == Some('/') {
                while self.I < self.S.len() && self.S[self.I] as char != '\n' {
                    self.Bump(self.S[self.I] as char);
                }
                continue;
            }
            if c == '/' && self.Peek(1) == Some('*') {
                self.Bump('/');
                self.Bump('*');
                while self.I + 1 < self.S.len()
                    && !(self.S[self.I] as char == '*' && self.S[self.I + 1] as char == '/')
                {
                    self.Bump(self.S[self.I] as char);
                }
                if self.I + 1 < self.S.len() {
                    self.Bump('*');
                    self.Bump('/');
                }
                continue;
            }
            let st = self.SpanStart();
            if c.is_ascii_alphabetic() || c == '_' {
                let text = self.take_while(|x| x.is_ascii_alphanumeric() || x == '_');
                self.Push(self.KeywordOrIdent(&text), st);
                continue;
            }
            if c.is_ascii_digit() {
                let num = self.take_while(|x| x.is_ascii_digit());
                if self.Peek(0) == Some('.')
                    && self.Peek(1).map(|x| x.is_ascii_digit()).unwrap_or(false)
                {
                    self.Bump('.');
                    let frac = self.take_while(|x| x.is_ascii_digit());
                    self.Push(SdslvTokenKind::FloatLiteral(format!("{num}.{frac}")), st);
                } else {
                    self.Push(SdslvTokenKind::IntegerLiteral(num), st);
                }
                continue;
            }
            if c == '"' {
                self.Bump('"');
                let mut text = String::new();
                while self.I < self.S.len() {
                    let ch = self.S[self.I] as char;
                    if ch == '"' {
                        self.Bump('"');
                        break;
                    }
                    if ch == '\n' {
                        self.Diagnostics.push(SdslvDiagnostic::New(
                            "unterminated string literal",
                            self.Span(st, self.I),
                        ));
                        break;
                    }
                    text.push(ch);
                    self.Bump(ch);
                }
                if self.I >= self.S.len() && self.Peek(0).is_none() {
                    self.Diagnostics.push(SdslvDiagnostic::New(
                        "unterminated string literal",
                        self.Span(st, self.I),
                    ));
                }
                self.Push(SdslvTokenKind::StringLiteral(text), st);
                continue;
            }
            if c == '=' && self.Peek(1) == Some('=') {
                self.Bump('=');
                self.Bump('=');
                self.Push(SdslvTokenKind::DoubleEquals, st);
                continue;
            }
            if c == '!' && self.Peek(1) == Some('=') {
                self.Bump('!');
                self.Bump('=');
                self.Push(SdslvTokenKind::BangEquals, st);
                continue;
            }
            if c == '!' {
                self.Bump('!');
                self.Push(SdslvTokenKind::Bang, st);
                continue;
            }
            if c == '?' {
                self.Bump('?');
                self.Push(SdslvTokenKind::Question, st);
                continue;
            }
            if c == '<' && self.Peek(1) == Some('=') {
                self.Bump('<');
                self.Bump('=');
                self.Push(SdslvTokenKind::LeftAngleEquals, st);
                continue;
            }
            if c == '>' && self.Peek(1) == Some('=') {
                self.Bump('>');
                self.Bump('=');
                self.Push(SdslvTokenKind::RightAngleEquals, st);
                continue;
            }
            if c == '=' && self.Peek(1) == Some('>') {
                self.Bump('=');
                self.Bump('>');
                self.Push(SdslvTokenKind::FatArrow, st);
                continue;
            }
            if c == '-' && self.Peek(1) == Some('>') {
                self.Bump('-');
                self.Bump('>');
                self.Push(SdslvTokenKind::Arrow, st);
                continue;
            }
            if c == '.' && self.Peek(1) == Some('.') {
                self.Bump('.');
                self.Bump('.');
                self.Push(SdslvTokenKind::Range, st);
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
                self.Bump(c);
                self.Push(k, st);
            } else {
                self.Diagnostics.push(SdslvDiagnostic::New(
                    &format!("invalid character '{c}'"),
                    self.Span(st, self.I + 1),
                ));
                self.Bump(c);
            }
        }
    }
    fn KeywordOrIdent(&self, t: &str) -> SdslvTokenKind {
        match t {
            "namespace" => SdslvTokenKind::KeywordNamespace,
            "use" => SdslvTokenKind::KeywordUse,
            "type" => SdslvTokenKind::KeywordType,
            "stream" => SdslvTokenKind::KeywordStream,
            "record" => SdslvTokenKind::KeywordRecord,
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
            "board" => SdslvTokenKind::KeywordBoard,
            "state" => SdslvTokenKind::KeywordState,
            "when" => SdslvTokenKind::KeywordWhen,
            "case" => SdslvTokenKind::KeywordCase,
            "else" => SdslvTokenKind::KeywordElse,
            "goto" => SdslvTokenKind::KeywordGoto,
            "let" => SdslvTokenKind::KeywordLet,
            "return" => SdslvTokenKind::KeywordReturn,
            "with" => SdslvTokenKind::KeywordWith,
            "if" => SdslvTokenKind::KeywordIf,
            "switch" => SdslvTokenKind::KeywordSwitch,
            "for" => SdslvTokenKind::KeywordFor,
            "while" => SdslvTokenKind::KeywordWhile,
            "in" => SdslvTokenKind::KeywordIn,
            "step" => SdslvTokenKind::KeywordStep,
            _ => SdslvTokenKind::Identifier(t.to_string()),
        }
    }
    fn SpanStart(&self) -> (usize, usize, usize) {
        (self.I, self.Line, self.Col)
    }
    fn Span(&self, s: (usize, usize, usize), e: usize) -> SdslvSpan {
        SdslvSpan {
            Start: s.0,
            End: e,
            Line: s.1,
            Column: s.2,
        }
    }
    fn Push(&mut self, kind: SdslvTokenKind, s: (usize, usize, usize)) {
        self.Tokens.push(SdslvToken {
            Kind: kind,
            Span: self.Span(s, self.I),
        });
    }
    fn Peek(&self, o: usize) -> Option<char> {
        self.S.get(self.I + o).map(|b| *b as char)
    }
    fn take_while<F: Fn(char) -> bool>(&mut self, f: F) -> String {
        let mut out = String::new();
        while self.I < self.S.len() && f(self.S[self.I] as char) {
            let c = self.S[self.I] as char;
            out.push(c);
            self.Bump(c);
        }
        out
    }
    fn Bump(&mut self, c: char) {
        self.I += 1;
        if c == '\n' {
            self.Line += 1;
            self.Col = 1;
        } else {
            self.Col += 1;
        }
    }
}
