pub mod binding_resolution;
pub mod evaluate;
pub mod parse;
pub mod prelude;
pub mod type_check;
pub use parse::parse;

use crate::binding_resolution::BindingResolutionError;
use crate::parse::ast::ParsedProgram;
use crate::type_check::{TypeCheckError, TypeCheckedProgram};

#[derive(Debug)]
pub enum AnalysisError {
    BindingResolution(Vec<BindingResolutionError>),
    TypeCheck(Vec<TypeCheckError>),
}

pub fn analyse(program: ParsedProgram) -> Result<TypeCheckedProgram, AnalysisError> {
    let resolved =
        binding_resolution::resolve(program).map_err(AnalysisError::BindingResolution)?;

    type_check::type_check(resolved).map_err(AnalysisError::TypeCheck)
}
