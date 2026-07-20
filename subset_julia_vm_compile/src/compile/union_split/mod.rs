//! Union splitting for specialized code generation.
//!
//! This module implements union splitting, which generates specialized code paths
//! for union-typed values. When a value has a union type like `Union{Int, String}`,
//! union splitting allows the compiler to generate optimized code for each branch
//! where the type is narrowed.
//!
//! # Example
//!
//! ```julia
//! function f(x::Union{Int, String})
//!     if x isa Int
//!         return x * 2        # Specialized Int path - x narrowed to Int
//!     else
//!         return length(x)    # Specialized String path - x narrowed to String
//!     end
//! end
//! ```
//!
//! Without union splitting, operations on `x` must handle all possible types
//! dynamically. With union splitting, each branch gets optimized code for the
//! specific type, enabling:
//!
//! - Elimination of runtime type checks
//! - Use of type-specific operations
//! - Better optimization opportunities
//!
//! # Architecture
//!
//! The union splitting system consists of four main components:
//!
//! 1. **Detection** (`detection.rs`): Identifies union-typed variables and
//!    conditions that can benefit from splitting (isa checks, nothing checks).
//!
//! 2. **Environment Splitting** (`env_split.rs`): Splits type environments
//!    for then/else branches, narrowing types appropriately.
//!
//! 3. **Code Specialization** (`specialize.rs`): Generates specialized code
//!    for each branch using narrowed type information.
//!
//! 4. **Result Merging** (`merge.rs`): Merges types and effects from
//!    specialized branches back into a unified result.
//!
//! # Usage
//!
//! The union splitting API is designed for internal compiler use. Here's
//! a conceptual example of how the components work together:
//!
//! ```no_run
//! use subset_julia_vm::compile::union_split::{
//!     find_split_candidates, SplitCondition, UnionSplitCandidate,
//! };
//!
//! // The typical workflow:
//! // 1. Analyze a function to find union-typed conditions
//! // 2. For each candidate, split the environment and specialize code
//! // 3. Merge results back together
//!
//! // find_split_candidates(&function, &env) returns candidates
//! // split_and_specialize(&candidate, &function, &env) specializes code
//! ```

pub mod detection;
pub mod env_split;
pub mod merge;
pub mod specialize;

pub use detection::{find_split_candidates, SplitCondition, UnionSplitCandidate};
pub use env_split::split_environment;
pub use merge::merge_split_results;
pub use specialize::specialize_block;
