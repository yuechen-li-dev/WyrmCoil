//! Canonical declaration and expression forms ported from Oct's SDSL-V frontend.

use super::*;

impl Parser<'_> {
    pub(super) fn ParseStageName(&mut self) -> Option<String> {
        let stage = match self.Tokens.get(self.I).map(|token| &token.Kind) {
            Some(SdslvTokenKind::KeywordCompute) => "compute",
            Some(SdslvTokenKind::KeywordVertex) => "vertex",
            Some(SdslvTokenKind::KeywordPixel) => "pixel",
            _ => return self.IdentReq("expected stage name"),
        };
        self.I += 1;
        Some(stage.to_string())
    }

    pub(super) fn ParseSpaceGroup(&mut self) -> Option<Vec<SdslvTypeAliasDecl>> {
        let prefix = self.ParsePathReq("expected semantic space group path")?;
        self.Expect(
            SdslvTokenKind::LeftBrace,
            "expected '{' after semantic space group",
        );
        let mut aliases = vec![];
        while !self.Check(SdslvTokenKind::RightBrace) && self.I < self.Tokens.len() {
            let span = self.CurrentSpan();
            let name = self.IdentReq("expected semantic space member")?;
            if !name.as_bytes().first().is_some_and(u8::is_ascii_uppercase)
                || !name.bytes().all(|byte| byte.is_ascii_alphanumeric())
            {
                self.ErrAt(
                    span,
                    "semantic space members require PascalCase ASCII names",
                );
                return None;
            }
            self.Expect(
                SdslvTokenKind::Colon,
                "expected ':' after semantic space member",
            );
            let target = self.ParseTypeRefReq("expected semantic space member type")?;
            self.Expect(
                SdslvTokenKind::Semicolon,
                "expected ';' after semantic space member",
            );
            let mut space = prefix.clone();
            space.Segments.push(CanonicalSpaceMember(&name));
            aliases.push(SdslvTypeAliasDecl {
                Name: name,
                TargetType: target,
                SpaceAnnotation: Some(space),
            });
        }
        self.Expect(
            SdslvTokenKind::RightBrace,
            "expected '}' after semantic space group",
        );
        Some(aliases)
    }

    pub(super) fn ParseForeignHlsl(&mut self) -> Option<SdslvExpression> {
        let start = self.PrevSpan();
        self.Expect(
            SdslvTokenKind::LeftAngle,
            "expected '<' before inline HLSL result type",
        );
        let result_type = self.ParseTypeRefReq("expected inline HLSL result type")?;
        self.ExpectTypeClose("expected '>' after inline HLSL result type")?;
        let mut captures = vec![];
        if self.MatchKw(SdslvTokenKind::LeftParen) {
            if !self.Check(SdslvTokenKind::RightParen) {
                loop {
                    captures.push(self.IdentReq("expected inline HLSL capture name")?);
                    if !self.MatchKw(SdslvTokenKind::Comma) {
                        break;
                    }
                }
            }
            self.Expect(
                SdslvTokenKind::RightParen,
                "expected ')' after inline HLSL captures",
            );
        }
        let Some(SdslvTokenKind::RawForeignSource(source)) =
            self.Tokens.get(self.I).map(|token| &token.Kind)
        else {
            self.ErrHere("expected inline HLSL body");
            return None;
        };
        self.I += 1;
        Some(SdslvExpression::ForeignHlsl {
            ResultType: result_type,
            Captures: captures,
            Source: source.clone(),
            Span: self.SpanSince(start),
        })
    }
    pub(super) fn ParseImport(&mut self) -> Option<SdslvImportDecl> {
        let start = self.PrevSpan();
        let (path, relative) = match self.Tokens.get(self.I).map(|token| &token.Kind) {
            Some(SdslvTokenKind::StringLiteral(path)) => {
                self.I += 1;
                (path.clone(), true)
            }
            _ => (
                self.ParsePathReq("expected import path")?
                    .Segments
                    .join("."),
                false,
            ),
        };
        let alias = if self.MatchIdent("as") {
            Some(self.IdentReq("expected import alias after as")?)
        } else {
            None
        };
        self.Expect(SdslvTokenKind::Semicolon, "expected ';' after import");
        Some(SdslvImportDecl {
            Path: path,
            Relative: relative,
            Alias: alias,
            Span: self.SpanSince(start),
        })
    }

    pub(super) fn ParseConcept(&mut self) -> Option<SdslvConceptDecl> {
        let start = self.PrevSpan();
        let name = self.IdentReq("expected concept name")?;
        self.Expect(SdslvTokenKind::LeftBrace, "expected '{' after concept name");
        let mut members = vec![];
        let mut requirements = vec![];
        while !self.Check(SdslvTokenKind::RightBrace) && self.I < self.Tokens.len() {
            if self.MatchKw(SdslvTokenKind::KeywordRequire) {
                requirements.push(self.ParseRequirement()?);
            } else {
                members.push(self.ParseConceptMember()?);
            }
        }
        self.Expect(SdslvTokenKind::RightBrace, "expected '}' after concept");
        Some(SdslvConceptDecl {
            Name: name,
            Members: members,
            Requirements: requirements,
            Span: self.SpanSince(start),
        })
    }

    fn ParseConceptMember(&mut self) -> Option<SdslvConceptMember> {
        let start = self.CurrentSpan();
        let name = self.IdentReq("expected concept member")?;
        self.Expect(SdslvTokenKind::Colon, "expected ':' after concept member");
        if self.MatchKw(SdslvTokenKind::LeftBrace) {
            let mut members = vec![];
            while !self.Check(SdslvTokenKind::RightBrace) && self.I < self.Tokens.len() {
                members.push(self.ParseConceptMember()?);
            }
            self.Expect(
                SdslvTokenKind::RightBrace,
                "expected '}' after concept group",
            );
            self.Expect(
                SdslvTokenKind::Semicolon,
                "expected ';' after concept group",
            );
            return Some(SdslvConceptMember::Group {
                Name: name,
                Members: members,
            });
        }
        let type_start = self.CurrentSpan();
        let ty = self.ParseCanonicalType("expected concept field type")?;
        let zero_allowed = self.MatchKw(SdslvTokenKind::Bang);
        if zero_allowed && ty.ToDisplayString() != "u32" {
            self.ErrAt(self.PrevSpan(), "only u32! permits zero in a concept field");
            return None;
        }
        let type_span = self.SpanSince(type_start);
        let default = if self.MatchKw(SdslvTokenKind::Equals) {
            Some(self.ParseExpression()?)
        } else {
            None
        };
        self.Expect(
            SdslvTokenKind::Semicolon,
            "expected ';' after concept field",
        );
        let field = SdslvFieldDecl {
            Name: name,
            TypeName: ty,
            Attributes: vec![],
            Access: None,
            Span: self.SpanSince(start),
            TypeSpan: type_span,
        };
        Some(SdslvConceptMember::Field {
            Field: field,
            Default: default,
            ZeroAllowed: zero_allowed,
        })
    }

    pub(super) fn ParseConfig(&mut self) -> Option<SdslvConfigDecl> {
        let start = self.PrevSpan();
        let name = self.IdentReq("expected config name")?;
        self.Expect(SdslvTokenKind::Colon, "expected ':' after config name");
        let concept = self.ParsePathReq("expected config concept")?;
        self.Expect(
            SdslvTokenKind::LeftBrace,
            "expected '{' after config concept",
        );
        let mut fields = vec![];
        let mut requirements = vec![];
        let mut uses_arrow = None;
        while !self.Check(SdslvTokenKind::RightBrace) && self.I < self.Tokens.len() {
            if self.MatchKw(SdslvTokenKind::KeywordRequire) {
                requirements.push(self.ParseRequirement()?);
                continue;
            }
            let field_start = self.CurrentSpan();
            let path = self.ParsePathReq("expected config field name")?;
            let arrow = self.MatchKw(SdslvTokenKind::FatArrow);
            if !arrow {
                self.Expect(
                    SdslvTokenKind::Colon,
                    "expected ':' or '=>' after config field",
                );
                if path.Segments.len() != 1 {
                    self.ErrAt(
                        field_start,
                        "dotted config field paths require '=>' assignments",
                    );
                    return None;
                }
            }
            if uses_arrow.is_some_and(|previous| previous != arrow) {
                self.ErrAt(field_start, "config must use a consistent assignment style");
                return None;
            }
            uses_arrow = Some(arrow);
            let value = self.ParseExpression()?;
            self.Expect(SdslvTokenKind::Semicolon, "expected ';' after config field");
            fields.push(SdslvConfigField {
                Path: path,
                Value: value,
                Span: self.SpanSince(field_start),
            });
        }
        self.Expect(SdslvTokenKind::RightBrace, "expected '}' after config");
        Some(SdslvConfigDecl {
            Name: name,
            Concept: concept,
            Fields: fields,
            Requirements: requirements,
            Span: self.SpanSince(start),
        })
    }

    fn ParseRequirement(&mut self) -> Option<SdslvExpression> {
        let condition = self.ParseExpression()?;
        self.Expect(SdslvTokenKind::Semicolon, "expected ';' after require");
        Some(condition)
    }

    pub(super) fn ParseAssert(&mut self) -> Option<SdslvExpression> {
        self.Expect(
            SdslvTokenKind::KeywordAssert,
            "expected assert after static",
        );
        let condition = self.ParseExpression()?;
        self.Expect(
            SdslvTokenKind::Semicolon,
            "expected ';' after static assert",
        );
        Some(condition)
    }

    pub(super) fn ParseTemplate(&mut self) -> Option<SdslvShaderDecl> {
        let parameter = if self.Check(SdslvTokenKind::LeftAngle) {
            Some(self.ParseTemplateParameter()?)
        } else {
            None
        };
        self.Expect(
            SdslvTokenKind::KeywordShader,
            "expected shader after template",
        );
        self.ParseShader(true, parameter)
    }

    pub(super) fn ParseTemplateParameter(&mut self) -> Option<SdslvTemplateParam> {
        let start = self.CurrentSpan();
        if !self.MatchKw(SdslvTokenKind::LeftAngle) {
            self.ErrHere("SDSL-V4303: template requires one concept parameter");
            return None;
        }
        let name = self.IdentReq("expected template parameter name")?;
        self.Expect(
            SdslvTokenKind::Colon,
            "expected ':' after template parameter",
        );
        let concept = self.ParsePathReq("expected template parameter concept")?;
        self.ExpectTypeClose("expected '>' after one template parameter")?;
        Some(SdslvTemplateParam {
            Name: name,
            Concept: concept,
            Span: self.SpanSince(start),
        })
    }

    pub(super) fn ParseResourceAccess(&mut self) -> Option<SdslvResourceAccess> {
        if !self.Canonical {
            return None;
        }
        if self.MatchKw(SdslvTokenKind::KeywordReadonly) {
            Some(SdslvResourceAccess::Readonly)
        } else if self.MatchKw(SdslvTokenKind::KeywordReadwrite) {
            Some(SdslvResourceAccess::Readwrite)
        } else {
            None
        }
    }

    pub(super) fn ParseResources(&mut self) -> Option<SdslvResourceDecl> {
        if self.MatchKw(SdslvTokenKind::LeftBrace) {
            let fields = self.ParseAggregateFields("resource name")?;
            self.Expect(SdslvTokenKind::RightBrace, "expected '}' after resources");
            if let Some(field) = fields.iter().find(|field| field.Access.is_none()) {
                self.ErrAt(field.Span, "expected readonly or readwrite resource access");
                return None;
            }
            return Some(SdslvResourceDecl::Inline(fields));
        }
        let name = self.ParsePathReq("expected resource stream name")?;
        self.Expect(
            SdslvTokenKind::Semicolon,
            "expected ';' after resource stream",
        );
        Some(SdslvResourceDecl::Stream(name))
    }

    pub(super) fn ParseWorkgroup(&mut self) -> Option<SdslvFieldDecl> {
        let start = self.PrevSpan();
        let name = self.IdentReq("expected workgroup name")?;
        self.Expect(SdslvTokenKind::Colon, "expected ':' after workgroup name");
        let type_start = self.CurrentSpan();
        let ty = self.ParseTypeRefReq("expected workgroup type")?;
        let type_span = self.SpanSince(type_start);
        self.Expect(SdslvTokenKind::Semicolon, "expected ';' after workgroup");
        Some(SdslvFieldDecl {
            Name: name,
            TypeName: ty,
            Attributes: vec![],
            Access: None,
            Span: self.SpanSince(start),
            TypeSpan: type_span,
        })
    }

    pub(super) fn ParseCanonicalType(&mut self, message: &str) -> Option<SdslvTypeRef> {
        let start = self.CurrentSpan();
        let path = if self.MatchKw(SdslvTokenKind::KeywordArray) {
            SdslvPath {
                Segments: vec!["array".to_string()],
            }
        } else {
            self.ParsePathReq(message)?
        };
        let name = path.Segments[0].as_str();
        if path.Segments.len() != 1
            || !matches!(
                name,
                "array" | "ndarray" | "matrix_view" | "tile" | "reg_tile" | "texture2d" | "uniform"
            )
            || !self.MatchKw(SdslvTokenKind::LeftAngle)
        {
            return Some(SdslvTypeRef::Named(path));
        }
        let element = self.ParseCanonicalType("expected element type")?;
        let mut extents = vec![];
        if self.MatchKw(SdslvTokenKind::Comma) {
            if name == "ndarray" {
                self.Expect(
                    SdslvTokenKind::LeftBracket,
                    "expected '[' before ndarray shape",
                );
                while !self.Check(SdslvTokenKind::RightBracket) && self.I < self.Tokens.len() {
                    extents.push(self.ParseExpression()?);
                    if !self.MatchKw(SdslvTokenKind::Comma) {
                        break;
                    }
                }
                self.Expect(
                    SdslvTokenKind::RightBracket,
                    "expected ']' after ndarray shape",
                );
            } else {
                extents.push(self.ParseWith(true)?);
                if matches!(name, "tile" | "reg_tile") {
                    self.Expect(
                        SdslvTokenKind::Comma,
                        "expected ',' before the tile column count",
                    );
                    extents.push(self.ParseWith(true)?);
                }
            }
        } else if matches!(name, "tile" | "reg_tile") {
            self.ErrHere("tile types require row and column counts");
            return None;
        }
        let close = self.ExpectTypeClose("expected '>' after type arguments")?;
        let span = SdslvSpan {
            End: close.End,
            ..start
        };
        if name == "array"
            && let [SdslvExpression::IntegerLiteral(value)] = extents.as_slice()
            && let Ok(length) = value.trim_end_matches(['u', 'U']).parse::<usize>()
            && length > 0
        {
            return Some(SdslvTypeRef::Array {
                Element: Box::new(element),
                Length: length,
                Span: span,
            });
        }
        Some(SdslvTypeRef::Applied {
            Name: path,
            Arguments: vec![element],
            Extents: extents,
            Span: span,
        })
    }

    pub(super) fn ExpectTypeClose(&mut self, message: &str) -> Option<SdslvSpan> {
        if self.MatchKw(SdslvTokenKind::RightAngle) {
            return Some(self.PrevSpan());
        }
        if self.Check(SdslvTokenKind::ShiftRight) {
            let token = &mut self.Tokens[self.I];
            let first = SdslvSpan {
                End: token.Span.Start + 1,
                ..token.Span
            };
            token.Kind = SdslvTokenKind::RightAngle;
            token.Span.Start += 1;
            token.Span.Column += 1;
            return Some(first);
        }
        self.ErrHere(message);
        None
    }

    pub(super) fn ParseInlineFlow(&mut self, start: SdslvSpan) -> Option<SdslvStatement> {
        let name = self.IdentReq("expected flow name")?;
        self.Expect(SdslvTokenKind::LeftBrace, "expected '{' after flow name");
        let mut states = vec![];
        let mut boards = vec![];
        while !self.Check(SdslvTokenKind::RightBrace) && self.I < self.Tokens.len() {
            if self.MatchKw(SdslvTokenKind::KeywordBoard) {
                let start = self.PrevSpan();
                if !states.is_empty() {
                    self.ErrAt(start, "flow board declarations must precede states");
                    return None;
                }
                let name = self.IdentReq("expected flow board name")?;
                self.Expect(SdslvTokenKind::Colon, "expected ':' after flow board name");
                let ty = self.ParseTypeRefReq("expected flow board type")?;
                self.Expect(
                    SdslvTokenKind::Equals,
                    "expected '=' before flow board initializer",
                );
                let initializer = self.ParseExpression()?;
                self.Expect(SdslvTokenKind::Semicolon, "expected ';' after flow board");
                boards.push(SdslvFlowBoardBinding {
                    Name: name,
                    TypeName: ty,
                    Initializer: initializer,
                    Span: self.SpanSince(start),
                });
                continue;
            }
            if !self.MatchKw(SdslvTokenKind::KeywordState) {
                self.ErrHere("expected flow state");
                return None;
            }
            let state = self.IdentReq("expected state name")?;
            if !self.MatchKw(SdslvTokenKind::LeftBrace) {
                self.ErrHere("expected '{' after state name");
                return None;
            }
            self.FlowDepth += 1;
            let body = self.ParseBody();
            self.FlowDepth -= 1;
            states.push(SdslvInlineState {
                Name: state,
                Body: body?,
            });
        }
        self.Expect(SdslvTokenKind::RightBrace, "expected '}' after flow");
        Some(SdslvStatement::InlineFlow {
            Name: name,
            Boards: boards,
            States: states,
            Span: self.SpanSince(start),
        })
    }

    pub(super) fn ParseFlowTransition(&mut self) -> Option<SdslvStatement> {
        let start = self.CurrentSpan();
        if self.FlowDepth == 0 {
            self.ErrAt(start, "flow transitions are only valid in a flow state");
            return None;
        }
        let transition = match self.Tokens[self.I].Kind {
            SdslvTokenKind::KeywordGoto => {
                self.I += 1;
                SdslvFlowTransition::Goto(self.IdentReq("expected goto target")?)
            }
            SdslvTokenKind::KeywordPush => {
                self.I += 1;
                SdslvFlowTransition::Push(self.IdentReq("expected push target")?)
            }
            SdslvTokenKind::KeywordPop => {
                self.I += 1;
                SdslvFlowTransition::Pop
            }
            SdslvTokenKind::KeywordFinish => {
                self.I += 1;
                SdslvFlowTransition::Finish
            }
            _ => return None,
        };
        self.Expect(
            SdslvTokenKind::Semicolon,
            "expected ';' after flow transition",
        );
        Some(SdslvStatement::FlowTransition {
            Transition: transition,
            Span: self.SpanSince(start),
        })
    }

    pub(super) fn ParseGuardedRead(&mut self) -> Option<SdslvExpression> {
        let start = self.PrevSpan();
        let target = Box::new(self.ParsePostfix()?);
        self.Expect(
            SdslvTokenKind::KeywordWhen,
            "expected when after guarded read target",
        );
        let condition = Box::new(self.ParseExpression()?);
        self.Expect(
            SdslvTokenKind::KeywordElse,
            "expected else after guarded read condition",
        );
        let fallback = Box::new(self.ParseExpression()?);
        Some(SdslvExpression::GuardedRead {
            Target: target,
            Condition: condition,
            Fallback: fallback,
            Span: self.SpanSince(start),
        })
    }

    pub(super) fn ParseGuardedWrite(&mut self, start: SdslvSpan) -> Option<SdslvStatement> {
        let target = self.ParseExpression()?;
        self.Expect(
            SdslvTokenKind::Equals,
            "expected '=' after guarded write target",
        );
        let value = self.ParseExpression()?;
        self.Expect(
            SdslvTokenKind::KeywordWhen,
            "expected when after guarded write value",
        );
        let condition = self.ParseExpression()?;
        self.Expect(
            SdslvTokenKind::Semicolon,
            "expected ';' after guarded write",
        );
        Some(SdslvStatement::GuardedWrite {
            Target: target,
            Value: value,
            Condition: condition,
            Span: self.SpanSince(start),
        })
    }

    pub(super) fn ParseGuardWhen(&mut self, start: SdslvSpan) -> Option<SdslvStatement> {
        self.Expect(SdslvTokenKind::LeftBrace, "expected '{' after guard when");
        let mut cases = vec![];
        let mut else_body = None;
        while !self.Check(SdslvTokenKind::RightBrace) && self.I < self.Tokens.len() {
            if else_body.is_some() {
                self.ErrHere("guard when else must be the final arm");
                return None;
            }
            if self.MatchKw(SdslvTokenKind::KeywordCase) {
                let condition = self.ParseExpression()?;
                let body = self.ParseGuardArm()?;
                cases.push(SdslvGuardCase {
                    Condition: condition,
                    Body: body,
                });
            } else if self.MatchKw(SdslvTokenKind::KeywordElse) {
                else_body = Some(self.ParseGuardArm()?);
            } else {
                self.ErrHere("expected case or else in guard when");
                return None;
            }
        }
        self.Expect(SdslvTokenKind::RightBrace, "expected '}' after guard when");
        if cases.is_empty() {
            self.ErrAt(start, "guard when requires at least one case");
            return None;
        }
        Some(SdslvStatement::GuardWhen {
            Cases: cases,
            ElseBody: else_body,
            Span: self.SpanSince(start),
        })
    }

    fn ParseGuardArm(&mut self) -> Option<SdslvBody> {
        if !self.MatchSwitchArmArrow("expected '->' after guard when arm") {
            return None;
        }
        if self.MatchKw(SdslvTokenKind::LeftBrace) {
            return self.ParseBody();
        }
        let start = self.CurrentSpan();
        let statement = self.ParseStatement()?;
        Some(SdslvBody {
            Statements: vec![statement],
            Span: self.SpanSince(start),
        })
    }

    pub(super) fn ParseFieldInitializers(
        &mut self,
        allow_semicolon: bool,
    ) -> Option<Vec<SdslvWithUpdate>> {
        let mut fields = vec![];
        while !self.Check(SdslvTokenKind::RightBrace) && self.I < self.Tokens.len() {
            let field = self.IdentReq("expected initializer field")?;
            self.Expect(SdslvTokenKind::Colon, "expected ':' after field name");
            fields.push(SdslvWithUpdate {
                Field: field,
                Value: self.ParseExpression()?,
            });
            if !(self.MatchKw(SdslvTokenKind::Comma)
                || (allow_semicolon && self.MatchKw(SdslvTokenKind::Semicolon)))
            {
                break;
            }
        }
        self.Expect(SdslvTokenKind::RightBrace, "expected '}' after fields");
        Some(fields)
    }

    pub(super) fn LooksLikeGenericCall(&self) -> bool {
        if !self.Check(SdslvTokenKind::LeftAngle) {
            return false;
        }
        let mut depth = 0;
        for (index, token) in self.Tokens.iter().enumerate().skip(self.I) {
            match token.Kind {
                SdslvTokenKind::LeftAngle => depth += 1,
                SdslvTokenKind::RightAngle => depth -= 1,
                SdslvTokenKind::ShiftRight => depth -= 2,
                SdslvTokenKind::Semicolon
                | SdslvTokenKind::LeftBrace
                | SdslvTokenKind::RightParen => return false,
                _ => {}
            }
            if depth <= 0 {
                return depth == 0
                    && self
                        .Tokens
                        .get(index + 1)
                        .is_some_and(|token| token.Kind == SdslvTokenKind::LeftParen);
            }
        }
        false
    }

    pub(super) fn QualifiedExpressionPath(expr: &SdslvExpression) -> Option<SdslvPath> {
        match expr {
            SdslvExpression::Identifier(name) => Some(SdslvPath {
                Segments: vec![name.clone()],
            }),
            SdslvExpression::FieldAccess { Base, Field } => {
                let mut path = Self::QualifiedExpressionPath(Base)?;
                path.Segments.push(Field.clone());
                Some(path)
            }
            _ => None,
        }
    }
}

fn CanonicalSpaceMember(name: &str) -> String {
    let bytes = name.as_bytes();
    let mut identity = String::new();
    for (index, &byte) in bytes.iter().enumerate() {
        if byte.is_ascii_uppercase() && index > 0 {
            let previous = bytes[index - 1];
            if previous.is_ascii_lowercase()
                || previous.is_ascii_digit()
                || bytes.get(index + 1).is_some_and(u8::is_ascii_lowercase)
            {
                identity.push('_');
            }
        }
        identity.push(byte.to_ascii_lowercase() as char);
    }
    identity
}
