//! Public VM surface used by the REPL integration layer.
//!
//! This facade keeps REPL code from depending on VM implementation modules such
//! as `state` and `builtins_linalg` directly. It is intentionally narrow: the
//! REPL still executes a `Vm`, but helper boundaries cross this module.

use subset_julia_vm_bytecode::value::{
    ArrayValue, CallableSingletonIdentity, StructInstance, Value,
};
use subset_julia_vm_bytecode::{FunctionInfo, MethodSig, ReplMethodIdentity};
use subset_julia_vm_types::types::JuliaType;

use super::VmError;
pub use super::{Vm, VmMemoryStats};

/// Compact, position-independent identity for one callable method. Keeping an
/// owned projection rather than `Rc<FunctionInfo>` avoids pinning every code
/// body and triggering full `Rc::make_mut` clones during later live world-age
/// activation (Issue #9784).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PersistedCallableIdentity {
    method: ReplMethodIdentity,
    is_base_extension: bool,
    is_lowering_helper: bool,
    suppress_short_name_alias: bool,
}

impl PersistedCallableIdentity {
    pub(crate) fn from_function(function: &FunctionInfo) -> Self {
        let signature = MethodSig::from_julia_projections(
            0,
            0,
            function
                .params
                .iter()
                .enumerate()
                .map(|(index, (name, _))| {
                    (
                        name.clone(),
                        function
                            .param_julia_types
                            .get(index)
                            .cloned()
                            .unwrap_or(JuliaType::Any),
                    )
                })
                .collect(),
            function.return_type.clone(),
            function.return_julia_type.clone(),
            function.is_base_extension,
            function.type_params.clone(),
            function.vararg_param_index,
            function.vararg_fixed_count,
        );
        Self {
            method: ReplMethodIdentity::from_method_sig(&function.name, &signature),
            is_base_extension: function.is_base_extension,
            is_lowering_helper: function.is_lowering_helper,
            suppress_short_name_alias: function.suppress_short_name_alias,
        }
    }

    pub(crate) fn name(&self) -> &str {
        self.method.function()
    }

    pub(crate) fn singleton_identity(&self) -> CallableSingletonIdentity {
        CallableSingletonIdentity::from_provenance(self.name().to_string(), self.is_lowering_helper)
    }
}

/// Stable source-side metadata for rebasing frozen callable candidate indices
/// after the live VM itself has been dropped. It contains only compact method
/// identities, never code, slots, captures, or shared `FunctionInfo` owners.
#[derive(Clone)]
pub struct PersistedCallableSnapshot {
    pub(crate) identities: Vec<PersistedCallableIdentity>,
}

impl PersistedCallableSnapshot {
    pub(crate) fn len(&self) -> usize {
        self.identities.len()
    }
}

pub fn reachable_compacted_struct_heap(
    prior_heap: &[StructInstance],
    roots: &mut [(String, Value)],
) -> (Vec<StructInstance>, Vec<Option<usize>>) {
    super::state::reachable_compacted_struct_heap(prior_heap, roots)
}

pub fn linalg_value_to_array_value(
    value: Value,
    struct_heap: &[StructInstance],
    op_name: &str,
    role: Option<&str>,
) -> Result<ArrayValue, VmError> {
    super::builtins_linalg::linalg_value_to_array_value(value, struct_heap, op_name, role)
}
