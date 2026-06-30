//! Named AoT pass stages, diagnostics, and verification hooks.

use crate::aot::ir::{AotExpr, AotFunction, AotProgram, AotStmt};
use crate::aot::native_calls::is_native_call_target;
use crate::aot::rooting::verify_aot_rooting_obligations;
use crate::aot::{AotError, AotResult};
use std::fmt;
use std::str::FromStr;

/// Stable names for major AoT compiler boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AotPassStage {
    AfterLowering,
    AfterDce,
    AfterInference,
    AfterAotIrConversion,
    AfterAbiLowering,
    AfterOptimization,
    BeforeBackendCodegen,
}

impl AotPassStage {
    pub fn name(self) -> &'static str {
        match self {
            AotPassStage::AfterLowering => "AfterLowering",
            AotPassStage::AfterDce => "AfterDCE",
            AotPassStage::AfterInference => "AfterInference",
            AotPassStage::AfterAotIrConversion => "AfterAotIrConversion",
            AotPassStage::AfterAbiLowering => "AfterAbiLowering",
            AotPassStage::AfterOptimization => "AfterOptimization",
            AotPassStage::BeforeBackendCodegen => "BeforeBackendCodegen",
        }
    }

    pub fn all() -> &'static [AotPassStage] {
        &[
            AotPassStage::AfterLowering,
            AotPassStage::AfterDce,
            AotPassStage::AfterInference,
            AotPassStage::AfterAotIrConversion,
            AotPassStage::AfterAbiLowering,
            AotPassStage::AfterOptimization,
            AotPassStage::BeforeBackendCodegen,
        ]
    }
}

impl fmt::Display for AotPassStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for AotPassStage {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let normalized = value
            .chars()
            .filter(|ch| *ch != '-' && *ch != '_')
            .flat_map(char::to_lowercase)
            .collect::<String>();
        Self::all()
            .iter()
            .copied()
            .find(|stage| {
                stage
                    .name()
                    .chars()
                    .filter(|ch| *ch != '-' && *ch != '_')
                    .flat_map(char::to_lowercase)
                    .collect::<String>()
                    == normalized
            })
            .ok_or_else(|| format!("unknown AoT dump stage `{}`", value))
    }
}

/// CLI dump selection for named AoT stages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AotDumpSelection {
    None,
    All,
    Stage(AotPassStage),
}

impl AotDumpSelection {
    pub fn parse(value: Option<&str>) -> Result<Self, String> {
        match value {
            None => Ok(AotDumpSelection::None),
            Some("all") | Some("ALL") => Ok(AotDumpSelection::All),
            Some(stage) => AotPassStage::from_str(stage).map(AotDumpSelection::Stage),
        }
    }

    pub fn should_dump(&self, stage: AotPassStage) -> bool {
        match self {
            AotDumpSelection::None => false,
            AotDumpSelection::All => true,
            AotDumpSelection::Stage(selected) => *selected == stage,
        }
    }
}

/// Per-stage AoT statistics that are cheap to collect from high-level AoT IR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AotPassStats {
    pub stage: AotPassStage,
    pub functions: usize,
    pub statements: usize,
    pub dynamic_calls: usize,
    pub structs: usize,
    pub globals: usize,
}

impl AotPassStats {
    pub fn from_program(stage: AotPassStage, program: &AotProgram) -> Self {
        Self {
            stage,
            functions: program.functions.len(),
            statements: program.instruction_count(),
            dynamic_calls: program.count_dynamic_calls(),
            structs: program.structs.len(),
            globals: program.globals.len(),
        }
    }
}

/// Captured dump for one named stage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AotStageDump {
    pub stage: AotPassStage,
    pub stats: AotPassStats,
    pub ir: String,
}

impl AotStageDump {
    pub fn render(&self) -> String {
        format!(
            "== {} ==\nfunctions={} statements={} dynamic_calls={} structs={} globals={}\n{}",
            self.stage,
            self.stats.functions,
            self.stats.statements,
            self.stats.dynamic_calls,
            self.stats.structs,
            self.stats.globals,
            self.ir
        )
    }
}

/// Records pass stats, optional dumps, and runs verifier hooks at named stages.
#[derive(Debug, Clone)]
pub struct AotPassDiagnostics {
    dump_selection: AotDumpSelection,
    stats: Vec<AotPassStats>,
    dumps: Vec<AotStageDump>,
}

impl AotPassDiagnostics {
    pub fn new(dump_selection: AotDumpSelection) -> Self {
        Self {
            dump_selection,
            stats: Vec::new(),
            dumps: Vec::new(),
        }
    }

    pub fn verify_and_record(
        &mut self,
        stage: AotPassStage,
        program: &AotProgram,
    ) -> AotResult<()> {
        verify_aot_program(stage, program)?;
        let stats = AotPassStats::from_program(stage, program);
        if self.dump_selection.should_dump(stage) {
            self.dumps.push(AotStageDump {
                stage,
                stats: stats.clone(),
                ir: format!("{:#?}", program),
            });
        }
        self.stats.push(stats);
        Ok(())
    }

    pub fn stats(&self) -> &[AotPassStats] {
        &self.stats
    }

    pub fn dumps(&self) -> &[AotStageDump] {
        &self.dumps
    }

    pub fn render_dumps(&self) -> String {
        self.dumps
            .iter()
            .map(AotStageDump::render)
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

/// Verify high-level AoT IR structural invariants at a named boundary.
pub fn verify_aot_program(stage: AotPassStage, program: &AotProgram) -> AotResult<()> {
    for function in &program.functions {
        verify_function(stage, function)?;
    }
    for (idx, stmt) in program.main.iter().enumerate() {
        verify_stmt(stage, "<main>", idx, stmt)?;
    }
    verify_aot_rooting_obligations(stage, program)?;
    Ok(())
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
    for (idx, stmt) in function.body.iter().enumerate() {
        verify_stmt(stage, &function.name, idx, stmt)?;
    }
    Ok(())
}

fn verify_stmt(
    stage: AotPassStage,
    function: &str,
    stmt_idx: usize,
    stmt: &AotStmt,
) -> AotResult<()> {
    match stmt {
        AotStmt::Let { name, value, .. } => {
            if name.trim().is_empty() {
                return verifier_error(
                    stage,
                    function,
                    &format!("statement #{} binds an empty variable name", stmt_idx),
                );
            }
            verify_expr(stage, function, value)
        }
        AotStmt::Assign { target, value } => {
            verify_expr(stage, function, target)?;
            verify_expr(stage, function, value)
        }
        AotStmt::CompoundAssign { target, value, .. } => {
            verify_expr(stage, function, target)?;
            verify_expr(stage, function, value)
        }
        AotStmt::Return(Some(expr)) | AotStmt::Expr(expr) => verify_expr(stage, function, expr),
        AotStmt::If {
            condition,
            then_branch,
            else_branch,
        } => {
            verify_expr(stage, function, condition)?;
            verify_block(stage, function, then_branch)?;
            if let Some(else_branch) = else_branch {
                verify_block(stage, function, else_branch)?;
            }
            Ok(())
        }
        AotStmt::While {
            condition, body, ..
        } => {
            verify_expr(stage, function, condition)?;
            verify_block(stage, function, body)
        }
        AotStmt::ForRange {
            var,
            start,
            stop,
            step,
            body,
        } => {
            if var.trim().is_empty() {
                return verifier_error(stage, function, "for-range variable name is empty");
            }
            verify_expr(stage, function, start)?;
            verify_expr(stage, function, stop)?;
            if let Some(step) = step {
                verify_expr(stage, function, step)?;
            }
            verify_block(stage, function, body)
        }
        AotStmt::ForEach { var, iter, body } => {
            if var.trim().is_empty() {
                return verifier_error(stage, function, "for-each variable name is empty");
            }
            verify_expr(stage, function, iter)?;
            verify_block(stage, function, body)
        }
        AotStmt::Return(None) | AotStmt::Break | AotStmt::Continue => Ok(()),
    }
}

fn verify_block(stage: AotPassStage, function: &str, block: &[AotStmt]) -> AotResult<()> {
    for (idx, stmt) in block.iter().enumerate() {
        verify_stmt(stage, function, idx, stmt)?;
    }
    Ok(())
}

fn verify_expr(stage: AotPassStage, function: &str, expr: &AotExpr) -> AotResult<()> {
    match expr {
        AotExpr::Var { name, .. } => {
            if name.trim().is_empty() {
                verifier_error(stage, function, "variable reference has an empty name")
            } else {
                Ok(())
            }
        }
        AotExpr::BinOpStatic { left, right, .. } | AotExpr::BinOpDynamic { left, right, .. } => {
            verify_expr(stage, function, left)?;
            verify_expr(stage, function, right)
        }
        AotExpr::UnaryOp { operand, .. } => verify_expr(stage, function, operand),
        AotExpr::CallStatic {
            function: name,
            args,
            ..
        }
        | AotExpr::CallDynamic {
            function: name,
            args,
        } => {
            if name.trim().is_empty() {
                return verifier_error(stage, function, "call target name is empty");
            }
            if is_native_call_target(name) {
                return verifier_error(
                    stage,
                    function,
                    &format!(
                        "native call boundary `{}` reached AoT backend as an ordinary call",
                        name
                    ),
                );
            }
            verify_exprs(stage, function, args)
        }
        AotExpr::CallBuiltin { args, .. } => verify_exprs(stage, function, args),
        AotExpr::ArrayLit {
            elements, shape, ..
        } => {
            let expected_len = shape
                .iter()
                .try_fold(1usize, |acc, dim| acc.checked_mul(*dim));
            if expected_len != Some(elements.len()) {
                return verifier_error(
                    stage,
                    function,
                    &format!(
                        "array literal shape {:?} expects {:?} elements, got {}",
                        shape,
                        expected_len,
                        elements.len()
                    ),
                );
            }
            verify_exprs(stage, function, elements)
        }
        AotExpr::TupleLit { elements }
        | AotExpr::StructNew {
            fields: elements, ..
        } => verify_exprs(stage, function, elements),
        AotExpr::SetFromIter { iter, .. } => verify_expr(stage, function, iter),
        AotExpr::NamedTupleLit { fields } => {
            for (name, field) in fields {
                if name.trim().is_empty() {
                    return verifier_error(stage, function, "named tuple field has an empty name");
                }
                verify_expr(stage, function, field)?;
            }
            Ok(())
        }
        AotExpr::Comprehension {
            body,
            var,
            iter,
            filter,
            ..
        }
        | AotExpr::Generator {
            body,
            var,
            iter,
            filter,
            ..
        } => {
            if var.trim().is_empty() {
                return verifier_error(
                    stage,
                    function,
                    "comprehension/generator variable name is empty",
                );
            }
            verify_expr(stage, function, iter)?;
            if let Some(filter) = filter {
                verify_expr(stage, function, filter)?;
            }
            verify_expr(stage, function, body)
        }
        AotExpr::MultiComprehension {
            body,
            iterations,
            filter,
            ..
        } => {
            if iterations.is_empty() {
                return verifier_error(stage, function, "multi-comprehension has no iterations");
            }
            for (var, iter) in iterations {
                if var.trim().is_empty() {
                    return verifier_error(stage, function, "comprehension variable name is empty");
                }
                verify_expr(stage, function, iter)?;
            }
            if let Some(filter) = filter {
                verify_expr(stage, function, filter)?;
            }
            verify_expr(stage, function, body)
        }
        AotExpr::Index { array, indices, .. } => {
            if indices.is_empty() {
                return verifier_error(stage, function, "index expression has no indices");
            }
            verify_expr(stage, function, array)?;
            verify_exprs(stage, function, indices)
        }
        AotExpr::Range {
            start, stop, step, ..
        } => {
            verify_expr(stage, function, start)?;
            verify_expr(stage, function, stop)?;
            if let Some(step) = step {
                verify_expr(stage, function, step)?;
            }
            Ok(())
        }
        AotExpr::FieldAccess { object, field, .. } => {
            if field.trim().is_empty() {
                return verifier_error(stage, function, "field access has an empty field name");
            }
            verify_expr(stage, function, object)
        }
        AotExpr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            verify_expr(stage, function, condition)?;
            verify_expr(stage, function, then_expr)?;
            verify_expr(stage, function, else_expr)
        }
        AotExpr::Box(inner)
        | AotExpr::Unbox { value: inner, .. }
        | AotExpr::Convert { value: inner, .. } => verify_expr(stage, function, inner),
        AotExpr::Lambda {
            params,
            body,
            captures,
            ..
        } => {
            for (name, _) in params.iter().chain(captures.iter()) {
                if name.trim().is_empty() {
                    return verifier_error(stage, function, "lambda binding has an empty name");
                }
            }
            verify_expr(stage, function, body)
        }
        AotExpr::LitI64(_)
        | AotExpr::LitI32(_)
        | AotExpr::LitF64(_)
        | AotExpr::LitF32(_)
        | AotExpr::LitBool(_)
        | AotExpr::LitStr(_)
        | AotExpr::LitChar(_)
        | AotExpr::LitNothing
        | AotExpr::LitMissing => Ok(()),
    }
}

fn verify_exprs(stage: AotPassStage, function: &str, exprs: &[AotExpr]) -> AotResult<()> {
    for expr in exprs {
        verify_expr(stage, function, expr)?;
    }
    Ok(())
}

fn verifier_error<T>(stage: AotPassStage, function: &str, message: &str) -> AotResult<T> {
    Err(AotError::InvalidIR(format!(
        "{} verifier failed in `{}`: {}",
        stage, function, message
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aot::ir::{AotFunction, AotInlinePolicy, AotStmt};
    use crate::aot::types::StaticType;

    #[test]
    fn stage_parser_accepts_canonical_and_relaxed_names() {
        assert_eq!(
            "AfterAotIrConversion".parse::<AotPassStage>().unwrap(),
            AotPassStage::AfterAotIrConversion
        );
        assert_eq!(
            "after-aot-ir-conversion".parse::<AotPassStage>().unwrap(),
            AotPassStage::AfterAotIrConversion
        );
    }

    #[test]
    fn dump_selection_filters_stages() {
        let selection = AotDumpSelection::parse(Some("BeforeBackendCodegen")).unwrap();

        assert!(selection.should_dump(AotPassStage::BeforeBackendCodegen));
        assert!(!selection.should_dump(AotPassStage::AfterOptimization));
        assert!(AotDumpSelection::parse(Some("all"))
            .unwrap()
            .should_dump(AotPassStage::AfterOptimization));
    }

    #[test]
    fn diagnostics_records_stats_and_dump() {
        let mut program = AotProgram::new();
        let mut func = AotFunction::new(
            "id".to_string(),
            vec![("x".to_string(), StaticType::I64)],
            StaticType::I64,
        );
        func.body.push(AotStmt::Return(Some(AotExpr::Var {
            name: "x".to_string(),
            ty: StaticType::I64,
        })));
        program.add_function(func);

        let mut diagnostics = AotPassDiagnostics::new(AotDumpSelection::All);
        diagnostics
            .verify_and_record(AotPassStage::AfterAotIrConversion, &program)
            .unwrap();

        assert_eq!(diagnostics.stats()[0].functions, 1);
        assert_eq!(diagnostics.dumps().len(), 1);
        assert!(diagnostics.render_dumps().contains("AfterAotIrConversion"));
    }

    #[test]
    fn verifier_rejects_malformed_array_shape() {
        let mut program = AotProgram::new();
        program.main.push(AotStmt::Expr(AotExpr::ArrayLit {
            elements: vec![AotExpr::LitI64(1), AotExpr::LitI64(2)],
            elem_ty: StaticType::I64,
            shape: vec![3],
        }));

        let err = verify_aot_program(AotPassStage::BeforeBackendCodegen, &program).unwrap_err();
        assert!(err
            .to_string()
            .contains("BeforeBackendCodegen verifier failed"));
    }

    #[test]
    fn verifier_rejects_empty_call_target() {
        let mut program = AotProgram::new();
        program.main.push(AotStmt::Expr(AotExpr::CallStatic {
            function: String::new(),
            args: vec![],
            return_ty: StaticType::I64,
            inline_policy: AotInlinePolicy::Auto,
        }));

        let err = verify_aot_program(AotPassStage::AfterOptimization, &program).unwrap_err();
        assert!(err.to_string().contains("call target name is empty"));
    }

    #[test]
    fn verifier_rejects_native_call_boundary_as_ordinary_call() {
        let mut program = AotProgram::new();
        program.main.push(AotStmt::Expr(AotExpr::CallStatic {
            function: "llvmcall".to_string(),
            args: vec![],
            return_ty: StaticType::Any,
            inline_policy: AotInlinePolicy::Auto,
        }));

        let err = verify_aot_program(AotPassStage::BeforeBackendCodegen, &program).unwrap_err();
        assert!(err
            .to_string()
            .contains("native call boundary `llvmcall` reached AoT backend"));
    }
}
