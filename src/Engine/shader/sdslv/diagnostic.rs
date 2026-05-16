#![allow(non_snake_case)]

use super::token::SdslvSpan;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvDiagnostic {
    pub Message: String,
    pub Span: SdslvSpan,
}

impl SdslvDiagnostic {
    pub fn New(Message: &str, Span: SdslvSpan) -> Self {
        Self {
            Message: Message.to_string(),
            Span,
        }
    }
}
