//! Owned, validated SDSL-V programs consumed by the shader backend.

use super::token::SdslvSpan;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VdMirModule {
    pub(super) Namespace: Option<String>,
    pub(super) Items: Vec<VdMirItem>,
}

impl VdMirModule {
    pub fn Namespace(&self) -> Option<&str> {
        self.Namespace.as_deref()
    }

    pub fn Items(&self) -> &[VdMirItem] {
        &self.Items
    }

    pub fn EntryPoints(&self) -> impl Iterator<Item = (&VdMirFunction, &VdMirEntry)> {
        self.Items.iter().filter_map(|item| {
            let VdMirItem::Function(function) = item else {
                return None;
            };
            Some((function, function.Entry.as_ref()?))
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VdMirItem {
    TypeAlias {
        Name: String,
        Target: VdMirType,
        Space: Option<String>,
    },
    Aggregate(VdMirAggregate),
    Enum {
        Name: String,
        Variants: Vec<String>,
    },
    LegacyInterface(String),
    StorageBinding(VdMirStorageBinding),
    Function(VdMirFunction),
    Flow(VdMirFlow),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VdMirScalar {
    Bool,
    I32,
    U32,
    F32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VdMirType {
    Void,
    Scalar(VdMirScalar),
    FloatVector(u8),
    UintVector(u8),
    Float4x4,
    Array {
        Element: Box<VdMirType>,
        Length: usize,
    },
    StorageArray {
        Element: VdMirScalar,
        Writable: bool,
    },
    Nominal {
        Space: String,
        Physical: Box<VdMirType>,
    },
    Aggregate(String),
    Enum(String),
    String,
    /// Type information that the legacy frontend leaves to the backend.
    Unresolved,
}

impl VdMirType {
    pub(super) fn Physical(&self) -> &VdMirType {
        match self {
            Self::Nominal { Physical, .. } => Physical.Physical(),
            _ => self,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VdMirAggregate {
    pub Name: String,
    pub Fields: Vec<VdMirField>,
    pub HasConstructor: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VdMirField {
    pub Name: String,
    pub Type: VdMirType,
    pub Role: Option<VdMirFieldRole>,
    pub Span: SdslvSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VdMirFieldRole {
    Location(u32, VdMirInterpolation),
    Target(u32),
    Builtin(VdMirBuiltin),
    ClipPosition,
    LegacyLocation(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VdMirInterpolation {
    Linear,
    Flat,
    NoPerspective,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum VdMirBuiltin {
    VertexId,
    InstanceId,
    Position,
    FrontFace,
    DispatchId,
    GroupId,
    GroupThreadId,
    GroupIndex,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VdMirStorageBinding {
    pub Name: String,
    pub Binding: u32,
    pub Element: VdMirScalar,
    pub Writable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VdMirFunction {
    pub Name: String,
    pub Entry: Option<VdMirEntry>,
    pub Parameters: Vec<VdMirParameter>,
    pub ReturnType: VdMirType,
    pub Body: Vec<VdMirStatement>,
    pub Span: Option<SdslvSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VdMirEntry {
    pub Program: String,
    pub Method: String,
    pub Kind: VdMirEntryKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VdMirEntryKind {
    Vertex,
    Pixel(VdMirPixelOutput),
    Compute(Option<[u32; 3]>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VdMirPixelOutput {
    Scalar,
    Stream,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VdMirParameter {
    pub Name: String,
    pub Type: VdMirType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VdMirStatement {
    Let {
        Name: String,
        Type: VdMirType,
        Initializer: Option<VdMirValue>,
    },
    Assign {
        Target: VdMirValue,
        Value: VdMirValue,
    },
    Return {
        Value: VdMirValue,
    },
    ReturnVoid,
    If {
        Condition: VdMirValue,
        ThenBody: Vec<VdMirStatement>,
        ElseBody: Option<Vec<VdMirStatement>>,
    },
    For {
        Iterator: String,
        Start: VdMirValue,
        End: VdMirValue,
        Step: Option<VdMirValue>,
        Body: Vec<VdMirStatement>,
    },
    Expression {
        Value: VdMirValue,
    },
    Empty,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VdMirValue {
    pub Type: VdMirType,
    pub Kind: VdMirValueKind,
    pub Span: Option<SdslvSpan>,
}

impl VdMirValue {
    pub(super) fn AnyInTree(&self, predicate: fn(&VdMirValueKind) -> bool) -> bool {
        use VdMirValueKind::*;
        if predicate(&self.Kind) {
            return true;
        }
        let any = |value: &VdMirValue| value.AnyInTree(predicate);
        match &self.Kind {
            ArrayLiteral { Elements } => Elements.iter().any(any),
            FieldAccess { Base, .. } | Unary { Operand: Base, .. } | Convert(Base) => any(Base),
            Index { Base, Index } => any(Base) || any(Index),
            Call { Callee, Arguments } => any(Callee) || Arguments.iter().any(any),
            Construct { Arguments, .. } => Arguments.iter().any(any),
            Binary { Left, Right, .. } => any(Left) || any(Right),
            With { Base, Updates } => any(Base) || Updates.iter().any(|update| any(&update.Value)),
            Switch {
                Subject,
                Cases,
                ElseValue,
            } => {
                Subject.as_deref().is_some_and(any)
                    || Cases
                        .iter()
                        .any(|case| any(&case.Condition) || any(&case.Value))
                    || any(ElseValue)
            }
            Match { Subject, Arms } => any(Subject) || Arms.iter().any(|arm| any(&arm.Value)),
            WhenUtility { Cases, ElseValue } => {
                Cases
                    .iter()
                    .any(|case| any(&case.Guard) || any(&case.Score) || any(&case.Value))
                    || any(ElseValue)
            }
            Name(_)
            | IntegerLiteral(_)
            | FloatLiteral(_)
            | StringLiteral(_)
            | BoolLiteral(_)
            | EnumVariant { .. } => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VdMirValueKind {
    Name(String),
    IntegerLiteral(String),
    FloatLiteral(String),
    StringLiteral(String),
    BoolLiteral(bool),
    ArrayLiteral {
        Elements: Vec<VdMirValue>,
    },
    FieldAccess {
        Base: Box<VdMirValue>,
        Field: String,
    },
    Index {
        Base: Box<VdMirValue>,
        Index: Box<VdMirValue>,
    },
    Call {
        Callee: Box<VdMirValue>,
        Arguments: Vec<VdMirValue>,
    },
    Convert(Box<VdMirValue>),
    Construct {
        Aggregate: String,
        Arguments: Vec<VdMirValue>,
    },
    EnumVariant {
        Enum: String,
        Variant: String,
    },
    Binary {
        Left: Box<VdMirValue>,
        Operator: SdslvBinaryOperator,
        Right: Box<VdMirValue>,
    },
    Unary {
        Operator: SdslvUnaryOperator,
        Operand: Box<VdMirValue>,
    },
    With {
        Base: Box<VdMirValue>,
        Updates: Vec<VdMirFieldUpdate>,
    },
    Switch {
        Subject: Option<Box<VdMirValue>>,
        Cases: Vec<VdMirSwitchCase>,
        ElseValue: Box<VdMirValue>,
    },
    Match {
        Subject: Box<VdMirValue>,
        Arms: Vec<VdMirMatchArm>,
    },
    WhenUtility {
        Cases: Vec<VdMirUtilityCase>,
        ElseValue: Box<VdMirValue>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VdMirFieldUpdate {
    pub Field: String,
    pub Value: VdMirValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VdMirSwitchCase {
    pub Condition: VdMirValue,
    pub Value: VdMirValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VdMirMatchArm {
    pub Enum: String,
    pub Variant: String,
    pub Value: VdMirValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VdMirUtilityCase {
    pub Guard: VdMirValue,
    pub Score: VdMirValue,
    pub Value: VdMirValue,
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
    Or,
    And,
    BitOr,
    BitXor,
    BitAnd,
    ShiftLeft,
    ShiftRight,
    Remainder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SdslvUnaryOperator {
    Negate,
    Not,
    BitNot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VdMirFlow {
    pub Name: String,
    pub Parameters: Vec<VdMirParameter>,
    pub ReturnType: VdMirType,
    pub Board: Vec<VdMirField>,
    pub States: Vec<VdMirFlowState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VdMirFlowState {
    pub Name: String,
    pub Span: SdslvSpan,
    pub Writes: Vec<VdMirFieldUpdate>,
    pub Terminator: VdMirFlowTerminator,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VdMirFlowTerminator {
    Return(VdMirValue),
    Goto(usize),
    Branch {
        Cases: Vec<(VdMirValue, VdMirFlowExit)>,
        Fallback: VdMirFlowExit,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VdMirFlowExit {
    Return(VdMirValue),
    Goto(usize),
}
