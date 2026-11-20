//! The resolved interface roles used by canonical graphics validation and HLSL.

use std::collections::{BTreeMap, BTreeSet};

use super::ast::*;
use super::diagnostic::SdslvDiagnostic;
use super::vdmir::{VdMirBuiltin as Builtin, VdMirInterpolation as Interpolation};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GraphicsFieldRole {
    Location(u32, Interpolation),
    Target(u32),
    Builtin(Builtin),
    ClipPosition,
    Resource,
    Unspecified,
}

pub(super) fn ResolveGraphicsField(
    module: &SdslvModule,
    field: &SdslvFieldDecl,
) -> Result<GraphicsFieldRole, SdslvDiagnostic> {
    let (ty, space) = ResolveGraphicsType(module, &field.TypeName);
    let mut interpolation = match ty.as_str() {
        "i32" | "u32" | "bool" => Interpolation::Flat,
        _ => Interpolation::Linear,
    };
    let mut role = if field.Access.is_some() {
        GraphicsFieldRole::Resource
    } else if space.as_deref() == Some("clip.position") {
        GraphicsFieldRole::ClipPosition
    } else {
        GraphicsFieldRole::Unspecified
    };
    for attribute in &field.Attributes {
        let candidate = match attribute.Name.as_str() {
            "location" => {
                GraphicsFieldRole::Location(AttributeIndex(attribute, "SDSL-V4105")?, interpolation)
            }
            "target" => GraphicsFieldRole::Target(AttributeIndex(attribute, "SDSL-V4108")?),
            "binding" if field.Access.is_some() => continue,
            "binding" => {
                return Err(SdslvDiagnostic::New(
                    "SDSL-V4102: binding requires resource access",
                    attribute.Span,
                ));
            }
            "builtin" => {
                let builtin = match AttributeName(attribute) {
                    Some("vertex_id") => Builtin::VertexId,
                    Some("instance_id") => Builtin::InstanceId,
                    Some("position") => Builtin::Position,
                    Some("front_face") => Builtin::FrontFace,
                    _ => {
                        return Err(SdslvDiagnostic::New(
                            "SDSL-V4109: unknown graphics builtin",
                            attribute.Span,
                        ));
                    }
                };
                GraphicsFieldRole::Builtin(builtin)
            }
            "interpolation" => {
                interpolation = match AttributeName(attribute) {
                    Some("flat") => Interpolation::Flat,
                    Some("linear") => Interpolation::Linear,
                    Some("noperspective") => Interpolation::NoPerspective,
                    _ => {
                        return Err(SdslvDiagnostic::New(
                            "SDSL-V4105: invalid interpolation mode",
                            attribute.Span,
                        ));
                    }
                };
                continue;
            }
            _ => {
                return Err(SdslvDiagnostic::New(
                    &format!("SDSL-V4103: unknown stream attribute [{}]", attribute.Name),
                    attribute.Span,
                ));
            }
        };
        if role != GraphicsFieldRole::Unspecified {
            return Err(SdslvDiagnostic::New(
                "SDSL-V4102: field has more than one interface role",
                attribute.Span,
            ));
        }
        role = candidate;
    }
    if let GraphicsFieldRole::Location(index, _) = role {
        role = GraphicsFieldRole::Location(index, interpolation);
    }
    Ok(role)
}

fn AttributeIndex(attribute: &SdslvAttribute, code: &str) -> Result<u32, SdslvDiagnostic> {
    if let [SdslvExpression::IntegerLiteral(value)] = attribute.Arguments.as_slice()
        && let Ok(index) = value.trim_end_matches(['u', 'U']).parse()
    {
        return Ok(index);
    }
    Err(SdslvDiagnostic::New(
        &format!(
            "{code}: {} requires one non-negative integer",
            attribute.Name
        ),
        attribute.Span,
    ))
}

fn AttributeName(attribute: &SdslvAttribute) -> Option<&str> {
    match attribute.Arguments.as_slice() {
        [SdslvExpression::Identifier(name)] | [SdslvExpression::StringLiteral(name)] => Some(name),
        _ => None,
    }
}

fn ResolveGraphicsType(module: &SdslvModule, ty: &SdslvTypeRef) -> (String, Option<String>) {
    let mut name = ty.ToDisplayString();
    let mut space = None;
    for _ in &module.Declarations {
        let Some(alias) = module
            .Declarations
            .iter()
            .find_map(|declaration| match declaration {
                SdslvDecl::TypeAlias(alias) if alias.Name == name => Some(alias),
                _ => None,
            })
        else {
            break;
        };
        if space.is_none() {
            space = alias
                .SpaceAnnotation
                .as_ref()
                .map(|path| path.Segments.join("."));
        }
        name = alias.TargetType.ToDisplayString();
    }
    if name == "float" {
        name = "f32".to_string();
    }
    (name, space)
}

pub(super) fn ValidateGraphics(module: &SdslvModule) -> Vec<SdslvDiagnostic> {
    let mut diagnostics = vec![];
    let mut streams = BTreeMap::new();
    for declaration in &module.Declarations {
        let SdslvDecl::Stream(stream) = declaration else {
            continue;
        };
        let mut fields = vec![];
        let mut locations = BTreeSet::new();
        let mut targets = BTreeSet::new();
        let mut builtins = BTreeSet::new();
        let mut kinds = BTreeSet::new();
        for field in &stream.Fields {
            let role = match ResolveGraphicsField(module, field) {
                Ok(role) => role,
                Err(diagnostic) => {
                    diagnostics.push(diagnostic);
                    continue;
                }
            };
            let kind = match role {
                GraphicsFieldRole::Location(index, _) => {
                    if !locations.insert(index) {
                        diagnostics.push(SdslvDiagnostic::New(
                            "SDSL-V4105: duplicate stream location",
                            field.Span,
                        ));
                    }
                    "stage-value"
                }
                GraphicsFieldRole::Target(index) => {
                    if !targets.insert(index) {
                        diagnostics.push(SdslvDiagnostic::New(
                            "SDSL-V4108: duplicate pixel target",
                            field.Span,
                        ));
                    }
                    "stage-value"
                }
                GraphicsFieldRole::Builtin(builtin) => {
                    if !builtins.insert(builtin) {
                        diagnostics.push(SdslvDiagnostic::New(
                            "SDSL-V4110: duplicate builtin",
                            field.Span,
                        ));
                    }
                    "builtin"
                }
                GraphicsFieldRole::ClipPosition => "stage-value",
                GraphicsFieldRole::Resource => "resource",
                GraphicsFieldRole::Unspecified => "",
            };
            if !kind.is_empty() {
                kinds.insert(kind);
            }
            fields.push((field, role));
        }
        if kinds.len() > 1 {
            diagnostics.push(SdslvDiagnostic::New(
                "SDSL-V4102: stream mixes interface roles",
                stream.Span,
            ));
        }
        streams.insert(stream.Name.as_str(), fields);
    }

    for declaration in &module.Declarations {
        let SdslvDecl::Shader(shader) = declaration else {
            continue;
        };
        let mut varyings = BTreeMap::new();
        let mut pixel_inputs = vec![];
        let mut has_vertex = false;
        for method in &shader.StageMethods {
            let Some(stage @ ("vertex" | "pixel")) = method.Stage.as_deref() else {
                continue;
            };
            for parameter in &method.Parameters {
                let Some(fields) = streams.get(parameter.TypeName.ToDisplayString().as_str())
                else {
                    diagnostics.push(SdslvDiagnostic::New(
                        "SDSL-V4104: graphics input must name a stream",
                        method.ReturnTypeSpan,
                    ));
                    continue;
                };
                for &(field, role) in fields {
                    if let GraphicsFieldRole::Builtin(builtin) = role {
                        ValidateBuiltin(module, field, builtin, stage, &mut diagnostics);
                    }
                    if stage == "pixel"
                        && let GraphicsFieldRole::Location(index, interpolation) = role
                    {
                        pixel_inputs.push((index, interpolation, field));
                    }
                }
            }
            let return_name = method.ReturnType.ToDisplayString();
            let output = streams.get(return_name.as_str());
            if stage == "vertex" {
                has_vertex = true;
                let mut position_count = 0;
                for &(field, role) in output.into_iter().flatten() {
                    if role == GraphicsFieldRole::ClipPosition {
                        position_count += 1;
                        if ResolveGraphicsType(module, &field.TypeName).0 != "float4" {
                            diagnostics.push(SdslvDiagnostic::New(
                                "SDSL-V4106: clip position must be float4",
                                field.TypeSpan,
                            ));
                        }
                    }
                    if let GraphicsFieldRole::Location(index, interpolation) = role {
                        varyings.insert(index, (interpolation, field));
                    }
                }
                let code = match position_count {
                    0 => Some("SDSL-V4106: vertex output requires one clip position"),
                    1 => None,
                    _ => Some("SDSL-V4107: vertex output has multiple clip positions"),
                };
                if let Some(message) = code {
                    diagnostics.push(SdslvDiagnostic::New(message, method.ReturnTypeSpan));
                }
            } else if let Some(fields) = output {
                for &(field, role) in fields {
                    if !matches!(role, GraphicsFieldRole::Target(_)) {
                        diagnostics.push(SdslvDiagnostic::New(
                            "SDSL-V4108: pixel output requires a target",
                            field.Span,
                        ));
                    }
                }
            } else {
                let (ty, space) = ResolveGraphicsType(module, &method.ReturnType);
                if space.is_some() || !matches!(ty.as_str(), "f32" | "float2" | "float3" | "float4")
                {
                    diagnostics.push(SdslvDiagnostic::New(
                        "SDSL-V4108: invalid pixel return type",
                        method.ReturnTypeSpan,
                    ));
                }
            }
        }
        if has_vertex {
            for (index, interpolation, input) in pixel_inputs {
                let linked = varyings
                    .get(&index)
                    .is_some_and(|(output_interpolation, output)| {
                        *output_interpolation == interpolation
                            && ResolveGraphicsType(module, &output.TypeName)
                                == ResolveGraphicsType(module, &input.TypeName)
                    });
                if !linked {
                    diagnostics.push(SdslvDiagnostic::New(
                        &format!("SDSL-V4111: pixel input at location {index} does not match its vertex output"),
                        input.Span,
                    ));
                }
            }
        }
    }
    diagnostics
}

fn ValidateBuiltin(
    module: &SdslvModule,
    field: &SdslvFieldDecl,
    builtin: Builtin,
    stage: &str,
    diagnostics: &mut Vec<SdslvDiagnostic>,
) {
    let (expected, allowed) = match builtin {
        Builtin::VertexId | Builtin::InstanceId => ("u32", "vertex"),
        Builtin::Position => ("float4", "pixel"),
        Builtin::FrontFace => ("bool", "pixel"),
    };
    if allowed != stage {
        let span = field
            .Attributes
            .iter()
            .find(|attr| attr.Name == "builtin")
            .map(|attr| attr.Span)
            .unwrap_or(field.Span);
        diagnostics.push(SdslvDiagnostic::New(
            "SDSL-V4109: builtin is invalid for this stage",
            span,
        ));
    }
    let (ty, space) = ResolveGraphicsType(module, &field.TypeName);
    if ty != expected || (space.is_some() && builtin != Builtin::Position) {
        diagnostics.push(SdslvDiagnostic::New(
            &format!("SDSL-V4110: builtin requires {expected}"),
            field.TypeSpan,
        ));
    }
}
