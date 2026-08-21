//! Named AoT pass stages, diagnostics, and verification hooks.

use crate::aot::ir::AotProgram;
use crate::aot::validation;
use crate::aot::AotResult;
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
    validation::verify_aot_program(stage, program)
}

#[cfg(test)]
mod tests;
