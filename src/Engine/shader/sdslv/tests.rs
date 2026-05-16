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
