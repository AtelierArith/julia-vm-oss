use crate::builtins::BuiltinId;
use crate::ir::core::Expr;
use crate::vm::{Instr, ValueType};

use super::super::{err, CResult, CoreCompiler};

impl CoreCompiler<'_> {
    pub(super) fn compile_builtin_set(
        &mut self,
        name: &str,
        args: &[Expr],
    ) -> CResult<Option<ValueType>> {
        match name {
            "union" => self.compile_set_binary(args, BuiltinId::SetUnion, ValueType::Set),
            "intersect" => self.compile_set_binary(args, BuiltinId::SetIntersect, ValueType::Set),
            "setdiff" => self.compile_set_binary(args, BuiltinId::SetSetdiff, ValueType::Set),
            "symdiff" => self.compile_set_binary(args, BuiltinId::SetSymdiff, ValueType::Set),
            "issubset" => self.compile_set_binary(args, BuiltinId::SetIssubset, ValueType::Bool),
            "isdisjoint" => {
                self.compile_set_binary(args, BuiltinId::SetIsdisjoint, ValueType::Bool)
            }
            "issetequal" => {
                self.compile_set_binary(args, BuiltinId::SetIssetequal, ValueType::Bool)
            }
            "union!" => self.compile_set_binary(args, BuiltinId::SetUnionMut, ValueType::Set),
            "intersect!" => {
                self.compile_set_binary(args, BuiltinId::SetIntersectMut, ValueType::Set)
            }
            "setdiff!" => self.compile_set_binary(args, BuiltinId::SetSetdiffMut, ValueType::Set),
            "symdiff!" => self.compile_set_binary(args, BuiltinId::SetSymdiffMut, ValueType::Set),
            _ => Ok(None),
        }
    }

    fn compile_set_binary(
        &mut self,
        args: &[Expr],
        builtin: BuiltinId,
        result_type: ValueType,
    ) -> CResult<Option<ValueType>> {
        if args.len() != 2 {
            return err(format!(
                "{} requires exactly 2 arguments",
                builtin.name()
            ));
        }
        self.compile_expr(&args[0])?;
        self.compile_expr(&args[1])?;
        self.emit(Instr::CallBuiltin(builtin, 2));
        Ok(Some(result_type))
    }
}
