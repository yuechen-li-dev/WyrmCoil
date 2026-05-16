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
        board.Fields[0].TypeName.Segments.join("."),
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
