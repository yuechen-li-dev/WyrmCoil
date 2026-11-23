#![allow(non_snake_case)]

pub mod artifact;
pub mod ast;
mod canonical_compute;
mod canonical_graphics;
pub mod diagnostic;
pub mod dxc;
pub mod emitter;
pub mod lexer;
mod lower;
pub mod parser;
pub mod runner;
pub mod spirv_cache;
pub mod token;
pub mod validation;
pub mod vdmir;

pub use artifact::*;
pub use ast::*;
pub use diagnostic::*;
pub use dxc::*;
pub use emitter::*;
pub use lexer::*;
pub use lower::LowerToVdMir;
pub use parser::*;
pub use runner::*;
pub use token::*;
pub use validation::*;

#[cfg(test)]
mod tests;
