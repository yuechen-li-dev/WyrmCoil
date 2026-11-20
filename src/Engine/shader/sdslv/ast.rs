#![allow(non_snake_case)]

use super::token::SdslvSpan;
pub use super::vdmir::{SdslvBinaryOperator, SdslvUnaryOperator};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvPath {
    pub Segments: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SdslvTypeRef {
    Named(SdslvPath),
    Applied {
        Name: SdslvPath,
        Arguments: Vec<SdslvTypeRef>,
        Extents: Vec<SdslvExpression>,
        Span: SdslvSpan,
    },
    Array {
        Element: Box<SdslvTypeRef>,
        Length: usize,
        Span: SdslvSpan,
    },
}

impl SdslvTypeRef {
    pub fn Named(path: SdslvPath) -> Self {
        Self::Named(path)
    }
    pub fn AsNamedPath(&self) -> Option<&SdslvPath> {
        match self {
            Self::Named(path) => Some(path),
            Self::Array { .. } | Self::Applied { .. } => None,
        }
    }
    pub fn ToDisplayString(&self) -> String {
        match self {
            Self::Named(path) => path.Segments.join("."),
            Self::Array {
                Element, Length, ..
            } => {
                format!("array<{}, {}>", Element.ToDisplayString(), Length)
            }
            Self::Applied {
                Name,
                Arguments,
                Extents,
                ..
            } => {
                let mut arguments: Vec<String> =
                    Arguments.iter().map(Self::ToDisplayString).collect();
                if !Extents.is_empty() {
                    arguments.push("extent expression".to_string());
                }
                format!("{}<{}>", Name.Segments.join("."), arguments.join(", "))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvModule {
    pub Namespace: Option<SdslvPath>,
    pub Uses: Vec<SdslvUseDecl>,
    pub Imports: Vec<SdslvImportDecl>,
    pub Exports: Vec<SdslvExportDecl>,
    pub Canonical: bool,
    pub Declarations: Vec<SdslvDecl>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvImportDecl {
    pub Path: String,
    pub Relative: bool,
    pub Alias: Option<String>,
    pub Span: SdslvSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvExportDecl {
    pub DeclarationIndex: usize,
    pub Span: SdslvSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvTestModule {
    pub Namespace: Option<SdslvPath>,
    pub Uses: Vec<SdslvUseDecl>,
    pub Tests: Vec<SdslvTestFunction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvTestFunction {
    pub Attributes: Vec<SdslvAttribute>,
    pub Name: String,
    pub Parameters: Vec<SdslvFunctionParameter>,
    pub Body: SdslvBody,
    pub Span: SdslvSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvAttribute {
    pub Name: String,
    pub Arguments: Vec<SdslvExpression>,
    pub Span: SdslvSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvUseDecl {
    pub Path: SdslvPath,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SdslvDecl {
    TypeAlias(SdslvTypeAliasDecl),
    Stream(SdslvStreamDecl),
    Record(SdslvRecordDecl),
    Board(SdslvRecordDecl),
    Interface(SdslvInterfaceDecl),
    Shader(SdslvShaderDecl),
    Flow(SdslvFlowDecl),
    Compile(SdslvCompileDecl),
    Enum(SdslvEnumDecl),
    Concept(SdslvConceptDecl),
    Config(SdslvConfigDecl),
    Function(SdslvFunctionDecl),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvConceptDecl {
    pub Name: String,
    pub Members: Vec<SdslvConceptMember>,
    pub Requirements: Vec<SdslvExpression>,
    pub Span: SdslvSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SdslvConceptMember {
    Field {
        Field: SdslvFieldDecl,
        Default: Option<SdslvExpression>,
        ZeroAllowed: bool,
    },
    Group {
        Name: String,
        Members: Vec<SdslvConceptMember>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvConfigDecl {
    pub Name: String,
    pub Concept: SdslvPath,
    pub Fields: Vec<SdslvConfigField>,
    pub Requirements: Vec<SdslvExpression>,
    pub Span: SdslvSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvConfigField {
    pub Path: SdslvPath,
    pub Value: SdslvExpression,
    pub Span: SdslvSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvEnumDecl {
    pub Name: String,
    pub Variants: Vec<SdslvEnumVariant>,
    pub Span: SdslvSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvEnumVariant {
    pub Name: String,
    pub Payload: Option<Vec<SdslvFieldDecl>>,
    pub Span: SdslvSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvFlowDecl {
    pub Name: String,
    pub Parameters: Vec<SdslvFunctionParameter>,
    pub ReturnType: SdslvTypeRef,
    pub Board: Option<SdslvFlowBoard>,
    pub States: Vec<SdslvFlowState>,
    pub Span: SdslvSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvFlowBoard {
    pub Fields: Vec<SdslvFlowBoardField>,
    pub Span: SdslvSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvFlowBoardField {
    pub Name: String,
    pub TypeName: SdslvTypeRef,
    pub Span: SdslvSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvFlowState {
    pub Name: String,
    pub Statements: Vec<SdslvFlowStatement>,
    pub Span: SdslvSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SdslvFlowStatement {
    When(SdslvFlowWhen),
    Goto(SdslvPath),
    Return(SdslvExpression),
    BoardAssign {
        Field: String,
        Value: SdslvExpression,
        Span: SdslvSpan,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvFlowWhen {
    pub Cases: Vec<SdslvFlowCase>,
    pub ElseAction: Option<SdslvFlowAction>,
    pub Span: SdslvSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvFlowCase {
    pub Condition: SdslvExpression,
    pub Action: SdslvFlowAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SdslvFlowAction {
    Goto(SdslvPath),
    Return(SdslvExpression),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvCompileDecl {
    pub GenericShader: SdslvPath,
    pub TypeArguments: Vec<SdslvTypeRef>,
    pub Alias: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvTypeAliasDecl {
    pub Name: String,
    pub TargetType: SdslvTypeRef,
    pub SpaceAnnotation: Option<SdslvPath>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvFieldDecl {
    pub Name: String,
    pub TypeName: SdslvTypeRef,
    pub Attributes: Vec<SdslvAttribute>,
    pub Access: Option<SdslvResourceAccess>,
    pub Span: SdslvSpan,
    pub TypeSpan: SdslvSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SdslvResourceAccess {
    Readonly,
    Readwrite,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvStreamDecl {
    pub Name: String,
    pub Fields: Vec<SdslvFieldDecl>,
    pub Span: SdslvSpan,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvRecordDecl {
    pub Name: String,
    pub Fields: Vec<SdslvFieldDecl>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvInterfaceDecl {
    pub Name: String,
    pub Methods: Vec<SdslvFunctionDecl>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvShaderDecl {
    pub Name: String,
    pub GenericParameters: Vec<String>,
    pub Implements: Vec<SdslvPath>,
    pub Constraints: Vec<SdslvWhereConstraint>,
    pub MaterialFields: Vec<SdslvFieldDecl>,
    pub Methods: Vec<SdslvFunctionDecl>,
    pub StageMethods: Vec<SdslvFunctionDecl>,
    pub Template: Option<SdslvTemplateParam>,
    pub Resources: Option<SdslvResourceDecl>,
    pub Workgroups: Vec<SdslvFieldDecl>,
    pub StaticAsserts: Vec<SdslvExpression>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvTemplateParam {
    pub Name: String,
    pub Concept: SdslvPath,
    pub Span: SdslvSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SdslvResourceDecl {
    Stream(SdslvPath),
    Inline(Vec<SdslvFieldDecl>),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvWhereConstraint {
    pub ParameterName: String,
    pub Bounds: Vec<SdslvPath>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvFunctionDecl {
    pub Attributes: Vec<SdslvAttribute>,
    pub IsOverride: bool,
    pub Stage: Option<String>,
    pub Name: String,
    pub Parameters: Vec<SdslvFunctionParameter>,
    pub ReturnType: SdslvTypeRef,
    pub ReturnTypeSpan: SdslvSpan,
    pub ErrorType: Option<SdslvTypeRef>,
    pub Body: Option<SdslvBody>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvFunctionParameter {
    pub Name: String,
    pub TypeName: SdslvTypeRef,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvBody {
    pub Span: SdslvSpan,
    pub Statements: Vec<SdslvStatement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SdslvStatement {
    Let {
        Name: String,
        TypeName: SdslvTypeRef,
        Initializer: Option<SdslvExpression>,
        Mutable: bool,
        Span: SdslvSpan,
    },
    Assign {
        Target: SdslvExpression,
        Value: SdslvExpression,
        Span: SdslvSpan,
    },
    Return {
        Value: SdslvExpression,
    },
    ReturnVoid,
    Comptime {
        Statement: Box<SdslvStatement>,
        Span: SdslvSpan,
    },
    StaticAssert {
        Condition: SdslvExpression,
        Span: SdslvSpan,
    },
    InlineFlow {
        Name: String,
        Boards: Vec<SdslvFlowBoardBinding>,
        States: Vec<SdslvInlineState>,
        Span: SdslvSpan,
    },
    FlowTransition {
        Transition: SdslvFlowTransition,
        Span: SdslvSpan,
    },
    GuardedWrite {
        Target: SdslvExpression,
        Value: SdslvExpression,
        Condition: SdslvExpression,
        Span: SdslvSpan,
    },
    GuardWhen {
        Cases: Vec<SdslvGuardCase>,
        ElseBody: Option<SdslvBody>,
        Span: SdslvSpan,
    },
    If {
        Condition: SdslvExpression,
        ThenBody: Vec<SdslvStatement>,
        ElseBody: Option<Vec<SdslvStatement>>,
        Span: SdslvSpan,
    },
    For {
        Iterator: String,
        Start: SdslvExpression,
        End: SdslvExpression,
        Step: Option<SdslvExpression>,
        Body: Vec<SdslvStatement>,
        Span: SdslvSpan,
    },
    Expression {
        Value: SdslvExpression,
    },
    Empty,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvInlineState {
    pub Name: String,
    pub Body: SdslvBody,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvFlowBoardBinding {
    pub Name: String,
    pub TypeName: SdslvTypeRef,
    pub Initializer: SdslvExpression,
    pub Span: SdslvSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SdslvFlowTransition {
    Goto(String),
    Push(String),
    Pop,
    Finish,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvGuardCase {
    pub Condition: SdslvExpression,
    pub Body: SdslvBody,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SdslvExpression {
    Identifier(String),
    IntegerLiteral(String),
    FloatLiteral(String),
    StringLiteral(String),
    BoolLiteral(bool),
    ArrayLiteral {
        Elements: Vec<SdslvExpression>,
        Span: SdslvSpan,
    },
    FieldAccess {
        Base: Box<SdslvExpression>,
        Field: String,
    },
    Index {
        Base: Box<SdslvExpression>,
        Index: Box<SdslvExpression>,
        Span: SdslvSpan,
    },
    MultiIndex {
        Base: Box<SdslvExpression>,
        Indices: Vec<SdslvExpression>,
        Span: SdslvSpan,
    },
    Call {
        Callee: Box<SdslvExpression>,
        Arguments: Vec<SdslvExpression>,
    },
    GenericCall {
        Callee: Box<SdslvExpression>,
        TypeArguments: Vec<SdslvTypeRef>,
        Arguments: Vec<SdslvExpression>,
        Span: SdslvSpan,
    },
    Construct {
        TypeName: SdslvPath,
        Fields: Vec<SdslvWithUpdate>,
        Span: SdslvSpan,
    },
    ForeignHlsl {
        ResultType: SdslvTypeRef,
        Captures: Vec<String>,
        Source: String,
        Span: SdslvSpan,
    },
    GuardedRead {
        Target: Box<SdslvExpression>,
        Condition: Box<SdslvExpression>,
        Fallback: Box<SdslvExpression>,
        Span: SdslvSpan,
    },
    Binary {
        Left: Box<SdslvExpression>,
        Operator: SdslvBinaryOperator,
        Right: Box<SdslvExpression>,
    },
    Unary {
        Operator: SdslvUnaryOperator,
        Operand: Box<SdslvExpression>,
    },
    With {
        Base: Box<SdslvExpression>,
        Updates: Vec<SdslvWithUpdate>,
    },
    Switch {
        Subject: Option<Box<SdslvExpression>>,
        Cases: Vec<SdslvSwitchCase>,
        ElseValue: Box<SdslvExpression>,
        Span: SdslvSpan,
    },
    Match {
        Subject: Box<SdslvExpression>,
        Arms: Vec<SdslvMatchArm>,
        Span: SdslvSpan,
    },
    WhenUtility {
        Options: Option<Box<SdslvUtilityOptions>>,
        Cases: Vec<SdslvUtilityCase>,
        ElseValue: Box<SdslvExpression>,
        Span: SdslvSpan,
    },
    TryPropagate {
        Expression: Box<SdslvExpression>,
        Span: SdslvSpan,
    },
    Unwrap {
        Expression: Box<SdslvExpression>,
        Span: SdslvSpan,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvSwitchCase {
    pub Condition: SdslvExpression,
    pub Value: SdslvExpression,
    pub Span: SdslvSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvUtilityOptions {
    pub Hysteresis: Option<SdslvExpression>,
    pub MinCommit: Option<SdslvExpression>,
    pub Span: SdslvSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvUtilityCase {
    pub Value: SdslvExpression,
    pub Guard: SdslvExpression,
    pub Score: SdslvExpression,
    pub Span: SdslvSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvWithUpdate {
    pub Field: String,
    pub Value: SdslvExpression,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvMatchArm {
    pub Kind: SdslvMatchArmKind,
    pub Value: SdslvExpression,
    pub Span: SdslvSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SdslvMatchArmKind {
    EnumVariant {
        VariantPath: SdslvPath,
    },
    PayloadVariant {
        VariantPath: SdslvPath,
        Binding: String,
    },
    FallibleOk {
        Binding: String,
    },
    FallibleErr {
        Binding: String,
    },
}
