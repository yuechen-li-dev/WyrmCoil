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
