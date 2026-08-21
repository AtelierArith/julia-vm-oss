mod aggregate;
mod array;
mod control;
mod expr;
mod ops;

use std::collections::HashMap;

use crate::aot::ir::{
    AotFunction, AotProgram, AotStmt, BasicBlock, IrFunction, IrModule, Terminator, VarRef,
};
use crate::aot::types::StaticType;
use crate::aot::{AotError, AotResult};

use super::layout::LayoutRegistry;
use super::types::unsupported;
use ops::{ensure_return_type, ensure_type};

pub(super) fn lower_program(program: &AotProgram) -> AotResult<IrModule> {
    if !program.globals.is_empty() || !program.enums.is_empty() {
        return Err(unsupported("Wasm AoT does not support globals or enums"));
    }
    let mut layouts = LayoutRegistry::collect(program)?;
    let mut module = IrModule::new("subset_julia_wasm".to_string());
    for function in &program.functions {
        module.add_function(Lowerer::new(function, &mut layouts).lower()?);
    }
    if !program.main.is_empty() {
        let main = AotFunction {
            name: "__juliars_main".to_string(),
            params: Vec::new(),
            return_type: StaticType::Nothing,
            body: program.main.clone(),
            is_generic: false,
            inline_policy: crate::aot::ir::AotInlinePolicy::Auto,
        };
        module.add_function(Lowerer::new(&main, &mut layouts).lower()?);
    }
    module.layouts = layouts.finish();
    Ok(module)
}

pub(super) struct Lowerer<'source, 'layouts> {
    source: &'source AotFunction,
    vars: HashMap<String, VarRef>,
    current: String,
    temp: usize,
    block: usize,
    layouts: &'layouts mut LayoutRegistry,
}

impl<'source, 'layouts> Lowerer<'source, 'layouts> {
    fn new(source: &'source AotFunction, layouts: &'layouts mut LayoutRegistry) -> Self {
        let vars = source
            .params
            .iter()
            .map(|(name, ty)| (name.clone(), VarRef::new(name.clone(), ty.clone())))
            .collect();
        Self {
            source,
            vars,
            current: "entry".to_string(),
            temp: 0,
            block: 0,
            layouts,
        }
    }

    fn lower(mut self) -> AotResult<IrFunction> {
        for (_, ty) in &self.source.params {
            ensure_type(ty)?;
        }
        ensure_return_type(&self.source.return_type)?;
        let mut function = IrFunction::new(
            self.source.name.clone(),
            self.source.params.clone(),
            self.source.return_type.clone(),
        );
        let tail_carrier = self.source.body.last().and_then(|stmt| match stmt {
            AotStmt::ValueCarrier(expr) if self.source.return_type != StaticType::Nothing => {
                Some(expr)
            }
            _ => None,
        });
        let statement_count = self.source.body.len() - usize::from(tail_carrier.is_some());
        self.stmts(&self.source.body[..statement_count], &mut function)?;
        if self.current_block(&function)?.terminator.is_none() {
            if let Some(expr) = tail_carrier {
                let value = self.expr(expr, &mut function)?;
                let value = if value.ty == self.source.return_type {
                    value
                } else {
                    let converted = self.temporary(self.source.return_type.clone());
                    self.current_block_mut(&mut function)?.push(
                        crate::aot::ir::Instruction::Copy {
                            dest: converted.clone(),
                            src: value,
                        },
                    );
                    converted
                };
                self.current_block_mut(&mut function)?
                    .set_terminator(Terminator::Return(Some(value)));
            }
        }
        if self.current_block(&function)?.terminator.is_none() {
            if self.source.return_type != StaticType::Nothing {
                return Err(unsupported(format!(
                    "Wasm AoT function `{}` has no unambiguous return value",
                    self.source.name
                )));
            }
            self.current_block_mut(&mut function)?
                .set_terminator(Terminator::Return(None));
        }
        Ok(function)
    }

    pub(super) fn stmts(&mut self, stmts: &[AotStmt], function: &mut IrFunction) -> AotResult<()> {
        for stmt in stmts {
            if self.current_block(function)?.terminator.is_some() {
                break;
            }
            self.stmt(stmt, function)?;
        }
        Ok(())
    }

    pub(super) fn temporary(&mut self, ty: StaticType) -> VarRef {
        self.temp += 1;
        VarRef::new(format!("__w{}", self.temp), ty)
    }

    pub(super) fn label(&mut self, prefix: &str) -> String {
        self.block += 1;
        format!("{prefix}_{}", self.block)
    }

    pub(super) fn current_block<'b>(&self, function: &'b IrFunction) -> AotResult<&'b BasicBlock> {
        function
            .blocks
            .iter()
            .find(|block| block.label == self.current)
            .ok_or_else(|| {
                AotError::InternalError(format!("missing Wasm IR block `{}`", self.current))
            })
    }

    pub(super) fn current_block_mut<'b>(
        &self,
        function: &'b mut IrFunction,
    ) -> AotResult<&'b mut BasicBlock> {
        function
            .blocks
            .iter_mut()
            .find(|block| block.label == self.current)
            .ok_or_else(|| {
                AotError::InternalError(format!("missing Wasm IR block `{}`", self.current))
            })
    }

    pub(super) fn jump_if_open(&self, target: &str, function: &mut IrFunction) -> AotResult<()> {
        if self.current_block(function)?.terminator.is_none() {
            self.current_block_mut(function)?
                .set_terminator(Terminator::Jump(target.to_string()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::lower_program;
    use crate::aot::ir::{AotExpr, AotFunction, AotProgram, AotStmt, Terminator};
    use crate::aot::types::StaticType;
    use crate::aot::AotError;

    #[test]
    fn final_value_carrier_becomes_return() {
        // Given: retained AoT IR with a typed final value carrier.
        let mut function = AotFunction::new("answer".to_string(), Vec::new(), StaticType::I64);
        function
            .body
            .push(AotStmt::ValueCarrier(AotExpr::LitI64(42)));
        let mut program = AotProgram::new();
        program.add_function(function);

        // When: Wasm lowering builds backend-neutral control flow.
        let module = lower_program(&program).expect("retained carrier should lower");

        // Then: the open block returns its value rather than jumping to itself.
        assert!(matches!(
            module.functions[0].blocks[0].terminator,
            Some(Terminator::Return(Some(_)))
        ));
    }

    #[test]
    fn missing_typed_return_is_unsupported() {
        // Given: a non-Nothing function with no explicit return or final carrier.
        let function = AotFunction::new("ambiguous".to_string(), Vec::new(), StaticType::I64);
        let mut program = AotProgram::new();
        program.add_function(function);

        // When: Wasm lowering reaches the open typed block.
        let error = lower_program(&program).expect_err("ambiguous return must fail");

        // Then: lowering returns the typed unsupported boundary.
        assert!(matches!(error, AotError::UnsupportedInstruction(_)));
    }
}
