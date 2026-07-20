//! Interned identifier strings for Core IR nodes (Issue #10124).
//!
//! `Expr::Var`'s identifier field allocated a fresh `String` at every
//! occurrence — `x + x + x` built three separate `"x"` `String`s during
//! lowering. [`InternedStr`] is a `Copy` handle to a canonicalized,
//! process-lifetime string: repeated occurrences of the same identifier
//! share one allocation, equality is a pointer compare, and hashing does not
//! scan the string.
//!
//! # Design
//!
//! - **Global, mutex-guarded interner**, not `thread_local!`. A prelude
//!   `Function` tree (shared across compiles via `Arc`, Issue #9140) can be
//!   built on one thread and consumed from another; a `thread_local!`
//!   interner would give the *same* identifier text two *different* canonical
//!   pointers depending which thread interned it first, silently breaking
//!   `InternedStr == InternedStr`. A single process-wide table avoids that
//!   trap. (Design Principle 9 permits, but does not require, thread-local
//!   state — a global `Mutex` is the correct choice here specifically
//!   because interned identifiers escape their creating thread via `Arc`.)
//! - **Leaked, `'static` storage.** Identifiers are a bounded set (the
//!   variable/function names appearing in the user's source plus the
//!   prelude/Base source, at most a few tens of thousands of unique strings)
//!   — not user data — so leaking them for the life of the process is the
//!   same accepted trade-off well-established interners make, and it lets
//!   `InternedStr` hold a `&'static str` directly: `.as_str()` needs no
//!   interner lookup and no lifetime parameter on the type.
//! - **Identity `Eq`/`Hash`.** Equality is `ptr::eq` (pointer + length, O(1),
//!   no byte compare); `Hash` hashes the pointer address, not the string
//!   content, so hashing does not scan the string (the issue's "ハッシュ値の
//!   計算が文字列長に依存しない" goal). This is **only valid within a single
//!   process run** — `InternedStr`'s own `Hash`/`Eq` must never feed a
//!   persisted value or an RNG seed that needs to be reproducible across
//!   runs. Anything that needs a content-stable hash (cache fingerprinting,
//!   RNG seeding, etc.) must go through [`InternedStr::as_str`] and hash the
//!   resolved `&str` directly — exactly what the existing cache-fingerprint
//!   code in `compile/cache.rs` already does via `format!("{:?}", ...)`,
//!   which keeps working unchanged (see the `Debug` impl below).
//! - **`Ord`/`PartialOrd` by content**, not pointer, so anything that sorts
//!   identifiers for deterministic output (the codebase already has
//!   `compile/sorted_serde.rs` for exactly this concern) keeps working the
//!   same way it would for a `String`.
//! - **Byte-identical wire format.** `Serialize`/`Deserialize` round-trip
//!   through a plain string using the same encoding serde's `String` impl
//!   uses (`serializer.serialize_str`), so an `Expr` field changing from
//!   `String` to `InternedStr` is a byte-for-byte no-op for existing bincode
//!   caches (Base cache, prelude Program cache, REPL persistence) — no
//!   `CACHE_VERSION` bump needed for this change alone.

use std::collections::HashSet;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, OnceLock};

fn interner() -> &'static Mutex<HashSet<&'static str>> {
    static INTERNER: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    INTERNER.get_or_init(|| Mutex::new(HashSet::new()))
}

/// A `Copy` handle to a canonicalized, process-lifetime string.
///
/// Two `InternedStr`s compare equal iff they were interned from
/// byte-identical content (via [`InternedStr::new`] or any of the `From`
/// impls below), regardless of how many times that content was interned.
#[derive(Clone, Copy, Eq)]
pub struct InternedStr(&'static str);

impl InternedStr {
    /// Intern `s`, returning the canonical handle. Interning the same
    /// content again (from anywhere in the process) returns a handle that
    /// compares equal to this one.
    pub fn new(s: &str) -> Self {
        let mut set = interner().lock().unwrap_or_else(|e| e.into_inner());
        if let Some(&existing) = set.get(s) {
            return InternedStr(existing);
        }
        let leaked: &'static str = Box::leak(s.to_owned().into_boxed_str());
        set.insert(leaked);
        InternedStr(leaked)
    }

    /// Borrow the interned content. No interner lookup — the handle already
    /// holds the `&'static str` directly.
    #[inline]
    pub fn as_str(self) -> &'static str {
        self.0
    }
}

impl std::ops::Deref for InternedStr {
    type Target = str;
    #[inline]
    fn deref(&self) -> &str {
        self.0
    }
}

impl AsRef<str> for InternedStr {
    #[inline]
    fn as_ref(&self) -> &str {
        self.0
    }
}

impl PartialEq for InternedStr {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.0, other.0)
    }
}

impl Hash for InternedStr {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Pointer identity, not string content: O(1) regardless of length.
        // NOT stable across process runs — never persist this or use it to
        // seed anything that must reproduce across runs (see module docs).
        (self.0.as_ptr() as usize).hash(state);
        self.0.len().hash(state);
    }
}

impl PartialOrd for InternedStr {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for InternedStr {
    #[inline]
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // By content, so anything that sorts identifiers for deterministic
        // output gets the same order it would for a `String`.
        self.0.cmp(other.0)
    }
}

impl PartialEq<str> for InternedStr {
    #[inline]
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}
impl PartialEq<InternedStr> for str {
    #[inline]
    fn eq(&self, other: &InternedStr) -> bool {
        self == other.0
    }
}
impl PartialEq<&str> for InternedStr {
    #[inline]
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}
impl PartialEq<InternedStr> for &str {
    #[inline]
    fn eq(&self, other: &InternedStr) -> bool {
        *self == other.0
    }
}
impl PartialEq<String> for InternedStr {
    #[inline]
    fn eq(&self, other: &String) -> bool {
        self.0 == other.as_str()
    }
}
impl PartialEq<InternedStr> for String {
    #[inline]
    fn eq(&self, other: &InternedStr) -> bool {
        self.as_str() == other.0
    }
}

impl fmt::Debug for InternedStr {
    /// Delegates to `str`'s `Debug` so a derived `Debug` on an enum holding
    /// an `InternedStr` field renders byte-identically to when the field was
    /// a plain `String` (quoted + escaped content, no wrapper visible). This
    /// is what keeps `compile/cache.rs`'s `format!("{:?}", program.main)`
    /// cache-fingerprint hash unaffected by this migration.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.0, f)
    }
}

impl fmt::Display for InternedStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self.0, f)
    }
}

impl From<&str> for InternedStr {
    #[inline]
    fn from(s: &str) -> Self {
        InternedStr::new(s)
    }
}
impl From<String> for InternedStr {
    #[inline]
    fn from(s: String) -> Self {
        InternedStr::new(&s)
    }
}
impl From<&String> for InternedStr {
    #[inline]
    fn from(s: &String) -> Self {
        InternedStr::new(s.as_str())
    }
}

impl From<InternedStr> for String {
    #[inline]
    fn from(s: InternedStr) -> Self {
        s.0.to_owned()
    }
}

impl serde::Serialize for InternedStr {
    /// Same wire encoding as `String::serialize` (`serializer.serialize_str`)
    /// — see the module docs' "byte-identical wire format" note.
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.0)
    }
}

impl<'de> serde::Deserialize<'de> for InternedStr {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Deserialize as an owned String (borrowing isn't guaranteed by
        // every Deserializer, e.g. bincode's Read-based one) and intern it.
        let s = String::deserialize(deserializer)?;
        Ok(InternedStr::new(&s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interns_identical_content_to_the_same_handle() {
        let a = InternedStr::new("hello");
        let b = InternedStr::new("hello");
        assert_eq!(a, b);
        assert!(std::ptr::eq(a.as_str(), b.as_str()));
    }

    #[test]
    fn distinct_content_is_not_equal() {
        assert_ne!(InternedStr::new("foo"), InternedStr::new("bar"));
    }

    #[test]
    fn compares_equal_to_str_and_string_both_directions() {
        let a = InternedStr::new("x");
        assert_eq!(a, "x");
        assert_eq!("x", a);
        assert_eq!(a, String::from("x"));
        assert_eq!(String::from("x"), a);
    }

    #[test]
    fn debug_matches_plain_string_debug() {
        let a = InternedStr::new("a\"b");
        assert_eq!(format!("{:?}", a), format!("{:?}", "a\"b"));
    }

    #[test]
    fn display_matches_plain_str() {
        let a = InternedStr::new("hello");
        assert_eq!(format!("{a}"), "hello");
    }

    #[test]
    fn orders_by_content_not_identity() {
        let a = InternedStr::new("apple_unique_order_test");
        let b = InternedStr::new("banana_unique_order_test");
        assert!(a < b);
    }

    #[test]
    fn serde_round_trips_and_matches_string_wire_bytes() {
        let interned = InternedStr::new("round_trip_me");
        let interned_bytes = bincode::serialize(&interned).expect("serialize InternedStr");
        let plain_bytes =
            bincode::serialize(&"round_trip_me".to_string()).expect("serialize String");
        assert_eq!(interned_bytes, plain_bytes);

        let back: InternedStr = bincode::deserialize(&interned_bytes).expect("deserialize");
        assert_eq!(back, "round_trip_me");
    }

    #[test]
    fn deref_gives_str_methods() {
        let a = InternedStr::new("__paren_func_1");
        assert!(a.starts_with("__paren_func_"));
        assert_eq!(a.len(), "__paren_func_1".len());
    }
}
