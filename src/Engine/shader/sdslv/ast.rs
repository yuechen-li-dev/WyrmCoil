#![allow(non_snake_case)]

use super::token::SdslvSpan;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvPath {
    pub Segments: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvModule {
    pub Namespace: Option<SdslvPath>,
    pub Uses: Vec<SdslvUseDecl>,
    pub Declarations: Vec<SdslvDecl>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvUseDecl {
    pub Path: SdslvPath,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SdslvDecl {
    TypeAlias(SdslvTypeAliasDecl),
    Stream(SdslvStreamDecl),
    Interface(SdslvInterfaceDecl),
    Shader(SdslvShaderDecl),
    Compile(SdslvCompileDecl),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvCompileDecl {
    pub GenericShader: SdslvPath,
    pub TypeArguments: Vec<SdslvPath>,
    pub Alias: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvTypeAliasDecl {
    pub Name: String,
    pub TargetType: SdslvPath,
    pub SpaceAnnotation: Option<SdslvPath>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvFieldDecl {
    pub Name: String,
    pub TypeName: SdslvPath,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvStreamDecl {
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
    pub ReturnType: SdslvPath,
    pub Body: Option<SdslvBody>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvFunctionParameter {
    pub Name: String,
    pub TypeName: SdslvPath,
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
        TypeName: SdslvPath,
        Initializer: Option<SdslvExpression>,
    },
    Assign {
        Target: SdslvExpression,
        Value: SdslvExpression,
    },
    Return {
        Value: SdslvExpression,
    },
    Empty,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SdslvExpression {
    Identifier(String),
    IntegerLiteral(String),
    FloatLiteral(String),
    BoolLiteral(bool),
    FieldAccess {
        Base: Box<SdslvExpression>,
        Field: String,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SdslvBinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SdslvUnaryOperator {
    Negate,
}
