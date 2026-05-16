#![allow(non_snake_case)]

use super::ast::*;
use super::diagnostic::SdslvDiagnostic;
use super::lexer::LexSource;
use super::token::{SdslvSpan, SdslvToken, SdslvTokenKind};

pub fn ParseSource(source: &str) -> Result<SdslvModule, Vec<SdslvDiagnostic>> {
    let tokens = LexSource(source)?;
    Parser::New(source, tokens).ParseModule()
}

pub fn ParseTestSource(source: &str) -> Result<SdslvTestModule, Vec<SdslvDiagnostic>> {
    let tokens = LexSource(source)?;
    Parser::New(source, tokens).ParseTestModule()
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
        /* unchanged */
        let mut m = SdslvModule {
            Namespace: None,
            Uses: vec![],
            Declarations: vec![],
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
            } else if self.MatchKw(SdslvTokenKind::KeywordType) {
                if let Some(d) = self.ParseType() {
                    m.Declarations.push(SdslvDecl::TypeAlias(d));
                }
            } else if self.MatchKw(SdslvTokenKind::KeywordStream) {
                if let Some(d) = self.ParseStream() {
                    m.Declarations.push(SdslvDecl::Stream(d));
                }
            } else if self.MatchKw(SdslvTokenKind::KeywordRecord) {
                if let Some(d) = self.ParseRecord() {
                    m.Declarations.push(SdslvDecl::Record(d));
                }
            } else if self.MatchKw(SdslvTokenKind::KeywordInterface) {
                if let Some(d) = self.ParseInterface() {
                    m.Declarations.push(SdslvDecl::Interface(d));
                }
            } else if self.MatchKw(SdslvTokenKind::KeywordShader) {
                if let Some(d) = self.ParseShader() {
                    m.Declarations.push(SdslvDecl::Shader(d));
                }
            } else if self.MatchKw(SdslvTokenKind::KeywordFlow) {
                if let Some(d) = self.ParseFlow() {
                    m.Declarations.push(SdslvDecl::Flow(d));
                }
            } else if self.MatchKw(SdslvTokenKind::KeywordCompile) {
                if let Some(d) = self.ParseCompile() {
                    m.Declarations.push(SdslvDecl::Compile(d));
                }
            } else {
                self.ErrHere("unexpected token at top level");
                self.I += 1;
            }
        }
        if self.Diagnostics.is_empty() {
            Ok(m)
        } else {
            Err(self.Diagnostics)
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
                Span: start,
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
            let t = self.ParsePathReq("expected parameter type")?;
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
            IsOverride: false,
            Stage: None,
            Name: name,
            Parameters: ps,
            ReturnType: SdslvPath {
                Segments: vec!["void".to_string()],
            },
            Body: body,
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
            let arg = self.ParsePathReq("expected type argument path")?;
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
        let t = self.ParsePathReq("expected type name in type alias")?;
        let mut s = None;
        if self.MatchKw(SdslvTokenKind::At) {
            let _ = self.Ident();
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
            let fname = self.IdentReq(&format!("expected {}", field_context))?;
            self.Expect(SdslvTokenKind::Colon, "expected ':' after field name");
            let t = self.ParsePathReq("expected field type")?;
            self.Expect(SdslvTokenKind::Semicolon, "expected ';' after field");
            fs.push(SdslvFieldDecl {
                Name: fname,
                TypeName: t,
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
            if let Some(f) = self.ParseFn(None, false) {
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
    fn ParseShader(&mut self) -> Option<SdslvShaderDecl> {
        let name = self.Ident()?;
        let mut gps = vec![];
        if self.MatchKw(SdslvTokenKind::LeftAngle) {
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
        while !self.Check(SdslvTokenKind::RightBrace) && self.I < self.Tokens.len() {
            if self.MatchKw(SdslvTokenKind::KeywordMaterial) {
                self.Expect(SdslvTokenKind::LeftBrace, "expected '{' after material");
                while !self.Check(SdslvTokenKind::RightBrace) && self.I < self.Tokens.len() {
                    let n = self.Ident()?;
                    self.Expect(SdslvTokenKind::Colon, "expected ':' after material field");
                    let t = self.ParsePathReq("expected material type")?;
                    self.Expect(SdslvTokenKind::Semicolon, "expected ';'");
                    mat.push(SdslvFieldDecl {
                        Name: n,
                        TypeName: t,
                    });
                }
                self.Expect(SdslvTokenKind::RightBrace, "expected '}' after material");
            } else if self.MatchKw(SdslvTokenKind::KeywordStage) {
                let stage = self.Ident();
                if let Some(f) = self.ParseFn(stage, false) {
                    sm.push(f);
                }
            } else if self.MatchKw(SdslvTokenKind::KeywordOverride) {
                if let Some(f) = self.ParseFn(None, true) {
                    ms.push(f);
                }
            } else if self.Check(SdslvTokenKind::KeywordFn) {
                if let Some(f) = self.ParseFn(None, false) {
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
        })
    }
    fn ParseFn(&mut self, stage: Option<String>, ov: bool) -> Option<SdslvFunctionDecl> {
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
            let t = self.ParsePathReq("expected parameter type")?;
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
        let rt = self.ParsePathReq("expected return type")?;
        let body = if self.MatchKw(SdslvTokenKind::Semicolon) {
            None
        } else if self.MatchKw(SdslvTokenKind::LeftBrace) {
            self.ParseBody()
        } else {
            self.ErrHere("expected ';' or function body");
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
    fn ParseFlow(&mut self) -> Option<SdslvFlowDecl> {
        let start = self.PrevSpan();
        let name = self.Ident()?;
        self.Expect(SdslvTokenKind::LeftParen, "expected '(' in flow signature");
        let mut parameters = vec![];
        while !self.Check(SdslvTokenKind::RightParen) && self.I < self.Tokens.len() {
            let n = self.Ident()?;
            self.Expect(SdslvTokenKind::Colon, "expected ':' in parameter");
            let t = self.ParsePathReq("expected parameter type")?;
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
        let return_type = self.ParsePathReq("expected return type in flow declaration")?;
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
            let type_name = self.ParsePathReq("expected board field type")?;
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
        if self.MatchKw(SdslvTokenKind::KeywordLet) {
            let name = self.IdentReq("expected identifier after let")?;
            self.Expect(SdslvTokenKind::Colon, "expected ':' in let declaration");
            let t = self.ParsePathReq("expected type in let declaration")?;
            let init = if self.MatchKw(SdslvTokenKind::Equals) {
                Some(self.ParseExpression()?)
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
            });
        }
        if self.MatchKw(SdslvTokenKind::KeywordReturn) {
            if self.Check(SdslvTokenKind::Semicolon) {
                self.ErrHere("expected expression after return");
                return None;
            }
            let value = self.ParseExpression()?;
            self.Expect(SdslvTokenKind::Semicolon, "expected ';' after return");
            return Some(SdslvStatement::Return { Value: value });
        }
        if self.Check(SdslvTokenKind::KeywordStage) || self.Check(SdslvTokenKind::KeywordFn) {
            self.ErrHere("statement form not supported in SDSL-V M4 body subset");
            return None;
        }
        if let Some(SdslvToken {
            Kind: SdslvTokenKind::Identifier(name),
            ..
        }) = self.Tokens.get(self.I)
            && (name == "if" || name == "for" || name == "while" || name == "match")
        {
            self.ErrHere("unsupported statement in SDSL-V M4 body subset");
            return None;
        }
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
        })
    }
    fn IsAssignmentTarget(&self, expr: &SdslvExpression) -> bool {
        matches!(
            expr,
            SdslvExpression::Identifier(_) | SdslvExpression::FieldAccess { .. }
        )
    }
    fn ParseExpression(&mut self) -> Option<SdslvExpression> {
        self.ParseComparison()
    }
    fn ParseComparison(&mut self) -> Option<SdslvExpression> {
        let mut left = self.ParseAdditive()?;
        loop {
            let op = if self.MatchKw(SdslvTokenKind::DoubleEquals) {
                Some(SdslvBinaryOperator::Equal)
            } else if self.MatchKw(SdslvTokenKind::BangEquals) {
                Some(SdslvBinaryOperator::NotEqual)
            } else if self.MatchKw(SdslvTokenKind::LeftAngleEquals) {
                Some(SdslvBinaryOperator::LessEqual)
            } else if self.MatchKw(SdslvTokenKind::RightAngleEquals) {
                Some(SdslvBinaryOperator::GreaterEqual)
            } else if self.MatchKw(SdslvTokenKind::LeftAngle) {
                Some(SdslvBinaryOperator::Less)
            } else if self.MatchKw(SdslvTokenKind::RightAngle) {
                Some(SdslvBinaryOperator::Greater)
            } else {
                None
            };
            let Some(operator) = op else { break };
            let right = self.ParseAdditive()?;
            left = SdslvExpression::Binary {
                Left: Box::new(left),
                Operator: operator,
                Right: Box::new(right),
            };
        }
        Some(left)
    }
    fn ParseAdditive(&mut self) -> Option<SdslvExpression> {
        let mut left = self.ParseMultiplicative()?;
        loop {
            let op = if self.MatchKw(SdslvTokenKind::Plus) {
                Some(SdslvBinaryOperator::Add)
            } else if self.MatchKw(SdslvTokenKind::Minus) {
                Some(SdslvBinaryOperator::Subtract)
            } else {
                None
            };
            let Some(operator) = op else { break };
            let right = self.ParseMultiplicative()?;
            left = SdslvExpression::Binary {
                Left: Box::new(left),
                Operator: operator,
                Right: Box::new(right),
            };
        }
        Some(left)
    }
    fn ParseMultiplicative(&mut self) -> Option<SdslvExpression> {
        let mut left = self.ParseUnary()?;
        loop {
            let op = if self.MatchKw(SdslvTokenKind::Star) {
                Some(SdslvBinaryOperator::Multiply)
            } else if self.MatchKw(SdslvTokenKind::Slash) {
                Some(SdslvBinaryOperator::Divide)
            } else {
                None
            };
            let Some(operator) = op else { break };
            let right = self.ParseUnary()?;
            left = SdslvExpression::Binary {
                Left: Box::new(left),
                Operator: operator,
                Right: Box::new(right),
            };
        }
        Some(left)
    }
    fn ParseUnary(&mut self) -> Option<SdslvExpression> {
        if self.MatchKw(SdslvTokenKind::Minus) {
            let o = self.ParseUnary()?;
            return Some(SdslvExpression::Unary {
                Operator: SdslvUnaryOperator::Negate,
                Operand: Box::new(o),
            });
        }
        self.ParsePostfix()
    }
    fn ParsePostfix(&mut self) -> Option<SdslvExpression> {
        let mut expr = self.ParsePrimary()?;
        loop {
            if self.MatchKw(SdslvTokenKind::Dot) {
                let field = self.IdentReq("expected identifier after '.'")?;
                expr = SdslvExpression::FieldAccess {
                    Base: Box::new(expr),
                    Field: field,
                };
            } else if self.MatchKw(SdslvTokenKind::LeftParen) {
                let mut args = vec![];
                if !self.Check(SdslvTokenKind::RightParen) {
                    loop {
                        args.push(self.ParseExpression()?);
                        if !self.MatchKw(SdslvTokenKind::Comma) {
                            break;
                        }
                    }
                }
                self.Expect(
                    SdslvTokenKind::RightParen,
                    "expected ')' to close function call",
                );
                expr = SdslvExpression::Call {
                    Callee: Box::new(expr),
                    Arguments: args,
                };
            } else if self.MatchKw(SdslvTokenKind::KeywordWith) {
                self.Expect(SdslvTokenKind::LeftBrace, "expected '{' after with keyword");
                let mut updates = Vec::new();
                loop {
                    let field = self.IdentReq("expected field name in with expression")?;
                    self.Expect(SdslvTokenKind::Colon, "expected ':' after with field name");
                    let value = self.ParseExpression()?;
                    updates.push(SdslvWithUpdate {
                        Field: field,
                        Value: value,
                    });
                    if self.MatchKw(SdslvTokenKind::Comma) {
                        if self.Check(SdslvTokenKind::RightBrace) {
                            break;
                        }
                        continue;
                    }
                    break;
                }
                if updates.is_empty() {
                    self.ErrHere("with expression requires at least one field update");
                    return None;
                }
                self.Expect(
                    SdslvTokenKind::RightBrace,
                    "expected '}' after with updates",
                );
                expr = SdslvExpression::With {
                    Base: Box::new(expr),
                    Updates: updates,
                };
            } else {
                break;
            }
        }
        Some(expr)
    }
    fn ParsePrimary(&mut self) -> Option<SdslvExpression> {
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
                let out = value.clone();
                self.I += 1;
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
    fn IdentReq(&mut self, m: &str) -> Option<String> {
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
    fn Check(&self, k: SdslvTokenKind) -> bool {
        if self.I >= self.Tokens.len() {
            return false;
        }
        std::mem::discriminant(&self.Tokens[self.I].Kind) == std::mem::discriminant(&k)
    }
    fn ErrHere(&mut self, m: &str) {
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
    fn PrevSpan(&self) -> SdslvSpan {
        self.Tokens[self.I - 1].Span
    }
    fn CurrentSpan(&self) -> SdslvSpan {
        self.Tokens
            .get(self.I)
            .map(|x| x.Span)
            .unwrap_or(SdslvSpan {
                Start: self.Source.len(),
                End: self.Source.len(),
                Line: 1,
                Column: 1,
            })
    }
}
