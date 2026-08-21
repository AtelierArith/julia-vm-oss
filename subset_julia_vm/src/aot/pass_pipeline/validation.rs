use crate::aot::ir::{AotFunction, AotProgram};
use crate::aot::pass_pipeline::AotPassStage;
use crate::aot::rooting::verify_aot_rooting_obligations;
use crate::aot::{AotError, AotResult};

mod expr;
mod stmt;

pub(crate) fn verify_aot_program(stage: AotPassStage, program: &AotProgram) -> AotResult<()> {
    for function in &program.functions {
        verify_function(stage, function)?;
    }
    for (idx, statement) in program.main.iter().enumerate() {
        stmt::verify_stmt(stage, "<main>", idx, statement)?;
    }
    verify_aot_rooting_obligations(stage, program)
}

fn verify_function(stage: AotPassStage, function: &AotFunction) -> AotResult<()> {
    if function.name.trim().is_empty() {
        return verifier_error(stage, "<unnamed>", "function name is empty");
    }
    for (idx, (name, _)) in function.params.iter().enumerate() {
        if name.trim().is_empty() {
            return verifier_error(
                stage,
                &function.name,
                &format!("parameter #{} has an empty name", idx),
            );
        }
    }
    for (idx, statement) in function.body.iter().enumerate() {
        stmt::verify_stmt(stage, &function.name, idx, statement)?;
    }
    Ok(())
}

fn verifier_error<T>(stage: AotPassStage, function: &str, message: &str) -> AotResult<T> {
    Err(AotError::InvalidIR(format!(
        "{} verifier failed in `{}`: {}",
        stage, function, message
    )))
}

pub(super) fn error<T>(stage: AotPassStage, function: &str, message: &str) -> AotResult<T> {
    verifier_error(stage, function, message)
}
