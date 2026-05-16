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
    KeywordFlow,
    KeywordBoard,
    KeywordState,
    KeywordWhen,
    KeywordCase,
    KeywordElse,
    KeywordGoto,
    KeywordLet,
    KeywordReturn,
    Identifier(String),
    IntegerLiteral(String),
    FloatLiteral(String),
    StringLiteral(String),
    LeftBrace,
    RightBrace,
    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    LeftAngle,
    RightAngle,
    LeftAngleEquals,
    RightAngleEquals,
    DoubleEquals,
    BangEquals,
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
