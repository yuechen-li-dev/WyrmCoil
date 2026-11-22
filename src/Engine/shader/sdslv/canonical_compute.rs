//! Validated storage and thread interfaces for canonical compute entries.

use std::collections::HashSet;

use super::ast::*;
use super::canonical_graphics::AttributeIndex;
use super::diagnostic::SdslvDiagnostic;
use super::vdmir::{VdMirBuiltin, VdMirScalar, VdMirStorageBinding};

pub(super) struct ComputeInterface {
    pub Shader: String,
    pub Input: String,
    pub ResourceStream: Option<String>,
    pub Threads: [u32; 3],
    pub Bindings: Vec<VdMirStorageBinding>,
}

pub(super) fn ResolveCompute(
    module: &SdslvModule,
) -> Result<Option<ComputeInterface>, SdslvDiagnostic> {
    let mut compute = None;
    for declaration in &module.Declarations {
        let SdslvDecl::Shader(shader) = declaration else {
            continue;
        };
        let Some(entry) = shader
            .StageMethods
            .iter()
            .find(|method| method.Stage.as_deref() == Some("compute"))
        else {
            continue;
        };
        let invalid = |message| SdslvDiagnostic::New(message, entry.ReturnTypeSpan);
        if compute.is_some()
            || shader.StageMethods.len() != 1
            || !shader.Methods.is_empty()
            || !shader.GenericParameters.is_empty()
            || !shader.Implements.is_empty()
        {
            return Err(invalid(
                "compute lowering supports one standalone entry per module",
            ));
        }
        if entry.ReturnType.ToDisplayString() != "void" || entry.Body.is_none() {
            return Err(invalid("compute entry requires a void return and a body"));
        }
        let [parameter] = entry.Parameters.as_slice() else {
            return Err(invalid("compute entry requires one thread input stream"));
        };
        let input = parameter.TypeName.ToDisplayString();
        let stream = FindStream(module, &input)
            .ok_or_else(|| invalid("compute entry input must name a thread stream"))?;
        if stream.Fields.is_empty() {
            return Err(invalid(
                "compute thread stream must declare a thread builtin",
            ));
        }
        for field in &stream.Fields {
            let expected = match ComputeBuiltin(&field.Name) {
                Some(VdMirBuiltin::GroupIndex) => "u32",
                Some(_) => "uint3",
                None => {
                    return Err(SdslvDiagnostic::New(
                        "unknown compute thread builtin",
                        field.Span,
                    ));
                }
            };
            if field.TypeName.ToDisplayString() != expected
                || field.Access.is_some()
                || !field.Attributes.is_empty()
            {
                return Err(SdslvDiagnostic::New(
                    &format!(
                        "compute builtin '{}' requires an unannotated {expected}",
                        field.Name
                    ),
                    field.Span,
                ));
            }
        }
        let threads = ThreadSize(entry)?;
        let mut resource_stream = None;
        let fields = match &shader.Resources {
            Some(SdslvResourceDecl::Stream(path)) => {
                let name = path.Segments.join(".");
                let stream = FindStream(module, &name)
                    .ok_or_else(|| invalid("compute resources must name a storage stream"))?;
                resource_stream = Some(name);
                stream.Fields.as_slice()
            }
            Some(SdslvResourceDecl::Inline(fields)) => fields,
            None => &[],
        };
        let mut bindings = vec![];
        let mut names = HashSet::new();
        let mut slots = HashSet::new();
        for field in fields {
            let [attribute] = field.Attributes.as_slice() else {
                return Err(SdslvDiagnostic::New(
                    "compute storage requires one [binding(n)]",
                    field.Span,
                ));
            };
            if attribute.Name != "binding" {
                return Err(SdslvDiagnostic::New(
                    "compute storage requires [binding(n)]",
                    attribute.Span,
                ));
            }
            let binding = AttributeIndex(attribute, "compute storage")?;
            if !slots.insert(binding) || !names.insert(&field.Name) {
                return Err(SdslvDiagnostic::New(
                    "duplicate compute storage name or binding",
                    field.Span,
                ));
            }
            let Some(access) = field.Access else {
                return Err(SdslvDiagnostic::New(
                    "compute storage requires readonly or readwrite access",
                    field.Span,
                ));
            };
            bindings.push(VdMirStorageBinding {
                Name: field.Name.clone(),
                Binding: binding,
                Element: StorageElement(&field.TypeName).ok_or_else(|| {
                    SdslvDiagnostic::New(
                        "compute storage requires array<f32>, array<i32>, or array<u32>",
                        field.TypeSpan,
                    )
                })?,
                Writable: access == SdslvResourceAccess::Readwrite,
            });
        }
        compute = Some(ComputeInterface {
            Shader: shader.Name.clone(),
            Input: input,
            ResourceStream: resource_stream,
            Threads: threads,
            Bindings: bindings,
        });
    }
    Ok(compute)
}

fn FindStream<'a>(module: &'a SdslvModule, name: &str) -> Option<&'a SdslvStreamDecl> {
    module
        .Declarations
        .iter()
        .find_map(|declaration| match declaration {
            SdslvDecl::Stream(stream) if stream.Name == name => Some(stream),
            _ => None,
        })
}

fn ThreadSize(entry: &SdslvFunctionDecl) -> Result<[u32; 3], SdslvDiagnostic> {
    let [attribute] = entry.Attributes.as_slice() else {
        return Err(SdslvDiagnostic::New(
            "compute entry requires one [numthreads(x, y, z)]",
            entry.ReturnTypeSpan,
        ));
    };
    let invalid = || SdslvDiagnostic::New("invalid compute [numthreads(x, y, z)]", attribute.Span);
    if attribute.Name != "numthreads" || attribute.Arguments.len() != 3 {
        return Err(invalid());
    }
    let mut size = [0; 3];
    for (axis, arg) in size.iter_mut().zip(&attribute.Arguments) {
        let SdslvExpression::IntegerLiteral(value) = arg else {
            return Err(invalid());
        };
        *axis = value
            .trim_end_matches(['u', 'U'])
            .parse::<u32>()
            .map_err(|_| invalid())?;
        if *axis == 0 || *axis > 1024 {
            return Err(invalid());
        }
    }
    if size[2] > 64 || size.iter().product::<u32>() > 1024 {
        return Err(invalid());
    }
    Ok(size)
}

pub(super) fn ComputeBuiltin(name: &str) -> Option<VdMirBuiltin> {
    Some(match name {
        "DispatchId" => VdMirBuiltin::DispatchId,
        "GroupId" => VdMirBuiltin::GroupId,
        "GroupThreadId" => VdMirBuiltin::GroupThreadId,
        "GroupIndex" => VdMirBuiltin::GroupIndex,
        _ => return None,
    })
}

fn StorageElement(ty: &SdslvTypeRef) -> Option<VdMirScalar> {
    let SdslvTypeRef::Applied {
        Name,
        Arguments,
        Extents,
        ..
    } = ty
    else {
        return None;
    };
    if Name.Segments.as_slice() != ["array"] || !Extents.is_empty() {
        return None;
    }
    let [element] = Arguments.as_slice() else {
        return None;
    };
    Some(match element.ToDisplayString().as_str() {
        "f32" => VdMirScalar::F32,
        "i32" => VdMirScalar::I32,
        "u32" => VdMirScalar::U32,
        _ => return None,
    })
}

pub(super) fn ScalarName(scalar: VdMirScalar) -> &'static str {
    match scalar {
        VdMirScalar::F32 => "f32",
        VdMirScalar::I32 => "i32",
        VdMirScalar::U32 => "u32",
        VdMirScalar::Bool => "bool",
    }
}
