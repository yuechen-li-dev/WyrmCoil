#![allow(non_snake_case)]

use super::diagnostic::SdslvDiagnostic;
use super::token::{SdslvSpan, SdslvToken, SdslvTokenKind};

pub fn LexSource(source: &str) -> Result<Vec<SdslvToken>, Vec<SdslvDiagnostic>> {
    Lex(source, false)
}

pub fn LexCanonicalSource(source: &str) -> Result<Vec<SdslvToken>, Vec<SdslvDiagnostic>> {
    Lex(source, true)
}

fn Lex(source: &str, canonical: bool) -> Result<Vec<SdslvToken>, Vec<SdslvDiagnostic>> {
    let mut l = Lexer::New(source, canonical);
    l.LexAll();
    if l.Diagnostics.is_empty() {
        Ok(l.Tokens)
    } else {
        Err(l.Diagnostics)
    }
}

struct Lexer<'a> {
    S: &'a str,
    I: usize,
    Line: usize,
    Col: usize,
    Tokens: Vec<SdslvToken>,
    Diagnostics: Vec<SdslvDiagnostic>,
    Canonical: bool,
    ForeignHeader: bool,
}
impl<'a> Lexer<'a> {
    fn New(source: &'a str, canonical: bool) -> Self {
        Self {
            S: source,
            I: 0,
            Line: 1,
            Col: 1,
            Tokens: vec![],
            Diagnostics: vec![],
            Canonical: canonical,
            ForeignHeader: false,
        }
    }
    fn LexAll(&mut self) {
        while let Some(c) = self.Peek(0) {
            if c.is_whitespace() {
                self.Bump(c);
                continue;
            }
            if c == '/' && self.Peek(1) == Some('/') {
                self.SkipLine();
                continue;
            }
            let st = self.SpanStart();
            if !self.Canonical && self.Take("/*") {
                if !self.SkipBlockComment() {
                    self.Error("unterminated block comment", st);
                }
                continue;
            }
            if c.is_alphabetic() || c == '_' {
                self.AdvanceWhile(|x| x.is_alphanumeric() || x == '_');
                let kind = self.KeywordOrIdent(&self.S[st.0..self.I]);
                if kind == SdslvTokenKind::KeywordHlsl {
                    self.ForeignHeader = true;
                }
                self.Push(kind, st);
                continue;
            }
            if c.is_ascii_digit() {
                self.LexNumber(st);
                continue;
            }
            if c == '"' {
                self.Bump('"');
                let start = self.I;
                self.AdvanceWhile(|ch| ch != '"' && ch != '\n');
                let end = self.I;
                if self.Take("\"") {
                    self.Push(
                        SdslvTokenKind::StringLiteral(self.S[start..end].to_string()),
                        st,
                    );
                } else {
                    self.Error("unterminated string literal", st);
                }
                continue;
            }
            if c == '{' && self.ForeignHeader {
                self.LexForeignBlock(st);
                continue;
            }
            if self.Take("&&") {
                self.Error("use 'and' instead of '&&'", st);
                continue;
            }
            if self.Take("||") {
                self.Error("use 'or' instead of '||'", st);
                continue;
            }
            if self.Canonical && self.Take("?") {
                self.Error(
                    "SDSL-V4100: fallible postfix '?' was removed from canonical SDSL-V",
                    st,
                );
                continue;
            }
            if let Some(kind) = self.CompoundPunctuation(c) {
                self.Push(kind, st);
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
                '%' => Some(SdslvTokenKind::Percent),
                '&' => Some(SdslvTokenKind::BitAnd),
                '|' => Some(SdslvTokenKind::BitOr),
                '^' => Some(SdslvTokenKind::BitXor),
                '~' => Some(SdslvTokenKind::BitNot),
                '!' => Some(SdslvTokenKind::Bang),
                '?' => Some(SdslvTokenKind::Question),
                _ => None,
            };
            self.Bump(c);
            if let Some(k) = kind {
                self.Push(k, st);
            } else {
                self.Error(&format!("invalid character '{c}'"), st);
            }
        }
    }

    fn CompoundPunctuation(&mut self, first: char) -> Option<SdslvTokenKind> {
        use SdslvTokenKind::*;
        let second = self.Peek(1)?;
        let kind = match (first, second) {
            ('=', '=') => DoubleEquals,
            ('!', '=') => BangEquals,
            ('<', '=') => LeftAngleEquals,
            ('>', '=') => RightAngleEquals,
            ('=', '>') => FatArrow,
            ('-', '>') => Arrow,
            ('.', '.') => Range,
            ('<', '<') => ShiftLeft,
            ('>', '>') if self.Canonical => ShiftRight,
            ('+', '=') => PlusEquals,
            _ => return None,
        };
        self.Bump(first);
        self.Bump(second);
        Some(kind)
    }

    fn LexNumber(&mut self, start: (usize, usize, usize)) {
        let mut float = false;
        if self.Take("0x") || self.Take("0X") {
            let digits = self.I;
            self.AdvanceWhile(|ch| ch.is_ascii_hexdigit());
            if self.I == digits {
                self.Error("SDSL-V4310: hexadecimal literal requires digits", start);
                return;
            }
        } else {
            self.AdvanceWhile(|ch| ch.is_ascii_digit());
            if self.Peek(0) == Some('.') && self.Peek(1).is_some_and(|ch| ch.is_ascii_digit()) {
                self.Bump('.');
                self.AdvanceWhile(|ch| ch.is_ascii_digit());
                float = true;
            }
        }
        if let Some(ch @ ('u' | 'U')) = self.Peek(0) {
            self.Bump(ch);
        }
        let text = self.S[start.0..self.I].to_string();
        let kind = if float {
            SdslvTokenKind::FloatLiteral(text)
        } else {
            SdslvTokenKind::IntegerLiteral(text)
        };
        self.Push(kind, start);
    }

    fn LexForeignBlock(&mut self, start: (usize, usize, usize)) {
        self.ForeignHeader = false;
        self.Bump('{');
        let body_start = self.I;
        let mut depth = 1;
        while let Some(ch) = self.Peek(0) {
            if self.Take("//") {
                self.SkipLine();
                continue;
            }
            if self.Take("/*") {
                self.SkipBlockComment();
                continue;
            }
            let offset = self.I;
            self.Bump(ch);
            match ch {
                '"' => self.SkipForeignString(),
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        self.Push(
                            SdslvTokenKind::RawForeignSource(
                                self.S[body_start..offset].to_string(),
                            ),
                            start,
                        );
                        return;
                    }
                }
                _ => {}
            }
        }
        self.Error("unterminated inline HLSL block", start);
    }

    fn SkipForeignString(&mut self) {
        while let Some(ch) = self.Peek(0) {
            self.Bump(ch);
            if ch == '"' {
                return;
            }
            if ch == '\\'
                && let Some(escaped) = self.Peek(0)
            {
                self.Bump(escaped);
            }
        }
    }

    fn SkipLine(&mut self) {
        self.AdvanceWhile(|ch| ch != '\n');
    }

    fn SkipBlockComment(&mut self) -> bool {
        while let Some(ch) = self.Peek(0) {
            if self.Take("*/") {
                return true;
            }
            self.Bump(ch);
        }
        false
    }
    fn KeywordOrIdent(&self, t: &str) -> SdslvTokenKind {
        match t {
            "namespace" => SdslvTokenKind::KeywordNamespace,
            "use" => SdslvTokenKind::KeywordUse,
            "type" => SdslvTokenKind::KeywordType,
            "stream" => SdslvTokenKind::KeywordStream,
            "array" => SdslvTokenKind::KeywordArray,
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
            "utility" => SdslvTokenKind::KeywordUtility,
            "policy" => SdslvTokenKind::KeywordPolicy,
            "case" => SdslvTokenKind::KeywordCase,
            "else" => SdslvTokenKind::KeywordElse,
            "score" => SdslvTokenKind::KeywordScore,
            "hysteresis" => SdslvTokenKind::KeywordHysteresis,
            "min_commit" => SdslvTokenKind::KeywordMinCommit,
            "goto" => SdslvTokenKind::KeywordGoto,
            "let" => SdslvTokenKind::KeywordLet,
            "return" => SdslvTokenKind::KeywordReturn,
            "with" => SdslvTokenKind::KeywordWith,
            "if" => SdslvTokenKind::KeywordIf,
            "switch" => SdslvTokenKind::KeywordSwitch,
            "enum" => SdslvTokenKind::KeywordEnum,
            "match" => SdslvTokenKind::KeywordMatch,
            "ok" => SdslvTokenKind::KeywordOk,
            "err" => SdslvTokenKind::KeywordErr,
            "for" => SdslvTokenKind::KeywordFor,
            "while" => SdslvTokenKind::KeywordWhile,
            "in" => SdslvTokenKind::KeywordIn,
            "step" => SdslvTokenKind::KeywordStep,
            _ if self.Canonical => CanonicalKeyword(t),
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
        self.S[self.I..].chars().nth(o)
    }
    fn AdvanceWhile(&mut self, predicate: impl Fn(char) -> bool) {
        while let Some(c) = self.Peek(0).filter(|ch| predicate(*ch)) {
            self.Bump(c);
        }
    }
    fn Take(&mut self, text: &str) -> bool {
        if !self.S[self.I..].starts_with(text) {
            return false;
        }
        for c in text.chars() {
            self.Bump(c);
        }
        true
    }
    fn Error(&mut self, message: &str, start: (usize, usize, usize)) {
        self.Diagnostics
            .push(SdslvDiagnostic::New(message, self.Span(start, self.I)));
    }
    fn Bump(&mut self, c: char) {
        self.I += c.len_utf8();
        if c == '\n' {
            self.Line += 1;
            self.Col = 1;
        } else {
            self.Col += 1;
        }
    }
}

fn CanonicalKeyword(text: &str) -> SdslvTokenKind {
    use SdslvTokenKind::*;
    match text {
        "var" => KeywordVar,
        "import" => KeywordImport,
        "export" => KeywordExport,
        "concept" => KeywordConcept,
        "config" => KeywordConfig,
        "template" => KeywordTemplate,
        "resources" => KeywordResources,
        "workgroup" => KeywordWorkgroup,
        "readonly" => KeywordReadonly,
        "readwrite" => KeywordReadwrite,
        "require" => KeywordRequire,
        "static" => KeywordStatic,
        "assert" => KeywordAssert,
        "comptime" => KeywordComptime,
        "and" => KeywordAnd,
        "or" => KeywordOr,
        "not" => KeywordNot,
        "HLSL" => KeywordHlsl,
        "read" => KeywordRead,
        "write" => KeywordWrite,
        "push" => KeywordPush,
        "pop" => KeywordPop,
        "finish" => KeywordFinish,
        "compute" => KeywordCompute,
        "vertex" => KeywordVertex,
        "pixel" => KeywordPixel,
        "true" => KeywordTrue,
        "false" => KeywordFalse,
        _ => Identifier(text.to_string()),
    }
}
