use super::super::value::Value;

/// Normalize Memory values to Array for dynamic arithmetic.
pub(super) fn normalize_memory(val: &Value) -> std::borrow::Cow<'_, Value> {
    match val {
        Value::Memory(mem) => {
            std::borrow::Cow::Owned(Value::Array(super::super::util::memory_to_array_ref(mem)))
        }
        other => std::borrow::Cow::Borrowed(other),
    }
}
