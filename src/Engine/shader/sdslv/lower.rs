//! Validation and lowering from source syntax into the owned VD-MIR module.

use std::collections::HashMap;

use super::ast::*;
use super::canonical_graphics::{GraphicsFieldRole, ResolveGraphicsField};
use super::diagnostic::SdslvDiagnostic;
use super::token::SdslvSpan;
use super::validation::{TypeRef, ValidateForLowering, Validator};
use super::vdmir::*;

mod flow;

type LowerResult<T> = Result<T, SdslvDiagnostic>;
type Locals = HashMap<String, TypeRef>;

pub fn LowerToVdMir(module: &SdslvModule) -> Result<VdMirModule, Vec<SdslvDiagnostic>> {
    let validator = ValidateForLowering(module)?;
    Lowerer {
        Module: module,
        Validator: validator,
    }
    .Lower()
    .map_err(|error| vec![error])
}

struct Lowerer<'a> {
    Module: &'a SdslvModule,
    Validator: Validator<'a>,
}

struct Specialization {
    Receiver: String,
    Implementation: String,
}

enum Scope<'a> {
    Shader {
        Shader: &'a SdslvShaderDecl,
        Locals: &'a Locals,
        Specialization: Option<&'a Specialization>,
    },
    Flow(&'a Locals),
}

impl Lowerer<'_> {
    fn Lower(&self) -> LowerResult<VdMirModule> {
        let mut items = vec![];
        for declaration in &self.Module.Declarations {
            match declaration {
                SdslvDecl::TypeAlias(alias) => items.push(VdMirItem::TypeAlias {
                    Name: alias.Name.clone(),
                    Target: self.LowerType(&alias.TargetType, None)?,
                    Space: alias.SpaceAnnotation.as_ref().map(PathText),
                }),
                SdslvDecl::Stream(stream) => {
                    let roles = self.LowerStreamRoles(stream)?;
                    let fields = stream
                        .Fields
                        .iter()
                        .zip(roles)
                        .map(|(field, role)| self.LowerField(field, Some(role)))
                        .collect::<LowerResult<_>>()?;
                    items.push(VdMirItem::Aggregate(VdMirAggregate {
                        Name: stream.Name.clone(),
                        Fields: fields,
                        HasConstructor: self.Module.Canonical,
                    }));
                }
                SdslvDecl::Record(record) => {
                    let fields = record
                        .Fields
                        .iter()
                        .map(|field| self.LowerField(field, None))
                        .collect::<LowerResult<_>>()?;
                    items.push(VdMirItem::Aggregate(VdMirAggregate {
                        Name: record.Name.clone(),
                        Fields: fields,
                        HasConstructor: self.Module.Canonical,
                    }));
                }
                SdslvDecl::Enum(declaration) => items.push(VdMirItem::Enum {
                    Name: declaration.Name.clone(),
                    Variants: declaration
                        .Variants
                        .iter()
                        .map(|variant| variant.Name.clone())
                        .collect(),
                }),
                SdslvDecl::Interface(interface) => {
                    items.push(VdMirItem::LegacyInterface(interface.Name.clone()))
                }
                SdslvDecl::Shader(shader) => self.LowerShader(shader, &mut items)?,
                SdslvDecl::Compile(compile) => self.LowerSpecialization(compile, &mut items)?,
                SdslvDecl::Flow(flow) => items.push(VdMirItem::Flow(self.LowerFlow(flow)?)),
                SdslvDecl::Concept(_)
                | SdslvDecl::Config(_)
                | SdslvDecl::Board(_)
                | SdslvDecl::Function(_) => {
                    return Err(Diagnostic(
                        "canonical declaration is outside the VD-MIR lowering subset",
                        None,
                    ));
                }
            }
        }
        Ok(VdMirModule {
            Namespace: self.Module.Namespace.as_ref().map(PathText),
            Items: items,
        })
    }

    fn LowerStreamRoles(&self, stream: &SdslvStreamDecl) -> LowerResult<Vec<VdMirFieldRole>> {
        let mut roles = Vec::with_capacity(stream.Fields.len());
        let mut next_location = 0;
        let mut has_position = false;
        for field in &stream.Fields {
            let role = if self.Module.Canonical {
                match ResolveGraphicsField(self.Module, field)? {
                    GraphicsFieldRole::Location(index, interpolation) => {
                        VdMirFieldRole::Location(index, interpolation)
                    }
                    GraphicsFieldRole::Target(index) => VdMirFieldRole::Target(index),
                    GraphicsFieldRole::Builtin(builtin) => VdMirFieldRole::Builtin(builtin),
                    GraphicsFieldRole::ClipPosition => VdMirFieldRole::ClipPosition,
                    GraphicsFieldRole::Resource | GraphicsFieldRole::Unspecified => {
                        return Err(Diagnostic(
                            "canonical graphics field has no HLSL interface semantic",
                            Some(field.Span),
                        ));
                    }
                }
            } else if self.IsLegacyPosition(field) {
                if has_position {
                    return Err(Diagnostic(
                        &format!(
                            "stream '{}' has duplicate field mapping to SV_Position",
                            stream.Name
                        ),
                        Some(field.Span),
                    ));
                }
                has_position = true;
                VdMirFieldRole::ClipPosition
            } else {
                let role = VdMirFieldRole::LegacyLocation(next_location);
                next_location += 1;
                role
            };
            roles.push(role);
        }
        Ok(roles)
    }

    fn IsLegacyPosition(&self, field: &SdslvFieldDecl) -> bool {
        let name = field.TypeName.ToDisplayString();
        (field.Name == "Position" && name == "ClipPosition4")
            || self
                .FindAlias(&name)
                .and_then(|alias| alias.SpaceAnnotation.as_ref())
                .is_some_and(|space| PathText(space) == "clip.position")
    }

    fn LowerField(
        &self,
        field: &SdslvFieldDecl,
        role: Option<VdMirFieldRole>,
    ) -> LowerResult<VdMirField> {
        Ok(VdMirField {
            Name: field.Name.clone(),
            Type: self.LowerType(&field.TypeName, Some(field.TypeSpan))?,
            Role: role,
            Span: field.Span,
        })
    }

    fn LowerShader(&self, shader: &SdslvShaderDecl, items: &mut Vec<VdMirItem>) -> LowerResult<()> {
        if shader
            .Methods
            .iter()
            .chain(&shader.StageMethods)
            .any(|method| method.ErrorType.is_some())
        {
            return Err(FallibleDiagnostic(None));
        }
        for method in &shader.Methods {
            items.push(VdMirItem::Function(self.LowerFunction(
                shader,
                method,
                &shader.Name,
                None,
            )?));
        }
        if shader.GenericParameters.is_empty() {
            for method in &shader.StageMethods {
                items.push(VdMirItem::Function(self.LowerFunction(
                    shader,
                    method,
                    &shader.Name,
                    None,
                )?));
            }
        }
        Ok(())
    }

    fn LowerSpecialization(
        &self,
        compile: &SdslvCompileDecl,
        items: &mut Vec<VdMirItem>,
    ) -> LowerResult<()> {
        let name = PathText(&compile.GenericShader);
        let shader = self
            .FindShader(&name)
            .ok_or_else(|| Diagnostic("unresolved shader specialization", None))?;
        for method in &shader.StageMethods {
            items.push(VdMirItem::Function(self.LowerFunction(
                shader,
                method,
                &compile.Alias,
                compile.TypeArguments.first(),
            )?));
        }
        Ok(())
    }

    fn LowerFunction(
        &self,
        shader: &SdslvShaderDecl,
        method: &SdslvFunctionDecl,
        program: &str,
        type_argument: Option<&SdslvTypeRef>,
    ) -> LowerResult<VdMirFunction> {
        let mut locals = Locals::new();
        let mut parameters = vec![];
        let mut specialization = None;
        for parameter in &method.Parameters {
            let ty = if specialization.is_none()
                && let Some(argument) = type_argument
                && shader
                    .GenericParameters
                    .contains(&parameter.TypeName.ToDisplayString())
            {
                specialization = Some(Specialization {
                    Receiver: parameter.Name.clone(),
                    Implementation: argument.ToDisplayString(),
                });
                argument
            } else {
                &parameter.TypeName
            };
            parameters.push(VdMirParameter {
                Name: parameter.Name.clone(),
                Type: self.LowerType(ty, None)?,
            });
            locals.insert(parameter.Name.clone(), self.Validator.TypeRefToType(ty));
        }
        let return_type = self.LowerType(&method.ReturnType, Some(method.ReturnTypeSpan))?;
        let kind = match method.Stage.as_deref() {
            Some("vertex") => Some(VdMirEntryKind::Vertex),
            Some("compute") => Some(VdMirEntryKind::Compute),
            Some("pixel") => {
                let stream = self.Module.Canonical
                    && self
                        .FindStream(&method.ReturnType.ToDisplayString())
                        .is_some_and(|stream| {
                            stream.Fields.iter().any(|field| {
                                matches!(
                                    ResolveGraphicsField(self.Module, field),
                                    Ok(GraphicsFieldRole::Target(_))
                                )
                            })
                        });
                let output = if stream {
                    VdMirPixelOutput::Stream
                } else {
                    VdMirPixelOutput::Scalar
                };
                Some(VdMirEntryKind::Pixel(output))
            }
            None => None,
            Some(_) => {
                return Err(Diagnostic(
                    "unresolved shader stage",
                    Some(method.ReturnTypeSpan),
                ));
            }
        };
        let entry = kind.map(|kind| VdMirEntry {
            Program: program.to_string(),
            Method: method.Name.clone(),
            Kind: kind,
        });
        let mut body = vec![];
        if let Some(block) = &method.Body {
            body = self.LowerBlock(
                shader,
                &block.Statements,
                &mut locals,
                specialization.as_ref(),
            )?;
        }
        Ok(VdMirFunction {
            Name: format!("{program}_{}", method.Name),
            Entry: entry,
            Parameters: parameters,
            ReturnType: return_type,
            Body: body,
            Span: method.Body.as_ref().map(|body| body.Span),
        })
    }

    fn LowerBlock(
        &self,
        shader: &SdslvShaderDecl,
        statements: &[SdslvStatement],
        locals: &mut Locals,
        specialization: Option<&Specialization>,
    ) -> LowerResult<Vec<VdMirStatement>> {
        let mut body = Vec::with_capacity(statements.len());
        for statement in statements {
            let scope = Scope::Shader {
                Shader: shader,
                Locals: locals,
                Specialization: specialization,
            };
            let lower = |value| self.LowerValue(value, &scope);
            let statement = match statement {
                SdslvStatement::Let {
                    Name,
                    TypeName,
                    Initializer,
                    Span,
                    ..
                } => {
                    let ty = self.LowerType(TypeName, Some(*Span))?;
                    let mut initial = Initializer.as_ref().map(lower).transpose()?;
                    if let Some(value) = &mut initial
                        && matches!(value.Kind, VdMirValueKind::ArrayLiteral { .. })
                    {
                        value.Type = ty.clone();
                    }
                    locals.insert(Name.clone(), self.Validator.TypeRefToType(TypeName));
                    VdMirStatement::Let {
                        Name: Name.clone(),
                        Type: ty,
                        Initializer: initial,
                    }
                }
                SdslvStatement::Assign { Target, Value, .. } => VdMirStatement::Assign {
                    Target: lower(Target)?,
                    Value: lower(Value)?,
                },
                SdslvStatement::Return { Value } => VdMirStatement::Return {
                    Value: lower(Value)?,
                },
                SdslvStatement::ReturnVoid => VdMirStatement::ReturnVoid,
                SdslvStatement::Expression { Value } => VdMirStatement::Expression {
                    Value: lower(Value)?,
                },
                SdslvStatement::Empty => VdMirStatement::Empty,
                SdslvStatement::If {
                    Condition,
                    ThenBody,
                    ElseBody,
                    ..
                } => VdMirStatement::If {
                    Condition: lower(Condition)?,
                    ThenBody: self.LowerBlock(
                        shader,
                        ThenBody,
                        &mut locals.clone(),
                        specialization,
                    )?,
                    ElseBody: ElseBody
                        .as_ref()
                        .map(|body| {
                            self.LowerBlock(shader, body, &mut locals.clone(), specialization)
                        })
                        .transpose()?,
                },
                SdslvStatement::For {
                    Iterator,
                    Start,
                    End,
                    Step,
                    Body,
                    ..
                } => {
                    let mut inner_locals = locals.clone();
                    inner_locals.insert(Iterator.clone(), TypeRef::Named("i32".to_string()));
                    VdMirStatement::For {
                        Iterator: Iterator.clone(),
                        Start: lower(Start)?,
                        End: lower(End)?,
                        Step: Step.as_ref().map(lower).transpose()?,
                        Body: self.LowerBlock(shader, Body, &mut inner_locals, specialization)?,
                    }
                }
                SdslvStatement::Comptime { Span, .. }
                | SdslvStatement::StaticAssert { Span, .. }
                | SdslvStatement::InlineFlow { Span, .. }
                | SdslvStatement::FlowTransition { Span, .. }
                | SdslvStatement::GuardedWrite { Span, .. }
                | SdslvStatement::GuardWhen { Span, .. } => {
                    return Err(Diagnostic(
                        "canonical statement is outside the VD-MIR lowering subset",
                        Some(*Span),
                    ));
                }
            };
            body.push(statement);
        }
        Ok(body)
    }

    fn LowerValue(
        &self,
        expression: &SdslvExpression,
        scope: &Scope<'_>,
    ) -> LowerResult<VdMirValue> {
        use VdMirValueKind as Kind;
        let inferred = match scope {
            Scope::Shader { Shader, Locals, .. } => self
                .Validator
                .ResolveExpressionType(Shader, Locals, expression),
            Scope::Flow(locals) => self.Validator.ResolveFlowExpressionType(locals, expression),
        };
        let ty = self.ResolveInferredType(&inferred);
        let span = ExpressionSpan(expression);
        let lower = |value: &SdslvExpression| self.LowerValue(value, scope);
        let boxed = |value: &SdslvExpression| lower(value).map(Box::new);
        let values = |expressions: &[SdslvExpression]| {
            expressions
                .iter()
                .map(lower)
                .collect::<LowerResult<Vec<_>>>()
        };
        let kind = match expression {
            SdslvExpression::Identifier(name) => Kind::Name(name.clone()),
            SdslvExpression::IntegerLiteral(value) => Kind::IntegerLiteral(value.clone()),
            SdslvExpression::FloatLiteral(value) => Kind::FloatLiteral(value.clone()),
            SdslvExpression::StringLiteral(value) => Kind::StringLiteral(value.clone()),
            SdslvExpression::BoolLiteral(value) => Kind::BoolLiteral(*value),
            SdslvExpression::ArrayLiteral { Elements, .. } => Kind::ArrayLiteral {
                Elements: values(Elements)?,
            },
            SdslvExpression::FieldAccess { Base, Field } => {
                if let SdslvExpression::Identifier(name) = Base.as_ref()
                    && self.IsEnum(name)
                {
                    Kind::EnumVariant {
                        Enum: name.clone(),
                        Variant: Field.clone(),
                    }
                } else if matches!(scope, Scope::Flow(_))
                    && matches!(Base.as_ref(), SdslvExpression::Identifier(name) if name == "board")
                {
                    Kind::Name(Field.clone())
                } else {
                    Kind::FieldAccess {
                        Base: boxed(Base)?,
                        Field: Field.clone(),
                    }
                }
            }
            SdslvExpression::Index { Base, Index, .. } => Kind::Index {
                Base: boxed(Base)?,
                Index: boxed(Index)?,
            },
            SdslvExpression::Call { Callee, Arguments } => {
                if matches!(Callee.as_ref(), SdslvExpression::Identifier(name) if name == "error") {
                    return Err(FallibleDiagnostic(span));
                }
                let mut callee = lower(Callee)?;
                if let Scope::Shader {
                    Specialization: Some(specialization),
                    ..
                } = scope
                    && let SdslvExpression::FieldAccess { Base, Field } = Callee.as_ref()
                    && matches!(Base.as_ref(), SdslvExpression::Identifier(name) if *name == specialization.Receiver)
                    && self
                        .FindShader(&specialization.Implementation)
                        .is_some_and(|shader| {
                            shader.Methods.iter().any(|method| method.Name == *Field)
                        })
                {
                    callee.Kind = Kind::Name(format!("{}_{Field}", specialization.Implementation));
                }
                Kind::Call {
                    Callee: Box::new(callee),
                    Arguments: values(Arguments)?,
                }
            }
            SdslvExpression::GenericCall { Arguments, .. } => {
                let [argument] = Arguments.as_slice() else {
                    return Err(Diagnostic("unsupported canonical generic call", span));
                };
                Kind::Convert(boxed(argument)?)
            }
            SdslvExpression::Construct {
                TypeName, Fields, ..
            } => {
                let name = PathText(TypeName);
                let fields = self
                    .FindAggregateFields(&name)
                    .ok_or_else(|| Diagnostic("unresolved aggregate constructor", span))?;
                let mut arguments = Vec::with_capacity(fields.len());
                for field in fields {
                    let initializer = Fields
                        .iter()
                        .find(|initializer| initializer.Field == field.Name)
                        .ok_or_else(|| Diagnostic("missing aggregate initializer", span))?;
                    arguments.push(lower(&initializer.Value)?);
                }
                Kind::Construct {
                    Aggregate: name,
                    Arguments: arguments,
                }
            }
            SdslvExpression::Binary {
                Left,
                Operator,
                Right,
            } => Kind::Binary {
                Left: boxed(Left)?,
                Operator: *Operator,
                Right: boxed(Right)?,
            },
            SdslvExpression::Unary { Operator, Operand } => Kind::Unary {
                Operator: *Operator,
                Operand: boxed(Operand)?,
            },
            SdslvExpression::With { Base, Updates } => Kind::With {
                Base: boxed(Base)?,
                Updates: Updates
                    .iter()
                    .map(|update| {
                        Ok(VdMirFieldUpdate {
                            Field: update.Field.clone(),
                            Value: lower(&update.Value)?,
                        })
                    })
                    .collect::<LowerResult<_>>()?,
            },
            SdslvExpression::Switch {
                Subject,
                Cases,
                ElseValue,
                ..
            } => Kind::Switch {
                Subject: Subject.as_deref().map(boxed).transpose()?,
                Cases: Cases
                    .iter()
                    .map(|case| {
                        Ok(VdMirSwitchCase {
                            Condition: lower(&case.Condition)?,
                            Value: lower(&case.Value)?,
                        })
                    })
                    .collect::<LowerResult<_>>()?,
                ElseValue: boxed(ElseValue)?,
            },
            SdslvExpression::Match { Subject, Arms, .. } => {
                let arms = Arms.iter().map(|arm| {
                    let SdslvMatchArmKind::EnumVariant { VariantPath } = &arm.Kind else {
                        return Err(Diagnostic(
                            "fallible match emission is not implemented in SDSL-V M65",
                            Some(arm.Span),
                        ));
                    };
                    let Some((variant, namespace)) = VariantPath.Segments.split_last() else {
                        return Err(Diagnostic("unresolved enum variant", Some(arm.Span)));
                    };
                    Ok(VdMirMatchArm {
                        Enum: namespace.join("_"),
                        Variant: variant.clone(),
                        Value: lower(&arm.Value)?,
                    })
                });
                Kind::Match {
                    Subject: boxed(Subject)?,
                    Arms: arms.collect::<LowerResult<_>>()?,
                }
            }
            SdslvExpression::WhenUtility {
                Options,
                Cases,
                ElseValue,
                ..
            } => {
                if Options.is_some() {
                    return Err(Diagnostic(
                        "stateful when utility options are not lowered in SDSL-V M66b",
                        span,
                    ));
                }
                Kind::WhenUtility {
                    Cases: Cases
                        .iter()
                        .map(|case| {
                            Ok(VdMirUtilityCase {
                                Guard: lower(&case.Guard)?,
                                Score: lower(&case.Score)?,
                                Value: lower(&case.Value)?,
                            })
                        })
                        .collect::<LowerResult<_>>()?,
                    ElseValue: boxed(ElseValue)?,
                }
            }
            SdslvExpression::TryPropagate { .. } | SdslvExpression::Unwrap { .. } => {
                return Err(FallibleDiagnostic(span));
            }
            SdslvExpression::ForeignHlsl { .. }
            | SdslvExpression::GuardedRead { .. }
            | SdslvExpression::MultiIndex { .. } => {
                return Err(Diagnostic(
                    "canonical expression is outside the VD-MIR lowering subset",
                    span,
                ));
            }
        };
        Ok(VdMirValue {
            Type: ty,
            Kind: kind,
            Span: span,
        })
    }

    fn LowerType(&self, ty: &SdslvTypeRef, span: Option<SdslvSpan>) -> LowerResult<VdMirType> {
        match ty {
            SdslvTypeRef::Named(path) => self.ResolveType(&PathText(path), 0, span),
            SdslvTypeRef::Array {
                Element, Length, ..
            } => Ok(VdMirType::Array {
                Element: Box::new(self.LowerType(Element, span)?),
                Length: *Length,
            }),
            SdslvTypeRef::Applied { .. } => Err(Diagnostic(
                &format!("HLSL lowering is unavailable for {}", ty.ToDisplayString()),
                span,
            )),
        }
    }

    fn ResolveType(
        &self,
        name: &str,
        depth: usize,
        span: Option<SdslvSpan>,
    ) -> LowerResult<VdMirType> {
        use VdMirScalar as Scalar;
        use VdMirType as Type;
        if depth > 8 {
            return Err(Diagnostic(
                &format!("type alias cycle or depth limit on '{name}'"),
                span,
            ));
        }
        let ty = match name {
            "void" => Type::Void,
            "bool" => Type::Scalar(Scalar::Bool),
            "i32" => Type::Scalar(Scalar::I32),
            "u32" => Type::Scalar(Scalar::U32),
            "f32" | "float" => Type::Scalar(Scalar::F32),
            "float2" => Type::FloatVector(2),
            "float3" => Type::FloatVector(3),
            "float4" => Type::FloatVector(4),
            "float4x4" => Type::Float4x4,
            _ => {
                if let Some(alias) = self.FindAlias(name) {
                    let physical =
                        self.ResolveType(&alias.TargetType.ToDisplayString(), depth + 1, span)?;
                    return Ok(match &alias.SpaceAnnotation {
                        Some(space) => Type::Nominal {
                            Space: PathText(space),
                            Physical: Box::new(physical),
                        },
                        None => physical,
                    });
                }
                if self.FindAggregateFields(name).is_some() || self.FindShader(name).is_some() {
                    Type::Aggregate(name.to_string())
                } else if self.IsEnum(name) {
                    Type::Enum(name.to_string())
                } else {
                    return Err(Diagnostic(
                        &format!("unknown type '{name}' in HLSL emission"),
                        span,
                    ));
                }
            }
        };
        Ok(ty)
    }

    fn ResolveInferredType(&self, ty: &TypeRef) -> VdMirType {
        match ty {
            TypeRef::Named(name) if name == "string" => VdMirType::String,
            TypeRef::Named(name) => self
                .ResolveType(name, 0, None)
                .unwrap_or(VdMirType::Unresolved),
            TypeRef::Array { Element, Length } => VdMirType::Array {
                Element: Box::new(self.ResolveInferredType(Element)),
                Length: *Length,
            },
            TypeRef::Unknown => VdMirType::Unresolved,
        }
    }

    fn FindAlias(&self, name: &str) -> Option<&SdslvTypeAliasDecl> {
        self.Module
            .Declarations
            .iter()
            .find_map(|declaration| match declaration {
                SdslvDecl::TypeAlias(alias) if alias.Name == name => Some(alias),
                _ => None,
            })
    }

    fn FindShader(&self, name: &str) -> Option<&SdslvShaderDecl> {
        self.Module
            .Declarations
            .iter()
            .find_map(|declaration| match declaration {
                SdslvDecl::Shader(shader) if shader.Name == name => Some(shader),
                _ => None,
            })
    }

    fn FindStream(&self, name: &str) -> Option<&SdslvStreamDecl> {
        self.Module
            .Declarations
            .iter()
            .find_map(|declaration| match declaration {
                SdslvDecl::Stream(stream) if stream.Name == name => Some(stream),
                _ => None,
            })
    }

    fn FindAggregateFields(&self, name: &str) -> Option<&[SdslvFieldDecl]> {
        self.Module
            .Declarations
            .iter()
            .find_map(|declaration| match declaration {
                SdslvDecl::Stream(stream) if stream.Name == name => Some(stream.Fields.as_slice()),
                SdslvDecl::Record(record) if record.Name == name => Some(record.Fields.as_slice()),
                _ => None,
            })
    }

    fn IsEnum(&self, name: &str) -> bool {
        self.Module
            .Declarations
            .iter()
            .any(|declaration| matches!(declaration, SdslvDecl::Enum(value) if value.Name == name))
    }
}

fn PathText(path: &SdslvPath) -> String {
    path.Segments.join(".")
}

fn ExpressionSpan(expression: &SdslvExpression) -> Option<SdslvSpan> {
    match expression {
        SdslvExpression::ArrayLiteral { Span, .. }
        | SdslvExpression::Index { Span, .. }
        | SdslvExpression::MultiIndex { Span, .. }
        | SdslvExpression::GenericCall { Span, .. }
        | SdslvExpression::Construct { Span, .. }
        | SdslvExpression::ForeignHlsl { Span, .. }
        | SdslvExpression::GuardedRead { Span, .. }
        | SdslvExpression::Switch { Span, .. }
        | SdslvExpression::Match { Span, .. }
        | SdslvExpression::WhenUtility { Span, .. }
        | SdslvExpression::TryPropagate { Span, .. }
        | SdslvExpression::Unwrap { Span, .. } => Some(*Span),
        _ => None,
    }
}

fn FallibleDiagnostic(span: Option<SdslvSpan>) -> SdslvDiagnostic {
    Diagnostic(
        "fallible function emission is not implemented in SDSL-V M58",
        span,
    )
}

fn Diagnostic(message: &str, span: Option<SdslvSpan>) -> SdslvDiagnostic {
    SdslvDiagnostic::New(
        message,
        span.unwrap_or(SdslvSpan {
            Start: 0,
            End: 0,
            Line: 1,
            Column: 1,
        }),
    )
}
