#![allow(non_snake_case)]

use super::*;

#[test]
fn LexerKeywordsAndPath() {
    let t = LexSource("namespace Wyrm.Coil; use A.B;").unwrap();
    assert!(matches!(t[0].Kind, SdslvTokenKind::KeywordNamespace));
    assert!(matches!(t[2].Kind, SdslvTokenKind::Dot));
}

#[test]
fn LexerArrowAtAndComments() {
    let src = "// c\ntype A = float4 @space(world.position); fn F()->float4;";
    let t = LexSource(src).unwrap();
    assert!(t.iter().any(|x| matches!(x.Kind, SdslvTokenKind::Arrow)));
    assert!(t.iter().any(|x| matches!(x.Kind, SdslvTokenKind::At)));
}

#[test]
fn LexerInvalidCharacterDiagnostic() {
    let d = LexSource("$").unwrap_err();
    assert!(d[0].Message.contains("invalid character"));
    assert_eq!(d[0].Span.Line, 1);
}

#[test]
fn ParserValidModule() {
    let src = include_str!("../../../../examples/sdslv/flat_color.sdslv");
    let m = ParseSource(src).unwrap();
    assert_eq!(m.Namespace.unwrap().Segments[0], "WyrmCoil");
    assert_eq!(m.Uses.len(), 1);
    assert!(m.Declarations.len() >= 4);
}

#[test]
fn ParserStreamAndInterfaceAndShaderShapes() {
    let src = include_str!("../../../../examples/sdslv/flat_color.sdslv");
    let m = ParseSource(src).unwrap();
    let stream = m
        .Declarations
        .iter()
        .find_map(|d| {
            if let SdslvDecl::Stream(s) = d {
                Some(s)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(stream.Name, "VertexOut");
    assert_eq!(stream.Fields.len(), 2);
    let shader = m
        .Declarations
        .iter()
        .find_map(|d| {
            if let SdslvDecl::Shader(s) = d {
                Some(s)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(shader.Implements[0].Segments[0], "IBaseColor");
    assert_eq!(shader.MaterialFields.len(), 1);
    assert!(shader.Methods[0].Body.is_some());
    assert_eq!(shader.StageMethods[0].Stage.as_deref(), Some("vertex"));
}

#[test]
fn ParserRecordDeclarationShape() {
    let src = r#"
        record SurfaceData {
            WorldPos: float3;
            Normal: float3;
            BaseColor: float4;
            Roughness: f32;
        }
        stream VertexOut {
            Position: float4;
        }
    "#;
    let m = ParseSource(src).unwrap();
    let record = m
        .Declarations
        .iter()
        .find_map(|d| {
            if let SdslvDecl::Record(r) = d {
                Some(r)
            } else {
                None
            }
        })
        .expect("record declaration should parse");
    assert_eq!(record.Name, "SurfaceData", "record name should parse");
    assert_eq!(record.Fields.len(), 4, "record fields should parse");
    assert_eq!(
        record.Fields[0].Name, "WorldPos",
        "field names should parse"
    );
    assert_eq!(
        record.Fields[3].TypeName.ToDisplayString(),
        "f32",
        "field types should parse"
    );
}

#[test]
fn ParserGenericWhereConstraints() {
    let src = include_str!("../../../../examples/sdslv/generic_forward_pass.sdslv");
    let m = ParseSource(src).unwrap();
    let shader = m
        .Declarations
        .iter()
        .find_map(|d| {
            if let SdslvDecl::Shader(s) = d
                && s.Name == "ForwardPass"
            {
                Some(s)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(shader.GenericParameters[0], "TMat");
    assert_eq!(shader.Constraints[0].Bounds[0].Segments[0], "IBaseColor");
    let compile = m
        .Declarations
        .iter()
        .find_map(|d| {
            if let SdslvDecl::Compile(c) = d {
                Some(c)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(compile.Alias, "ForwardFlatMaterial");
}

#[test]
fn LexerIfSwitchAndFatArrowTokens() {
    let t = LexSource("if cond { } switch { case true => 1 else => 2 }").unwrap();
    assert!(
        t.iter()
            .any(|x| matches!(x.Kind, SdslvTokenKind::KeywordIf))
    );
    assert!(
        t.iter()
            .any(|x| matches!(x.Kind, SdslvTokenKind::KeywordSwitch))
    );
    assert!(t.iter().any(|x| matches!(x.Kind, SdslvTokenKind::FatArrow)));
}

#[test]
fn ParserIfStatementParsesWithAndWithoutElse() {
    let src = r#"
shader S {
    fn A(v: i32) -> i32 {
        if v < 10 { return 1; }
        if v < 20 { return 2; } else { return 3; }
        return 0;
    }
}
"#;
    let module = ParseSource(src).expect("if statements should parse");
    let shader = module
        .Declarations
        .iter()
        .find_map(|d| match d {
            SdslvDecl::Shader(s) => Some(s),
            _ => None,
        })
        .expect("shader expected");
    let body = shader.Methods[0].Body.as_ref().expect("body expected");
    assert!(
        matches!(
            &body.Statements[0],
            SdslvStatement::If { ElseBody: None, .. }
        ),
        "first if should have no else"
    );
    assert!(
        matches!(
            &body.Statements[1],
            SdslvStatement::If {
                ElseBody: Some(_),
                ..
            }
        ),
        "second if should have else body"
    );
}

#[test]
fn ParserSwitchExpressionParsesBothArrows() {
    let src = r#"
shader S {
    fn A(age: i32, weight: i32) -> i32 {
        let a: i32 = switch { case age < 13 => 0 case age < 18 => 1 else => 2 };
        let b: i32 = switch { case weight < 1 -> 1 else -> 3 };
        return a + b;
    }
}
"#;
    let module = ParseSource(src).expect("switch expressions should parse");
    let shader = module
        .Declarations
        .iter()
        .find_map(|d| match d {
            SdslvDecl::Shader(s) => Some(s),
            _ => None,
        })
        .expect("shader expected");
    let body = shader.Methods[0].Body.as_ref().expect("body expected");
    let SdslvStatement::Let {
        Initializer: Some(SdslvExpression::Switch {
            Cases, ElseValue, ..
        }),
        ..
    } = &body.Statements[0]
    else {
        panic!("first let should be switch expression");
    };
    assert_eq!(Cases.len(), 2, "switch case count should match source");
    assert!(
        matches!(**ElseValue, SdslvExpression::IntegerLiteral(_)),
        "switch else value should be present"
    );
}

#[test]
fn ParserSubjectSwitchParsesAndSetsSubject() {
    let src = r#"
shader S {
    fn A(code: i32) -> i32 {
        let a: i32 = switch code { case 408 => 3 case 429 => 5 else => 0 };
        let b: i32 = switch code { case 1 -> 9 else -> 2 };
        let c: i32 = switch { case code > 0 => 1 else => 0 };
        return a + b + c;
    }
}
"#;
    let module = ParseSource(src).expect("subject-switch expressions should parse");
    let shader = module
        .Declarations
        .iter()
        .find_map(|d| match d {
            SdslvDecl::Shader(s) => Some(s),
            _ => None,
        })
        .expect("shader expected");
    let body = shader.Methods[0].Body.as_ref().expect("body expected");
    let SdslvStatement::Let {
        Initializer: Some(SdslvExpression::Switch { Subject, Cases, .. }),
        ..
    } = &body.Statements[0]
    else {
        panic!("first let should be subject switch");
    };
    assert!(
        Subject.is_some(),
        "subject-switch must capture subject expression"
    );
    assert_eq!(Cases.len(), 2, "subject-switch should preserve case count");
    let SdslvStatement::Let {
        Initializer: Some(SdslvExpression::Switch { Subject, .. }),
        ..
    } = &body.Statements[2]
    else {
        panic!("third let should be condition-switch");
    };
    assert!(
        Subject.is_none(),
        "condition-switch should keep Subject=None"
    );
}

#[test]
fn ParserInvalidCases() {
    assert!(ParseSource("namespace ;").is_err());
    assert!(ParseSource("stream A { X float4; }").is_err());
    assert!(ParseSource("interface I { fn A(x: T) float4; }").is_err());
    assert!(ParseSource("shader S { fn A() -> X { ").is_err());
    assert!(ParseSource("bogus").is_err());
    assert!(ParseSource("shader S { fn A() -> i32 { if { return 1; } } }").is_err());
    assert!(
        ParseSource(
            "shader S { fn A(x: i32) -> i32 { let y: i32 = switch { else => 1 }; return y; } }"
        )
        .is_err()
    );
}

#[test]
fn LexerForRangeAndDotTokens() {
    let tokens = LexSource("for i in 0..4 { let x: i32; } input.Color;").unwrap();
    assert!(
        tokens
            .iter()
            .any(|x| matches!(x.Kind, SdslvTokenKind::KeywordFor)),
        "for should tokenize as keyword"
    );
    assert!(
        tokens
            .iter()
            .any(|x| matches!(x.Kind, SdslvTokenKind::KeywordIn)),
        "in should tokenize as keyword"
    );
    assert!(
        tokens
            .iter()
            .any(|x| matches!(x.Kind, SdslvTokenKind::Range)),
        ".. should tokenize as range"
    );
    assert!(
        tokens.iter().any(|x| matches!(x.Kind, SdslvTokenKind::Dot)),
        ". should still tokenize as dot for member access"
    );
}

#[test]
fn ParserForStatementParsesWithAndWithoutStep() {
    let src = "shader S { fn Sum(limit: i32) -> i32 { let sum: i32 = 0; for i in 0..limit { sum = sum + i; } for j in 0..limit step 2 { sum = sum + j; } return sum; } }";
    let module = ParseSource(src).expect("for loops should parse");
    let shader = module
        .Declarations
        .iter()
        .find_map(|d| match d {
            SdslvDecl::Shader(s) => Some(s),
            _ => None,
        })
        .expect("shader expected");
    let body = shader.Methods[0].Body.as_ref().expect("body expected");
    assert!(matches!(
        &body.Statements[1],
        SdslvStatement::For { Step: None, .. }
    ));
    assert!(matches!(
        &body.Statements[2],
        SdslvStatement::For { Step: Some(_), .. }
    ));
}

#[test]
fn ParserForMalformedFormsReportErrors() {
    assert!(
        ParseSource("shader S { fn F() -> i32 { for i 0..4 { return 0; } return 0; } }").is_err(),
        "missing in should fail"
    );
    assert!(
        ParseSource("shader S { fn F() -> i32 { for i in 0 4 { return 0; } return 0; } }").is_err(),
        "missing range operator should fail"
    );
    assert!(
        ParseSource("shader S { fn F() -> i32 { for i in 0..4 return 0; } }").is_err(),
        "missing loop body braces should fail"
    );
}

#[test]
fn ParserFallibleSignatureAndPostfixOperators() {
    let src = r#"shader S { fn F() -> i32 ! Error { return 1; } fn G() -> i32 { let a: i32 = F()?; let b: i32 = F()!; return a + b; } }"#;
    let module = ParseSource(src).expect("fallible signature and postfix operators should parse");
    let shader = module
        .Declarations
        .iter()
        .find_map(|d| match d {
            SdslvDecl::Shader(s) => Some(s),
            _ => None,
        })
        .expect("shader expected");
    assert!(
        shader.Methods[0].ErrorType.is_some(),
        "fallible signature should include error type"
    );
}

#[test]
fn ParserWhileReportsExplicitUnsupportedDiagnostic() {
    let diagnostics =
        ParseSource("shader S { fn F(a: i32) -> i32 { while a < 4 { return a; } return a; } }")
            .expect_err("while should be explicitly unsupported");
    assert!(
        diagnostics.iter().any(|d| d.Message.contains(
            "while loops are not supported in SDSL-V yet; use bounded for loops instead"
        )),
        "while should report explicit unsupported diagnostic"
    );
}

#[test]
fn ParserRejectsReservedKeywordsAsIdentifiers() {
    for (source, keyword) in [
        (
            "shader S { fn F() -> i32 { let step: i32 = 1; return step; } }",
            "step",
        ),
        (
            "shader S { fn F() -> i32 { let flow: i32 = 1; return flow; } }",
            "flow",
        ),
        (
            "shader S { fn F() -> i32 { let state: i32 = 1; return state; } }",
            "state",
        ),
        (
            "shader S { fn F() -> i32 { let sum: i32 = 0; for step in 0..4 { sum = sum + 1; } return sum; } }",
            "step",
        ),
    ] {
        let diagnostics =
            ParseSource(source).expect_err("reserved keyword identifier should be rejected");
        assert!(
            diagnostics.iter().any(|d| d.Message.contains(&format!(
                "'{}' is a reserved keyword in SDSL-V and cannot be used as an identifier",
                keyword
            ))),
            "expected reserved-keyword diagnostic for '{}'",
            keyword
        );
    }
}

#[test]
fn ParserStillAcceptsNormalIdentifiersAndFlowStateSyntax() {
    ParseSource("shader S { fn F(limit: i32) -> i32 { let count: i32 = 1; for i in 0..limit step 2 { count = count + i; } return count; } }")
        .expect("normal identifiers and for-step syntax should parse");
    ParseSource("flow Router(x: i32) -> i32 { state Start { when { case x > 0 -> return x else -> return 0 } } }")
        .expect("flow state syntax should still parse");
}

#[test]
fn ValidationValidFixture() {
    let src = include_str!("../../../../examples/sdslv/flat_color.sdslv");
    assert!(ValidateSource(src).is_ok());
}

#[test]
fn ValidationDuplicateTopLevel() {
    let src = "stream Surface { A: float4; } shader Surface { stage pixel fn PS() -> float4 { return X; } }";
    let d = ValidateSource(src).unwrap_err();
    assert!(d.iter().any(|x| {
        x.Message
            .contains("duplicate top-level declaration 'Surface'")
    }));
}

#[test]
fn ValidationRejectsDuplicateRecordFieldAndTopLevelCollision() {
    let duplicate_field = "record SurfaceData { Color: float4; Color: float4; }";
    let duplicate_field_diags = ValidateSource(duplicate_field).unwrap_err();
    assert!(
        duplicate_field_diags.iter().any(|x| x
            .Message
            .contains("duplicate record field 'Color' in record 'SurfaceData'")),
        "duplicate record fields should be rejected"
    );

    let collision = "record SurfaceData { Color: float4; } stream SurfaceData { Color: float4; }";
    let collision_diags = ValidateSource(collision).unwrap_err();
    assert!(
        collision_diags.iter().any(|x| x
            .Message
            .contains("duplicate top-level declaration 'SurfaceData'")),
        "record names should participate in top-level uniqueness"
    );
}

#[test]
fn ValidationDuplicateMembersAndStageErrors() {
    let src = r#"
interface IBase { fn F(a: float4) -> float4; }
shader S implements IBase {
    material { Color: float4; Color: float4; }
    fn F(a: float4) -> float4 { return a; }
    fn F(a: float4) -> float4 { return a; }
    stage geometry fn PS() -> float4;
}
"#;
    let d = ValidateSource(src).unwrap_err();
    assert!(
        d.iter()
            .any(|x| x.Message.contains("duplicate material field 'Color'"))
    );
    assert!(
        d.iter()
            .any(|x| x.Message.contains("duplicate shader method 'F'"))
    );
    assert!(
        d.iter()
            .any(|x| x.Message.contains("stage 'geometry' is not supported"))
    );
    assert!(d.iter().any(|x| x.Message.contains("must have a body")));
}

#[test]
fn ValidationInterfaceImplementationRules() {
    let src = r#"
interface IBase { fn BaseColor(s: Surface) -> float4; }
shader S implements IBase {
    fn BaseColor(s: Surface) -> float3 { return X; }
    override fn Extra() -> float4 { return X; }
}
"#;
    let d = ValidateSource(src).unwrap_err();
    assert!(
        d.iter()
            .any(|x| x.Message.contains("must be marked override"))
    );
    assert!(
        d.iter()
            .any(|x| x.Message.contains("signature does not match"))
    );
    assert!(
        d.iter()
            .any(|x| x.Message.contains("not declared by implemented interfaces"))
    );
}

#[test]
fn ValidationDuplicateGenericParameter() {
    let src = "shader Forward<TMat, TMat> { stage pixel fn PS() -> float4 { return X; } }";
    let d = ValidateSource(src).unwrap_err();
    assert!(
        d.iter()
            .any(|x| x.Message.contains("duplicate generic parameter 'TMat'"))
    );
}

#[test]
fn ValidationUnknownInterfaceAndWhereRules() {
    let src = r#"
interface IBaseColor { fn C() -> float4; }
shader Forward<TMat> implements IBaseColor where TMat: Missing {
    override fn C() -> float4 { return X; }
}
"#;
    let d = ValidateSource(src).unwrap_err();
    assert!(d.iter().any(|x| x.Message.contains("Missing")));
}

#[test]
fn ValidationCompileRules() {
    let valid = include_str!("../../../../examples/sdslv/generic_forward_pass.sdslv");
    assert!(
        ValidateSource(valid).is_ok(),
        "valid compile declaration should validate"
    );

    let unknown_generic = "shader Flat {} compile Missing<Flat> as X;";
    assert!(
        ValidateSource(unknown_generic)
            .unwrap_err()
            .iter()
            .any(|d| d.Message.contains("unknown generic shader"))
    );

    let nongeneric = "shader Flat {} compile Flat<Flat> as X;";
    assert!(
        ValidateSource(nongeneric)
            .unwrap_err()
            .iter()
            .any(|d| d.Message.contains("not generic"))
    );
}

#[test]
fn EmitHlslTypeAliasMappings() {
    let src = r#"
        type Color = float4;
        type Scalar = f32;
        type Count = i32;
        type Mask = u32;
        type ClipPosition4 = float4 @space(clip.position);
    "#;
    let module = ValidateSource(src).unwrap();
    let hlsl = EmitHlsl(&module).unwrap();

    assert!(
        hlsl.contains("typedef float4 Color;"),
        "expected float4 alias"
    );
    assert!(
        hlsl.contains("typedef float Scalar;"),
        "expected f32->float alias"
    );
    assert!(
        hlsl.contains("typedef int Count;"),
        "expected i32->int alias"
    );
    assert!(
        hlsl.contains("typedef uint Mask;"),
        "expected u32->uint alias"
    );
    assert!(
        hlsl.contains("// @space(clip.position)"),
        "expected space annotation comment"
    );
}

#[test]
fn EmitHlslStreamSemanticsAreDeterministic() {
    let src = r#"
        type ClipPosition4 = float4 @space(clip.position);
        type WorldPosition3 = float3 @space(world.position);
        stream VertexOut {
            Position: ClipPosition4;
            WorldPos: WorldPosition3;
            Color: float4;
        }
    "#;
    let module = ValidateSource(src).unwrap();
    let hlsl = EmitHlsl(&module).unwrap();

    assert!(
        hlsl.contains("struct VertexOut {"),
        "expected stream struct"
    );
    assert!(
        hlsl.contains("float4 Position : SV_Position;"),
        "expected SV_Position mapping"
    );
    assert!(
        hlsl.contains("float3 WorldPos : TEXCOORD0;"),
        "expected first TEXCOORD mapping"
    );
    assert!(
        hlsl.contains("float4 Color : TEXCOORD1;"),
        "expected second TEXCOORD mapping"
    );
}

#[test]
fn EmitHlslRecordHasNoStageSemanticsAndLocalsUseRecordType() {
    let src = r#"
        type ClipPosition4 = float4 @space(clip.position);
        record VertexData {
            Position: ClipPosition4;
            Color: float4;
        }
        stream VertexOut {
            Position: ClipPosition4;
            Color: float4;
        }
        shader Test {
            stage pixel fn PS() -> float4 {
                let surface: VertexData;
                return float4(1.0, 0.0, 1.0, 1.0);
            }
        }
    "#;
    let module = ValidateSource(src).unwrap();
    let hlsl = EmitHlsl(&module).unwrap();
    assert!(
        hlsl.contains("struct VertexData {"),
        "record should emit plain struct"
    );
    assert!(
        hlsl.contains("float4 Position;"),
        "record fields should not have stage semantics"
    );
    assert!(
        !hlsl.contains("struct VertexData {\n    float4 Position :"),
        "record fields should not emit SV_Position"
    );
    assert!(
        hlsl.contains("float4 Position : SV_Position;"),
        "stream fields should still emit SV_Position"
    );
    assert!(
        hlsl.contains("VertexData surface;"),
        "record locals should emit as record type declarations"
    );
}

#[test]
fn CompileSourceToHlslFlatColorContainsExpectedShape() {
    let src = r#"
        type ClipPosition4 = float4 @space(clip.position);
        stream VertexOut {
            Position: ClipPosition4;
            Color: float4;
        }
        shader FlatColor {
            stage vertex fn VS(pos: float3, color: float4) -> VertexOut {
                let output: VertexOut;
                return output;
            }
            stage pixel fn PS(input: VertexOut) -> float4 {
                return input.Color;
            }
        }
    "#;
    let hlsl = CompileSourceToHlsl(src).unwrap();

    assert!(hlsl.contains("struct VertexOut"), "expected stream struct");
    assert!(hlsl.contains("SV_Position"), "expected position semantic");
    assert!(hlsl.contains("TEXCOORD0"), "expected texcoord semantic");
    assert!(hlsl.contains("FlatColor_VS"), "expected vertex signature");
    assert!(hlsl.contains("FlatColor_PS"), "expected pixel signature");
    assert!(hlsl.contains("SV_Target"), "expected pixel return semantic");
    assert!(
        hlsl.contains("return input.Color;"),
        "expected raw body preservation"
    );
}

#[test]
fn EmitHlslCompileMonomorphizationShape() {
    let src = include_str!("../../../../examples/sdslv/generic_forward_pass.sdslv");
    let module = ValidateSource(src).unwrap();
    let hlsl = EmitHlsl(&module).unwrap();
    assert!(
        hlsl.contains("FlatMaterial_BaseColor"),
        "expected concrete helper method"
    );
    assert!(
        hlsl.contains("ForwardFlatMaterial_PS"),
        "expected compile alias stage name"
    );
    assert!(
        !hlsl.contains("ForwardPass_PS"),
        "generic stage should not emit directly"
    );
    assert!(
        hlsl.contains("FlatMaterial mat"),
        "expected substituted parameter type"
    );
    assert!(
        hlsl.contains("FlatMaterial_BaseColor(s)"),
        "expected rewritten interface call"
    );
}

#[test]
fn CompileSourceToHlslInvalidSourceReturnsDiagnostics() {
    let src = "shader S { stage pixel fn PS() -> float4; }";
    let diagnostics = CompileSourceToHlsl(src).unwrap_err();
    assert!(
        diagnostics
            .iter()
            .any(|x| x.Message.contains("must have a body")),
        "expected validation failure before emission"
    );
}

#[test]
fn CompileSourceToShaderArtifactBasicArtifactMetadata() {
    let src = r#"
        type ClipPosition4 = float4 @space(clip.position);
        stream VertexOut { Position: ClipPosition4; Color: float4; }
        shader FlatColor {
            stage vertex fn VS(pos: float3, color: float4) -> VertexOut {
                let output: VertexOut;
                output.Position = float4(pos, 1.0);
                output.Color = color;
                return output;
            }
            stage pixel fn PS(input: VertexOut) -> float4 {
                return input.Color;
            }
        }
    "#;
    let artifact = CompileSourceToShaderArtifact("flat_color.sdslv", src)
        .expect("expected flat color artifact to compile");

    assert_eq!(
        artifact.SourceName, "flat_color.sdslv",
        "source name should be preserved"
    );
    assert!(
        !artifact.Hlsl.is_empty(),
        "artifact hlsl text should not be empty"
    );
    assert_eq!(
        artifact.EntryPoints.len(),
        2,
        "expected vertex + pixel entry points"
    );
    assert_eq!(
        artifact.EntryPoints[0].Name, "FlatColor_VS",
        "vertex entry name should match HLSL name"
    );
    assert_eq!(
        artifact.EntryPoints[0].TargetProfile, "vs_6_0",
        "vertex profile should map to vs_6_0"
    );
    assert_eq!(
        artifact.EntryPoints[1].Name, "FlatColor_PS",
        "pixel entry name should match HLSL name"
    );
    assert_eq!(
        artifact.EntryPoints[1].TargetProfile, "ps_6_0",
        "pixel profile should map to ps_6_0"
    );
    assert_eq!(
        artifact.EntryPoints[0].ShaderName, "FlatColor",
        "shader name should be captured"
    );
    assert_eq!(
        artifact.EntryPoints[0].MethodName, "VS",
        "method name should be captured"
    );
}

#[test]
fn CompileSourceToShaderArtifactExcludesHelpersAndFlows() {
    let src = r#"
        type ClipPosition4 = float4 @space(clip.position);
        stream VertexOut { Position: ClipPosition4; Color: float4; }
        shader FlatColor {
            fn BaseColor(input: VertexOut) -> float4 { return input.Color; }
            stage vertex fn VS(pos: float3, color: float4) -> VertexOut {
                let output: VertexOut;
                output.Position = float4(pos, 1.0);
                output.Color = color;
                return output;
            }
            stage pixel fn PS(input: VertexOut) -> float4 {
                return FlatColor_BaseColor(input);
            }
        }
        flow PickMode(useSoft: bool, quality: i32) -> i32 {
            state Select {
                when {
                    case useSoft -> return 2
                    else -> return quality
                }
            }
        }
    "#;
    let artifact = CompileSourceToShaderArtifact("flow_value_lowering.sdslv", src)
        .expect("expected flow fixture artifact to compile");
    assert!(
        artifact.Hlsl.contains("int PickMode("),
        "flow helper should be emitted into HLSL"
    );
    assert!(
        artifact.Hlsl.contains("FlatColor_BaseColor"),
        "ordinary helper methods should still be emitted"
    );
    assert!(
        artifact
            .EntryPoints
            .iter()
            .all(|entry| entry.Name != "PickMode"),
        "flow helper must not be listed as entry point"
    );
    assert!(
        artifact
            .EntryPoints
            .iter()
            .all(|entry| entry.Name != "FlatColor_BaseColor"),
        "non-stage helper must not be listed as entry point"
    );
}

#[test]
fn CompileSourceToShaderArtifactUsesCompileAliasEntries() {
    let src = include_str!("../../../../examples/sdslv/generic_forward_pass.sdslv");
    let artifact = CompileSourceToShaderArtifact("generic_forward_pass.sdslv", src)
        .expect("expected generic fixture artifact to compile");

    assert!(
        artifact
            .EntryPoints
            .iter()
            .any(|entry| entry.Name == "ForwardFlatMaterial_PS"),
        "compile alias entry should be present"
    );
    assert!(
        artifact
            .EntryPoints
            .iter()
            .all(|entry| entry.Name != "ForwardPass_PS"),
        "generic template entry should not be present"
    );
    let alias_entry = artifact
        .EntryPoints
        .iter()
        .find(|entry| entry.Name == "ForwardFlatMaterial_PS")
        .expect("expected compile alias entry");
    assert_eq!(
        alias_entry.TargetProfile, "ps_6_0",
        "pixel alias profile should map to ps_6_0"
    );
}

#[test]
fn CompileSourceToShaderArtifactFailuresReturnDiagnostics() {
    let invalid = "shader S { stage pixel fn PS() -> float4; }";
    let invalid_diagnostics = CompileSourceToShaderArtifact("invalid.sdslv", invalid).unwrap_err();
    assert!(
        invalid_diagnostics
            .iter()
            .any(|d| d.Message.contains("must have a body")),
        "invalid parse/validate/emission path should return diagnostics"
    );

    let unsupported_compute =
        "shader C { stage compute fn CS() -> float4 { return float4(0.0, 0.0, 0.0, 1.0); } }";
    let compute_diagnostics =
        CompileSourceToShaderArtifact("compute.sdslv", unsupported_compute).unwrap_err();
    assert!(
        compute_diagnostics
            .iter()
            .any(|d| d.Message.contains("unsupported stage 'compute'")),
        "unsupported compute emission should return diagnostics and no artifact"
    );
}

#[test]
fn CompileSourceToShaderArtifactDeterministicOutputAndOrder() {
    let src = r#"
        type ClipPosition4 = float4 @space(clip.position);
        stream VertexOut { Position: ClipPosition4; Color: float4; }
        shader FlatColor {
            stage vertex fn VS(pos: float3, color: float4) -> VertexOut {
                let output: VertexOut;
                output.Position = float4(pos, 1.0);
                output.Color = color;
                return output;
            }
            stage pixel fn PS(input: VertexOut) -> float4 {
                return input.Color;
            }
        }
    "#;
    let artifact_a = CompileSourceToShaderArtifact("flat_color.sdslv", src)
        .expect("first artifact build should succeed");
    let artifact_b = CompileSourceToShaderArtifact("flat_color.sdslv", src)
        .expect("second artifact build should succeed");

    assert_eq!(
        artifact_a.Hlsl, artifact_b.Hlsl,
        "artifact HLSL should be deterministic"
    );
    assert_eq!(
        artifact_a.EntryPoints, artifact_b.EntryPoints,
        "artifact entry points should be deterministic"
    );
    assert_eq!(
        artifact_a
            .EntryPoints
            .iter()
            .map(|entry| entry.Name.as_str())
            .collect::<Vec<&str>>(),
        vec!["FlatColor_VS", "FlatColor_PS"],
        "entry order should remain stable and declaration ordered"
    );
}

#[test]
fn ParserFlowShapes() {
    let src = r#"
flow SelectShadow(mode: i32) -> float4 {
    state Select {
        when {
            case mode == 1 -> goto Hard
            case mode == 2 -> return 2
            else -> goto None
        }
    }
    state None { return 0; }
    state Hard { goto None; }
}
"#;
    let module = ParseSource(src).expect("flow source should parse");
    let flow = module
        .Declarations
        .iter()
        .find_map(|d| match d {
            SdslvDecl::Flow(flow) => Some(flow),
            _ => None,
        })
        .expect("expected flow declaration");
    assert_eq!(flow.Parameters.len(), 1, "expected one flow parameter");
    assert_eq!(flow.States.len(), 3, "expected three states");
}

#[test]
fn ValidationFlowRules() {
    let valid = r#"
type Color = float4;
stream S { C: Color; }
shader P { stage pixel fn PS() -> float4 { return float4(0, 0, 0, 1); } }
flow F(mode: i32) -> i32 {
    state A {
        when {
            case mode == 1 -> goto B
            else -> return 0
        }
    }
    state B { return 1; }
}
"#;
    assert!(ValidateSource(valid).is_ok(), "valid flow should validate");

    let bad = r#"
flow F() -> i32 {
    state A {
        when {
            case 1 == 1 -> goto Missing
        }
    }
    state A {}
}
"#;
    let diagnostics = ValidateSource(bad).unwrap_err();
    assert!(
        diagnostics
            .iter()
            .any(|d| d.Message.contains("duplicate state 'A'"))
    );
    assert!(
        diagnostics
            .iter()
            .any(|d| d.Message.contains("unknown state 'Missing'"))
    );
    assert!(
        diagnostics
            .iter()
            .any(|d| d.Message.contains("must include else"))
    );
    assert!(
        diagnostics
            .iter()
            .any(|d| d.Message.contains("must contain at least one statement"))
    );
}

#[test]
fn ParserFlowRejectsUnsupportedStateStatement() {
    let src = r#"
flow F() -> i32 {
    state A {
        let x: i32 = 1;
    }
}
"#;
    let diagnostics = ParseSource(src).unwrap_err();
    assert!(
        diagnostics.iter().any(|d| d
            .Message
            .contains("unsupported statement in flow state body")),
        "unsupported flow-state statement should produce diagnostic"
    );
}

#[test]
fn EmitHlslFlowValueLoweringWorks() {
    let src = include_str!("../../../../examples/sdslv/flow_value_lowering.sdslv");
    let module = ParseSource(src).expect("flow source should parse");
    let hlsl = EmitHlsl(&module).expect("acyclic value flow should emit");
    assert!(
        hlsl.contains("int PickMode(bool useSoft, int quality) {"),
        "expected flow helper signature"
    );
    assert!(
        hlsl.contains("int SelectedMode = 0;"),
        "expected board local"
    );
    assert!(hlsl.contains("if (useSoft) {"), "expected when if lowering");
    assert!(
        hlsl.contains("else if (quality > 2) {"),
        "expected when else-if lowering"
    );
    assert!(
        hlsl.contains("SelectedMode = 2;"),
        "expected board assignment lowering"
    );
    assert!(
        hlsl.contains("return SelectedMode;"),
        "expected board read lowering"
    );
}

#[test]
fn EmitHlslIsDeterministic() {
    let src = include_str!("../../../../examples/sdslv/flat_color.sdslv");
    let module = ValidateSource(src).unwrap();
    let hlsl_a = EmitHlsl(&module).unwrap();
    let hlsl_b = EmitHlsl(&module).unwrap();
    assert_eq!(hlsl_a, hlsl_b, "expected deterministic output");
}

#[test]
fn ParserBodySubsetParsesLetAssignReturnAndCalls() {
    let src = r#"shader S {
        stage vertex fn VS(pos: float3, input: VertexOut) -> VertexOut {
            let output: VertexOut;
            let color: float4 = input.Color;
            output.Position = float4(pos, 1.0);
            output.Color = color;
            return output;
        }
    }"#;
    let module = ParseSource(src).unwrap();
    let shader = module
        .Declarations
        .iter()
        .find_map(|d| {
            if let SdslvDecl::Shader(s) = d {
                Some(s)
            } else {
                None
            }
        })
        .unwrap();
    let body = shader.StageMethods[0].Body.as_ref().unwrap();
    assert_eq!(body.Statements.len(), 5, "expected 5 parsed statements");
}

#[test]
fn ParserBodySubsetParsesArithmeticPrecedence() {
    let src = "shader S { stage pixel fn PS(a: float4) -> float4 { return (1 + 2) * 3; } }";
    let module = ParseSource(src).unwrap();
    let shader = module
        .Declarations
        .iter()
        .find_map(|d| {
            if let SdslvDecl::Shader(s) = d {
                Some(s)
            } else {
                None
            }
        })
        .unwrap();
    let body = shader.StageMethods[0].Body.as_ref().unwrap();
    assert!(
        matches!(&body.Statements[0], SdslvStatement::Return { .. }),
        "expected return statement"
    );
}

#[test]
fn ParserBodySubsetFailuresProduceDiagnostics() {
    let missing_semicolon =
        ParseSource("shader S { stage pixel fn PS() -> float4 { let x: float4 } }").unwrap_err();
    assert!(
        missing_semicolon
            .iter()
            .any(|d| d.Message.contains("expected ';' after let declaration"))
    );

    let missing_return_expr =
        ParseSource("shader S { stage pixel fn PS() -> float4 { return; } }").unwrap_err();
    assert!(
        missing_return_expr
            .iter()
            .any(|d| d.Message.contains("expected expression after return"))
    );

    let unsupported_if = ParseSource(
        "shader S { stage pixel fn PS() -> float4 { if (a) { return a; } else return a; } }",
    )
    .unwrap_err();
    assert!(
        unsupported_if
            .iter()
            .any(|d| d.Message.contains("else clause missing '{'"))
    );

    let bad_call =
        ParseSource("shader S { stage pixel fn PS() -> float4 { return float4(1.0, 0.0; } }")
            .unwrap_err();
    assert!(
        bad_call
            .iter()
            .any(|d| d.Message.contains("expected ')' to close function call"))
    );
}

#[test]
fn ParserBodySubsetParsesWithExpression() {
    let src = r#"
        record SurfaceData { Roughness: f32; BaseColor: float4; }
        shader S {
            fn F(surface: SurfaceData) -> SurfaceData {
                let adjusted: SurfaceData = surface with { Roughness: 0.5, BaseColor: surface.BaseColor, };
                return adjusted;
            }
        }
    "#;
    let module = ParseSource(src).expect("with expression should parse");
    let shader = module
        .Declarations
        .iter()
        .find_map(|d| {
            if let SdslvDecl::Shader(s) = d {
                Some(s)
            } else {
                None
            }
        })
        .unwrap();
    let body = shader.Methods[0].Body.as_ref().unwrap();
    match &body.Statements[0] {
        SdslvStatement::Let {
            Initializer: Some(SdslvExpression::With { Updates, .. }),
            ..
        } => {
            assert_eq!(Updates.len(), 2, "with updates should parse");
        }
        _ => panic!("expected with expression initializer"),
    }
}

#[test]
fn ValidationWithExpressionDiagnostics() {
    let src = r#"
        record SurfaceData { Roughness: f32; }
        shader S {
            fn F(surface: SurfaceData, color: float4) -> SurfaceData {
                let a: SurfaceData = surface with { Missing: 0.5 };
                let b: SurfaceData = surface with { Roughness: 0.5, Roughness: 0.8 };
                let c: SurfaceData = surface with { Roughness: color };
                return c;
            }
        }
    "#;
    let diagnostics = ValidateSource(src).expect_err("invalid with usage should fail");
    assert!(
        diagnostics
            .iter()
            .any(|d| d.Message.contains("does not exist on type 'SurfaceData'"))
    );
    assert!(diagnostics.iter().any(|d| {
        d.Message
            .contains("duplicate with field update 'Roughness'")
    }));
    assert!(
        diagnostics
            .iter()
            .any(|d| d.Message.contains("expects f32, found float4"))
    );
}

#[test]
fn EmitHlslWithExpressionLowersDeterministically() {
    let src = r#"
        record SurfaceData { Roughness: f32; }
        shader S {
            fn F(surface: SurfaceData) -> SurfaceData {
                let adjusted: SurfaceData = surface with { Roughness: 0.5 };
                adjusted = surface with { Roughness: 0.25 };
                return surface with { Roughness: 0.75 };
            }
        }
    "#;
    let hlsl_a = CompileSourceToHlsl(src).expect("with emission should compile");
    let hlsl_b = CompileSourceToHlsl(src).expect("with emission should be deterministic");
    assert!(hlsl_a.contains("SurfaceData adjusted = surface;"));
    assert!(hlsl_a.contains("adjusted.Roughness = 0.5;"));
    assert!(hlsl_a.contains("adjusted = surface;"));
    assert!(hlsl_a.contains("__with0"));
    assert_eq!(hlsl_a, hlsl_b, "same source should emit identical hlsl");
}

#[test]
fn ValidationRejectsImmutableStreamParameterFieldAssignment() {
    let src = r#"
        stream VertexOut { Color: float4; }
        shader S {
            stage pixel fn PS(input: VertexOut) -> float4 {
                input.Color = float4(1.0, 0.0, 0.0, 1.0);
                return input.Color;
            }
        }
    "#;
    let diagnostics = ValidateSource(src).expect_err("stream parameter field mutation should fail");
    assert!(
        diagnostics.iter().any(|d| d.Message.contains(
            "cannot assign to field 'Color' of immutable stream parameter 'input'; use with to create a modified copy"
        )),
        "stream parameter field assignment should be rejected with immutability guidance"
    );
}

#[test]
fn ValidationRejectsImmutableRecordParameterFieldAssignment() {
    let src = r#"
        record SurfaceData { Roughness: f32; }
        shader S {
            fn Adjust(surface: SurfaceData) -> SurfaceData {
                surface.Roughness = 0.5;
                return surface;
            }
        }
    "#;
    let diagnostics = ValidateSource(src).expect_err("record parameter field mutation should fail");
    assert!(
        diagnostics.iter().any(|d| d.Message.contains(
            "cannot assign to field 'Roughness' of immutable record parameter 'surface'; use with to create a modified copy"
        )),
        "record parameter field assignment should be rejected with immutability guidance"
    );
}

#[test]
fn ValidationAllowsLocalAggregateConstructionAndCopyUpdateMutation() {
    let src = r#"
        type ClipPosition4 = float4 @space(clip.position);
        stream VertexOut { Position: ClipPosition4; Color: float4; }
        record SurfaceData { Roughness: f32; }
        shader S {
            stage vertex fn VS(pos: float3, color: float4) -> VertexOut {
                let output: VertexOut;
                output.Position = float4(pos, 1.0);
                output.Color = color;
                return output;
            }
            fn Adjust(surface: SurfaceData) -> SurfaceData {
                let copy: SurfaceData = surface;
                copy.Roughness = 0.5;
                return copy;
            }
        }
    "#;
    assert!(
        ValidateSource(src).is_ok(),
        "local stream construction and local record copy update should remain valid"
    );
    let hlsl =
        CompileSourceToHlsl(src).expect("valid local aggregate assignment patterns should emit");
    assert!(
        hlsl.contains("output.Position = float4(pos, 1.0);"),
        "stream local-construction assignment should still emit"
    );
    assert!(
        hlsl.contains("copy.Roughness = 0.5;"),
        "local record field update should still emit"
    );
}

#[test]
fn ValidationAllowsWithForImmutableAggregateParameters() {
    let src = r#"
        stream VertexOut { Color: float4; }
        record SurfaceData { Roughness: f32; }
        shader S {
            fn Adjust(surface: SurfaceData) -> SurfaceData {
                let adjusted: SurfaceData = surface with { Roughness: 0.5, };
                return adjusted;
            }
            stage pixel fn PS(input: VertexOut) -> float4 {
                let adjusted: VertexOut = input with { Color: input.Color, };
                return adjusted.Color;
            }
        }
    "#;
    assert!(
        ValidateSource(src).is_ok(),
        "with should remain the supported update path for immutable aggregate parameters"
    );
}

#[test]
fn EmitHlslBodySubsetDeterministicFormatting() {
    let src = r#"
        type ClipPosition4 = float4 @space(clip.position);
        stream VertexOut {
            Position: ClipPosition4;
            Color: float4;
        }
        shader FlatColor {
            stage vertex fn VS(pos: float3, color: float4) -> VertexOut {
                let output: VertexOut;
                output.Position = float4(pos, 1.0);
                output.Color = color;
                return output;
            }
            stage pixel fn PS(input: VertexOut) -> float4 {
                return input.Color;
            }
        }
    "#;
    let hlsl = CompileSourceToHlsl(src).unwrap();
    assert!(hlsl.contains("    VertexOut output;"));
    assert!(hlsl.contains("    output.Position = float4(pos, 1.0);"));
    assert!(hlsl.contains("    return input.Color;"));
}

#[test]
fn ValidationM6CoordinateSpaceCallAssignmentReturnMismatches() {
    let src = r#"
        type WorldPosition3 = float3 @space(world.position);
        type ClipPosition4 = float4 @space(clip.position);

        shader S {
            fn UseWorld(p: WorldPosition3) -> WorldPosition3 { return p; }
            fn Bad(c: ClipPosition4) -> WorldPosition3 {
                let world: WorldPosition3 = c;
                return UseWorld(c);
            }
        }
    "#;
    let d = ValidateSource(src).unwrap_err();
    assert!(d.iter().any(|x| {
        x.Message
            .contains("expected WorldPosition3, found ClipPosition4")
    }));
    assert!(d.iter().any(|x| {
        x.Message
            .contains("argument 1 of UseWorld expects WorldPosition3, found ClipPosition4")
    }));
}

#[test]
fn ValidationM6CoordinateSpaceSameUnderlyingStillMismatch() {
    let src = r#"
        type WorldVector3 = float3 @space(world.vector);
        type WorldNormal3 = float3 @space(world.normal);

        shader S {
            fn Bad(v: WorldVector3) -> WorldNormal3 {
                return v;
            }
        }
    "#;
    let d = ValidateSource(src).unwrap_err();
    assert!(d.iter().any(|x| {
        x.Message
            .contains("expected WorldNormal3, found WorldVector3")
    }));
}

#[test]
fn CompileSourceToHlslM6BlocksInvalidCoordinateMismatch() {
    let src = r#"
        type WorldPosition3 = float3 @space(world.position);
        type ClipPosition4 = float4 @space(clip.position);
        shader S {
            fn Bad(c: ClipPosition4) -> WorldPosition3 { return c; }
        }
    "#;
    let d = CompileSourceToHlsl(src).unwrap_err();
    assert!(d.iter().any(|x| x.Message.contains("return type mismatch")));
}

#[test]
fn ParseTestSourceValidFactAndAsserts() {
    let src = include_str!("../../../../examples/sdslv/basic_asserts.sdslvtest");
    let module = ParseTestSource(src).unwrap();
    assert_eq!(module.Tests.len(), 1);
    assert_eq!(module.Tests[0].Attributes[0].Name, "Fact");
    assert!(matches!(
        module.Tests[0].Body.Statements.last().unwrap(),
        SdslvStatement::Expression { .. }
    ));
}

#[test]
fn ParseTestSourceInvalidCases() {
    assert!(ParseTestSource("[Fact fn A() {}").is_err());
    assert!(ParseTestSource("[Fact]").is_err());
    assert!(ParseTestSource("[Fact] fn A() { Assert.True(, \"x\"); }").is_err());
    assert!(
        ParseTestSource(
            "[Theory][InlineData(1,2] fn A(a: i32, b: i32) { Assert.True(true, \"x\"); }"
        )
        .is_err()
    );
}

#[test]
fn ParseTestSourceTheoryAndInlineDataRows() {
    let src = r#"
[Theory]
[InlineData(1, 2, 3)]
[InlineData(4, 5, 9)]
[InlineData(1.5, 1.0)]
[InlineData(true, false)]
fn Mixed(a: i32, b: i32, expected: i32) {
    Assert.Equals(a + b, expected, "sum should match");
}
"#;
    let module = ParseTestSource(src).unwrap();
    assert_eq!(module.Tests.len(), 1);
    let attributes = &module.Tests[0].Attributes;
    assert_eq!(attributes[0].Name, "Theory");
    assert_eq!(attributes[1].Name, "InlineData");
    assert_eq!(attributes[1].Arguments.len(), 3);
}

#[test]
fn ValidateTestSourceValid() {
    let src = r#"
namespace T;
[Fact]
fn A() {
    let a: f32 = 1.0;
    Assert.True(true, "truth holds");
    Assert.Equals(a, 1.0, "values equal");
    Assert.Near(a, 1.0, 0.1, "values near");
}
"#;
    assert!(ValidateTestSource(src).is_ok());
}

#[test]
fn ValidateTestSourceInvalidRules() {
    let src = r#"
namespace T;
[Fact]
fn A(x: f32) {
    Assert.True(true);
    Assert.Equals(1.0, 1.0, 42);
    Assert.Near(1.0, 1.0, 0.01);
    Assert.Approximately(1.0, 1.0, "x");
    NotAssert();
}
[Fact(one)]
fn A() { Assert.True(true, "ok"); }
"#;
    let d = ValidateTestSource(src).unwrap_err();
    assert!(
        d.iter()
            .any(|x| x.Message.contains("[Fact] does not accept arguments"))
    );
    assert!(
        d.iter()
            .any(|x| x.Message.contains("must not declare parameters"))
    );
    assert!(
        d.iter()
            .any(|x| x.Message.contains("[Fact] does not accept arguments"))
    );
    assert!(
        d.iter()
            .any(|x| x.Message.contains("duplicate test function 'A'"))
    );
    assert!(
        d.iter()
            .any(|x| x.Message.contains("Assert.True requires 2 arguments"))
    );
    assert!(d.iter().any(|x| {
        x.Message
            .contains("Assert.Equals requires a custom message string argument")
    }));
    assert!(
        d.iter()
            .any(|x| x.Message.contains("Assert.Near requires 4 arguments"))
    );
    assert!(d.iter().any(|x| {
        x.Message
            .contains("unsupported Assert method 'Assert.Approximately'")
    }));
    assert!(
        d.iter()
            .any(|x| x.Message.contains("non-assert expression statement"))
    );
}

#[test]
fn ValidateTestSourceTheoryRules() {
    let valid = r#"
[Theory]
[InlineData(0.0, 0.0)]
[InlineData(1.5, 1.0)]
fn ClampWorks(input: f32, expected: f32) {
    Assert.Near(clamp(input, 0.0, 1.0), expected, 0.0001, "clamp should saturate the input");
}
"#;
    assert!(ValidateTestSource(valid).is_ok());

    let missing_data =
        ValidateTestSource("[Theory] fn MissingData(a: i32) { Assert.True(true, \"x\"); }")
            .unwrap_err();
    assert!(missing_data.iter().any(|x| {
        x.Message
            .contains("[Theory] requires at least one [InlineData(...)] row")
    }));

    let no_params =
        ValidateTestSource("[Theory][InlineData(1)] fn NoParams() { Assert.True(true, \"x\"); }")
            .unwrap_err();
    assert!(no_params.iter().any(|x| {
        x.Message
            .contains("[Theory] test must declare parameters matching InlineData rows")
    }));

    let inline_on_fact =
        ValidateTestSource("[Fact][InlineData(1)] fn Bad() { Assert.True(true, \"x\"); }")
            .unwrap_err();
    assert!(inline_on_fact.iter().any(|x| {
        x.Message
            .contains("[InlineData] is only valid on [Theory] tests")
    }));

    let both = ValidateTestSource(
        "[Fact][Theory][InlineData(1)] fn Bad(a: i32) { Assert.True(true, \"x\"); }",
    )
    .unwrap_err();
    assert!(both.iter().any(|x| {
        x.Message
            .contains("test function cannot have both [Fact] and [Theory]")
    }));

    let arity = ValidateTestSource(
        "[Theory][InlineData(1,2)] fn Bad(a: i32) { Assert.True(true, \"x\"); }",
    )
    .unwrap_err();
    assert!(arity.iter().any(|x| {
        x.Message
            .contains("InlineData arity mismatch: expected 1 values, found 2")
    }));

    let type_mismatch = ValidateTestSource(
        "[Theory][InlineData(true)] fn Bad(a: i32) { Assert.True(true, \"x\"); }",
    )
    .unwrap_err();
    assert!(type_mismatch.iter().any(|x| {
        x.Message
            .contains("InlineData type mismatch for parameter 'a': expected i32, found bool")
    }));
}

#[test]
fn RunTestSourcePassesBasicFixture() {
    let src = include_str!("../../../../examples/sdslv/basic_asserts.sdslvtest");
    let result = RunTestSource(src);
    assert!(result.Diagnostics.is_empty(), "expected no diagnostics");
    assert!(result.Passed, "expected basic asserts fixture to pass");
}

#[test]
fn RunTestSourceReportsParseAndValidationDiagnostics() {
    let parse_result = RunTestSource("[Fact]");
    assert!(!parse_result.Passed, "parse failures should not pass");
    assert!(
        !parse_result.Diagnostics.is_empty(),
        "expected parse diagnostics"
    );

    let validation_source = r#"
[Fact]
fn A() {
    Assert.True(1.0, "not bool");
    Assert.True(true);
}
"#;
    let validation_result = RunTestSource(validation_source);
    assert!(
        !validation_result.Passed,
        "validation failures should not pass"
    );
    assert!(
        !validation_result.Diagnostics.is_empty(),
        "expected validation diagnostics"
    );
}

#[test]
fn RunTestSourceEvaluatesLocalsArithmeticComparisonsAndAsserts() {
    let src = r#"
[Fact]
fn Evaluates() {
    let x: f32 = 1.0 + 2.0 * 3.0;
    let y: f32 = (1.0 + 2.0) * 3.0;
    let z: i32 = -2;
    let b: bool;
    Assert.Equals(x, 7.0, "precedence should work");
    Assert.Equals(y, 9.0, "parentheses should work");
    Assert.True(x < y, "comparison should work");
    Assert.True(z == -2, "unary minus should work");
    Assert.Equals(b, false, "bool default should be false");
}
"#;
    let result = RunTestSource(src);
    assert!(result.Passed, "expected evaluator scenario to pass");
}

#[test]
fn RunTestSourceCollectsFailuresAndContinuesAcrossTests() {
    let src = r#"
[Fact]
fn FailsTwice() {
    let value: f32 = 1.0;
    Assert.True(false, "first failure");
    Assert.Equals(value, 2.0, "second failure");
}

[Fact]
fn StillRuns() {
    let actual: f32;
    actual = 2.0;
    Assert.Near(actual, 2.001, 0.01, "near should pass");
}
"#;
    let result = RunTestSource(src);
    assert!(!result.Passed, "expected run to fail");
    assert_eq!(result.Tests.len(), 2, "expected both tests to execute");
    assert_eq!(
        result.Tests[0].Failures.len(),
        2,
        "expected both assertion failures"
    );
    assert!(result.Tests[1].Passed, "second test should still pass");
}

#[test]
fn RunTestSourceReportsUnsupportedExecutionCleanly() {
    let src = r#"
[Fact]
fn UnsupportedCall() {
    let value: f32 = Custom(1.0);
    Assert.True(true, "unreachable");
}
"#;
    let result = RunTestSource(src);
    assert!(!result.Passed, "unsupported calls should fail");
    assert!(
        result.Tests[0].Failures[0]
            .Message
            .contains("unsupported function call")
    );
}

#[test]
fn RunTestSourceExecutesTheoryRowsAndReportsPerRowFailures() {
    let src = r#"
[Theory]
[InlineData(1, 2, 3)]
[InlineData(2, 2, 5)]
[InlineData(3, 3, 6)]
fn AddWorks(a: i32, b: i32, expected: i32) {
    Assert.Equals(a + b, expected, "sum should match expected value");
}
"#;
    let result = RunTestSource(src);
    assert!(!result.Passed);
    assert_eq!(
        result.Tests.len(),
        3,
        "expected one result per InlineData row"
    );
    assert_eq!(result.Tests[0].Name, "AddWorks[0]");
    assert!(result.Tests[0].Passed);
    assert_eq!(result.Tests[1].Name, "AddWorks[1]");
    assert!(!result.Tests[1].Passed);
    assert!(result.Tests[1].Failures[0].Message.contains("row 1"));
    assert!(
        result.Tests[2].Passed,
        "later rows should continue executing"
    );
}

#[test]
fn RunTestSourceTheoryBindsParameterValues() {
    let src = r#"
[Theory]
[InlineData(0.0, 0.0)]
[InlineData(0.5, 0.5)]
[InlineData(1.5, 1.0)]
fn SaturateClampsToUnit(input: f32, expected: f32) {
    Assert.Near(saturate(input), expected, 0.0001, "saturate should clamp into [0, 1]");
}
"#;
    let result = RunTestSource(src);
    assert!(
        result.Passed,
        "theory rows should bind parameters for evaluation"
    );
    assert_eq!(result.Tests.len(), 3);
}

#[test]
fn ParserFlowBoardShapes() {
    let src = r#"
flow ShadowVariant(useSoft: bool, quality: i32) -> i32 {
    board {
        HasSelection: bool;
        SelectedMode: i32;
        BlendWeight: f32;
    }
    state Select { return quality; }
}
"#;
    let module = ParseSource(src).expect("flow with board should parse");
    let flow = module
        .Declarations
        .iter()
        .find_map(|d| match d {
            SdslvDecl::Flow(flow) => Some(flow),
            _ => None,
        })
        .expect("expected flow declaration");
    let board = flow.Board.as_ref().expect("expected board block");
    assert_eq!(board.Fields.len(), 3, "expected 3 board fields");
    assert_eq!(board.Fields[0].Name, "HasSelection", "expected field name");
    assert_eq!(
        board.Fields[0].TypeName.ToDisplayString(),
        "bool",
        "expected field type"
    );
}

#[test]
fn ParserFlowBoardRejectsInvalidFieldSyntaxAndPlacement() {
    let missing_colon =
        ParseSource("flow F() -> i32 { board { X i32; } state A { return 1; } }").unwrap_err();
    assert!(
        missing_colon
            .iter()
            .any(|d| d.Message.contains("expected ':' after board field name")),
        "missing colon should be diagnosed"
    );

    let missing_semicolon =
        ParseSource("flow F() -> i32 { board { X: i32 } state A { return 1; } }").unwrap_err();
    assert!(
        missing_semicolon
            .iter()
            .any(|d| d.Message.contains("expected ';' after board field")),
        "missing semicolon should be diagnosed"
    );

    let initializer =
        ParseSource("flow F() -> i32 { board { X: i32 = 1; } state A { return 1; } }").unwrap_err();
    assert!(
        initializer
            .iter()
            .any(|d| d.Message.contains("unsupported board initializer")),
        "board initializer should be rejected"
    );

    let board_after_state =
        ParseSource("flow F() -> i32 { state A { return 1; } board { X: i32; } }").unwrap_err();
    assert!(
        board_after_state
            .iter()
            .any(|d| d.Message.contains("board must be declared before states")),
        "board placement should be diagnosed"
    );
}

#[test]
fn ValidationFlowBoardRules() {
    let valid = r#"
flow F() -> i32 {
    board {
        Flag: bool;
        Weight: float;
    }
    state A { return 1; }
}
"#;
    assert!(
        ValidateSource(valid).is_ok(),
        "valid board flow should validate"
    );

    let without_board = "flow G() -> i32 { state A { return 1; } }";
    assert!(
        ValidateSource(without_board).is_ok(),
        "flow without board should validate"
    );

    let duplicate_field = "flow F() -> i32 { board { X: i32; X: i32; } state A { return 1; } }";
    assert!(
        ValidateSource(duplicate_field)
            .unwrap_err()
            .iter()
            .any(|d| d.Message.contains("duplicate board field 'X'")),
        "duplicate board fields should be rejected"
    );

    let empty_board = "flow F() -> i32 { board { } state A { return 1; } }";
    assert!(
        ValidateSource(empty_board)
            .unwrap_err()
            .iter()
            .any(|d| d.Message.contains("board must declare at least one field")),
        "empty board should be rejected"
    );

    let unknown_type = "flow F() -> i32 { board { X: UnknownType; } state A { return 1; } }";
    assert!(
        ValidateSource(unknown_type).unwrap_err().iter().any(|d| d
            .Message
            .contains("unsupported board field type 'UnknownType'")),
        "unknown board type should be rejected"
    );

    let duplicate_board =
        "flow F() -> i32 { board { X: i32; } board { Y: i32; } state A { return 1; } }";
    assert!(
        ParseSource(duplicate_board)
            .unwrap_err()
            .iter()
            .any(|d| d.Message.contains("at most one board block")),
        "duplicate board blocks should be rejected"
    );
}

#[test]
fn ValidationFlowBoardReadsResolvesKnownFields() {
    let src = r#"
flow ShadowVariant(useSoft: bool) -> i32 {
    board {
        HasSelection: bool;
        SelectedMode: i32;
    }
    state Select {
        when {
            case board.HasSelection -> goto Selected
            case board.SelectedMode == 2 -> return board.SelectedMode
            else -> return Choose(board.SelectedMode)
        }
    }
    state Selected { return board.SelectedMode; }
}
"#;
    assert!(
        ValidateSource(src).is_ok(),
        "known board reads in case/return expressions should validate"
    );
}

#[test]
fn ValidationFlowBoardReadsRejectUnknownAndMissingBoard() {
    let unknown_case = r#"
flow ShadowVariant() -> i32 {
    board {
        HasSelection: bool;
    }
    state Select {
        when {
            case board.SelectedMode == 2 -> goto Done
            else -> goto Done
        }
    }
    state Done { return 1; }
}
"#;
    assert!(
        ValidateSource(unknown_case).unwrap_err().iter().any(|d| d
            .Message
            .contains("unknown board field 'SelectedMode' in flow 'ShadowVariant'")),
        "unknown board field in guard condition should be rejected"
    );

    let unknown_return = r#"
flow F() -> i32 {
    board { Known: i32; }
    state A { return board.Missing; }
}
"#;
    assert!(
        ValidateSource(unknown_return).unwrap_err().iter().any(|d| d
            .Message
            .contains("unknown board field 'Missing' in flow 'F'")),
        "unknown board field in return expression should be rejected"
    );

    let missing_board = r#"
flow F() -> i32 {
    state A {
        when {
            case board.Mode == 1 -> return board.Mode
            else -> return 0
        }
    }
}
"#;
    assert!(
        ValidateSource(missing_board).unwrap_err().iter().any(|d| d
            .Message
            .contains("flow 'F' does not declare a board, but expression references board.Mode")),
        "board reads without board declaration should be rejected"
    );
}

#[test]
fn ValidationFlowRejectsReservedBoardParameterName() {
    let src = r#"
flow F(board: i32) -> i32 {
    state A { return 1; }
}
"#;
    assert!(
        ParseSource(src).is_err(),
        "flow parameter name 'board' should be rejected"
    );
}

#[test]
fn EmitHlslFlowBoardEmitsValueFlowHelper() {
    let module = ParseSource("flow F() -> i32 { board { X: i32; } state A { return 1; } }")
        .expect("flow board source should parse");
    let hlsl = EmitHlsl(&module).expect("flow+board subset should emit");
    assert!(hlsl.contains("int F() {"), "expected flow helper signature");
    assert!(hlsl.contains("int X = 0;"), "expected board local default");
}

#[test]
fn ParserFlowBoardAssignmentParses() {
    let src = r#"
flow SelectShadow(useSoft: bool) -> i32 {
    board { HasSelection: bool; SelectedMode: i32; BlendWeight: f32; }
    state Select {
        board.HasSelection = true;
        board.SelectedMode = 2;
        board.BlendWeight = 0.75 + 0.0;
        when { case board.HasSelection -> goto Done else -> goto Done }
    }
    state Done { return board.SelectedMode; }
}
"#;
    assert!(
        ParseSource(src).is_ok(),
        "flow board assignments should parse"
    );
}

#[test]
fn ParserFlowBoardAssignmentRejectsBadTargets() {
    assert!(
        ParseSource("flow F() -> i32 { state A { board.A.B = 1; return 1; } }").is_err(),
        "nested board assignment target should fail"
    );
    assert!(
        ParseSource("flow F() -> i32 { state A { foo.A = 1; return 1; } }")
            .unwrap_err()
            .iter()
            .any(|d| d
                .Message
                .contains("unsupported statement in flow state body")),
        "non-board assignment should be unsupported"
    );
}

#[test]
fn ValidationFlowBoardAssignmentsAndTypes() {
    let ok = r#"
flow F(mode: i32, enabled: bool) -> i32 {
    board { HasSelection: bool; SelectedMode: i32; BlendWeight: f32; }
    state A {
        board.HasSelection = enabled;
        board.SelectedMode = mode;
        board.BlendWeight = 0.25 + 0.25;
        when { case board.HasSelection -> return board.SelectedMode else -> return 0 }
    }
}
"#;
    assert!(ValidateSource(ok).is_ok(), "valid board writes should pass");

    let bad = r#"
flow F(mode: i32) -> i32 {
    board { HasSelection: bool; SelectedMode: i32; BlendWeight: f32; }
    state A {
        board.Unknown = 1;
        board.HasSelection = 1;
        board.SelectedMode = 0.5;
        board.BlendWeight = true;
        when { case board.SelectedMode -> return board.HasSelection else -> return 0 }
    }
}
"#;
    let d = ValidateSource(bad).unwrap_err();
    assert!(
        d.iter()
            .any(|x| x.Message.contains("unknown board field 'Unknown'"))
    );
    assert!(d.iter().any(|x| {
        x.Message
            .contains("board assignment type mismatch: expected bool, found i32")
    }));
    assert!(d.iter().any(|x| {
        x.Message
            .contains("board assignment type mismatch: expected i32, found float")
    }));
    assert!(d.iter().any(|x| {
        x.Message
            .contains("board assignment type mismatch: expected f32, found bool")
    }));
    assert!(d.iter().any(|x| {
        x.Message
            .contains("guard condition type mismatch in flow 'F': expected bool, found i32")
    }));
    assert!(d.iter().any(|x| {
        x.Message
            .contains("return type mismatch in flow 'F': expected i32, found bool")
    }));
}

#[test]
fn EmitHlslFlowCycleAndNonReturningAndUnsupportedReturnAreDiagnosed() {
    let cycle = include_str!("../../../../examples/sdslv/flow_cycle_invalid.sdslv");
    let cycle_diag = CompileSourceToHlsl(cycle).unwrap_err();
    assert!(
        cycle_diag
            .iter()
            .any(|d| d.Message.contains("contains a state cycle")),
        "expected cycle diagnostic"
    );

    let non_returning = r#"
flow BadFlow() -> i32 {
    board { X: i32; }
    state A { goto B; }
    state B { board.X = 1; }
}
"#;
    let non_returning_diag = CompileSourceToHlsl(non_returning).unwrap_err();
    assert!(
        non_returning_diag
            .iter()
            .any(|d| d.Message.contains("non-returning path")),
        "expected non-returning diagnostic"
    );

    let unknown_return = r#"
flow SelectShadow() -> MissingType { state A { return 1; } }
"#;
    let unknown_diag = CompileSourceToHlsl(unknown_return).unwrap_err();
    assert!(
        !unknown_diag.is_empty(),
        "expected unsupported return diagnostic"
    );
}

#[test]
fn ValidationFallibilityHandlingRules() {
    let ok = r#"shader S { fn F() -> i32 ! Error { return 1; } fn G() -> i32 ! Error { let x: i32 = F()?; return x; } fn H() -> i32 { let y: i32 = F()!; return y; } }"#;
    assert!(
        ValidateSource(ok).is_ok(),
        "handled fallible expressions should validate"
    );

    let bad_q_infallible =
        ValidateSource(r#"shader S { fn F() -> i32 ! Error { return 1; } fn G() -> i32 { let x: i32 = F()?; return x; } }"#)
            .expect_err("? inside infallible function should fail");
    assert!(
        bad_q_infallible.iter().any(|d| d
            .Message
            .contains("? can only be used inside a fallible function")),
        "expected infallible-context ? diagnostic"
    );

    let bad_q_non_fallible =
        ValidateSource(r#"shader S { fn F() -> i32 { return 1; } fn G() -> i32 ! Error { let x: i32 = F()?; return x; } }"#)
            .expect_err("? on infallible expression should fail");
    assert!(
        bad_q_non_fallible
            .iter()
            .any(|d| d.Message.contains("? requires a fallible expression")),
        "expected ? requires fallible expression diagnostic"
    );

    let bad_unwrap =
        ValidateSource(r#"shader S { fn F() -> i32 { return 1; } fn G() -> i32 { let x: i32 = F()!; return x; } }"#)
            .expect_err("! on infallible expression should fail");
    assert!(
        bad_unwrap.iter().any(|d| d
            .Message
            .contains("! unwrap requires a fallible expression")),
        "expected unwrap diagnostic"
    );
}

#[test]
fn ValidationRejectsUnhandledFallibleExpressions() {
    let src = r#"shader S { fn F() -> i32 ! Error { return 1; } fn G() -> i32 ! Error { F(); let x: i32 = F(); return F(); } }"#;
    let diagnostics = ValidateSource(src).expect_err("unhandled fallible usages should fail");
    let count = diagnostics
        .iter()
        .filter(|d| {
            d.Message
                .contains("fallible expression must be handled with ? or !")
        })
        .count();
    assert!(
        count >= 3,
        "expected unhandled fallible diagnostics for expression statement, let initializer, and return expression"
    );
}

#[test]
fn ValidationErrorCallPositionRules() {
    assert!(
        ValidateSource(r#"shader S { fn F() -> i32 ! Error { return error("bad"); } }"#).is_ok(),
        "error(...) return should pass in fallible function"
    );
    let infallible = ValidateSource(r#"shader S { fn F() -> i32 { return error("bad"); } }"#)
        .expect_err("infallible return error(...) should fail");
    assert!(
        infallible.iter().any(|d| d
            .Message
            .contains("error(...) can only be returned from a fallible function in SDSL-V M58")),
        "expected fallible return-position diagnostic"
    );

    let non_return = ValidateSource(
        r#"shader S { fn F() -> i32 ! Error { let e: Error = error("bad"); return 0; } }"#,
    )
    .expect_err("non-return error(...) should fail");
    assert!(
        non_return.iter().any(|d| d
            .Message
            .contains("error(...) is only valid in fallible return position in SDSL-V M58")),
        "expected return-position-only diagnostic"
    );
}

#[test]
fn EmitHlslRejectsFallibleModulesWithClearDiagnostic() {
    let fallible_src = r#"shader S { fn F() -> i32 ! Error { return 1; } stage pixel fn PS() -> float4 { return float4(1.0, 0.0, 0.0, 1.0); } }"#;
    let diagnostics = CompileSourceToHlsl(fallible_src)
        .expect_err("fallible modules should be unsupported for HLSL emission");
    assert!(
        diagnostics.iter().any(|d| d
            .Message
            .contains("fallible function emission is not implemented in SDSL-V M58")),
        "expected clear unsupported fallible emission diagnostic"
    );
}

#[test]
fn DxcBuildCommandIncludesEntryProfileSpirvAndExtraArgs() {
    let request = DxcCompileRequest {
        SourceName: "flat_color.sdslv".to_string(),
        Hlsl: "float4 FlatColor_VS() : SV_Position { return 0; }".to_string(),
        EntryPoint: "FlatColor_VS".to_string(),
        TargetProfile: "vs_6_0".to_string(),
    };
    let options = DxcOptions {
        DxcPath: "dxc".to_string(),
        OutputSpirv: true,
        ExtraArgs: vec!["-O3".to_string(), "-Ges".to_string()],
    };
    let args = BuildDxcCommand(&request, &options);

    assert_eq!(
        args,
        vec![
            "-E",
            "FlatColor_VS",
            "-T",
            "vs_6_0",
            "-spirv",
            "-O3",
            "-Ges"
        ],
        "DXC command args should remain deterministic with spirv + extra args"
    );
}

#[test]
fn DxcCompileRequestFromArtifactEntryPreservesHlslAndProfiles() {
    let src = r#"
        type ClipPosition4 = float4 @space(clip.position);
        stream VertexOut { Position: ClipPosition4; Color: float4; }
        shader FlatColor {
            stage vertex fn VS(pos: float3, color: float4) -> VertexOut {
                let output: VertexOut;
                output.Position = float4(pos, 1.0);
                output.Color = color;
                return output;
            }
            stage pixel fn PS(input: VertexOut) -> float4 {
                return input.Color;
            }
        }
    "#;
    let artifact = CompileSourceToShaderArtifact("flat_color.sdslv", src)
        .expect("flat color artifact should compile for request mapping");
    let vertex = artifact
        .EntryPoints
        .iter()
        .find(|x| x.TargetProfile == "vs_6_0")
        .expect("expected vertex entry");
    let pixel = artifact
        .EntryPoints
        .iter()
        .find(|x| x.TargetProfile == "ps_6_0")
        .expect("expected pixel entry");

    let vertex_request = DxcCompileRequest::FromArtifactEntry(&artifact, vertex);
    let pixel_request = DxcCompileRequest::FromArtifactEntry(&artifact, pixel);

    assert_eq!(
        vertex_request.EntryPoint, "FlatColor_VS",
        "vertex entry name should map to request"
    );
    assert_eq!(
        vertex_request.TargetProfile, "vs_6_0",
        "vertex profile should map to request"
    );
    assert_eq!(
        pixel_request.EntryPoint, "FlatColor_PS",
        "pixel entry name should map to request"
    );
    assert_eq!(
        pixel_request.TargetProfile, "ps_6_0",
        "pixel profile should map to request"
    );
    assert_eq!(
        vertex_request.Hlsl, artifact.Hlsl,
        "artifact HLSL should be copied into request"
    );
}

#[test]
fn DxcCompileRequestFromArtifactEntryNameErrorsForUnknownEntry() {
    let src = include_str!("../../../../examples/sdslv/flat_color.sdslv");
    let artifact = CompileSourceToShaderArtifact("flat_color.sdslv", src)
        .expect("flat color artifact should compile for entry lookup");
    let error = DxcCompileRequest::FromArtifactEntryName(&artifact, "Missing_Entry").unwrap_err();

    match error {
        DxcError::EntryPointNotFound {
            EntryPoint,
            SourceName,
        } => {
            assert_eq!(
                EntryPoint, "Missing_Entry",
                "missing entry should be preserved in error"
            );
            assert_eq!(
                SourceName, "flat_color.sdslv",
                "source name should be preserved in lookup error"
            );
        }
        _ => panic!("expected EntryPointNotFound error for unknown entry"),
    }
}

#[test]
fn CompileHlslWithDxcReturnsToolUnavailableForMissingPath() {
    let request = DxcCompileRequest {
        SourceName: "flat_color.sdslv".to_string(),
        Hlsl: "float4 FlatColor_PS() : SV_Target0 { return float4(1,1,1,1); }".to_string(),
        EntryPoint: "FlatColor_PS".to_string(),
        TargetProfile: "ps_6_0".to_string(),
    };
    let options = DxcOptions {
        DxcPath: "__definitely_missing_dxc_binary__".to_string(),
        OutputSpirv: true,
        ExtraArgs: Vec::new(),
    };

    let error = CompileHlslWithDxc(&request, &options).unwrap_err();
    match error {
        DxcError::ToolUnavailable { Path } => {
            assert_eq!(
                Path, "__definitely_missing_dxc_binary__",
                "unavailable path should be surfaced"
            );
        }
        _ => panic!("expected ToolUnavailable error when DXC path does not exist"),
    }
}

#[test]
#[ignore = "requires local DXC and explicit opt-in"]
fn CompileHlslWithDxcIntegrationWhenEnabled() {
    if std::env::var("WYRMCOIL_RUN_DXC_TESTS").ok().as_deref() != Some("1") {
        return;
    }

    let options = DxcOptions::default();
    if !FindDxc(&options) {
        return;
    }

    let src = include_str!("../../../../examples/sdslv/flat_color.sdslv");
    let artifact = CompileSourceToShaderArtifact("flat_color.sdslv", src)
        .expect("flat color artifact should compile before DXC invocation");
    let vertex = artifact
        .EntryPoints
        .iter()
        .find(|x| x.TargetProfile == "vs_6_0")
        .expect("expected vertex entry for DXC integration probe");

    let result = CompileArtifactEntryWithDxc(&artifact, vertex, &options)
        .expect("DXC invocation should compile when tool is available and opt-in is enabled");

    assert!(result.Success, "DXC result should mark success");
    assert!(
        !result.OutputBytes.is_empty(),
        "DXC should produce output bytes"
    );
}

#[test]
fn ValidationIfAndSwitchRulesM55c() {
    let valid_if = r#"
shader S {
    fn F(weight: i32, outer: bool, inner: bool) -> i32 {
        if outer { if inner { return 1; } }
        if weight < 5 { return 2; } else { return 3; }
    }
}
"#;
    ValidateSource(valid_if).expect("bool if conditions and non-ladder nested if should validate");

    let invalid_if = ValidateSource(
        "shader S { fn F(weight: i32) -> i32 { if weight { return 1; } return 0; } }",
    )
    .expect_err("non-bool if condition should fail");
    assert!(
        invalid_if
            .iter()
            .any(|d| d.Message.contains("if condition must be bool; found i32")),
        "expected non-bool if condition diagnostic"
    );

    let ladder = ValidateSource("shader S { fn F(weight: i32) -> i32 { if weight < 1 { return 1; } else { if weight < 5 { return 2; } else { return 3; } } } }").expect_err("else-if ladder shape should fail");
    assert!(
        ladder.iter().any(|d| d
            .Message
            .contains("nested decision ladder is not allowed; use switch { case ... else ... }")),
        "expected nested ladder diagnostic"
    );

    let switch_ok = ValidateSource("shader S { fn F(weight: i32) -> i32 { return switch { case weight < 1 => 1 case weight < 5 => 2 else => 3 }; } }").expect("switch with bool cases and matching arm types should validate");
    let _ = switch_ok;

    let switch_bad_condition = ValidateSource(
        "shader S { fn F(weight: i32) -> i32 { return switch { case weight => 1 else => 2 }; } }",
    )
    .expect_err("non-bool switch case condition should fail");
    assert!(
        switch_bad_condition.iter().any(|d| d
            .Message
            .contains("switch case condition must be bool; found i32")),
        "expected switch bool-condition diagnostic"
    );

    let switch_bad_type = ValidateSource("shader S { fn F(weight: i32) -> i32 { return switch { case weight < 1 => 1 else => float4(1.0, 0.0, 0.0, 1.0) }; } }").expect_err("switch arm type mismatch should fail");
    assert!(
        switch_bad_type
            .iter()
            .any(|d| d.Message.contains("switch arm type mismatch:")
                || d.Message.contains("return type mismatch in F")),
        "expected switch arm mismatch diagnostic"
    );

    ValidateSource("shader S { fn F(code: i32) -> i32 { return switch code { case 408 => 3 case 429 => 5 else => 0 }; } }")
        .expect("subject switch with matching i32 subject/cases should validate");

    let switch_case_mismatch = ValidateSource(
        "shader S { fn F(code: i32) -> i32 { return switch code { case true => 3 else => 0 }; } }",
    )
    .expect_err("subject switch case type mismatch should fail");
    assert!(
        switch_case_mismatch.iter().any(|d| d
            .Message
            .contains("switch case type mismatch: expected i32, found bool")),
        "expected subject switch case type mismatch diagnostic"
    );
}

#[test]
fn EmitHlslIfAndSwitchLoweringM55c() {
    let src = r#"
shader S {
    fn F(weight: i32, alpha: float, inner: bool) -> i32 {
        let tier: i32 = switch { case weight < 1 => 1 case weight < 5 => 2 else => 3 };
        tier = switch { case weight < 2 => 4 case weight < 4 => 5 else => 6 };
        if alpha < 0.5 {
            return switch { case inner => 7 else => 8 };
        } else {
            return tier;
        }
    }
}
"#;
    let hlsl = CompileSourceToHlsl(src).expect("if + switch lowering should compile to HLSL");
    assert!(
        hlsl.contains("int tier;"),
        "local switch init should lower to declaration"
    );
    assert!(
        hlsl.contains("if (weight < 1) {"),
        "switch lowering should start with if"
    );
    assert!(
        hlsl.contains("else if (weight < 5) {"),
        "switch lowering should include else-if"
    );
    assert!(
        hlsl.contains("tier = 1;"),
        "switch init should assign arm values"
    );
    assert!(
        hlsl.contains("tier = 4;"),
        "switch assignment RHS should lower to assignments"
    );
    assert!(
        hlsl.contains("if (alpha < 0.5) {"),
        "if statement should lower"
    );
    assert!(
        hlsl.contains("return 7;"),
        "return switch arm should lower to return"
    );
    assert!(
        !hlsl.contains("not implemented"),
        "supported control flow must not emit placeholders"
    );

    let again = CompileSourceToHlsl(src).expect("second compile should also succeed");
    assert_eq!(hlsl, again, "emission should be deterministic");
}

#[test]
fn EmitHlslSubjectSwitchLoweringM56() {
    let src = r#"
shader S {
    fn F(code: i32) -> i32 {
        let retries: i32 = switch code { case 408 => 3 case 429 => 5 else => 0 };
        retries = switch code { case 500 => 9 else => retries };
        return switch code { case 200 => retries case 204 => 1 else => 2 };
    }
}
"#;
    let hlsl = CompileSourceToHlsl(src).expect("subject switch lowering should compile to HLSL");
    assert!(
        hlsl.contains("if (code == 408) {"),
        "subject-switch init should compare subject against case"
    );
    assert!(
        hlsl.contains("else if (code == 429) {"),
        "subject-switch init should emit else-if comparison"
    );
    assert!(
        hlsl.contains("retries = 3;"),
        "subject-switch init arm should assign result"
    );
    assert!(
        hlsl.contains("if (code == 500) {"),
        "subject-switch assignment RHS should compare subject"
    );
    assert!(
        hlsl.contains("if (code == 200) {"),
        "subject-switch return should compare subject"
    );
    assert!(
        !hlsl.contains("if (code) {"),
        "subject-switch lowering should not treat case values as boolean conditions"
    );
}

#[test]
fn ValidationForLoopsEnforceIntegerBoundsAndStepRules() {
    ValidateSource("shader S { fn F(limit: i32) -> i32 { let sum: i32 = 0; for i in 0..limit { sum = sum + i; } return sum; } }")
        .expect("integer-bounded for loop should validate");
    let float_start = ValidateSource("shader S { fn F(limit: i32) -> i32 { let sum: i32 = 0; for i in 1.5..limit { sum = sum + i; } return sum; } }").expect_err("float start bound should fail");
    assert!(
        float_start.iter().any(|d| d
            .Message
            .contains("for loop start bound must be integer; found float")),
        "float start should report integer-bound diagnostic"
    );
    let bool_end = ValidateSource("shader S { fn F(flag: bool) -> i32 { let sum: i32 = 0; for i in 0..flag { sum = sum + i; } return sum; } }").expect_err("bool end bound should fail");
    assert!(
        bool_end.iter().any(|d| d
            .Message
            .contains("for loop end bound must be integer; found bool")),
        "bool end should report integer-bound diagnostic"
    );
    let float_step = ValidateSource("shader S { fn F(limit: i32) -> i32 { let sum: i32 = 0; for i in 0..limit step 1.5 { sum = sum + i; } return sum; } }").expect_err("float step should fail");
    assert!(
        float_step.iter().any(|d| d
            .Message
            .contains("for loop step must be integer; found float")),
        "float step should report integer diagnostic"
    );
    let zero_step = ValidateSource("shader S { fn F(limit: i32) -> i32 { let sum: i32 = 0; for i in 0..limit step 0 { sum = sum + i; } return sum; } }").expect_err("zero step should fail");
    assert!(
        zero_step.iter().any(|d| d
            .Message
            .contains("for loop step must be greater than zero")),
        "non-positive step should report positivity diagnostic"
    );
}

#[test]
fn ForLoopEmissionIsDeterministicAndStructured() {
    let src = "shader S { fn F(limit: i32) -> i32 { let sum: i32 = 0; for i in 0..limit step 2 { sum = sum + i; } return sum; } }";
    let hlsl_a = CompileSourceToHlsl(src).expect("for loop should lower to hlsl");
    let hlsl_b = CompileSourceToHlsl(src).expect("for loop emission should be deterministic");
    assert!(
        hlsl_a.contains("for (int i = 0; i < limit; i = i + 2) {"),
        "expected explicit bounded for-loop lowering"
    );
    assert!(
        hlsl_a.contains("sum = sum + i;"),
        "loop body should be emitted"
    );
    assert_eq!(hlsl_a, hlsl_b, "repeated emissions must match exactly");
}

#[test]
fn ParseArrayTypeRefM59b() {
    ParseSource("shader S { fn F() -> i32 { let weights: array<f32, 4>; return 0; } }")
        .expect("array type refs should parse in M59b");
}

#[test]
fn ValidateArrayIndexingM59b() {
    ValidateSource("shader S { fn F(i: i32) -> f32 { let weights: array<f32, 4>; let x: f32 = weights[i]; return x; } }")
        .expect("array indexing should validate");
}

#[test]
fn ValidateArrayIndexRejectsNonArrayBase() {
    let diagnostics = ValidateSource(
        "shader S { fn F() -> f32 { let color: float4 = float4(1.0, 0.0, 1.0, 1.0); return color[0]; } }",
    )
    .expect_err("vector indexing should be rejected");
    assert!(
        diagnostics
            .iter()
            .any(|d| d.Message.contains("indexing requires array type")),
        "expected non-array indexing diagnostic"
    );
}

#[test]
fn ValidateArrayElementAssignmentM60() {
    ValidateSource("shader S { fn F(i: i32) -> f32 { let weights: array<f32, 4>; weights[i] = 1.0; return weights[i]; } }")
        .expect("array element assignment should validate");
}

#[test]
fn ValidateArrayElementAssignmentRejectsNonIntegerIndexM60() {
    let diagnostics =
        ValidateSource("shader S { fn F() -> f32 { let weights: array<f32, 4>; weights[0.5] = 1.0; return 0.0; } }")
            .expect_err("non-integer array index assignment should be rejected");
    assert!(
        diagnostics.iter().any(|d| d
            .Message
            .contains("array index must be integer; found float")),
        "expected non-integer index assignment diagnostic"
    );
}

#[test]
fn ValidateArrayElementAssignmentRejectsTypeMismatchM60() {
    let diagnostics = ValidateSource(
        "shader S { fn F(i: i32) -> f32 { let weights: array<f32, 4>; weights[i] = true; return 0.0; } }",
    )
    .expect_err("array element assignment type mismatch should be rejected");
    assert!(
        diagnostics
            .iter()
            .any(|d| d.Message.contains("array element assignment type mismatch")),
        "expected array element assignment mismatch diagnostic"
    );
}

#[test]
fn ValidateArrayParameterElementAssignmentRejectedM60() {
    let diagnostics = ValidateSource(
        "shader S { fn Fill(weights: array<f32, 4>) -> f32 { weights[0] = 1.0; return weights[0]; } }",
    )
    .expect_err("array parameter element mutation should be rejected");
    assert!(
        diagnostics.iter().any(|d| d
            .Message
            .contains("cannot assign to element of immutable array parameter 'weights'")),
        "expected immutable array parameter assignment diagnostic"
    );
}

#[test]
fn EmitArrayElementAssignmentM60() {
    let hlsl = CompileSourceToHlsl(
        "shader S { fn F() -> f32 { let weights: array<f32, 4>; weights[0] = 1.0; return weights[0]; } }",
    )
    .expect("array element assignment should lower to hlsl");
    assert!(
        hlsl.contains("weights[0] = 1.0;"),
        "expected array element assignment lowering"
    );
}

#[test]
fn EmitArrayElementAssignmentInForLoopM60Deterministic() {
    let src = "shader S { fn F() -> f32 { let weights: array<f32, 4>; for i in 0..4 { weights[i] = 1.0; } return weights[0]; } }";
    let hlsl_a = CompileSourceToHlsl(src).expect("array assignment in for-loop should lower");
    let hlsl_b =
        CompileSourceToHlsl(src).expect("array assignment lowering should be deterministic");
    assert!(
        hlsl_a.contains("for (int i = 0; i < 4; i = i + 1) {"),
        "expected deterministic for-loop lowering"
    );
    assert!(
        hlsl_a.contains("weights[i] = 1.0;"),
        "expected array element assignment in loop body"
    );
    assert_eq!(hlsl_a, hlsl_b, "repeated emission should match exactly");
}

#[test]
fn ParseArrayLiteralM61() {
    let module = ParseSource(
        "shader S { fn F() -> i32 { let weights: array<f32, 3> = [1.0, 2.0, 3.0,]; return 0; } }",
    )
    .expect("array literal should parse");
    let shader = module
        .Declarations
        .iter()
        .find_map(|d| match d {
            SdslvDecl::Shader(s) => Some(s),
            _ => None,
        })
        .expect("shader expected");
    let body = shader.Methods[0].Body.as_ref().expect("body expected");
    let SdslvStatement::Let {
        Initializer: Some(SdslvExpression::ArrayLiteral { Elements, .. }),
        ..
    } = &body.Statements[0]
    else {
        panic!("typed let initializer should parse as array literal");
    };
    assert_eq!(Elements.len(), 3, "array literal should keep all elements");
}

#[test]
fn ValidateArrayLiteralTypedLocalM61() {
    ValidateSource("shader S { fn F() -> f32 { let weights: array<f32, 4> = [1.0, 2.0, 3.0, 4.0]; return weights[0]; } }")
        .expect("typed local array literal should validate");
}

#[test]
fn ValidateArrayLiteralRejectsNonArrayTargetM61() {
    let diagnostics = ValidateSource(
        "shader S { fn F() -> float4 { let color: float4 = [1.0, 0.0, 1.0, 1.0]; return color; } }",
    )
    .expect_err("array literal to vector should fail");
    assert!(
        diagnostics.iter().any(|d| d
            .Message
            .contains("array literal cannot initialize non-array type")),
        "expected vector/matrix confusion diagnostic"
    );
}

#[test]
fn ValidateArrayLiteralLengthMismatchM61() {
    let diagnostics = ValidateSource(
        "shader S { fn F() -> f32 { let weights: array<f32, 4> = [1.0, 2.0]; return 0.0; } }",
    )
    .expect_err("array literal length mismatch should fail");
    assert!(
        diagnostics.iter().any(|d| d
            .Message
            .contains("array literal length mismatch: expected 4 elements, found 2")),
        "expected array literal length mismatch diagnostic"
    );
}

#[test]
fn ValidateArrayLiteralElementTypeMismatchM61() {
    let diagnostics = ValidateSource(
        "shader S { fn F() -> f32 { let weights: array<f32, 2> = [1.0, true]; return 0.0; } }",
    )
    .expect_err("array literal element mismatch should fail");
    assert!(
        diagnostics.iter().any(|d| d
            .Message
            .contains("array literal element type mismatch: expected f32, found bool")),
        "expected array literal element type mismatch diagnostic"
    );
}

#[test]
fn EmitArrayLiteralLocalInitializerM61() {
    let src = "shader S { fn F() -> f32 { let weights: array<f32, 4> = [1.0, 2.0, 3.0, 4.0]; let sum: f32 = 0.0; for i in 0..4 { sum = sum + weights[i]; } return sum; } }";
    let hlsl_a = CompileSourceToHlsl(src).expect("array literal should lower");
    let hlsl_b = CompileSourceToHlsl(src).expect("array literal emission should be deterministic");
    assert!(
        hlsl_a.contains("weights[0] = 1.0;"),
        "array local initializer should lower to element assignments"
    );
    assert!(
        hlsl_a.contains("weights[0] = 1.0;"),
        "array element 0 init should lower"
    );
    assert!(
        hlsl_a.contains("weights[3] = 4.0;"),
        "array element 3 init should lower"
    );
    assert_eq!(
        hlsl_a, hlsl_b,
        "array literal lowering should be deterministic"
    );
}

#[test]
fn ValidateVectorConstructorsM62() {
    ValidateSource("shader S { fn F() -> float2 { return float2(0.0, 1.0); } }")
        .expect("float2 constructor with two scalar numeric arguments should validate");
    ValidateSource("shader S { fn F() -> float3 { return float3(0.0, 1.0, 0.0); } }")
        .expect("float3 constructor with three scalar numeric arguments should validate");
    ValidateSource("shader S { fn F() -> float4 { return float4(1.0, 0.0, 1.0, 1.0); } }")
        .expect("float4 constructor with four scalar numeric arguments should validate");
}

#[test]
fn ValidateVectorConstructorRejectsWrongArityM62() {
    let diagnostics = ValidateSource("shader S { fn F() -> float3 { return float3(1.0, 2.0); } }")
        .expect_err("float3 constructor wrong arity should fail");
    assert!(
        diagnostics.iter().any(|d| d
            .Message
            .contains("float3 constructor expects 3 scalar numeric arguments; found 2")),
        "expected constructor arity diagnostic"
    );
}

#[test]
fn ValidateVectorConstructorRejectsNonNumericScalarArgumentM62() {
    let diagnostics =
        ValidateSource("shader S { fn F() -> float3 { return float3(true, 0.0, 1.0); } }")
            .expect_err("float3 constructor bool argument should fail");
    assert!(
        diagnostics.iter().any(|d| d
            .Message
            .contains("float3 constructor argument 0 must be numeric scalar; found bool")),
        "expected constructor numeric scalar diagnostic"
    );
}

#[test]
fn ValidateMatrixConstructorArityM62() {
    ValidateSource("shader S { fn F() -> float4x4 { return float4x4(1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0); } }")
        .expect("float4x4 constructor with sixteen scalar numeric arguments should validate");
    let diagnostics = ValidateSource("shader S { fn F() -> float4x4 { return float4x4(1.0); } }")
        .expect_err("float4x4 constructor wrong arity should fail");
    assert!(
        diagnostics.iter().any(|d| d
            .Message
            .contains("float4x4 constructor expects 16 scalar numeric arguments; found 1")),
        "expected matrix constructor arity diagnostic"
    );
}

#[test]
fn ValidateArrayLiteralRejectsFloat3TargetWithConstructorGuidanceM62() {
    let diagnostics = ValidateSource(
        "shader S { fn F() -> float3 { let v: float3 = [1.0, 2.0, 3.0]; return v; } }",
    )
    .expect_err("array literal should not initialize float3");
    assert!(
        diagnostics.iter().any(|d| d.Message.contains("array literal cannot initialize non-array type float3; use floatN(...) or float4x4(...) constructors for vector/matrix values")),
        "expected array-vs-vector constructor guidance diagnostic"
    );
}

#[test]
fn ValidateArrayOfFloat4InitializerM62() {
    ValidateSource("shader S { fn F() -> float4 { let colors: array<float4, 2> = [float4(1.0, 0.0, 1.0, 1.0), float4(0.0, 1.0, 0.0, 1.0)]; return colors[1]; } }")
        .expect("array<float4,2> with float4 constructors should validate");
}

#[test]
fn EmitVectorConstructorCallsRemainUnchangedM62() {
    let hlsl =
        CompileSourceToHlsl("shader S { fn F() -> float4 { return float4(1.0, 0.0, 1.0, 1.0); } }")
            .expect("float4 constructor should lower");
    assert!(
        hlsl.contains("return float4(1.0, 0.0, 1.0, 1.0);"),
        "expected constructor call emission to remain unchanged"
    );
}

#[test]
fn ValidationM63FallibilityIntegrationAcrossArraysConstructorsWithAndControlFlow() {
    let unhandled_array_literal = ValidateSource("shader S { fn F() -> i32 ! Error { return 1; } fn G() -> i32 ! Error { let xs: array<i32, 2> = [0, F()]; return xs[0]; } }")
        .expect_err("unhandled fallible array literal element should fail");
    assert!(
        unhandled_array_literal.iter().any(|d| d
            .Message
            .contains("fallible expression must be handled with ? or !")),
        "expected unhandled fallible diagnostic for array literal element"
    );
    ValidateSource("shader S { fn F() -> i32 ! Error { return 1; } fn G() -> i32 ! Error { let xs: array<i32, 2> = [0, F()?]; return xs[F()?]; } }")
        .expect("handled fallible array literal/index should validate");

    let unhandled_assignment_index = ValidateSource("shader S { fn F() -> i32 ! Error { return 1; } fn G(values: array<i32, 2>) -> i32 ! Error { values[F()] = 1; return values[0]; } }")
        .expect_err("unhandled fallible array assignment index should fail");
    assert!(
        unhandled_assignment_index.iter().any(|d| d
            .Message
            .contains("fallible expression must be handled with ? or !")),
        "expected unhandled fallible diagnostic for assignment index"
    );

    let unhandled_assignment_rhs = ValidateSource("shader S { fn F() -> i32 ! Error { return 1; } fn G() -> i32 ! Error { let values: array<i32, 2>; values[0] = F(); return values[0]; } }")
        .expect_err("unhandled fallible array assignment rhs should fail");
    assert!(
        unhandled_assignment_rhs.iter().any(|d| d
            .Message
            .contains("fallible expression must be handled with ? or !")),
        "expected unhandled fallible diagnostic for assignment rhs"
    );
    ValidateSource("shader S { fn F() -> i32 ! Error { return 1; } fn G() -> i32 ! Error { let values: array<i32, 2>; values[F()?] = 1; values[0] = F()?; return values[0]; } }")
        .expect("handled fallible array assignment index/rhs should validate");

    let unhandled_ctor_arg = ValidateSource("shader S { fn F() -> float ! Error { return 1.0; } fn G() -> float4 ! Error { return float4(1.0, F(), 0.0, 1.0); } }")
        .expect_err("unhandled fallible constructor arg should fail");
    assert!(
        unhandled_ctor_arg.iter().any(|d| d
            .Message
            .contains("fallible expression must be handled with ? or !")),
        "expected unhandled fallible diagnostic for constructor arg"
    );
    ValidateSource("shader S { fn F() -> float ! Error { return 1.0; } fn G() -> float4 ! Error { return float4(1.0, F()?, 0.0, 1.0); } fn H() -> float4 { return float4(1.0, F()!, 0.0, 1.0); } }")
        .expect("handled fallible constructor args should validate with success type");

    let unhandled_with_value = ValidateSource("record SurfaceData { Roughness: float; } shader S { fn LoadRoughness() -> float ! Error { return 0.5; } fn G(surface: SurfaceData) -> SurfaceData ! Error { return surface with { Roughness: LoadRoughness(), }; } }")
        .expect_err("unhandled fallible with update should fail");
    assert!(
        unhandled_with_value.iter().any(|d| d
            .Message
            .contains("fallible expression must be handled with ? or !")),
        "expected unhandled fallible diagnostic for with update value"
    );
    ValidateSource("record SurfaceData { Roughness: float; } shader S { fn LoadRoughness() -> float ! Error { return 0.5; } fn G(surface: SurfaceData) -> SurfaceData ! Error { return surface with { Roughness: LoadRoughness()?, }; } }")
        .expect("handled fallible with update value should validate");

    let unhandled_if_condition = ValidateSource("shader S { fn IsEnabled() -> bool ! Error { return true; } fn G() -> i32 ! Error { if IsEnabled() { return 1; } return 0; } }")
        .expect_err("unhandled fallible if condition should fail");
    assert!(
        unhandled_if_condition.iter().any(|d| d
            .Message
            .contains("fallible expression must be handled with ? or !")),
        "expected unhandled fallible diagnostic for if condition"
    );
    ValidateSource("shader S { fn IsEnabled() -> bool ! Error { return true; } fn G() -> i32 ! Error { if IsEnabled()? { return 1; } return 0; } }")
        .expect("handled fallible if condition should validate");

    let unhandled_switch_and_for = ValidateSource("shader S { fn IsLow() -> bool ! Error { return true; } fn LoadCode() -> i32 ! Error { return 408; } fn Step() -> i32 ! Error { return 1; } fn F() -> i32 ! Error { let code: i32 = LoadCode()?; let a: i32 = switch { case IsLow() => 1 else => 2 }; let b: i32 = switch LoadCode() { case 408 => 1 else => 0 }; let c: i32 = switch code { case 1 => LoadCode() else => 0 }; let sum: i32 = 0; for i in 0..LoadCode() { sum = sum + i; } for j in 0..10 step Step() { sum = sum + j; } return a + b + c + sum; } }")
        .expect_err("unhandled fallible switch/for expressions should fail");
    assert!(
        unhandled_switch_and_for.iter().any(|d| d
            .Message
            .contains("fallible expression must be handled with ? or !")),
        "expected unhandled fallible diagnostic across switch/for contexts"
    );

    ValidateSource("shader S { fn IsLow() -> bool ! Error { return true; } fn LoadCode() -> i32 ! Error { return 408; } fn Step() -> i32 ! Error { return 1; } fn F() -> i32 ! Error { let code: i32 = LoadCode()?; let a: i32 = switch { case IsLow()? => 1 else => 2 }; let b: i32 = switch LoadCode()? { case 408 => 1 else => 0 }; let c: i32 = switch code { case 1 => LoadCode()? else => 0 }; let sum: i32 = 0; for i in 0..LoadCode()? { sum = sum + i; } for j in 0..10 step Step()? { sum = sum + j; } return a + b + c + sum; } }")
        .expect("handled fallible switch/for expressions should validate");
}

#[test]
fn ValidationM64bEnumTypeAndVariantResolution() {
    ValidateSource("enum ShadowMode { None; Hard; Soft; } shader S { fn Quality(mode: ShadowMode) -> ShadowMode { let local: ShadowMode = ShadowMode.Hard; return local; } }")
        .expect("enum type should validate in parameter/local/return");

    let unknown_type =
        ValidateSource("shader S { fn Quality(mode: MissingMode) -> i32 { return 1; } }")
            .expect_err("unknown enum type should be rejected");
    assert!(
        unknown_type.iter().any(|d| d
            .Message
            .contains("unknown type 'MissingMode' in parameter 'mode' of Quality")),
        "expected unknown parameter type diagnostic"
    );

    let unknown_enum = ValidateSource(
        "enum ShadowMode { None; Hard; Soft; } shader S { fn Quality() -> ShadowMode { return MissingMode.Hard; } }",
    )
    .expect_err("unknown enum in variant expression should be rejected");
    assert!(
        unknown_enum
            .iter()
            .any(|d| d.Message.contains("unknown enum 'MissingMode'")),
        "expected unknown enum diagnostic"
    );

    let unknown_variant = ValidateSource(
        "enum ShadowMode { None; Hard; Soft; } shader S { fn Quality() -> ShadowMode { return ShadowMode.Ultra; } }",
    )
    .expect_err("unknown variant should be rejected");
    assert!(
        unknown_variant.iter().any(|d| d
            .Message
            .contains("unknown variant 'Ultra' for enum 'ShadowMode'")),
        "expected unknown variant diagnostic"
    );
}

#[test]
fn ValidationM64bMatchSemanticsAndFallibility() {
    ValidateSource("enum ShadowMode { None; Hard; Soft; } shader S { fn Quality(mode: ShadowMode) -> i32 { let q: i32 = match mode { ShadowMode.None => 0 ShadowMode.Hard => 1 ShadowMode.Soft => 8 }; return match mode { ShadowMode.None => q ShadowMode.Hard => 1 ShadowMode.Soft => 8 }; } }")
        .expect("exhaustive enum match should validate and resolve i32 type");

    let non_enum_subject = ValidateSource("enum ShadowMode { None; Hard; Soft; } shader S { fn Quality(code: i32) -> i32 { return match code { ShadowMode.None => 0 ShadowMode.Hard => 1 ShadowMode.Soft => 8 }; } }")
        .expect_err("match subject must be enum");
    assert!(
        non_enum_subject.iter().any(|d| d
            .Message
            .contains("match subject must be enum type; found i32")),
        "expected non-enum subject diagnostic"
    );

    let wrong_enum_duplicate_missing = ValidateSource("enum ShadowMode { None; Hard; Soft; } enum QualityMode { Low; High; } shader S { fn Quality(mode: ShadowMode) -> i32 { return match mode { QualityMode.Low => 0 ShadowMode.Hard => 1 ShadowMode.Hard => 2 }; } }")
        .expect_err("wrong enum arm + duplicate + missing should fail");
    assert!(
        wrong_enum_duplicate_missing
            .iter()
            .any(|d| d.Message.contains("does not belong to enum 'ShadowMode'")),
        "expected wrong-enum arm diagnostic"
    );
    assert!(
        wrong_enum_duplicate_missing.iter().any(|d| d
            .Message
            .contains("duplicate match arm for variant ShadowMode.Hard")),
        "expected duplicate match arm diagnostic"
    );
    assert!(
        wrong_enum_duplicate_missing.iter().any(|d| d
            .Message
            .contains("match over enum ShadowMode is missing variant")),
        "expected missing variant diagnostic"
    );

    let type_mismatch = ValidateSource("enum ShadowMode { None; Hard; Soft; } shader S { fn Quality(mode: ShadowMode) -> i32 { return match mode { ShadowMode.None => 0 ShadowMode.Hard => float4(1.0, 0.0, 0.0, 1.0) ShadowMode.Soft => 8 }; } }")
        .expect_err("match arm type mismatch should fail");
    assert!(
        type_mismatch.iter().any(|d| d
            .Message
            .contains("match arm type mismatch: expected i32, found float4")),
        "expected match arm type mismatch diagnostic"
    );

    let unhandled_fallible = ValidateSource("enum ShadowMode { None; Hard; Soft; } shader S { fn LoadMode() -> ShadowMode ! Error { return ShadowMode.None; } fn FallibleInt() -> i32 ! Error { return 1; } fn Quality(mode: ShadowMode) -> i32 ! Error { let a: i32 = match LoadMode() { ShadowMode.None => 0 ShadowMode.Hard => 1 ShadowMode.Soft => 8 }; return match mode { ShadowMode.None => 0 ShadowMode.Hard => FallibleInt() ShadowMode.Soft => a }; } }")
        .expect_err("unhandled fallible subject/arm should fail");
    assert!(
        unhandled_fallible.iter().any(|d| d
            .Message
            .contains("fallible expression must be handled with ? or !")),
        "expected unhandled fallible diagnostic in match traversal"
    );

    ValidateSource("enum ShadowMode { None; Hard; Soft; } shader S { fn LoadMode() -> ShadowMode ! Error { return ShadowMode.None; } fn FallibleInt() -> i32 ! Error { return 1; } fn Quality(mode: ShadowMode) -> i32 ! Error { let a: i32 = match LoadMode()? { ShadowMode.None => 0 ShadowMode.Hard => 1 ShadowMode.Soft => 8 }; return match mode { ShadowMode.None => 0 ShadowMode.Hard => FallibleInt()? ShadowMode.Soft => a }; } }")
        .expect("handled fallible match subject/arms should validate");
}

#[test]
fn EmitHlslM64cEnumAndMatchLowering() {
    let src = r#"
enum ShadowMode { None; Hard; Soft; }
record Settings { Mode: ShadowMode; }
shader S {
    fn Quality(mode: ShadowMode) -> i32 {
        let local: ShadowMode = ShadowMode.Hard;
        let quality: i32 = match mode { ShadowMode.None => 0 ShadowMode.Hard => 1 ShadowMode.Soft => 8 };
        let copy: i32 = quality;
        copy = match local { ShadowMode.None => 2 ShadowMode.Hard => 3 ShadowMode.Soft => 4 };
        return match mode { ShadowMode.None => copy ShadowMode.Hard => 5 ShadowMode.Soft => 6 };
    }
}
"#;
    let hlsl_a = CompileSourceToHlsl(src).expect("enum + match source should emit HLSL");
    let hlsl_b = CompileSourceToHlsl(src).expect("repeated enum + match emission should succeed");
    assert_eq!(hlsl_a, hlsl_b, "enum/match emission must be deterministic");
    assert!(
        hlsl_a.contains("static const int ShadowMode_None = 0;")
            && hlsl_a.contains("static const int ShadowMode_Hard = 1;")
            && hlsl_a.contains("static const int ShadowMode_Soft = 2;"),
        "enum constants should emit in declaration order with deterministic names"
    );
    assert!(
        hlsl_a.contains("int S_Quality(int mode)"),
        "enum parameters should lower to int"
    );
    assert!(
        hlsl_a.contains("int Mode;"),
        "enum record fields should lower to int"
    );
    assert!(
        hlsl_a.contains("int local = ShadowMode_Hard;"),
        "qualified variant references should lower to Enum_Variant constants"
    );
    assert!(
        !hlsl_a.contains("ShadowMode.Hard"),
        "HLSL must not preserve Enum.Variant dot syntax"
    );
    assert!(
        hlsl_a.contains("if (mode == ShadowMode_None) {")
            && hlsl_a.contains("else if (mode == ShadowMode_Hard) {")
            && hlsl_a.contains("else {"),
        "match lowering should produce if/else-if/else chain with final else arm"
    );
    assert!(
        hlsl_a.contains("quality = 0;")
            && hlsl_a.contains("copy = 3;")
            && hlsl_a.contains("return 6;"),
        "match lowering should support local initializer, assignment RHS, and return contexts"
    );
}

#[test]
fn EmitHlslM64cRejectsMatchInNestedExpressionContext() {
    let diagnostics = CompileSourceToHlsl(
        "enum ShadowMode { None; Hard; Soft; } shader S { fn F(mode: ShadowMode) -> i32 { let value: i32 = 1 + match mode { ShadowMode.None => 0 ShadowMode.Hard => 1 ShadowMode.Soft => 2 }; return value; } }",
    )
    .expect_err("nested match expressions should not be lowered in M64c");
    assert!(
        diagnostics.iter().any(|d| d.Message.contains(
            "match expression is not supported in this expression context in SDSL-V M64c"
        )),
        "nested expression match should produce clear M64c bounded-context diagnostic"
    );
}

#[test]
fn MatchM65FallibleOkErrValidation() {
    ValidateSource("shader S { fn Parse(raw: i32) -> i32 ! Error { return raw; } fn Use(raw: i32) -> i32 { return match Parse(raw) { ok(v) => v err(_) => 30 }; } }")
        .expect("fallible match over fallible subject should validate");
    ValidateSource("shader S { fn Parse(raw: i32) -> i32 ! Error { return raw; } fn Use(raw: i32) -> i32 { return match Parse(raw) { err(_) -> 30 ok(v) -> v }; } }")
        .expect("fallible match should accept err/ok order and -> arrow");
}

#[test]
fn MatchM65FallibleDiagnostics() {
    let non_fallible = ValidateSource("shader S { fn Value() -> i32 { return 1; } fn Use() -> i32 { return match Value() { ok(v) => v err(_) => 30 }; } }")
        .expect_err("fallible match subject must be fallible");
    assert!(
        non_fallible.iter().any(|d| d
            .Message
            .contains("fallible match requires a fallible expression")),
        "expected non-fallible subject diagnostic"
    );
    let missing_err = ValidateSource("shader S { fn Parse(raw: i32) -> i32 ! Error { return raw; } fn Use(raw: i32) -> i32 { return match Parse(raw) { ok(v) => v }; } }")
        .expect_err("missing err arm should fail");
    assert!(
        missing_err.iter().any(|d| d
            .Message
            .contains("fallible match requires both ok and err arms")),
        "expected missing err diagnostic"
    );
    let duplicate_ok = ValidateSource("shader S { fn Parse(raw: i32) -> i32 ! Error { return raw; } fn Use(raw: i32) -> i32 { return match Parse(raw) { ok(v) => v ok(w) => w err(_) => 0 }; } }")
        .expect_err("duplicate ok arm should fail");
    assert!(
        duplicate_ok
            .iter()
            .any(|d| d.Message.contains("duplicate ok arm in fallible match")),
        "expected duplicate ok diagnostic"
    );
}

#[test]
fn EmitHlslM66bWhenUtilityLowersInBoundedContexts() {
    let src = "shader S { fn Choose(a: i32, b: i32) -> i32 { let result: i32 = when utility { case 100 when a > 0 score a case 200 when b > 0 score b else -1 }; result = when utility { case 300 when a > b score a case 400 when b > a score b else -2 }; return when utility { case 500 when a > b score a case 600 when b > a score b else -3 }; } }";
    let hlsl =
        CompileSourceToHlsl(src).expect("when utility in let/assign/return should lower in M66b");
    assert!(
        hlsl.contains("int result = -1;"),
        "local initializer should start from else fallback"
    );
    assert!(
        hlsl.contains("bool __utility_has0 = false;"),
        "first utility should declare deterministic has-choice temp"
    );
    assert!(
        hlsl.contains("float __utility_score0 = 0.0;"),
        "first utility should declare deterministic best-score temp"
    );
    assert!(
        hlsl.contains("float __utility_case_score0 = a;"),
        "first case score temp should lower"
    );
    assert!(
        hlsl.contains("__utility_case_score0 > __utility_score0"),
        "comparison must be strict > to preserve first-tie wins"
    );
    assert!(
        !hlsl.contains(">="),
        "utility lowering must not use >= tie-breaking"
    );
    assert!(
        hlsl.contains("result = 100;"),
        "first utility should assign winning case value"
    );
    assert!(
        hlsl.contains("result = -2;"),
        "assignment RHS should initialize target to else fallback"
    );
    assert!(
        hlsl.contains("int __utility_result2 = -3;"),
        "return utility should lower through deterministic temp result variable"
    );
    assert!(
        hlsl.contains("return __utility_result2;"),
        "return utility should return lowered temp result"
    );
}

#[test]
fn ParseWhenPolicyRejectedOutsideFlowStateBodies() {
    let src = "shader S { fn Bad(a: i32) -> i32 { return when policy { hysteresis: 2 min_commit: 3 } { case 1 when a > 0 score a else 0 }; } }";
    let diagnostics = ParseSource(src)
        .expect_err("when policy must be rejected in ordinary shader/helper function bodies");
    assert!(
        diagnostics.iter().any(|d| d.Message.contains(
            "when policy is only valid inside flow/state bodies; use when utility for standalone ranked expressions"
        )),
        "expected flow/state-only when policy diagnostic"
    );
}

#[test]
fn EmitHlslM66bWhenUtilityRejectsOptionsAndNestedContext() {
    let with_options = CompileSourceToHlsl("shader S { fn F(a: i32) -> i32 { return when utility { hysteresis: 2 min_commit: 3 } { case 100 when a > 0 score a else -1 }; } }")
        .expect_err("stateful when utility options must not silently lower in M66b");
    assert!(
        with_options.iter().any(|d| d
            .Message
            .contains("stateful when utility options are not lowered in SDSL-V M66b")),
        "stateful option form should produce explicit M66b unsupported diagnostic"
    );

    let nested = CompileSourceToHlsl("shader S { fn F(a: i32) -> i32 { let x: i32 = 1 + when utility { case 100 when a > 0 score a else -1 }; return x; } }")
        .expect_err("nested when utility expression should fail bounded-context lowering");
    assert!(
        nested.iter().any(|d| d.Message.contains(
            "when utility expression is not supported in this expression context in SDSL-V M66b"
        )),
        "nested utility expression should report clear bounded-context unsupported diagnostic"
    );
}
