#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

pub mod abstract_;
mod closure_box;
pub mod expr;
pub mod function;
pub mod generated_unquote;
// VM-backed macro expansion moved to the integration crate root
// (`crate::macro_runtime`, Issue #8656): lowering reaches it only through
// the `macro_expander::MacroExpander` seam so lowering-core keeps zero
// upward (compile/vm) edges.
pub mod macro_expander;
pub mod macros_registry;
pub mod primitive;
pub(crate) mod scope_bindings;
pub mod soft_scope;
pub mod stmt;
pub mod struct_;
pub mod type_alias;
pub mod type_binder_env;

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use std::collections::HashMap;

use crate::error::{IncludeError, UnsupportedFeature, UnsupportedFeatureKind};
use crate::include::{read_include_file, resolve_include_path};
use crate::ir::core::{
    AbstractTypeDef, Block, BuiltinOp, Expr, Function, KwParam, Literal, MacroDef, Module,
    PrimitiveTypeDef, Program, RuntimeNominalDef, Stmt, StructDef, TypeAliasDef, TypedParam,
    UsingImport,
};
use crate::parser::cst::{CstWalker, Node, NodeKind};
use crate::parser::{ParseOutcome, Parser};
use crate::span::Span;
use crate::types::TypeParam;
use macros_registry::{
    check_type_compatibility, ensure_bundled_package_macros_loaded, ensure_stdlib_macros_loaded,
};

pub use macros_registry::{get_node_macro_type, MacroHygieneInfo, MacroParamType, StoredMacroDef};

pub type LowerResult<T> = Result<T, UnsupportedFeature>;

/// Result type for include operations that can fail with IncludeError.
pub type IncludeResult<T> = Result<T, IncludeError>;

/// Build an internal-error [`UnsupportedFeature`] for a proof-backed lowering
/// invariant that should be unreachable given the immediately preceding
/// control flow. Mirrors the parser crate's `internal_parser_error` helper
/// (Issue #10904): a lowering-side invariant break must surface as a typed
/// error instead of an uncaught host crash if a future refactor ever
/// invalidates the precondition (Issue #10905, Phase 1b of #10869).
pub fn internal_lowering_error(span: Span, context: &str) -> UnsupportedFeature {
    UnsupportedFeature::new(
        UnsupportedFeatureKind::Other(format!("internal lowering error: {context}")),
        span,
    )
}

/// Returns `true` when the node at `idx` is a top-level docstring — a
/// `StringLiteral` whose next non-comment sibling is a definition. Matches
/// Julia's docstring convention (`"""doc""" function f end` → `@doc "doc" f`)
/// so the string is documentation rather than a value-producing statement.
fn is_top_level_docstring(walker: &CstWalker<'_>, children: &[Node<'_>], idx: usize) -> bool {
    let mut next_idx = idx + 1;
    while let Some(next) = children.get(next_idx) {
        match walker.kind(next) {
            NodeKind::LineComment | NodeKind::BlockComment => {
                next_idx += 1;
            }
            next_kind => return is_docstring_target_kind(next_kind),
        }
    }
    false
}

const DOC_BINDING_PREFIX: &str = "__sjulia_doc_";

/// True for a generated doc-registration binding name produced by
/// [`push_doc_registration`] (the `var` of the `Stmt::Assign {
/// var: "__sjulia_doc_<Name>", .. }` a docstring-preceded definition lowers
/// to). These `Assign`s register a docstring for `@doc` lookup — they are
/// compiler-internal bookkeeping, not user-typed value statements, so callers
/// determining a REPL echo value must NOT count them (Issue #10164).
pub fn is_doc_registration_binding(var: &str) -> bool {
    var.starts_with(DOC_BINDING_PREFIX)
}

/// True for a `Stmt` that is a generated doc-registration assignment (see
/// [`is_doc_registration_binding`]). Used by the REPL to treat a `main` block
/// whose only statements are doc registrations as definition-only, so a
/// documented `struct`/`abstract type`/definition does not echo its docstring
/// as the eval result (Issue #10164).
pub fn is_doc_registration_stmt(stmt: &Stmt) -> bool {
    matches!(stmt, Stmt::Assign { var, .. } if is_doc_registration_binding(var))
}

fn doc_binding_name(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    format!("{DOC_BINDING_PREFIX}{sanitized}")
}

fn doc_registration_stmt(name: &str, doc: Expr, span: Span) -> Stmt {
    Stmt::Assign {
        var: doc_binding_name(name),
        value: doc,
        span,
    }
}

fn push_doc_registration(
    stmts: &mut Vec<Stmt>,
    pending_doc: &mut Option<(Expr, Span)>,
    names: impl IntoIterator<Item = String>,
) {
    let Some((doc, span)) = pending_doc.take() else {
        return;
    };
    let names: Vec<String> = names.into_iter().collect();
    if names.is_empty() {
        return;
    }
    for name in names {
        stmts.push(doc_registration_stmt(&name, doc.clone(), span));
    }
}

/// Names bound by a lowered top-level `const` statement, for doc registration
/// (Issue #10164, general fix alongside the `pending_doc` capture below).
/// `is_docstring_target_kind` treats `ConstStatement` as a valid docstring
/// target (`"""doc""" const X = value` is documented, same as upstream Julia),
/// but the `ConstStatement` arm previously never called
/// [`push_doc_registration`] — so a preceding docstring was neither attached
/// to `X` nor cleared, and silently leaked forward to whatever later
/// definition happened to consume `pending_doc` next (across a file boundary
/// in the Base prelude, this misattributed `VERSION`'s docstring to an
/// unrelated function in the next file). `lower_const_statement` /
/// `wrap_const_assignment` wraps a simple `const X = value` into
/// `Stmt::Block([declare_const call, Stmt::Assign { var: X, .. }])`; a
/// type-alias-only const (`const X = SomeType{P}`) or a const with no
/// assignment lowers to a bare non-`Assign` statement, which yields no names
/// here so [`push_doc_registration`] safely drops the pending docstring
/// instead of misattributing it.
fn const_statement_doc_names(stmt: &Stmt) -> Vec<String> {
    match stmt {
        Stmt::Assign { var, .. } => vec![var.clone()],
        Stmt::Block(block) => block
            .stmts
            .iter()
            .filter_map(|s| match s {
                Stmt::Assign { var, .. } => Some(var.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn contains_macro_call(walker: &CstWalker<'_>, node: Node<'_>) -> bool {
    walker.kind(&node) == NodeKind::MacroCall
        || walker
            .named_children(&node)
            .any(|child| contains_macro_call(walker, child))
}

/// Central routing authority for function-definition lowering (Issues #10936,
/// #10965): the typed enumeration of everything a function-definition subtree
/// can require from the live [`LambdaContext`].
///
/// Every function-definition entry surface (short form, full form, block/local
/// definition, operator definition, macro-/eval-generated definition) must
/// route through this authority — via [`requires_lambda_context`] /
/// [`requires_nested_lambda_lowering`] or the `lower_*_with_ctx_if_needed`
/// helpers below — instead of consulting a narrow predicate such as
/// `contains_macro_call` directly. `scripts/check_lambda_context_routing.sh`
/// enforces this structurally.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FunctionLoweringCapabilities {
    /// A macro call occurs in the subtree: expansion needs the context's macro
    /// tables, hygiene state, and lifted-definition queues.
    pub macro_expansion: bool,
    /// A `where` binder occurs in the subtree: body type expressions must be
    /// able to distinguish a lexical binder from a same-spelled builtin type
    /// (Issue #10934), and nested definitions must be lowered through the
    /// context so binder state threads into them.
    pub where_binders: bool,
    /// The subtree applies type parameters (`T{...}`): lexical binder and
    /// value-parameter lookup must be available while lowering the application
    /// (Issue #10948). This capability alone must NOT switch closures to the
    /// nested lowering representation or change capture analysis — that
    /// regression is exactly what Issue #10965 tracks.
    pub parametric_type_applications: bool,
}

impl FunctionLoweringCapabilities {
    /// Whether lowering must retain the live [`LambdaContext`] at all.
    pub fn requires_context(self) -> bool {
        self.macro_expansion || self.where_binders || self.parametric_type_applications
    }

    /// Whether the live context must also switch closures to the nested
    /// lowering path. Deliberately EXCLUDES `parametric_type_applications`: a
    /// static parametric type expression (`Tuple{Int64}`, `Complex{Float64}`)
    /// in an otherwise ordinary body must not change closure representation
    /// (Issues #10948, #10965).
    pub fn requires_nested_lambda_lowering(self) -> bool {
        self.macro_expansion || self.where_binders
    }
}

/// Compute the [`FunctionLoweringCapabilities`] of a function-definition
/// subtree. This is the only place the narrow structural predicates may be
/// consulted; routing call sites use the derived `requires_*` views.
pub fn function_lowering_capabilities(
    walker: &CstWalker<'_>,
    node: Node<'_>,
) -> FunctionLoweringCapabilities {
    FunctionLoweringCapabilities {
        macro_expansion: contains_macro_call(walker, node),
        where_binders: contains_where_binder(walker, node),
        parametric_type_applications: contains_parametrized_type_expression(walker, node),
    }
}

/// Whether lowering must retain the live [`LambdaContext`]. Besides macros,
/// function `where` binders need the context so body type expressions can
/// distinguish a lexical binder from a same-spelled builtin type (Issue
/// #10934). Derived view of [`function_lowering_capabilities`].
pub fn requires_lambda_context(walker: &CstWalker<'_>, node: Node<'_>) -> bool {
    function_lowering_capabilities(walker, node).requires_context()
}

/// Whether the live context must also switch closures to the nested lowering
/// path. Parametric type applications need lexical binding state for Issue
/// #10948, but that alone must not change closure representation or capture
/// analysis in an otherwise ordinary function body. Derived view of
/// [`function_lowering_capabilities`].
pub fn requires_nested_lambda_lowering(walker: &CstWalker<'_>, node: Node<'_>) -> bool {
    function_lowering_capabilities(walker, node).requires_nested_lambda_lowering()
}

fn contains_where_binder(walker: &CstWalker<'_>, node: Node<'_>) -> bool {
    matches!(
        walker.kind(&node),
        NodeKind::WhereExpression | NodeKind::WhereClause
    ) || walker
        .named_children(&node)
        .any(|child| contains_where_binder(walker, child))
}

fn contains_parametrized_type_expression(walker: &CstWalker<'_>, node: Node<'_>) -> bool {
    walker.kind(&node) == NodeKind::ParametrizedTypeExpression
        || walker
            .named_children(&node)
            .any(|child| contains_parametrized_type_expression(walker, child))
}

pub fn contains_value_parametric_base(
    walker: &CstWalker<'_>,
    node: Node<'_>,
    params: &[TypedParam],
    kwparams: &[KwParam],
) -> bool {
    let names: HashSet<String> = params
        .iter()
        .flat_map(function::parameter_binding_names)
        .chain(kwparams.iter().map(|param| param.name.clone()))
        .collect();
    contains_parametric_base_named(walker, node, &names)
}

fn contains_parametric_base_named(
    walker: &CstWalker<'_>,
    node: Node<'_>,
    names: &HashSet<String>,
) -> bool {
    if walker.kind(&node) == NodeKind::ParametrizedTypeExpression {
        if let Some(base) = walker.named_children(&node).next() {
            if walker.kind(&base) == NodeKind::Identifier && names.contains(walker.text(&base)) {
                return true;
            }
        }
    }
    walker
        .named_children(&node)
        .any(|child| contains_parametric_base_named(walker, child, names))
}

pub fn lower_function_all_with_ctx_if_needed<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Vec<Function>> {
    if requires_lambda_context(walker, node) {
        function::lower_function_all_with_ctx(walker, node, lambda_ctx)
    } else {
        function::lower_function_all(walker, node)
    }
}

pub fn lower_operator_method_with_ctx_if_needed<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Function> {
    if requires_lambda_context(walker, node) {
        function::lower_operator_method_with_ctx(walker, node, lambda_ctx)
    } else {
        function::lower_operator_method(walker, node)
    }
}

pub fn lower_short_function_all_with_ctx_if_needed<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Vec<Function>> {
    if requires_lambda_context(walker, node) {
        function::lower_short_function_all_with_ctx(walker, node, lambda_ctx)
    } else {
        function::lower_short_function_all(walker, node)
    }
}

/// Lower a global helper declared inside a struct body while preserving that
/// body's privileged `new` binding and the active source-file lambda context.
///
/// The authority scope must be established before lowering so every lifted
/// descendant is stamped at creation time. Keep this semantic exception in the
/// central routing authority so the direct-lowering audit can distinguish it
/// from accidental bypasses (Issues #11005, #11179, #11197).
pub fn lower_struct_global_function_all<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    struct_name: &str,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Vec<Function>> {
    match lambda_ctx {
        Some(ctx) => ctx.with_new_struct_authority(Some(struct_name), || {
            lower_function_all_with_ctx_if_needed(walker, node, ctx)
        }),
        None => function::lower_function_all(walker, node),
    }
}

pub fn lower_struct_global_short_function_all<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    struct_name: &str,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Vec<Function>> {
    match lambda_ctx {
        Some(ctx) => ctx.with_new_struct_authority(Some(struct_name), || {
            lower_short_function_all_with_ctx_if_needed(walker, node, ctx)
        }),
        None => function::lower_short_function_all(walker, node),
    }
}

/// Lower a `lower_module_definition` catch-all-arm statement, threading the
/// module's `LambdaContext` through whenever one is available.
///
/// Despite the historical name, this now threads `macro_ctx` unconditionally
/// (not just when `contains_macro_call` finds a macro call in `node`),
/// mirroring `Lowering::lower_source_file_inner` and
/// `LoweringWithInclude::lower_source_file_inner`'s catch-all arms, which
/// always pass `Some(&lambda_ctx)` regardless of whether the statement
/// contains a macro call. Every call site passes `macro_ctx = Some(..)` in
/// practice (`lower_module_definition` is always invoked with a live
/// context), so the old `contains_macro_call` gate was purely an
/// optimization that left module bodies inconsistent with true top level:
/// a `let`/`begin` statement with no macro call anywhere (e.g. a nested
/// `struct`) took the ctx-less path here and fell back to sjulia's
/// unsupported-`struct`-nesting error, even though the identical statement
/// at Program/file top level already worked (Issue #10382).
fn lower_stmt_with_macro_ctx_if_needed<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    macro_ctx: Option<&LambdaContext>,
) -> LowerResult<Stmt> {
    match macro_ctx {
        Some(ctx) => stmt::lower_stmt_with_ctx(walker, node, ctx),
        None => stmt::lower_stmt(walker, node),
    }
}

/// Kinds of top-level constructs that absorb a preceding string literal as a
/// docstring. Mirrors the definition-producing arms of `lower_source_file`.
fn is_docstring_target_kind(kind: NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::FunctionDefinition
            | NodeKind::ShortFunctionDefinition
            | NodeKind::MacroDefinition
            | NodeKind::StructDefinition
            | NodeKind::MutableStructDefinition
            | NodeKind::AbstractDefinition
            | NodeKind::PrimitiveDefinition
            | NodeKind::ModuleDefinition
            | NodeKind::BaremoduleDefinition
            | NodeKind::ConstStatement
            | NodeKind::Assignment
            | NodeKind::MacroCall
    )
}

fn explicit_doc_module_target<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> Option<(Node<'a>, bool)> {
    if walker.kind(&node) != NodeKind::MacroCall {
        return None;
    }

    let macro_ident = walker.find_child(&node, NodeKind::MacroIdentifier)?;
    let name = walker.text(&macro_ident).trim_start_matches('@');
    if name != "doc" {
        return None;
    }

    walker
        .named_children(&node)
        .find_map(|child| match walker.kind(&child) {
            NodeKind::ModuleDefinition => Some((child, false)),
            NodeKind::BaremoduleDefinition => Some((child, true)),
            _ => None,
        })
}

/// Pre-scan a CST subtree and register every user-defined type alias (both the
/// non-parametric `const Name = T` form and the parametric `Name{P...} = T`
/// form) into the thread-local alias table (Issue #5055). Running this before
/// statement lowering lets alias uses anywhere in the program resolve to their
/// target type strings, independent of source order. Descends into module
/// bodies so module-level aliases are visible too.
fn prescan_and_register_type_aliases(
    walker: &CstWalker<'_>,
    node: Node<'_>,
    module_owner: &mut Vec<String>,
    source_scope: &type_alias::SourceScope,
) {
    // Issue #11104: the type NAMES the program declares must be known before the
    // alias gate runs, because `const AE = E` is a type alias exactly when `E`
    // names a type. A `struct` may be declared after the `const` that aliases it,
    // so collect the declarations of this subtree first, in a separate walk.
    prescan_and_register_declared_types(walker, node);
    // Iterate the binding walk to a fixpoint: an alias OF an alias
    // (`const BE = AE`) only becomes recognizable once `AE` itself is
    // registered, and the two bindings may appear in either source order.
    // Chains are short; the walk is bounded so a pathological program cannot
    // spin here.
    for _ in 0..8 {
        let before = type_alias::registered_count();
        prescan_type_alias_bindings(walker, node, module_owner, source_scope);
        if type_alias::registered_count() == before {
            break;
        }
    }
}

/// Register every type declared in this CST subtree (`struct`, `mutable
/// struct`, `abstract type`, `primitive type`), descending into module bodies
/// and nested blocks (Issue #11104).
fn prescan_and_register_declared_types(walker: &CstWalker<'_>, node: Node<'_>) {
    for child in walker.named_children(&node) {
        match walker.kind(&child) {
            NodeKind::StructDefinition
            | NodeKind::MutableStructDefinition
            | NodeKind::AbstractDefinition
            | NodeKind::PrimitiveDefinition => {
                if let Some(name) = declared_type_head_name(walker.text(&child)) {
                    type_alias::register_declared_type(&name);
                }
            }
            NodeKind::ModuleDefinition | NodeKind::BaremoduleDefinition => {
                for inner in walker.named_children(&child) {
                    if walker.kind(&inner) == NodeKind::Block {
                        prescan_and_register_declared_types(walker, inner);
                    }
                }
            }
            NodeKind::Block => {
                prescan_and_register_declared_types(walker, child);
            }
            _ => {}
        }
    }
}

/// Extract the declared type name from the source text of a type-definition
/// node: `"mutable struct Wrap{T} <: Base ... end"` -> `Some("Wrap")`.
/// Leading definition keywords are skipped and the head name is cut at its
/// type-parameter list / subtype operator (Issue #11104).
fn declared_type_head_name(text: &str) -> Option<String> {
    let head = text.split_whitespace().find(|tok| {
        !matches!(
            *tok,
            "mutable" | "struct" | "abstract" | "primitive" | "type"
        )
    })?;
    let name: String = head
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '!')
        .collect();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// The alias-binding half of the pre-scan: register every `const Name = T` /
/// `Name{P...} = T` binding of this subtree into the alias table.
fn prescan_type_alias_bindings(
    walker: &CstWalker<'_>,
    node: Node<'_>,
    module_owner: &mut Vec<String>,
    source_scope: &type_alias::SourceScope,
) {
    for child in walker.named_children(&node) {
        match walker.kind(&child) {
            NodeKind::ConstStatement => {
                if let Some(alias) = stmt::try_extract_type_alias(walker, child) {
                    type_alias::register_prescanned(
                        &alias.name,
                        alias.params.clone(),
                        &alias.target_type,
                        source_scope.position(alias.span.start),
                        module_owner,
                    );
                } else if let Some(name) = prescanned_value_binding_name(walker, child) {
                    type_alias::register_prescanned_non_alias(
                        &name,
                        source_scope.position(walker.span(&child).start),
                        module_owner,
                    );
                }
            }
            NodeKind::Assignment => {
                if let Some(alias) = stmt::try_extract_type_alias_from_assignment(walker, child) {
                    type_alias::register_prescanned(
                        &alias.name,
                        alias.params.clone(),
                        &alias.target_type,
                        source_scope.position(alias.span.start),
                        module_owner,
                    );
                } else if !function::is_short_function_definition(walker, child) {
                    if let Some(name) = prescanned_value_binding_name(walker, child) {
                        type_alias::register_prescanned_non_alias(
                            &name,
                            source_scope.position(walker.span(&child).start),
                            module_owner,
                        );
                    }
                }
            }
            NodeKind::UsingStatement | NodeKind::ImportStatement => {
                // Record which modules this lexical scope imports, so the
                // bare-name alias fallback stays limited to visible owners
                // (Issue #11452).
                for path in imported_module_paths(walker.text(&child)) {
                    type_alias::register_import_edge(module_owner, &path);
                }
            }
            NodeKind::ModuleDefinition | NodeKind::BaremoduleDefinition => {
                // Recurse into module bodies (their `Block` child) so aliases
                // defined inside modules are registered as well.
                if let Some(name_node) = walker.child_by_field(&child, "name") {
                    if walker.kind(&name_node) == NodeKind::Identifier {
                        let name = walker.text(&name_node).to_string();
                        module_owner.push(name.clone());
                        let _module_scope = type_alias::ModuleScope::new(&name);
                        for inner in walker.named_children(&child) {
                            if walker.kind(&inner) == NodeKind::Block {
                                prescan_type_alias_bindings(
                                    walker,
                                    inner,
                                    module_owner,
                                    source_scope,
                                );
                            }
                        }
                        module_owner.pop();
                    }
                }
            }
            NodeKind::Block => {
                prescan_type_alias_bindings(walker, child, module_owner, source_scope);
            }
            _ => {}
        }
    }
}

/// Parse the module paths named by a `using`/`import` statement's source text:
/// `"using .M: A, b"` -> `[["M"]]`, `"using A.B, C"` -> `[["A","B"], ["C"]]`.
/// Leading relative dots are stripped; an `as` rename keeps the original path.
/// Only used to widen bare-alias owner visibility, so the spelled path (not a
/// resolved absolute path) is sufficient (Issue #11452).
fn imported_module_paths(text: &str) -> Vec<Vec<String>> {
    let rest = text
        .trim_start()
        .strip_prefix("using")
        .or_else(|| text.trim_start().strip_prefix("import"))
        .unwrap_or(text);
    // `using M: names` imports only from `M`; without `:` every comma-separated
    // spec names a module.
    let specs = rest.split(':').next().unwrap_or(rest);
    specs
        .split(',')
        .filter_map(|spec| {
            let spec = spec.trim();
            let spec = spec.split_whitespace().next().unwrap_or(spec);
            let path: Vec<String> = spec
                .trim_start_matches('.')
                .split('.')
                .filter(|seg| !seg.is_empty())
                .map(str::to_string)
                .collect();
            if path.is_empty() {
                None
            } else {
                Some(path)
            }
        })
        .collect()
}

/// Return the simple identifier rebound by a non-alias `const A = value` or
/// `A = value` node. Composite destructuring and function-definition LHS forms
/// deliberately return `None`; they do not replace one alias binding by name.
fn prescanned_value_binding_name(walker: &CstWalker<'_>, node: Node<'_>) -> Option<String> {
    match walker.kind(&node) {
        NodeKind::ConstStatement => walker
            .named_children(&node)
            .find_map(|child| prescanned_value_binding_name(walker, child)),
        NodeKind::Assignment | NodeKind::BinaryExpression => {
            let lhs = walker.named_children(&node).next()?;
            (walker.kind(&lhs) == NodeKind::Identifier).then(|| walker.text(&lhs).to_string())
        }
        _ => None,
    }
}

fn extract_top_level_function_defs(stmt: Stmt) -> Result<Vec<Function>, Box<Stmt>> {
    match stmt {
        Stmt::FunctionDef { func, .. } => Ok(vec![*func]),
        Stmt::Block(block)
            if block
                .stmts
                .iter()
                .all(|stmt| matches!(stmt, Stmt::FunctionDef { .. })) =>
        {
            let mut funcs = Vec::new();
            for stmt in block.stmts {
                if let Stmt::FunctionDef { func, .. } = stmt {
                    funcs.push(*func);
                }
            }
            Ok(funcs)
        }
        other => Err(Box::new(other)),
    }
}

/// Module-body variant of [`extract_top_level_function_defs`]
/// (Issue #10874): a `@eval` function definition must BOTH hoist into the
/// module's function list (module functions are collected from the lowered
/// `Module`, unlike the top-level `Program` whose compile-time collector
/// walks `EvalFunctionDef` statements directly) AND stay in the body as the
/// runtime `DefineEvalFunction` statement — mirroring the top-level
/// behavior, where collection and the statement both happen.
fn extract_module_function_defs(stmt: Stmt) -> (Vec<Function>, Option<Box<Stmt>>) {
    match extract_top_level_function_defs(stmt) {
        Ok(funcs) => (funcs, None),
        Err(stmt) => match *stmt {
            // No `new_struct_name` re-stamping here: mutation authority is
            // restricted to the root/lifted/collector sites (Issue #11211
            // R4), and a module-body `@eval` definition is lowered outside
            // any struct authority, so the field is already `None`.
            Stmt::EvalFunctionDef { ref func, .. } => (vec![(**func).clone()], Some(stmt)),
            _ => (Vec::new(), Some(stmt)),
        },
    }
}

fn extend_source_function_definitions(
    ctx: Option<&LambdaContext>,
    functions: &mut Vec<Function>,
    mut definitions: Vec<Function>,
) {
    if let Some(ctx) = ctx {
        ctx.stamp_function_definitions(&mut definitions);
    }
    functions.extend(definitions);
}

/// Move a struct body's `global` helper methods into the same source function
/// list as the struct's enclosing scope. Keeping them inside `StructDef` after
/// the definition is registered makes the ordinary global binding disappear
/// on transparent-block and macro-expansion paths (Issue #11186).
fn extend_struct_global_helpers(
    ctx: Option<&LambdaContext>,
    functions: &mut Vec<Function>,
    definition: &mut StructDef,
) {
    let mut helpers = std::mem::take(&mut definition.global_new_helpers);
    if helpers.is_empty() {
        return;
    }
    if let Some(ctx) = ctx {
        ctx.stamp_function_definitions(&mut helpers);
        ctx.add_compile_time_functions(&helpers);
    }
    functions.extend(helpers);
}

fn drain_macro_expanded_structs(
    ctx: &LambdaContext,
    structs: &mut Vec<StructDef>,
    functions: &mut Vec<Function>,
) {
    let mut expanded = ctx.take_macro_expanded_structs();
    for definition in &mut expanded {
        ctx.stamp_struct_definition(definition);
        extend_struct_global_helpers(Some(ctx), functions, definition);
    }
    structs.extend(expanded);
}

/// Move macros produced by `@doc "…" macro …` expansion (Issue #9159) from the
/// lowering context into the Program/Module `macros` list, so they are exported
/// via `using` — the bundled-package macro registry reads `module.macros`, not
/// the lowering context (Issue #9185). Paired with `drain_macro_expanded_structs`
/// at every top-level/module-body drain site so a macro is attributed to the
/// scope whose statement defined it.
fn drain_macro_expanded_macros(ctx: &LambdaContext, macros: &mut Vec<MacroDef>) {
    macros.extend(ctx.take_macro_expanded_macros());
}

fn reject_macro_expanded_structs_in_non_toplevel(
    ctx: &LambdaContext,
    span: crate::span::Span,
) -> LowerResult<()> {
    let structs = ctx.take_macro_expanded_structs();
    if structs.is_empty() {
        return Ok(());
    }
    Err(
        UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
            "macro-expanded struct definitions are only supported at top level or module body",
        ),
    )
}

/// Like [`reject_macro_expanded_structs_in_non_toplevel`], but only inspects
/// structs queued *after* `watermark` instead of draining the whole queue.
///
/// Callers at true Program/module top level may use the full-drain version
/// safely because each statement there drains the queue immediately after
/// itself, so by the time a nested `FunctionDefinition` child is lowered the
/// queue is guaranteed to hold only that function's own (illegal) structs.
/// That invariant does NOT hold inside [`stmt::lower_stmt_impl`]: a
/// `FunctionDefinition` there may be a *sibling* of an earlier, legally
/// queued struct within the same still-undrained transparent block (e.g. a
/// top-level `let` containing `struct Good; ...; end` followed by
/// `function f(); @show 1; end`) — draining unconditionally would reject
/// that legal program and silently discard `Good`. Comparing against a
/// watermark (`LambdaContext::macro_expanded_struct_count`) taken right
/// before lowering the function isolates exactly what the function's own
/// body queued, mirroring `take_lifted_functions_from`'s watermark pattern
/// (Issue #10402).
pub fn reject_macro_expanded_structs_added_since(
    ctx: &LambdaContext,
    watermark: usize,
    span: crate::span::Span,
) -> LowerResult<()> {
    let structs = ctx.take_macro_expanded_structs_from(watermark);
    if structs.is_empty() {
        return Ok(());
    }
    Err(
        UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
            "macro-expanded struct definitions are only supported at top level or module body",
        ),
    )
}

/// Context for collecting lifted lambda functions during lowering.
/// Used when lambdas appear as arguments to function calls (e.g., `map(x -> x^2, arr)`).
/// Also tracks which modules have been imported via `using` for macro availability.
/// Also stores user-defined macro definitions for expansion.
pub struct LambdaContext {
    lifted_functions: RefCell<Vec<Function>>,
    lambda_counter: RefCell<usize>,
    /// Optional REPL-fragment namespace for compiler-generated anonymous
    /// function names. Independent source fragments otherwise restart their
    /// counters/spans and can create same-named helpers in one live session.
    anonymous_name_namespace: Option<u64>,
    /// Shared by a root source and every recursively included file so
    /// definition order is comparable even though their byte spans are not.
    definition_counter: Cell<u64>,
    /// Modules imported via `using` statements, used to gate macro availability
    usings: RefCell<HashSet<String>>,
    /// User-defined macro definitions, indexed by name, supporting multiple arities
    macros: RefCell<HashMap<String, Vec<StoredMacroDef>>>,
    /// Top-level functions visible to expansion-time macro execution.
    compile_time_functions: RefCell<Vec<Function>>,
    /// Top-level type definitions visible to expansion-time macro execution. A
    /// bundled-package macro (e.g. Plots' `@animate`) is expanded by compiling and
    /// running ALL `compile_time_functions`; if one of those references a user struct
    /// (e.g. `step!(l::Lorenz)` reading `l.x`), the expansion program needs the type
    /// definitions too, or compilation fails with "Unknown field: …".
    compile_time_structs: RefCell<Vec<StructDef>>,
    /// Struct definitions produced by macro expansion while lowering a
    /// top-level/module-body statement. The surrounding top-level lowering pass
    /// drains this immediately and adds the definitions to Program/Module metadata
    /// (Issue #7915).
    macro_expanded_structs: RefCell<Vec<StructDef>>,
    /// Macro definitions produced by expanding a macro that returns a `macro`
    /// definition — notably `@doc "…" macro name(…) … end`, which reaches
    /// statement conversion as `Expr(:macro, …)` (Issue #9159). Like
    /// `macro_expanded_structs`, the surrounding top-level/module lowering pass
    /// drains this so the macro lands in the Program/Module `macros` list and is
    /// exported via `using` (Issue #9185); `add_macro` alone only makes it
    /// resolvable within the same lowering context, not across a bundled-package
    /// boundary.
    macro_expanded_macros: RefCell<Vec<MacroDef>>,
    compile_time_abstract_types: RefCell<Vec<AbstractTypeDef>>,
    compile_time_primitive_types: RefCell<Vec<PrimitiveTypeDef>>,
    /// Hygiene info for module-defined macros (Issue #7355 / #7350 A4): macro name
    /// → defining module metadata. Drives qualification of a macro's non-`esc`
    /// call targets so unexported module members resolve in the macro's
    /// *defining* module rather than the caller scope.
    module_macro_hygiene: RefCell<HashMap<String, MacroHygieneInfo>>,
    /// Active hygiene frames while converting a module-defined macro's returned AST
    /// to IR. Each frame carries the defining module, its member names, and the
    /// current `esc` nesting depth (qualification is suppressed inside `esc`).
    macro_hygiene_stack: RefCell<Vec<MacroHygieneFrame>>,
    /// Current file path (for @__FILE__ and @__DIR__ macros)
    /// None means REPL or unknown source
    current_file: RefCell<Option<String>>,
    /// Function value parameters currently in scope while lowering a body.
    /// A parameter shadows an equally named global type constructor in every
    /// value position, including the base of `T{...}` (Issue #10948).
    active_value_params: RefCell<Vec<HashSet<String>>>,
    /// Stack of enclosing module names while lowering module bodies. A macro
    /// expanded inside `module M ... end` must receive `M` as `__module__`, not
    /// the hard-coded `Main` (Issue #7919). The top of the stack is the
    /// innermost active module; an empty stack means top-level (`Main`).
    current_module_stack: RefCell<Vec<String>>,
    /// Lexically active struct owner for lifted functions that may use the
    /// privileged `new`. Runtime `@eval` temporarily clears this authority.
    active_new_struct_name: RefCell<Option<String>>,
    prefer_nested_lambdas: Cell<bool>,
    /// Dynamic nesting depth of top-level control-flow bodies. Runtime nominal
    /// declarations are structured statements only while this is non-zero.
    top_level_control_flow_depth: Cell<usize>,
    /// Subset of `top_level_control_flow_depth` contributed by `for` bodies.
    /// Upstream Julia's `@enum` macro publishes nothing in this placement.
    top_level_for_depth: Cell<usize>,
    /// Whether lowering is currently inside a function body (any body-lowering
    /// entry that threads the live context sets this). Independent of
    /// [`Self::prefer_nested_lambdas`]: a body kept on the context only for
    /// lexical binder lookup (Issue #10948) still needs value-position arrows
    /// lowered as NESTED closures so capture analysis sees enclosing locals —
    /// the lifted top-level-lambda path has no captures (Issues #11030,
    /// #10965).
    in_function_body: Cell<bool>,
}

/// One active hygiene context for a module-defined macro being expanded.
struct MacroHygieneFrame {
    module: String,
    members: HashSet<String>,
    esc_depth: usize,
}

impl LambdaContext {
    pub fn new() -> Self {
        Self {
            lifted_functions: RefCell::new(Vec::new()),
            lambda_counter: RefCell::new(0),
            anonymous_name_namespace: None,
            definition_counter: Cell::new(0),
            usings: RefCell::new(HashSet::new()),
            macros: RefCell::new(HashMap::new()),
            compile_time_functions: RefCell::new(Vec::new()),
            compile_time_structs: RefCell::new(Vec::new()),
            macro_expanded_structs: RefCell::new(Vec::new()),
            macro_expanded_macros: RefCell::new(Vec::new()),
            compile_time_abstract_types: RefCell::new(Vec::new()),
            compile_time_primitive_types: RefCell::new(Vec::new()),
            module_macro_hygiene: RefCell::new(HashMap::new()),
            macro_hygiene_stack: RefCell::new(Vec::new()),
            current_file: RefCell::new(None),
            active_value_params: RefCell::new(Vec::new()),
            current_module_stack: RefCell::new(Vec::new()),
            active_new_struct_name: RefCell::new(None),
            prefer_nested_lambdas: Cell::new(false),
            top_level_control_flow_depth: Cell::new(0),
            top_level_for_depth: Cell::new(0),
            in_function_body: Cell::new(false),
        }
    }

    /// Create a new LambdaContext with a specific file path.
    /// Used when lowering files (not REPL).
    pub fn with_file(file_path: Option<String>) -> Self {
        Self {
            lifted_functions: RefCell::new(Vec::new()),
            lambda_counter: RefCell::new(0),
            anonymous_name_namespace: None,
            definition_counter: Cell::new(0),
            usings: RefCell::new(HashSet::new()),
            macros: RefCell::new(HashMap::new()),
            compile_time_functions: RefCell::new(Vec::new()),
            compile_time_structs: RefCell::new(Vec::new()),
            macro_expanded_structs: RefCell::new(Vec::new()),
            macro_expanded_macros: RefCell::new(Vec::new()),
            compile_time_abstract_types: RefCell::new(Vec::new()),
            compile_time_primitive_types: RefCell::new(Vec::new()),
            module_macro_hygiene: RefCell::new(HashMap::new()),
            macro_hygiene_stack: RefCell::new(Vec::new()),
            current_file: RefCell::new(file_path),
            active_value_params: RefCell::new(Vec::new()),
            current_module_stack: RefCell::new(Vec::new()),
            active_new_struct_name: RefCell::new(None),
            prefer_nested_lambdas: Cell::new(false),
            top_level_control_flow_depth: Cell::new(0),
            top_level_for_depth: Cell::new(0),
            in_function_body: Cell::new(false),
        }
    }

    /// Create an isolated lowering context whose anonymous helpers are unique
    /// within one REPL session while retaining ordinary file-less semantics.
    pub fn for_repl_fragment(namespace: u64) -> Self {
        let mut context = Self::with_file(None);
        context.anonymous_name_namespace = Some(namespace);
        context
    }

    pub fn prefer_nested_lambdas(&self) -> bool {
        self.prefer_nested_lambdas.get()
    }

    pub fn inside_top_level_control_flow(&self) -> bool {
        self.top_level_control_flow_depth.get() > 0
    }

    pub fn inside_top_level_for(&self) -> bool {
        self.top_level_for_depth.get() > 0
    }

    pub fn with_top_level_control_flow<T>(&self, f: impl FnOnce() -> T) -> T {
        let previous = self.top_level_control_flow_depth.get();
        self.top_level_control_flow_depth
            .set(previous.saturating_add(1));
        let result = f();
        self.top_level_control_flow_depth.set(previous);
        result
    }

    pub fn with_top_level_for<T>(&self, f: impl FnOnce() -> T) -> T {
        let previous_control = self.top_level_control_flow_depth.get();
        let previous_for = self.top_level_for_depth.get();
        self.top_level_control_flow_depth
            .set(previous_control.saturating_add(1));
        self.top_level_for_depth.set(previous_for.saturating_add(1));
        let result = f();
        self.top_level_for_depth.set(previous_for);
        self.top_level_control_flow_depth.set(previous_control);
        result
    }

    fn next_definition_order(&self) -> u64 {
        let next = self.definition_counter.get().saturating_add(1);
        self.definition_counter.set(next);
        next
    }

    fn stamp_struct_definition(&self, definition: &mut StructDef) {
        definition.span.definition_order = self.next_definition_order();
    }

    fn stamp_abstract_definition(&self, definition: &mut AbstractTypeDef) {
        definition.span.definition_order = self.next_definition_order();
    }

    fn stamp_primitive_definition(&self, definition: &mut PrimitiveTypeDef) {
        definition.span.definition_order = self.next_definition_order();
    }

    pub(crate) fn stamp_runtime_nominal_definition(
        &self,
        definition: &mut RuntimeNominalDef,
    ) -> u64 {
        let order = self.next_definition_order();
        match definition {
            RuntimeNominalDef::Struct(definition) => definition.span.definition_order = order,
            RuntimeNominalDef::AbstractType(definition) => {
                definition.span.definition_order = order;
            }
            RuntimeNominalDef::PrimitiveType(definition) => {
                definition.span.definition_order = order;
            }
            RuntimeNominalDef::Enum(definition) => definition.span.definition_order = order,
        }
        order
    }

    fn stamp_function_definitions(&self, definitions: &mut [Function]) {
        let order = self.next_definition_order();
        for definition in definitions {
            definition.span.definition_order = order;
        }
    }

    fn stamp_using_import(&self, using_import: &mut UsingImport) {
        using_import.span.definition_order = self.next_definition_order();
    }

    pub fn with_prefer_nested_lambdas<T>(&self, value: bool, f: impl FnOnce() -> T) -> T {
        let previous = self.prefer_nested_lambdas.replace(value);
        let result = f();
        self.prefer_nested_lambdas.set(previous);
        result
    }

    /// Whether lowering is currently inside a function body. See the field
    /// documentation: value-position arrows must lower as nested closures here
    /// even when `prefer_nested_lambdas` is off (Issues #11030, #10965).
    pub fn in_function_body(&self) -> bool {
        self.in_function_body.get()
    }

    /// Mark the dynamic extent of `f` as function-body lowering.
    pub fn with_function_body_scope<T>(&self, f: impl FnOnce() -> T) -> T {
        let previous = self.in_function_body.replace(true);
        let result = f();
        self.in_function_body.set(previous);
        result
    }

    /// Set the lexical constructor authority for functions lifted during `f`.
    /// Passing `None` creates the hard runtime-`@eval` boundary (#11197).
    pub fn with_new_struct_authority<T>(
        &self,
        struct_name: Option<&str>,
        f: impl FnOnce() -> T,
    ) -> T {
        let previous = self
            .active_new_struct_name
            .replace(struct_name.map(str::to_string));
        let result = f();
        self.active_new_struct_name.replace(previous);
        result
    }

    pub fn with_current_file<T>(&self, file_path: Option<String>, f: impl FnOnce() -> T) -> T {
        let previous = self.current_file.replace(file_path);
        let result = f();
        self.current_file.replace(previous);
        result
    }

    pub fn with_active_type_params<T>(
        &self,
        type_params: &[TypeParam],
        f: impl FnOnce() -> T,
    ) -> T {
        if type_params.is_empty() {
            return f();
        }
        let _scope = type_binder_env::Scope::new(type_params);
        f()
    }

    pub fn is_active_type_param(&self, name: &str) -> bool {
        type_binder_env::contains(name)
    }

    /// Whether expression lowering is nested inside any lexical `where`
    /// binder scope. Closure bodies must retain the live context while this is
    /// true even when the closure declares no type parameters of its own
    /// (Issue #11031).
    pub fn has_active_type_params(&self) -> bool {
        type_binder_env::is_active()
    }

    pub fn with_active_value_params<T>(
        &self,
        params: &[TypedParam],
        kwparams: &[KwParam],
        f: impl FnOnce() -> T,
    ) -> T {
        if params.is_empty() && kwparams.is_empty() {
            return f();
        }
        let names = params
            .iter()
            .flat_map(function::parameter_binding_names)
            .chain(kwparams.iter().map(|param| param.name.clone()))
            .collect();
        self.active_value_params.borrow_mut().push(names);
        let result = f();
        self.active_value_params.borrow_mut().pop();
        result
    }

    pub fn is_active_value_param(&self, name: &str) -> bool {
        self.active_value_params
            .borrow()
            .iter()
            .rev()
            .any(|frame| frame.contains(name))
    }

    /// Push the name of the module whose body is currently being lowered, so a
    /// macro expanded inside it receives that module as `__module__` instead of
    /// the hard-coded `Main` (Issue #7919). Must be paired with
    /// [`Self::pop_current_module`].
    pub fn push_current_module(&self, name: &str) {
        self.current_module_stack
            .borrow_mut()
            .push(name.to_string());
    }

    /// Pop the module pushed by a matching [`Self::push_current_module`].
    pub fn pop_current_module(&self) {
        self.current_module_stack.borrow_mut().pop();
    }

    /// Name of the innermost module body currently being lowered, or `None` at
    /// top level. Used to bind `__module__` at macro-expansion time (#7919).
    pub fn current_module(&self) -> Option<String> {
        self.current_module_stack.borrow().last().cloned()
    }

    /// Full lexical module path active at this lowering point. Included files
    /// execute in their caller's module and seed alias owners from this path.
    pub fn current_module_path(&self) -> Vec<String> {
        self.current_module_stack.borrow().clone()
    }

    /// Get the current file path (for @__FILE__ macro).
    /// Returns "none" for REPL/unknown sources.
    pub fn get_current_file(&self) -> String {
        self.current_file
            .borrow()
            .clone()
            .unwrap_or_else(|| "none".to_string())
    }

    /// Get the current directory (for @__DIR__ macro).
    /// Returns "." for REPL/unknown sources.
    pub fn get_current_dir(&self) -> String {
        match &*self.current_file.borrow() {
            Some(path) => {
                let path = std::path::Path::new(path);
                path.parent()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| ".".to_string())
            }
            None => ".".to_string(),
        }
    }

    /// Generate a unique name for a lifted lambda function.
    pub fn next_lambda_name(&self) -> String {
        let mut counter = self.lambda_counter.borrow_mut();
        let name = self.anonymous_name_namespace.map_or_else(
            || format!("__lambda_{}", *counter),
            |namespace| format!("__lambda_repl_{namespace}_{}", *counter),
        );
        *counter += 1;
        name
    }

    /// Generate a stable nested-arrow helper name. Source-span naming remains
    /// unchanged outside REPL fragment contexts for cache/test compatibility.
    pub fn nested_lambda_name(&self, source_start: usize) -> String {
        self.anonymous_name_namespace.map_or_else(
            || format!("__lambda_nested_{source_start}"),
            |namespace| format!("__lambda_nested_repl_{namespace}_{source_start}"),
        )
    }

    /// Generate a stable do-block helper name. REPL fragments need the same
    /// session-unique namespace as arrow helpers because byte spans restart at
    /// zero for every input.
    pub fn do_block_name(&self, source_start: usize) -> String {
        self.anonymous_name_namespace.map_or_else(
            || format!("__do_block_{source_start}"),
            |namespace| format!("__do_block_repl_{namespace}_{source_start}"),
        )
    }

    /// Generate a stable generator body helper name. REPL fragments namespace
    /// span-derived helpers because every input restarts byte offsets at zero.
    pub fn generator_body_name(&self, source_start: usize, level: Option<usize>) -> String {
        let suffix = level.map_or_else(String::new, |level| format!("_{level}"));
        self.anonymous_name_namespace.map_or_else(
            || format!("__gen_body_{source_start}{suffix}"),
            |namespace| format!("__gen_body_repl_{namespace}_{source_start}{suffix}"),
        )
    }

    /// Generate the predicate peer of [`Self::generator_body_name`].
    pub fn generator_predicate_name(&self, source_start: usize) -> String {
        self.anonymous_name_namespace.map_or_else(
            || format!("__gen_pred_{source_start}"),
            |namespace| format!("__gen_pred_repl_{namespace}_{source_start}"),
        )
    }

    /// Add a lifted function to the collection.
    pub fn add_lifted_function(&self, mut func: Function) {
        if func.new_struct_name.is_none() {
            func.new_struct_name = self.active_new_struct_name.borrow().clone();
        }
        func = func.into_lowering_helper();
        self.lifted_functions.borrow_mut().push(func);
    }

    /// Take all collected lifted functions.
    pub fn take_lifted_functions(&self) -> Vec<Function> {
        std::mem::take(&mut *self.lifted_functions.borrow_mut())
    }

    /// Number of lifted functions currently pending in this context.
    pub fn lifted_function_count(&self) -> usize {
        self.lifted_functions.borrow().len()
    }

    pub fn lifted_functions_from_index(&self, start: usize) -> Vec<Function> {
        self.lifted_functions
            .borrow()
            .iter()
            .skip(start)
            .cloned()
            .collect()
    }

    /// Take only lifted functions appended after `start`.
    ///
    /// Included files share the parent lowering context so macros and
    /// compile-time definitions remain visible across sequential `include`
    /// calls. This drains only the lambdas created by the included file,
    /// leaving any parent-file pending lambdas in place (Issue #7510).
    pub fn take_lifted_functions_from(&self, start: usize) -> Vec<Function> {
        let mut lifted = self.lifted_functions.borrow_mut();
        if start >= lifted.len() {
            return Vec::new();
        }
        lifted.split_off(start)
    }

    /// Record that a module has been imported via `using`.
    pub fn add_using(&self, module: &str) {
        self.usings.borrow_mut().insert(module.to_string());
    }

    fn snapshot_usings(&self) -> HashSet<String> {
        self.usings.borrow().clone()
    }

    fn restore_usings(&self, usings: HashSet<String>) {
        self.usings.replace(usings);
    }

    /// Check if a module has been imported via `using`.
    pub fn has_using(&self, module: &str) -> bool {
        self.usings.borrow().contains(module)
    }

    /// Register a user-defined macro (supports multiple arities).
    pub fn add_macro(&self, name: &str, macro_def: StoredMacroDef) {
        let mut macros = self.macros.borrow_mut();
        macros.entry(name.to_string()).or_default().push(macro_def);
    }

    pub fn add_macro_def(&self, macro_def: &MacroDef) {
        self.add_macro(
            &macro_def.name,
            StoredMacroDef {
                params: macro_def.params.clone(),
                param_types: vec![MacroParamType::Any; macro_def.params.len()],
                has_varargs: macro_def.has_varargs,
                body: macro_def.body.clone(),
                expansion_functions: vec![],
                expansion_structs: vec![],
                hygiene: None,
                span: macro_def.span,
            },
        );
    }

    /// Get a user-defined macro by name and arity.
    /// If arity is provided, returns the macro with matching arity.
    /// If no exact match, falls back to a varargs macro if available.
    pub fn get_macro_with_arity(&self, name: &str, arity: usize) -> Option<StoredMacroDef> {
        let macros = self.macros.borrow();
        if let Some(macro_defs) = macros.get(name) {
            // First, try to find an exact arity match
            for def in macro_defs {
                let expected_arity = if def.has_varargs {
                    def.params.len() - 1 // min args for varargs
                } else {
                    def.params.len()
                };
                let matches = if def.has_varargs {
                    arity >= expected_arity
                } else {
                    arity == expected_arity
                };
                if matches {
                    return Some(def.clone());
                }
            }
            // No exact match found
            None
        } else {
            None
        }
    }

    /// Get a user-defined macro by name, arity, and argument types.
    /// This enables Julia-style type-based macro dispatch.
    ///
    /// The dispatch algorithm:
    /// 1. First pass: find macros matching arity with exact type matches
    ///    (prioritize more specific types over Any)
    /// 2. Second pass: find macros matching arity with compatible types
    ///    (Any matches anything)
    pub fn get_macro_with_types(
        &self,
        name: &str,
        arg_types: &[MacroParamType],
    ) -> Option<StoredMacroDef> {
        let macros = self.macros.borrow();
        if let Some(macro_defs) = macros.get(name) {
            let arity = arg_types.len();

            // First pass: look for exact type matches (more specific takes priority)
            let mut best_match: Option<(StoredMacroDef, usize)> = None;

            for def in macro_defs {
                let expected_arity = if def.has_varargs {
                    def.params.len() - 1
                } else {
                    def.params.len()
                };

                let arity_matches = if def.has_varargs {
                    arity >= expected_arity
                } else {
                    arity == expected_arity
                };

                if !arity_matches {
                    continue;
                }

                // Check type compatibility and count specificity
                let (compatible, specificity) =
                    check_type_compatibility(&def.param_types, arg_types, def.has_varargs);

                if compatible {
                    match &best_match {
                        None => best_match = Some((def.clone(), specificity)),
                        Some((_, best_specificity)) if specificity > *best_specificity => {
                            best_match = Some((def.clone(), specificity));
                        }
                        _ => {}
                    }
                }
            }

            best_match.map(|(def, _)| def)
        } else {
            None
        }
    }

    /// Get a user-defined macro by name (returns first one, for backward compatibility).
    pub fn get_macro(&self, name: &str) -> Option<StoredMacroDef> {
        self.macros
            .borrow()
            .get(name)
            .and_then(|defs| defs.first().cloned())
    }

    /// Check if a macro with the given name is defined.
    pub fn has_macro(&self, name: &str) -> bool {
        self.macros.borrow().contains_key(name)
    }

    /// Record that `macro_name` is defined inside `module` whose member names are
    /// `members`. Used to qualify the macro's non-`esc` call targets at expansion
    /// time (Issue #7355 / #7350 A4).
    pub fn register_module_macro_hygiene(
        &self,
        macro_name: &str,
        module: &str,
        members: HashSet<String>,
        exports: HashSet<String>,
    ) {
        self.module_macro_hygiene.borrow_mut().insert(
            macro_name.to_string(),
            MacroHygieneInfo {
                module: module.to_string(),
                members,
                exports,
            },
        );
    }

    /// Begin converting `macro_name`'s expansion: if it is a module-defined macro,
    /// push its hygiene frame so member call targets get qualified. Returns whether
    /// a frame was pushed (caller must call [`Self::end_macro_hygiene`] iff true).
    pub fn begin_macro_hygiene(&self, macro_name: &str) -> bool {
        let frame = self
            .module_macro_hygiene
            .borrow()
            .get(macro_name)
            .map(|info| MacroHygieneFrame {
                module: info.module.clone(),
                members: info.members.clone(),
                esc_depth: 0,
            });
        match frame {
            Some(frame) => {
                self.macro_hygiene_stack.borrow_mut().push(frame);
                true
            }
            None => false,
        }
    }

    pub fn begin_macro_hygiene_frame(&self, module: &str, members: HashSet<String>) {
        self.macro_hygiene_stack
            .borrow_mut()
            .push(MacroHygieneFrame {
                module: module.to_string(),
                members,
                esc_depth: 0,
            });
    }

    pub fn macro_hygiene_entry(&self, macro_name: &str) -> Option<MacroHygieneInfo> {
        self.module_macro_hygiene.borrow().get(macro_name).cloned()
    }

    pub fn macro_hygiene_info_for_module(&self, module: &str) -> Option<MacroHygieneInfo> {
        self.module_macro_hygiene
            .borrow()
            .values()
            .find_map(|info| (info.module == module).then(|| info.clone()))
    }

    /// Pop the hygiene frame pushed by a matching [`Self::begin_macro_hygiene`].
    pub fn end_macro_hygiene(&self) {
        self.macro_hygiene_stack.borrow_mut().pop();
    }

    /// Enter an `esc(...)` subtree of the current macro expansion: identifiers
    /// inside resolve in the caller scope, so qualification is suppressed.
    pub fn enter_macro_esc(&self) {
        if let Some(frame) = self.macro_hygiene_stack.borrow_mut().last_mut() {
            frame.esc_depth += 1;
        }
    }

    /// Leave an `esc(...)` subtree.
    pub fn exit_macro_esc(&self) {
        if let Some(frame) = self.macro_hygiene_stack.borrow_mut().last_mut() {
            frame.esc_depth = frame.esc_depth.saturating_sub(1);
        }
    }

    /// If a module-defined macro is being expanded outside any `esc` and `name`
    /// is a member of its defining module, return that module name so the call
    /// target can be qualified `Module.name` (Issue #7355 / #7350 A4).
    pub fn qualify_module_macro_member(&self, name: &str) -> Option<String> {
        let stack = self.macro_hygiene_stack.borrow();
        let frame = stack.last()?;
        if frame.esc_depth == 0 && frame.members.contains(name) {
            // Hygiene resolves in the macro's defining module, independent of
            // whether the caller imported that module root. Use an absolute
            // module path so a selective macro import inside `module Client`
            // does not get rewritten as `Client.DefiningModule.member`
            // (Issue #11240).
            Some(
                if crate::module_names::is_language_root_path(&frame.module) {
                    frame.module.clone()
                } else {
                    format!("Main.{}", frame.module)
                },
            )
        } else {
            None
        }
    }

    /// Register functions that may be called while evaluating macro bodies.
    pub fn add_compile_time_functions(&self, funcs: &[Function]) {
        self.compile_time_functions
            .borrow_mut()
            .extend(funcs.iter().cloned());
    }

    /// Snapshot functions visible to expansion-time macro execution.
    pub fn compile_time_functions(&self) -> Vec<Function> {
        self.compile_time_functions.borrow().clone()
    }

    pub fn add_compile_time_structs(&self, structs: &[StructDef]) {
        self.compile_time_structs
            .borrow_mut()
            .extend(structs.iter().cloned());
    }

    pub fn add_macro_expanded_struct(&self, struct_def: StructDef) {
        self.add_compile_time_structs(std::slice::from_ref(&struct_def));
        self.macro_expanded_structs.borrow_mut().push(struct_def);
    }

    pub fn take_macro_expanded_structs(&self) -> Vec<StructDef> {
        std::mem::take(&mut *self.macro_expanded_structs.borrow_mut())
    }

    /// Number of macro-expanded structs currently pending in this context.
    /// Paired with [`take_macro_expanded_structs_from`] as a watermark, the
    /// same pattern `lifted_function_count`/`lifted_functions_from_index` use
    /// for lifted lambdas (Issue #10402).
    pub fn macro_expanded_struct_count(&self) -> usize {
        self.macro_expanded_structs.borrow().len()
    }

    /// Take only macro-expanded structs queued after `start`, leaving earlier
    /// (sibling) entries in place. See
    /// [`reject_macro_expanded_structs_added_since`] for why this
    /// non-destructive variant is needed inside `lower_stmt_impl`.
    pub fn take_macro_expanded_structs_from(&self, start: usize) -> Vec<StructDef> {
        let mut structs = self.macro_expanded_structs.borrow_mut();
        if start >= structs.len() {
            return Vec::new();
        }
        structs.split_off(start)
    }

    pub fn add_macro_expanded_macro(&self, macro_def: MacroDef) {
        self.macro_expanded_macros.borrow_mut().push(macro_def);
    }

    pub fn take_macro_expanded_macros(&self) -> Vec<MacroDef> {
        std::mem::take(&mut *self.macro_expanded_macros.borrow_mut())
    }

    pub fn add_compile_time_abstract_types(&self, types: &[AbstractTypeDef]) {
        self.compile_time_abstract_types
            .borrow_mut()
            .extend(types.iter().cloned());
    }

    pub fn add_compile_time_primitive_types(&self, types: &[PrimitiveTypeDef]) {
        self.compile_time_primitive_types
            .borrow_mut()
            .extend(types.iter().cloned());
    }

    /// Snapshot user type definitions visible to expansion-time macro execution.
    /// A bundled-package macro is expanded by compiling all `compile_time_functions`,
    /// so any user struct those functions touch must be present too (Issue #7272).
    pub fn compile_time_structs(&self) -> Vec<StructDef> {
        self.compile_time_structs.borrow().clone()
    }

    pub fn compile_time_abstract_types(&self) -> Vec<AbstractTypeDef> {
        self.compile_time_abstract_types.borrow().clone()
    }

    pub fn compile_time_primitive_types(&self) -> Vec<PrimitiveTypeDef> {
        self.compile_time_primitive_types.borrow().clone()
    }

    /// Get all imported module names.
    pub fn get_usings(&self) -> Vec<String> {
        self.usings.borrow().iter().cloned().collect()
    }
}

impl Default for LambdaContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Context for tracking include files during lowering.
/// Used to detect circular includes and resolve relative paths.
pub struct IncludeContext {
    /// Base directory for resolving relative paths.
    /// None means current working directory.
    base_dir: Option<PathBuf>,
    /// Set of already-included files (canonicalized paths) for circular detection.
    included_files: Rc<RefCell<HashSet<PathBuf>>>,
    /// Stack of include paths for error messages.
    include_stack: Rc<RefCell<Vec<PathBuf>>>,
}

impl IncludeContext {
    /// Create a new include context with an optional base directory.
    pub fn new(base_dir: Option<PathBuf>) -> Self {
        Self {
            base_dir,
            included_files: Rc::new(RefCell::new(HashSet::new())),
            include_stack: Rc::new(RefCell::new(Vec::new())),
        }
    }

    /// Create a child context for processing an included file.
    /// Shares the included_files and include_stack with the parent.
    pub fn child(&self, new_base_dir: Option<PathBuf>) -> Self {
        Self {
            base_dir: new_base_dir,
            included_files: Rc::clone(&self.included_files),
            include_stack: Rc::clone(&self.include_stack),
        }
    }

    /// Get the base directory for path resolution.
    pub fn base_dir(&self) -> Option<&PathBuf> {
        self.base_dir.as_ref()
    }

    /// Include a file: read, parse, and lower it.
    /// Returns the lowered Program if successful.
    pub fn include_file(&self, path: &str, span: Span) -> IncludeResult<IncludedContent> {
        self.include_file_with_macro_context(path, span, None)
    }

    /// Include a file using the caller's macro/lambda lowering context.
    ///
    /// Julia evaluates sequential includes in the same module scope. Sharing the
    /// lowering context makes macros and compile-time helper definitions from an
    /// earlier include visible to later includes in that same scope (Issue #7510).
    pub fn include_file_with_macro_context(
        &self,
        path: &str,
        span: Span,
        macro_ctx: Option<&LambdaContext>,
    ) -> IncludeResult<IncludedContent> {
        // 1. Resolve the path
        let resolved = resolve_include_path(path, self.base_dir.as_deref());

        // 2. Canonicalize for consistent circular detection
        let canonical = resolved.canonicalize().unwrap_or_else(|_| resolved.clone());

        // 3. Check for circular include
        if self.included_files.borrow().contains(&canonical) {
            let chain: Vec<String> = self
                .include_stack
                .borrow()
                .iter()
                .map(|p| p.display().to_string())
                .collect();
            return Err(IncludeError::CircularInclude {
                path: canonical,
                include_chain: chain,
            });
        }

        // 4. Mark as included and push to stack
        self.included_files.borrow_mut().insert(canonical.clone());
        self.include_stack.borrow_mut().push(canonical.clone());

        // 5. Read the file
        let content = read_include_file(&resolved)?;

        // 6. Parse the content
        let mut parser = Parser::new().map_err(|e| IncludeError::ParseError {
            file_path: path.to_string(),
            message: e.to_string(),
        })?;
        let parse_outcome = parser
            .parse(&content)
            .map_err(|e| IncludeError::ParseError {
                file_path: path.to_string(),
                message: e.to_string(),
            })?;
        let exports =
            collect_source_file_exports(&parse_outcome).map_err(|e| IncludeError::LowerError {
                file_path: path.to_string(),
                message: e.to_string(),
            })?;

        // 7. Create child context with new base directory
        let child_base = resolved.parent().map(|p| p.to_path_buf());
        let child_ctx = self.child(child_base);

        // 8. Lower the parsed content
        let current_file = Some(resolved.to_string_lossy().to_string());
        let mut lowering =
            LoweringWithInclude::new_with_file(&content, child_ctx, Some(resolved.clone()));
        let program = match macro_ctx {
            Some(ctx) => ctx.with_current_file(current_file, || {
                lowering.lower_with_lambda_context(parse_outcome, ctx)
            }),
            None => {
                let lambda_ctx = LambdaContext::with_file(current_file);
                lowering.lower_with_lambda_context(parse_outcome, &lambda_ctx)
            }
        }
        .map_err(|e| IncludeError::LowerError {
            file_path: path.to_string(),
            message: e.to_string(),
        })?;

        // 9. Pop from stack (file is still in included_files to prevent re-include)
        self.include_stack.borrow_mut().pop();

        Ok(IncludedContent {
            program,
            file_path: resolved,
            span,
            exports,
        })
    }
}

impl Default for IncludeContext {
    fn default() -> Self {
        Self::new(None)
    }
}

/// Content from an included file, ready to be merged into the parent program.
pub struct IncludedContent {
    /// The lowered program from the included file.
    pub program: Program,
    pub exports: Vec<String>,
    /// The resolved file path.
    pub file_path: PathBuf,
    /// The span of the include call in the parent file.
    pub span: Span,
}

impl IncludedContent {
    /// Merge the included content into a parent program.
    /// Functions, structs, abstract types, type aliases, modules, usings, and
    /// macros are added to the parent. Main statements are returned to be
    /// inlined at the include site.
    ///
    /// Macros must be threaded through too (Issue #6355): a bundled package like
    /// Plots defines `@animate`/`@gif` in an `include`d file (`api.jl`), and dropping
    /// them here made the macros invisible to `using Plots`.
    pub fn merge_into(
        self,
        functions: &mut Vec<Function>,
        structs: &mut Vec<StructDef>,
        abstract_types: &mut Vec<AbstractTypeDef>,
        primitive_types: &mut Vec<PrimitiveTypeDef>,
        type_aliases: &mut Vec<TypeAliasDef>,
        modules: &mut Vec<Module>,
        usings: &mut Vec<UsingImport>,
        macros: &mut Vec<MacroDef>,
        exports: Option<&mut Vec<String>>,
    ) -> Block {
        // Unwrap the Arc back to an owned `Function` (Issue #9140): these Arcs
        // were just created by this included sub-program's own lowering, so
        // the refcount is 1 and `try_unwrap` never falls through to a clone.
        functions.extend(
            self.program
                .functions
                .into_iter()
                .map(|f| Arc::try_unwrap(f).unwrap_or_else(|arc| (*arc).clone())),
        );
        structs.extend(self.program.structs);
        abstract_types.extend(self.program.abstract_types);
        primitive_types.extend(self.program.primitive_types);
        type_aliases.extend(self.program.type_aliases);
        modules.extend(self.program.modules);
        usings.extend(self.program.usings);
        macros.extend(self.program.macros);
        if let Some(parent_exports) = exports {
            parent_exports.extend(self.exports);
        }
        self.program.main
    }
}

fn collect_source_file_exports(parse_outcome: &ParseOutcome) -> LowerResult<Vec<String>> {
    let ParseOutcome::Rust(parsed) = parse_outcome;
    let walker = CstWalker::new(parsed.source());
    let root = Node::new(parsed.root(), parsed.source());
    let mut exports = Vec::new();
    for child in walker.named_children(&root) {
        if walker.kind(&child) == NodeKind::ExportStatement {
            exports.extend(lower_export_statement(&walker, child)?);
        }
    }
    Ok(exports)
}

/// Shared source-file top-level `NodeKind` walk (Issue #10628, follow-up of
/// #10271). Both [`Lowering::lower_source_file_inner`] (plain — Base/prelude,
/// REPL one-shot eval, and every other no-`include()` caller) and
/// [`LoweringWithInclude::lower_source_file_inner`] (file/CLI lowering, one
/// `include()`d file at a time) drove hand-synced copies of this loop before
/// this unification; a lowering feature added to only one of them silently
/// diverged Base/prelude behavior from user-program behavior (#10164's 286
/// missing Base docstrings was exactly this class of bug). Now there is one
/// loop, parameterized over whether `include()` is available:
///
/// - `include_ctx: None` — the historical plain-`Lowering` behavior: a
///   `CallExpression` (including a literal call to `include`) is never
///   special-cased and falls through to the generic statement-lowering arm.
/// - `include_ctx: Some(ctx)` — the historical `LoweringWithInclude`
///   behavior: a `CallExpression` is checked against `try_process_include_call`
///   first, and only falls through to the generic arm when it is not an
///   `include(...)` call.
///
/// `lambda_ctx` is always a live context (never optional) — both callers
/// already have one by the time they reach this loop; only include support
/// varies between them.
fn lower_source_file_body<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
    include_ctx: Option<&IncludeContext>,
) -> LowerResult<Program> {
    let mut abstract_types = Vec::new();
    let mut primitive_types = Vec::new();
    let mut type_aliases = Vec::new();
    let mut structs = Vec::new();
    let mut functions = Vec::new();
    let mut modules = Vec::new();
    let mut usings = Vec::new();
    let mut macros = Vec::new();
    let mut main_stmts = Vec::new();

    // Includes share the caller's context so definitions from one included
    // file are visible while lowering later includes in the same scope. For
    // the no-include (`Lowering`) caller, `lambda_ctx` is always fresh, so
    // this is always 0 there — `take_lifted_functions_from(0)` below is then
    // equivalent to draining the whole (freshly created) list.
    let lifted_start = lambda_ctx.lifted_function_count();

    let children = walker.named_children_vec(&node);
    let mut pending_doc: Option<(Expr, Span)> = None;
    for (idx, child_ref) in children.iter().enumerate() {
        let child = *child_ref;
        let kind = walker.kind(&child);

        // Issue #4357/#10164: skip top-level string literals that precede a
        // definition — these are docstrings in Julia (`"""doc""" function f
        // end` ≡ `@doc "doc" f`) and should not become value-producing main
        // statements. Capture it into `pending_doc` so the following
        // definition still registers a `__sjulia_doc_<Name>` binding.
        if matches!(kind, NodeKind::StringLiteral) && is_top_level_docstring(walker, &children, idx)
        {
            let doc = expr::lower_expr_with_ctx(walker, child, lambda_ctx)?;
            pending_doc = Some((doc, walker.span(&child)));
            continue;
        }

        if let Some((module_node, is_bare)) = explicit_doc_module_target(walker, child) {
            let module = lower_module_definition(
                walker,
                module_node,
                is_bare,
                include_ctx,
                Some(lambda_ctx),
            )?;
            // An explicit `@doc "..." module M ... end` consumes any
            // preceding `pending_doc` here rather than letting it leak
            // forward to whatever later definition next calls
            // `push_doc_registration` — matching the safety net below, which
            // this `continue` would otherwise skip. This is Issue #10628's
            // one unification-visible behavior delta (`LoweringWithInclude`
            // already reset here; plain `Lowering` did not) — confirmed
            // inert in practice: any top-level definition placed after an
            // `explicit_doc_module_target` construct already fails
            // (independent of docstrings) due to the unrelated, pre-existing
            // Issue #10911, so this reset has no observable effect on any
            // program that currently runs.
            pending_doc = None;
            modules.push(module);
            continue;
        }

        match kind {
            // Skip comments
            NodeKind::LineComment | NodeKind::BlockComment => continue,
            NodeKind::AbstractDefinition => {
                let mut abstract_def = abstract_::lower_abstract_definition(walker, child)?;
                lambda_ctx.stamp_abstract_definition(&mut abstract_def);
                push_doc_registration(
                    &mut main_stmts,
                    &mut pending_doc,
                    [abstract_def.name.clone()],
                );
                lambda_ctx.add_compile_time_abstract_types(std::slice::from_ref(&abstract_def));
                abstract_types.push(abstract_def);
            }
            NodeKind::PrimitiveDefinition => {
                let mut primitive_def = primitive::lower_primitive_definition(walker, child)?;
                lambda_ctx.stamp_primitive_definition(&mut primitive_def);
                push_doc_registration(
                    &mut main_stmts,
                    &mut pending_doc,
                    [primitive_def.name.clone()],
                );
                lambda_ctx.add_compile_time_primitive_types(std::slice::from_ref(&primitive_def));
                primitive_types.push(primitive_def);
            }
            NodeKind::StructDefinition | NodeKind::MutableStructDefinition => {
                let mut struct_def =
                    struct_::lower_struct_definition_with_ctx(walker, child, lambda_ctx)?;
                lambda_ctx.stamp_struct_definition(&mut struct_def);
                push_doc_registration(&mut main_stmts, &mut pending_doc, [struct_def.name.clone()]);
                lambda_ctx.add_compile_time_structs(std::slice::from_ref(&struct_def));
                // `global` helpers declared in the struct body are ordinary
                // global methods (with privileged `new`), so they join the
                // program's function list (Issue #11005).
                extend_struct_global_helpers(Some(lambda_ctx), &mut functions, &mut struct_def);
                structs.push(struct_def);
            }
            NodeKind::FunctionDefinition => {
                let mut funcs = lower_function_all_with_ctx_if_needed(walker, child, lambda_ctx)?;
                lambda_ctx.stamp_function_definitions(&mut funcs);
                reject_macro_expanded_structs_in_non_toplevel(lambda_ctx, walker.span(&child))?;
                push_doc_registration(
                    &mut main_stmts,
                    &mut pending_doc,
                    funcs.iter().map(|f| f.name.clone()),
                );
                lambda_ctx.add_compile_time_functions(&funcs);
                functions.extend(funcs);
            }
            NodeKind::ShortFunctionDefinition => {
                // Operator method definitions: *(x, y) = expr
                let mut func = lower_operator_method_with_ctx_if_needed(walker, child, lambda_ctx)?;
                lambda_ctx.stamp_function_definitions(std::slice::from_mut(&mut func));
                reject_macro_expanded_structs_in_non_toplevel(lambda_ctx, walker.span(&child))?;
                push_doc_registration(&mut main_stmts, &mut pending_doc, [func.name.clone()]);
                lambda_ctx.add_compile_time_functions(std::slice::from_ref(&func));
                functions.push(func);
            }
            NodeKind::MacroDefinition => {
                let lifted_start = lambda_ctx.lifted_function_count();
                let (macro_def, param_types) =
                    lower_macro_definition(walker, child, Some(lambda_ctx))?;
                push_doc_registration(&mut main_stmts, &mut pending_doc, [macro_def.name.clone()]);
                let macro_lambdas = lambda_ctx.lifted_functions_from_index(lifted_start);
                lambda_ctx.add_compile_time_functions(&macro_lambdas);
                // Register macro in context for expansion during lowering
                lambda_ctx.add_macro(
                    &macro_def.name,
                    StoredMacroDef {
                        params: macro_def.params.clone(),
                        param_types,
                        has_varargs: macro_def.has_varargs,
                        body: macro_def.body.clone(),
                        expansion_functions: vec![],
                        expansion_structs: vec![],
                        hygiene: None,
                        span: macro_def.span,
                    },
                );
                macros.push(macro_def);
            }
            NodeKind::ModuleDefinition => {
                let module =
                    lower_module_definition(walker, child, false, include_ctx, Some(lambda_ctx))?;
                pending_doc = None;
                modules.push(module);
            }
            NodeKind::BaremoduleDefinition => {
                let module =
                    lower_module_definition(walker, child, true, include_ctx, Some(lambda_ctx))?;
                pending_doc = None;
                modules.push(module);
            }
            NodeKind::UsingStatement | NodeKind::ImportStatement => {
                // using Module or import Module (possibly comma-separated, e.g.
                // `using A, B` → one UsingImport per module).
                for mut using_import in lower_using_statement(walker, child)? {
                    lambda_ctx.stamp_using_import(&mut using_import);
                    // Record in lambda context for macro availability checks
                    lambda_ctx.add_using(&using_import.module);
                    // Load stdlib module macros early so they can be expanded
                    ensure_stdlib_macros_loaded(&using_import.module);
                    // Same for embedded bundled packages (e.g. Plots' @animate/@gif).
                    ensure_bundled_package_macros_loaded(&using_import.module);
                    // Keep an executable marker at the exact source position.
                    // Most import lookup remains compile-time metadata, but
                    // same-named exported submodules become ambiguous only when
                    // the conflicting `using` executes (Issues #11203/#11216).
                    main_stmts.push(Stmt::Using {
                        module: using_import.module.clone(),
                        span: using_import.span,
                    });
                    usings.push(using_import);
                }
            }
            NodeKind::Assignment if function::is_short_function_definition(walker, child) => {
                // Short function definition: f(x) = expr
                let mut funcs =
                    lower_short_function_all_with_ctx_if_needed(walker, child, lambda_ctx)?;
                lambda_ctx.stamp_function_definitions(&mut funcs);
                reject_macro_expanded_structs_in_non_toplevel(lambda_ctx, walker.span(&child))?;
                push_doc_registration(
                    &mut main_stmts,
                    &mut pending_doc,
                    funcs.iter().map(|f| f.name.clone()),
                );
                lambda_ctx.add_compile_time_functions(&funcs);
                functions.extend(funcs);
            }
            NodeKind::Assignment if function::is_lambda_assignment(walker, child) => {
                // Lambda assignment: f = x -> expr
                // May return multiple methods: the main lambda plus reduced-arity
                // default-arg stubs for `(x, d=2) -> ...` (Issue #8047).
                let mut funcs =
                    function::lower_lambda_assignment_with_ctx(walker, child, lambda_ctx)?;
                lambda_ctx.stamp_function_definitions(&mut funcs);
                push_doc_registration(
                    &mut main_stmts,
                    &mut pending_doc,
                    funcs.iter().map(|f| f.name.clone()),
                );
                lambda_ctx.add_compile_time_functions(&funcs);
                functions.extend(funcs);
            }
            NodeKind::Assignment
                if stmt::try_extract_type_alias_from_assignment(walker, child).is_some() =>
            {
                // Issue #5055: a plain (non-`const`) type-alias definition
                // such as `MyVec{T} = Vector{T}` or `IntVec = Vector{Int}`.
                // Keep the compile-time alias registration used by type
                // annotations, but also emit Julia's runtime type-object
                // binding. Omitting it made `z = Alias{T}` freeze `z` as a
                // second string alias instead of binding the resolved type
                // object (Issue #10501, blocking #10372).
                if let Some(type_alias) =
                    stmt::try_extract_type_alias_from_assignment(walker, child)
                {
                    type_aliases.push(type_alias);
                }
                main_stmts.push(stmt::lower_stmt_with_ctx(walker, child, lambda_ctx)?);
            }
            NodeKind::MacroCall if is_kwdef_macro(walker, child) => {
                // @kwdef struct ... end - expand to struct def + constructor
                let (mut struct_def, mut ctor_func) =
                    expand_kwdef_macro(walker, child, lambda_ctx)?;
                lambda_ctx.stamp_struct_definition(&mut struct_def);
                lambda_ctx.stamp_function_definitions(std::slice::from_mut(&mut ctor_func));
                lambda_ctx.add_compile_time_structs(std::slice::from_ref(&struct_def));
                extend_struct_global_helpers(Some(lambda_ctx), &mut functions, &mut struct_def);
                structs.push(struct_def);
                functions.push(ctor_func);
            }
            NodeKind::ConstStatement => {
                // Check if this is a type alias definition
                if let Some(type_alias) = stmt::try_extract_type_alias(walker, child) {
                    type_aliases.push(type_alias);
                }
                // Always lower const statements so the variable is accessible at runtime
                let stmt = stmt::lower_stmt_with_ctx(walker, child, lambda_ctx)?;
                drain_macro_expanded_structs(lambda_ctx, &mut structs, &mut functions);
                drain_macro_expanded_macros(lambda_ctx, &mut macros);
                push_doc_registration(
                    &mut main_stmts,
                    &mut pending_doc,
                    const_statement_doc_names(&stmt),
                );
                main_stmts.push(stmt);
            }
            NodeKind::CallExpression => {
                // Check if this is an include() call. Only possible when an
                // `IncludeContext` is available — the plain `Lowering` path
                // (Base/prelude, REPL one-shot eval, and every other
                // no-`include()` caller) passes `include_ctx: None` and always
                // falls through to the generic statement arm below, matching
                // its historical behavior of never special-casing `include`.
                let included = match include_ctx {
                    Some(include_ctx) => {
                        try_process_include_call(walker, include_ctx, child, lambda_ctx)?
                    }
                    None => None,
                };
                if let Some(included) = included {
                    lambda_ctx.add_compile_time_abstract_types(&included.program.abstract_types);
                    lambda_ctx.add_compile_time_primitive_types(&included.program.primitive_types);
                    lambda_ctx.add_compile_time_structs(&included.program.structs);
                    let included_funcs: Vec<Function> = included
                        .program
                        .functions
                        .iter()
                        .map(|f| (**f).clone())
                        .collect();
                    lambda_ctx.add_compile_time_functions(&included_funcs);
                    for macro_def in &included.program.macros {
                        lambda_ctx.add_macro_def(macro_def);
                    }
                    // Merge included content
                    let inline_block = included.merge_into(
                        &mut functions,
                        &mut structs,
                        &mut abstract_types,
                        &mut primitive_types,
                        &mut type_aliases,
                        &mut modules,
                        &mut usings,
                        &mut macros,
                        None,
                    );
                    drain_macro_expanded_structs(lambda_ctx, &mut structs, &mut functions);
                    drain_macro_expanded_macros(lambda_ctx, &mut macros);
                    // Add the inline statements from the included file
                    main_stmts.extend(inline_block.stmts);
                } else {
                    // Not an include call (or no include support at all):
                    // process as a normal statement.
                    let stmt = stmt::lower_stmt_with_ctx(walker, child, lambda_ctx)?;
                    drain_macro_expanded_structs(lambda_ctx, &mut structs, &mut functions);
                    drain_macro_expanded_macros(lambda_ctx, &mut macros);
                    main_stmts.push(stmt);
                }
            }
            _ => {
                // Use context-aware lowering to handle inline lambdas
                let stmt = stmt::lower_stmt_with_ctx(walker, child, lambda_ctx)?;
                // Drain any struct/macro definitions discovered while lowering
                // this statement (e.g. a stdlib-macro-expanded `struct` nested
                // in a `@testset`/`let` block, or a user-defined macro that
                // returns `Expr(:struct, ...)`) into Program metadata — a
                // `CallExpression` (e.g. `include(...)` when not
                // include-aware, or any other bare call) can never itself be
                // a function definition, so `extract_top_level_function_defs`
                // below is a no-op passthrough for it and this arm covers both
                // the historical plain-`Lowering` catch-all and
                // `LoweringWithInclude`'s catch-all identically.
                drain_macro_expanded_structs(lambda_ctx, &mut structs, &mut functions);
                drain_macro_expanded_macros(lambda_ctx, &mut macros);
                match extract_top_level_function_defs(stmt) {
                    Ok(funcs) => {
                        extend_source_function_definitions(Some(lambda_ctx), &mut functions, funcs)
                    }
                    Err(stmt) => main_stmts.push(*stmt),
                }
            }
        }

        // Safety net (Issue #10164): `is_docstring_target_kind` lists kinds
        // whose preceding docstring is captured into `pending_doc` above, but
        // not every arm for those kinds calls [`push_doc_registration`] (e.g.
        // a non-`const` type-alias `Assignment`, the plain-`Assignment` /
        // bare `MacroCall` catch-all, and `@kwdef`). Without this, a
        // docstring preceding one of those would silently leak forward and
        // get misattributed to whatever later definition next calls
        // `push_doc_registration` (exactly the cross-file `VERSION` →
        // `_findlast_char` misattribution `ConstStatement` had before its own
        // fix above). Arms that already consumed `pending_doc` leave it
        // `None`, so this is a no-op for them; it only guards the remaining
        // arms that never attach anything.
        if is_docstring_target_kind(kind) {
            pending_doc = None;
        }
    }

    // Collect lifted lambda functions
    let lifted_functions = lambda_ctx.take_lifted_functions_from(lifted_start);
    functions.extend(lifted_functions);

    // Box scalar locals that are captured by a closure and reassigned, so the
    // closure observes the new value (Julia cell semantics, Issue #6262).
    closure_box::box_captured_reassigned_locals(&mut functions, &mut main_stmts);

    let span = walker.span(&node);
    Ok(Program {
        abstract_types,
        primitive_types,
        type_aliases,
        structs,
        // Program.functions is Arc-wrapped (Issue #9140); lowering still
        // builds up a plain `Vec<Function>` throughout (cheap — one user
        // program, not the ~5000-function prelude), so wrap once here.
        functions: functions.into_iter().map(Arc::new).collect(),
        base_function_count: 0,
        modules,
        usings,
        macros,
        enums: vec![],
        main: Block {
            stmts: main_stmts,
            span,
        },
    })
}

/// Try to process a CallExpression as an include() call.
/// Returns Some(IncludedContent) if it was an include call, None otherwise.
fn try_process_include_call<'a>(
    walker: &CstWalker<'a>,
    include_ctx: &IncludeContext,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Option<IncludedContent>> {
    // Check if this is a call to "include"
    let call_node = node;
    let children = walker.named_children_vec(&call_node);

    // Get the function name
    let callee = children.first().ok_or_else(|| {
        UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedCallTarget,
            walker.span(&call_node),
        )
    })?;

    let func_name = match walker.kind(callee) {
        NodeKind::Identifier => walker.text(callee),
        _ => return Ok(None), // Not a simple identifier call
    };

    if func_name != "include" {
        return Ok(None);
    }

    let span = walker.span(&call_node);

    // Find the argument list
    let args_node = children
        .iter()
        .find(|n| walker.kind(n) == NodeKind::ArgumentList);

    // Extract the path argument
    let path = if let Some(args) = args_node {
        let arg_children = walker.named_children_vec(args);
        if let Some(first_arg) = arg_children.first() {
            if walker.kind(first_arg) == NodeKind::StringLiteral {
                let text = walker.text(first_arg);
                text.trim_matches('"').to_string()
            } else {
                // Dynamic path not supported
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::IncludeCall("<dynamic path>".to_string()),
                    span,
                )
                .with_hint("include() requires a string literal path"));
            }
        } else {
            return Err(UnsupportedFeature::new(
                UnsupportedFeatureKind::IncludeCall("<missing argument>".to_string()),
                span,
            )
            .with_hint("include() requires a path argument"));
        }
    } else {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::IncludeCall("<no arguments>".to_string()),
            span,
        )
        .with_hint("include() requires a path argument"));
    };

    // Process the include
    let included = include_ctx
        .include_file_with_macro_context(&path, span, Some(lambda_ctx))
        .map_err(|e| UnsupportedFeature::new(UnsupportedFeatureKind::Other(e.to_string()), span))?;

    Ok(Some(included))
}

pub struct Lowering<'a> {
    _source: &'a str,
    walker: CstWalker<'a>,
    /// Store the parsed source so it lives long enough for Node references
    parsed_rust: Option<crate::parser::RustParsedSource>,
    initial_usings: Vec<String>,
    /// User macros defined by earlier REPL evaluations, pre-seeded into the
    /// lowering's macro table before the walk so a macro defined in one
    /// top-level expression is usable by a later one on the same session
    /// (Issue #9172). Empty for one-shot compilation.
    initial_macros: Vec<MacroDef>,
}

impl<'a> Lowering<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            _source: source,
            walker: CstWalker::new(source),
            parsed_rust: None,
            initial_usings: Vec::new(),
            initial_macros: Vec::new(),
        }
    }

    pub fn new_with_usings(source: &'a str, usings: &[UsingImport]) -> Self {
        let mut lowering = Self::new(source);
        lowering.initial_usings = usings.iter().map(|using| using.module.clone()).collect();
        lowering
    }

    /// Like [`new_with_usings`], but also pre-seeds user macros defined by
    /// earlier evaluations so they are visible during macro expansion of this
    /// input (Issue #9172). Used by the REPL session to carry macro definitions
    /// across evaluations.
    pub fn new_with_usings_and_macros(
        source: &'a str,
        usings: &[UsingImport],
        macros: &[MacroDef],
    ) -> Self {
        let mut lowering = Self::new_with_usings(source, usings);
        lowering.initial_macros = macros.to_vec();
        lowering
    }

    pub fn lower(&mut self, parse_outcome: ParseOutcome) -> LowerResult<Program> {
        let lambda_ctx = LambdaContext::new();
        self.lower_with_lambda_context(parse_outcome, &lambda_ctx)
    }

    /// Lower one source with a caller-owned lambda context. REPL sessions use
    /// this seam to namespace compiler-generated helpers per input while the
    /// ordinary file path retains [`Self::lower`]'s fresh context.
    pub fn lower_with_lambda_context(
        &mut self,
        parse_outcome: ParseOutcome,
        lambda_ctx: &LambdaContext,
    ) -> LowerResult<Program> {
        let ParseOutcome::Rust(parsed) = parse_outcome;
        self.parsed_rust = Some(parsed);
        // `parsed_rust` was just assigned `Some(..)` on the line above and
        // `&mut self` excludes concurrent mutation, so the read below cannot
        // observe `None` today; guarded rather than a raw unwrap so a future
        // refactor that breaks this invariant surfaces a typed error instead
        // of an uncaught host crash (Issue #10905, Phase 1b of #10869).
        let parsed_ref = self.parsed_rust.as_ref().ok_or_else(|| {
            internal_lowering_error(
                Span::new(0, 0, 0, 0, 0, 0),
                "parsed_rust was just assigned Some on the previous line",
            )
        })?;
        let root = Node::new(parsed_ref.root(), parsed_ref.source());
        self.lower_source_file(root, lambda_ctx)
    }

    /// Lower a source file (or module body) to a `Program`. Wraps the inner
    /// lowering with a type-alias scope (Issue #5055): user-defined aliases are
    /// pre-registered for this pass and the prior alias table is restored on
    /// exit, so a nested pass (e.g. the stdlib load triggered by `using Test`)
    /// cannot destroy the enclosing program's aliases.
    fn lower_source_file(
        &self,
        node: Node<'a>,
        lambda_ctx: &LambdaContext,
    ) -> LowerResult<Program> {
        let scope = type_alias::snapshot();
        let source_scope = type_alias::SourceScope::new();
        prescan_and_register_type_aliases(&self.walker, node, &mut Vec::new(), &source_scope);
        let result = self.lower_source_file_inner(node, lambda_ctx);
        scope.restore();
        result
    }

    fn lower_source_file_inner(
        &self,
        node: Node<'a>,
        lambda_ctx: &LambdaContext,
    ) -> LowerResult<Program> {
        for module in &self.initial_usings {
            lambda_ctx.add_using(module);
            ensure_stdlib_macros_loaded(module);
            ensure_bundled_package_macros_loaded(module);
        }
        // Pre-seed user macros defined by earlier REPL evaluations so a macro
        // defined in one top-level expression is usable by a later one on the
        // same session (Issue #9172). A macro redefined in the current input is
        // added again during the walk; arity lookup returns the first match, so
        // seeded (prior) defs of the same name+arity take effect only when the
        // current input does not redefine them.
        for macro_def in &self.initial_macros {
            lambda_ctx.add_macro_def(macro_def);
        }

        // Issue #10628: the actual `NodeKind` walk is shared with
        // `LoweringWithInclude::lower_source_file_inner` — `include_ctx: None`
        // here means the `CallExpression` arm never special-cases
        // `include(...)` (it falls through to the plain-statement path, the
        // historical behavior of this no-include entry point).
        lower_source_file_body(&self.walker, node, lambda_ctx, None)
    }
}

/// Extract the string path from an `include("path")` CallExpression node.
/// Returns `Ok(Some(path))` if the node is a valid include() call,
/// `Ok(None)` if it is not an include() call, or an error if the call
/// is malformed (e.g., dynamic path or missing argument).
fn try_extract_include_path<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<Option<String>> {
    let children = walker.named_children_vec(&node);
    let callee = match children.first() {
        Some(n) => n,
        None => return Ok(None),
    };
    if walker.kind(callee) != NodeKind::Identifier {
        return Ok(None);
    }
    if walker.text(callee) != "include" {
        return Ok(None);
    }
    let span = walker.span(&node);
    let args_node = children
        .iter()
        .find(|n| walker.kind(n) == NodeKind::ArgumentList);
    let path = if let Some(args) = args_node {
        let arg_children = walker.named_children_vec(args);
        if let Some(first_arg) = arg_children.first() {
            if walker.kind(first_arg) == NodeKind::StringLiteral {
                walker.text(first_arg).trim_matches('"').to_string()
            } else {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::IncludeCall("<dynamic path>".to_string()),
                    span,
                )
                .with_hint("include() requires a string literal path"));
            }
        } else {
            return Err(UnsupportedFeature::new(
                UnsupportedFeatureKind::IncludeCall("<missing argument>".to_string()),
                span,
            )
            .with_hint("include() requires a path argument"));
        }
    } else {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::IncludeCall("<no arguments>".to_string()),
            span,
        )
        .with_hint("include() requires a path argument"));
    };
    Ok(Some(path))
}

/// Lower a module definition: `module Name ... end` or `baremodule Name ... end`
fn lower_module_definition<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    is_bare: bool,
    include_ctx: Option<&IncludeContext>,
    // Macros defined inside a module are registered here so that they can be
    // resolved at the call site (Issue #7355 / #7350 A4). sjulia uses a flat
    // namespace — module functions are already hoisted globally — so a
    // module-defined macro must likewise be callable both unqualified (after
    // `using .M`) and qualified (`M.@m(...)`), and the parser drops the module
    // prefix in either case, leaving a bare macro name to look up.
    macro_ctx: Option<&LambdaContext>,
) -> LowerResult<Module> {
    let span = walker.span(&node);

    // Get module name from the 'name' field
    let name_node = walker.child_by_field(&node, "name").ok_or_else(|| {
        UnsupportedFeature::new(UnsupportedFeatureKind::ModuleDefinition, span)
            .with_hint("module definition must have a name")
    })?;

    // Only support simple identifier names (not interpolation)
    let name = match walker.kind(&name_node) {
        NodeKind::Identifier => walker.text(&name_node).to_string(),
        _ => {
            return Err(
                UnsupportedFeature::new(UnsupportedFeatureKind::ModuleDefinition, span).with_hint(
                    "module name must be a simple identifier (interpolation not supported)",
                ),
            )
        }
    };
    let _alias_module_scope = type_alias::ModuleScope::new(&name);

    // Get block (body of the module) - it's a child, not a field
    let body_node = walker
        .named_children(&node)
        .find(|n| walker.kind(n) == NodeKind::Block);

    // Extract functions, structs, abstract types, type aliases, exports, submodules, using statements, macros, and statements from the module body
    let mut functions = Vec::new();
    let mut structs = Vec::new();
    let mut abstract_types = Vec::new();
    let mut primitive_types = Vec::new();
    let mut type_aliases = Vec::new();
    let mut submodules = Vec::new();
    let mut usings = Vec::new();
    let mut macros = Vec::new();
    let mut exports = Vec::new();
    let mut publics = Vec::new();
    let mut body_stmts = Vec::new();
    let previous_macro_ctx_usings = macro_ctx.map(LambdaContext::snapshot_usings);

    // Track the enclosing module so a macro expanded inside this body binds
    // `__module__` to this module rather than the hard-coded `Main` (Issue
    // #7919). Only the macro-context path consumes this; pop after the loop.
    if let Some(ctx) = macro_ctx {
        ctx.push_current_module(&name);
    }
    let defining_module_path = macro_ctx
        .map(LambdaContext::current_module_path)
        .filter(|path| !path.is_empty())
        .map(|path| path.join("."))
        .unwrap_or_else(|| name.clone());

    if let Some(block_node) = body_node {
        let children = walker.named_children_vec(&block_node);
        for (idx, child) in children.iter().enumerate() {
            let kind = walker.kind(child);

            // Issue #4357: drop top-level docstrings inside module bodies too.
            // Same rule as `lower_source_file` — a `StringLiteral` immediately
            // followed by a definition is Julia documentation, not a value.
            if matches!(kind, NodeKind::StringLiteral)
                && is_top_level_docstring(walker, &children, idx)
            {
                continue;
            }

            let child = *child;
            if let Some((module_node, is_bare)) = explicit_doc_module_target(walker, child) {
                let submodule =
                    lower_module_definition(walker, module_node, is_bare, include_ctx, macro_ctx)?;
                submodules.push(submodule);
                continue;
            }

            match kind {
                // Skip comments
                NodeKind::LineComment | NodeKind::BlockComment => continue,
                // Handle struct definitions
                NodeKind::StructDefinition | NodeKind::MutableStructDefinition => {
                    let mut struct_def = match macro_ctx {
                        Some(ctx) => struct_::lower_struct_definition_with_ctx(walker, child, ctx)?,
                        None => struct_::lower_struct_definition(walker, child)?,
                    };
                    if let Some(ctx) = macro_ctx {
                        ctx.stamp_struct_definition(&mut struct_def);
                        ctx.add_compile_time_structs(std::slice::from_ref(&struct_def));
                    }
                    // Struct-body `global` helpers become global methods (#11005).
                    extend_struct_global_helpers(macro_ctx, &mut functions, &mut struct_def);
                    structs.push(struct_def);
                }
                // Handle abstract type definitions
                NodeKind::AbstractDefinition => {
                    let mut abstract_def = abstract_::lower_abstract_definition(walker, child)?;
                    if let Some(ctx) = macro_ctx {
                        ctx.stamp_abstract_definition(&mut abstract_def);
                        ctx.add_compile_time_abstract_types(std::slice::from_ref(&abstract_def));
                    }
                    abstract_types.push(abstract_def);
                }
                // Handle primitive type definitions
                NodeKind::PrimitiveDefinition => {
                    let mut primitive_def = primitive::lower_primitive_definition(walker, child)?;
                    if let Some(ctx) = macro_ctx {
                        ctx.stamp_primitive_definition(&mut primitive_def);
                        ctx.add_compile_time_primitive_types(std::slice::from_ref(&primitive_def));
                    }
                    primitive_types.push(primitive_def);
                }
                NodeKind::FunctionDefinition => {
                    let mut funcs = match macro_ctx {
                        Some(ctx) => lower_function_all_with_ctx_if_needed(walker, child, ctx)?,
                        None => function::lower_function_all(walker, child)?,
                    };
                    if let Some(ctx) = macro_ctx {
                        ctx.stamp_function_definitions(&mut funcs);
                        reject_macro_expanded_structs_in_non_toplevel(ctx, walker.span(&child))?;
                        ctx.add_compile_time_functions(&funcs);
                    }
                    functions.extend(funcs);
                }
                NodeKind::ShortFunctionDefinition => {
                    // Operator method definitions: *(x, y) = expr
                    let mut func = match macro_ctx {
                        Some(ctx) => lower_operator_method_with_ctx_if_needed(walker, child, ctx)?,
                        None => function::lower_operator_method(walker, child)?,
                    };
                    if let Some(ctx) = macro_ctx {
                        ctx.stamp_function_definitions(std::slice::from_mut(&mut func));
                        reject_macro_expanded_structs_in_non_toplevel(ctx, walker.span(&child))?;
                        ctx.add_compile_time_functions(std::slice::from_ref(&func));
                    }
                    functions.push(func);
                }
                NodeKind::ModuleDefinition => {
                    // Nested module: recursively lower. Pass the macro context
                    // through so submodule macros are also resolvable (flat
                    // namespace, Issue #7355).
                    let submodule =
                        lower_module_definition(walker, child, false, include_ctx, macro_ctx)?;
                    submodules.push(submodule);
                }
                NodeKind::BaremoduleDefinition => {
                    // Nested baremodule: recursively lower
                    let submodule =
                        lower_module_definition(walker, child, true, include_ctx, macro_ctx)?;
                    submodules.push(submodule);
                }
                NodeKind::UsingStatement | NodeKind::ImportStatement => {
                    // using Module or import Module (possibly comma-separated, e.g.
                    // `using A, B` → one UsingImport per module).
                    for mut using_import in lower_using_statement(walker, child)? {
                        if let Some(ctx) = macro_ctx {
                            ctx.stamp_using_import(&mut using_import);
                            ctx.add_using(&using_import.module);
                            ensure_stdlib_macros_loaded(&using_import.module);
                            ensure_bundled_package_macros_loaded(&using_import.module);
                        }
                        // Preserve the top-level execution point for runtime
                        // submodule-alias activation (Issues #11203/#11216).
                        body_stmts.push(Stmt::Using {
                            module: using_import.module.clone(),
                            span: using_import.span,
                        });
                        usings.push(using_import);
                    }
                }
                NodeKind::ExportStatement => {
                    // export func1, func2, ...
                    let export_names = lower_export_statement(walker, child)?;
                    exports.extend(export_names.clone());
                    body_stmts.push(Stmt::Export {
                        names: export_names,
                        span: walker.span(&child),
                    });
                }
                NodeKind::PublicStatement => {
                    // public func1, func2, ... (Julia 1.11+)
                    let public_names = lower_public_statement(walker, child)?;
                    publics.extend(public_names);
                }
                NodeKind::MacroDefinition => {
                    // Macro definition within module
                    let lifted_start = macro_ctx.map(LambdaContext::lifted_function_count);
                    let (macro_def, param_types) =
                        lower_macro_definition(walker, child, macro_ctx)?;
                    // Register into the call-site macro context so the macro is
                    // resolvable from outside the module (Issue #7355 / #7350 A4).
                    if let Some(ctx) = macro_ctx {
                        if let Some(start) = lifted_start {
                            let macro_lambdas = ctx.lifted_functions_from_index(start);
                            ctx.add_compile_time_functions(&macro_lambdas);
                        }
                        ctx.add_macro(
                            &macro_def.name,
                            StoredMacroDef {
                                params: macro_def.params.clone(),
                                param_types,
                                has_varargs: macro_def.has_varargs,
                                body: macro_def.body.clone(),
                                expansion_functions: vec![],
                                expansion_structs: vec![],
                                hygiene: None,
                                span: macro_def.span,
                            },
                        );
                    }
                    macros.push(macro_def);
                }
                NodeKind::Assignment if function::is_short_function_definition(walker, child) => {
                    // Short function definition: f(x) = expr
                    let mut funcs = match macro_ctx {
                        Some(ctx) => {
                            lower_short_function_all_with_ctx_if_needed(walker, child, ctx)?
                        }
                        None => function::lower_short_function_all(walker, child)?,
                    };
                    if let Some(ctx) = macro_ctx {
                        ctx.stamp_function_definitions(&mut funcs);
                        reject_macro_expanded_structs_in_non_toplevel(ctx, walker.span(&child))?;
                        ctx.add_compile_time_functions(&funcs);
                    }
                    functions.extend(funcs);
                }
                NodeKind::Assignment if function::is_lambda_assignment(walker, child) => {
                    // Lambda assignment: f = x -> expr
                    // May return multiple methods: the main lambda plus reduced-arity
                    // default-arg stubs for `(x, d=2) -> ...` (Issue #8047).
                    let mut funcs = match macro_ctx {
                        Some(ctx) => {
                            function::lower_lambda_assignment_with_ctx(walker, child, ctx)?
                        }
                        None => function::lower_lambda_assignment(walker, child)?,
                    };
                    if let Some(ctx) = macro_ctx {
                        ctx.stamp_function_definitions(&mut funcs);
                        ctx.add_compile_time_functions(&funcs);
                    }
                    functions.extend(funcs);
                }
                NodeKind::Assignment
                    if stmt::try_extract_type_alias_from_assignment(walker, child).is_some() =>
                {
                    // Issue #5055: a plain (non-`const`) type-alias definition.
                    // Preserve both the compile-time alias and Julia's runtime
                    // type-object binding (Issue #10501).
                    if let Some(type_alias) =
                        stmt::try_extract_type_alias_from_assignment(walker, child)
                    {
                        type_aliases.push(type_alias);
                    }
                    body_stmts.push(lower_stmt_with_macro_ctx_if_needed(
                        walker, child, macro_ctx,
                    )?);
                }
                NodeKind::ConstStatement => {
                    // Check if this is a type alias definition
                    if let Some(type_alias) = stmt::try_extract_type_alias(walker, child) {
                        type_aliases.push(type_alias);
                    }
                    // Always lower const statements so the variable is accessible at runtime
                    let stmt = lower_stmt_with_macro_ctx_if_needed(walker, child, macro_ctx)?;
                    if let Some(ctx) = macro_ctx {
                        drain_macro_expanded_structs(ctx, &mut structs, &mut functions);
                        drain_macro_expanded_macros(ctx, &mut macros);
                    }
                    body_stmts.push(stmt);
                }
                NodeKind::CallExpression => {
                    // Handle include() calls inside the module body when an include
                    // context is available (e.g. package loader, LoweringWithInclude).
                    if let Some(ctx) = include_ctx {
                        let span = walker.span(&child);
                        match try_extract_include_path(walker, child)? {
                            Some(path) => {
                                let included = ctx
                                    .include_file_with_macro_context(&path, span, macro_ctx)
                                    .map_err(|e| {
                                        UnsupportedFeature::new(
                                            UnsupportedFeatureKind::Other(e.to_string()),
                                            span,
                                        )
                                    })?;
                                if let Some(parent_ctx) = macro_ctx {
                                    parent_ctx.add_compile_time_abstract_types(
                                        &included.program.abstract_types,
                                    );
                                    parent_ctx.add_compile_time_primitive_types(
                                        &included.program.primitive_types,
                                    );
                                    parent_ctx.add_compile_time_structs(&included.program.structs);
                                    let included_funcs: Vec<Function> = included
                                        .program
                                        .functions
                                        .iter()
                                        .map(|f| (**f).clone())
                                        .collect();
                                    parent_ctx.add_compile_time_functions(&included_funcs);
                                    for macro_def in &included.program.macros {
                                        parent_ctx.add_macro_def(macro_def);
                                    }
                                }
                                let inline_block = included.merge_into(
                                    &mut functions,
                                    &mut structs,
                                    &mut abstract_types,
                                    &mut primitive_types,
                                    &mut type_aliases,
                                    &mut submodules,
                                    &mut usings,
                                    &mut macros,
                                    Some(&mut exports),
                                );
                                if let Some(parent_ctx) = macro_ctx {
                                    // Macros expanded while lowering an included
                                    // file may store struct metadata on the
                                    // parent module context. Drain after merging
                                    // so definitions like `@attributes mutable
                                    // struct ... end` become module structs
                                    // (Issue #7945).
                                    drain_macro_expanded_structs(
                                        parent_ctx,
                                        &mut structs,
                                        &mut functions,
                                    );
                                    drain_macro_expanded_macros(parent_ctx, &mut macros);
                                }
                                body_stmts.extend(inline_block.stmts);
                            }
                            None => {
                                let stmt =
                                    lower_stmt_with_macro_ctx_if_needed(walker, child, macro_ctx)?;
                                if let Some(ctx) = macro_ctx {
                                    drain_macro_expanded_structs(ctx, &mut structs, &mut functions);
                                    drain_macro_expanded_macros(ctx, &mut macros);
                                }
                                let (funcs, residual) = extract_module_function_defs(stmt);
                                extend_source_function_definitions(
                                    macro_ctx,
                                    &mut functions,
                                    funcs,
                                );
                                if let Some(stmt) = residual {
                                    body_stmts.push(*stmt);
                                }
                            }
                        }
                    } else {
                        let stmt = lower_stmt_with_macro_ctx_if_needed(walker, child, macro_ctx)?;
                        if let Some(ctx) = macro_ctx {
                            drain_macro_expanded_structs(ctx, &mut structs, &mut functions);
                            drain_macro_expanded_macros(ctx, &mut macros);
                        }
                        let (funcs, residual) = extract_module_function_defs(stmt);
                        extend_source_function_definitions(macro_ctx, &mut functions, funcs);
                        if let Some(stmt) = residual {
                            body_stmts.push(*stmt);
                        }
                    }
                }
                _ => {
                    let stmt = lower_stmt_with_macro_ctx_if_needed(walker, child, macro_ctx)?;
                    if let Some(ctx) = macro_ctx {
                        drain_macro_expanded_structs(ctx, &mut structs, &mut functions);
                        drain_macro_expanded_macros(ctx, &mut macros);
                    }
                    let (funcs, residual) = extract_module_function_defs(stmt);
                    extend_source_function_definitions(macro_ctx, &mut functions, funcs);
                    if let Some(stmt) = residual {
                        body_stmts.push(*stmt);
                    }
                }
            }
        }
    }

    if let Some(ctx) = macro_ctx {
        ctx.pop_current_module();
    }

    let mut body = Block {
        stmts: body_stmts,
        span,
    };
    // Module functions are hoisted separately from the top-level Program, so run
    // the closure boxing pass here as well as in the file-level lowering path.
    closure_box::box_captured_reassigned_locals(&mut functions, &mut body.stmts);
    collect_module_body_exports(&body, &mut exports, &name);

    // Register hygiene info for this module's macros now that the full set of
    // member names and real exports is known (Issue #7355 / #7350 A4). A
    // non-`esc` call target in a module-defined macro that names a module
    // member is qualified `M.name` at expansion time so unexported members
    // resolve in the defining module. The export set is kept separate so
    // macro-expanded `$__module__` literals do not expose unexported hygiene
    // members through `names($__module__)`.
    if let Some(ctx) = macro_ctx {
        if !macros.is_empty() {
            let mut members: HashSet<String> = HashSet::new();
            members.extend(
                functions
                    .iter()
                    .filter(|f| !f.is_base_extension)
                    .map(|f| f.name.clone()),
            );
            members.extend(structs.iter().map(|s| s.name.clone()));
            members.extend(abstract_types.iter().map(|a| a.name.clone()));
            members.extend(primitive_types.iter().map(|p| p.name.clone()));
            members.extend(type_aliases.iter().map(|t| t.name.clone()));
            let export_set: HashSet<String> = exports.iter().cloned().collect();
            for macro_def in &macros {
                ctx.register_module_macro_hygiene(
                    &macro_def.name,
                    &defining_module_path,
                    members.clone(),
                    export_set.clone(),
                );
            }
        }
    }

    let module = Module {
        name,
        is_bare,
        is_package_origin: false,
        is_base_origin: false,
        functions,
        structs,
        abstract_types,
        primitive_types,
        type_aliases,
        submodules,
        usings,
        macros,
        exports,
        publics,
        body,
        span,
    };

    if let (Some(ctx), Some(previous_usings)) = (macro_ctx, previous_macro_ctx_usings) {
        ctx.restore_usings(previous_usings);
    }

    Ok(module)
}

/// Lower an export statement: `export func1, func2, ...`
/// Returns the list of exported names.
pub fn lower_export_statement<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<Vec<String>> {
    let mut names = Vec::new();

    // Collect all identifier children as exported names
    for child in walker.named_children(&node) {
        if walker.kind(&child) == NodeKind::Identifier {
            names.push(walker.text(&child).to_string());
        }
    }

    Ok(names)
}

fn collect_module_body_exports(block: &Block, exports: &mut Vec<String>, module_name: &str) {
    let mut known_exports: HashSet<String> = HashSet::new();
    let mut emitted_exports: HashSet<String> = exports.iter().cloned().collect();
    known_exports.insert(module_name.to_string());
    collect_module_body_exports_inner(
        block,
        exports,
        &mut known_exports,
        &mut emitted_exports,
        module_name,
    );
}

fn collect_module_body_exports_inner(
    block: &Block,
    exports: &mut Vec<String>,
    known_exports: &mut HashSet<String>,
    emitted_exports: &mut HashSet<String>,
    module_name: &str,
) {
    for stmt in &block.stmts {
        match stmt {
            Stmt::Export { names, .. } => {
                for name in names {
                    known_exports.insert(name.clone());
                    if emitted_exports.insert(name.clone()) {
                        exports.push(name.clone());
                    }
                }
            }
            Stmt::Block(inner) => {
                collect_module_body_exports_inner(
                    inner,
                    exports,
                    known_exports,
                    emitted_exports,
                    module_name,
                );
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => match eval_module_export_condition(condition, known_exports, module_name) {
                Some(true) => collect_module_body_exports_inner(
                    then_branch,
                    exports,
                    known_exports,
                    emitted_exports,
                    module_name,
                ),
                Some(false) => {
                    if let Some(else_branch) = else_branch {
                        collect_module_body_exports_inner(
                            else_branch,
                            exports,
                            known_exports,
                            emitted_exports,
                            module_name,
                        );
                    }
                }
                None => {}
            },
            _ => {}
        }
    }
}

fn eval_module_export_condition(
    expr: &Expr,
    known_exports: &HashSet<String>,
    module_name: &str,
) -> Option<bool> {
    match expr {
        Expr::Literal(Literal::Bool(value), _) => Some(*value),
        Expr::Var(name, _) if name == "true" => Some(true),
        Expr::Var(name, _) if name == "false" => Some(false),
        Expr::UnaryOp {
            op: crate::ir::core::UnaryOp::Not,
            operand,
            ..
        } => eval_module_export_condition(operand, known_exports, module_name).map(|value| !value),
        Expr::Call { function, args, .. }
            if matches!(function.as_str(), "in" | "∈") && args.len() == 2 =>
        {
            eval_symbol_in_module_names(&args[0], &args[1], known_exports, module_name)
        }
        Expr::Call { function, args, .. }
            if matches!(function.as_str(), "∉") && args.len() == 2 =>
        {
            eval_symbol_in_module_names(&args[0], &args[1], known_exports, module_name)
                .map(|value| !value)
        }
        Expr::Builtin {
            name: BuiltinOp::In,
            args,
            ..
        } if args.len() == 2 => {
            eval_symbol_in_module_names(&args[0], &args[1], known_exports, module_name)
        }
        _ => None,
    }
}

fn eval_symbol_in_module_names(
    needle: &Expr,
    haystack: &Expr,
    known_exports: &HashSet<String>,
    module_name: &str,
) -> Option<bool> {
    let symbol = expr_symbol_name(needle)?;
    if names_call_module_name(haystack)? != module_name {
        return None;
    }
    Some(known_exports.contains(symbol))
}

fn expr_symbol_name(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Literal(Literal::Symbol(name), _) => Some(name),
        Expr::Literal(Literal::QuoteNode(inner), _) => match inner.as_ref() {
            Literal::Symbol(name) => Some(name),
            _ => None,
        },
        Expr::QuoteLiteral { constructor, .. } => expr_symbol_name(constructor),
        Expr::Builtin {
            name: BuiltinOp::SymbolNew,
            args,
            ..
        } if args.len() == 1 => match &args[0] {
            Expr::Literal(Literal::Str(name), _) => Some(name),
            _ => None,
        },
        _ => None,
    }
}

fn names_call_module_name(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Call { function, args, .. } if function == "names" && args.len() == 1 => {
            expr_module_name(&args[0])
        }
        _ => None,
    }
}

fn expr_module_name(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Literal(Literal::Module(name), _) => Some(name),
        Expr::Var(name, _) => Some(name),
        _ => None,
    }
}

/// Lower a public statement: `public func1, func2, ...` (Julia 1.11+)
/// Returns the list of public names.
fn lower_public_statement<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Vec<String>> {
    let mut names = Vec::new();

    // Collect all identifier children as public names
    for child in walker.named_children(&node) {
        if walker.kind(&child) == NodeKind::Identifier {
            names.push(walker.text(&child).to_string());
        }
    }

    Ok(names)
}

/// Lower a using/import statement: `using Module` or `using Module: func1, func2`
/// Also handles relative imports: `using .Module` (references user-defined modules)
/// Lower a `using` / `import` statement into source-ordered `UsingImport` entries.
///
/// Returns a vector because Julia allows a comma-separated list — `using A, B` is
/// `using A; using B` (and `import A, B` likewise). The CST shape is
/// `UsingStatement > import_list > import_path+`, so each `import_path` child is
/// lowered independently via [`lower_one_import_path`]. A selective path may
/// produce one entry per binding to retain conflict order; a single-module
/// `using A` still yields one entry. (Previously only the whole `import_list` was
/// read, so `using A, B` produced a single bogus module named `"A, B"`.)
fn lower_using_statement<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<Vec<UsingImport>> {
    let span = walker.span(&node);

    // The statement's named child is the `import_list`; its named children are the
    // individual `import_path`s. Fall back to treating the statement's own named
    // children as paths if no `import_list` wrapper is present (defensive).
    let named = walker.named_children_vec(&node);
    let paths: Vec<Node<'a>> = match named.first() {
        Some(first) if first.kind() == "import_list" => walker.named_children_vec(first),
        _ => named,
    };

    if paths.is_empty() {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::UsingStatement, span)
                .with_hint("using statement must specify a module name"),
        );
    }

    let is_import_statement = walker.kind(&node) == NodeKind::ImportStatement;
    let mut imports = Vec::with_capacity(paths.len());
    for path in paths {
        imports.extend(lower_one_import_path(
            walker,
            path,
            span,
            is_import_statement,
        )?);
    }
    Ok(imports)
}

/// Split a single import entry of the form `name as alias` into its parts,
/// returning `None` when there is no `as` rename. The `as` is matched as a
/// whitespace-delimited keyword token, so identifiers that merely contain the
/// letters `as` (e.g. `mass`) are left untouched (Issue #8117).
fn split_as_rename(entry: &str) -> Option<(String, String)> {
    let tokens: Vec<&str> = entry.split_whitespace().collect();
    if tokens.len() == 3 && tokens[1] == "as" {
        Some((tokens[0].to_string(), tokens[2].to_string()))
    } else {
        None
    }
}

/// Lower a single `import_path` node into source-ordered `UsingImport` entries.
///
/// Handles plain (`A`), scoped (`Base.Sort`), relative (`.A`), selective
/// (`A: f, g`), and renaming (`A as B`, `A: f as g`) forms. Selective imports
/// produce one entry per selected binding so an explicit conflict between a
/// plain symbol and an `as` rename keeps its textual first-wins order (Issue
/// #11176). Scoped names like `Base.Sort` have no `:` and remain one entry.
fn lower_one_import_path<'a>(
    walker: &CstWalker<'a>,
    path: Node<'a>,
    span: Span,
    is_import_statement: bool,
) -> LowerResult<Vec<UsingImport>> {
    // The path text carries no leading `using`/`import` keyword (that lives on the
    // enclosing statement node), e.g. `"Plots"`, `"Base.Sort"`, `"Plots: plot"`.
    let path_text = walker.text(&path);

    // Selective import: `Module: func1, func2` (each entry may be `f as g`).
    if let Some(colon_pos) = path_text.find(':') {
        let before_colon = path_text[..colon_pos].trim();
        let relative_level = before_colon.chars().take_while(|c| *c == '.').count();
        let is_relative = relative_level > 0;
        let module_name = before_colon.trim_start_matches('.').to_string();

        if module_name.is_empty() {
            return Err(
                UnsupportedFeature::new(UnsupportedFeatureKind::UsingStatement, span)
                    .with_hint("using statement must specify a module name"),
            );
        }

        let after_colon = path_text[colon_pos + 1..].trim();
        let mut imports = Vec::new();
        for raw_entry in after_colon.split(',') {
            let entry = raw_entry.trim();
            if entry.is_empty() {
                continue;
            }
            if let Some((original, alias)) = split_as_rename(entry) {
                // `using M: f as g` binds only `g` to `M.f`; `f` itself stays
                // unbound (matching Julia), so it is kept out of `symbols`.
                // `Base` is sjulia's implicit global namespace, so a `Base.f`
                // rename binds to the bare global name `f` (which always resolves,
                // unlike `Base.f` as a value for many builtins).
                let source = if module_name == "Base" {
                    original
                } else {
                    format!("{module_name}.{original}")
                };
                imports.push(UsingImport {
                    module: module_name.clone(),
                    is_import: is_import_statement,
                    symbols: Some(Vec::new()),
                    is_relative,
                    relative_level,
                    alias_bindings: vec![(source, alias)],
                    span,
                });
            } else {
                imports.push(UsingImport {
                    module: module_name.clone(),
                    is_import: is_import_statement,
                    symbols: Some(vec![entry.to_string()]),
                    is_relative,
                    relative_level,
                    alias_bindings: Vec::new(),
                    span,
                });
            }
        }

        if imports.is_empty() {
            return Err(
                UnsupportedFeature::new(UnsupportedFeatureKind::UsingStatement, span)
                    .with_hint("selective import must specify at least one symbol"),
            );
        }

        return Ok(imports);
    }

    // Regular import: `Module`, `.Module`, `Base.Sort`, or a renaming
    // `Module as Alias`. Take the path text as the module name; whitespace
    // (other than around `as`) cannot occur inside a single non-selective path.
    let raw_module_text = path_text.trim();
    if raw_module_text.is_empty() {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::UsingStatement, span)
                .with_hint("using statement must specify a module name"),
        );
    }

    // Whole-module rename: `import Module as Alias` binds `Alias` to `Module`.
    let (module_text, alias_bindings) = match split_as_rename(raw_module_text) {
        Some((module_part, alias)) => {
            let source = module_part.trim_start_matches('.').to_string();
            (module_part, vec![(source, alias)])
        }
        None => (raw_module_text.to_string(), Vec::new()),
    };

    let relative_level = module_text.chars().take_while(|c| *c == '.').count();
    let is_relative = relative_level > 0;
    let module_name = module_text.trim_start_matches('.').to_string();

    if is_import_statement && alias_bindings.is_empty() {
        if let Some((parent_module, symbol)) = module_name.rsplit_once('.') {
            return Ok(vec![UsingImport {
                module: parent_module.to_string(),
                is_import: is_import_statement,
                symbols: Some(vec![symbol.to_string()]),
                is_relative,
                relative_level,
                alias_bindings,
                span,
            }]);
        }
    }

    Ok(vec![UsingImport {
        module: module_name,
        is_import: is_import_statement,
        symbols: None,
        is_relative,
        relative_level,
        alias_bindings,
        span,
    }])
}

/// Parse a type annotation for a macro parameter.
/// Recognizes types like Symbol, Expr, Integer, Float, String, LineNumberNode.
fn parse_macro_param_type<'a>(walker: &CstWalker<'a>, type_node: &Node<'a>) -> MacroParamType {
    let type_name = walker.text(type_node);
    match type_name {
        "Symbol" => MacroParamType::Symbol,
        "Expr" => MacroParamType::Expr,
        "Integer" | "Int" | "Int64" => MacroParamType::Integer,
        "Float" | "Float64" => MacroParamType::Float,
        "String" => MacroParamType::String,
        "LineNumberNode" => MacroParamType::LineNumberNode,
        _ => MacroParamType::Any, // Unknown types match anything
    }
}

/// Lower a macro definition: `macro name(args) body end`
/// Macros receive AST nodes as parameters, not values.
/// Returns both the MacroDef and the extracted parameter types for dispatch.
fn lower_macro_definition<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<(MacroDef, Vec<MacroParamType>)> {
    let span = walker.span(&node);
    let mut name: Option<String> = None;
    let mut params: Vec<String> = Vec::new();
    let mut param_types: Vec<MacroParamType> = Vec::new();
    let mut has_varargs = false;
    let mut body: Option<Block> = None;

    for child in walker.named_children(&node) {
        match walker.kind(&child) {
            NodeKind::Identifier | NodeKind::Operator if name.is_none() => {
                name = Some(walker.text(&child).to_string());
            }
            NodeKind::ParameterList => {
                // Extract parameter names and types from the parameter list
                let param_nodes = walker.named_children_vec(&child);
                for (idx, param_node) in param_nodes.iter().enumerate() {
                    // Check if this is the last parameter and if it's a splat parameter
                    let is_last = idx == param_nodes.len() - 1;
                    let kind = walker.kind(param_node);
                    let text = walker.text(param_node);

                    match kind {
                        NodeKind::Identifier => {
                            params.push(text.to_string());
                            param_types.push(MacroParamType::Any);
                        }
                        NodeKind::TypedParameter => {
                            // For typed params like `ex::Expr`, extract name and type
                            let children = walker.named_children_vec(param_node);
                            if let Some(id) = children.first() {
                                if walker.kind(id) == NodeKind::Identifier {
                                    params.push(walker.text(id).to_string());
                                }
                            }
                            // Extract type from the second child (type annotation)
                            let param_type = if children.len() > 1 {
                                parse_macro_param_type(walker, &children[1])
                            } else {
                                MacroParamType::Any
                            };
                            param_types.push(param_type);
                        }
                        NodeKind::Parameter => {
                            // The Rust parser may return Parameter with text like "x::Symbol"
                            // Check if this is a typed parameter by looking for "::" in text
                            if let Some(colon_pos) = text.find("::") {
                                let name = text[..colon_pos].trim();
                                let type_name = text[colon_pos + 2..].trim();
                                params.push(name.to_string());
                                let param_type = match type_name {
                                    "Symbol" => MacroParamType::Symbol,
                                    "Expr" => MacroParamType::Expr,
                                    "Integer" | "Int" | "Int64" => MacroParamType::Integer,
                                    "Float" | "Float64" => MacroParamType::Float,
                                    "String" => MacroParamType::String,
                                    "LineNumberNode" => MacroParamType::LineNumberNode,
                                    _ => MacroParamType::Any,
                                };
                                param_types.push(param_type);
                            } else {
                                // No type annotation, treat as identifier
                                params.push(text.to_string());
                                param_types.push(MacroParamType::Any);
                            }
                        }
                        NodeKind::SplatParameter | NodeKind::SplatExpression => {
                            // Varargs parameter: p... or p::T...
                            // Handle both SplatParameter (full-form) and SplatExpression (short-form)
                            // per Issue #2253 duality requirement
                            // Extract the parameter name from the first child (Identifier)
                            let named = walker.named_children_vec(param_node);
                            if let Some(name_node) = named.first() {
                                if walker.kind(name_node) == NodeKind::Identifier {
                                    params.push(walker.text(name_node).to_string());
                                } else {
                                    // Try to get text as parameter name
                                    let text = walker.text(name_node);
                                    if !text.is_empty() {
                                        params.push(text.to_string());
                                    }
                                }
                            }
                            param_types.push(MacroParamType::Any);
                            // Mark that this macro has varargs (must be the last parameter)
                            if is_last {
                                has_varargs = true;
                            }
                        }
                        _ => {
                            // Try to get text as parameter name
                            if !text.is_empty() {
                                params.push(text.to_string());
                                param_types.push(MacroParamType::Any);
                            }
                        }
                    }
                }
            }
            NodeKind::Block => {
                body = Some(match lambda_ctx {
                    Some(ctx) if requires_lambda_context(walker, child) => {
                        stmt::lower_block_with_ctx(walker, child, ctx)?
                    }
                    _ => stmt::lower_block(walker, child)?,
                });
            }
            _ => {}
        }
    }

    let name = name.ok_or_else(|| {
        UnsupportedFeature::new(UnsupportedFeatureKind::MacroDefinition, span)
            .with_hint("macro definition must have a name")
    })?;

    let body = body.unwrap_or_else(|| Block {
        stmts: vec![],
        span,
    });

    Ok((
        MacroDef {
            name,
            params,
            has_varargs,
            body,
            span,
        },
        param_types,
    ))
}

/// Lowering with include support.
/// This struct extends the basic Lowering with the ability to process include() calls.
pub struct LoweringWithInclude<'a> {
    _source: &'a str,
    walker: CstWalker<'a>,
    include_ctx: IncludeContext,
    current_file: Option<PathBuf>,
    /// Store the parsed source so it lives long enough for Node references
    parsed_rust: Option<crate::parser::RustParsedSource>,
}

impl<'a> LoweringWithInclude<'a> {
    pub fn new(source: &'a str, include_ctx: IncludeContext) -> Self {
        Self::new_with_file(source, include_ctx, None)
    }

    pub fn new_with_file(
        source: &'a str,
        include_ctx: IncludeContext,
        current_file: Option<PathBuf>,
    ) -> Self {
        Self {
            _source: source,
            walker: CstWalker::new(source),
            include_ctx,
            current_file,
            parsed_rust: None,
        }
    }

    /// Create a new lowering context with optional base directory.
    pub fn with_base_dir(source: &'a str, base_dir: Option<PathBuf>) -> Self {
        let current_file = base_dir.as_ref().map(|dir| dir.join("__source__.jl"));
        Self::new_with_file(source, IncludeContext::new(base_dir), current_file)
    }

    pub fn lower(&mut self, parse_outcome: ParseOutcome) -> LowerResult<Program> {
        let lambda_ctx = LambdaContext::with_file(self.current_file_literal());
        self.lower_with_lambda_context(parse_outcome, &lambda_ctx)
    }

    pub fn lower_with_lambda_context(
        &mut self,
        parse_outcome: ParseOutcome,
        lambda_ctx: &LambdaContext,
    ) -> LowerResult<Program> {
        let ParseOutcome::Rust(parsed) = parse_outcome;
        self.parsed_rust = Some(parsed);
        // `parsed_rust` was just assigned `Some(..)` on the line above and
        // `&mut self` excludes concurrent mutation, so the read below cannot
        // observe `None` today; guarded rather than a raw unwrap so a future
        // refactor that breaks this invariant surfaces a typed error instead
        // of an uncaught host crash (Issue #10905, Phase 1b of #10869).
        let parsed_ref = self.parsed_rust.as_ref().ok_or_else(|| {
            internal_lowering_error(
                Span::new(0, 0, 0, 0, 0, 0),
                "parsed_rust was just assigned Some on the previous line",
            )
        })?;
        let root = Node::new(parsed_ref.root(), parsed_ref.source());
        lambda_ctx.with_current_file(self.current_file_literal(), || {
            self.lower_source_file(root, lambda_ctx)
        })
    }

    /// Like [`lower_with_lambda_context`], but skips this call's own
    /// type-alias `snapshot`/`restore` wrap (Issue #10119/#10122).
    ///
    /// Used to lower a BATCH of independent source fragments (the Base
    /// prelude split into one fragment per file so cold-start parsing can be
    /// timed/parallelized per file) that must behave as if they were one
    /// concatenated whole-text lowering pass, just performed incrementally:
    /// a type alias registered by an earlier fragment must stay visible while
    /// lowering a later one, which a per-fragment snapshot/restore would
    /// discard.
    ///
    /// Callers MUST wrap the whole batch (every fragment) in one
    /// `type_alias::snapshot()` / `.restore()` pair themselves, and must lower
    /// fragments strictly in dependency order, threading ONE shared
    /// [`LambdaContext`] through every call (so `LambdaContext`'s
    /// per-instance `lambda_counter` — which names lifted anonymous lambdas —
    /// stays globally unique across the batch instead of restarting at 0 per
    /// fragment and risking a name collision between two files that each lift
    /// an anonymous lambda).
    pub fn lower_fragment_with_shared_context(
        &mut self,
        parse_outcome: ParseOutcome,
        lambda_ctx: &LambdaContext,
    ) -> LowerResult<Program> {
        let ParseOutcome::Rust(parsed) = parse_outcome;
        self.parsed_rust = Some(parsed);
        // `parsed_rust` was just assigned `Some(..)` on the line above and
        // `&mut self` excludes concurrent mutation, so the read below cannot
        // observe `None` today; guarded rather than a raw unwrap so a future
        // refactor that breaks this invariant surfaces a typed error instead
        // of an uncaught host crash (Issue #10905, Phase 1b of #10869).
        let parsed_ref = self.parsed_rust.as_ref().ok_or_else(|| {
            internal_lowering_error(
                Span::new(0, 0, 0, 0, 0, 0),
                "parsed_rust was just assigned Some on the previous line",
            )
        })?;
        let root = Node::new(parsed_ref.root(), parsed_ref.source());
        let source_scope = type_alias::SourceScope::new();
        let mut module_owner = lambda_ctx.current_module_path();
        prescan_and_register_type_aliases(&self.walker, root, &mut module_owner, &source_scope);
        lambda_ctx.with_current_file(self.current_file_literal(), || {
            self.lower_source_file_inner(root, lambda_ctx)
        })
    }

    /// Get a reference to the include context.
    pub fn include_context(&self) -> &IncludeContext {
        &self.include_ctx
    }

    fn current_file_literal(&self) -> Option<String> {
        self.current_file
            .as_ref()
            .map(|path| path.to_string_lossy().to_string())
    }

    /// Lower a source file (or module body) to a `Program`. Wraps the inner
    /// lowering with a type-alias scope (Issue #5055): user-defined aliases are
    /// pre-registered for this pass and the prior alias table is restored on
    /// exit, so a nested pass (e.g. the stdlib load triggered by `using Test`)
    /// cannot destroy the enclosing program's aliases.
    fn lower_source_file(
        &self,
        node: Node<'a>,
        lambda_ctx: &LambdaContext,
    ) -> LowerResult<Program> {
        let scope = type_alias::snapshot();
        let source_scope = type_alias::SourceScope::new();
        let mut module_owner = lambda_ctx.current_module_path();
        prescan_and_register_type_aliases(&self.walker, node, &mut module_owner, &source_scope);
        let result = self.lower_source_file_inner(node, lambda_ctx);
        scope.restore();
        result
    }

    fn lower_source_file_inner(
        &self,
        node: Node<'a>,
        lambda_ctx: &LambdaContext,
    ) -> LowerResult<Program> {
        lower_source_file_body(&self.walker, node, lambda_ctx, Some(&self.include_ctx))
    }
}

// ==================== @kwdef Macro Expansion ====================

/// Check if a macro call is @kwdef
fn is_kwdef_macro<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> bool {
    if let Some(macro_ident) = walker.find_child(&node, NodeKind::MacroIdentifier) {
        let text = walker.text(&macro_ident);
        let name = text.trim_start_matches('@');
        return name == "kwdef";
    }
    false
}

/// Expand @kwdef macro to a struct definition and constructor function
fn expand_kwdef_macro<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<(StructDef, Function)> {
    use crate::ir::core::{Expr, KwParam, Literal, Stmt};
    use crate::types::JuliaType;

    let span = walker.span(&node);

    // Find the struct definition child
    let struct_node = walker
        .named_children(&node)
        .find(|n| {
            matches!(
                walker.kind(n),
                NodeKind::StructDefinition | NodeKind::MutableStructDefinition
            )
        })
        .ok_or_else(|| {
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@kwdef requires a struct definition")
        })?;

    // Parse the struct definition
    let struct_def = struct_::lower_struct_definition_with_ctx(walker, struct_node, lambda_ctx)?;

    // Parse default values from the struct body
    let defaults = parse_kwdef_defaults(walker, struct_node)?;

    // Build keyword parameters from struct fields with defaults
    let kwparams: Vec<KwParam> = struct_def
        .fields
        .iter()
        .map(|f| {
            // Convert TypeExpr to JuliaType if available
            let type_annotation = f.type_expr.as_ref().and_then(|te| {
                match te {
                    crate::types::TypeExpr::Concrete(jt) => Some(jt.clone()),
                    crate::types::TypeExpr::TypeVar(name) => JuliaType::from_name(name),
                    crate::types::TypeExpr::Parameterized { base, .. } => {
                        JuliaType::from_name(base)
                    }
                    crate::types::TypeExpr::RuntimeExpr(_) => None, // Runtime expressions can't be resolved at lowering
                }
            });

            // Use the parsed default value, or Undef to mark as required
            let default_expr = defaults
                .get(&f.name)
                .cloned()
                .unwrap_or(Expr::Literal(Literal::Undef, f.span));

            KwParam {
                name: f.name.clone(),
                default: default_expr,
                type_annotation,
                is_varargs: false,
                body_evaluated_default: false,
                span: f.span,
            }
        })
        .collect();

    // Create constructor body: Point(; x, y) = Point(x, y)
    let field_args: Vec<Expr> = struct_def
        .fields
        .iter()
        .map(|f| Expr::Var(f.name.clone().into(), f.span))
        .collect();

    let constructor_call = Expr::Call {
        function: struct_def.name.clone().into(),
        args: field_args,
        kwargs: vec![],
        splat_mask: vec![],
        kwargs_splat_mask: vec![],
        span,
    };

    let body = Block {
        stmts: vec![Stmt::Expr {
            expr: constructor_call,
            span,
        }],
        span,
    };

    let constructor_func = Function {
        name: struct_def.name.clone(),
        params: vec![],
        kwparams,
        type_params: struct_def.type_params.clone(),
        return_type: None,
        body,
        is_base_extension: false,
        is_runtime_eval: false,
        span,
        new_struct_name: None,
    };

    Ok((struct_def, constructor_func))
}

/// Parse default values from @kwdef struct body
/// Returns a map from field name to default expression
fn parse_kwdef_defaults<'a>(
    walker: &CstWalker<'a>,
    struct_node: Node<'a>,
) -> LowerResult<std::collections::HashMap<String, crate::ir::core::Expr>> {
    use std::collections::HashMap;

    let mut defaults = HashMap::new();
    let lambda_ctx = LambdaContext::new();

    // Find the block/body inside the struct
    for child in walker.named_children(&struct_node) {
        let kind = walker.kind(&child);
        if kind == NodeKind::Block || kind == NodeKind::CompoundStatement {
            // Parse each field in the block
            for field_node in walker.named_children(&child) {
                let field_kind = walker.kind(&field_node);
                if field_kind == NodeKind::Assignment {
                    // This is a field with a default value: x::Type = default
                    let children = walker.named_children_vec(&field_node);
                    if children.len() >= 2 {
                        let lhs = children[0];
                        let rhs = children[children.len() - 1];

                        // Get the field name from the LHS
                        let field_name = match walker.kind(&lhs) {
                            NodeKind::TypedExpression => {
                                // x::Type = default
                                let typed_children = walker.named_children_vec(&lhs);
                                if !typed_children.is_empty() {
                                    walker.text(&typed_children[0]).to_string()
                                } else {
                                    continue;
                                }
                            }
                            NodeKind::Identifier => {
                                // x = default (no type annotation)
                                walker.text(&lhs).to_string()
                            }
                            _ => continue,
                        };

                        // Parse the default value expression
                        if let Ok(default_expr) =
                            expr::lower_expr_with_ctx(walker, rhs, &lambda_ctx)
                        {
                            defaults.insert(field_name, default_expr);
                        }
                    }
                }
            }
        }
    }

    Ok(defaults)
}

/// Lower a Julia expression from text.
/// This is used to compile runtime type expressions like `Symbol(s)` in `MIME{Symbol(s)}`.
///
/// This is a simplified parser that handles common patterns:
/// - `Symbol(s)` -> Builtin { name: SymbolNew, args: [Var("s")] }
/// - Variable references like `T` -> Var("T")
pub fn lower_expr_from_text(text: &str) -> LowerResult<crate::ir::core::Expr> {
    use crate::ir::core::Expr;

    let text = text.trim();
    let span = Span::new(0, text.len(), 1, 1, 1, text.len() + 1);

    // Route the source text through the real parser + expression lowering so
    // nested constructs survive — a runtime type argument such as
    // `typeof(float(x))` (Issue #7240) contains a NESTED call inside the
    // argument list, which the previous hand-rolled comma-splitter captured as
    // a single `Var("float(x)")`. Reusing the full pipeline keeps these in sync
    // with ordinary expression lowering (nested calls, broadcasts, operators,
    // string literals, …) instead of re-implementing a partial parser here.
    if let Some(expr) = try_lower_expr_via_parser(text) {
        return Ok(expr);
    }

    // Fallback: treat as variable reference. Reached only when the text fails to
    // parse as a standalone expression (e.g. a bare type variable like `T`,
    // which the parser yields as an identifier and we already handle).
    Ok(Expr::Var(text.to_string().into(), span))
}

/// Parse `text` as a single Julia expression and lower it to Core IR using the
/// real parser/lowering pipeline. Returns `None` when the text does not parse
/// into exactly one expression-bearing top-level node, leaving the caller to
/// fall back. Used by `lower_expr_from_text` for runtime type-argument
/// expressions captured as source strings (Issue #7240).
fn try_lower_expr_via_parser(text: &str) -> Option<crate::ir::core::Expr> {
    let mut parser = Parser::new().ok()?;
    let parse_outcome = parser.parse(text).ok()?;
    let ParseOutcome::Rust(parsed) = parse_outcome;
    let root = Node::new(parsed.root(), parsed.source());
    let walker = CstWalker::new(text);

    // The first non-comment named child of the source file is the expression.
    let first = walker.named_children(&root).find(|child| {
        !matches!(
            walker.kind(child),
            NodeKind::LineComment | NodeKind::BlockComment
        )
    })?;

    expr::lower_expr(&walker, first).ok()
}

#[cfg(test)]
mod macro_hygiene_path_tests {
    #[test]
    fn macro_hygiene_uses_absolute_defining_module_path_11240() {
        let ctx = super::LambdaContext::new();
        ctx.begin_macro_hygiene_frame(
            "MacroTools",
            std::collections::HashSet::from(["trymatch".to_string()]),
        );
        assert_eq!(
            ctx.qualify_module_macro_member("trymatch").as_deref(),
            Some("Main.MacroTools")
        );
        assert_eq!(ctx.qualify_module_macro_member("caller_local"), None);
        ctx.end_macro_hygiene();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use crate::ir::core::{Expr, Literal, Stmt};

    #[test]
    fn flat_nonliteral_destructuring_has_explicit_ir_10464() {
        let program = crate::pipeline::parse_and_lower(
            r#"
            pair() = (1, 2)
            (a, b) = pair()
            "#,
        )
        .expect("flat nonliteral destructuring should lower");

        let destructuring = program
            .main
            .stmts
            .iter()
            .find(|stmt| matches!(stmt, Stmt::DestructuringAssign { .. }))
            .unwrap_or_else(|| {
                panic!(
                    "flat nonliteral destructuring must retain explicit IR identity; got {:?}",
                    program.main.stmts
                )
            });
        assert!(matches!(
            destructuring,
            Stmt::DestructuringAssign {
                targets,
                value: Expr::Call { function, .. },
                ..
            } if targets == &["a".to_string(), "b".to_string()] && function == "pair"
        ));
    }

    #[test]
    fn destructuring_tail_temps_are_unique_even_for_same_span_10464() {
        let span = crate::span::Span::new(7, 9, 1, 1, 1, 3);
        let stmt = || Stmt::DestructuringAssign {
            targets: vec!["a".to_string(), "b".to_string()],
            value: Expr::Var("rhs".to_string().into(), span),
            span,
        };
        let (first, _, _) = super::expr::split_destructuring_stmt_via_temp(stmt()).unwrap();
        let (second, _, _) = super::expr::split_destructuring_stmt_via_temp(stmt()).unwrap();

        assert_ne!(first, second);
        assert!(first.starts_with('#') && second.starts_with('#'));
    }

    fn lower_expr_for_test(source: &str) -> Expr {
        super::lower_expr_from_text(source).expect("expression should lower")
    }

    fn assert_nonempty_letblock(expr: &Expr, label: &str) {
        assert!(
            matches!(expr, Expr::LetBlock { bindings, .. } if !bindings.is_empty()),
            "{label} should bind the non-atomic comparison-chain interior once; got {expr:?}"
        );
    }

    fn broadcast_operand_tuple(expr: &Expr) -> &[Expr] {
        let Expr::Call { function, args, .. } = expr else {
            panic!("expected materialize(Broadcasted(...)), got {expr:?}");
        };
        assert_eq!(function, "materialize");
        assert_eq!(args.len(), 1);
        let Expr::Call {
            function,
            args: broadcast_args,
            ..
        } = &args[0]
        else {
            panic!("expected Broadcasted call, got {:?}", args[0]);
        };
        assert_eq!(function, "Broadcasted");
        assert_eq!(broadcast_args.len(), 2);
        let Expr::TupleLiteral { elements, .. } = &broadcast_args[1] else {
            panic!(
                "expected Broadcasted operand tuple, got {:?}",
                broadcast_args[1]
            );
        };
        elements
    }

    #[test]
    fn broadcast_call_lowering_distinguishes_argument_list_from_tuple_operand_9805() {
        let two_operands = lower_expr_for_test("f.(x, y)");
        let args = broadcast_operand_tuple(&two_operands);
        assert_eq!(
            args.len(),
            2,
            "f.(x, y) should lower to two broadcast operands, got {args:?}"
        );

        let one_tuple_operand = lower_expr_for_test("f.((x, y))");
        let args = broadcast_operand_tuple(&one_tuple_operand);
        assert_eq!(
            args.len(),
            1,
            "f.((x, y)) should lower to one tuple broadcast operand, got {args:?}"
        );
        assert!(
            matches!(
                &args[0],
                Expr::TupleLiteral { elements, .. } if elements.len() == 2
            ),
            "the single broadcast operand should be the tuple literal, got {:?}",
            args[0]
        );
    }

    #[test]
    fn comparison_chain_non_atomic_interiors_lower_to_letblock_9632() {
        let scalar = lower_expr_for_test("0 < f() < 2");
        assert_nonempty_letblock(&scalar, "scalar chain");

        let dotted = lower_expr_for_test("0 .< f() .< 2");
        assert_nonempty_letblock(&dotted, "dotted chain");

        let atomic = lower_expr_for_test("0 < x < 2");
        assert!(
            !matches!(&atomic, Expr::LetBlock { bindings, .. } if !bindings.is_empty()),
            "atomic comparison-chain interior should not need a temporary; got {atomic:?}"
        );
    }

    #[test]
    fn begin_statement_preserves_following_function_tail_8761() {
        let program = crate::pipeline::parse_and_lower(
            r#"
            function begin_tail_8761(c::Bool)
                begin
                    if c
                        return 0
                    end
                end
                "s"
            end
            "#,
        )
        .expect("source should lower");

        let func = program
            .functions
            .iter()
            .find(|func| func.name == "begin_tail_8761")
            .expect("function should lower");
        assert_eq!(func.body.stmts.len(), 2);
        assert!(matches!(
            &func.body.stmts[0],
            Stmt::Expr {
                expr: Expr::LetBlock { bindings, .. },
                ..
            } if bindings.is_empty()
        ));
        assert!(matches!(
            &func.body.stmts[1],
            Stmt::Expr {
                expr: Expr::Literal(Literal::Str(s), _),
                ..
            } if s == "s"
        ));
    }

    #[test]
    fn conditional_export_collection_preserves_source_order_7959() {
        let program = crate::pipeline::parse_and_lower(
            r#"
            module SequentialExport7959
            if :x in names(SequentialExport7959)
                export y
            end
            export x
            x = 1
            y = 2
            end

            module PriorExport7959
            export x
            if :x in names(PriorExport7959)
                export y
            end
            x = 1
            y = 2
            end
            "#,
        )
        .expect("source should lower");

        let sequential = program
            .modules
            .iter()
            .find(|module| module.name == "SequentialExport7959")
            .expect("sequential module");
        assert!(sequential.exports.contains(&"x".to_string()));
        assert!(!sequential.exports.contains(&"y".to_string()));

        let prior = program
            .modules
            .iter()
            .find(|module| module.name == "PriorExport7959")
            .expect("prior module");
        assert!(prior.exports.contains(&"x".to_string()));
        assert!(prior.exports.contains(&"y".to_string()));
    }

    /// Issue #10164: `pipeline::parse_source` drives the plain, non-`include()`
    /// `Lowering::lower_source_file_inner` path (the Base/prelude lowering
    /// entry point, distinct from `parse_source_with_include`'s
    /// `LoweringWithInclude` used for ordinary user programs). It used to
    /// never populate `pending_doc`, so a top-level docstring preceding a
    /// definition was always dropped instead of becoming a
    /// `__sjulia_doc_<Name>` registration — silently discarding every Base
    /// docstring (`Val`, `Exception`, `BoundsError`, etc.). Cover both a
    /// docstring before a function definition and one before a top-level
    /// `const` statement (the latter needed its own fix: the `ConstStatement`
    /// arm never called `push_doc_registration`, so a preceding docstring
    /// leaked past the const to whatever later definition consumed
    /// `pending_doc` next instead of being dropped or attached correctly).
    #[test]
    fn lower_source_file_captures_top_level_docstring_10164() {
        let program = crate::pipeline::parse_source(
            r#"
"""
doc for foo
"""
function foo(x)
    x + 1
end

"""
doc for MY_CONST
"""
const MY_CONST = 42
"#,
        )
        .expect("source with docstrings should lower");

        let doc_names: Vec<&str> = program
            .main
            .stmts
            .iter()
            .filter_map(|s| match s {
                Stmt::Assign { var, .. } if var.starts_with("__sjulia_doc_") => Some(var.as_str()),
                _ => None,
            })
            .collect();

        assert!(
            doc_names.contains(&"__sjulia_doc_foo"),
            "docstring preceding a function definition should register \
             __sjulia_doc_foo via the plain `Lowering` path (Issue #10164), \
             got: {doc_names:?}"
        );
        assert!(
            doc_names.contains(&"__sjulia_doc_MY_CONST"),
            "docstring preceding a top-level const statement should register \
             __sjulia_doc_MY_CONST (Issue #10164), got: {doc_names:?}"
        );
    }

    /// Issue #10164 (safety net): `is_docstring_target_kind` treats a plain
    /// `Assignment` as a valid docstring target, but the arm that actually
    /// handles a non-`const`, non-short-function, non-type-alias, non-lambda
    /// assignment (the `_ => { .. }` catch-all) never calls
    /// `push_doc_registration`. Before the blanket
    /// `if is_docstring_target_kind(kind) { pending_doc = None; }` cleanup
    /// added alongside the `ConstStatement` fix, a docstring preceding such an
    /// assignment stayed pending and got misattributed to whatever later
    /// definition consumed `pending_doc` next — the exact same class of bug
    /// as the `VERSION` → `_findlast_char` cross-file misattribution the
    /// `ConstStatement` fix addressed, just for a different node kind. `bar`
    /// below has no docstring of its own, so it must end up with NO
    /// `__sjulia_doc_bar` entry rather than inheriting `some_global`'s.
    #[test]
    fn dangling_docstring_before_plain_assignment_does_not_leak_to_next_definition_10164() {
        let program = crate::pipeline::parse_source(
            r#"
"""
doc for a plain top-level assignment (not const, not a function) -- this must
be dropped, never misattributed to a later, undocumented definition
"""
some_global = 1

function bar(x)
    x + 1
end
"#,
        )
        .expect("source should lower");

        let doc_names: Vec<&str> = program
            .main
            .stmts
            .iter()
            .filter_map(|s| match s {
                Stmt::Assign { var, .. } if var.starts_with("__sjulia_doc_") => Some(var.as_str()),
                _ => None,
            })
            .collect();

        assert!(
            doc_names.is_empty(),
            "docstring preceding a plain top-level assignment must be dropped, \
             not misattributed to a later, undocumented definition (Issue \
             #10164 safety net); got: {doc_names:?}"
        );
    }
}
