//! Type stability analysis report.
//!
//! This module defines the overall analysis report containing results for all functions.

use crate::compile::type_stability::report::{FunctionStabilityReport, StabilityStatus};
#[cfg(test)]
use crate::inference_core::{CorePrimitive, CoreType};
use serde::{Deserialize, Serialize};

/// Provenance for the inferred facts used by the type-stability report.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InferenceProvenance {
    /// Human-readable name of the inference source.
    pub source: String,

    /// Whether this report is backed by the same inferred facts used by
    /// production code generation.
    pub uses_production_inference: bool,

    /// Compatibility notes and known limitations for consumers.
    pub notes: Vec<String>,
}

impl InferenceProvenance {
    /// Current report provenance for the standalone type-stability analyzer.
    pub fn standalone_type_stability_analyzer() -> Self {
        Self {
            source: "standalone type-stability analyzer".to_string(),
            uses_production_inference: false,
            notes: vec![
                "uses production global type collection but not full codegen inference snapshots"
                    .to_string(),
                "compare report return types with runtime/reflection fixtures for user-facing guarantees"
                    .to_string(),
            ],
        }
    }

    /// Report provenance for facts produced by the production shared inference
    /// engine used by code generation.
    pub fn production_shared_inference_snapshot() -> Self {
        Self {
            source: "production shared inference snapshot".to_string(),
            uses_production_inference: true,
            notes: vec![
                "return-type stability is classified from the shared inference engine used by code generation".to_string(),
                "statement-level typed IR, effects, cache invalidation, and world-age metadata remain outside this report snapshot".to_string(),
            ],
        }
    }
}

impl Default for InferenceProvenance {
    fn default() -> Self {
        Self::standalone_type_stability_analyzer()
    }
}

/// Summary statistics for the type stability analysis.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AnalysisSummary {
    /// Total number of functions analyzed.
    pub total_functions: usize,

    /// Number of type-stable functions.
    pub stable_count: usize,

    /// Number of type-unstable functions.
    pub unstable_count: usize,

    /// Number of functions with unknown stability.
    pub unknown_count: usize,
}

impl AnalysisSummary {
    /// Returns the percentage of stable functions.
    pub fn stable_percentage(&self) -> f64 {
        if self.total_functions == 0 {
            100.0
        } else {
            (self.stable_count as f64 / self.total_functions as f64) * 100.0
        }
    }

    /// Returns the percentage of unstable functions.
    pub fn unstable_percentage(&self) -> f64 {
        if self.total_functions == 0 {
            0.0
        } else {
            (self.unstable_count as f64 / self.total_functions as f64) * 100.0
        }
    }
}

/// Complete type stability analysis report for a program.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TypeStabilityAnalysisReport {
    /// Where the inferred facts in this report came from.
    pub inference_provenance: InferenceProvenance,

    /// Summary statistics.
    pub summary: AnalysisSummary,

    /// Reports for each function analyzed.
    pub functions: Vec<FunctionStabilityReport>,
}

impl TypeStabilityAnalysisReport {
    /// Creates a new empty analysis report.
    pub fn new() -> Self {
        Self {
            inference_provenance: InferenceProvenance::default(),
            summary: AnalysisSummary::default(),
            functions: Vec::new(),
        }
    }

    /// Adds a function report and updates the summary.
    pub fn add_function(&mut self, report: FunctionStabilityReport) {
        self.summary.total_functions += 1;
        match report.status {
            StabilityStatus::Stable => self.summary.stable_count += 1,
            StabilityStatus::Unstable => self.summary.unstable_count += 1,
            StabilityStatus::Unknown => self.summary.unknown_count += 1,
        }
        self.functions.push(report);
    }

    /// Returns an iterator over stable functions.
    pub fn stable_functions(&self) -> impl Iterator<Item = &FunctionStabilityReport> {
        self.functions.iter().filter(|f| f.is_stable())
    }

    /// Returns an iterator over unstable functions.
    pub fn unstable_functions(&self) -> impl Iterator<Item = &FunctionStabilityReport> {
        self.functions.iter().filter(|f| f.is_unstable())
    }

    /// Returns true if all analyzed functions are type-stable.
    pub fn all_stable(&self) -> bool {
        self.summary.unstable_count == 0 && self.summary.unknown_count == 0
    }

    /// Returns true if any function is type-unstable.
    pub fn has_unstable(&self) -> bool {
        self.summary.unstable_count > 0
    }
}

impl Default for TypeStabilityAnalysisReport {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::lattice::types::{ConcreteType, LatticeType};

    fn create_stable_report(name: &str) -> FunctionStabilityReport {
        FunctionStabilityReport::new(
            name.to_string(),
            1,
            vec![],
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        )
    }

    fn create_unstable_report(name: &str) -> FunctionStabilityReport {
        FunctionStabilityReport::new(name.to_string(), 1, vec![], LatticeType::Top)
    }

    #[test]
    fn test_empty_report() {
        let report = TypeStabilityAnalysisReport::new();
        assert_eq!(
            report.inference_provenance.source,
            "standalone type-stability analyzer"
        );
        assert!(!report.inference_provenance.uses_production_inference);
        assert_eq!(report.summary.total_functions, 0);
        assert!(report.all_stable());
        assert!(!report.has_unstable());
    }

    #[test]
    fn test_production_snapshot_provenance() {
        let provenance = InferenceProvenance::production_shared_inference_snapshot();
        assert_eq!(provenance.source, "production shared inference snapshot");
        assert!(provenance.uses_production_inference);
    }

    #[test]
    fn test_all_stable() {
        let mut report = TypeStabilityAnalysisReport::new();
        report.add_function(create_stable_report("f1"));
        report.add_function(create_stable_report("f2"));

        assert_eq!(report.summary.total_functions, 2);
        assert_eq!(report.summary.stable_count, 2);
        assert_eq!(report.summary.unstable_count, 0);
        assert!(report.all_stable());
        assert!(!report.has_unstable());
        assert_eq!(report.summary.stable_percentage(), 100.0);
    }

    #[test]
    fn test_mixed_stability() {
        let mut report = TypeStabilityAnalysisReport::new();
        report.add_function(create_stable_report("stable_func"));
        report.add_function(create_unstable_report("unstable_func"));

        assert_eq!(report.summary.total_functions, 2);
        assert_eq!(report.summary.stable_count, 1);
        assert_eq!(report.summary.unstable_count, 1);
        assert!(!report.all_stable());
        assert!(report.has_unstable());
        assert_eq!(report.summary.stable_percentage(), 50.0);
        assert_eq!(report.summary.unstable_percentage(), 50.0);
    }

    #[test]
    fn test_iterators() {
        let mut report = TypeStabilityAnalysisReport::new();
        report.add_function(create_stable_report("f1"));
        report.add_function(create_unstable_report("f2"));
        report.add_function(create_stable_report("f3"));

        let stable: Vec<_> = report.stable_functions().collect();
        let unstable: Vec<_> = report.unstable_functions().collect();

        assert_eq!(stable.len(), 2);
        assert_eq!(unstable.len(), 1);
    }
}
