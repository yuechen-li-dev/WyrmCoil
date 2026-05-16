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
            } else if self.match_kw(SdslvTokenKind::KeywordFlow) {
                if let Some(d) = self.parse_flow() {
                    m.Declarations.push(SdslvDecl::Flow(d));
                }
            } else if self.match_kw(SdslvTokenKind::KeywordCompile) {
                if let Some(d) = self.parse_compile() {
                    m.Declarations.push(SdslvDecl::Compile(d));
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
    fn ParseTestModule(mut self) -> Result<SdslvTestModule, Vec<SdslvDiagnostic>> {
        let mut m = SdslvTestModule {
            Namespace: None,
            Uses: vec![],
            Tests: vec![],
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
            } else if self.check(SdslvTokenKind::LeftBracket) {
                let attributes = self.parse_attributes();
                let start = self.current_span();
                if let Some(function) = self.parse_test_fn()
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
                self.err_here("unexpected token at top level in test source");
                self.I += 1;
            }
        }
        if self.Diagnostics.is_empty() {
            Ok(m)
        } else {
            Err(self.Diagnostics)
        }
    }
    fn parse_attributes(&mut self) -> Vec<SdslvAttribute> {
        let mut out = vec![];
        while self.match_kw(SdslvTokenKind::LeftBracket) {
            let start = self.prev_span();
            let Some(name) = self.ident_req("expected attribute name") else {
                break;
            };
            let mut arguments = vec![];
            if self.match_kw(SdslvTokenKind::LeftParen) {
                if !self.check(SdslvTokenKind::RightParen) {
                    loop {
                        let Some(arg) = self.parse_expression() else {
                            break;
                        };
                        arguments.push(arg);
                        if !self.match_kw(SdslvTokenKind::Comma) {
                            break;
                        }
                    }
                }
                self.expect(
                    SdslvTokenKind::RightParen,
                    "expected ')' after attribute arguments",
                );
            }
            self.expect(SdslvTokenKind::RightBracket, "expected ']' after attribute");
            out.push(SdslvAttribute {
                Name: name,
                Arguments: arguments,
                Span: start,
            });
        }
        out
    }
    fn parse_test_fn(&mut self) -> Option<SdslvFunctionDecl> {
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
        let body = if self.match_kw(SdslvTokenKind::LeftBrace) {
            self.parse_body()
        } else {
            self.err_here("expected function body");
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
    fn parse_compile(&mut self) -> Option<SdslvCompileDecl> {
        let generic_shader = self.parse_path_req("expected shader path after compile")?;
        self.expect(
            SdslvTokenKind::LeftAngle,
            "expected '<' in compile declaration",
        );
        let mut type_arguments = vec![];
        while !self.check(SdslvTokenKind::RightAngle) && self.I < self.Tokens.len() {
            let arg = self.parse_path_req("expected type argument path")?;
            type_arguments.push(arg);
            if !self.match_kw(SdslvTokenKind::Comma) {
                break;
            }
        }
        self.expect(
            SdslvTokenKind::RightAngle,
            "expected '>' after type arguments",
        );
        let as_keyword = self.ident()?;
        if as_keyword != "as" {
            self.err_here("expected 'as' in compile declaration");
            return None;
        }
        let alias = self.ident()?;
        self.expect(
            SdslvTokenKind::Semicolon,
            "expected ';' after compile declaration",
        );
        Some(SdslvCompileDecl {
            GenericShader: generic_shader,
            TypeArguments: type_arguments,
            Alias: alias,
        })
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
    fn parse_flow(&mut self) -> Option<SdslvFlowDecl> {
        let start = self.prev_span();
        let name = self.ident()?;
        self.expect(SdslvTokenKind::LeftParen, "expected '(' in flow signature");
        let mut parameters = vec![];
        while !self.check(SdslvTokenKind::RightParen) && self.I < self.Tokens.len() {
            let n = self.ident()?;
            self.expect(SdslvTokenKind::Colon, "expected ':' in parameter");
            let t = self.parse_path_req("expected parameter type")?;
            parameters.push(SdslvFunctionParameter {
                Name: n,
                TypeName: t,
            });
            if !self.match_kw(SdslvTokenKind::Comma) {
                break;
            }
        }
        self.expect(SdslvTokenKind::RightParen, "expected ')' after parameters");
        self.expect(SdslvTokenKind::Arrow, "expected '->' in flow signature");
        let return_type = self.parse_path_req("expected return type in flow declaration")?;
        self.expect(
            SdslvTokenKind::LeftBrace,
            "expected '{' after flow signature",
        );
        let mut board = None;
        let mut states = vec![];
        let mut saw_state = false;
        while !self.check(SdslvTokenKind::RightBrace) && self.I < self.Tokens.len() {
            if self.match_kw(SdslvTokenKind::KeywordBoard) {
                if saw_state {
                    self.err_here("flow board must be declared before states");
                }
                if board.is_some() {
                    self.err_here("flow can declare at most one board block");
                }
                if let Some(parsed) = self.parse_flow_board() {
                    if board.is_none() {
                        board = Some(parsed);
                    }
                }
                continue;
            }
            if !self.match_kw(SdslvTokenKind::KeywordState) {
                self.err_here("expected state declaration in flow body");
                self.I += 1;
                continue;
            }
            saw_state = true;
            if let Some(state) = self.parse_flow_state() {
                states.push(state);
            }
        }
        self.expect(SdslvTokenKind::RightBrace, "expected '}' after flow body");
        let end = self.prev_span();
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

    fn parse_flow_board(&mut self) -> Option<SdslvFlowBoard> {
        let start = self.prev_span();
        self.expect(SdslvTokenKind::LeftBrace, "expected '{' after board");
        let mut fields = vec![];
        while !self.check(SdslvTokenKind::RightBrace) && self.I < self.Tokens.len() {
            let field_start = self.current_span();
            let name = self.ident_req("expected board field name")?;
            self.expect(SdslvTokenKind::Colon, "expected ':' after board field name");
            let type_name = self.parse_path_req("expected board field type")?;
            if self.match_kw(SdslvTokenKind::Equals) {
                self.err_here("unsupported board initializer in SDSL-V M9");
                let _ = self.parse_expression();
            }
            self.expect(SdslvTokenKind::Semicolon, "expected ';' after board field");
            fields.push(SdslvFlowBoardField {
                Name: name,
                TypeName: type_name,
                Span: field_start,
            });
        }
        self.expect(SdslvTokenKind::RightBrace, "expected '}' after board");
        let end = self.prev_span();
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
    fn parse_flow_state(&mut self) -> Option<SdslvFlowState> {
        let start = self.prev_span();
        let name = self.ident()?;
        self.expect(SdslvTokenKind::LeftBrace, "expected '{' after state name");
        let mut statements = vec![];
        while !self.check(SdslvTokenKind::RightBrace) && self.I < self.Tokens.len() {
            if let Some(statement) = self.parse_flow_statement() {
                statements.push(statement);
            } else {
                self.recover_statement();
            }
        }
        self.expect(SdslvTokenKind::RightBrace, "expected '}' after state body");
        let end = self.prev_span();
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
    fn parse_flow_statement(&mut self) -> Option<SdslvFlowStatement> {
        if self.check_board_assignment_shape() {
            let span = self.current_span();
            self.expect(SdslvTokenKind::KeywordBoard, "expected board");
            self.expect(SdslvTokenKind::Dot, "expected '.' after board");
            let field = self.ident_req("expected board field name after board.")?;
            self.expect(SdslvTokenKind::Equals, "expected '=' in board assignment");
            let value = self.parse_expression()?;
            self.expect(
                SdslvTokenKind::Semicolon,
                "expected ';' after board assignment",
            );
            return Some(SdslvFlowStatement::BoardAssign {
                Field: field,
                Value: value,
                Span: span,
            });
        }
        if self.match_kw(SdslvTokenKind::KeywordWhen) {
            return Some(SdslvFlowStatement::When(self.parse_flow_when()?));
        }
        if self.match_kw(SdslvTokenKind::KeywordGoto) {
            let target = self.parse_path_req("expected state path after goto")?;
            self.expect(SdslvTokenKind::Semicolon, "expected ';' after goto");
            return Some(SdslvFlowStatement::Goto(target));
        }
        if self.match_kw(SdslvTokenKind::KeywordReturn) {
            let value = self.parse_expression()?;
            self.expect(SdslvTokenKind::Semicolon, "expected ';' after return");
            return Some(SdslvFlowStatement::Return(value));
        }
        self.err_here("unsupported statement in flow state body");
        None
    }
    fn check_board_assignment_shape(&self) -> bool {
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
    fn parse_flow_when(&mut self) -> Option<SdslvFlowWhen> {
        let start = self.prev_span();
        self.expect(SdslvTokenKind::LeftBrace, "expected '{' after when");
        let mut cases = vec![];
        let mut else_action = None;
        while !self.check(SdslvTokenKind::RightBrace) && self.I < self.Tokens.len() {
            if self.match_kw(SdslvTokenKind::KeywordCase) {
                let condition = self.parse_expression()?;
                self.expect(SdslvTokenKind::Arrow, "expected '->' in when case");
                let action = self.parse_flow_action()?;
                cases.push(SdslvFlowCase {
                    Condition: condition,
                    Action: action,
                });
                continue;
            }
            if self.match_kw(SdslvTokenKind::KeywordElse) {
                self.expect(SdslvTokenKind::Arrow, "expected '->' after else");
                else_action = self.parse_flow_action();
                continue;
            }
            self.err_here("expected case or else in when");
            self.I += 1;
        }
        self.expect(SdslvTokenKind::RightBrace, "expected '}' after when");
        let end = self.prev_span();
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
    fn parse_flow_action(&mut self) -> Option<SdslvFlowAction> {
        if self.match_kw(SdslvTokenKind::KeywordGoto) {
            let target = self.parse_path_req("expected goto target")?;
            return Some(SdslvFlowAction::Goto(target));
        }
        if self.match_kw(SdslvTokenKind::KeywordReturn) {
            let value = self.parse_expression()?;
            return Some(SdslvFlowAction::Return(value));
        }
        self.err_here("expected goto or return flow action");
        None
    }
    fn parse_body(&mut self) -> Option<SdslvBody> {
        let start = self.prev_span();
        let mut statements = vec![];
        while !self.check(SdslvTokenKind::RightBrace) && self.I < self.Tokens.len() {
            if let Some(statement) = self.parse_statement() {
                statements.push(statement);
            } else {
                self.recover_statement();
            }
        }
        if !self.match_kw(SdslvTokenKind::RightBrace) {
            self.err_here("unexpected end of file while parsing block");
            return None;
        }
        let end = self.prev_span();
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
    fn parse_statement(&mut self) -> Option<SdslvStatement> {
        if self.match_kw(SdslvTokenKind::Semicolon) {
            return Some(SdslvStatement::Empty);
        }
        if self.match_kw(SdslvTokenKind::KeywordLet) {
            let name = self.ident_req("expected identifier after let")?;
            self.expect(SdslvTokenKind::Colon, "expected ':' in let declaration");
            let t = self.parse_path_req("expected type in let declaration")?;
            let init = if self.match_kw(SdslvTokenKind::Equals) {
                Some(self.parse_expression()?)
            } else {
                None
            };
            self.expect(
                SdslvTokenKind::Semicolon,
                "expected ';' after let declaration",
            );
            return Some(SdslvStatement::Let {
                Name: name,
                TypeName: t,
                Initializer: init,
            });
        }
        if self.match_kw(SdslvTokenKind::KeywordReturn) {
            if self.check(SdslvTokenKind::Semicolon) {
                self.err_here("expected expression after return");
                return None;
            }
            let value = self.parse_expression()?;
            self.expect(SdslvTokenKind::Semicolon, "expected ';' after return");
            return Some(SdslvStatement::Return { Value: value });
        }
        if self.check(SdslvTokenKind::KeywordStage) || self.check(SdslvTokenKind::KeywordFn) {
            self.err_here("statement form not supported in SDSL-V M4 body subset");
            return None;
        }
        if let Some(SdslvToken {
            Kind: SdslvTokenKind::Identifier(name),
            ..
        }) = self.Tokens.get(self.I)
            && (name == "if" || name == "for" || name == "while" || name == "match")
        {
            self.err_here("unsupported statement in SDSL-V M4 body subset");
            return None;
        }
        let target = self.parse_expression()?;
        if !self.match_kw(SdslvTokenKind::Equals) {
            self.expect(
                SdslvTokenKind::Semicolon,
                "expected ';' after expression statement",
            );
            return Some(SdslvStatement::Expression { Value: target });
        }
        if !self.is_assignment_target(&target) {
            self.err_here("invalid assignment target in SDSL-V M4 body subset");
            return None;
        }
        let value = self.parse_expression()?;
        self.expect(SdslvTokenKind::Semicolon, "expected ';' after assignment");
        Some(SdslvStatement::Assign {
            Target: target,
            Value: value,
        })
    }
    fn is_assignment_target(&self, expr: &SdslvExpression) -> bool {
        matches!(
            expr,
            SdslvExpression::Identifier(_) | SdslvExpression::FieldAccess { .. }
        )
    }
    fn parse_expression(&mut self) -> Option<SdslvExpression> {
        self.parse_comparison()
    }
    fn parse_comparison(&mut self) -> Option<SdslvExpression> {
        let mut left = self.parse_additive()?;
        loop {
            let op = if self.match_kw(SdslvTokenKind::DoubleEquals) {
                Some(SdslvBinaryOperator::Equal)
            } else if self.match_kw(SdslvTokenKind::BangEquals) {
                Some(SdslvBinaryOperator::NotEqual)
            } else if self.match_kw(SdslvTokenKind::LeftAngleEquals) {
                Some(SdslvBinaryOperator::LessEqual)
            } else if self.match_kw(SdslvTokenKind::RightAngleEquals) {
                Some(SdslvBinaryOperator::GreaterEqual)
            } else if self.match_kw(SdslvTokenKind::LeftAngle) {
                Some(SdslvBinaryOperator::Less)
            } else if self.match_kw(SdslvTokenKind::RightAngle) {
                Some(SdslvBinaryOperator::Greater)
            } else {
                None
            };
            let Some(operator) = op else { break };
            let right = self.parse_additive()?;
            left = SdslvExpression::Binary {
                Left: Box::new(left),
                Operator: operator,
                Right: Box::new(right),
            };
        }
        Some(left)
    }
    fn parse_additive(&mut self) -> Option<SdslvExpression> {
        let mut left = self.parse_multiplicative()?;
        loop {
            let op = if self.match_kw(SdslvTokenKind::Plus) {
                Some(SdslvBinaryOperator::Add)
            } else if self.match_kw(SdslvTokenKind::Minus) {
                Some(SdslvBinaryOperator::Subtract)
            } else {
                None
            };
            let Some(operator) = op else { break };
            let right = self.parse_multiplicative()?;
            left = SdslvExpression::Binary {
                Left: Box::new(left),
                Operator: operator,
                Right: Box::new(right),
            };
        }
        Some(left)
    }
    fn parse_multiplicative(&mut self) -> Option<SdslvExpression> {
        let mut left = self.parse_unary()?;
        loop {
            let op = if self.match_kw(SdslvTokenKind::Star) {
                Some(SdslvBinaryOperator::Multiply)
            } else if self.match_kw(SdslvTokenKind::Slash) {
                Some(SdslvBinaryOperator::Divide)
            } else {
                None
            };
            let Some(operator) = op else { break };
            let right = self.parse_unary()?;
            left = SdslvExpression::Binary {
                Left: Box::new(left),
                Operator: operator,
                Right: Box::new(right),
            };
        }
        Some(left)
    }
    fn parse_unary(&mut self) -> Option<SdslvExpression> {
        if self.match_kw(SdslvTokenKind::Minus) {
            let o = self.parse_unary()?;
            return Some(SdslvExpression::Unary {
                Operator: SdslvUnaryOperator::Negate,
                Operand: Box::new(o),
            });
        }
        self.parse_postfix()
    }
    fn parse_postfix(&mut self) -> Option<SdslvExpression> {
        let mut expr = self.parse_primary()?;
        loop {
            if self.match_kw(SdslvTokenKind::Dot) {
                let field = self.ident_req("expected identifier after '.'")?;
                expr = SdslvExpression::FieldAccess {
                    Base: Box::new(expr),
                    Field: field,
                };
            } else if self.match_kw(SdslvTokenKind::LeftParen) {
                let mut args = vec![];
                if !self.check(SdslvTokenKind::RightParen) {
                    loop {
                        args.push(self.parse_expression()?);
                        if !self.match_kw(SdslvTokenKind::Comma) {
                            break;
                        }
                    }
                }
                self.expect(
                    SdslvTokenKind::RightParen,
                    "expected ')' to close function call",
                );
                expr = SdslvExpression::Call {
                    Callee: Box::new(expr),
                    Arguments: args,
                };
            } else {
                break;
            }
        }
        Some(expr)
    }
    fn parse_primary(&mut self) -> Option<SdslvExpression> {
        if self.match_kw(SdslvTokenKind::LeftParen) {
            let expr = self.parse_expression()?;
            self.expect(
                SdslvTokenKind::RightParen,
                "expected ')' to close grouped expression",
            );
            return Some(expr);
        }
        if self.I >= self.Tokens.len() {
            self.err_here("unexpected token in expression");
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
                self.err_here("unexpected token in expression");
                None
            }
        }
    }
    fn recover_statement(&mut self) {
        while self.I < self.Tokens.len()
            && !self.check(SdslvTokenKind::Semicolon)
            && !self.check(SdslvTokenKind::RightBrace)
        {
            self.I += 1;
        }
        if self.check(SdslvTokenKind::Semicolon) {
            self.I += 1;
        }
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
    fn current_span(&self) -> SdslvSpan {
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
