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
fn ParserGenericWhereConstraints() {
    let src = include_str!("../../../../examples/sdslv/generic_forward_pass.sdslv");
    let m = ParseSource(src).unwrap();
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
    assert_eq!(shader.GenericParameters[0], "TMat");
    assert_eq!(shader.Constraints[0].Bounds[0].Segments[0], "IBaseColor");
}

#[test]
fn ParserInvalidCases() {
    assert!(ParseSource("namespace ;").is_err());
    assert!(ParseSource("stream A { X float4; }").is_err());
    assert!(ParseSource("interface I { fn A(x: T) float4; }").is_err());
    assert!(ParseSource("shader S { fn A() -> X { ").is_err());
    assert!(ParseSource("bogus").is_err());
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
    assert!(
        d.iter()
            .any(|x| x.Message.contains("interface 'Missing' is unknown"))
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
fn EmitHlslGenericShaderReturnsDiagnostic() {
    let src = r#"
        interface IBaseColor { fn BaseColor(s: Surface) -> float4; }
        interface INormalProvider { fn Normal(s: Surface) -> float3; }
        stream Surface { Color: float4; }
        shader ForwardPass<TMat>
            where TMat : IBaseColor, INormalProvider
        {
            stage pixel fn PS(s: Surface, mat: TMat) -> float4 {
                return mat.BaseColor(s);
            }
        }
    "#;
    let module = ValidateSource(src).unwrap();
    let diagnostics = EmitHlsl(&module).unwrap_err();
    assert!(
        diagnostics.iter().any(|x| x
            .Message
            .contains("cannot emit generic shader 'ForwardPass' in SDSL-V M3")),
        "expected generic emission diagnostic"
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

    let unsupported_if =
        ParseSource("shader S { stage pixel fn PS() -> float4 { if (a) { return a; } } }")
            .unwrap_err();
    assert!(
        unsupported_if
            .iter()
            .any(|d| d.Message.contains("unsupported statement"))
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
