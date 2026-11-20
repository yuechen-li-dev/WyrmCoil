#![allow(non_snake_case)]

use super::ast::*;
use super::diagnostic::SdslvDiagnostic;
use super::lexer::{LexCanonicalSource, LexSource};
use super::token::{SdslvSpan, SdslvToken, SdslvTokenKind};

pub fn ParseSource(source: &str) -> Result<SdslvModule, Vec<SdslvDiagnostic>> {
    let tokens = LexSource(source)?;
    Parser::New(source, tokens, false).ParseModule()
}

pub fn ParseCanonicalSource(source: &str) -> Result<SdslvModule, Vec<SdslvDiagnostic>> {
    let tokens = LexCanonicalSource(source)?;
    Parser::New(source, tokens, true).ParseModule()
}

pub fn ParseTestSource(source: &str) -> Result<SdslvTestModule, Vec<SdslvDiagnostic>> {
    let tokens = LexSource(source)?;
    Parser::New(source, tokens, false).ParseTestModule()
}

#[path = "canonical_parser.rs"]
mod canonical_parser;

struct Parser<'a> {
    Source: &'a str,
    Tokens: Vec<SdslvToken>,
    I: usize,
    Diagnostics: Vec<SdslvDiagnostic>,
    Canonical: bool,
    FlowDepth: usize,
}
impl<'a> Parser<'a> {
    fn New(Source: &'a str, Tokens: Vec<SdslvToken>, canonical: bool) -> Self {
        Self {
            Source,
            Tokens,
            I: 0,
            Diagnostics: vec![],
            Canonical: canonical,
            FlowDepth: 0,
        }
    }
    fn ParseModule(mut self) -> Result<SdslvModule, Vec<SdslvDiagnostic>> {
        let mut m = SdslvModule {
            Namespace: None,
            Uses: vec![],
            Imports: vec![],
            Exports: vec![],
            Canonical: self.Canonical,
            Declarations: vec![],
        };
        while self.I < self.Tokens.len() {
            let start = self.I;
            if self.MatchKw(SdslvTokenKind::KeywordNamespace) {
                if self.Canonical && start != 0 {
                    self.ErrAt(
                        self.PrevSpan(),
                        "namespace must precede imports and declarations",
                    );
                }
                m.Namespace = self.ParsePathReq("expected identifier after namespace");
                self.Expect(SdslvTokenKind::Semicolon, "expected ';' after namespace");
            } else if self.MatchKw(SdslvTokenKind::KeywordUse) {
                if self.Canonical && !m.Declarations.is_empty() {
                    self.ErrAt(self.PrevSpan(), "use must precede declarations");
                }
                if let Some(p) = self.ParsePathReq("expected path after use") {
                    m.Uses.push(SdslvUseDecl { Path: p });
                }
                self.Expect(SdslvTokenKind::Semicolon, "expected ';' after use");
            } else if self.Canonical && self.MatchKw(SdslvTokenKind::KeywordImport) {
                if !m.Declarations.is_empty() {
                    self.ErrAt(self.PrevSpan(), "import must precede declarations");
                }
                if let Some(import) = self.ParseImport() {
                    m.Imports.push(import);
                }
            } else if self.Canonical && self.MatchIdent("space") {
                if let Some(aliases) = self.ParseSpaceGroup() {
                    m.Declarations
                        .extend(aliases.into_iter().map(SdslvDecl::TypeAlias));
                }
            } else {
                let exported = self.Canonical && self.MatchKw(SdslvTokenKind::KeywordExport);
                if let Some(declaration) = self.ParseDeclaration() {
                    if exported {
                        m.Exports.push(SdslvExportDecl {
                            DeclarationIndex: m.Declarations.len(),
                            Span: self.SpanSince(self.Tokens[start].Span),
                        });
                    }
                    m.Declarations.push(declaration);
                } else if self.Canonical && self.Diagnostics.is_empty() {
                    self.ErrHere("expected declaration");
                }
            }
            if self.I == start {
                self.I += 1;
            }
            if self.Canonical && !self.Diagnostics.is_empty() {
                break;
            }
        }
        if self.Diagnostics.is_empty() {
            Ok(m)
        } else {
            Err(self.Diagnostics)
        }
    }

    fn ParseDeclaration(&mut self) -> Option<SdslvDecl> {
        use SdslvTokenKind::*;
        let Some(token) = self.Tokens.get(self.I) else {
            self.ErrHere("expected declaration");
            return None;
        };
        let kind = &token.Kind;
        match kind {
            KeywordType => {
                self.I += 1;
                self.ParseType().map(SdslvDecl::TypeAlias)
            }
            KeywordStream => {
                self.I += 1;
                self.ParseStream().map(SdslvDecl::Stream)
            }
            KeywordRecord => {
                self.I += 1;
                self.ParseRecord().map(SdslvDecl::Record)
            }
            KeywordBoard if self.Canonical => {
                self.I += 1;
                self.ParseRecord().map(SdslvDecl::Board)
            }
            KeywordInterface if self.Canonical => {
                self.ErrHere(
                    "SDSL-V4101: interface declarations were removed; use concepts and templates",
                );
                None
            }
            KeywordInterface => {
                self.I += 1;
                self.ParseInterface().map(SdslvDecl::Interface)
            }
            KeywordShader => {
                self.I += 1;
                self.ParseShader(false, None).map(SdslvDecl::Shader)
            }
            KeywordFlow if !self.Canonical => {
                self.I += 1;
                self.ParseFlow().map(SdslvDecl::Flow)
            }
            KeywordCompile => {
                self.I += 1;
                self.ParseCompile().map(SdslvDecl::Compile)
            }
            KeywordEnum => {
                self.I += 1;
                self.ParseEnum().map(SdslvDecl::Enum)
            }
            KeywordConcept if self.Canonical => {
                self.I += 1;
                self.ParseConcept().map(SdslvDecl::Concept)
            }
            KeywordConfig if self.Canonical => {
                self.I += 1;
                self.ParseConfig().map(SdslvDecl::Config)
            }
            KeywordTemplate if self.Canonical => {
                self.I += 1;
                self.ParseTemplate().map(SdslvDecl::Shader)
            }
            KeywordFn | LeftBracket if self.Canonical => {
                let attributes = self.ParseAttributes();
                self.ParseFn(None, false, attributes)
                    .map(SdslvDecl::Function)
            }
            _ => {
                self.ErrHere("unexpected token at top level");
                None
            }
        }
    }
    fn ParseTestModule(mut self) -> Result<SdslvTestModule, Vec<SdslvDiagnostic>> {
        let mut m = SdslvTestModule {
            Namespace: None,
            Uses: vec![],
            Tests: vec![],
        };
        while self.I < self.Tokens.len() {
            if self.MatchKw(SdslvTokenKind::KeywordNamespace) {
                m.Namespace = self.ParsePathReq("expected identifier after namespace");
                self.Expect(SdslvTokenKind::Semicolon, "expected ';' after namespace");
            } else if self.MatchKw(SdslvTokenKind::KeywordUse) {
                if let Some(p) = self.ParsePathReq("expected path after use") {
                    m.Uses.push(SdslvUseDecl { Path: p });
                }
                self.Expect(SdslvTokenKind::Semicolon, "expected ';' after use");
            } else if self.Check(SdslvTokenKind::LeftBracket) {
                let attributes = self.ParseAttributes();
                let start = self.CurrentSpan();
                if let Some(function) = self.ParseTestFn()
                    && let Some(body) = function.Body
                {
                    m.Tests.push(SdslvTestFunction {
                        Attributes: attributes,
                        Name: function.Name,
                        Parameters: function.Parameters,
                        Body: body,
                        Span: start,
                    });
                }
            } else {
                self.ErrHere("unexpected token at top level in test source");
                self.I += 1;
            }
        }
        if self.Diagnostics.is_empty() {
            Ok(m)
        } else {
            Err(self.Diagnostics)
        }
    }
    fn ParseAttributes(&mut self) -> Vec<SdslvAttribute> {
        let mut out = vec![];
        while self.MatchKw(SdslvTokenKind::LeftBracket) {
            let start = self.PrevSpan();
            let Some(name) = self.IdentReq("expected attribute name") else {
                break;
            };
            let mut arguments = vec![];
            if self.MatchKw(SdslvTokenKind::LeftParen) {
                if !self.Check(SdslvTokenKind::RightParen) {
                    loop {
                        let Some(arg) = self.ParseExpression() else {
                            break;
                        };
                        arguments.push(arg);
                        if !self.MatchKw(SdslvTokenKind::Comma) {
                            break;
                        }
                    }
                }
                self.Expect(
                    SdslvTokenKind::RightParen,
                    "expected ')' after attribute arguments",
                );
            }
            self.Expect(SdslvTokenKind::RightBracket, "expected ']' after attribute");
            out.push(SdslvAttribute {
                Name: name,
                Arguments: arguments,
                Span: self.SpanSince(start),
            });
        }
        out
    }
    fn ParseTestFn(&mut self) -> Option<SdslvFunctionDecl> {
        self.Expect(SdslvTokenKind::KeywordFn, "expected fn");
        let name = self.Ident()?;
        self.Expect(
            SdslvTokenKind::LeftParen,
            "expected '(' in function signature",
        );
        let mut ps = vec![];
        while !self.Check(SdslvTokenKind::RightParen) && self.I < self.Tokens.len() {
            let n = self.Ident()?;
            self.Expect(SdslvTokenKind::Colon, "expected ':' in parameter");
            let t = self.ParseTypeRefReq("expected parameter type")?;
            ps.push(SdslvFunctionParameter {
                Name: n,
                TypeName: t,
            });
            if !self.MatchKw(SdslvTokenKind::Comma) {
                break;
            }
        }
        self.Expect(SdslvTokenKind::RightParen, "expected ')' after parameters");
        let body = if self.MatchKw(SdslvTokenKind::LeftBrace) {
            self.ParseBody()
        } else {
            self.ErrHere("expected function body");
            None
        };
        Some(SdslvFunctionDecl {
            Attributes: vec![],
            IsOverride: false,
            Stage: None,
            Name: name,
            Parameters: ps,
            ReturnType: SdslvTypeRef::Named(SdslvPath {
                Segments: vec!["void".to_string()],
            }),
            ReturnTypeSpan: self.PrevSpan(),
            ErrorType: None,
            Body: body,
        })
    }

    fn ParseEnum(&mut self) -> Option<SdslvEnumDecl> {
        let start = self.PrevSpan();
        let name = self.IdentReq("expected enum name")?;
        self.Expect(SdslvTokenKind::LeftBrace, "expected '{' after enum name");
        let mut variants = vec![];
        while !self.Check(SdslvTokenKind::RightBrace) && self.I < self.Tokens.len() {
            let span = self.CurrentSpan();
            let variant_name = self.IdentReq("expected enum variant name")?;
            let payload = if self.Canonical && self.MatchKw(SdslvTokenKind::LeftBrace) {
                let fields = self.ParseAggregateFields("payload field name")?;
                self.Expect(
                    SdslvTokenKind::RightBrace,
                    "expected '}' after payload fields",
                );
                Some(fields)
            } else {
                self.Expect(SdslvTokenKind::Semicolon, "expected ';' after enum variant");
                None
            };
            variants.push(SdslvEnumVariant {
                Name: variant_name,
                Payload: payload,
                Span: self.SpanSince(span),
            });
        }
        self.Expect(
            SdslvTokenKind::RightBrace,
            "expected '}' after enum variants",
        );
        let end = self.PrevSpan();
        Some(SdslvEnumDecl {
            Name: name,
            Variants: variants,
            Span: SdslvSpan {
                Start: start.Start,
                End: end.End,
                Line: start.Line,
                Column: start.Column,
            },
        })
    }

    fn ParseCompile(&mut self) -> Option<SdslvCompileDecl> {
        let generic_shader = self.ParsePathReq("expected shader path after compile")?;
        self.Expect(
            SdslvTokenKind::LeftAngle,
            "expected '<' in compile declaration",
        );
        let mut type_arguments = vec![];
        while !self.Check(SdslvTokenKind::RightAngle) && self.I < self.Tokens.len() {
            let arg = self.ParseTypeRefReq("expected type argument path")?;
            type_arguments.push(arg);
            if !self.MatchKw(SdslvTokenKind::Comma) {
                break;
            }
        }
        self.Expect(
            SdslvTokenKind::RightAngle,
            "expected '>' after type arguments",
        );
        let as_keyword = self.Ident()?;
        if as_keyword != "as" {
            self.ErrHere("expected 'as' in compile declaration");
            return None;
        }
        let alias = self.Ident()?;
        self.Expect(
            SdslvTokenKind::Semicolon,
            "expected ';' after compile declaration",
        );
        Some(SdslvCompileDecl {
            GenericShader: generic_shader,
            TypeArguments: type_arguments,
            Alias: alias,
        })
    }
    fn ParseType(&mut self) -> Option<SdslvTypeAliasDecl> {
        let n = self.Ident()?;
        self.Expect(SdslvTokenKind::Equals, "expected '=' in type alias");
        let t = self.ParseTypeRefReq("expected type name in type alias")?;
        let mut s = None;
        if self.MatchKw(SdslvTokenKind::At) {
            let annotation = self.CurrentSpan();
            let name = self.Ident();
            if self.Canonical && name.as_deref() != Some("space") {
                self.ErrAt(
                    annotation,
                    "only @space(...) type annotations are supported",
                );
                return None;
            }
            self.Expect(SdslvTokenKind::LeftParen, "expected '(' after @annotation");
            s = self.ParsePathReq("expected semantic path");
            self.Expect(
                SdslvTokenKind::RightParen,
                "expected ')' after semantic path",
            );
        }
        self.Expect(SdslvTokenKind::Semicolon, "expected ';' after type alias");
        Some(SdslvTypeAliasDecl {
            Name: n,
            TargetType: t,
            SpaceAnnotation: s,
        })
    }
    fn ParseStream(&mut self) -> Option<SdslvStreamDecl> {
        let start = self.PrevSpan();
        let name = self.Ident()?;
        self.Expect(SdslvTokenKind::LeftBrace, "expected '{' after stream name");
        let fs = self.ParseAggregateFields("stream field name")?;
        self.Expect(
            SdslvTokenKind::RightBrace,
            "expected '}' after stream fields",
        );
        Some(SdslvStreamDecl {
            Name: name,
            Fields: fs,
            Span: self.SpanSince(start),
        })
    }
    fn ParseRecord(&mut self) -> Option<SdslvRecordDecl> {
        let name = self.Ident()?;
        self.Expect(SdslvTokenKind::LeftBrace, "expected '{' after record name");
        let fs = self.ParseAggregateFields("record field name")?;
        self.Expect(
            SdslvTokenKind::RightBrace,
            "expected '}' after record fields",
        );
        Some(SdslvRecordDecl {
            Name: name,
            Fields: fs,
        })
    }
    fn ParseAggregateFields(&mut self, field_context: &str) -> Option<Vec<SdslvFieldDecl>> {
        let mut fs = vec![];
        while !self.Check(SdslvTokenKind::RightBrace) && self.I < self.Tokens.len() {
            let start = self.CurrentSpan();
            let attributes = if self.Canonical {
                self.ParseAttributes()
            } else {
                vec![]
            };
            let fname = self.IdentReq(&format!("expected {}", field_context))?;
            self.Expect(SdslvTokenKind::Colon, "expected ':' after field name");
            let access = self.ParseResourceAccess();
            let type_start = self.CurrentSpan();
            let t = self.ParseTypeRefReq("expected field type")?;
            let type_span = self.SpanSince(type_start);
            self.Expect(SdslvTokenKind::Semicolon, "expected ';' after field");
            fs.push(SdslvFieldDecl {
                Name: fname,
                TypeName: t,
                Attributes: attributes,
                Access: access,
                Span: self.SpanSince(start),
                TypeSpan: type_span,
            });
        }
        Some(fs)
    }
    fn ParseInterface(&mut self) -> Option<SdslvInterfaceDecl> {
        let name = self.Ident()?;
        self.Expect(
            SdslvTokenKind::LeftBrace,
            "expected '{' after interface name",
        );
        let mut ms = vec![];
        while !self.Check(SdslvTokenKind::RightBrace) && self.I < self.Tokens.len() {
            if let Some(f) = self.ParseFn(None, false, vec![]) {
                ms.push(f)
            } else {
                self.I += 1;
            }
        }
        self.Expect(SdslvTokenKind::RightBrace, "expected '}' after interface");
        Some(SdslvInterfaceDecl {
            Name: name,
            Methods: ms,
        })
    }
    fn ParseShader(
        &mut self,
        template_shader: bool,
        mut template: Option<SdslvTemplateParam>,
    ) -> Option<SdslvShaderDecl> {
        let name = self.Ident()?;
        let mut gps = vec![];
        if self.Canonical {
            if template_shader && template.is_none() {
                template = Some(self.ParseTemplateParameter()?);
            } else if self.Check(SdslvTokenKind::LeftAngle) {
                self.ErrHere("SDSL-V4303: shader parameters require template");
                return None;
            }
            if let Some(param) = &template {
                gps.push(param.Name.clone());
            }
            if self.Check(SdslvTokenKind::KeywordImplements)
                || self.Check(SdslvTokenKind::KeywordWhere)
            {
                self.ErrHere("SDSL-V4101: legacy shader constraints were removed; use a concept");
                return None;
            }
        } else if self.MatchKw(SdslvTokenKind::LeftAngle) {
            while !self.Check(SdslvTokenKind::RightAngle) && self.I < self.Tokens.len() {
                if let Some(p) = self.Ident() {
                    gps.push(p);
                }
                if !self.MatchKw(SdslvTokenKind::Comma) {
                    break;
                }
            }
            self.Expect(
                SdslvTokenKind::RightAngle,
                "expected '>' for generic params",
            );
        }
        let mut imps = vec![];
        if self.MatchKw(SdslvTokenKind::KeywordImplements) {
            while let Some(p) = self.ParsePathReq("expected interface path") {
                imps.push(p);
                if !self.MatchKw(SdslvTokenKind::Comma) {
                    break;
                }
            }
        }
        let mut cons = vec![];
        if self.MatchKw(SdslvTokenKind::KeywordWhere) {
            loop {
                let n = match self.Ident() {
                    Some(x) => x,
                    None => break,
                };
                self.Expect(SdslvTokenKind::Colon, "expected ':' in where constraint");
                let mut bs = vec![];
                while let Some(p) = self.ParsePathReq("expected bound path") {
                    bs.push(p);
                    if !self.MatchKw(SdslvTokenKind::Comma) {
                        break;
                    }
                }
                cons.push(SdslvWhereConstraint {
                    ParameterName: n,
                    Bounds: bs,
                });
                if !self.MatchKw(SdslvTokenKind::Comma) {
                    break;
                }
            }
        }
        self.Expect(
            SdslvTokenKind::LeftBrace,
            "expected '{' after shader header",
        );
        let mut mat = vec![];
        let mut ms = vec![];
        let mut sm = vec![];
        let mut resources = None;
        let mut workgroups = vec![];
        let mut static_asserts = vec![];
        while !self.Check(SdslvTokenKind::RightBrace) && self.I < self.Tokens.len() {
            if self.Canonical && self.MatchKw(SdslvTokenKind::KeywordResources) {
                if resources.is_some() {
                    self.ErrAt(self.PrevSpan(), "shader already declares resources");
                    return None;
                }
                resources = Some(self.ParseResources()?);
            } else if self.Canonical && self.MatchKw(SdslvTokenKind::KeywordWorkgroup) {
                workgroups.push(self.ParseWorkgroup()?);
            } else if self.Canonical && self.MatchKw(SdslvTokenKind::KeywordStatic) {
                static_asserts.push(self.ParseAssert()?);
            } else if self.MatchKw(SdslvTokenKind::KeywordMaterial) {
                self.Expect(SdslvTokenKind::LeftBrace, "expected '{' after material");
                mat.extend(self.ParseAggregateFields("material field name")?);
                self.Expect(SdslvTokenKind::RightBrace, "expected '}' after material");
            } else if self.MatchKw(SdslvTokenKind::KeywordStage) {
                let stage = self.ParseStageName();
                let attributes = if self.Canonical {
                    self.ParseAttributes()
                } else {
                    vec![]
                };
                if let Some(f) = self.ParseFn(stage, false, attributes) {
                    sm.push(f);
                }
            } else if self.MatchKw(SdslvTokenKind::KeywordOverride) {
                if self.Canonical {
                    self.ErrAt(
                        self.PrevSpan(),
                        "SDSL-V4101: override methods were removed; use a template",
                    );
                    return None;
                }
                if let Some(f) = self.ParseFn(None, true, vec![]) {
                    ms.push(f);
                }
            } else if self.Check(SdslvTokenKind::KeywordFn)
                || (self.Canonical && self.Check(SdslvTokenKind::LeftBracket))
            {
                let attributes = if self.Canonical {
                    self.ParseAttributes()
                } else {
                    vec![]
                };
                if let Some(f) = self.ParseFn(None, false, attributes) {
                    ms.push(f);
                }
            } else {
                self.ErrHere("unexpected token in shader body");
                self.I += 1;
            }
        }
        self.Expect(SdslvTokenKind::RightBrace, "expected '}' after shader");
        Some(SdslvShaderDecl {
            Name: name,
            GenericParameters: gps,
            Implements: imps,
            Constraints: cons,
            MaterialFields: mat,
            Methods: ms,
            StageMethods: sm,
            Template: template,
            Resources: resources,
            Workgroups: workgroups,
            StaticAsserts: static_asserts,
        })
    }
    fn ParseFn(
        &mut self,
        stage: Option<String>,
        ov: bool,
        attributes: Vec<SdslvAttribute>,
    ) -> Option<SdslvFunctionDecl> {
        self.Expect(SdslvTokenKind::KeywordFn, "expected fn");
        let name = self.Ident()?;
        self.Expect(
            SdslvTokenKind::LeftParen,
            "expected '(' in function signature",
        );
        let mut ps = vec![];
        while !self.Check(SdslvTokenKind::RightParen) && self.I < self.Tokens.len() {
            let n = self.Ident()?;
            self.Expect(SdslvTokenKind::Colon, "expected ':' in parameter");
            let t = self.ParseTypeRefReq("expected parameter type")?;
            ps.push(SdslvFunctionParameter {
                Name: n,
                TypeName: t,
            });
            if !self.MatchKw(SdslvTokenKind::Comma) {
                break;
            }
        }
        self.Expect(SdslvTokenKind::RightParen, "expected ')' after parameters");
        self.Expect(SdslvTokenKind::Arrow, "expected '->' in function signature");
        let return_start = self.CurrentSpan();
        let rt = self.ParseTypeRefReq("expected return type")?;
        let return_span = self.SpanSince(return_start);
        let error_type = if self.MatchKw(SdslvTokenKind::Bang) {
            if self.Canonical {
                self.ErrAt(
                    self.PrevSpan(),
                    "SDSL-V4100: fallible returns were removed; use a payload enum",
                );
                return None;
            }
            Some(self.ParseTypeRefReq("expected error type after '!' in function signature")?)
        } else {
            None
        };
        let body = if self.MatchKw(SdslvTokenKind::Semicolon) {
            if self.Canonical {
                self.ErrAt(self.PrevSpan(), "canonical functions require bodies");
            }
            None
        } else if self.MatchKw(SdslvTokenKind::LeftBrace) {
            self.ParseBody()
        } else {
            self.ErrHere("expected ';' or function body");
            None
        };
        Some(SdslvFunctionDecl {
            Attributes: attributes,
            IsOverride: ov,
            Stage: stage,
            Name: name,
            Parameters: ps,
            ReturnType: rt,
            ReturnTypeSpan: return_span,
            ErrorType: error_type,
            Body: body,
        })
    }
    fn ParseFlow(&mut self) -> Option<SdslvFlowDecl> {
        let start = self.PrevSpan();
        let name = self.Ident()?;
        self.Expect(SdslvTokenKind::LeftParen, "expected '(' in flow signature");
        let mut parameters = vec![];
        while !self.Check(SdslvTokenKind::RightParen) && self.I < self.Tokens.len() {
            let n = self.Ident()?;
            self.Expect(SdslvTokenKind::Colon, "expected ':' in parameter");
            let t = self.ParseTypeRefReq("expected parameter type")?;
            parameters.push(SdslvFunctionParameter {
                Name: n,
                TypeName: t,
            });
            if !self.MatchKw(SdslvTokenKind::Comma) {
                break;
            }
        }
        self.Expect(SdslvTokenKind::RightParen, "expected ')' after parameters");
        self.Expect(SdslvTokenKind::Arrow, "expected '->' in flow signature");
        let return_type = self.ParseTypeRefReq("expected return type in flow declaration")?;
        self.Expect(
            SdslvTokenKind::LeftBrace,
            "expected '{' after flow signature",
        );
        let mut board = None;
        let mut states = vec![];
        let mut saw_state = false;
        while !self.Check(SdslvTokenKind::RightBrace) && self.I < self.Tokens.len() {
            if self.MatchKw(SdslvTokenKind::KeywordBoard) {
                if saw_state {
                    self.ErrHere("flow board must be declared before states");
                }
                if board.is_some() {
                    self.ErrHere("flow can declare at most one board block");
                }
                if let Some(parsed) = self.ParseFlowBoard() {
                    if board.is_none() {
                        board = Some(parsed);
                    }
                }
                continue;
            }
            if !self.MatchKw(SdslvTokenKind::KeywordState) {
                self.ErrHere("expected state declaration in flow body");
                self.I += 1;
                continue;
            }
            saw_state = true;
            if let Some(state) = self.ParseFlowState() {
                states.push(state);
            }
        }
        self.Expect(SdslvTokenKind::RightBrace, "expected '}' after flow body");
        let end = self.PrevSpan();
        Some(SdslvFlowDecl {
            Name: name,
            Parameters: parameters,
            ReturnType: return_type,
            Board: board,
            States: states,
            Span: SdslvSpan {
                Start: start.Start,
                End: end.End,
                Line: start.Line,
                Column: start.Column,
            },
        })
    }

    fn ParseFlowBoard(&mut self) -> Option<SdslvFlowBoard> {
        let start = self.PrevSpan();
        self.Expect(SdslvTokenKind::LeftBrace, "expected '{' after board");
        let mut fields = vec![];
        while !self.Check(SdslvTokenKind::RightBrace) && self.I < self.Tokens.len() {
            let field_start = self.CurrentSpan();
            let name = self.IdentReq("expected board field name")?;
            self.Expect(SdslvTokenKind::Colon, "expected ':' after board field name");
            let type_name = self.ParseTypeRefReq("expected board field type")?;
            if self.MatchKw(SdslvTokenKind::Equals) {
                self.ErrHere("unsupported board initializer in SDSL-V M9");
                let _ = self.ParseExpression();
            }
            self.Expect(SdslvTokenKind::Semicolon, "expected ';' after board field");
            fields.push(SdslvFlowBoardField {
                Name: name,
                TypeName: type_name,
                Span: field_start,
            });
        }
        self.Expect(SdslvTokenKind::RightBrace, "expected '}' after board");
        let end = self.PrevSpan();
        Some(SdslvFlowBoard {
            Fields: fields,
            Span: SdslvSpan {
                Start: start.Start,
                End: end.End,
                Line: start.Line,
                Column: start.Column,
            },
        })
    }
    fn ParseFlowState(&mut self) -> Option<SdslvFlowState> {
        let start = self.PrevSpan();
        let name = self.Ident()?;
        self.Expect(SdslvTokenKind::LeftBrace, "expected '{' after state name");
        let mut statements = vec![];
        while !self.Check(SdslvTokenKind::RightBrace) && self.I < self.Tokens.len() {
            if let Some(statement) = self.ParseFlowStatement() {
                statements.push(statement);
            } else {
                self.RecoverStatement();
            }
        }
        self.Expect(SdslvTokenKind::RightBrace, "expected '}' after state body");
        let end = self.PrevSpan();
        Some(SdslvFlowState {
            Name: name,
            Statements: statements,
            Span: SdslvSpan {
                Start: start.Start,
                End: end.End,
                Line: start.Line,
                Column: start.Column,
            },
        })
    }
    fn ParseFlowStatement(&mut self) -> Option<SdslvFlowStatement> {
        if self.CheckBoardAssignmentShape() {
            let span = self.CurrentSpan();
            self.Expect(SdslvTokenKind::KeywordBoard, "expected board");
            self.Expect(SdslvTokenKind::Dot, "expected '.' after board");
            let field = self.IdentReq("expected board field name after board.")?;
            self.Expect(SdslvTokenKind::Equals, "expected '=' in board assignment");
            let value = self.ParseExpression()?;
            self.Expect(
                SdslvTokenKind::Semicolon,
                "expected ';' after board assignment",
            );
            return Some(SdslvFlowStatement::BoardAssign {
                Field: field,
                Value: value,
                Span: span,
            });
        }
        if self.MatchKw(SdslvTokenKind::KeywordWhen) {
            return Some(SdslvFlowStatement::When(self.ParseFlowWhen()?));
        }
        if self.MatchKw(SdslvTokenKind::KeywordGoto) {
            let target = self.ParsePathReq("expected state path after goto")?;
            self.Expect(SdslvTokenKind::Semicolon, "expected ';' after goto");
            return Some(SdslvFlowStatement::Goto(target));
        }
        if self.MatchKw(SdslvTokenKind::KeywordReturn) {
            let value = self.ParseExpression()?;
            self.Expect(SdslvTokenKind::Semicolon, "expected ';' after return");
            return Some(SdslvFlowStatement::Return(value));
        }
        self.ErrHere("unsupported statement in flow state body");
        None
    }
    fn CheckBoardAssignmentShape(&self) -> bool {
        let Some(token) = self.Tokens.get(self.I) else {
            return false;
        };
        if !matches!(token.Kind, SdslvTokenKind::KeywordBoard) {
            return false;
        }
        matches!(
            (
                self.Tokens.get(self.I + 1).map(|x| &x.Kind),
                self.Tokens.get(self.I + 2).map(|x| &x.Kind)
            ),
            (
                Some(SdslvTokenKind::Dot),
                Some(SdslvTokenKind::Identifier(_))
            )
        )
    }
    fn ParseFlowWhen(&mut self) -> Option<SdslvFlowWhen> {
        let start = self.PrevSpan();
        self.Expect(SdslvTokenKind::LeftBrace, "expected '{' after when");
        let mut cases = vec![];
        let mut else_action = None;
        while !self.Check(SdslvTokenKind::RightBrace) && self.I < self.Tokens.len() {
            if self.MatchKw(SdslvTokenKind::KeywordCase) {
                let condition = self.ParseExpression()?;
                self.Expect(SdslvTokenKind::Arrow, "expected '->' in when case");
                let action = self.ParseFlowAction()?;
                cases.push(SdslvFlowCase {
                    Condition: condition,
                    Action: action,
                });
                continue;
            }
            if self.MatchKw(SdslvTokenKind::KeywordElse) {
                self.Expect(SdslvTokenKind::Arrow, "expected '->' after else");
                else_action = self.ParseFlowAction();
                continue;
            }
            self.ErrHere("expected case or else in when");
            self.I += 1;
        }
        self.Expect(SdslvTokenKind::RightBrace, "expected '}' after when");
        let end = self.PrevSpan();
        Some(SdslvFlowWhen {
            Cases: cases,
            ElseAction: else_action,
            Span: SdslvSpan {
                Start: start.Start,
                End: end.End,
                Line: start.Line,
                Column: start.Column,
            },
        })
    }
    fn ParseFlowAction(&mut self) -> Option<SdslvFlowAction> {
        if self.MatchKw(SdslvTokenKind::KeywordGoto) {
            let target = self.ParsePathReq("expected goto target")?;
            return Some(SdslvFlowAction::Goto(target));
        }
        if self.MatchKw(SdslvTokenKind::KeywordReturn) {
            let value = self.ParseExpression()?;
            return Some(SdslvFlowAction::Return(value));
        }
        self.ErrHere("expected goto or return flow action");
        None
    }
    fn ParseBody(&mut self) -> Option<SdslvBody> {
        let start = self.PrevSpan();
        let mut statements = vec![];
        while !self.Check(SdslvTokenKind::RightBrace) && self.I < self.Tokens.len() {
            if let Some(statement) = self.ParseStatement() {
                statements.push(statement);
            } else {
                self.RecoverStatement();
            }
        }
        if !self.MatchKw(SdslvTokenKind::RightBrace) {
            self.ErrHere("unexpected end of file while parsing block");
            return None;
        }
        let end = self.PrevSpan();
        Some(SdslvBody {
            Span: SdslvSpan {
                Start: start.Start,
                End: end.End,
                Line: start.Line,
                Column: start.Column,
            },
            Statements: statements,
        })
    }
    fn ParseStatement(&mut self) -> Option<SdslvStatement> {
        if self.MatchKw(SdslvTokenKind::Semicolon) {
            return Some(SdslvStatement::Empty);
        }
        if self.Canonical && self.MatchKw(SdslvTokenKind::KeywordComptime) {
            let start = self.PrevSpan();
            if !(self.Check(SdslvTokenKind::KeywordIf)
                || self.Check(SdslvTokenKind::KeywordLet)
                || self.Check(SdslvTokenKind::KeywordFor))
            {
                self.ErrHere("expected if, let, or for after comptime");
                return None;
            }
            let statement = Box::new(self.ParseStatement()?);
            return Some(SdslvStatement::Comptime {
                Statement: statement,
                Span: self.SpanSince(start),
            });
        }
        if self.Canonical && self.MatchKw(SdslvTokenKind::KeywordStatic) {
            let start = self.PrevSpan();
            let condition = self.ParseAssert()?;
            return Some(SdslvStatement::StaticAssert {
                Condition: condition,
                Span: self.SpanSince(start),
            });
        }
        if self.Canonical && self.MatchKw(SdslvTokenKind::KeywordFlow) {
            return self.ParseInlineFlow(self.PrevSpan());
        }
        if self.Canonical && self.MatchKw(SdslvTokenKind::KeywordWrite) {
            return self.ParseGuardedWrite(self.PrevSpan());
        }
        if self.Canonical
            && self.Check(SdslvTokenKind::KeywordWhen)
            && self
                .Tokens
                .get(self.I + 1)
                .is_some_and(|token| token.Kind == SdslvTokenKind::LeftBrace)
        {
            self.I += 1;
            return self.ParseGuardWhen(self.PrevSpan());
        }
        if self.Canonical
            && matches!(
                self.Tokens.get(self.I).map(|token| &token.Kind),
                Some(
                    SdslvTokenKind::KeywordGoto
                        | SdslvTokenKind::KeywordPush
                        | SdslvTokenKind::KeywordPop
                        | SdslvTokenKind::KeywordFinish
                )
            )
        {
            return self.ParseFlowTransition();
        }
        let mutable = self.Canonical && self.MatchKw(SdslvTokenKind::KeywordVar);
        if mutable || self.MatchKw(SdslvTokenKind::KeywordLet) {
            let start = self.PrevSpan();
            let name = self.IdentReq("expected binding name")?;
            self.Expect(SdslvTokenKind::Colon, "expected ':' in binding declaration");
            let t = self.ParseTypeRefReq("expected binding type")?;
            let init = if self.MatchKw(SdslvTokenKind::Equals) {
                Some(self.ParseExpression()?)
            } else if self.Canonical {
                self.ErrAt(
                    start,
                    "SDSL-V3703: canonical bindings require an initializer",
                );
                return None;
            } else {
                None
            };
            self.Expect(
                SdslvTokenKind::Semicolon,
                "expected ';' after let declaration",
            );
            return Some(SdslvStatement::Let {
                Name: name,
                TypeName: t,
                Initializer: init,
                Mutable: mutable || !self.Canonical,
                Span: self.SpanSince(start),
            });
        }
        if self.MatchKw(SdslvTokenKind::KeywordReturn) {
            if self.Check(SdslvTokenKind::Semicolon) {
                if self.Canonical {
                    self.I += 1;
                    return Some(SdslvStatement::ReturnVoid);
                }
                self.ErrHere("expected expression after return");
                return None;
            }
            let value = self.ParseExpression()?;
            self.Expect(SdslvTokenKind::Semicolon, "expected ';' after return");
            return Some(SdslvStatement::Return { Value: value });
        }
        if self.MatchKw(SdslvTokenKind::KeywordIf) {
            return self.ParseIfStatement();
        }
        if self.MatchKw(SdslvTokenKind::KeywordFor) {
            return self.ParseForStatement();
        }
        if self.MatchKw(SdslvTokenKind::KeywordWhile) {
            self.ErrHere(
                "while loops are not supported in SDSL-V yet; use bounded for loops instead",
            );
            return None;
        }
        if self.Check(SdslvTokenKind::KeywordStage) || self.Check(SdslvTokenKind::KeywordFn) {
            self.ErrHere("statement form not supported in SDSL-V M4 body subset");
            return None;
        }
        if let Some(SdslvToken {
            Kind: SdslvTokenKind::Identifier(name),
            ..
        }) = self.Tokens.get(self.I)
            && name == "match"
        {
            self.ErrHere("unsupported statement in SDSL-V M4 body subset");
            return None;
        }
        let start = self.CurrentSpan();
        let target = self.ParseExpression()?;
        if !self.MatchKw(SdslvTokenKind::Equals) {
            self.Expect(
                SdslvTokenKind::Semicolon,
                "expected ';' after expression statement",
            );
            return Some(SdslvStatement::Expression { Value: target });
        }
        if !self.IsAssignmentTarget(&target) {
            self.ErrHere("invalid assignment target in SDSL-V M4 body subset");
            return None;
        }
        let value = self.ParseExpression()?;
        self.Expect(SdslvTokenKind::Semicolon, "expected ';' after assignment");
        Some(SdslvStatement::Assign {
            Target: target,
            Value: value,
            Span: self.SpanSince(start),
        })
    }
    fn IsAssignmentTarget(&self, expr: &SdslvExpression) -> bool {
        match expr {
            SdslvExpression::Identifier(_) => true,
            SdslvExpression::FieldAccess { Base, .. } => self.IsAssignmentTarget(Base),
            SdslvExpression::Index { Base, .. } | SdslvExpression::MultiIndex { Base, .. } => {
                self.IsAssignmentTarget(Base)
            }
            _ => false,
        }
    }
    fn ParseExpression(&mut self) -> Option<SdslvExpression> {
        self.ParseWith(false)
    }
    fn ParseWith(&mut self, stop_at_angle: bool) -> Option<SdslvExpression> {
        let base = self.ParseBinary(1, stop_at_angle)?;
        if !self.Canonical || !self.MatchKw(SdslvTokenKind::KeywordWith) {
            return Some(base);
        }
        self.Expect(SdslvTokenKind::LeftBrace, "expected '{' after with");
        let updates = self.ParseFieldInitializers(false)?;
        Some(SdslvExpression::With {
            Base: Box::new(base),
            Updates: updates,
        })
    }
    fn ParseIfStatement(&mut self) -> Option<SdslvStatement> {
        let start = self.PrevSpan();
        let condition = if self.Check(SdslvTokenKind::LeftBrace) {
            self.ErrHere("if statement missing condition expression");
            return None;
        } else {
            self.ParseExpression()?
        };
        if !self.MatchKw(SdslvTokenKind::LeftBrace) {
            self.ErrHere("if statement missing '{' after condition");
            return None;
        }
        let then_body = self.ParseBody()?.Statements;
        let else_body = if self.MatchKw(SdslvTokenKind::KeywordElse) {
            if !self.MatchKw(SdslvTokenKind::LeftBrace) {
                self.ErrHere("else clause missing '{'");
                return None;
            }
            Some(self.ParseBody()?.Statements)
        } else {
            None
        };
        let end = self.PrevSpan();
        Some(SdslvStatement::If {
            Condition: condition,
            ThenBody: then_body,
            ElseBody: else_body,
            Span: SdslvSpan {
                Start: start.Start,
                End: end.End,
                Line: start.Line,
                Column: start.Column,
            },
        })
    }
    fn ParseForStatement(&mut self) -> Option<SdslvStatement> {
        let start_span = self.PrevSpan();
        let iterator = self.IdentReq("for statement missing iterator identifier")?;
        if !self.MatchKw(SdslvTokenKind::KeywordIn) {
            self.ErrHere("for statement missing 'in' after iterator");
            return None;
        }
        let start = self.ParseExpression().or_else(|| {
            self.ErrHere("for statement missing range start expression");
            None
        })?;
        if !self.MatchKw(SdslvTokenKind::Range) {
            self.ErrHere("for statement missing '..' range operator");
            return None;
        }
        let end = self.ParseExpression().or_else(|| {
            self.ErrHere("for statement missing range end expression");
            None
        })?;
        let step = if self.MatchKw(SdslvTokenKind::KeywordStep) {
            Some(self.ParseExpression().or_else(|| {
                self.ErrHere("for statement missing step expression");
                None
            })?)
        } else {
            None
        };
        if !self.MatchKw(SdslvTokenKind::LeftBrace) {
            self.ErrHere("for statement missing '{' loop body");
            return None;
        }
        let body = self.ParseBody()?.Statements;
        let end_span = self.PrevSpan();
        Some(SdslvStatement::For {
            Iterator: iterator,
            Start: start,
            End: end,
            Step: step,
            Body: body,
            Span: SdslvSpan {
                Start: start_span.Start,
                End: end_span.End,
                Line: start_span.Line,
                Column: start_span.Column,
            },
        })
    }
    fn MatchSwitchArmArrow(&mut self, missing_message: &str) -> bool {
        if self.MatchKw(SdslvTokenKind::Arrow) || self.MatchKw(SdslvTokenKind::FatArrow) {
            return true;
        }
        self.ErrHere(missing_message);
        false
    }
    fn ParseSwitchExpression(&mut self) -> Option<SdslvExpression> {
        let start = self.PrevSpan();
        let subject = if self.Check(SdslvTokenKind::LeftBrace) {
            None
        } else {
            Some(Box::new(self.ParseExpression().or_else(|| {
                self.ErrHere("switch expression missing subject expression");
                None
            })?))
        };
        if !self.MatchKw(SdslvTokenKind::LeftBrace) {
            self.ErrHere("switch expression missing '{'");
            return None;
        }
        let mut cases = vec![];
        let mut else_value = None;
        while !self.Check(SdslvTokenKind::RightBrace) && self.I < self.Tokens.len() {
            if self.MatchKw(SdslvTokenKind::KeywordCase) {
                let case_start = self.PrevSpan();
                let condition = self.ParseExpression()?;
                if !self.MatchSwitchArmArrow("switch case missing '->' or '=>' arrow") {
                    return None;
                }
                let value = self.ParseExpression().or_else(|| {
                    self.ErrHere("switch case missing value expression");
                    None
                })?;
                cases.push(SdslvSwitchCase {
                    Condition: condition,
                    Value: value,
                    Span: case_start,
                });
                continue;
            }
            if self.MatchKw(SdslvTokenKind::KeywordElse) {
                if !self.MatchSwitchArmArrow("switch else arm missing '->' or '=>' arrow") {
                    return None;
                }
                else_value = self.ParseExpression().or_else(|| {
                    self.ErrHere("switch else arm missing value expression");
                    None
                });
                continue;
            }
            self.ErrHere("expected case or else in switch expression");
            return None;
        }
        if !self.MatchKw(SdslvTokenKind::RightBrace) {
            self.ErrHere("expected '}' after switch expression");
            return None;
        }
        if cases.is_empty() {
            self.ErrHere("switch expression requires at least one case");
            return None;
        }
        let Some(else_expr) = else_value else {
            self.ErrHere("switch expression requires else arm");
            return None;
        };
        let end = self.PrevSpan();
        Some(SdslvExpression::Switch {
            Subject: subject,
            Cases: cases,
            ElseValue: Box::new(else_expr),
            Span: SdslvSpan {
                Start: start.Start,
                End: end.End,
                Line: start.Line,
                Column: start.Column,
            },
        })
    }
    fn ParseBinary(&mut self, min_precedence: u8, stop_at_angle: bool) -> Option<SdslvExpression> {
        let mut left = self.ParseUnary()?;
        while let Some((operator, precedence)) = self.BinaryOperator(stop_at_angle) {
            if precedence < min_precedence {
                break;
            }
            self.I += 1;
            let right = self.ParseBinary(precedence + 1, stop_at_angle)?;
            left = SdslvExpression::Binary {
                Left: Box::new(left),
                Operator: operator,
                Right: Box::new(right),
            };
        }
        Some(left)
    }

    fn BinaryOperator(&self, stop_at_angle: bool) -> Option<(SdslvBinaryOperator, u8)> {
        use SdslvBinaryOperator as Operator;
        use SdslvTokenKind as Token;

        // Oct's SDSL-V precedence. C/HLSL order the bitwise operators differently.
        let pair = match self.Tokens.get(self.I)?.Kind {
            Token::KeywordOr => (Operator::Or, 1),
            Token::KeywordAnd => (Operator::And, 2),
            Token::DoubleEquals => (Operator::Equal, 3),
            Token::BangEquals => (Operator::NotEqual, 3),
            Token::LeftAngle => (Operator::Less, 3),
            Token::LeftAngleEquals => (Operator::LessEqual, 3),
            Token::RightAngle if !stop_at_angle => (Operator::Greater, 3),
            Token::RightAngleEquals => (Operator::GreaterEqual, 3),
            Token::BitOr => (Operator::BitOr, 4),
            Token::BitXor => (Operator::BitXor, 5),
            Token::BitAnd => (Operator::BitAnd, 6),
            Token::ShiftLeft => (Operator::ShiftLeft, 7),
            Token::ShiftRight if !stop_at_angle => (Operator::ShiftRight, 7),
            Token::Plus => (Operator::Add, 8),
            Token::Minus => (Operator::Subtract, 8),
            Token::Star => (Operator::Multiply, 9),
            Token::Slash => (Operator::Divide, 9),
            Token::Percent => (Operator::Remainder, 9),
            _ => return None,
        };
        Some(pair)
    }

    fn ParseUnary(&mut self) -> Option<SdslvExpression> {
        let operator = match self.Tokens.get(self.I).map(|token| &token.Kind) {
            Some(SdslvTokenKind::Minus) => SdslvUnaryOperator::Negate,
            Some(SdslvTokenKind::KeywordNot) => SdslvUnaryOperator::Not,
            Some(SdslvTokenKind::BitNot) => SdslvUnaryOperator::BitNot,
            Some(SdslvTokenKind::Bang) if self.Canonical => {
                self.ErrHere("use 'not' for logical negation");
                return None;
            }
            _ => return self.ParsePostfix(),
        };
        self.I += 1;
        Some(SdslvExpression::Unary {
            Operator: operator,
            Operand: Box::new(self.ParseUnary()?),
        })
    }
    fn ParsePostfix(&mut self) -> Option<SdslvExpression> {
        let start = self.CurrentSpan();
        let mut expr = self.ParsePrimary()?;
        loop {
            if self.MatchKw(SdslvTokenKind::Dot) {
                let field = self.IdentReq("expected identifier after '.'")?;
                expr = SdslvExpression::FieldAccess {
                    Base: Box::new(expr),
                    Field: field,
                };
            } else if self.MatchKw(SdslvTokenKind::LeftParen) {
                expr = SdslvExpression::Call {
                    Callee: Box::new(expr),
                    Arguments: self.ParseCallArguments()?,
                };
            } else if self.Canonical
                && matches!(
                    &expr,
                    SdslvExpression::Identifier(_) | SdslvExpression::FieldAccess { .. }
                )
                && self.LooksLikeGenericCall()
            {
                self.I += 1;
                let mut types = vec![self.ParseTypeRefReq("expected type argument")?];
                while self.MatchKw(SdslvTokenKind::Comma) {
                    types.push(self.ParseTypeRefReq("expected type argument")?);
                }
                self.ExpectTypeClose("expected '>' after call type arguments")?;
                self.Expect(
                    SdslvTokenKind::LeftParen,
                    "expected '(' after call type arguments",
                );
                expr = SdslvExpression::GenericCall {
                    Callee: Box::new(expr),
                    TypeArguments: types,
                    Arguments: self.ParseCallArguments()?,
                    Span: self.SpanSince(start),
                };
            } else if self.Canonical
                && self.Check(SdslvTokenKind::LeftBrace)
                && matches!(
                    self.Tokens.get(self.I + 1).map(|token| &token.Kind),
                    Some(SdslvTokenKind::Identifier(_))
                )
                && self
                    .Tokens
                    .get(self.I + 2)
                    .is_some_and(|token| token.Kind == SdslvTokenKind::Colon)
            {
                let Some(path) = Self::QualifiedExpressionPath(&expr) else {
                    break;
                };
                self.I += 1;
                expr = SdslvExpression::Construct {
                    TypeName: path,
                    Fields: self.ParseFieldInitializers(true)?,
                    Span: self.SpanSince(start),
                };
            } else if !self.Canonical && self.MatchKw(SdslvTokenKind::KeywordWith) {
                self.Expect(SdslvTokenKind::LeftBrace, "expected '{' after with keyword");
                let updates = self.ParseFieldInitializers(false)?;
                if updates.is_empty() {
                    self.ErrHere("with expression requires at least one field update");
                    return None;
                }
                expr = SdslvExpression::With {
                    Base: Box::new(expr),
                    Updates: updates,
                };
            } else if self.MatchKw(SdslvTokenKind::LeftBracket) {
                let index = self.ParseExpression().or_else(|| {
                    self.ErrHere("expected array index expression");
                    None
                })?;
                if self.Canonical && self.MatchKw(SdslvTokenKind::Comma) {
                    let mut indices = vec![index];
                    loop {
                        indices.push(self.ParseExpression()?);
                        if !self.MatchKw(SdslvTokenKind::Comma) {
                            break;
                        }
                    }
                    self.Expect(SdslvTokenKind::RightBracket, "expected ']' after indices");
                    expr = SdslvExpression::MultiIndex {
                        Base: Box::new(expr),
                        Indices: indices,
                        Span: self.SpanSince(start),
                    };
                    continue;
                }
                self.Expect(
                    SdslvTokenKind::RightBracket,
                    "expected ']' after array index expression",
                );
                expr = SdslvExpression::Index {
                    Base: Box::new(expr),
                    Index: Box::new(index),
                    Span: self.PrevSpan(),
                };
            } else if self.MatchKw(SdslvTokenKind::Question) {
                let span = self.PrevSpan();
                expr = SdslvExpression::TryPropagate {
                    Expression: Box::new(expr),
                    Span: span,
                };
            } else if self.MatchKw(SdslvTokenKind::Bang) {
                let span = self.PrevSpan();
                if self.Canonical {
                    self.ErrAt(
                        span,
                        "SDSL-V4100: postfix '!' was removed; use a payload enum",
                    );
                    return None;
                }
                expr = SdslvExpression::Unwrap {
                    Expression: Box::new(expr),
                    Span: span,
                };
            } else {
                break;
            }
        }
        Some(expr)
    }

    fn ParseCallArguments(&mut self) -> Option<Vec<SdslvExpression>> {
        let mut arguments = vec![];
        if !self.Check(SdslvTokenKind::RightParen) {
            loop {
                arguments.push(self.ParseExpression()?);
                if !self.MatchKw(SdslvTokenKind::Comma) {
                    break;
                }
            }
        }
        self.Expect(
            SdslvTokenKind::RightParen,
            "expected ')' to close function call",
        );
        Some(arguments)
    }

    fn ParseMatchExpression(&mut self) -> Option<SdslvExpression> {
        let start = self.PrevSpan();
        let subject = Box::new(self.ParseExpression().or_else(|| {
            self.ErrHere("match expression missing subject expression");
            None
        })?);
        self.Expect(
            SdslvTokenKind::LeftBrace,
            "expected '{' after match subject",
        );
        let mut arms = vec![];
        let mut has_fallible_arm = false;
        let mut has_enum_arm = false;
        while !self.Check(SdslvTokenKind::RightBrace) && self.I < self.Tokens.len() {
            let span = self.CurrentSpan();
            if self.Canonical
                && (self.Check(SdslvTokenKind::KeywordOk) || self.Check(SdslvTokenKind::KeywordErr))
            {
                self.ErrAt(span, "SDSL-V4100: use payload enum variants in match arms");
                return None;
            }
            let arm_kind = if self.MatchKw(SdslvTokenKind::KeywordOk) {
                has_fallible_arm = true;
                let binding = self.ParseFallibleMatchBinding("ok")?;
                SdslvMatchArmKind::FallibleOk { Binding: binding }
            } else if self.MatchKw(SdslvTokenKind::KeywordErr) {
                has_fallible_arm = true;
                let binding = self.ParseFallibleMatchBinding("err")?;
                SdslvMatchArmKind::FallibleErr { Binding: binding }
            } else {
                has_enum_arm = true;
                let variant_path = self.ParsePathReq("expected Enum.Variant in match arm")?;
                if self.Canonical && self.MatchKw(SdslvTokenKind::LeftParen) {
                    let binding = self.IdentReq("expected enum payload binding")?;
                    self.Expect(
                        SdslvTokenKind::RightParen,
                        "expected ')' after enum payload binding",
                    );
                    SdslvMatchArmKind::PayloadVariant {
                        VariantPath: variant_path,
                        Binding: binding,
                    }
                } else {
                    SdslvMatchArmKind::EnumVariant {
                        VariantPath: variant_path,
                    }
                }
            };
            if has_fallible_arm && has_enum_arm {
                self.ErrHere("fallible match cannot mix ok/err arms with enum variant arms");
                return None;
            }
            if !self.MatchSwitchArmArrow("expected '=>' or '->' in match arm") {
                return None;
            }
            let value = self.ParseExpression().or_else(|| {
                self.ErrHere("expected match arm expression after arrow");
                None
            })?;
            arms.push(SdslvMatchArm {
                Kind: arm_kind,
                Value: value,
                Span: span,
            });
            if self.Canonical {
                self.MatchKw(SdslvTokenKind::Comma);
            }
        }
        self.Expect(SdslvTokenKind::RightBrace, "expected '}' after match arms");
        let end = self.PrevSpan();
        Some(SdslvExpression::Match {
            Subject: subject,
            Arms: arms,
            Span: SdslvSpan {
                Start: start.Start,
                End: end.End,
                Line: start.Line,
                Column: start.Column,
            },
        })
    }

    fn ParseFallibleMatchBinding(&mut self, arm_name: &str) -> Option<String> {
        self.Expect(
            SdslvTokenKind::LeftParen,
            &format!("expected '(' after {} in match arm", arm_name),
        );
        let binding = self.IdentReq("expected binding name in fallible match arm")?;
        self.Expect(
            SdslvTokenKind::RightParen,
            &format!("expected ')' after {} binding", arm_name),
        );
        Some(binding)
    }

    fn ParsePrimary(&mut self) -> Option<SdslvExpression> {
        if self.Canonical && self.MatchKw(SdslvTokenKind::KeywordRead) {
            return self.ParseGuardedRead();
        }
        if self.Canonical && self.MatchKw(SdslvTokenKind::KeywordHlsl) {
            return self.ParseForeignHlsl();
        }
        if self.MatchKw(SdslvTokenKind::KeywordWhen) {
            return self.ParseWhenExpression();
        }
        if !self.Canonical && self.MatchKw(SdslvTokenKind::KeywordSwitch) {
            return self.ParseSwitchExpression();
        }
        if self.MatchKw(SdslvTokenKind::KeywordMatch) {
            return self.ParseMatchExpression();
        }
        if self.MatchKw(SdslvTokenKind::LeftBracket) {
            let span = self.PrevSpan();
            let mut elements = vec![];
            if !self.Check(SdslvTokenKind::RightBracket) {
                loop {
                    let element = self.ParseExpression().or_else(|| {
                        self.ErrHere("expected expression in array literal");
                        None
                    })?;
                    elements.push(element);
                    if self.MatchKw(SdslvTokenKind::Comma) {
                        if self.Check(SdslvTokenKind::RightBracket) {
                            break;
                        }
                        continue;
                    }
                    break;
                }
            }
            self.Expect(
                SdslvTokenKind::RightBracket,
                "expected ']' after array literal",
            );
            return Some(SdslvExpression::ArrayLiteral {
                Elements: elements,
                Span: span,
            });
        }
        if self.MatchKw(SdslvTokenKind::LeftParen) {
            let expr = self.ParseExpression()?;
            self.Expect(
                SdslvTokenKind::RightParen,
                "expected ')' to close grouped expression",
            );
            return Some(expr);
        }
        if self.I >= self.Tokens.len() {
            self.ErrHere("unexpected token in expression");
            return None;
        }
        match &self.Tokens[self.I].Kind {
            SdslvTokenKind::KeywordTrue => {
                self.I += 1;
                Some(SdslvExpression::BoolLiteral(true))
            }
            SdslvTokenKind::KeywordFalse => {
                self.I += 1;
                Some(SdslvExpression::BoolLiteral(false))
            }
            SdslvTokenKind::Identifier(name) => {
                let value = name.clone();
                self.I += 1;
                if value == "true" {
                    Some(SdslvExpression::BoolLiteral(true))
                } else if value == "false" {
                    Some(SdslvExpression::BoolLiteral(false))
                } else {
                    Some(SdslvExpression::Identifier(value))
                }
            }
            SdslvTokenKind::KeywordBoard => {
                self.I += 1;
                Some(SdslvExpression::Identifier("board".to_string()))
            }
            SdslvTokenKind::IntegerLiteral(value) => {
                let span = self.CurrentSpan();
                let mut out = value.clone();
                self.I += 1;
                if out.starts_with("0x") || out.starts_with("0X") {
                    let unsigned = out.ends_with(['u', 'U']);
                    let digits = out[2..].trim_end_matches(['u', 'U']);
                    let parsed = u32::from_str_radix(digits, 16);
                    let Ok(number) = parsed else {
                        self.ErrAt(
                            span,
                            "SDSL-V4310: hexadecimal literal must fit its i32 or u32 type",
                        );
                        return None;
                    };
                    if !unsigned && number > i32::MAX as u32 {
                        self.ErrAt(
                            span,
                            "SDSL-V4310: hexadecimal literal must fit its i32 or u32 type",
                        );
                        return None;
                    }
                    out = if unsigned {
                        format!("{number}u")
                    } else {
                        number.to_string()
                    };
                }
                Some(SdslvExpression::IntegerLiteral(out))
            }
            SdslvTokenKind::FloatLiteral(value) => {
                let out = value.clone();
                self.I += 1;
                Some(SdslvExpression::FloatLiteral(out))
            }
            SdslvTokenKind::StringLiteral(value) => {
                let out = value.clone();
                self.I += 1;
                Some(SdslvExpression::StringLiteral(out))
            }
            _ => {
                self.ErrHere("unexpected token in expression");
                None
            }
        }
    }
    fn ParseWhenExpression(&mut self) -> Option<SdslvExpression> {
        let start = self.PrevSpan();
        if self.MatchKw(SdslvTokenKind::KeywordPolicy) {
            self.ErrHere("when policy is only valid inside flow/state bodies; use when utility for standalone ranked expressions");
            return None;
        }
        if !self.MatchKw(SdslvTokenKind::KeywordUtility) {
            self.ErrHere("missing 'utility' after 'when'");
            return None;
        }
        if !self.MatchKw(SdslvTokenKind::LeftBrace) {
            self.ErrHere("when utility expression missing '{'");
            return None;
        }
        let mut options = None;
        if !self.Check(SdslvTokenKind::KeywordCase) {
            options = Some(self.ParseWhenUtilityOptionsFromOpenBrace(start)?);
            if !self.MatchKw(SdslvTokenKind::LeftBrace) {
                self.ErrHere("when utility expression missing '{' before cases");
                return None;
            }
        }
        let mut cases = vec![];
        let mut else_value = None;
        while !self.Check(SdslvTokenKind::RightBrace) && self.I < self.Tokens.len() {
            if self.MatchKw(SdslvTokenKind::KeywordCase) {
                let case_span = self.PrevSpan();
                let value = self.ParseExpression().or_else(|| {
                    self.ErrHere("when utility case missing result expression");
                    None
                })?;
                if !self.MatchKw(SdslvTokenKind::KeywordWhen) {
                    self.ErrHere("when utility case missing when guard");
                    return None;
                }
                let guard = self.ParseExpression().or_else(|| {
                    self.ErrHere("when utility case missing guard expression");
                    None
                })?;
                if !self.MatchKw(SdslvTokenKind::KeywordScore) {
                    self.ErrHere("when utility case missing score");
                    return None;
                }
                let score = self.ParseExpression().or_else(|| {
                    self.ErrHere("when utility case missing score expression");
                    None
                })?;
                cases.push(SdslvUtilityCase {
                    Value: value,
                    Guard: guard,
                    Score: score,
                    Span: case_span,
                });
                continue;
            }
            if self.MatchKw(SdslvTokenKind::KeywordElse) {
                else_value = self.ParseExpression().or_else(|| {
                    self.ErrHere("when utility missing else value expression");
                    None
                });
                continue;
            }
            self.ErrHere("expected case or else in when utility expression");
            return None;
        }
        self.Expect(
            SdslvTokenKind::RightBrace,
            "expected '}' after when utility expression",
        );
        if cases.is_empty() {
            self.ErrHere("when utility expression requires at least one case");
            return None;
        }
        let Some(else_expr) = else_value else {
            self.ErrHere("when utility expression requires else arm");
            return None;
        };
        let end = self.PrevSpan();
        Some(SdslvExpression::WhenUtility {
            Options: options.map(Box::new),
            Cases: cases,
            ElseValue: Box::new(else_expr),
            Span: SdslvSpan {
                Start: start.Start,
                End: end.End,
                Line: start.Line,
                Column: start.Column,
            },
        })
    }
    fn ParseWhenUtilityOptionsFromOpenBrace(
        &mut self,
        start: SdslvSpan,
    ) -> Option<SdslvUtilityOptions> {
        let mut hysteresis = None;
        let mut min_commit = None;
        loop {
            if self.MatchKw(SdslvTokenKind::KeywordHysteresis) {
                if hysteresis.is_some() {
                    self.ErrHere("duplicate when utility option 'hysteresis'");
                    return None;
                }
                self.Expect(
                    SdslvTokenKind::Colon,
                    "expected ':' after hysteresis option name",
                );
                hysteresis = Some(self.ParseExpression()?);
            } else if self.MatchKw(SdslvTokenKind::KeywordMinCommit) {
                if min_commit.is_some() {
                    self.ErrHere("duplicate when utility option 'min_commit'");
                    return None;
                }
                self.Expect(
                    SdslvTokenKind::Colon,
                    "expected ':' after min_commit option name",
                );
                min_commit = Some(self.ParseExpression()?);
            } else {
                self.ErrHere("unknown utility option name");
                return None;
            }
            if self.Check(SdslvTokenKind::RightBrace) {
                break;
            }
        }
        self.Expect(
            SdslvTokenKind::RightBrace,
            "expected '}' after when utility options",
        );
        Some(SdslvUtilityOptions {
            Hysteresis: hysteresis,
            MinCommit: min_commit,
            Span: SdslvSpan {
                Start: start.Start,
                End: self.PrevSpan().End,
                Line: start.Line,
                Column: start.Column,
            },
        })
    }
    fn RecoverStatement(&mut self) {
        while self.I < self.Tokens.len()
            && !self.Check(SdslvTokenKind::Semicolon)
            && !self.Check(SdslvTokenKind::RightBrace)
        {
            self.I += 1;
        }
        if self.Check(SdslvTokenKind::Semicolon) {
            self.I += 1;
        }
    }
    fn ParsePathReq(&mut self, msg: &str) -> Option<SdslvPath> {
        let first = match self.Ident() {
            Some(x) => x,
            None => {
                self.ErrHere(msg);
                return None;
            }
        };
        let mut seg = vec![first];
        while self.MatchKw(SdslvTokenKind::Dot) {
            seg.push(self.IdentReq("expected identifier after '.'")?);
        }
        Some(SdslvPath { Segments: seg })
    }

    fn ParseTypeRefReq(&mut self, msg: &str) -> Option<SdslvTypeRef> {
        if self.Canonical {
            return self.ParseCanonicalType(msg);
        }
        if self.Check(SdslvTokenKind::KeywordArray) {
            let start = self.Tokens[self.I].Span;
            self.I += 1;
            self.Expect(SdslvTokenKind::LeftAngle, "expected '<' after array");
            let element = self.ParseTypeRefReq("expected element type in array type")?;
            self.Expect(
                SdslvTokenKind::Comma,
                "expected ',' after array element type",
            );
            let length_text = if let Some(SdslvToken {
                Kind: SdslvTokenKind::IntegerLiteral(value),
                ..
            }) = self.Tokens.get(self.I)
            {
                self.I += 1;
                value.clone()
            } else {
                self.ErrHere("array length must be a positive integer literal");
                return None;
            };
            self.Expect(
                SdslvTokenKind::RightAngle,
                "expected '>' after array length",
            );
            let Ok(length) = length_text.parse::<usize>() else {
                self.ErrHere("array length must be a positive integer literal");
                return None;
            };
            if length == 0 {
                self.ErrHere("array length must be a positive integer literal");
                return None;
            }
            return Some(SdslvTypeRef::Array {
                Element: Box::new(element),
                Length: length,
                Span: SdslvSpan {
                    Start: start.Start,
                    End: self.PrevSpan().End,
                    Line: start.Line,
                    Column: start.Column,
                },
            });
        }
        let path = self.ParsePathReq(msg)?;
        if self.MatchKw(SdslvTokenKind::LeftAngle) {
            self.ErrHere("generic type references are not supported in SDSL-V M59b");
            return None;
        }
        Some(SdslvTypeRef::Named(path))
    }
    fn IdentReq(&mut self, m: &str) -> Option<String> {
        if let Some(keyword_name) = self.CurrentReservedKeywordName() {
            self.ErrHere(&format!(
                "'{}' is a reserved keyword in SDSL-V and cannot be used as an identifier",
                keyword_name
            ));
            return None;
        }
        let r = self.Ident();
        if r.is_none() {
            self.ErrHere(m);
        }
        r
    }
    fn Ident(&mut self) -> Option<String> {
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
    fn Expect(&mut self, k: SdslvTokenKind, m: &str) {
        if !self.MatchKw(k) {
            self.ErrHere(m);
        }
    }
    fn MatchKw(&mut self, k: SdslvTokenKind) -> bool {
        if self.Check(k) {
            self.I += 1;
            true
        } else {
            false
        }
    }
    fn MatchIdent(&mut self, name: &str) -> bool {
        if matches!(self.Tokens.get(self.I).map(|token| &token.Kind), Some(SdslvTokenKind::Identifier(value)) if value == name)
        {
            self.I += 1;
            true
        } else {
            false
        }
    }
    fn Check(&self, k: SdslvTokenKind) -> bool {
        if self.I >= self.Tokens.len() {
            return false;
        }
        std::mem::discriminant(&self.Tokens[self.I].Kind) == std::mem::discriminant(&k)
    }
    fn ErrHere(&mut self, m: &str) {
        self.ErrAt(self.CurrentSpan(), m);
    }
    fn ErrAt(&mut self, span: SdslvSpan, message: &str) {
        self.Diagnostics.push(SdslvDiagnostic::New(message, span));
    }
    fn SpanSince(&self, start: SdslvSpan) -> SdslvSpan {
        SdslvSpan {
            End: self.PrevSpan().End.max(start.End),
            ..start
        }
    }
    fn CurrentReservedKeywordName(&self) -> Option<&'static str> {
        let token = self.Tokens.get(self.I)?;
        match token.Kind {
            SdslvTokenKind::KeywordFlow => Some("flow"),
            SdslvTokenKind::KeywordState => Some("state"),
            SdslvTokenKind::KeywordStep => Some("step"),
            SdslvTokenKind::KeywordWhile => Some("while"),
            SdslvTokenKind::KeywordFor => Some("for"),
            SdslvTokenKind::KeywordSwitch => Some("switch"),
            SdslvTokenKind::KeywordCase => Some("case"),
            SdslvTokenKind::KeywordElse => Some("else"),
            SdslvTokenKind::KeywordWhen => Some("when"),
            SdslvTokenKind::KeywordUtility => Some("utility"),
            SdslvTokenKind::KeywordPolicy => Some("policy"),
            SdslvTokenKind::KeywordScore => Some("score"),
            SdslvTokenKind::KeywordHysteresis => Some("hysteresis"),
            SdslvTokenKind::KeywordMinCommit => Some("min_commit"),
            SdslvTokenKind::KeywordIf => Some("if"),
            SdslvTokenKind::KeywordRecord => Some("record"),
            SdslvTokenKind::KeywordStream => Some("stream"),
            SdslvTokenKind::KeywordArray => Some("array"),
            SdslvTokenKind::KeywordShader => Some("shader"),
            SdslvTokenKind::KeywordInterface => Some("interface"),
            SdslvTokenKind::KeywordFn => Some("fn"),
            SdslvTokenKind::KeywordLet => Some("let"),
            SdslvTokenKind::KeywordReturn => Some("return"),
            _ => None,
        }
    }
    fn PrevSpan(&self) -> SdslvSpan {
        self.Tokens[self.I - 1].Span
    }
    fn CurrentSpan(&self) -> SdslvSpan {
        self.Tokens
            .get(self.I)
            .map(|x| x.Span)
            .unwrap_or_else(|| SdslvSpan {
                Start: self.Source.len(),
                End: self.Source.len(),
                Line: self.Source.bytes().filter(|byte| *byte == b'\n').count() + 1,
                Column: self
                    .Source
                    .rsplit('\n')
                    .next()
                    .unwrap_or("")
                    .chars()
                    .count()
                    + 1,
            })
    }
}
