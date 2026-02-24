//! Output formatters for type stability reports.
//!
//! This module provides formatters for displaying type stability analysis results
//! in various formats (text, JSON).

use crate::compile::type_stability::analysis_report::TypeStabilityAnalysisReport;
use crate::compile::type_stability::report::FunctionStabilityReport;

/// Formats the analysis report as human-readable text.
pub fn format_text_report(report: &TypeStabilityAnalysisReport) -> String {
    let mut output = String::new();

    // Header
    output.push_str("Type Stability Analysis Report\n");
    output.push_str("==============================\n\n");

    // Summary section
    output.push_str("Summary:\n");
    output.push_str(&format!(
        "  Total functions: {}\n",
        report.summary.total_functions
    ));
    output.push_str(&format!(
        "  Type-stable: {} ({:.1}%)\n",
        report.summary.stable_count,
        report.summary.stable_percentage()
    ));
    output.push_str(&format!(
        "  Type-unstable: {} ({:.1}%)\n",
        report.summary.unstable_count,
        report.summary.unstable_percentage()
    ));

    if report.summary.unknown_count > 0 {
        output.push_str(&format!("  Unknown: {}\n", report.summary.unknown_count));
    }
    output.push('\n');

    // Unstable functions section
    let unstable_functions: Vec<&FunctionStabilityReport> = report.unstable_functions().collect();

    if !unstable_functions.is_empty() {
        output.push_str("Unstable Functions:\n");
        output.push_str("-------------------\n\n");

        for (idx, func) in unstable_functions.iter().enumerate() {
            output.push_str(&format_unstable_function(idx + 1, func));
            output.push('\n');
        }
    } else {
        output.push_str("All functions are type-stable!\n");
    }

    output
}

/// Formats a single unstable function report.
fn format_unstable_function(index: usize, report: &FunctionStabilityReport) -> String {
    let mut output = String::new();

    output.push_str(&format!(
        "{}. {} (line {})\n",
        index, report.function_name, report.line
    ));
    output.push_str(&format!("   Input: {}\n", format_input_signature(report)));
    output.push_str(&format!("   Return: {}\n", report.format_return_type()));

    // Add reasons
    for reason in &report.reasons {
        output.push_str(&format!("   Reason: {}\n", reason.description()));
    }

    // Add suggestions
    if !report.suggestions.is_empty() {
        output.push_str(&format!("   Suggestion: {}\n", report.suggestions[0]));
    }

    output
}

/// Formats the input signature for display.
fn format_input_signature(report: &FunctionStabilityReport) -> String {
    if report.input_signature.is_empty() {
        return "()".to_string();
    }

    let params: Vec<String> = report
        .input_signature
        .iter()
        .map(|(name, ty)| {
            let type_str = format_lattice_type_short(ty);
            format!("{}::{}", name, type_str)
        })
        .collect();

    format!("({})", params.join(", "))
}

/// Formats a LatticeType in short form for display.
fn format_lattice_type_short(ty: &crate::compile::lattice::types::LatticeType) -> String {
    use crate::compile::lattice::types::LatticeType;

    match ty {
        LatticeType::Bottom => "Bottom".to_string(),
        LatticeType::Const(_) => "Const".to_string(),
        LatticeType::Concrete(ct) => format!("{:?}", ct),
        LatticeType::Union(types) => {
            let type_strs: Vec<String> = types.iter().map(|t| format!("{:?}", t)).collect();
            format!("Union{{{}}}", type_strs.join(", "))
        }
        LatticeType::Conditional { .. } => "Conditional".to_string(),
        LatticeType::Top => "Any".to_string(),
    }
}

/// Formats the analysis report as JSON.
pub fn format_json_report(report: &TypeStabilityAnalysisReport) -> Result<String, String> {
    serde_json::to_string_pretty(report).map_err(|e| format!("JSON serialization error: {}", e))
}

/// Formats the analysis report as compact JSON.
pub fn format_json_compact(report: &TypeStabilityAnalysisReport) -> Result<String, String> {
    serde_json::to_string(report).map_err(|e| format!("JSON serialization error: {}", e))
}

/// Output format options.
#[derive(Clone, Debug, PartialEq, Eq)]
#[derive(Default)]
pub enum OutputFormat {
    /// Human-readable text format.
    #[default]
    Text,
    /// Pretty-printed JSON format.
    Json,
    /// Compact JSON format.
    JsonCompact,
}


/// Formats the report according to the specified format.
pub fn format_report(
    report: &TypeStabilityAnalysisReport,
    format: OutputFormat,
) -> Result<String, String> {
    match format {
        OutputFormat::Text => Ok(format_text_report(report)),
        OutputFormat::Json => format_json_report(report),
        OutputFormat::JsonCompact => format_json_compact(report),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::lattice::types::{ConcreteType, LatticeType};
    use crate::compile::type_stability::reason::TypeStabilityReason;
    use crate::compile::type_stability::report::FunctionStabilityReport;

    fn create_unstable_report() -> FunctionStabilityReport {
        let mut report = FunctionStabilityReport::new(
            "compute_value".to_string(),
            15,
            vec![
                (
                    "x".to_string(),
                    LatticeType::Concrete(ConcreteType::Float64),
                ),
                (
                    "flag".to_string(),
                    LatticeType::Concrete(ConcreteType::Bool),
                ),
            ],
            LatticeType::Top,
        );
        report.add_reason(TypeStabilityReason::ReturnsTop);
        report
    }

    #[test]
    fn test_format_text_report() {
        let mut analysis = TypeStabilityAnalysisReport::new();
        analysis.add_function(create_unstable_report());

        let text = format_text_report(&analysis);

        assert!(text.contains("Type Stability Analysis Report"));
        assert!(text.contains("compute_value"));
        assert!(text.contains("line 15"));
        assert!(text.contains("Unstable"));
    }

    #[test]
    fn test_format_json_report() {
        let mut analysis = TypeStabilityAnalysisReport::new();
        analysis.add_function(FunctionStabilityReport::new(
            "stable_func".to_string(),
            1,
            vec![],
            LatticeType::Concrete(ConcreteType::Int64),
        ));

        let json = format_json_report(&analysis).unwrap();

        assert!(json.contains("stable_func"));
        assert!(json.contains("\"status\":"));
    }

    #[test]
    fn test_all_stable_message() {
        let mut analysis = TypeStabilityAnalysisReport::new();
        analysis.add_function(FunctionStabilityReport::new(
            "stable_func".to_string(),
            1,
            vec![],
            LatticeType::Concrete(ConcreteType::Int64),
        ));

        let text = format_text_report(&analysis);

        assert!(text.contains("All functions are type-stable!"));
    }

    #[test]
    fn test_format_report_dispatch() {
        let analysis = TypeStabilityAnalysisReport::new();

        let text = format_report(&analysis, OutputFormat::Text).unwrap();
        let json = format_report(&analysis, OutputFormat::Json).unwrap();
        let compact = format_report(&analysis, OutputFormat::JsonCompact).unwrap();

        assert!(text.contains("Type Stability Analysis Report"));
        assert!(json.contains("{\n")); // Pretty printed
        assert!(!compact.contains("\n  ")); // Compact (no indentation)
    }
}
