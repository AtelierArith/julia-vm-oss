//! Shared splat expansion helper for function call handlers.
//!
//! This module centralizes the logic for expanding splatted arguments
//! (`f(args...)`) from Array, Tuple, and Range values into flat argument lists.
//! Used by `call.rs`, `call_dynamic.rs`, and `sync_exec.rs`.

use super::value::Value;

/// Expand splatted arguments into a flat argument list.
///
/// Given a list of arguments and a splat mask indicating which arguments should
/// be expanded, this function produces a flat `Vec<Value>` with splatted
/// collections (Array, Tuple, Range) inlined.
///
/// # Arguments
/// * `args` - The arguments to process (consumed)
/// * `splat_mask` - Boolean mask where `true` at index `i` means `args[i]` should be splatted
///
/// # Returns
/// A flat `Vec<Value>` with splatted arguments expanded inline.
pub fn expand_splat_arguments(args: Vec<Value>, splat_mask: &[bool]) -> Vec<Value> {
    let mut expanded = Vec::new();
    for (idx, arg) in args.into_iter().enumerate() {
        if splat_mask.get(idx).copied().unwrap_or(false) {
            // Expand this argument
            match &arg {
                Value::Array(arr) => {
                    let borrowed = arr.borrow();
                    for i in 0..borrowed.len() {
                        if let Ok(val) = borrowed.get(&[(i + 1) as i64]) {
                            expanded.push(val);
                        }
                    }
                }
                Value::Tuple(tuple) => {
                    for elem in &tuple.elements {
                        expanded.push(elem.clone());
                    }
                }
                Value::Range(range) => {
                    // Julia ranges are inclusive: 1:3 = [1, 2, 3]
                    let mut i = range.start;
                    while (range.step > 0.0 && i <= range.stop)
                        || (range.step < 0.0 && i >= range.stop)
                    {
                        expanded.push(Value::I64(i as i64));
                        i += range.step;
                    }
                }
                _ => expanded.push(arg),
            }
        } else {
            expanded.push(arg);
        }
    }
    expanded
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::value::{RangeValue, TupleValue};

    fn i64_val(v: i64) -> Value {
        Value::I64(v)
    }

    // ── expand_splat_arguments ────────────────────────────────────────────────

    #[test]
    fn test_no_splat_passes_args_through() {
        // Without any splat, args are returned unchanged
        let args = vec![i64_val(1), i64_val(2)];
        let mask = vec![false, false];
        let result = expand_splat_arguments(args, &mask);
        assert_eq!(result.len(), 2);
        assert!(matches!(result[0], Value::I64(1)));
        assert!(matches!(result[1], Value::I64(2)));
    }

    #[test]
    fn test_tuple_splat_expands_elements() {
        // f((1, 2, 3)...) → f(1, 2, 3)
        let tuple = Value::Tuple(TupleValue::new(vec![i64_val(10), i64_val(20)]));
        let args = vec![tuple];
        let mask = vec![true];
        let result = expand_splat_arguments(args, &mask);
        assert_eq!(result.len(), 2);
        assert!(matches!(result[0], Value::I64(10)));
        assert!(matches!(result[1], Value::I64(20)));
    }

    #[test]
    fn test_range_splat_expands_to_integers() {
        // f((1:3)...) → f(1, 2, 3)
        let range = Value::Range(RangeValue::unit_range(1.0, 3.0));
        let args = vec![range];
        let mask = vec![true];
        let result = expand_splat_arguments(args, &mask);
        assert_eq!(result.len(), 3);
        assert!(matches!(result[0], Value::I64(1)));
        assert!(matches!(result[1], Value::I64(2)));
        assert!(matches!(result[2], Value::I64(3)));
    }

    #[test]
    fn test_non_splatted_arg_before_splatted() {
        // f(0, (1, 2)...) → [0, 1, 2]
        let tuple = Value::Tuple(TupleValue::new(vec![i64_val(1), i64_val(2)]));
        let args = vec![i64_val(0), tuple];
        let mask = vec![false, true];
        let result = expand_splat_arguments(args, &mask);
        assert_eq!(result.len(), 3);
        assert!(matches!(result[0], Value::I64(0)));
        assert!(matches!(result[1], Value::I64(1)));
        assert!(matches!(result[2], Value::I64(2)));
    }

    #[test]
    fn test_non_collection_value_with_splat_passes_through() {
        // Non-collection values (scalars) with splat=true are passed through as-is
        let args = vec![i64_val(42)];
        let mask = vec![true];
        let result = expand_splat_arguments(args, &mask);
        assert_eq!(result.len(), 1);
        assert!(matches!(result[0], Value::I64(42)));
    }

    #[test]
    fn test_empty_tuple_splat_produces_no_args() {
        // f(()...) → f()
        let tuple = Value::Tuple(TupleValue::new(vec![]));
        let args = vec![tuple];
        let mask = vec![true];
        let result = expand_splat_arguments(args, &mask);
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_splat_mask_shorter_than_args_treats_extras_as_false() {
        // mask only covers first arg; second arg is NOT splatted
        let args = vec![i64_val(1), i64_val(2)];
        let mask = vec![false]; // shorter than args
        let result = expand_splat_arguments(args, &mask);
        assert_eq!(result.len(), 2);
    }
}
