#![allow(non_snake_case)]

use super::token::SdslvSpan;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvPath {
    pub Segments: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SdslvTypeRef {
    Named(SdslvPath),
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
            Self::Array { .. } => None,
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
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvModule {
    pub Namespace: Option<SdslvPath>,
    pub Uses: Vec<SdslvUseDecl>,
    pub Declarations: Vec<SdslvDecl>,
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
    Interface(SdslvInterfaceDecl),
    Shader(SdslvShaderDecl),
    Flow(SdslvFlowDecl),
    Compile(SdslvCompileDecl),
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
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvStreamDecl {
    pub Name: String,
    pub Fields: Vec<SdslvFieldDecl>,
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
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvWhereConstraint {
    pub ParameterName: String,
    pub Bounds: Vec<SdslvPath>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvFunctionDecl {
    pub IsOverride: bool,
    pub Stage: Option<String>,
    pub Name: String,
    pub Parameters: Vec<SdslvFunctionParameter>,
    pub ReturnType: SdslvTypeRef,
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
    },
    Assign {
        Target: SdslvExpression,
        Value: SdslvExpression,
    },
    Return {
        Value: SdslvExpression,
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
pub enum SdslvExpression {
    Identifier(String),
    IntegerLiteral(String),
    FloatLiteral(String),
    StringLiteral(String),
    BoolLiteral(bool),
    FieldAccess {
        Base: Box<SdslvExpression>,
        Field: String,
    },
    Index {
        Base: Box<SdslvExpression>,
        Index: Box<SdslvExpression>,
        Span: SdslvSpan,
    },
    Call {
        Callee: Box<SdslvExpression>,
        Arguments: Vec<SdslvExpression>,
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
pub struct SdslvWithUpdate {
    pub Field: String,
    pub Value: SdslvExpression,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SdslvBinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SdslvUnaryOperator {
    Negate,
}
