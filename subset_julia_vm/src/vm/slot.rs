use std::collections::HashMap;

use super::{Instr, KwParamInfo, ValueType};

pub(crate) struct SlotInfo {
    pub slot_names: Vec<String>,
    pub name_to_slot: HashMap<String, usize>,
    pub param_slots: Vec<usize>,
    pub kwparam_slots: Vec<usize>,
}

pub(crate) fn build_slot_info(
    params: &[(String, ValueType)],
    kwparams: &[KwParamInfo],
    code: &[Instr],
) -> SlotInfo {
    let mut slot_names: Vec<String> = Vec::new();
    let mut name_to_slot: HashMap<String, usize> = HashMap::new();

    let mut ensure_slot = |name: &str| {
        name_to_slot
            .entry(name.to_string())
            .or_insert_with(|| {
                let idx = slot_names.len();
                slot_names.push(name.to_string());
                idx
            });
    };

    for (name, _) in params {
        ensure_slot(name);
    }
    for kw in kwparams {
        ensure_slot(&kw.name);
    }

    for instr in code {
        if let Some(name) = store_name(instr) {
            ensure_slot(name);
        }
    }

    let param_slots = params
        .iter()
        .filter_map(|(name, _)| name_to_slot.get(name).copied())
        .collect();
    let kwparam_slots = kwparams
        .iter()
        .filter_map(|kw| name_to_slot.get(&kw.name).copied())
        .collect();

    SlotInfo {
        slot_names,
        name_to_slot,
        param_slots,
        kwparam_slots,
    }
}

pub(crate) fn slotize_code(code: &mut [Instr], name_to_slot: &HashMap<String, usize>) {
    for instr in code.iter_mut() {
        match instr {
            Instr::LoadStr(name)
            | Instr::LoadI64(name)
            | Instr::LoadF64(name)
            | Instr::LoadF32(name)
            | Instr::LoadF16(name)
            | Instr::LoadAny(name)
            | Instr::LoadStruct(name)
            | Instr::LoadRng(name)
            | Instr::LoadRange(name)
            | Instr::LoadTuple(name)
            | Instr::LoadNamedTuple(name)
            | Instr::LoadDict(name)
            | Instr::LoadSet(name)
            | Instr::LoadArray(name) => {
                if let Some(&slot) = name_to_slot.get(name) {
                    *instr = Instr::LoadSlot(slot);
                }
            }
            Instr::StoreStr(name)
            | Instr::StoreI64(name)
            | Instr::StoreF64(name)
            | Instr::StoreF32(name)
            | Instr::StoreF16(name)
            | Instr::StoreAny(name)
            | Instr::StoreStruct(name)
            | Instr::StoreRng(name)
            | Instr::StoreRange(name)
            | Instr::StoreTuple(name)
            | Instr::StoreNamedTuple(name)
            | Instr::StoreDict(name)
            | Instr::StoreSet(name)
            | Instr::StoreArray(name) => {
                if let Some(&slot) = name_to_slot.get(name) {
                    *instr = Instr::StoreSlot(slot);
                }
            }
            Instr::LoadAddI64(name) => {
                if let Some(&slot) = name_to_slot.get(name) {
                    *instr = Instr::LoadAddI64Slot(slot);
                }
            }
            Instr::LoadSubI64(name) => {
                if let Some(&slot) = name_to_slot.get(name) {
                    *instr = Instr::LoadSubI64Slot(slot);
                }
            }
            Instr::LoadMulI64(name) => {
                if let Some(&slot) = name_to_slot.get(name) {
                    *instr = Instr::LoadMulI64Slot(slot);
                }
            }
            Instr::LoadModI64(name) => {
                if let Some(&slot) = name_to_slot.get(name) {
                    *instr = Instr::LoadModI64Slot(slot);
                }
            }
            Instr::IncVarI64(name) => {
                if let Some(&slot) = name_to_slot.get(name) {
                    *instr = Instr::IncVarI64Slot(slot);
                }
            }
            Instr::DecVarI64(name) => {
                if let Some(&slot) = name_to_slot.get(name) {
                    *instr = Instr::DecVarI64Slot(slot);
                }
            }
            _ => {}
        }
    }
}

fn store_name(instr: &Instr) -> Option<&str> {
    match instr {
        Instr::StoreStr(name)
        | Instr::StoreI64(name)
        | Instr::StoreF64(name)
        | Instr::StoreF32(name)
        | Instr::StoreF16(name)
        | Instr::StoreAny(name)
        | Instr::StoreStruct(name)
        | Instr::StoreRng(name)
        | Instr::StoreRange(name)
        | Instr::StoreTuple(name)
        | Instr::StoreNamedTuple(name)
        | Instr::StoreDict(name)
        | Instr::StoreSet(name)
        | Instr::StoreArray(name)
        | Instr::IncVarI64(name)
        | Instr::DecVarI64(name) => Some(name.as_str()),
        _ => None,
    }
}
