#![allow(non_snake_case)]

use super::ast::*;
use super::diagnostic::SdslvDiagnostic;
use super::lexer::LexSource;
use super::token::{SdslvSpan, SdslvToken, SdslvTokenKind};

pub fn ParseSource(source: &str) -> Result<SdslvModule, Vec<SdslvDiagnostic>> {
    let tokens = LexSource(source)?;
    Parser::New(source, tokens).ParseModule()
}

struct Parser<'a> {
    Source: &'a str,
    Tokens: Vec<SdslvToken>,
    I: usize,
    Diagnostics: Vec<SdslvDiagnostic>,
}
impl<'a> Parser<'a> {
    fn New(Source: &'a str, Tokens: Vec<SdslvToken>) -> Self {
        Self {
            Source,
            Tokens,
            I: 0,
            Diagnostics: vec![],
        }
    }
    fn ParseModule(mut self) -> Result<SdslvModule, Vec<SdslvDiagnostic>> {
        let mut m = SdslvModule {
            Namespace: None,
            Uses: vec![],
            Declarations: vec![],
        };
        while self.I < self.Tokens.len() {
            if self.match_kw(SdslvTokenKind::KeywordNamespace) {
                m.Namespace = self.parse_path_req("expected identifier after namespace");
                self.expect(SdslvTokenKind::Semicolon, "expected ';' after namespace");
            } else if self.match_kw(SdslvTokenKind::KeywordUse) {
                if let Some(p) = self.parse_path_req("expected path after use") {
                    m.Uses.push(SdslvUseDecl { Path: p });
                }
                self.expect(SdslvTokenKind::Semicolon, "expected ';' after use");
            } else if self.match_kw(SdslvTokenKind::KeywordType) {
                if let Some(d) = self.parse_type() {
                    m.Declarations.push(SdslvDecl::TypeAlias(d));
                }
            } else if self.match_kw(SdslvTokenKind::KeywordStream) {
                if let Some(d) = self.parse_stream() {
                    m.Declarations.push(SdslvDecl::Stream(d));
                }
            } else if self.match_kw(SdslvTokenKind::KeywordInterface) {
                if let Some(d) = self.parse_interface() {
                    m.Declarations.push(SdslvDecl::Interface(d));
                }
            } else if self.match_kw(SdslvTokenKind::KeywordShader) {
                if let Some(d) = self.parse_shader() {
                    m.Declarations.push(SdslvDecl::Shader(d));
                }
            } else {
                self.err_here("unexpected token at top level");
                self.I += 1;
            }
        }
        if self.Diagnostics.is_empty() {
            Ok(m)
        } else {
            Err(self.Diagnostics)
        }
    }
    fn parse_type(&mut self) -> Option<SdslvTypeAliasDecl> {
        let n = self.ident()?;
        self.expect(SdslvTokenKind::Equals, "expected '=' in type alias");
        let t = self.parse_path_req("expected type name in type alias")?;
        let mut s = None;
        if self.match_kw(SdslvTokenKind::At) {
            let _ = self.ident();
            self.expect(SdslvTokenKind::LeftParen, "expected '(' after @annotation");
            s = self.parse_path_req("expected semantic path");
            self.expect(
                SdslvTokenKind::RightParen,
                "expected ')' after semantic path",
            );
        }
        self.expect(SdslvTokenKind::Semicolon, "expected ';' after type alias");
        Some(SdslvTypeAliasDecl {
            Name: n,
            TargetType: t,
            SpaceAnnotation: s,
        })
    }
    fn parse_stream(&mut self) -> Option<SdslvStreamDecl> {
        let name = self.ident()?;
        self.expect(SdslvTokenKind::LeftBrace, "expected '{' after stream name");
        let mut fs = vec![];
        while !self.check(SdslvTokenKind::RightBrace) && self.I < self.Tokens.len() {
            let fname = self.ident()?;
            self.expect(
                SdslvTokenKind::Colon,
                "expected ':' after stream field name",
            );
            let t = self.parse_path_req("expected field type")?;
            self.expect(SdslvTokenKind::Semicolon, "expected ';' after field");
            fs.push(SdslvFieldDecl {
                Name: fname,
                TypeName: t,
            });
        }
        self.expect(
            SdslvTokenKind::RightBrace,
            "expected '}' after stream fields",
        );
        Some(SdslvStreamDecl {
            Name: name,
            Fields: fs,
        })
    }
    fn parse_interface(&mut self) -> Option<SdslvInterfaceDecl> {
        let name = self.ident()?;
        self.expect(
            SdslvTokenKind::LeftBrace,
            "expected '{' after interface name",
        );
        let mut ms = vec![];
        while !self.check(SdslvTokenKind::RightBrace) && self.I < self.Tokens.len() {
            if let Some(f) = self.parse_fn(None, false) {
                ms.push(f)
            } else {
                self.I += 1;
            }
        }
        self.expect(SdslvTokenKind::RightBrace, "expected '}' after interface");
        Some(SdslvInterfaceDecl {
            Name: name,
            Methods: ms,
        })
    }
    fn parse_shader(&mut self) -> Option<SdslvShaderDecl> {
        let name = self.ident()?;
        let mut gps = vec![];
        if self.match_kw(SdslvTokenKind::LeftAngle) {
            while !self.check(SdslvTokenKind::RightAngle) && self.I < self.Tokens.len() {
                if let Some(p) = self.ident() {
                    gps.push(p);
                }
                if !self.match_kw(SdslvTokenKind::Comma) {
                    break;
                }
            }
            self.expect(
                SdslvTokenKind::RightAngle,
                "expected '>' for generic params",
            );
        }
        let mut imps = vec![];
        if self.match_kw(SdslvTokenKind::KeywordImplements) {
            while let Some(p) = self.parse_path_req("expected interface path") {
                imps.push(p);
                if !self.match_kw(SdslvTokenKind::Comma) {
                    break;
                }
            }
        }
        let mut cons = vec![];
        if self.match_kw(SdslvTokenKind::KeywordWhere) {
            loop {
                let n = match self.ident() {
                    Some(x) => x,
                    None => break,
                };
                self.expect(SdslvTokenKind::Colon, "expected ':' in where constraint");
                let mut bs = vec![];
                while let Some(p) = self.parse_path_req("expected bound path") {
                    bs.push(p);
                    if !self.match_kw(SdslvTokenKind::Comma) {
                        break;
                    }
                }
                cons.push(SdslvWhereConstraint {
                    ParameterName: n,
                    Bounds: bs,
                });
                if !self.match_kw(SdslvTokenKind::Comma) {
                    break;
                }
            }
        }
        self.expect(
            SdslvTokenKind::LeftBrace,
            "expected '{' after shader header",
        );
        let mut mat = vec![];
        let mut ms = vec![];
        let mut sm = vec![];
        while !self.check(SdslvTokenKind::RightBrace) && self.I < self.Tokens.len() {
            if self.match_kw(SdslvTokenKind::KeywordMaterial) {
                self.expect(SdslvTokenKind::LeftBrace, "expected '{' after material");
                while !self.check(SdslvTokenKind::RightBrace) && self.I < self.Tokens.len() {
                    let n = self.ident()?;
                    self.expect(SdslvTokenKind::Colon, "expected ':' after material field");
                    let t = self.parse_path_req("expected material type")?;
                    self.expect(SdslvTokenKind::Semicolon, "expected ';'");
                    mat.push(SdslvFieldDecl {
                        Name: n,
                        TypeName: t,
                    });
                }
                self.expect(SdslvTokenKind::RightBrace, "expected '}' after material");
            } else if self.match_kw(SdslvTokenKind::KeywordStage) {
                let stage = self.ident();
                if let Some(f) = self.parse_fn(stage, false) {
                    sm.push(f);
                }
            } else if self.match_kw(SdslvTokenKind::KeywordOverride) {
                if let Some(f) = self.parse_fn(None, true) {
                    ms.push(f);
                }
            } else if self.check(SdslvTokenKind::KeywordFn) {
                if let Some(f) = self.parse_fn(None, false) {
                    ms.push(f);
                }
            } else {
                self.err_here("unexpected token in shader body");
                self.I += 1;
            }
        }
        self.expect(SdslvTokenKind::RightBrace, "expected '}' after shader");
        Some(SdslvShaderDecl {
            Name: name,
            GenericParameters: gps,
            Implements: imps,
            Constraints: cons,
            MaterialFields: mat,
            Methods: ms,
            StageMethods: sm,
        })
    }
    fn parse_fn(&mut self, stage: Option<String>, ov: bool) -> Option<SdslvFunctionDecl> {
        self.expect(SdslvTokenKind::KeywordFn, "expected fn");
        let name = self.ident()?;
        self.expect(
            SdslvTokenKind::LeftParen,
            "expected '(' in function signature",
        );
        let mut ps = vec![];
        while !self.check(SdslvTokenKind::RightParen) && self.I < self.Tokens.len() {
            let n = self.ident()?;
            self.expect(SdslvTokenKind::Colon, "expected ':' in parameter");
            let t = self.parse_path_req("expected parameter type")?;
            ps.push(SdslvFunctionParameter {
                Name: n,
                TypeName: t,
            });
            if !self.match_kw(SdslvTokenKind::Comma) {
                break;
            }
        }
        self.expect(SdslvTokenKind::RightParen, "expected ')' after parameters");
        self.expect(SdslvTokenKind::Arrow, "expected '->' in function signature");
        let rt = self.parse_path_req("expected return type")?;
        let body = if self.match_kw(SdslvTokenKind::Semicolon) {
            None
        } else if self.match_kw(SdslvTokenKind::LeftBrace) {
            self.parse_body()
        } else {
            self.err_here("expected ';' or function body");
            None
        };
        Some(SdslvFunctionDecl {
            IsOverride: ov,
            Stage: stage,
            Name: name,
            Parameters: ps,
            ReturnType: rt,
            Body: body,
        })
    }
    fn parse_body(&mut self) -> Option<SdslvBody> {
        let start = self.prev_span();
        let mut d = 1;
        let mut end = start.End;
        while self.I < self.Tokens.len() {
            match self.Tokens[self.I].Kind {
                SdslvTokenKind::LeftBrace => d += 1,
                SdslvTokenKind::RightBrace => {
                    d -= 1;
                    if d == 0 {
                        end = self.Tokens[self.I].Span.End;
                        self.I += 1;
                        break;
                    }
                }
                _ => {}
            }
            self.I += 1;
        }
        if d != 0 {
            self.Diagnostics.push(SdslvDiagnostic::New(
                "unexpected end of file while parsing block",
                start,
            ));
            return None;
        }
        Some(SdslvBody {
            Span: SdslvSpan {
                Start: start.Start,
                End: end,
                Line: start.Line,
                Column: start.Column,
            },
            RawText: self.Source[start.Start..end].to_string(),
        })
    }
    fn parse_path_req(&mut self, msg: &str) -> Option<SdslvPath> {
        let first = match self.ident() {
            Some(x) => x,
            None => {
                self.err_here(msg);
                return None;
            }
        };
        let mut seg = vec![first];
        while self.match_kw(SdslvTokenKind::Dot) {
            seg.push(self.ident_req("expected identifier after '.'")?);
        }
        Some(SdslvPath { Segments: seg })
    }
    fn ident_req(&mut self, m: &str) -> Option<String> {
        let r = self.ident();
        if r.is_none() {
            self.err_here(m);
        }
        r
    }
    fn ident(&mut self) -> Option<String> {
        if self.I >= self.Tokens.len() {
            return None;
        }
        if let SdslvTokenKind::Identifier(ref x) = self.Tokens[self.I].Kind {
            self.I += 1;
            Some(x.clone())
        } else {
            None
        }
    }
    fn expect(&mut self, k: SdslvTokenKind, m: &str) {
        if !self.match_kw(k) {
            self.err_here(m);
        }
    }
    fn match_kw(&mut self, k: SdslvTokenKind) -> bool {
        if self.check(k) {
            self.I += 1;
            true
        } else {
            false
        }
    }
    fn check(&self, k: SdslvTokenKind) -> bool {
        if self.I >= self.Tokens.len() {
            return false;
        }
        std::mem::discriminant(&self.Tokens[self.I].Kind) == std::mem::discriminant(&k)
    }
    fn err_here(&mut self, m: &str) {
        let s = self
            .Tokens
            .get(self.I)
            .map(|t| t.Span)
            .unwrap_or(SdslvSpan {
                Start: self.Source.len(),
                End: self.Source.len(),
                Line: 1,
                Column: 1,
            });
        self.Diagnostics.push(SdslvDiagnostic::New(m, s));
    }
    fn prev_span(&self) -> SdslvSpan {
        self.Tokens[self.I - 1].Span
    }
}
