#![allow(non_snake_case)]

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SdslvSpan {
    pub Start: usize,
    pub End: usize,
    pub Line: usize,
    pub Column: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SdslvTokenKind {
    KeywordNamespace,
    KeywordUse,
    KeywordType,
    KeywordStream,
    KeywordInterface,
    KeywordShader,
    KeywordMaterial,
    KeywordStage,
    KeywordFn,
    KeywordImplements,
    KeywordWhere,
    KeywordOverride,
    KeywordCompile,
    KeywordLet,
    KeywordReturn,
    Identifier(String),
    IntegerLiteral(String),
    FloatLiteral(String),
    LeftBrace,
    RightBrace,
    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    LeftAngle,
    RightAngle,
    Colon,
    Semicolon,
    Comma,
    Dot,
    At,
    Equals,
    Plus,
    Minus,
    Star,
    Slash,
    Arrow,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SdslvToken {
    pub Kind: SdslvTokenKind,
    pub Span: SdslvSpan,
}
