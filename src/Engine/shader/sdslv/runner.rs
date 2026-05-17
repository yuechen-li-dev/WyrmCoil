#![allow(non_snake_case)]

use std::collections::HashMap;

use super::ast::*;
use super::diagnostic::SdslvDiagnostic;
use super::parser::ParseTestSource;
use super::token::SdslvSpan;
use super::validation::ValidateTestSource;

#[derive(Debug, Clone, PartialEq)]
pub struct SdslvTestRunResult {
    pub Passed: bool,
    pub Diagnostics: Vec<SdslvDiagnostic>,
    pub Tests: Vec<SdslvTestCaseResult>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SdslvTestCaseResult {
    pub Name: String,
    pub Passed: bool,
    pub Failures: Vec<SdslvAssertFailure>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdslvAssertFailure {
    pub Message: String,
    pub Span: Option<SdslvSpan>,
}

#[derive(Debug, Clone, PartialEq)]
enum RuntimeValue {
    Bool(bool),
    I32(i32),
    F32(f32),
    String(String),
}

pub fn RunTestSource(source: &str) -> SdslvTestRunResult {
    let parsed = ParseTestSource(source);
    if let Err(diagnostics) = parsed {
        return SdslvTestRunResult {
            Passed: false,
            Diagnostics: diagnostics,
            Tests: vec![],
        };
    }
    let validated = ValidateTestSource(source);
    if let Err(diagnostics) = validated {
        return SdslvTestRunResult {
            Passed: false,
            Diagnostics: diagnostics,
            Tests: vec![],
        };
    }
    let module = validated.unwrap();
    RunTests(&module)
}

pub fn RunTests(module: &SdslvTestModule) -> SdslvTestRunResult {
    let mut results = vec![];
    for test in &module.Tests {
        results.push(run_test_case(test));
    }
    let passed = results.iter().all(|x| x.Passed);
    SdslvTestRunResult {
        Passed: passed,
        Diagnostics: vec![],
        Tests: results,
    }
}

fn run_test_case(test: &SdslvTestFunction) -> SdslvTestCaseResult {
    let mut env = HashMap::<String, RuntimeValue>::new();
    let mut failures = vec![];
    let mut fatal = false;
    for statement in &test.Body.Statements {
        if fatal {
            break;
        }
        if let Err(failure) = execute_statement(statement, &mut env, &mut failures) {
            failures.push(failure);
            fatal = true;
        }
    }
    SdslvTestCaseResult {
        Name: test.Name.clone(),
        Passed: failures.is_empty(),
        Failures: failures,
    }
}

fn execute_statement(
    statement: &SdslvStatement,
    env: &mut HashMap<String, RuntimeValue>,
    failures: &mut Vec<SdslvAssertFailure>,
) -> Result<(), SdslvAssertFailure> {
    match statement {
        SdslvStatement::Empty => Ok(()),
        SdslvStatement::Let {
            Name,
            TypeName,
            Initializer,
        } => {
            let value = if let Some(initializer) = Initializer {
                EvalExpression(initializer, env)?
            } else {
                DefaultValueForType(TypeName).ok_or_else(|| SdslvAssertFailure {
                    Message: format!("let '{Name}' requires initializer or supported default type"),
                    Span: None,
                })?
            };
            env.insert(Name.clone(), value);
            Ok(())
        }
        SdslvStatement::Assign { Target, Value } => {
            let SdslvExpression::Identifier(name) = Target else {
                return Err(SdslvAssertFailure {
                    Message: "unsupported assignment target in SDSL-V M7b runner".to_string(),
                    Span: None,
                });
            };
            if !env.contains_key(name) {
                return Err(SdslvAssertFailure {
                    Message: format!("assignment to unknown local '{name}'"),
                    Span: None,
                });
            }
            let value = EvalExpression(Value, env)?;
            env.insert(name.clone(), value);
            Ok(())
        }
        SdslvStatement::Expression { Value } => ExecuteAssertExpression(Value, env, failures),
        SdslvStatement::Return { .. } => Err(SdslvAssertFailure {
            Message: "return statement is not supported in SDSL-V M7b test execution".to_string(),
            Span: None,
        }),
        SdslvStatement::If { .. } => Err(SdslvAssertFailure {
            Message: "if statement is not supported in SDSL-V M55c test execution".to_string(),
            Span: None,
        }),
        SdslvStatement::For { .. } => Err(SdslvAssertFailure {
            Message: "for statement is not supported in SDSL-V M57 test execution".to_string(),
            Span: None,
        }),
    }
}

fn ExecuteAssertExpression(
    expr: &SdslvExpression,
    env: &mut HashMap<String, RuntimeValue>,
    failures: &mut Vec<SdslvAssertFailure>,
) -> Result<(), SdslvAssertFailure> {
    let (method, arguments) = AssertCallParts(expr)?;
    let custom_message = GetMessageArgument(arguments.last())?;
    match method {
        "True" => {
            let condition = EvalExpression(&arguments[0], env)?;
            if !matches!(condition, RuntimeValue::Bool(true)) {
                failures.push(SdslvAssertFailure {
                    Message: format!("Assert.True failed: {custom_message}"),
                    Span: None,
                });
            }
            Ok(())
        }
        "Equals" => {
            let actual = EvalExpression(&arguments[0], env)?;
            let expected = EvalExpression(&arguments[1], env)?;
            if !RuntimeEquals(&actual, &expected) {
                failures.push(SdslvAssertFailure {
                    Message: format!("Assert.Equals failed: {custom_message}"),
                    Span: None,
                });
            }
            Ok(())
        }
        "Near" => {
            let actual = EvalExpression(&arguments[0], env)?;
            let expected = EvalExpression(&arguments[1], env)?;
            let tolerance = EvalExpression(&arguments[2], env)?;
            let (a, e, t) = (AsF32(&actual)?, AsF32(&expected)?, AsF32(&tolerance)?);
            if (a - e).abs() > t {
                failures.push(SdslvAssertFailure {
                    Message: format!("Assert.Near failed: {custom_message}"),
                    Span: None,
                });
            }
            Ok(())
        }
        _ => Err(SdslvAssertFailure {
            Message: format!("unsupported Assert method 'Assert.{method}' in SDSL-V M7b runner"),
            Span: None,
        }),
    }
}

fn AssertCallParts(
    expr: &SdslvExpression,
) -> Result<(&str, &Vec<SdslvExpression>), SdslvAssertFailure> {
    let SdslvExpression::Call { Callee, Arguments } = expr else {
        return Err(SdslvAssertFailure {
            Message: "unsupported non-call expression statement in SDSL-V M7b runner".to_string(),
            Span: None,
        });
    };
    let SdslvExpression::FieldAccess { Base, Field } = Callee.as_ref() else {
        return Err(SdslvAssertFailure {
            Message: "unsupported non-Assert expression statement in SDSL-V M7b runner".to_string(),
            Span: None,
        });
    };
    let SdslvExpression::Identifier(base) = Base.as_ref() else {
        return Err(SdslvAssertFailure {
            Message: "unsupported non-Assert expression statement in SDSL-V M7b runner".to_string(),
            Span: None,
        });
    };
    if base != "Assert" {
        return Err(SdslvAssertFailure {
            Message: "unsupported non-Assert expression statement in SDSL-V M7b runner".to_string(),
            Span: None,
        });
    }
    Ok((Field.as_str(), Arguments))
}

fn GetMessageArgument(argument: Option<&SdslvExpression>) -> Result<String, SdslvAssertFailure> {
    if let Some(SdslvExpression::StringLiteral(message)) = argument {
        Ok(message.clone())
    } else {
        Err(SdslvAssertFailure {
            Message: "assert custom message must be a string literal".to_string(),
            Span: None,
        })
    }
}

fn EvalExpression(
    expression: &SdslvExpression,
    env: &HashMap<String, RuntimeValue>,
) -> Result<RuntimeValue, SdslvAssertFailure> {
    match expression {
        SdslvExpression::BoolLiteral(x) => Ok(RuntimeValue::Bool(*x)),
        SdslvExpression::IntegerLiteral(x) => {
            x.parse::<i32>()
                .map(RuntimeValue::I32)
                .map_err(|_| SdslvAssertFailure {
                    Message: format!("invalid integer literal '{x}'"),
                    Span: None,
                })
        }
        SdslvExpression::FloatLiteral(x) => {
            x.parse::<f32>()
                .map(RuntimeValue::F32)
                .map_err(|_| SdslvAssertFailure {
                    Message: format!("invalid float literal '{x}'"),
                    Span: None,
                })
        }
        SdslvExpression::StringLiteral(x) => Ok(RuntimeValue::String(x.clone())),
        SdslvExpression::Identifier(name) => {
            env.get(name).cloned().ok_or_else(|| SdslvAssertFailure {
                Message: format!("unknown local '{name}'"),
                Span: None,
            })
        }
        SdslvExpression::Unary {
            Operator: SdslvUnaryOperator::Negate,
            Operand,
        } => {
            let value = EvalExpression(Operand, env)?;
            match value {
                RuntimeValue::I32(x) => Ok(RuntimeValue::I32(-x)),
                RuntimeValue::F32(x) => Ok(RuntimeValue::F32(-x)),
                _ => Err(SdslvAssertFailure {
                    Message: "unary minus requires numeric operand".to_string(),
                    Span: None,
                }),
            }
        }
        SdslvExpression::Binary {
            Left,
            Operator,
            Right,
        } => {
            let left = EvalExpression(Left, env)?;
            let right = EvalExpression(Right, env)?;
            EvalBinary(&left, *Operator, &right)
        }
        SdslvExpression::Call { Callee, Arguments } => EvalCall(Callee, Arguments, env),
        SdslvExpression::FieldAccess { .. } => Err(SdslvAssertFailure {
            Message: "field access is not supported in SDSL-V M7b expression evaluation"
                .to_string(),
            Span: None,
        }),
        SdslvExpression::Index { .. } => Err(SdslvAssertFailure {
            Message: "array indexing is not supported in SDSL-V test expression evaluation"
                .to_string(),
            Span: None,
        }),
        SdslvExpression::ArrayLiteral { .. } => Err(SdslvAssertFailure {
            Message: "array literals are not supported in SDSL-V test expression evaluation"
                .to_string(),
            Span: None,
        }),
        SdslvExpression::With { .. } => Err(SdslvAssertFailure {
            Message: "with expression is not supported in SDSL-V M7b expression evaluation"
                .to_string(),
            Span: None,
        }),
        SdslvExpression::Switch { .. } => Err(SdslvAssertFailure {
            Message: "switch expression is not supported in SDSL-V M55c test execution".to_string(),
            Span: None,
        }),
        SdslvExpression::Match { .. } => Err(SdslvAssertFailure {
            Message: "match expression is not supported in SDSL-V M64 test execution".to_string(),
            Span: None,
        }),
        SdslvExpression::TryPropagate { .. } | SdslvExpression::Unwrap { .. } => {
            Err(SdslvAssertFailure {
                Message: "fallible expressions are not supported in SDSL-V M58 test execution"
                    .to_string(),
                Span: None,
            })
        }
    }
}

fn EvalBinary(
    left: &RuntimeValue,
    operator: SdslvBinaryOperator,
    right: &RuntimeValue,
) -> Result<RuntimeValue, SdslvAssertFailure> {
    match operator {
        SdslvBinaryOperator::Add => NumericBinary(left, right, |a, b| a + b, |a, b| a + b),
        SdslvBinaryOperator::Subtract => NumericBinary(left, right, |a, b| a - b, |a, b| a - b),
        SdslvBinaryOperator::Multiply => NumericBinary(left, right, |a, b| a * b, |a, b| a * b),
        SdslvBinaryOperator::Divide => NumericBinary(left, right, |a, b| a / b, |a, b| a / b),
        SdslvBinaryOperator::Equal => Ok(RuntimeValue::Bool(RuntimeEquals(left, right))),
        SdslvBinaryOperator::NotEqual => Ok(RuntimeValue::Bool(!RuntimeEquals(left, right))),
        SdslvBinaryOperator::Less => CompareNumeric(left, right, |a, b| a < b),
        SdslvBinaryOperator::LessEqual => CompareNumeric(left, right, |a, b| a <= b),
        SdslvBinaryOperator::Greater => CompareNumeric(left, right, |a, b| a > b),
        SdslvBinaryOperator::GreaterEqual => CompareNumeric(left, right, |a, b| a >= b),
    }
}

fn EvalCall(
    callee: &SdslvExpression,
    arguments: &Vec<SdslvExpression>,
    env: &HashMap<String, RuntimeValue>,
) -> Result<RuntimeValue, SdslvAssertFailure> {
    let SdslvExpression::Identifier(name) = callee else {
        return Err(SdslvAssertFailure {
            Message: "unsupported function call shape in SDSL-V M7b evaluator".to_string(),
            Span: None,
        });
    };
    let mut values = vec![];
    for argument in arguments {
        values.push(EvalExpression(argument, env)?);
    }
    match name.as_str() {
        "abs" if values.len() == 1 => Ok(RuntimeValue::F32(AsF32(&values[0])?.abs())),
        "min" if values.len() == 2 => Ok(RuntimeValue::F32(
            AsF32(&values[0])?.min(AsF32(&values[1])?),
        )),
        "max" if values.len() == 2 => Ok(RuntimeValue::F32(
            AsF32(&values[0])?.max(AsF32(&values[1])?),
        )),
        "clamp" if values.len() == 3 => Ok(RuntimeValue::F32(
            AsF32(&values[0])?.clamp(AsF32(&values[1])?, AsF32(&values[2])?),
        )),
        "saturate" if values.len() == 1 => {
            Ok(RuntimeValue::F32(AsF32(&values[0])?.clamp(0.0, 1.0)))
        }
        _ => Err(SdslvAssertFailure {
            Message: format!("unsupported function call '{name}' in SDSL-V M7b evaluator"),
            Span: None,
        }),
    }
}

fn NumericBinary(
    left: &RuntimeValue,
    right: &RuntimeValue,
    f32_op: fn(f32, f32) -> f32,
    i32_op: fn(i32, i32) -> i32,
) -> Result<RuntimeValue, SdslvAssertFailure> {
    match (left, right) {
        (RuntimeValue::I32(a), RuntimeValue::I32(b)) => Ok(RuntimeValue::I32(i32_op(*a, *b))),
        _ => Ok(RuntimeValue::F32(f32_op(AsF32(left)?, AsF32(right)?))),
    }
}
fn CompareNumeric(
    left: &RuntimeValue,
    right: &RuntimeValue,
    op: fn(f32, f32) -> bool,
) -> Result<RuntimeValue, SdslvAssertFailure> {
    Ok(RuntimeValue::Bool(op(AsF32(left)?, AsF32(right)?)))
}
fn AsF32(value: &RuntimeValue) -> Result<f32, SdslvAssertFailure> {
    match value {
        RuntimeValue::I32(x) => Ok(*x as f32),
        RuntimeValue::F32(x) => Ok(*x),
        _ => Err(SdslvAssertFailure {
            Message: "expected numeric scalar value".to_string(),
            Span: None,
        }),
    }
}
fn RuntimeEquals(left: &RuntimeValue, right: &RuntimeValue) -> bool {
    match (left, right) {
        (RuntimeValue::Bool(a), RuntimeValue::Bool(b)) => a == b,
        (RuntimeValue::I32(a), RuntimeValue::I32(b)) => a == b,
        (RuntimeValue::F32(a), RuntimeValue::F32(b)) => a == b,
        (RuntimeValue::String(a), RuntimeValue::String(b)) => a == b,
        _ => false,
    }
}

fn DefaultValueForType(type_name: &SdslvTypeRef) -> Option<RuntimeValue> {
    let path = type_name.AsNamedPath()?;
    let name = path.Segments.last()?.as_str();
    match name {
        "bool" => Some(RuntimeValue::Bool(false)),
        "i32" => Some(RuntimeValue::I32(0)),
        "u32" => Some(RuntimeValue::I32(0)),
        "f32" | "float" => Some(RuntimeValue::F32(0.0)),
        _ => None,
    }
}
