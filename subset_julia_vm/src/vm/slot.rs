use std::collections::HashMap;

use super::{frame::VarTypeTag, Instr, KwParamInfo, ValueType};

pub(crate) struct SlotInfo {
    pub slot_names: Vec<String>,
    pub name_to_slot: HashMap<String, usize>,
    pub slot_types: Vec<Option<VarTypeTag>>,
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

    // Anonymous placeholder `_` parameters are positionally distinct and are
    // never read by name. Julia allows them to repeat (e.g.
    // `f(::Type{K}, ::Type{V}, n) where {K,V}` lowers both `::Type{...}` args to
    // `_`), so each one must own its own slot. Deduplicating them to a single
    // slot let argument binding overwrite a previous `_`, and `where`-type
    // extraction (`infer_type_binding_from_frame_args`) then read every type
    // variable from the same collapsed slot, conflating distinct bindings
    // (Issue #6661). Named parameters keep the dedup-to-a-stable-slot behavior.
    let mut param_slots: Vec<usize> = Vec::with_capacity(params.len());
    for (name, _) in params {
        let slot = if name == "_" {
            let idx = slot_names.len();
            slot_names.push(name.to_string());
            idx
        } else {
            *name_to_slot.entry(name.to_string()).or_insert_with(|| {
                let idx = slot_names.len();
                slot_names.push(name.to_string());
                idx
            })
        };
        param_slots.push(slot);
    }

    let mut ensure_slot = |name: &str| {
        name_to_slot.entry(name.to_string()).or_insert_with(|| {
            let idx = slot_names.len();
            slot_names.push(name.to_string());
            idx
        });
    };
    for kw in kwparams {
        ensure_slot(&kw.name);
    }

    for instr in code {
        if let Some(name) = store_name(instr) {
            ensure_slot(name);
        }
    }

    let slot_types = build_slot_types(params, kwparams, code, &name_to_slot, slot_names.len());

    let kwparam_slots = kwparams
        .iter()
        .filter_map(|kw| name_to_slot.get(&kw.name).copied())
        .collect();

    SlotInfo {
        slot_names,
        name_to_slot,
        slot_types,
        param_slots,
        kwparam_slots,
    }
}

pub(crate) fn slotize_code(
    code: &mut [Instr],
    name_to_slot: &HashMap<String, usize>,
    slot_types: &[Option<VarTypeTag>],
) {
    for instr in code.iter_mut() {
        match instr {
            Instr::LoadStr(name) => {
                if let Some(&slot) = name_to_slot.get(name) {
                    *instr = if slot_type(slot_types, slot) == Some(VarTypeTag::Str) {
                        Instr::LoadSlotStr(slot)
                    } else {
                        Instr::LoadSlot(slot)
                    };
                }
            }
            Instr::LoadF32(name) => {
                if let Some(&slot) = name_to_slot.get(name) {
                    *instr = if slot_type(slot_types, slot) == Some(VarTypeTag::F32) {
                        Instr::LoadSlotF32(slot)
                    } else {
                        Instr::LoadSlot(slot)
                    };
                }
            }
            Instr::LoadF16(name) => {
                if let Some(&slot) = name_to_slot.get(name) {
                    *instr = if slot_type(slot_types, slot) == Some(VarTypeTag::F16) {
                        Instr::LoadSlotF16(slot)
                    } else {
                        Instr::LoadSlot(slot)
                    };
                }
            }
            Instr::LoadAny(name) | Instr::LoadSet(name) | Instr::LoadArray(name) => {
                if let Some(&slot) = name_to_slot.get(name) {
                    *instr = match slot_type(slot_types, slot) {
                        Some(VarTypeTag::Array) => Instr::LoadSlotArray(slot),
                        Some(VarTypeTag::Set) => Instr::LoadSlotSet(slot),
                        Some(VarTypeTag::Generator) => Instr::LoadSlotGenerator(slot),
                        Some(VarTypeTag::Char) => Instr::LoadSlotChar(slot),
                        Some(VarTypeTag::Symbol) => Instr::LoadSlotSymbol(slot),
                        Some(VarTypeTag::NarrowInt) => Instr::LoadSlotNarrowInt(slot),
                        Some(VarTypeTag::Nothing) => Instr::LoadSlotNothing(slot),
                        _ => Instr::LoadSlot(slot),
                    };
                }
            }
            Instr::LoadTuple(name) => {
                if let Some(&slot) = name_to_slot.get(name) {
                    *instr = if slot_type(slot_types, slot) == Some(VarTypeTag::Tuple) {
                        Instr::LoadSlotTuple(slot)
                    } else {
                        Instr::LoadSlot(slot)
                    };
                }
            }
            Instr::LoadNamedTuple(name) => {
                if let Some(&slot) = name_to_slot.get(name) {
                    *instr = if slot_type(slot_types, slot) == Some(VarTypeTag::NamedTuple) {
                        Instr::LoadSlotNamedTuple(slot)
                    } else {
                        Instr::LoadSlot(slot)
                    };
                }
            }
            Instr::LoadDict(name) => {
                if let Some(&slot) = name_to_slot.get(name) {
                    *instr = if slot_type(slot_types, slot) == Some(VarTypeTag::Dict) {
                        Instr::LoadSlotDict(slot)
                    } else {
                        Instr::LoadSlot(slot)
                    };
                }
            }
            Instr::LoadStruct(name) => {
                if let Some(&slot) = name_to_slot.get(name) {
                    *instr = if slot_type(slot_types, slot) == Some(VarTypeTag::Struct) {
                        Instr::LoadSlotStruct(slot)
                    } else {
                        Instr::LoadSlot(slot)
                    };
                }
            }
            Instr::LoadRange(name) => {
                if let Some(&slot) = name_to_slot.get(name) {
                    *instr = if slot_type(slot_types, slot) == Some(VarTypeTag::Range) {
                        Instr::LoadSlotRange(slot)
                    } else {
                        Instr::LoadSlot(slot)
                    };
                }
            }
            Instr::LoadRng(name) => {
                if let Some(&slot) = name_to_slot.get(name) {
                    *instr = if slot_type(slot_types, slot) == Some(VarTypeTag::Rng) {
                        Instr::LoadSlotRng(slot)
                    } else {
                        Instr::LoadSlot(slot)
                    };
                }
            }
            Instr::LoadI64(name) => {
                if let Some(&slot) = name_to_slot.get(name) {
                    // Only emit the typed fast-path load when the slot is
                    // unambiguously I64. A slot whose stored type changes
                    // (e.g. `ex = :(0)` then `ex = :($ex + $i)`) has type
                    // tag `None`; reading it as `LoadSlotI64` would crash when
                    // the slot holds the re-assigned non-numeric value. Fall
                    // back to the generic `LoadSlot`, mirroring the typed
                    // string/float carriers above. (Issue #5935)
                    *instr = if slot_type(slot_types, slot) == Some(VarTypeTag::I64) {
                        Instr::LoadSlotI64(slot)
                    } else {
                        Instr::LoadSlot(slot)
                    };
                }
            }
            Instr::LoadF64(name) => {
                if let Some(&slot) = name_to_slot.get(name) {
                    *instr = if slot_type(slot_types, slot) == Some(VarTypeTag::F64) {
                        Instr::LoadSlotF64(slot)
                    } else {
                        Instr::LoadSlot(slot)
                    };
                }
            }
            Instr::LoadBool(name) => {
                if let Some(&slot) = name_to_slot.get(name) {
                    *instr = if slot_type(slot_types, slot) == Some(VarTypeTag::Bool) {
                        Instr::LoadSlotBool(slot)
                    } else {
                        Instr::LoadSlot(slot)
                    };
                }
            }
            Instr::StoreStr(name) => {
                if let Some(&slot) = name_to_slot.get(name) {
                    *instr = if slot_type(slot_types, slot) == Some(VarTypeTag::Str) {
                        Instr::StoreSlotStr(slot)
                    } else {
                        Instr::StoreSlot(slot)
                    };
                }
            }
            Instr::StoreF32(name) => {
                if let Some(&slot) = name_to_slot.get(name) {
                    *instr = if slot_type(slot_types, slot) == Some(VarTypeTag::F32) {
                        Instr::StoreSlotF32(slot)
                    } else {
                        Instr::StoreSlot(slot)
                    };
                }
            }
            Instr::StoreF16(name) => {
                if let Some(&slot) = name_to_slot.get(name) {
                    *instr = if slot_type(slot_types, slot) == Some(VarTypeTag::F16) {
                        Instr::StoreSlotF16(slot)
                    } else {
                        Instr::StoreSlot(slot)
                    };
                }
            }
            Instr::StoreAny(name) | Instr::StoreSet(name) | Instr::StoreArray(name) => {
                if let Some(&slot) = name_to_slot.get(name) {
                    *instr = match slot_type(slot_types, slot) {
                        Some(VarTypeTag::Array) => Instr::StoreSlotArray(slot),
                        Some(VarTypeTag::Set) => Instr::StoreSlotSet(slot),
                        Some(VarTypeTag::Generator) => Instr::StoreSlotGenerator(slot),
                        Some(VarTypeTag::Char) => Instr::StoreSlotChar(slot),
                        Some(VarTypeTag::Symbol) => Instr::StoreSlotSymbol(slot),
                        Some(VarTypeTag::NarrowInt) => Instr::StoreSlotNarrowInt(slot),
                        Some(VarTypeTag::Nothing) => Instr::StoreSlotNothing(slot),
                        _ => Instr::StoreSlot(slot),
                    };
                }
            }
            Instr::StoreTuple(name) => {
                if let Some(&slot) = name_to_slot.get(name) {
                    *instr = if slot_type(slot_types, slot) == Some(VarTypeTag::Tuple) {
                        Instr::StoreSlotTuple(slot)
                    } else {
                        Instr::StoreSlot(slot)
                    };
                }
            }
            Instr::StoreNamedTuple(name) => {
                if let Some(&slot) = name_to_slot.get(name) {
                    *instr = if slot_type(slot_types, slot) == Some(VarTypeTag::NamedTuple) {
                        Instr::StoreSlotNamedTuple(slot)
                    } else {
                        Instr::StoreSlot(slot)
                    };
                }
            }
            Instr::StoreDict(name) => {
                if let Some(&slot) = name_to_slot.get(name) {
                    *instr = if slot_type(slot_types, slot) == Some(VarTypeTag::Dict) {
                        Instr::StoreSlotDict(slot)
                    } else {
                        Instr::StoreSlot(slot)
                    };
                }
            }
            Instr::StoreStruct(name) => {
                if let Some(&slot) = name_to_slot.get(name) {
                    *instr = if slot_type(slot_types, slot) == Some(VarTypeTag::Struct) {
                        Instr::StoreSlotStruct(slot)
                    } else {
                        Instr::StoreSlot(slot)
                    };
                }
            }
            Instr::StoreRange(name) => {
                if let Some(&slot) = name_to_slot.get(name) {
                    *instr = if slot_type(slot_types, slot) == Some(VarTypeTag::Range) {
                        Instr::StoreSlotRange(slot)
                    } else {
                        Instr::StoreSlot(slot)
                    };
                }
            }
            Instr::StoreRng(name) => {
                if let Some(&slot) = name_to_slot.get(name) {
                    *instr = if slot_type(slot_types, slot) == Some(VarTypeTag::Rng) {
                        Instr::StoreSlotRng(slot)
                    } else {
                        Instr::StoreSlot(slot)
                    };
                }
            }
            Instr::StoreI64(name) => {
                if let Some(&slot) = name_to_slot.get(name) {
                    *instr = if slot_type(slot_types, slot) == Some(VarTypeTag::I64) {
                        Instr::StoreSlotI64(slot)
                    } else {
                        Instr::StoreSlot(slot)
                    };
                }
            }
            Instr::StoreF64(name) => {
                if let Some(&slot) = name_to_slot.get(name) {
                    *instr = if slot_type(slot_types, slot) == Some(VarTypeTag::F64) {
                        Instr::StoreSlotF64(slot)
                    } else {
                        Instr::StoreSlot(slot)
                    };
                }
            }
            Instr::StoreBool(name) => {
                if let Some(&slot) = name_to_slot.get(name) {
                    *instr = if slot_type(slot_types, slot) == Some(VarTypeTag::Bool) {
                        Instr::StoreSlotBool(slot)
                    } else {
                        Instr::StoreSlot(slot)
                    };
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

fn slot_type(slot_types: &[Option<VarTypeTag>], slot: usize) -> Option<VarTypeTag> {
    slot_types.get(slot).copied().flatten()
}

fn store_name(instr: &Instr) -> Option<&str> {
    match instr {
        Instr::StoreStr(name)
        | Instr::StoreI64(name)
        | Instr::StoreF64(name)
        | Instr::StoreF32(name)
        | Instr::StoreF16(name)
        | Instr::StoreBool(name)
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

fn build_slot_types(
    params: &[(String, ValueType)],
    kwparams: &[KwParamInfo],
    code: &[Instr],
    name_to_slot: &HashMap<String, usize>,
    total_slots: usize,
) -> Vec<Option<VarTypeTag>> {
    // Size by the true slot count, not `name_to_slot.len()`: anonymous `_`
    // parameters own slots without a `name_to_slot` entry (Issue #6661), so the
    // map can be smaller than the number of slots.
    let mut slot_types = vec![None; total_slots];
    let mut unknown_slots = vec![false; total_slots];

    for (name, ty) in params {
        if let Some(&slot) = name_to_slot.get(name) {
            merge_slot_type(
                &mut slot_types,
                &mut unknown_slots,
                slot,
                value_type_to_var_type_tag(ty),
            );
        }
    }

    for kw in kwparams {
        if let Some(&slot) = name_to_slot.get(&kw.name) {
            merge_slot_type(
                &mut slot_types,
                &mut unknown_slots,
                slot,
                value_type_to_var_type_tag(&kw.ty),
            );
        }
    }

    for instr in code {
        if let Some(name) = store_name(instr) {
            if let Some(&slot) = name_to_slot.get(name) {
                if store_erases_slot_type(instr) {
                    slot_types[slot] = None;
                    unknown_slots[slot] = true;
                } else {
                    merge_slot_type(
                        &mut slot_types,
                        &mut unknown_slots,
                        slot,
                        store_type_tag(instr),
                    );
                }
            }
        }
    }

    slot_types
}

fn merge_slot_type(
    slot_types: &mut [Option<VarTypeTag>],
    unknown_slots: &mut [bool],
    slot: usize,
    tag: Option<VarTypeTag>,
) {
    if unknown_slots[slot] {
        return;
    }

    match (slot_types[slot], tag) {
        (None, Some(tag)) => slot_types[slot] = Some(tag),
        (Some(existing), Some(tag)) if existing == tag => {}
        _ => {
            slot_types[slot] = None;
            unknown_slots[slot] = true;
        }
    }
}

fn value_type_to_var_type_tag(ty: &ValueType) -> Option<VarTypeTag> {
    match ty {
        ValueType::I64 => Some(VarTypeTag::I64),
        ValueType::F64 => Some(VarTypeTag::F64),
        ValueType::F32 => Some(VarTypeTag::F32),
        ValueType::F16 => Some(VarTypeTag::F16),
        ValueType::Bool => Some(VarTypeTag::Bool),
        ValueType::Str => Some(VarTypeTag::Str),
        ValueType::Char => Some(VarTypeTag::Char),
        ValueType::Symbol => Some(VarTypeTag::Symbol),
        ValueType::Array | ValueType::ArrayOf(_, _) => Some(VarTypeTag::Array),
        ValueType::Range => Some(VarTypeTag::Range),
        ValueType::Struct(_) | ValueType::ComplexF32 | ValueType::ComplexF64 => {
            Some(VarTypeTag::Struct)
        }
        ValueType::Rng => Some(VarTypeTag::Rng),
        ValueType::Tuple => Some(VarTypeTag::Tuple),
        ValueType::NamedTuple => Some(VarTypeTag::NamedTuple),
        ValueType::Dict => Some(VarTypeTag::Dict),
        ValueType::Generator => Some(VarTypeTag::Generator),
        ValueType::Nothing => Some(VarTypeTag::Nothing),
        ValueType::I8
        | ValueType::I16
        | ValueType::I32
        | ValueType::I128
        | ValueType::U8
        | ValueType::U16
        | ValueType::U32
        | ValueType::U64
        | ValueType::U128 => Some(VarTypeTag::NarrowInt),
        ValueType::Set => Some(VarTypeTag::Set),
        ValueType::Memory
        | ValueType::MemoryOf(_)
        | ValueType::Missing
        | ValueType::BigInt
        | ValueType::BigFloat
        | ValueType::DataType
        | ValueType::Module
        | ValueType::Function
        | ValueType::IO
        | ValueType::Expr
        | ValueType::QuoteNode
        | ValueType::LineNumberNode
        | ValueType::GlobalRef
        | ValueType::Pairs
        | ValueType::Regex
        | ValueType::RegexMatch
        | ValueType::Any
        | ValueType::Union(_)
        | ValueType::Enum => None,
    }
}

fn store_type_tag(instr: &Instr) -> Option<VarTypeTag> {
    match instr {
        Instr::StoreI64(_) | Instr::IncVarI64(_) | Instr::DecVarI64(_) => Some(VarTypeTag::I64),
        Instr::StoreF64(_) => Some(VarTypeTag::F64),
        Instr::StoreF32(_) => Some(VarTypeTag::F32),
        Instr::StoreF16(_) => Some(VarTypeTag::F16),
        Instr::StoreBool(_) => Some(VarTypeTag::Bool),
        Instr::StoreStr(_) => Some(VarTypeTag::Str),
        Instr::StoreStruct(_) => Some(VarTypeTag::Struct),
        Instr::StoreRng(_) => Some(VarTypeTag::Rng),
        Instr::StoreRange(_) => Some(VarTypeTag::Range),
        Instr::StoreTuple(_) => Some(VarTypeTag::Tuple),
        Instr::StoreNamedTuple(_) => Some(VarTypeTag::NamedTuple),
        Instr::StoreDict(_) => Some(VarTypeTag::Dict),
        Instr::StoreArray(_) => Some(VarTypeTag::Array),
        Instr::StoreSet(_) => Some(VarTypeTag::Set),
        Instr::StoreAny(_) => None,
        _ => None,
    }
}

fn store_erases_slot_type(instr: &Instr) -> bool {
    matches!(instr, Instr::StoreAny(_))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::Value;

    #[test]
    fn slotize_preserves_typed_numeric_slot_ops() {
        let mut code = vec![
            Instr::LoadI64("i".to_string()),
            Instr::StoreI64("i".to_string()),
            Instr::LoadF64("x".to_string()),
            Instr::StoreF64("x".to_string()),
            Instr::LoadBool("flag".to_string()),
            Instr::StoreBool("flag".to_string()),
            Instr::LoadAny("flag".to_string()),
            Instr::StoreAny("flag".to_string()),
        ];
        let slot_info = build_slot_info(
            &[
                ("i".to_string(), ValueType::I64),
                ("x".to_string(), ValueType::F64),
            ],
            &[],
            &code,
        );

        slotize_code(&mut code, &slot_info.name_to_slot, &slot_info.slot_types);

        assert!(matches!(code[0], Instr::LoadSlotI64(_)));
        assert!(matches!(code[1], Instr::StoreSlotI64(_)));
        assert!(matches!(code[2], Instr::LoadSlotF64(_)));
        assert!(matches!(code[3], Instr::StoreSlotF64(_)));
        // `flag` is stored as both Bool and Any, so its slot type is ambiguous
        // (None). Typed loads and stores of such a slot must fall back to the
        // generic slot path, which is safe when the slot holds the Any-stored
        // value. (Issues #5935, #7555)
        assert!(matches!(code[4], Instr::LoadSlot(_)));
        assert!(matches!(code[5], Instr::StoreSlot(_)));
        assert!(matches!(code[6], Instr::LoadSlot(_)));
        assert!(matches!(code[7], Instr::StoreSlot(_)));
    }

    #[test]
    fn slotize_falls_back_to_generic_store_for_type_changing_slots_issue_7555() {
        let mut code = vec![
            Instr::StoreF64("name".to_string()),
            Instr::StoreAny("name".to_string()),
            Instr::StoreBool("flag".to_string()),
            Instr::StoreAny("flag".to_string()),
            Instr::StoreI64("count".to_string()),
            Instr::StoreAny("count".to_string()),
        ];
        let slot_info = build_slot_info(&[], &[], &code);

        slotize_code(&mut code, &slot_info.name_to_slot, &slot_info.slot_types);

        assert!(
            matches!(code[0], Instr::StoreSlot(_)),
            "name: {:?}",
            code[0]
        );
        assert!(
            matches!(code[1], Instr::StoreSlot(_)),
            "name any: {:?}",
            code[1]
        );
        assert!(
            matches!(code[2], Instr::StoreSlot(_)),
            "flag: {:?}",
            code[2]
        );
        assert!(
            matches!(code[3], Instr::StoreSlot(_)),
            "flag any: {:?}",
            code[3]
        );
        assert!(
            matches!(code[4], Instr::StoreSlot(_)),
            "count: {:?}",
            code[4]
        );
        assert!(
            matches!(code[5], Instr::StoreSlot(_)),
            "count any: {:?}",
            code[5]
        );
    }

    #[test]
    fn slotize_falls_back_to_generic_load_for_type_changing_numeric_slots_issue_5935() {
        // A local whose stored type changes (e.g. `ex = :(0)` collapses to the
        // literal I64 0, then `ex = :($ex + $i)` builds an Expr stored via
        // StoreAny) has an ambiguous slot type (None). A typed `LoadI64`/`LoadF64`/
        // `LoadBool` read of such a slot (the `$ex` interpolation in the loop body)
        // must NOT be slotized to the unconditional `LoadSlot{I64,F64,Bool}` — that
        // crashes at runtime ("LoadSlotI64: expected numeric ... got Expr") when the
        // slot holds the re-assigned non-numeric value. It must fall back to the
        // generic `LoadSlot`, mirroring LoadStr/LoadF32/LoadF16/LoadAny. (Issue #5935)
        let mut code = vec![
            Instr::LoadI64("i".to_string()),
            Instr::LoadF64("f".to_string()),
            Instr::LoadBool("b".to_string()),
        ];
        let slot_info = build_slot_info(
            &[],
            &[],
            &[
                Instr::StoreI64("i".to_string()),
                Instr::StoreAny("i".to_string()),
                Instr::StoreF64("f".to_string()),
                Instr::StoreAny("f".to_string()),
                Instr::StoreBool("b".to_string()),
                Instr::StoreAny("b".to_string()),
            ],
        );

        slotize_code(&mut code, &slot_info.name_to_slot, &slot_info.slot_types);

        assert!(matches!(code[0], Instr::LoadSlot(_)), "i: {:?}", code[0]);
        assert!(matches!(code[1], Instr::LoadSlot(_)), "f: {:?}", code[1]);
        assert!(matches!(code[2], Instr::LoadSlot(_)), "b: {:?}", code[2]);
    }

    #[test]
    fn slotize_uses_slot_type_metadata_for_f32_f16_and_str_issue_5081() {
        let mut code = vec![
            Instr::LoadF32("x32".to_string()),
            Instr::StoreF32("x32".to_string()),
            Instr::LoadF16("x16".to_string()),
            Instr::StoreF16("x16".to_string()),
            Instr::LoadStr("name".to_string()),
            Instr::StoreStr("name".to_string()),
            Instr::LoadF32("dynamic".to_string()),
            Instr::StoreF32("dynamic".to_string()),
        ];
        let slot_info = build_slot_info(
            &[("dynamic".to_string(), ValueType::Any)],
            &[],
            &[
                Instr::StoreF32("x32".to_string()),
                Instr::StoreF16("x16".to_string()),
                Instr::StoreStr("name".to_string()),
                Instr::StoreAny("dynamic".to_string()),
            ],
        );

        slotize_code(&mut code, &slot_info.name_to_slot, &slot_info.slot_types);

        assert!(matches!(code[0], Instr::LoadSlotF32(_)));
        assert!(matches!(code[1], Instr::StoreSlotF32(_)));
        assert!(matches!(code[2], Instr::LoadSlotF16(_)));
        assert!(matches!(code[3], Instr::StoreSlotF16(_)));
        assert!(matches!(code[4], Instr::LoadSlotStr(_)));
        assert!(matches!(code[5], Instr::StoreSlotStr(_)));
        assert!(matches!(code[6], Instr::LoadSlot(_)));
        assert!(matches!(code[7], Instr::StoreSlot(_)));
    }

    #[test]
    fn slotize_uses_slot_type_metadata_for_scalar_carriers_issue_5081() {
        let mut code = vec![
            Instr::LoadAny("ch".to_string()),
            Instr::LoadAny("sym".to_string()),
            Instr::StoreAny("sym".to_string()),
            Instr::LoadAny("small".to_string()),
            Instr::LoadAny("none".to_string()),
            Instr::LoadAny("dynamic".to_string()),
        ];
        let slot_info = build_slot_info(
            &[
                ("ch".to_string(), ValueType::Char),
                ("sym".to_string(), ValueType::Symbol),
                ("small".to_string(), ValueType::U64),
                ("none".to_string(), ValueType::Nothing),
                ("dynamic".to_string(), ValueType::Any),
            ],
            &[],
            &[Instr::StoreAny("dynamic".to_string())],
        );

        slotize_code(&mut code, &slot_info.name_to_slot, &slot_info.slot_types);

        assert!(matches!(code[0], Instr::LoadSlotChar(_)));
        assert!(matches!(code[1], Instr::LoadSlotSymbol(_)));
        assert!(matches!(code[2], Instr::StoreSlotSymbol(_)));
        assert!(matches!(code[3], Instr::LoadSlotNarrowInt(_)));
        assert!(matches!(code[4], Instr::LoadSlotNothing(_)));
        assert!(matches!(code[5], Instr::LoadSlot(_)));
    }

    #[test]
    fn slotize_uses_slot_type_metadata_for_container_slots_issue_5081() {
        let mut code = vec![
            Instr::LoadArray("arr".to_string()),
            Instr::StoreArray("arr".to_string()),
            Instr::LoadTuple("items".to_string()),
            Instr::StoreTuple("items".to_string()),
            Instr::LoadNamedTuple("named".to_string()),
            Instr::StoreNamedTuple("named".to_string()),
            Instr::LoadDict("dict".to_string()),
            Instr::StoreDict("dict".to_string()),
            Instr::LoadSet("set".to_string()),
            Instr::StoreSet("set".to_string()),
            Instr::LoadStruct("point".to_string()),
            Instr::StoreStruct("point".to_string()),
            Instr::LoadRange("range".to_string()),
            Instr::StoreRange("range".to_string()),
            Instr::LoadRng("rng".to_string()),
            Instr::StoreRng("rng".to_string()),
            Instr::LoadAny("gen".to_string()),
            Instr::LoadArray("dynamic".to_string()),
            Instr::StoreArray("dynamic".to_string()),
        ];
        let slot_info = build_slot_info(
            &[
                ("gen".to_string(), ValueType::Generator),
                ("dynamic".to_string(), ValueType::Any),
            ],
            &[],
            &[
                Instr::StoreArray("arr".to_string()),
                Instr::StoreTuple("items".to_string()),
                Instr::StoreNamedTuple("named".to_string()),
                Instr::StoreDict("dict".to_string()),
                Instr::StoreSet("set".to_string()),
                Instr::StoreStruct("point".to_string()),
                Instr::StoreRange("range".to_string()),
                Instr::StoreRng("rng".to_string()),
                Instr::StoreAny("dynamic".to_string()),
            ],
        );

        slotize_code(&mut code, &slot_info.name_to_slot, &slot_info.slot_types);

        assert!(matches!(code[0], Instr::LoadSlotArray(_)));
        assert!(matches!(code[1], Instr::StoreSlotArray(_)));
        assert!(matches!(code[2], Instr::LoadSlotTuple(_)));
        assert!(matches!(code[3], Instr::StoreSlotTuple(_)));
        assert!(matches!(code[4], Instr::LoadSlotNamedTuple(_)));
        assert!(matches!(code[5], Instr::StoreSlotNamedTuple(_)));
        assert!(matches!(code[6], Instr::LoadSlotDict(_)));
        assert!(matches!(code[7], Instr::StoreSlotDict(_)));
        assert!(matches!(code[8], Instr::LoadSlotSet(_)));
        assert!(matches!(code[9], Instr::StoreSlotSet(_)));
        assert!(matches!(code[10], Instr::LoadSlotStruct(_)));
        assert!(matches!(code[11], Instr::StoreSlotStruct(_)));
        assert!(matches!(code[12], Instr::LoadSlotRange(_)));
        assert!(matches!(code[13], Instr::StoreSlotRange(_)));
        assert!(matches!(code[14], Instr::LoadSlotRng(_)));
        assert!(matches!(code[15], Instr::StoreSlotRng(_)));
        assert!(matches!(code[16], Instr::LoadSlotGenerator(_)));
        assert!(matches!(code[17], Instr::LoadSlot(_)));
        assert!(matches!(code[18], Instr::StoreSlot(_)));
    }

    #[test]
    fn slot_info_records_static_slot_types_issue_5081() {
        let code = vec![
            Instr::StoreBool("flag".to_string()),
            Instr::StoreTuple("items".to_string()),
            Instr::IncVarI64("i".to_string()),
        ];
        let slot_info = build_slot_info(
            &[
                ("i".to_string(), ValueType::I64),
                ("x".to_string(), ValueType::F64),
            ],
            &[KwParamInfo {
                name: "name".to_string(),
                default: Value::Str("default".to_string()),
                default_expr: None,
                ty: ValueType::Str,
                slot: 0,
                required: false,
                is_varargs: false,
            }],
            &code,
        );

        let tag_for = |name: &str| {
            let slot = slot_info.name_to_slot[name];
            slot_info.slot_types[slot]
        };

        assert_eq!(tag_for("i"), Some(VarTypeTag::I64));
        assert_eq!(tag_for("x"), Some(VarTypeTag::F64));
        assert_eq!(tag_for("name"), Some(VarTypeTag::Str));
        assert_eq!(tag_for("flag"), Some(VarTypeTag::Bool));
        assert_eq!(tag_for("items"), Some(VarTypeTag::Tuple));
    }

    #[test]
    fn slot_info_marks_dynamic_or_conflicting_slots_unknown_issue_5081() {
        let code = vec![
            Instr::StoreI64("mixed".to_string()),
            Instr::StoreF64("mixed".to_string()),
            Instr::StoreAny("dynamic".to_string()),
            Instr::StoreI64("dynamic".to_string()),
        ];
        let slot_info = build_slot_info(&[("arg".to_string(), ValueType::Any)], &[], &code);

        let tag_for = |name: &str| {
            let slot = slot_info.name_to_slot[name];
            slot_info.slot_types[slot]
        };

        assert_eq!(tag_for("mixed"), None);
        assert_eq!(tag_for("dynamic"), None);
        assert_eq!(tag_for("arg"), None);
    }

    #[test]
    fn repeated_anonymous_params_get_distinct_slots_issue_6661() {
        // `f(::Type{K}, ::Type{V}, n) where {K,V}` lowers both `::Type{...}`
        // arguments to the anonymous placeholder `_`. The two `_` parameters must
        // own distinct slots; deduplicating them onto one slot let argument
        // binding overwrite the first `_` and made `where`-type extraction read
        // both K and V from the same slot (Issue #6661).
        let slot_info = build_slot_info(
            &[
                ("_".to_string(), ValueType::DataType),
                ("_".to_string(), ValueType::DataType),
                ("n".to_string(), ValueType::Any),
            ],
            &[],
            &[],
        );

        assert_eq!(slot_info.param_slots.len(), 3);
        assert_ne!(slot_info.param_slots[0], slot_info.param_slots[1]);
        assert_ne!(slot_info.param_slots[0], slot_info.param_slots[2]);
        assert_ne!(slot_info.param_slots[1], slot_info.param_slots[2]);
        // Three parameters -> three distinct slots.
        assert_eq!(slot_info.slot_names.len(), 3);
        // `slot_types` is sized to the true slot count, not the (smaller) name
        // map (anonymous slots have no `name_to_slot` entry).
        assert_eq!(slot_info.slot_types.len(), 3);
    }

    #[test]
    fn named_params_still_dedup_to_a_stable_slot() {
        // Named parameters keep the dedup-to-a-stable-slot behavior; only the
        // anonymous `_` placeholder is exempted (Issue #6661).
        let slot_info = build_slot_info(
            &[
                ("a".to_string(), ValueType::I64),
                ("b".to_string(), ValueType::F64),
            ],
            &[],
            &[Instr::StoreI64("a".to_string())],
        );
        assert_eq!(slot_info.param_slots.len(), 2);
        assert_ne!(slot_info.param_slots[0], slot_info.param_slots[1]);
        assert_eq!(slot_info.name_to_slot["a"], slot_info.param_slots[0]);
        assert_eq!(slot_info.name_to_slot["b"], slot_info.param_slots[1]);
        assert_eq!(slot_info.slot_names.len(), 2);
    }
}
