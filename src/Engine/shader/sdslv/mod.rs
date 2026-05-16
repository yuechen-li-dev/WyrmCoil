#![allow(non_snake_case)]

pub mod ast;
pub mod diagnostic;
pub mod lexer;
pub mod parser;
pub mod token;
pub mod validation;

pub use ast::*;
pub use diagnostic::*;
pub use lexer::*;
pub use parser::*;
pub use token::*;
pub use validation::*;

#[cfg(test)]
mod tests;
