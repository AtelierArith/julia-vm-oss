//! Stack-bytecode finalization helpers shared by compiler frontends.
//!
//! These functions group peephole optimization and slotization entry points at
//! the bytecode crate boundary so compiler crates do not need an integration
//! crate facade to finish stack bytecode (Issue #9090).

use crate::peephole::{self, MainStoreProtection};
use crate::slot::{self, SlotInfo, SlotParamInfo};
use crate::{Instr, KwParamInfo, ValueType, VarTypeTag};
use std::collections::{HashMap, HashSet};

pub fn optimize(code: Vec<Instr>) -> (Vec<Instr>, Vec<usize>) {
    peephole::optimize(code)
}

/// Like [`optimize`], but installs fusion barriers at `boundaries` so no fusion
/// spans them (Issue #9199 LV2).
pub fn optimize_with_boundaries(
    code: Vec<Instr>,
    boundaries: &[usize],
) -> (Vec<Instr>, Vec<usize>) {
    peephole::optimize_with_boundaries(code, boundaries)
}

/// [`optimize_protecting_main_stores`] plus the fusion barriers of
/// [`optimize_with_boundaries`] (Issue #9199 LV2).
pub fn optimize_protecting_main_stores_with_boundaries(
    code: Vec<Instr>,
    protection: MainStoreProtection<'_>,
    boundaries: &[usize],
) -> (Vec<Instr>, Vec<usize>) {
    peephole::optimize_protecting_main_stores_with_boundaries(code, protection, boundaries)
}

/// Like [`optimize`], but keeps protected main/global-frame stores observable
/// after their statement group's `Return*` (Issue #9157).
pub fn optimize_protecting_main_stores(
    code: Vec<Instr>,
    protection: MainStoreProtection<'_>,
) -> (Vec<Instr>, Vec<usize>) {
    peephole::optimize_protecting_main_stores(code, protection)
}

pub fn build_slot_info(
    params: &[(String, ValueType)],
    kwparams: &[KwParamInfo],
    code: &[Instr],
) -> SlotInfo {
    build_slot_info_with_generic_params(params, kwparams, code, &HashSet::new())
}

/// [`build_slot_info`] variant that forces the named parameters to generic
/// (untyped) slots even when their declared [`ValueType`] is a machine scalar.
pub fn build_slot_info_with_generic_params(
    params: &[(String, ValueType)],
    kwparams: &[KwParamInfo],
    code: &[Instr],
    generic_param_slots: &HashSet<String>,
) -> SlotInfo {
    let kw_slot_info = kwparams
        .iter()
        .map(|kw| SlotParamInfo {
            name: kw.name.clone(),
            ty: kw.ty.clone(),
        })
        .collect::<Vec<_>>();
    slot::build_slot_info_with_generic_params(params, &kw_slot_info, code, generic_param_slots)
}

/// Like [`build_slot_info`] for the global/main block, but seeds the slot
/// assignment with `seed` so existing globals keep their frame-0 index and new
/// globals append (Issue #9199 LV2).
pub fn build_global_slot_info_seeded(seed: &[String], code: &[Instr]) -> SlotInfo {
    slot::build_global_slot_info_seeded(seed, code)
}

pub fn slotize_code(
    code: &mut [Instr],
    name_to_slot: &HashMap<String, usize>,
    slot_types: &[Option<VarTypeTag>],
) {
    slot::slotize_code(code, name_to_slot, slot_types);
}
