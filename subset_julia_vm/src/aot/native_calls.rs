//! Classification for Julia native-call boundary forms in AoT.
//!
//! Julia treats `ccall` and `Core.Intrinsics.llvmcall` as codegen-boundary
//! constructs, not ordinary method calls.  sjulia does not yet implement a safe
//! supported subset, so AoT classifies them explicitly and rejects them before
//! Rust or Cranelift codegen can see them as normal calls.

use crate::aot::abi::AotCallAbi;
use crate::aot::{AotError, AotResult, UnsupportedInstructionDiagnostic};
use crate::span::Span;

/// Native-call boundary kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AotNativeCallKind {
    Ccall,
    LlvmCall,
}

impl AotNativeCallKind {
    pub fn name(self) -> &'static str {
        match self {
            AotNativeCallKind::Ccall => "ccall",
            AotNativeCallKind::LlvmCall => "llvmcall",
        }
    }

    pub fn from_call_name(name: &str) -> Option<Self> {
        match name {
            "ccall" => Some(AotNativeCallKind::Ccall),
            "llvmcall" => Some(AotNativeCallKind::LlvmCall),
            other if other.ends_with(".ccall") => Some(AotNativeCallKind::Ccall),
            other if other.ends_with(".llvmcall") => Some(AotNativeCallKind::LlvmCall),
            _ => None,
        }
    }
}

/// Support status for a classified native call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AotNativeCallSupport {
    /// Future supported path.  Any accepted native call must carry typed ABI data.
    Supported { abi: AotCallAbi },
    /// Current behavior for all native-call boundary forms.
    Unsupported { reason: String },
}

/// Classified native-call boundary with source location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AotNativeCallBoundary {
    pub kind: AotNativeCallKind,
    pub display_name: String,
    pub span: Span,
    pub support: AotNativeCallSupport,
}

impl AotNativeCallBoundary {
    fn unsupported(kind: AotNativeCallKind, display_name: String, span: Span) -> Self {
        let reason = match kind {
            AotNativeCallKind::Ccall => {
                "ccall requires static signature validation and native ABI lowering, which sjulia AoT does not implement yet"
            }
            AotNativeCallKind::LlvmCall => {
                "llvmcall can contain arbitrary LLVM IR and is rejected unless a backend explicitly supports a safe subset"
            }
        };
        Self {
            kind,
            display_name,
            span,
            support: AotNativeCallSupport::Unsupported {
                reason: reason.to_string(),
            },
        }
    }

    pub fn is_supported(&self) -> bool {
        matches!(self.support, AotNativeCallSupport::Supported { .. })
    }
}

/// Classify a direct Core IR call such as `ccall(...)` or `llvmcall(...)`.
pub fn classify_direct_native_call(function: &str, span: Span) -> Option<AotNativeCallBoundary> {
    AotNativeCallKind::from_call_name(function)
        .map(|kind| AotNativeCallBoundary::unsupported(kind, function.to_string(), span))
}

/// Classify a module-qualified Core IR call such as `Core.Intrinsics.llvmcall(...)`.
pub fn classify_module_native_call(
    module: &str,
    function: &str,
    span: Span,
) -> Option<AotNativeCallBoundary> {
    let display_name = format!("{}.{}", module, function);
    if module == "Core.Intrinsics" && function == "llvmcall" {
        return Some(AotNativeCallBoundary::unsupported(
            AotNativeCallKind::LlvmCall,
            display_name,
            span,
        ));
    }
    if function == "ccall" {
        return Some(AotNativeCallBoundary::unsupported(
            AotNativeCallKind::Ccall,
            display_name,
            span,
        ));
    }
    None
}

/// True if an AoT call target must not reach backend codegen as an ordinary call.
pub fn is_native_call_target(function: &str) -> bool {
    AotNativeCallKind::from_call_name(function).is_some()
}

/// Convert an unsupported classified boundary into a user-facing AoT error.
pub fn reject_unsupported_native_call(boundary: &AotNativeCallBoundary) -> AotResult<()> {
    match &boundary.support {
        AotNativeCallSupport::Supported { .. } => Ok(()),
        AotNativeCallSupport::Unsupported { reason } => Err(AotError::UnsupportedInstruction(
            UnsupportedInstructionDiagnostic::new(format!(
                "unsupported AoT native call boundary `{}`: {}. This form must be handled at the lowering/AoT boundary, not emitted as an ordinary function call.",
                boundary.display_name, reason
            ))
            .with_span(boundary.span)
            .with_workaround(
                "run the program through the VM, or replace the native call with a pure Julia helper that AoT can lower",
            ),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span() -> Span {
        Span::new(10, 28, 3, 3, 5, 23)
    }

    #[test]
    fn classifies_direct_ccall_as_unsupported_boundary() {
        let boundary = classify_direct_native_call("ccall", span()).unwrap();

        assert_eq!(boundary.kind, AotNativeCallKind::Ccall);
        assert!(!boundary.is_supported());
    }

    #[test]
    fn classifies_core_intrinsics_llvmcall_as_unsupported_boundary() {
        let boundary = classify_module_native_call("Core.Intrinsics", "llvmcall", span()).unwrap();

        assert_eq!(boundary.kind, AotNativeCallKind::LlvmCall);
        assert!(!boundary.is_supported());
    }

    #[test]
    fn reject_error_mentions_span_and_boundary_kind() {
        let boundary = classify_direct_native_call("ccall", span()).unwrap();
        let err = reject_unsupported_native_call(&boundary).unwrap_err();
        let msg = err.to_string();

        assert!(msg.contains("ccall"));
        assert!(msg.contains("line 3, column 5"));
        assert!(msg.contains("native ABI lowering"));
        assert!(msg.contains("Workaround:"));
    }

    #[test]
    fn non_boundary_call_is_not_classified() {
        assert!(classify_direct_native_call("sin", span()).is_none());
        assert!(classify_module_native_call("Base", "sin", span()).is_none());
    }
}
