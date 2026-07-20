//! Interprocedural analysis (IPO) support: call-graph construction.
//!
//! This module builds a call graph from IR functions and, in production, is used
//! only to collect the set of names a function body calls
//! (`call_graph::extract_called_functions`, consumed by `compile::stmt`).
//!
//! # Where interprocedural type inference actually lives
//!
//! Production interprocedural inference is NOT here. It is the recursion +
//! caching path of the abstract-interpretation engine
//! (`compile::abstract_interp::engine`), which infers and memoizes return types
//! per method specialization (`InferenceCacheKey` / `CodeInstanceKey`).
//!
//! The former `worklist::IPOInferenceEngine` was a never-wired stub whose
//! `infer_function_body` always returned `LatticeType::Top`; it and its
//! `recursion` (SCC) and `cache` helpers had no production caller and misled
//! investigators about where inference happens. They were removed (Issue #9205
//! acceptance criterion 2).

pub mod call_graph;

pub use call_graph::CallGraph;
