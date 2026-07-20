//! Shared lexical environment for type binders introduced by `where` clauses.
//!
//! Binder names are lexical lookup keys only. Semantic identity is assigned by
//! the structured `CoreTypeVar` projection after lowering; this environment
//! prevents signature parsing, alias expansion, and function-body lowering
//! from maintaining divergent scope stacks (Issue #10436).

use std::cell::RefCell;

use crate::types::TypeParam;

thread_local! {
    static FRAMES: RefCell<Vec<Vec<TypeParam>>> = const { RefCell::new(Vec::new()) };
}

/// One lexical binder frame. Dropping the guard restores the enclosing scope.
pub struct Scope {
    active: bool,
}

impl Scope {
    pub fn new(type_params: &[TypeParam]) -> Self {
        if type_params.is_empty() {
            return Self { active: false };
        }
        FRAMES.with(|frames| frames.borrow_mut().push(type_params.to_vec()));
        Self { active: true }
    }

    pub fn from_names(names: &[String]) -> Self {
        let type_params = names
            .iter()
            .cloned()
            .map(TypeParam::new)
            .collect::<Vec<_>>();
        Self::new(&type_params)
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        if self.active {
            FRAMES.with(|frames| {
                frames.borrow_mut().pop();
            });
        }
    }
}

/// Resolve the nearest binder with this spelling.
pub fn resolve(name: &str) -> Option<TypeParam> {
    FRAMES.with(|frames| {
        frames
            .borrow()
            .iter()
            .rev()
            .find_map(|frame| frame.iter().rev().find(|param| param.name == name).cloned())
    })
}

pub fn contains(name: &str) -> bool {
    resolve(name).is_some()
}

pub fn is_active() -> bool {
    FRAMES.with(|frames| !frames.borrow().is_empty())
}
