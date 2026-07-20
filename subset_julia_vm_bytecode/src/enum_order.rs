//! Upstream-compatible `@enum` member binding publication order.

const MIN_TABLE_SIZE: usize = 16;
const MAX_ALLOWED_PROBE: usize = 16;
const MAX_PROBE_SHIFT: usize = 6;

type Slot = Option<(i64, usize)>;

struct JuliaIntDict {
    slots: Vec<Slot>,
    count: usize,
    max_probe: usize,
}

impl JuliaIntDict {
    fn new() -> Self {
        Self {
            slots: Vec::new(),
            count: 0,
            max_probe: 0,
        }
    }

    fn table_size(requested: usize) -> usize {
        requested.max(MIN_TABLE_SIZE).next_power_of_two()
    }

    fn hash_integer(value: i64) -> u64 {
        let mut hash = value.cast_unsigned();
        hash = (!hash).wrapping_add(hash << 21);
        hash ^= hash >> 24;
        hash = hash.wrapping_add(hash << 3).wrapping_add(hash << 8);
        hash ^= hash >> 14;
        hash = hash.wrapping_add(hash << 2).wrapping_add(hash << 4);
        hash ^= hash >> 28;
        hash.wrapping_add(hash << 31)
    }

    fn rehash(&mut self, requested: usize) {
        let mut resized = vec![None; Self::table_size(requested)];
        let mut max_probe = 0usize;
        for (value, source_index) in self.slots.drain(..).flatten() {
            let mask = resized.len() - 1;
            let initial = Self::hash_integer(value) as usize & mask;
            let mut index = initial;
            while resized[index].is_some() {
                index = (index + 1) & mask;
            }
            max_probe = max_probe.max(index.wrapping_sub(initial) & mask);
            resized[index] = Some((value, source_index));
        }
        self.slots = resized;
        self.max_probe = max_probe;
    }

    fn insert(&mut self, value: i64, source_index: usize) {
        if self.slots.is_empty() {
            self.rehash(4);
        }

        let mask = self.slots.len() - 1;
        let mut index = Self::hash_integer(value) as usize & mask;
        let mut probe = 0usize;

        loop {
            match self.slots[index] {
                None => break,
                Some((stored, _)) if stored == value => {
                    self.slots[index] = Some((value, source_index));
                    return;
                }
                Some(_) => {
                    index = (index + 1) & mask;
                    probe += 1;
                    if probe > self.max_probe {
                        break;
                    }
                }
            }
        }

        let max_allowed = MAX_ALLOWED_PROBE.max(self.slots.len() >> MAX_PROBE_SHIFT);
        while probe < max_allowed {
            if self.slots[index].is_none() {
                self.max_probe = probe;
                break;
            }
            index = (index + 1) & mask;
            probe += 1;
        }

        if probe >= max_allowed {
            let multiplier = if self.count > 64_000 { 2 } else { 4 };
            self.rehash(self.slots.len().saturating_mul(multiplier));
            self.insert(value, source_index);
            return;
        }

        self.slots[index] = Some((value, source_index));
        self.count += 1;
        if self.count.saturating_mul(3) > self.slots.len().saturating_mul(2) {
            let requested = if self.count > 64_000 {
                self.count.saturating_mul(2)
            } else {
                self.count.saturating_mul(4).max(4)
            };
            self.rehash(requested);
        }
    }
}

/// Return enum-member indices in the order upstream Julia's `@enum` macro
/// emits their `const` declarations.
///
/// `base/Enums.jl` keeps member metadata in source order but appends constant
/// declarations by iterating a `Dict{basetype,Symbol}`. For every integer value
/// representable by the current `i64` enum IR, Julia hashes equal integer values
/// identically across base types. This reproduces Julia 1.12's 64-bit integer
/// hash, linear probing, both rehash triggers, and slot-order rehash. Wider and
/// out-of-range base-type conversion remains tracked by Issue #11667.
pub fn julia_enum_member_binding_order(members: &[(String, i64)]) -> Vec<usize> {
    let mut dict = JuliaIntDict::new();
    for (source_index, (_, value)) in members.iter().enumerate() {
        dict.insert(*value, source_index);
    }
    dict.slots
        .into_iter()
        .flatten()
        .map(|(_, source_index)| source_index)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn members(values: &[i64]) -> Vec<(String, i64)> {
        values
            .iter()
            .enumerate()
            .map(|(index, value)| (format!("m{index}"), *value))
            .collect()
    }

    #[test]
    fn enum_member_binding_order_matches_upstream_dict_slots_11656() {
        assert_eq!(
            julia_enum_member_binding_order(&members(&(0..8).collect::<Vec<_>>())),
            vec![0, 4, 5, 6, 2, 7, 3, 1]
        );
        assert_eq!(
            julia_enum_member_binding_order(&members(&(0..16).collect::<Vec<_>>())),
            vec![5, 12, 8, 1, 0, 6, 11, 9, 14, 3, 7, 4, 13, 15, 2, 10]
        );
        assert_eq!(
            julia_enum_member_binding_order(&members(&[10, -2, 77, 0])),
            vec![3, 2, 0, 1]
        );
    }

    #[test]
    fn enum_member_binding_order_matches_probe_limit_rehash_11656() {
        let colliding = [
            5, 78, 169, 176, 260, 316, 352, 402, 456, 551, 729, 799, 924, 971, 1015, 1102, 1193,
        ];
        assert_eq!(
            julia_enum_member_binding_order(&members(&colliding)),
            vec![0, 7, 3, 5, 8, 9, 14, 16, 2, 6, 10, 11, 12, 15, 1, 4, 13]
        );
    }
}
