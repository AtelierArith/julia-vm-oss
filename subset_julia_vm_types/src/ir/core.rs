use crate::types::{JuliaType, TypeExpr, TypeParam};
use half::f16;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use subset_julia_vm_ir::Span;
// Re-exported so downstream crates can `use crate::ir::core::InternedStr`
// alongside `Expr` (Issue #10124).
pub use subset_julia_vm_ir::InternedStr;

pub const BASE_USER_MAIN_BOUNDARY_META: &str = "__sjulia_base_user_main_boundary";

/// Reserved provenance marker for callable functions synthesized by lowering.
///
/// Source definitions use monotonically increasing ordinals (or zero for
/// legacy/unstamped IR). Keeping the marker outside that range lets compiler
/// stages distinguish private lowering callables without relying on generated
/// spellings, while preserving the existing serialized `Span` representation.
pub const LOWERING_HELPER_DEFINITION_ORDER: u64 = u64::MAX;

pub fn is_source_definition_order(order: u64) -> bool {
    order != 0 && order != LOWERING_HELPER_DEFINITION_ORDER
}

const TUPLE_COMPREHENSION_BINDING_PREFIX: &str = "__tuple_comprehension_binding__:";

pub fn encode_tuple_comprehension_binding(vars: &[String]) -> String {
    format!("{}{}", TUPLE_COMPREHENSION_BINDING_PREFIX, vars.join(","))
}

pub fn decode_tuple_comprehension_binding(var: &str) -> Option<Vec<String>> {
    var.strip_prefix(TUPLE_COMPREHENSION_BINDING_PREFIX)
        .map(|encoded| {
            encoded
                .split(',')
                .filter(|name| !name.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
}

/// Using/import statement representation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsingImport {
    pub module: String,
    /// `true` for `import`, `false` for `using`. A plain `import M` binds only
    /// `M`; a plain `using M` additionally brings M's exports into scope.
    #[serde(default)]
    pub is_import: bool,
    /// If None, import all exported functions (`using Module`).
    /// If Some, import only these specific functions (`using Module: func1, func2`).
    pub symbols: Option<Vec<String>>,
    /// If true, this is a relative import (`using .Module`).
    /// Relative imports refer to user-defined modules in the current program,
    /// not stdlib or external packages.
    #[serde(default)]
    pub is_relative: bool,
    /// Number of leading dots in a relative import. `using .M` has level 1,
    /// `using ..M` has level 2. Non-relative imports keep this at 0.
    #[serde(default)]
    pub relative_level: usize,
    /// Renaming aliases introduced by `... as ...` (Issue #8117). Each entry is
    /// `(source_dotted_path, target_name)` and is realized as a runtime binding
    /// `target_name = source_dotted_path` so the alias name resolves to the
    /// imported entity. A whole-module `import M as N` yields `("M", "N")`; a
    /// symbol alias `using M: f as g` yields `("M.f", "g")`. Aliased symbols are
    /// kept out of [`symbols`] (only the renamed name is bound, mirroring Julia
    /// where the original name stays unbound).
    #[serde(default)]
    pub alias_bindings: Vec<(String, String)>,
    pub span: Span,
}

/// Core IR - minimal representation of Julia subset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Program {
    #[serde(default)]
    pub abstract_types: Vec<AbstractTypeDef>,
    /// User-declared primitive type definitions (`primitive type Name Bits end`)
    #[serde(default)]
    pub primitive_types: Vec<PrimitiveTypeDef>,
    /// Type alias definitions (const TypeName = TypeExpr)
    #[serde(default)]
    pub type_aliases: Vec<TypeAliasDef>,
    pub structs: Vec<StructDef>,
    /// Function definitions, Arc-wrapped so `merge_prelude_into_user_program`
    /// (Issue #9140) can share the ~5000 prelude `Function`s across every
    /// compile via a cheap refcount-bump `.cloned()` instead of deep-cloning
    /// each function's IR body. Serializes identically to `Vec<Function>`
    /// under serde's `rc` feature, so cached-program wire format is unchanged.
    pub functions: Vec<Arc<Function>>,
    /// Number of base functions (from prelude). Functions at index >= base_function_count are user functions.
    #[serde(default)]
    pub base_function_count: usize,
    pub modules: Vec<Module>,
    /// Using/import statements
    pub usings: Vec<UsingImport>,
    /// Macro definitions
    #[serde(default)]
    pub macros: Vec<MacroDef>,
    /// Enum definitions
    #[serde(default)]
    pub enums: Vec<EnumDef>,
    pub main: Block,
}

impl Program {
    /// Lowest and highest lowering-assigned definition ordinals in this
    /// program, including nested modules. `None` means no definition carries
    /// ordinal metadata.
    pub fn definition_order_bounds(&self) -> Option<(u64, u64)> {
        self.functions
            .iter()
            .map(|function| function.span.definition_order)
            .chain(
                self.abstract_types
                    .iter()
                    .map(|definition| definition.span.definition_order),
            )
            .chain(
                self.primitive_types
                    .iter()
                    .map(|definition| definition.span.definition_order),
            )
            .chain(
                self.type_aliases
                    .iter()
                    .map(|definition| definition.span.definition_order),
            )
            .chain(
                self.structs
                    .iter()
                    .map(|definition| definition.span.definition_order),
            )
            .chain(self.modules.iter().flat_map(|module| {
                module
                    .definition_order_bounds()
                    .into_iter()
                    .flat_map(|(min, max)| [min, max])
            }))
            .chain(self.usings.iter().map(|using| using.span.definition_order))
            .chain(
                self.macros
                    .iter()
                    .map(|definition| definition.span.definition_order),
            )
            .chain(
                self.main
                    .definition_order_bounds()
                    .into_iter()
                    .flat_map(|(min, max)| [min, max]),
            )
            .chain(
                self.enums
                    .iter()
                    .map(|definition| definition.span.definition_order),
            )
            .filter(|order| is_source_definition_order(*order))
            .fold(None, |bounds, order| match bounds {
                Some((min, max)) => Some((min.min(order), max.max(order))),
                None => Some((order, order)),
            })
    }

    /// Rebase source definitions so a separately lowered fragment can be
    /// appended after an existing program without losing Julia evaluation
    /// order. Definitions without ordinal metadata remain zero.
    fn shift_definition_orders(&mut self, offset: u64) {
        if offset == 0 {
            return;
        }
        for function in &mut self.functions {
            let function = Arc::make_mut(function);
            if is_source_definition_order(function.span.definition_order) {
                function.span.definition_order =
                    function.span.definition_order.saturating_add(offset);
            }
            function.body.shift_definition_orders(offset);
        }
        for definition in &mut self.structs {
            if definition.span.definition_order != 0 {
                definition.span.definition_order =
                    definition.span.definition_order.saturating_add(offset);
            }
            for constructor in &mut definition.inner_constructors {
                if constructor.span.definition_order != 0 {
                    constructor.span.definition_order =
                        constructor.span.definition_order.saturating_add(offset);
                }
                constructor.body.shift_definition_orders(offset);
            }
        }
        for definition in &mut self.abstract_types {
            if definition.span.definition_order != 0 {
                definition.span.definition_order =
                    definition.span.definition_order.saturating_add(offset);
            }
        }
        for definition in &mut self.primitive_types {
            if definition.span.definition_order != 0 {
                definition.span.definition_order =
                    definition.span.definition_order.saturating_add(offset);
            }
        }
        for definition in &mut self.type_aliases {
            if definition.span.definition_order != 0 {
                definition.span.definition_order =
                    definition.span.definition_order.saturating_add(offset);
            }
        }
        for module in &mut self.modules {
            module.shift_definition_orders(offset);
        }
        for using in &mut self.usings {
            if using.span.definition_order != 0 {
                using.span.definition_order = using.span.definition_order.saturating_add(offset);
            }
        }
        for macro_def in &mut self.macros {
            if macro_def.span.definition_order != 0 {
                macro_def.span.definition_order =
                    macro_def.span.definition_order.saturating_add(offset);
            }
            macro_def.body.shift_definition_orders(offset);
        }
        for enum_def in &mut self.enums {
            if enum_def.span.definition_order != 0 {
                enum_def.span.definition_order =
                    enum_def.span.definition_order.saturating_add(offset);
            }
        }
        self.main.shift_definition_orders(offset);
    }

    fn shift_definition_orders_after(&mut self, anchor: u64, offset: u64) {
        if offset == 0 {
            return;
        }
        for function in &mut self.functions {
            let function = Arc::make_mut(function);
            if is_source_definition_order(function.span.definition_order)
                && function.span.definition_order > anchor
            {
                function.span.definition_order =
                    function.span.definition_order.saturating_add(offset);
            }
            function.body.shift_definition_orders_after(anchor, offset);
        }
        for definition in &mut self.structs {
            if definition.span.definition_order > anchor {
                definition.span.definition_order =
                    definition.span.definition_order.saturating_add(offset);
            }
            for constructor in &mut definition.inner_constructors {
                if constructor.span.definition_order > anchor {
                    constructor.span.definition_order =
                        constructor.span.definition_order.saturating_add(offset);
                }
                constructor
                    .body
                    .shift_definition_orders_after(anchor, offset);
            }
        }
        for definition in &mut self.abstract_types {
            if definition.span.definition_order > anchor {
                definition.span.definition_order =
                    definition.span.definition_order.saturating_add(offset);
            }
        }
        for definition in &mut self.primitive_types {
            if definition.span.definition_order > anchor {
                definition.span.definition_order =
                    definition.span.definition_order.saturating_add(offset);
            }
        }
        for definition in &mut self.type_aliases {
            if definition.span.definition_order > anchor {
                definition.span.definition_order =
                    definition.span.definition_order.saturating_add(offset);
            }
        }
        for module in &mut self.modules {
            module.shift_definition_orders_after(anchor, offset);
        }
        for using in &mut self.usings {
            if using.span.definition_order > anchor {
                using.span.definition_order = using.span.definition_order.saturating_add(offset);
            }
        }
        for macro_def in &mut self.macros {
            if macro_def.span.definition_order > anchor {
                macro_def.span.definition_order =
                    macro_def.span.definition_order.saturating_add(offset);
            }
            macro_def.body.shift_definition_orders_after(anchor, offset);
        }
        for enum_def in &mut self.enums {
            if enum_def.span.definition_order > anchor {
                enum_def.span.definition_order =
                    enum_def.span.definition_order.saturating_add(offset);
            }
        }
        self.main.shift_definition_orders_after(anchor, offset);
    }

    /// Start a chronology cursor after every definition already in this
    /// program. Use the cursor's [`DefinitionOrderCursor::append_fragment`]
    /// before transferring any independently lowered `Program` or `Module`
    /// definitions into this program.
    pub fn definition_order_cursor(&self) -> DefinitionOrderCursor {
        DefinitionOrderCursor::after_program(self)
    }
}

/// Cumulative evaluation chronology for independently lowered IR fragments.
///
/// Lowering assigns definition ordinals from one inside each context. Any
/// fragment produced by a different context must pass through
/// [`Self::append_fragment`] before its definitions are merged or replayed.
/// The operation recursively rebases nested modules and advances the cursor,
/// so consecutive fragments occupy disjoint, increasing ordinal ranges.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DefinitionOrderCursor {
    max_definition_order: u64,
}

impl DefinitionOrderCursor {
    pub fn after_program(program: &Program) -> Self {
        Self {
            max_definition_order: program.definition_order_bounds().map_or(0, |(_, max)| max),
        }
    }

    /// Highest definition ordinal already covered by this cursor.
    pub fn max_definition_order(self) -> u64 {
        self.max_definition_order
    }

    /// Build a cursor from definitions retained outside a `Program`, as in a
    /// persistent REPL session between independently lowered evaluations.
    pub fn after_stored_definitions(
        functions: &[Function],
        structs: &[StructDef],
        abstract_types: &[AbstractTypeDef],
        primitive_types: &[PrimitiveTypeDef],
        type_aliases: &[TypeAliasDef],
        modules: &[Module],
        macros: &[MacroDef],
        enums: &[EnumDef],
    ) -> Self {
        let max_definition_order = functions
            .iter()
            .map(|function| function.span.definition_order)
            .chain(
                structs
                    .iter()
                    .map(|definition| definition.span.definition_order),
            )
            .chain(
                abstract_types
                    .iter()
                    .map(|definition| definition.span.definition_order),
            )
            .chain(
                primitive_types
                    .iter()
                    .map(|definition| definition.span.definition_order),
            )
            .chain(
                type_aliases
                    .iter()
                    .map(|definition| definition.span.definition_order),
            )
            .chain(
                modules
                    .iter()
                    .filter_map(|module| module.definition_order_bounds().map(|(_, max)| max)),
            )
            .chain(
                macros
                    .iter()
                    .map(|definition| definition.span.definition_order),
            )
            .chain(
                enums
                    .iter()
                    .map(|definition| definition.span.definition_order),
            )
            .filter(|order| is_source_definition_order(*order))
            .max()
            .unwrap_or(0);
        Self {
            max_definition_order,
        }
    }

    /// Place one independently lowered fragment after all fragments previously
    /// observed by this cursor.
    pub fn append_fragment<'a>(&mut self, fragment: impl Into<DefinitionOrderFragment<'a>>) {
        let prior_max = self.max_definition_order;
        let bounds = match fragment.into() {
            DefinitionOrderFragment::Program(fragment) => {
                fragment.shift_definition_orders(prior_max);
                fragment.definition_order_bounds()
            }
            DefinitionOrderFragment::Module(fragment) => {
                fragment.shift_definition_orders(prior_max);
                fragment.definition_order_bounds()
            }
        };

        let Some((min, max)) = bounds else {
            return;
        };
        debug_assert!(
            min > prior_max,
            "independently lowered definition ordinals must follow prior chronology: \
             fragment minimum {min}, prior maximum {prior_max}"
        );
        debug_assert!(
            max >= min,
            "definition-order bounds must be ordered: minimum {min}, maximum {max}"
        );
        self.max_definition_order = max;
    }

    /// Insert an independently lowered fragment immediately after a stamped
    /// `using`/`import` event inside `program`.
    ///
    /// Definitions later than `anchor` move forward by the fragment's local
    /// ordinal width. The returned ordinal is the inserted fragment's end and
    /// can be used as the next anchor when a package brings dependencies that
    /// must be inserted in loader order at the same source event.
    pub fn insert_fragment_after<'a>(
        &mut self,
        program: &mut Program,
        anchor: u64,
        fragment: impl Into<DefinitionOrderFragment<'a>>,
    ) -> u64 {
        let fragment = fragment.into();
        let bounds = match &fragment {
            DefinitionOrderFragment::Program(fragment) => fragment.definition_order_bounds(),
            DefinitionOrderFragment::Module(fragment) => fragment.definition_order_bounds(),
        };
        let Some((_, local_max)) = bounds else {
            return anchor;
        };
        debug_assert!(
            anchor <= self.max_definition_order,
            "definition-order insertion anchor {anchor} exceeds chronology maximum {}",
            self.max_definition_order
        );

        program.shift_definition_orders_after(anchor, local_max);
        let inserted_bounds = match fragment {
            DefinitionOrderFragment::Program(fragment) => {
                fragment.shift_definition_orders(anchor);
                fragment.definition_order_bounds()
            }
            DefinitionOrderFragment::Module(fragment) => {
                fragment.shift_definition_orders(anchor);
                fragment.definition_order_bounds()
            }
        };
        let Some((inserted_min, inserted_max)) = inserted_bounds else {
            debug_assert!(
                false,
                "nonempty definition-order bounds disappeared during rebasing"
            );
            return anchor;
        };
        debug_assert!(
            inserted_min > anchor,
            "inserted definitions must follow using/import anchor {anchor}, got {inserted_min}"
        );
        self.max_definition_order = self.max_definition_order.saturating_add(local_max);
        inserted_max
    }

    #[cfg(test)]
    fn current(self) -> u64 {
        self.max_definition_order
    }
}

/// A definition-bearing lowered fragment accepted by the chronology cursor.
#[derive(Debug)]
#[doc(hidden)]
pub enum DefinitionOrderFragment<'a> {
    Program(&'a mut Program),
    Module(&'a mut Module),
}

impl<'a> From<&'a mut Program> for DefinitionOrderFragment<'a> {
    fn from(program: &'a mut Program) -> Self {
        Self::Program(program)
    }
}

impl<'a> From<&'a mut Module> for DefinitionOrderFragment<'a> {
    fn from(module: &'a mut Module) -> Self {
        Self::Module(module)
    }
}

/// Module definition: `module Name ... end` or `baremodule Name ... end`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Module {
    pub name: String,
    /// Whether this is a baremodule (no automatic Base import)
    #[serde(default)]
    pub is_bare: bool,
    /// Whether this module tree was loaded from an external/bundled package
    /// rather than declared in the current source input. Package definition
    /// ordinals live in an independently inserted chronology and must not be
    /// compared with user call-site offsets (Issue #11716).
    #[serde(default)]
    pub is_package_origin: bool,
    /// True when this module was merged from the bundled Base/prelude or
    /// loaded through the stdlib lane rather than declared by current source.
    #[serde(default)]
    pub is_base_origin: bool,
    pub functions: Vec<Function>,
    /// Struct definitions within this module
    #[serde(default)]
    pub structs: Vec<StructDef>,
    /// Abstract type definitions within this module
    #[serde(default)]
    pub abstract_types: Vec<AbstractTypeDef>,
    /// Primitive type definitions within this module
    #[serde(default)]
    pub primitive_types: Vec<PrimitiveTypeDef>,
    /// Type alias definitions within this module
    #[serde(default)]
    pub type_aliases: Vec<TypeAliasDef>,
    /// Nested submodules
    pub submodules: Vec<Module>,
    /// Using/import statements within this module
    #[serde(default)]
    pub usings: Vec<UsingImport>,
    /// Macro definitions within this module
    #[serde(default)]
    pub macros: Vec<MacroDef>,
    /// Exported names (functions, structs, abstract types)
    pub exports: Vec<String>,
    /// Public names (Julia 1.11+): part of public API but not automatically exported
    #[serde(default)]
    pub publics: Vec<String>,
    pub body: Block,
    pub span: Span,
}

impl Module {
    pub fn mark_as_package_origin(&mut self) {
        self.is_package_origin = true;
        for submodule in &mut self.submodules {
            submodule.mark_as_package_origin();
        }
    }

    fn definition_order_bounds(&self) -> Option<(u64, u64)> {
        self.functions
            .iter()
            .map(|function| function.span.definition_order)
            .chain(
                self.abstract_types
                    .iter()
                    .map(|definition| definition.span.definition_order),
            )
            .chain(
                self.primitive_types
                    .iter()
                    .map(|definition| definition.span.definition_order),
            )
            .chain(
                self.type_aliases
                    .iter()
                    .map(|definition| definition.span.definition_order),
            )
            .chain(
                self.structs
                    .iter()
                    .map(|definition| definition.span.definition_order),
            )
            .chain(self.submodules.iter().flat_map(|module| {
                module
                    .definition_order_bounds()
                    .into_iter()
                    .flat_map(|(min, max)| [min, max])
            }))
            .chain(self.usings.iter().map(|using| using.span.definition_order))
            .chain(
                self.macros
                    .iter()
                    .map(|definition| definition.span.definition_order),
            )
            .chain(
                self.body
                    .definition_order_bounds()
                    .into_iter()
                    .flat_map(|(min, max)| [min, max]),
            )
            .filter(|order| is_source_definition_order(*order))
            .fold(None, |bounds, order| match bounds {
                Some((min, max)) => Some((min.min(order), max.max(order))),
                None => Some((order, order)),
            })
    }

    fn shift_definition_orders(&mut self, offset: u64) {
        for function in &mut self.functions {
            if is_source_definition_order(function.span.definition_order) {
                function.span.definition_order =
                    function.span.definition_order.saturating_add(offset);
            }
            function.body.shift_definition_orders(offset);
        }
        for definition in &mut self.structs {
            if definition.span.definition_order != 0 {
                definition.span.definition_order =
                    definition.span.definition_order.saturating_add(offset);
            }
            for constructor in &mut definition.inner_constructors {
                if constructor.span.definition_order != 0 {
                    constructor.span.definition_order =
                        constructor.span.definition_order.saturating_add(offset);
                }
                constructor.body.shift_definition_orders(offset);
            }
        }
        for definition in &mut self.abstract_types {
            if definition.span.definition_order != 0 {
                definition.span.definition_order =
                    definition.span.definition_order.saturating_add(offset);
            }
        }
        for definition in &mut self.primitive_types {
            if definition.span.definition_order != 0 {
                definition.span.definition_order =
                    definition.span.definition_order.saturating_add(offset);
            }
        }
        for definition in &mut self.type_aliases {
            if definition.span.definition_order != 0 {
                definition.span.definition_order =
                    definition.span.definition_order.saturating_add(offset);
            }
        }
        for module in &mut self.submodules {
            module.shift_definition_orders(offset);
        }
        for using in &mut self.usings {
            if using.span.definition_order != 0 {
                using.span.definition_order = using.span.definition_order.saturating_add(offset);
            }
        }
        for macro_def in &mut self.macros {
            if macro_def.span.definition_order != 0 {
                macro_def.span.definition_order =
                    macro_def.span.definition_order.saturating_add(offset);
            }
            macro_def.body.shift_definition_orders(offset);
        }
        self.body.shift_definition_orders(offset);
    }

    fn shift_definition_orders_after(&mut self, anchor: u64, offset: u64) {
        for function in &mut self.functions {
            if is_source_definition_order(function.span.definition_order)
                && function.span.definition_order > anchor
            {
                function.span.definition_order =
                    function.span.definition_order.saturating_add(offset);
            }
            function.body.shift_definition_orders_after(anchor, offset);
        }
        for definition in &mut self.structs {
            if definition.span.definition_order > anchor {
                definition.span.definition_order =
                    definition.span.definition_order.saturating_add(offset);
            }
            for constructor in &mut definition.inner_constructors {
                if constructor.span.definition_order > anchor {
                    constructor.span.definition_order =
                        constructor.span.definition_order.saturating_add(offset);
                }
                constructor
                    .body
                    .shift_definition_orders_after(anchor, offset);
            }
        }
        for definition in &mut self.abstract_types {
            if definition.span.definition_order > anchor {
                definition.span.definition_order =
                    definition.span.definition_order.saturating_add(offset);
            }
        }
        for definition in &mut self.primitive_types {
            if definition.span.definition_order > anchor {
                definition.span.definition_order =
                    definition.span.definition_order.saturating_add(offset);
            }
        }
        for definition in &mut self.type_aliases {
            if definition.span.definition_order > anchor {
                definition.span.definition_order =
                    definition.span.definition_order.saturating_add(offset);
            }
        }
        for module in &mut self.submodules {
            module.shift_definition_orders_after(anchor, offset);
        }
        for using in &mut self.usings {
            if using.span.definition_order > anchor {
                using.span.definition_order = using.span.definition_order.saturating_add(offset);
            }
        }
        for macro_def in &mut self.macros {
            if macro_def.span.definition_order > anchor {
                macro_def.span.definition_order =
                    macro_def.span.definition_order.saturating_add(offset);
            }
            macro_def.body.shift_definition_orders_after(anchor, offset);
        }
        self.body.shift_definition_orders_after(anchor, offset);
    }
}

impl Block {
    fn definition_order_bounds(&self) -> Option<(u64, u64)> {
        // Keep bounds exactly symmetric with the mutation/rebasing visitor.
        // Chronology queries are confined to fragment assembly, where cloning
        // the executable block is preferable to maintaining a second recursive
        // statement/expression walker that can silently miss a new IR variant.
        let mut block = self.clone();
        let mut bounds: Option<(u64, u64)> = None;
        block.visit_definition_orders_mut(&mut |order| {
            if !is_source_definition_order(*order) {
                return;
            }
            bounds = Some(match bounds {
                Some((min, max)) => (min.min(*order), max.max(*order)),
                None => (*order, *order),
            });
        });
        bounds
    }

    fn shift_definition_orders(&mut self, offset: u64) {
        if offset == 0 {
            return;
        }
        self.visit_definition_orders_mut(&mut |order| {
            if is_source_definition_order(*order) {
                *order = order.saturating_add(offset);
            }
        });
    }

    fn shift_definition_orders_after(&mut self, anchor: u64, offset: u64) {
        if offset == 0 {
            return;
        }
        self.visit_definition_orders_mut(&mut |order| {
            if is_source_definition_order(*order) && *order > anchor {
                *order = order.saturating_add(offset);
            }
        });
    }

    fn visit_definition_orders_mut(&mut self, visit: &mut impl FnMut(&mut u64)) {
        for stmt in &mut self.stmts {
            stmt.visit_definition_orders_mut(visit);
        }
    }
}

impl Stmt {
    fn visit_definition_orders_mut(&mut self, visit: &mut impl FnMut(&mut u64)) {
        match self {
            Self::Block(block) => block.visit_definition_orders_mut(visit),
            Self::For {
                start,
                end,
                step,
                body,
                ..
            } => {
                start.visit_definition_orders_mut(visit);
                end.visit_definition_orders_mut(visit);
                if let Some(step) = step {
                    step.visit_definition_orders_mut(visit);
                }
                body.visit_definition_orders_mut(visit);
            }
            Self::ForEach { iterable, body, .. } | Self::ForEachTuple { iterable, body, .. } => {
                iterable.visit_definition_orders_mut(visit);
                body.visit_definition_orders_mut(visit);
            }
            Self::While {
                condition, body, ..
            } => {
                condition.visit_definition_orders_mut(visit);
                body.visit_definition_orders_mut(visit);
            }
            Self::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                condition.visit_definition_orders_mut(visit);
                then_branch.visit_definition_orders_mut(visit);
                if let Some(else_branch) = else_branch {
                    else_branch.visit_definition_orders_mut(visit);
                }
            }
            Self::Try {
                try_block,
                catch_block,
                else_block,
                finally_block,
                ..
            } => {
                try_block.visit_definition_orders_mut(visit);
                for block in [catch_block, else_block, finally_block]
                    .into_iter()
                    .flatten()
                {
                    block.visit_definition_orders_mut(visit);
                }
            }
            Self::Timed { body, .. } | Self::TestSet { body, .. } => {
                body.visit_definition_orders_mut(visit);
            }
            Self::FunctionDef { func, span } | Self::EvalFunctionDef { func, span } => {
                visit(&mut span.definition_order);
                visit(&mut func.span.definition_order);
                func.body.visit_definition_orders_mut(visit);
            }
            Self::Assign { value, .. }
            | Self::AddAssign { value, .. }
            | Self::FieldAssign { value, .. }
            | Self::DestructuringAssign { value, .. } => value.visit_definition_orders_mut(visit),
            Self::Return { value, .. } => {
                if let Some(value) = value {
                    value.visit_definition_orders_mut(visit);
                }
            }
            Self::Expr { expr, .. } => expr.visit_definition_orders_mut(visit),
            Self::Test { condition, .. } => condition.visit_definition_orders_mut(visit),
            Self::TestThrows { expr, .. } => expr.visit_definition_orders_mut(visit),
            Self::IndexAssign { indices, value, .. } => {
                for expr in indices {
                    expr.visit_definition_orders_mut(visit);
                }
                value.visit_definition_orders_mut(visit);
            }
            Self::DictAssign { key, value, .. } => {
                key.visit_definition_orders_mut(visit);
                value.visit_definition_orders_mut(visit);
            }
            Self::Using { span, .. } => visit(&mut span.definition_order),
            Self::EnumDef { enum_def, span, .. } => {
                visit(&mut span.definition_order);
                visit(&mut enum_def.span.definition_order);
            }
            Self::RuntimeNominalDef {
                definition, span, ..
            } => {
                visit(&mut span.definition_order);
                match definition {
                    RuntimeNominalDef::Struct(definition) => {
                        visit(&mut definition.span.definition_order);
                        for constructor in &mut definition.inner_constructors {
                            visit(&mut constructor.span.definition_order);
                            constructor.body.visit_definition_orders_mut(visit);
                        }
                        for helper in &mut definition.global_new_helpers {
                            visit(&mut helper.span.definition_order);
                            helper.body.visit_definition_orders_mut(visit);
                        }
                    }
                    RuntimeNominalDef::AbstractType(definition) => {
                        visit(&mut definition.span.definition_order);
                    }
                    RuntimeNominalDef::PrimitiveType(definition) => {
                        visit(&mut definition.span.definition_order);
                    }
                    RuntimeNominalDef::Enum(definition) => {
                        visit(&mut definition.span.definition_order);
                    }
                }
            }
            Self::Meta { .. }
            | Self::Break { .. }
            | Self::Continue { .. }
            | Self::Export { .. }
            | Self::Label { .. }
            | Self::Goto { .. }
            | Self::Global { .. }
            | Self::LocalDecl { .. } => {}
        }
    }
}

impl Expr {
    fn visit_definition_orders_mut(&mut self, visit: &mut impl FnMut(&mut u64)) {
        match self {
            Self::BinaryOp { left, right, .. }
            | Self::Pair {
                key: left,
                value: right,
                ..
            } => {
                left.visit_definition_orders_mut(visit);
                right.visit_definition_orders_mut(visit);
            }
            Self::UnaryOp { operand, .. }
            | Self::FieldAccess {
                object: operand, ..
            }
            | Self::QuoteLiteral {
                constructor: operand,
                ..
            }
            | Self::AssignExpr { value: operand, .. }
            | Self::Convert { operand, .. } => operand.visit_definition_orders_mut(visit),
            Self::Call { args, kwargs, .. } | Self::ModuleCall { args, kwargs, .. } => {
                for expr in args {
                    expr.visit_definition_orders_mut(visit);
                }
                for (_, expr) in kwargs {
                    expr.visit_definition_orders_mut(visit);
                }
            }
            Self::Builtin { args, .. }
            | Self::ArrayLiteral { elements: args, .. }
            | Self::TupleLiteral { elements: args, .. }
            | Self::StringConcat { parts: args, .. }
            | Self::New { args, .. } => {
                for expr in args {
                    expr.visit_definition_orders_mut(visit);
                }
            }
            Self::Index { array, indices, .. } => {
                array.visit_definition_orders_mut(visit);
                for expr in indices {
                    expr.visit_definition_orders_mut(visit);
                }
            }
            Self::Range {
                start, step, stop, ..
            } => {
                start.visit_definition_orders_mut(visit);
                if let Some(step) = step {
                    step.visit_definition_orders_mut(visit);
                }
                stop.visit_definition_orders_mut(visit);
            }
            Self::Comprehension {
                body, iter, filter, ..
            }
            | Self::Generator {
                body, iter, filter, ..
            } => {
                body.visit_definition_orders_mut(visit);
                iter.visit_definition_orders_mut(visit);
                if let Some(filter) = filter {
                    filter.visit_definition_orders_mut(visit);
                }
            }
            Self::MultiComprehension {
                body,
                iterations,
                filter,
                ..
            } => {
                body.visit_definition_orders_mut(visit);
                for (_, expr) in iterations {
                    expr.visit_definition_orders_mut(visit);
                }
                if let Some(filter) = filter {
                    filter.visit_definition_orders_mut(visit);
                }
            }
            Self::NamedTupleLiteral { fields, .. } => {
                for (_, expr) in fields {
                    expr.visit_definition_orders_mut(visit);
                }
            }
            Self::DictLiteral { pairs, .. } => {
                for (key, value) in pairs {
                    key.visit_definition_orders_mut(visit);
                    value.visit_definition_orders_mut(visit);
                }
            }
            Self::LetBlock { bindings, body, .. } => {
                for (_, expr) in bindings {
                    expr.visit_definition_orders_mut(visit);
                }
                body.visit_definition_orders_mut(visit);
            }
            Self::Ternary {
                condition,
                then_expr,
                else_expr,
                ..
            } => {
                condition.visit_definition_orders_mut(visit);
                then_expr.visit_definition_orders_mut(visit);
                else_expr.visit_definition_orders_mut(visit);
            }
            Self::DynamicTypeConstruct {
                base_expr,
                type_args,
                ..
            } => {
                if let Some(base_expr) = base_expr {
                    base_expr.visit_definition_orders_mut(visit);
                }
                for expr in type_args {
                    expr.visit_definition_orders_mut(visit);
                }
            }
            Self::ReturnExpr { value, .. } => {
                if let Some(value) = value {
                    value.visit_definition_orders_mut(visit);
                }
            }
            Self::Literal(..)
            | Self::Var(..)
            | Self::TypedEmptyArray { .. }
            | Self::SliceAll { .. }
            | Self::FunctionRef { .. }
            | Self::BreakExpr { .. }
            | Self::ContinueExpr { .. } => {}
        }
    }
}

#[cfg(test)]
mod definition_order_tests {
    use super::*;

    fn span_with_order(definition_order: u64) -> Span {
        let mut span = Span::new(0, 0, 1, 1, 1, 1);
        span.definition_order = definition_order;
        span
    }

    fn block() -> Block {
        Block {
            stmts: Vec::new(),
            span: span_with_order(0),
        }
    }

    fn module_with_local_struct(name: &str, definition_order: u64) -> Module {
        Module {
            name: name.to_string(),
            is_bare: false,
            is_package_origin: false,
            is_base_origin: false,
            functions: Vec::new(),
            structs: vec![StructDef {
                name: format!("{name}Type"),
                is_mutable: false,
                type_params: Vec::new(),
                parent_type: None,
                fields: Vec::new(),
                inner_constructors: Vec::new(),
                is_base_origin: false,
                span: span_with_order(definition_order),
                global_new_helpers: Vec::new(),
            }],
            abstract_types: Vec::new(),
            primitive_types: Vec::new(),
            type_aliases: Vec::new(),
            submodules: Vec::new(),
            usings: Vec::new(),
            macros: Vec::new(),
            exports: Vec::new(),
            publics: Vec::new(),
            body: block(),
            span: span_with_order(0),
        }
    }

    fn struct_with_order(name: &str, definition_order: u64) -> StructDef {
        StructDef {
            name: name.to_string(),
            is_mutable: false,
            type_params: Vec::new(),
            parent_type: None,
            fields: Vec::new(),
            inner_constructors: Vec::new(),
            global_new_helpers: Vec::new(),
            is_base_origin: false,
            span: span_with_order(definition_order),
        }
    }

    fn function_with_order(name: &str, definition_order: u64) -> Function {
        Function {
            name: name.to_string(),
            params: Vec::new(),
            kwparams: Vec::new(),
            type_params: Vec::new(),
            return_type: None,
            body: block(),
            is_base_extension: false,
            is_runtime_eval: false,
            new_struct_name: None,
            span: span_with_order(definition_order),
        }
    }

    fn program() -> Program {
        Program {
            abstract_types: Vec::new(),
            primitive_types: Vec::new(),
            type_aliases: Vec::new(),
            structs: Vec::new(),
            functions: Vec::new(),
            base_function_count: 0,
            modules: Vec::new(),
            usings: Vec::new(),
            macros: Vec::new(),
            enums: Vec::new(),
            main: block(),
        }
    }

    #[test]
    fn separately_lowered_modules_are_rebased_cumulatively() {
        let mut program = program();
        let mut first = module_with_local_struct("First", 1);
        first
            .submodules
            .push(module_with_local_struct("FirstNested", 2));
        let mut second = module_with_local_struct("Second", 1);
        let mut chronology = program.definition_order_cursor();
        chronology.append_fragment(&mut first);
        chronology.append_fragment(&mut second);
        program.modules.push(first);
        program.modules.push(second);

        assert_eq!(program.modules[0].structs[0].span.definition_order, 1);
        assert_eq!(
            program.modules[0].submodules[0].structs[0]
                .span
                .definition_order,
            2
        );
        assert_eq!(program.modules[1].structs[0].span.definition_order, 3);
        assert_eq!(chronology.current(), 3);
    }

    #[test]
    fn all_program_definition_categories_are_rebased_and_bounded() {
        let mut target = program();
        target.structs.push(struct_with_order("Existing", 10));
        let mut fragment = program();
        fragment.abstract_types.push(AbstractTypeDef {
            name: "AbstractThing".to_string(),
            parent: None,
            type_params: Vec::new(),
            span: span_with_order(1),
        });
        fragment.primitive_types.push(PrimitiveTypeDef {
            name: "PrimitiveThing".to_string(),
            parent: None,
            bits: 8,
            span: span_with_order(2),
        });
        fragment.type_aliases.push(TypeAliasDef {
            name: "ThingAlias".to_string(),
            target_type: "AbstractThing".to_string(),
            params: Vec::new(),
            span: span_with_order(3),
        });
        fragment.macros.push(MacroDef {
            name: "thing".to_string(),
            params: Vec::new(),
            has_varargs: false,
            body: block(),
            span: span_with_order(4),
        });
        fragment.enums.push(EnumDef {
            name: "Things".to_string(),
            base_type: "Int32".to_string(),
            members: Vec::new(),
            span: span_with_order(5),
        });

        let mut chronology = target.definition_order_cursor();
        chronology.append_fragment(&mut fragment);

        assert_eq!(fragment.abstract_types[0].span.definition_order, 11);
        assert_eq!(fragment.primitive_types[0].span.definition_order, 12);
        assert_eq!(fragment.type_aliases[0].span.definition_order, 13);
        assert_eq!(fragment.macros[0].span.definition_order, 14);
        assert_eq!(fragment.enums[0].span.definition_order, 15);
        assert_eq!(chronology.current(), 15);
    }

    #[test]
    fn all_module_definition_categories_are_rebased_recursively() {
        let mut target = program();
        target.structs.push(struct_with_order("Existing", 10));
        let mut fragment = module_with_local_struct("Loaded", 1);
        fragment.abstract_types.push(AbstractTypeDef {
            name: "LoadedAbstract".to_string(),
            parent: None,
            type_params: Vec::new(),
            span: span_with_order(2),
        });
        fragment.primitive_types.push(PrimitiveTypeDef {
            name: "LoadedPrimitive".to_string(),
            parent: None,
            bits: 8,
            span: span_with_order(3),
        });
        fragment.type_aliases.push(TypeAliasDef {
            name: "LoadedAlias".to_string(),
            target_type: "LoadedAbstract".to_string(),
            params: Vec::new(),
            span: span_with_order(4),
        });
        fragment.macros.push(MacroDef {
            name: "loaded".to_string(),
            params: Vec::new(),
            has_varargs: false,
            body: block(),
            span: span_with_order(5),
        });
        fragment
            .submodules
            .push(module_with_local_struct("Nested", 6));

        let mut chronology = target.definition_order_cursor();
        chronology.append_fragment(&mut fragment);

        assert_eq!(fragment.structs[0].span.definition_order, 11);
        assert_eq!(fragment.abstract_types[0].span.definition_order, 12);
        assert_eq!(fragment.primitive_types[0].span.definition_order, 13);
        assert_eq!(fragment.type_aliases[0].span.definition_order, 14);
        assert_eq!(fragment.macros[0].span.definition_order, 15);
        assert_eq!(fragment.submodules[0].structs[0].span.definition_order, 16);
        assert_eq!(chronology.current(), 16);
    }

    #[test]
    fn stored_definition_cursor_includes_every_retained_category() {
        let functions = [function_with_order("f", 1)];
        let structs = [struct_with_order("S", 2)];
        let abstract_types = [AbstractTypeDef {
            name: "A".to_string(),
            parent: None,
            type_params: Vec::new(),
            span: span_with_order(3),
        }];
        let primitive_types = [PrimitiveTypeDef {
            name: "P".to_string(),
            parent: None,
            bits: 8,
            span: span_with_order(4),
        }];
        let type_aliases = [TypeAliasDef {
            name: "Alias".to_string(),
            target_type: "A".to_string(),
            params: Vec::new(),
            span: span_with_order(5),
        }];
        let macros = [MacroDef {
            name: "m".to_string(),
            params: Vec::new(),
            has_varargs: false,
            body: block(),
            span: span_with_order(6),
        }];
        let enums = [EnumDef {
            name: "E".to_string(),
            base_type: "Int32".to_string(),
            members: Vec::new(),
            span: span_with_order(7),
        }];

        let chronology = DefinitionOrderCursor::after_stored_definitions(
            &functions,
            &structs,
            &abstract_types,
            &primitive_types,
            &type_aliases,
            &[],
            &macros,
            &enums,
        );

        assert_eq!(chronology.current(), 7);
    }

    #[test]
    fn lowering_helper_provenance_is_not_rebased_or_counted_as_chronology_11685() {
        let mut target = program();
        target
            .functions
            .push(Arc::new(function_with_order("existing", 4)));
        target.functions.push(Arc::new(
            function_with_order("existing_helper", 0).into_lowering_helper(),
        ));

        let mut fragment = program();
        fragment
            .functions
            .push(Arc::new(function_with_order("source", 1)));
        fragment.functions.push(Arc::new(
            function_with_order("top_helper", 0).into_lowering_helper(),
        ));
        fragment.main.stmts.push(Stmt::FunctionDef {
            func: Box::new(function_with_order("nested_helper", 0).into_lowering_helper()),
            span: span_with_order(0),
        });

        let mut chronology = target.definition_order_cursor();
        assert_eq!(chronology.current(), 4);
        chronology.append_fragment(&mut fragment);

        assert_eq!(fragment.functions[0].span.definition_order, 5);
        assert!(fragment.functions[1].is_lowering_helper());
        let Stmt::FunctionDef { func, .. } = &fragment.main.stmts[0] else {
            panic!("expected nested helper")
        };
        assert!(func.is_lowering_helper());
        assert_eq!(chronology.current(), 5);
    }

    #[test]
    fn insertion_shifts_every_later_program_definition_category() {
        let mut target = program();
        target.usings.push(UsingImport {
            module: "Loaded".to_string(),
            symbols: None,
            is_import: false,
            is_relative: false,
            relative_level: 0,
            alias_bindings: Vec::new(),
            span: span_with_order(1),
        });
        target.abstract_types.push(AbstractTypeDef {
            name: "A".to_string(),
            parent: None,
            type_params: Vec::new(),
            span: span_with_order(2),
        });
        target.primitive_types.push(PrimitiveTypeDef {
            name: "P".to_string(),
            parent: None,
            bits: 8,
            span: span_with_order(3),
        });
        target.type_aliases.push(TypeAliasDef {
            name: "Alias".to_string(),
            target_type: "A".to_string(),
            params: Vec::new(),
            span: span_with_order(4),
        });
        target.macros.push(MacroDef {
            name: "m".to_string(),
            params: Vec::new(),
            has_varargs: false,
            body: block(),
            span: span_with_order(5),
        });
        target.enums.push(EnumDef {
            name: "E".to_string(),
            base_type: "Int32".to_string(),
            members: Vec::new(),
            span: span_with_order(6),
        });
        let mut loaded = program();
        loaded.structs.push(struct_with_order("Loaded", 1));

        let mut chronology = target.definition_order_cursor();
        let inserted_end = chronology.insert_fragment_after(&mut target, 1, &mut loaded);

        assert_eq!(inserted_end, 2);
        assert_eq!(target.abstract_types[0].span.definition_order, 3);
        assert_eq!(target.primitive_types[0].span.definition_order, 4);
        assert_eq!(target.type_aliases[0].span.definition_order, 5);
        assert_eq!(target.macros[0].span.definition_order, 6);
        assert_eq!(target.enums[0].span.definition_order, 7);
        assert_eq!(chronology.current(), 7);
    }

    #[test]
    fn module_body_function_copy_is_rebased_with_stored_definition() {
        let mut target = program();
        target.structs.push(struct_with_order("Existing", 10));
        let mut fragment = module_with_local_struct("Loaded", 1);
        let function = function_with_order("f", 2);
        fragment.functions.push(function.clone());
        fragment.body.stmts.push(Stmt::TestSet {
            name: "nested".to_string(),
            body: Block {
                stmts: vec![Stmt::FunctionDef {
                    func: Box::new(function),
                    span: span_with_order(2),
                }],
                span: span_with_order(0),
            },
            span: span_with_order(0),
        });

        let mut chronology = target.definition_order_cursor();
        chronology.append_fragment(&mut fragment);

        assert_eq!(fragment.structs[0].span.definition_order, 11);
        assert_eq!(fragment.functions[0].span.definition_order, 12);
        let Stmt::TestSet { body, .. } = &fragment.body.stmts[0] else {
            panic!("expected nested testset");
        };
        let Stmt::FunctionDef { func, span } = &body.stmts[0] else {
            panic!("expected body function definition");
        };
        assert_eq!(span.definition_order, 12);
        assert_eq!(func.span.definition_order, 12);
        assert_eq!(chronology.current(), 12);
    }

    #[test]
    fn independently_lowered_module_is_inserted_after_using_anchor() {
        let mut target = program();
        target.structs = vec![
            struct_with_order("Before", 1),
            struct_with_order("After", 3),
        ];
        target.usings.push(UsingImport {
            module: "Loaded".to_string(),
            symbols: None,
            is_import: false,
            is_relative: false,
            relative_level: 0,
            alias_bindings: Vec::new(),
            span: span_with_order(2),
        });
        let mut loaded = module_with_local_struct("Loaded", 1);
        loaded
            .submodules
            .push(module_with_local_struct("LoadedNested", 2));

        let mut chronology = target.definition_order_cursor();
        let inserted_end = chronology.insert_fragment_after(&mut target, 2, &mut loaded);
        target.modules.push(loaded);

        assert_eq!(target.structs[0].span.definition_order, 1);
        assert_eq!(target.usings[0].span.definition_order, 2);
        assert_eq!(target.modules[0].structs[0].span.definition_order, 3);
        assert_eq!(
            target.modules[0].submodules[0].structs[0]
                .span
                .definition_order,
            4
        );
        assert_eq!(target.structs[1].span.definition_order, 5);
        assert_eq!(inserted_end, 4);
        assert_eq!(chronology.current(), 5);
    }

    #[test]
    fn fragment_insertion_preserves_multi_anchor_nested_chronology() {
        let mut target = program();
        target.structs = vec![
            struct_with_order("Before", 1),
            struct_with_order("Between", 3),
            struct_with_order("After", 5),
        ];
        target.usings = vec![
            UsingImport {
                module: "First".to_string(),
                symbols: None,
                is_import: false,
                is_relative: false,
                relative_level: 0,
                alias_bindings: Vec::new(),
                span: span_with_order(2),
            },
            UsingImport {
                module: "Second".to_string(),
                symbols: None,
                is_import: false,
                is_relative: false,
                relative_level: 0,
                alias_bindings: Vec::new(),
                span: span_with_order(4),
            },
        ];
        let mut existing_nested = module_with_local_struct("ExistingNested", 6);
        existing_nested
            .submodules
            .push(module_with_local_struct("LaterNested", 7));
        target.modules.push(existing_nested);

        let mut first = program();
        first.modules.push(module_with_local_struct("First", 1));
        first.modules.push(module_with_local_struct("FirstWide", 3));
        let mut second = program();
        second.modules.push(module_with_local_struct("Empty", 0));
        second.modules.push(module_with_local_struct("Second", 1));
        let mut zero = module_with_local_struct("LegacyZero", 0);

        let mut chronology = target.definition_order_cursor();
        let first_end = chronology.insert_fragment_after(&mut target, 2, &mut first);
        target.modules.append(&mut first.modules);
        let second_anchor = target.usings[1].span.definition_order;
        let second_end = chronology.insert_fragment_after(&mut target, second_anchor, &mut second);
        target.modules.append(&mut second.modules);
        let zero_end = chronology.insert_fragment_after(&mut target, second_end, &mut zero);
        target.modules.push(zero);

        assert_eq!(first_end, 5);
        assert_eq!(target.structs[1].span.definition_order, 6);
        assert_eq!(second_anchor, 7);
        assert_eq!(second_end, 8);
        assert_eq!(target.structs[2].span.definition_order, 9);
        assert_eq!(target.modules[0].structs[0].span.definition_order, 10);
        assert_eq!(
            target.modules[0].submodules[0].structs[0]
                .span
                .definition_order,
            11
        );
        assert_eq!(zero_end, second_end);
        assert_eq!(
            target
                .modules
                .last()
                .map(|module| module.structs[0].span.definition_order),
            Some(0)
        );
        assert_eq!(chronology.current(), 11);
    }
}

/// Struct definition: `struct Point x::Float64; y::Float64 end`
///
/// Also supports parametric types: `struct Point{T} x::T; y::T end`
/// Also supports subtyping: `struct Dog <: Animal ... end`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructDef {
    pub name: String,
    pub is_mutable: bool,
    /// Compiler provenance set when this definition is merged from the bundled
    /// Base/prelude program. User syntax always lowers with `false`; merge code
    /// owns the transition (Issue #10959/W-70).
    #[serde(default)]
    pub is_base_origin: bool,
    /// Type parameters for parametric structs (e.g., [T] for Point{T})
    pub type_params: Vec<TypeParam>,
    /// Parent abstract type for subtyping (e.g., "Animal" in `struct Dog <: Animal`)
    #[serde(default)]
    pub parent_type: Option<String>,
    pub fields: Vec<StructField>,
    /// Inner constructors defined within the struct body.
    /// If non-empty, no default constructor is generated.
    #[serde(default)]
    pub inner_constructors: Vec<InnerConstructor>,
    /// `global` helper methods declared inside the struct body (Issue #11005).
    /// They are ordinary global methods whose bodies may call `new`; lowering
    /// moves them into the program's function list, so this is empty by the
    /// time a `StructDef` reaches the compiler.
    #[serde(default)]
    pub global_new_helpers: Vec<Function>,
    pub span: Span,
}

/// Abstract type definition: `abstract type Animal end`
///
/// Also supports subtyping: `abstract type Mammal <: Animal end`
/// Also supports parametric types: `abstract type Container{T} end`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AbstractTypeDef {
    pub name: String,
    /// Parent abstract type (e.g., "Animal" in `abstract type Mammal <: Animal`).
    /// If None, defaults to Any.
    pub parent: Option<String>,
    /// Type parameters for parametric abstract types (e.g., [T] for Container{T})
    pub type_params: Vec<TypeParam>,
    pub span: Span,
}

/// Primitive type definition: `primitive type MyBits 8 end`
///
/// Also supports an explicit supertype: `primitive type MyU8 <: Unsigned 8 end`.
/// The number of bits must be a positive multiple of 8 (matching upstream Julia).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrimitiveTypeDef {
    pub name: String,
    /// Parent abstract type (e.g., "Unsigned" in `primitive type MyU8 <: Unsigned 8 end`).
    /// If None, defaults to Any.
    #[serde(default)]
    pub parent: Option<String>,
    /// Number of bits declared for the primitive type (always a multiple of 8).
    pub bits: u32,
    pub span: Span,
}

/// Type alias definition: `const IntOrFloat = Union{Int64, Float64}`
///
/// Type aliases allow creating shorthand names for complex type expressions.
/// Examples:
/// - `const IntOrFloat = Union{Int64, Float64}`
/// - `const ComplexF64 = Complex{Float64}`
/// - `const RealArray = Array{<:Real}`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TypeAliasDef {
    /// The alias name (e.g., "IntOrFloat", "ComplexF64")
    pub name: String,
    /// The target type expression as a string (e.g., "Union{Int64, Float64}", "Complex{Float64}")
    /// Stored as string to preserve the original syntax for resolution at compile time.
    pub target_type: String,
    /// Positional type parameter names for parametric aliases (Issue #5055).
    /// For `MyVec{T} = Vector{T}` this is `["T"]`; empty for non-parametric
    /// aliases such as `const ComplexF64 = Complex{Float64}`.
    #[serde(default)]
    pub params: Vec<String>,
    pub span: Span,
}

/// Macro definition: `macro name(args) body end`
///
/// Macros are compile-time AST transformations. They receive their arguments
/// as Expr objects (unevaluated syntax) and return an Expr to be compiled.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MacroDef {
    pub name: String,
    /// Parameter names (the macro receives AST nodes, not values)
    pub params: Vec<String>,
    /// Whether the last parameter is a varargs parameter (p...)
    pub has_varargs: bool,
    /// The macro body - an expression that should return an Expr
    pub body: Block,
    pub span: Span,
}

/// Enum definition: `@enum Color red green blue`
///
/// Enums are integer-backed symbolic types created at compile time.
/// Each member has a unique integer value, auto-incremented if not specified.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnumDef {
    /// The enum type name (e.g., "Color")
    pub name: String,
    /// The underlying integer type (default: "Int32")
    #[serde(default = "default_enum_base_type")]
    pub base_type: String,
    /// The enum members
    pub members: Vec<EnumMember>,
    pub span: Span,
}

fn default_enum_base_type() -> String {
    "Int32".to_string()
}

/// A single member of an enum definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnumMember {
    /// Member name (e.g., "red")
    pub name: String,
    /// Integer value
    pub value: i64,
    pub span: Span,
}

/// A nominal type declaration whose publication is conditional on runtime
/// top-level control flow reaching its source position (Issue #11654).
///
/// Concrete definitions are boxed because they contain constructor functions,
/// whose bodies recursively contain `Stmt` values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RuntimeNominalDef {
    Struct(Box<StructDef>),
    AbstractType(AbstractTypeDef),
    PrimitiveType(PrimitiveTypeDef),
    Enum(EnumDef),
}

impl StructDef {
    /// Check if this struct has type parameters.
    pub fn is_parametric(&self) -> bool {
        !self.type_params.is_empty()
    }
}

impl Program {
    /// Mark struct definitions in an owned bundled Base/prelude program before
    /// it is merged with user IR.
    pub fn mark_structs_as_base_origin(&mut self) {
        for struct_def in &mut self.structs {
            struct_def.is_base_origin = true;
        }
        for module in &mut self.modules {
            module.mark_structs_as_base_origin();
        }
    }
}

impl Module {
    /// Mark every struct in this prelude module tree as Base-origin before it
    /// is merged with user modules. Kept on the IR node because source spans
    /// are not a reliable provenance boundary after batched/cold merging.
    pub fn mark_structs_as_base_origin(&mut self) {
        self.is_base_origin = true;
        for struct_def in &mut self.structs {
            struct_def.is_base_origin = true;
        }
        for submodule in &mut self.submodules {
            submodule.mark_structs_as_base_origin();
        }
    }
}

/// Field in a struct definition
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructField {
    pub name: String,
    /// Type expression (can be concrete, type variable, or parameterized)
    pub type_expr: Option<TypeExpr>,
    pub span: Span,
}

impl StructField {
    /// Get the type expression as a JuliaType if it's concrete.
    /// Returns None if the type is a type variable or parameterized.
    pub fn as_julia_type(&self) -> Option<JuliaType> {
        match &self.type_expr {
            Some(TypeExpr::Concrete(jt)) => Some(jt.clone()),
            _ => None,
        }
    }
}

/// Inner constructor definition within a struct.
///
/// Represents constructors defined inside a struct body that use `new` to create instances.
/// When a struct has inner constructors, no default constructor is generated.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InnerConstructor {
    pub params: Vec<TypedParam>,
    pub kwparams: Vec<KwParam>,
    pub type_params: Vec<TypeParam>,
    /// Whether the constructor self is explicitly parameterized (`Foo{T}`)
    /// rather than bare (`Foo`). Julia dispatches these through distinct
    /// `Type{Foo{T}}` / `Type{Foo}` method families.
    #[serde(default)]
    pub is_explicit_parametric: bool,
    /// Type-variable names written in the constructor self application, in
    /// positional order (`Foo{T,U}` -> `["T", "U"]`). Method-local `where`
    /// binders that appear only in value arguments are excluded (Issue #10959).
    #[serde(default)]
    pub explicit_type_parameter_names: Vec<String>,
    /// Complete constructor-self argument patterns (`Foo{T,T}` -> `[T,T]`,
    /// `Foo{Int}` -> `[Int]`). Names alone cannot enforce diagonal or concrete
    /// self constraints during explicit-constructor selection (Issue #10959).
    #[serde(default)]
    pub explicit_type_arguments: Vec<TypeExpr>,
    pub body: Block,
    pub span: Span,
}

/// Typed parameter in function signature.
///
/// Represents a parameter with optional type annotation.
/// If `type_annotation` is `None`, the parameter is treated as `Any`.
/// If `is_varargs` is `true`, this parameter collects all remaining arguments as a Tuple.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TypedParam {
    pub name: String,
    pub type_annotation: Option<JuliaType>,
    /// True if this is a varargs parameter (e.g., `args...`)
    #[serde(default)]
    pub is_varargs: bool,
    /// For Vararg{T, N}: fixed argument count N. None = any count. (Issue #2525)
    #[serde(default)]
    pub vararg_count: Option<usize>,
    pub span: Span,
}

impl TypedParam {
    /// Create a new typed parameter with a type annotation.
    pub fn new(name: String, type_annotation: Option<JuliaType>, span: Span) -> Self {
        Self {
            name,
            type_annotation,
            is_varargs: false,
            vararg_count: None,
            span,
        }
    }

    /// Create an untyped parameter (treated as Any).
    pub fn untyped(name: String, span: Span) -> Self {
        Self {
            name,
            type_annotation: None,
            is_varargs: false,
            vararg_count: None,
            span,
        }
    }

    /// Create a varargs parameter (e.g., `args...`).
    /// Collects all remaining arguments as a Tuple.
    pub fn varargs(name: String, type_annotation: Option<JuliaType>, span: Span) -> Self {
        Self {
            name,
            type_annotation,
            is_varargs: true,
            vararg_count: None,
            span,
        }
    }

    /// Create a fixed-count varargs parameter (e.g., `x::Vararg{Int64, 2}`). (Issue #2525)
    pub fn varargs_fixed(
        name: String,
        type_annotation: Option<JuliaType>,
        count: usize,
        span: Span,
    ) -> Self {
        Self {
            name,
            type_annotation,
            is_varargs: true,
            vararg_count: Some(count),
            span,
        }
    }

    /// Get the effective type (returns Any if no annotation).
    pub fn effective_type(&self) -> JuliaType {
        self.type_annotation.clone().unwrap_or(JuliaType::Any)
    }
}

/// Keyword parameter in function signature.
///
/// Represents a keyword parameter with a default value.
/// Example: `function f(; x=1, y=2.0)` has kwparams [KwParam{name="x", default=1}, ...]
/// For varargs kwargs like `kwargs...`, set `is_varargs=true` to collect all remaining kwargs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KwParam {
    pub name: String,
    pub default: Expr,
    pub type_annotation: Option<JuliaType>,
    /// True if this is a varargs kwparam (e.g., `kwargs...`).
    /// Collects all remaining keyword arguments as a NamedTuple.
    #[serde(default)]
    pub is_varargs: bool,
    /// True when this keyword argument's default expression must be
    /// (re-)evaluated inside the function body on every call where the keyword
    /// is omitted, rather than baked into a constant or run by the throwaway
    /// default side-interpreter (Issue #5121). When set, the kwsorter binds a
    /// sentinel (`Value::Undef`) to the slot and a prologue injected into the
    /// body evaluates `default` in the real call frame, so side effects (e.g.
    /// a default that calls a counter) persist and per-call semantics match
    /// upstream Julia. The `default` expression is retained here unchanged so
    /// the prologue can consume it (and so `is_required_kwarg` still sees a
    /// non-`Undef` default and does NOT mistake this for a required kwarg).
    #[serde(default)]
    pub body_evaluated_default: bool,
    pub span: Span,
}

impl KwParam {
    /// Create a new keyword parameter.
    pub fn new(
        name: String,
        default: Expr,
        type_annotation: Option<JuliaType>,
        span: Span,
    ) -> Self {
        Self {
            name,
            default,
            type_annotation,
            is_varargs: false,
            body_evaluated_default: false,
            span,
        }
    }

    /// Create a varargs keyword parameter (e.g., `kwargs...`).
    pub fn varargs(name: String, span: Span) -> Self {
        Self {
            name,
            default: Expr::Literal(Literal::Nothing, span),
            type_annotation: None,
            is_varargs: true,
            body_evaluated_default: false,
            span,
        }
    }

    /// Get the effective type (returns Any if no annotation).
    pub fn effective_type(&self) -> JuliaType {
        self.type_annotation.clone().unwrap_or(JuliaType::Any)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Function {
    pub name: String,
    pub params: Vec<TypedParam>,
    /// Keyword parameters (after `;` in function signature)
    pub kwparams: Vec<KwParam>,
    /// Type parameters from `where` clause (e.g., `where T<:Number`)
    #[serde(default)]
    pub type_params: Vec<TypeParam>,
    /// Return type annotation (e.g., `::Int` in `f(x)::Int = x`)
    /// If present, the return value will be converted to this type using `convert`.
    #[serde(default)]
    pub return_type: Option<JuliaType>,
    pub body: Block,
    /// True if this function extends a Base operator (e.g., `function Base.:+(...)`)
    /// Base extension functions do NOT shadow builtin operators for primitive types.
    #[serde(default)]
    pub is_base_extension: bool,
    /// True for methods introduced by runtime `@eval`.
    #[serde(default)]
    pub is_runtime_eval: bool,
    /// Enclosing struct of a `global` helper declared inside a struct body
    /// (`struct Foo{T}; x::T; global raw(::Type{T}, x) where {T} = new{T}(x); end`).
    /// The method is an ordinary global function, but its body keeps the struct
    /// body's privileged access to `new`/`new{T}` — this is how upstream
    /// `Rational` exposes `unsafe_rational` (Issue #11005).
    #[serde(default)]
    pub new_struct_name: Option<String>,
    pub span: Span,
}

impl Function {
    /// Mark this callable as private lowering output rather than a Julia source
    /// definition. The marker is explicit provenance; callers must not infer
    /// helper status from the function's generated-looking name or from an
    /// otherwise-unstamped zero definition order.
    pub fn into_lowering_helper(mut self) -> Self {
        self.span.definition_order = LOWERING_HELPER_DEFINITION_ORDER;
        self
    }

    pub fn is_lowering_helper(&self) -> bool {
        self.span.definition_order == LOWERING_HELPER_DEFINITION_ORDER
    }
}

/// Block of statements.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub span: Span,
}

/// Compiler metadata annotation retained in lowered IR.
///
/// Upstream Julia represents inference/optimization hints such as
/// `@inline`, `@nospecialize`, and `Base.@constprop` as `Expr(:meta, ...)`.
/// SubsetJuliaVM keeps the supported statement-position subset explicit so
/// later inference/cache/optimizer work can consume it instead of recovering
/// intent from a no-op expression.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetaAnnotation {
    pub name: String,
    #[serde(default)]
    pub args: Vec<String>,
}

/// Statement in Core IR.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Stmt {
    /// Inline block (for lowering else blocks as statements)
    Block(Block),
    Assign {
        var: String,
        value: Expr,
        span: Span,
    },
    AddAssign {
        var: String,
        value: Expr,
        span: Span,
    },
    For {
        var: String,
        start: Expr,
        end: Expr,
        step: Option<Expr>,
        body: Block,
        span: Span,
    },
    /// For-each loop over an iterable (string, array, tuple, etc.)
    /// `for var in iterable ... end`
    ForEach {
        var: String,
        iterable: Expr,
        body: Block,
        span: Span,
    },
    /// For-each loop with tuple destructuring
    /// `for (a, b) in iterable ... end`
    ForEachTuple {
        vars: Vec<String>,
        iterable: Expr,
        body: Block,
        span: Span,
    },
    While {
        condition: Expr,
        body: Block,
        span: Span,
    },
    If {
        condition: Expr,
        then_branch: Block,
        else_branch: Option<Block>,
        span: Span,
    },
    Break {
        span: Span,
    },
    Continue {
        span: Span,
    },
    Try {
        try_block: Block,
        catch_var: Option<String>,
        catch_block: Option<Block>,
        else_block: Option<Block>,
        finally_block: Option<Block>,
        span: Span,
    },
    Return {
        value: Option<Expr>,
        span: Span,
    },
    Expr {
        expr: Expr,
        span: Span,
    },
    /// Compiler metadata marker retained from upstream-compatible meta macros.
    Meta {
        annotation: MetaAnnotation,
        span: Span,
    },
    /// @time macro: execute body, measure and print elapsed time
    /// Note: @time is now Pure Julia but Timed IR is kept for backwards compatibility
    Timed {
        body: Block,
        span: Span,
    },
    /// @test macro: test that condition is true, record result
    Test {
        condition: Expr,
        message: Option<String>,
        span: Span,
    },
    /// @testset macro: group tests and report results
    TestSet {
        name: String,
        body: Block,
        span: Span,
    },
    /// @test_throws macro: test that expression throws expected exception
    TestThrows {
        exception_type: String,
        expr: Box<Expr>,
        span: Span,
    },
    /// Array element assignment: arr[i] = x or arr[i, j] = x
    IndexAssign {
        array: String,
        indices: Vec<Expr>,
        value: Expr,
        span: Span,
    },
    /// Field assignment: obj.field = value (for mutable structs)
    FieldAssign {
        object: String,
        field: String,
        value: Expr,
        span: Span,
    },
    /// Destructuring assignment: (a, b, c) = tuple
    DestructuringAssign {
        targets: Vec<String>,
        value: Expr,
        span: Span,
    },
    /// Dict key-value assignment: dict[key] = value
    DictAssign {
        dict: String,
        key: Expr,
        value: Expr,
        span: Span,
    },
    /// Using statement: `using Module` - imports module's exported functions
    Using {
        module: String,
        span: Span,
    },
    /// Export statement: `export func1, func2` - exports functions from module
    Export {
        names: Vec<String>,
        span: Span,
    },
    /// Function definition statement (for functions defined inside blocks)
    /// This allows function definitions inside @testset and other macro bodies.
    FunctionDef {
        func: Box<Function>,
        span: Span,
    },
    /// Function definition introduced by runtime `@eval`.
    EvalFunctionDef {
        func: Box<Function>,
        span: Span,
    },
    /// Label statement: @label name
    /// Defines a jump target for @goto. Part of Julia's low-level control flow.
    Label {
        name: String,
        span: Span,
    },
    /// Goto statement: @goto name
    /// Unconditionally jumps to the corresponding @label. Part of Julia's low-level control flow.
    Goto {
        name: String,
        span: Span,
    },
    /// Enum definition statement: @enum TypeName member1 member2 ...
    /// Creates an integer-backed enum type with named constants.
    EnumDef {
        enum_def: EnumDef,
        /// `None` for a source declaration. Recovery may persist `Some(names)`
        /// when the enum type was published but an exception stopped its
        /// constant-binding sequence; replay still registers the complete enum
        /// metadata while recreating only the bindings that actually ran.
        #[serde(default)]
        published_members: Option<Vec<String>>,
        span: Span,
    },
    /// Global declaration: `global x` (or `global x, y`).
    ///
    /// Records that the named variables resolve to the top-level (module)
    /// binding for the remainder of the enclosing local scope, rather than
    /// introducing local bindings. The compiler collects these names so that
    /// assignments route to the global frame and reads fall back to it,
    /// matching upstream Julia (Issues #5548, #5549). A bare declaration
    /// compiles to a no-op; `global x = v` / `global x += v` are lowered to a
    /// `Global` marker followed by the assignment.
    Global {
        names: Vec<String>,
        span: Span,
    },
    /// Typed local declaration, matching Core.NewvarNode provenance.
    ///
    /// This is distinct from metadata so macro-produced `Expr(:meta, ...)`
    /// cannot forge lexical-binding provenance. The declaration has no direct
    /// runtime effect; lexical-scope analysis uses it to distinguish a fresh
    /// shadowing local from a soft assignment to an enclosing binding.
    /// Appended for bincode discriminant compatibility.
    LocalDecl {
        var: String,
        kind: LocalDeclKind,
        span: Span,
    },
    /// A nominal declaration executed at its exact position inside top-level
    /// control flow. Unlike root definitions, this metadata is inert until the
    /// statement's bytecode executes. Appended for serialized discriminant
    /// compatibility (Issue #11654).
    RuntimeNominalDef {
        definition: RuntimeNominalDef,
        /// Enum recovery may retain the exact member-binding prefix that was
        /// published before a catchable error (Issues #11652/#11654).
        #[serde(default)]
        published_members: Option<Vec<String>>,
        span: Span,
    },
}

/// Provenance of a typed local declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LocalDeclKind {
    /// A user-visible `local x` declaration that shadows an enclosing local.
    Explicit,
    /// A compiler-generated binding owned by the containing transparent block.
    /// Nested soft scopes update it instead of creating fresh clause locals.
    CompilerEnclosing,
}

/// Expression in Core IR.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Expr {
    Literal(Literal, Span),
    /// Variable reference. The identifier is interned (Issue #10124): every
    /// occurrence of the same name (e.g. `x` in `x + x + x`) shares one
    /// canonical, process-lifetime allocation instead of a fresh `String`
    /// per occurrence. Construct via [`Expr::var`] rather than the tuple
    /// variant directly so the intern happens in one place.
    Var(InternedStr, Span),
    BinaryOp {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
        span: Span,
    },
    UnaryOp {
        op: UnaryOp,
        operand: Box<Expr>,
        span: Span,
    },
    Call {
        function: InternedStr,
        args: Vec<Expr>,
        /// Keyword arguments: [(name, value), ...]
        kwargs: Vec<(InternedStr, Expr)>,
        /// Splat mask: true at index i means args[i] should be splatted
        splat_mask: Vec<bool>,
        /// Kwargs splat mask: true at index i means kwargs[i] should be splatted.
        /// When true, the key is "" (empty) and value is the expression to expand.
        #[serde(default)]
        kwargs_splat_mask: Vec<bool>,
        span: Span,
    },
    Builtin {
        name: BuiltinOp,
        args: Vec<Expr>,
        span: Span,
    },
    /// Array literal: [1, 2, 3] or [1 2; 3 4]
    ArrayLiteral {
        elements: Vec<Expr>,
        shape: Vec<usize>,
        span: Span,
    },
    /// Typed empty array: Int[], Float64[], Complex{Float64}[], etc.
    TypedEmptyArray {
        element_type: InternedStr,
        span: Span,
    },
    /// Array indexing: arr[i] or arr[i, j]
    Index {
        array: Box<Expr>,
        indices: Vec<Expr>,
        span: Span,
    },
    /// Range expression: start:stop or start:step:stop
    Range {
        start: Box<Expr>,
        step: Option<Box<Expr>>,
        stop: Box<Expr>,
        span: Span,
    },
    /// Comprehension: [expr for var in iter] or [expr for var in iter if cond]
    Comprehension {
        body: Box<Expr>,
        var: InternedStr,
        iter: Box<Expr>,
        filter: Option<Box<Expr>>,
        span: Span,
    },
    /// Multi-variable comprehension. Two distinct source forms lower here
    /// (Issue #8014):
    ///
    /// * Comma / cartesian form `[expr for i in R1, j in R2]` (`flatten ==
    ///   false`): a single `for` clause with comma-separated bindings. Produces
    ///   an `N`-dimensional array (rank = number of bindings) via the cartesian
    ///   product of independent iterators (Issue #2143).
    /// * Whitespace / flatten form `[expr for i in R1 for j in R2]` (`flatten ==
    ///   true`): multiple `for` clauses separated by whitespace. Produces a
    ///   1-D `Vector` with `Iterators.flatten` semantics — the inner iterators
    ///   may depend on outer variables and are re-evaluated per outer step.
    ///
    /// For the flatten form `iterations` is stored in outermost→innermost loop
    /// order (lowering already reverses bindings inside a comma-grouped clause
    /// so a comma group iterates column-major within the flatten).
    MultiComprehension {
        body: Box<Expr>,
        /// Each iteration clause: (variable_name, iterator_expression)
        iterations: Vec<(InternedStr, Expr)>,
        filter: Option<Box<Expr>>,
        /// Whitespace `for ... for ...` flatten form (1-D Vector) vs comma
        /// cartesian form (N-D Array). See the variant docs (Issue #8014).
        flatten: bool,
        span: Span,
    },
    /// Generator expression: (expr for var in iter) or (expr for var in iter if cond)
    /// Unlike Comprehension, Generator is lazy - it doesn't evaluate until iterated.
    Generator {
        body: Box<Expr>,
        var: InternedStr,
        iter: Box<Expr>,
        filter: Option<Box<Expr>>,
        span: Span,
    },
    /// Slice all elements in a dimension (:) within indexing
    SliceAll {
        span: Span,
    },
    /// Field access: obj.field
    FieldAccess {
        object: Box<Expr>,
        field: InternedStr,
        span: Span,
    },
    /// Function reference (for passing to higher-order functions)
    /// Resolved to a function index at compile time
    ///
    /// `name` is interned (Issue #10124), like [`Expr::Var`]: many call
    /// sites pattern-match `Expr::Var(name, _) | Expr::FunctionRef { name,
    /// .. }` as an "identifier reference" or-pattern, which requires both
    /// arms to bind the same type — leaving one converted and the other a
    /// plain `String` would force awkward per-arm normalization instead of a
    /// single consistent identifier representation.
    FunctionRef {
        name: InternedStr,
        span: Span,
    },
    /// Tuple literal: (1, 2, 3) or (x, y, z)
    TupleLiteral {
        elements: Vec<Expr>,
        span: Span,
    },
    /// Named tuple literal: (a=1, b=2, c=3)
    NamedTupleLiteral {
        fields: Vec<(InternedStr, Expr)>,
        span: Span,
    },
    /// Pair expression: key => value
    Pair {
        key: Box<Expr>,
        value: Box<Expr>,
        span: Span,
    },
    /// Dict literal: Dict("a" => 1, "b" => 2) or Dict(pairs...)
    DictLiteral {
        pairs: Vec<(Expr, Expr)>,
        span: Span,
    },
    /// Let block: let a = 1, b = 2; body end
    /// Evaluates to the value of the last expression in body
    LetBlock {
        /// Variable bindings: (name, value)
        bindings: Vec<(InternedStr, Expr)>,
        /// Body block containing statements, last one is the return value
        body: Block,
        span: Span,
    },
    /// String concatenation for interpolation: "x = $(x)" becomes StringConcat(["x = ", ToString(x)])
    StringConcat {
        parts: Vec<Expr>,
        span: Span,
    },
    /// Module-qualified call: Module.func(args)
    ModuleCall {
        module: InternedStr,
        function: InternedStr,
        args: Vec<Expr>,
        kwargs: Vec<(InternedStr, Expr)>,
        splat_mask: Vec<bool>,
        kwargs_splat_mask: Vec<bool>,
        span: Span,
    },
    /// Ternary conditional expression: cond ? then_expr : else_expr
    /// Short-circuit evaluation: only one branch is evaluated
    Ternary {
        condition: Box<Expr>,
        then_expr: Box<Expr>,
        else_expr: Box<Expr>,
        span: Span,
    },
    /// `new(args...)` or `new{T}(args...)` in inner constructor.
    /// Creates a new instance of the enclosing struct.
    New {
        type_args: Vec<TypeExpr>,
        args: Vec<Expr>,
        is_splat: bool,
        span: Span,
    },
    /// Construct a parametric type at runtime with dynamically evaluated type arguments.
    /// Example: `Complex{promote_type(T, S)}` where T, S are runtime type values.
    /// The type_args are expressions that evaluate to DataType values at runtime.
    DynamicTypeConstruct {
        /// Base type name (e.g., "Complex", "Vector")
        base: InternedStr,
        /// Runtime expression for a dynamically bound base type, e.g.
        /// `T1{Int,2}` where `T1` is a `UnionAll` value.
        base_expr: Option<Box<Expr>>,
        /// Expressions that evaluate to DataType values
        type_args: Vec<Expr>,
        /// Parallel to `type_args`: `splat_mask[i] == true` means `type_args[i]`
        /// is a `...`-splat of a collection of type values whose elements
        /// become individual type parameters (`Tuple{xs...}`). Empty or
        /// all-false means no splats (Issue #5112).
        splat_mask: Vec<bool>,
        span: Span,
    },
    /// Quoted expression: :symbol or :(expr)
    /// The inner expression constructs the quoted value at runtime.
    /// For :x -> creates Symbol("x")
    /// For :(1+2) -> creates Expr(:call, :+, 1, 2)
    QuoteLiteral {
        /// The expression that constructs the quoted value
        constructor: Box<Expr>,
        span: Span,
    },
    /// Assignment as an expression: x = value
    /// In Julia, assignments are expressions that return the assigned value.
    /// This is used for chained assignments like `local result = x = 42`
    /// or when an assignment is used in expression context.
    AssignExpr {
        /// Variable name to assign to
        var: InternedStr,
        /// Value expression
        value: Box<Expr>,
        span: Span,
    },
    /// Return expression: return expr (used in short-circuit context like `cond && return x`)
    ReturnExpr {
        value: Option<Box<Expr>>,
        span: Span,
    },
    /// Break expression: break (used in short-circuit context like `cond && break`)
    BreakExpr {
        span: Span,
    },
    /// Continue expression: continue (used in short-circuit context like `cond && continue`)
    ContinueExpr {
        span: Span,
    },
    /// Structural explicit numeric type-constructor call (Issue #9803):
    /// `Float64(x)` / `Int64(x)`.
    ///
    /// The shared SSA plan builder (`compile::ssa_ir::plan`) recognizes the
    /// closed set of bare, unqualified, single-positional-argument calls to
    /// these two primitive numeric constructors and rewrites them into this
    /// node *at plan-build time*, where resolving a call target by name is
    /// already the established, legitimate mechanism (mirrors
    /// `compile_builtin_types`'s existing `"Int64"` / `"Float64"` match arms
    /// in the stack compiler). Both backends then dispatch on the `target`
    /// enum discriminant instead of re-matching the callee name as a string:
    /// the register backend in particular must not special-case a type name
    /// by string (repo rule), so this node lets it recognize the conversion
    /// structurally. Any other shape of a call to `Int64`/`Float64`
    /// (qualified, multiple arguments, keyword arguments, splats) stays a
    /// plain `Expr::Call` and is unaffected.
    ///
    /// Appended at the end of the enum: bincode tags `Expr` by declaration
    /// index, so inserting a variant in the middle would silently re-tag
    /// every later variant for any code compiled against an older
    /// declaration order (CACHE_VERSION is bumped alongside this change).
    Convert {
        target: NumericConvertTarget,
        operand: Box<Expr>,
        span: Span,
    },
}

/// Target type for a structural [`Expr::Convert`] node (Issue #9803).
///
/// A small, closed, backend-neutral discriminant — not a string — so both
/// the stack and register backends can dispatch on the conversion kind
/// without matching a type name. Scoped to the two constructors the shared
/// SSA plan currently recognizes; extend alongside `plan.rs` when more
/// numeric constructors gain structural plan support.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NumericConvertTarget {
    Int64,
    Float64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Literal {
    Int(i64),
    Int128(i128),
    BigInt(String),
    BigFloat(String), // Julia's BigFloat type (from big"1.0" literals)
    Float(f64),
    Float32(f32), // Julia's Float32 type (from 1.0f0 literals)
    Float16(f16), // Julia's Float16 type (for REPL persistence)
    Bool(bool),
    Str(String),
    /// Julia String literal whose escape-processed bytes are not valid UTF-8
    /// (Issue #8995), e.g. `"\xff"`. Valid-UTF-8 literals use `Str`.
    StrBytes(Vec<u8>),
    Char(char), // Julia's Char type (32-bit Unicode codepoint)
    /// Malformed Julia Char literal by its 32-bit char pattern (Issue #8995),
    /// e.g. `'\xff'`. Valid scalars use `Char`.
    CharMalformed(u32),
    Nothing, // Julia's `nothing` literal
    Missing, // Julia's `missing` literal
    /// Internal marker for required keyword arguments (no default value)
    /// Used to distinguish required kwargs from optional ones during compilation
    Undef,
    /// Module literal (e.g., Base, Core, Main)
    Module(String),
    /// DataType literal for type objects produced by macro expansion.
    DataType(String),
    /// Array literal with data and shape (for REPL persistence)
    Array(Vec<f64>, Vec<usize>),
    /// Int64 array literal with data and shape (for REPL persistence)
    ArrayI64(Vec<i64>, Vec<usize>),
    /// Bool array literal with data and shape (for REPL persistence)
    ArrayBool(Vec<bool>, Vec<usize>),
    /// Struct literal with type name and field values (for REPL persistence)
    /// The type name is used to look up the type_id at compile time
    Struct(String, Vec<Literal>),
    /// Symbol literal for REPL persistence (e.g., :foo)
    Symbol(String),
    /// Expr literal for REPL persistence (e.g., Meta.parse("1+1"))
    /// head: the expression head (e.g., "call", "block")
    /// args: child arguments (can contain nested Expr, Symbol, or other Literals)
    Expr {
        head: String,
        args: Vec<Literal>,
    },
    /// QuoteNode literal for REPL persistence
    QuoteNode(Box<Literal>),
    /// LineNumberNode literal for REPL persistence
    LineNumberNode {
        line: i64,
        file: Option<String>,
    },
    /// Regex literal (r"pattern" or r"pattern"imsx)
    /// pattern: the regex pattern string
    /// flags: optional flags (i=case insensitive, m=multiline, s=dotall, x=extended)
    Regex {
        pattern: String,
        flags: String,
    },
    /// Enum literal for REPL persistence (@enum type values)
    /// type_name: the enum type (e.g., "Color")
    /// value: the integer backing value of the enum member
    Enum {
        type_name: String,
        value: i64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    IntDiv, // Integer division (÷)
    Mod,
    Pow,
    Lt,
    Gt,
    Le,
    Ge,
    Eq,
    Ne,
    Egal,    // === (object identity)
    NotEgal, // !== (not object identity)
    Subtype, // <: (subtype check)
    And,
    Or,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnaryOp {
    Neg,
    Not,
    Pos, // Unary plus (identity)
}

// `strum::VariantNames` feeds `compile::precompile::enum_variant_fingerprint()`
// (Issue #8626): `BuiltinOp` is serialized both in the prelude Program cache
// (`Expr::Builtin`) and in the Base bytecode cache (`SpecializableFunction.ir`),
// so variant insert/remove/reorder must be detected at cache load time.
//
// Serialize/Deserialize are implemented in `compile::instr_wire_ids` via stable
// wire IDs (Issue #8627) — not derived, to decouple declaration order from
// the wire representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, strum::VariantNames)]
pub enum BuiltinOp {
    Rand,
    Sqrt,
    IfElse,
    TimeNs, // Get current time in nanoseconds
    // Array operations
    Zeros, // zeros(dims...) - create array filled with zeros
    Ones,  // ones(dims...) - create array filled with ones
    // Note: Fill, Trues, Falses are now Pure Julia (base/array.jl) — Issue #2640
    Reshape, // reshape(arr, dims...) - change array dimensions
    Length,  // length(arr) - total number of elements
    // Note: Sum is now Pure Julia (base/array.jl)
    Size,      // size(arr) or size(arr, dim) - dimensions
    Ndims,     // ndims(arr) - number of dimensions
    Push,      // push!(arr, val) - append element
    Pop,       // pop!(arr) - remove and return last element
    PushFirst, // pushfirst!(arr, val) - prepend element
    PopFirst,  // popfirst!(arr) - remove and return first element
    Insert,    // insert!(arr, i, val) - insert at position
    DeleteAt,  // deleteat!(arr, i) - delete at position
    Zero,      // zero(x) - return zero of the same type as x
    // Note: Complex operations (complex, real, imag, conj, abs, abs2) are now Pure Julia
    // Note: Adjoint and Transpose are now Pure Julia (base/array.jl, base/number.jl, base/complex.jl)
    // Linear algebra operations (via faer library)
    Lu,  // lu(A) - LU decomposition with partial pivoting
    Det, // det(A) - matrix determinant
    // Note: Inv removed — BuiltinOp::Inv was dead code (Issue #2643)
    // inv() is handled via BuiltinId::Inv in call.rs, not through BuiltinOp
    // RNG constructors
    StableRNG,  // StableRNG(seed) - create StableRNG instance
    XoshiroRNG, // Xoshiro(seed) - create Xoshiro256++ RNG instance
    // Normal distribution
    Randn, // randn() or randn(rng) - standard normal distribution
    // Tuple operations
    TupleFirst, // first(tuple) - get first element
    TupleLast,  // last(tuple) - get last element
    // Note: TupleLength removed — dead code, generic Length handles tuples (Issue #2643)
    // Dict operations
    HasKey,        // haskey(dict, key) - check if key exists
    DictGet,       // get(dict, key, default) - get value with default
    DictDelete,    // delete!(dict, key) - remove key-value pair
    DictKeys,      // keys(dict) - get all keys
    DictValues,    // values(dict) - get all values
    DictPairs,     // pairs(dict) - iterate over key-value pairs
    DictMerge,     // merge(dict1, dict2) - merge dictionaries
    DictGetBang,   // get!(dict, key, default) - get or insert default
    DictMergeBang, // merge!(dict1, dict2) - merge in-place
    DictEmpty,     // empty!(dict) - clear all entries
    DictGetkey,    // getkey(dict, key, default) - get the key if it exists, else default
    // Broadcasting control
    Ref, // Ref(x) - wrap value to protect from broadcasting (treated as scalar)
    // Type operations
    TypeOf,  // typeof(x) - get type name as string
    Isa,     // isa(x, T) - check if x is of type T
    Eltype,  // eltype(x) - get element type of collection
    Keytype, // keytype(x) - get key type of collection
    Valtype, // valtype(x) - get value type of collection
    Sizeof,  // sizeof(x) - size of value in bytes
    // Isbits removed - pure Julia (Issue #6738)
    Isbitstype,   // isbitstype(T) - check if T is a bits type
    Supertype,    // _supertype(T) - internal parent type intrinsic
    Typename,     // _typename(T) - canonical TypeName symbol (Issue #5106)
    FunctionName, // _function_name(f) - function name symbol (Issue #5580)
    Subtypes,     // subtypes(T) - vector of direct subtypes
    // Typeintersect and Typejoin removed - now Pure Julia (base/reflection.jl)
    // Fieldcount removed - now Pure Julia (base/reflection.jl)
    // Hasfield removed - pure Julia (Issue #6738)
    // Isconcretetype, Isabstracttype, Isprimitivetype, Isstructtype, Ismutabletype
    // removed - now Pure Julia (base/reflection.jl) with internal intrinsics
    // Ismutable removed - pure Julia (Issue #6738)
    // NameOf removed - now Pure Julia (base/reflection.jl)
    Objectid,    // objectid(x) - unique object identifier
    Isunordered, // isunordered(x) - check if x is unordered (NaN, Missing)
    // Reflection (method introspection)
    Methods,   // _methods_by_ftype(f[, types]) - method query intrinsic
    HasMethod, // hasmethod(f, types) - check if method exists
    // Set operations
    In, // in(x, collection) - check if element is in collection
    // RNG seeding
    Seed, // seed!(n) - reseed global RNG
    // Iterator Protocol
    Iterate,   // iterate(collection) or iterate(collection, state)
    Collect,   // collect(iterable) -> Array
    Generator, // Generator(f, iter) - create lazy generator
    // Metaprogramming
    SymbolNew,          // Symbol("name") - create a symbol
    ExprNew,            // Expr(head, args...) - create an expression
    LineNumberNodeNew,  // LineNumberNode(line) or LineNumberNode(line, file)
    QuoteNodeNew,       // QuoteNode(value) - wrap value in QuoteNode
    GlobalRefNew,       // GlobalRef(mod, name) - create a global reference
    Gensym,             // gensym() or gensym("base") - generate unique symbol
    Esc,                // esc(expr) - escape expression for macro hygiene
    Eval,               // eval(expr) - evaluate an Expr at runtime
    MacroExpand,        // macroexpand(m, x) - return expanded form of macro call
    MacroExpandBang,    // macroexpand!(m, x) - destructively expand macro call
    IncludeString,      // include_string(m, code) - parse and evaluate code string
    EvalFile,           // evalfile(path) - evaluate all expressions in a file
    SplatInterpolation, // Marker for $(expr...) splat interpolation in quotes (compile-time)
    // Note: RuntimeSplatInterpolation, ExprNewWithSplat removed — dead code (Issue #2643)
    // Test operations (for Pure Julia @test/@testset/@test_throws macros)
    TestRecord,       // _test_record!(passed, msg) - record test result
    TestRecordBroken, // _test_record_broken!(passed, msg) - record broken test result
    TestSetBegin,     // _testset_begin!(name) - begin test set
    TestSetEnd,       // _testset_end!() - end test set and print summary
    // Variable reflection
    IsDefined, // @isdefined(x) - check if variable is defined
    // Compiler-internal generated-function compatibility eval. Appended for
    // serialized IR discriminant compatibility (Issue #5936).
    GeneratedEval,
    // MersenneTwister(seed) - create MT19937-64 RNG instance (Issue #7306).
    // Appended at the end (NOT grouped with the other RNG constructors above)
    // to preserve serialized IR/BuiltinOp discriminant compatibility: BuiltinOp
    // derives Serialize/Deserialize and round-trips through the base/prelude
    // bincode caches by declaration order, so inserting a variant mid-enum
    // shifts every later discriminant and corrupts cached bytecode (same
    // rationale as GeneratedEval / Issue #5936).
    MersenneTwisterRNG,
    // Internal native Range step access for `step(::UnitRange/StepRange)`.
    // Tail-appended to preserve serialized IR/BuiltinOp discriminants
    // (Issue #9519).
    RangeStep,
    // _test_record_error!(msg, detail) - record an errored test outcome
    // (exception thrown or non-Boolean value while evaluating a `@test`
    // expression), mirroring upstream `Test.Error` / `do_test`'s `Threw`
    // branch (Issue #10093). Tail-appended (NOT grouped with the other Test
    // operations above) to preserve serialized IR/BuiltinOp discriminant
    // compatibility, same rationale as GeneratedEval / Issue #5936.
    TestRecordError,
}

impl Expr {
    /// Construction hub for `Expr::Var` (Issue #10124): interns `name` once
    /// here rather than at each of the ~800 call sites that used to write
    /// `Expr::Var(name.to_string(), span)`. Accepts `&str`/`String`/
    /// `InternedStr` via `Into<InternedStr>`.
    pub fn var(name: impl Into<InternedStr>, span: Span) -> Self {
        Self::Var(name.into(), span)
    }

    /// Construction hub for `Expr::FunctionRef` (Issue #10124), mirroring
    /// [`Expr::var`].
    pub fn function_ref(name: impl Into<InternedStr>, span: Span) -> Self {
        Self::FunctionRef {
            name: name.into(),
            span,
        }
    }

    /// Construction hub for `Expr::Call.function` (Issue #10184), mirroring
    /// [`Expr::var`] and [`Expr::function_ref`].
    pub fn call(function: impl Into<InternedStr>, args: Vec<Expr>, span: Span) -> Self {
        Self::Call {
            function: function.into(),
            args,
            kwargs: Vec::new(),
            splat_mask: Vec::new(),
            kwargs_splat_mask: Vec::new(),
            span,
        }
    }

    pub fn span(&self) -> Span {
        match self {
            Self::Literal(_, span) => *span,
            Self::Var(_, span) => *span,
            Self::BinaryOp { span, .. } => *span,
            Self::UnaryOp { span, .. } => *span,
            Self::Call { span, .. } => *span,
            Self::Builtin { span, .. } => *span,
            Self::ArrayLiteral { span, .. } => *span,
            Self::TypedEmptyArray { span, .. } => *span,
            Self::Index { span, .. } => *span,
            Self::Range { span, .. } => *span,
            Self::Comprehension { span, .. } => *span,
            Self::MultiComprehension { span, .. } => *span,
            Self::Generator { span, .. } => *span,
            Self::SliceAll { span, .. } => *span,
            Self::FieldAccess { span, .. } => *span,
            Self::FunctionRef { span, .. } => *span,
            Self::TupleLiteral { span, .. } => *span,
            Self::NamedTupleLiteral { span, .. } => *span,
            Self::Pair { span, .. } => *span,
            Self::DictLiteral { span, .. } => *span,
            Self::LetBlock { span, .. } => *span,
            Self::StringConcat { span, .. } => *span,
            Self::ModuleCall { span, .. } => *span,
            Self::Ternary { span, .. } => *span,
            Self::New { span, .. } => *span,
            Self::DynamicTypeConstruct { span, .. } => *span,
            Self::QuoteLiteral { span, .. } => *span,
            Self::AssignExpr { span, .. } => *span,
            Self::ReturnExpr { span, .. } => *span,
            Self::BreakExpr { span } => *span,
            Self::ContinueExpr { span } => *span,
            Self::Convert { span, .. } => *span,
        }
    }
}

impl Stmt {
    pub fn span(&self) -> Span {
        match self {
            Self::Block(block) => block.span,
            Self::Assign { span, .. } => *span,
            Self::AddAssign { span, .. } => *span,
            Self::For { span, .. } => *span,
            Self::ForEach { span, .. } => *span,
            Self::ForEachTuple { span, .. } => *span,
            Self::While { span, .. } => *span,
            Self::If { span, .. } => *span,
            Self::Break { span } => *span,
            Self::Continue { span } => *span,
            Self::Try { span, .. } => *span,
            Self::Return { span, .. } => *span,
            Self::Expr { span, .. } => *span,
            Self::Meta { span, .. } => *span,
            Self::Timed { span, .. } => *span,
            Self::Test { span, .. } => *span,
            Self::TestSet { span, .. } => *span,
            Self::TestThrows { span, .. } => *span,
            Self::IndexAssign { span, .. } => *span,
            Self::FieldAssign { span, .. } => *span,
            Self::DestructuringAssign { span, .. } => *span,
            Self::DictAssign { span, .. } => *span,
            Self::Using { span, .. } => *span,
            Self::Export { span, .. } => *span,
            Self::FunctionDef { span, .. } => *span,
            Self::EvalFunctionDef { span, .. } => *span,
            Self::Label { span, .. } => *span,
            Self::Goto { span, .. } => *span,
            Self::EnumDef { span, .. } => *span,
            Self::Global { span, .. } => *span,
            Self::LocalDecl { span, .. } => *span,
            Self::RuntimeNominalDef { span, .. } => *span,
        }
    }
}

#[cfg(test)]
mod issue_11281_local_decl_tests {
    use super::*;

    #[test]
    fn bincode_roundtrip_preserves_both_typed_provenances() {
        let span = Span::new(0, 0, 1, 1, 0, 0);
        for kind in [LocalDeclKind::Explicit, LocalDeclKind::CompilerEnclosing] {
            let stmt = Stmt::LocalDecl {
                var: "x".into(),
                kind,
                span,
            };
            let Ok(bytes) = bincode::serialize(&stmt) else {
                panic!("serialize LocalDecl");
            };
            let Ok(decoded) = bincode::deserialize::<Stmt>(&bytes) else {
                panic!("deserialize LocalDecl");
            };
            assert_eq!(decoded, stmt);
        }
    }
}

#[cfg(test)]
mod interned_expr_tests {
    //! Issue #10124: `Expr::Var` / `Expr::FunctionRef` identifier fields moved
    //! from `String` to `InternedStr`. These tests fix the two invariants
    //! that make the migration safe: (1) identical identifiers really do
    //! dedupe to one canonical allocation, so `Expr` cloning and equality
    //! stay cheap and correct, and (2) the bincode wire encoding of the
    //! interned field is byte-identical to a plain `String`'s, mirroring
    //! `test_value_str_bincode_wire_is_stable_8631` for the earlier
    //! `Value::Str` -> `Rc<str>` migration (Issue #8631).

    use super::*;

    fn span() -> Span {
        Span::new(0, 0, 1, 1, 1, 1)
    }

    fn lit(n: i64) -> Expr {
        Expr::Literal(Literal::Int(n), span())
    }

    fn empty_block() -> Block {
        Block {
            stmts: Vec::new(),
            span: span(),
        }
    }

    fn assert_interned_same_allocation(a: &InternedStr, b: &InternedStr) {
        assert_eq!(a, b);
        assert!(std::ptr::eq(a.as_str(), b.as_str()));
    }

    fn assert_string_wire(name: &InternedStr, expected: &str) {
        let interned_bytes = bincode::serialize(name).expect("serialize InternedStr field");
        let string_bytes =
            bincode::serialize(&expected.to_string()).expect("serialize String field");
        assert_eq!(interned_bytes, string_bytes);
    }

    #[test]
    fn expr_var_hub_interns_so_repeated_identifiers_share_one_allocation() {
        // The issue's own motivating example: `x + x + x` used to allocate
        // three separate `"x"` Strings during lowering.
        let a = Expr::var("x", span());
        let b = Expr::var("x".to_string(), span());
        let c = Expr::var(String::from("x"), span());
        let (Expr::Var(a, _), Expr::Var(b, _), Expr::Var(c, _)) = (&a, &b, &c) else {
            panic!("Expr::var must construct Expr::Var");
        };
        assert_eq!(a, b);
        assert_eq!(a, c);
        assert!(std::ptr::eq(a.as_str(), b.as_str()));
        assert!(std::ptr::eq(a.as_str(), c.as_str()));
    }

    #[test]
    fn expr_function_ref_hub_interns_like_expr_var() {
        let a = Expr::function_ref("foo", span());
        let b = Expr::function_ref("foo".to_string(), span());
        let (Expr::FunctionRef { name: a, .. }, Expr::FunctionRef { name: b, .. }) = (&a, &b)
        else {
            panic!("Expr::function_ref must construct Expr::FunctionRef");
        };
        assert_eq!(a, b);
        assert!(std::ptr::eq(a.as_str(), b.as_str()));
    }

    #[test]
    fn expr_call_hub_interns_function_name_issue_10184() {
        let a = Expr::call("println", Vec::new(), span());
        let b = Expr::call("println".to_string(), Vec::new(), span());
        let (Expr::Call { function: a, .. }, Expr::Call { function: b, .. }) = (&a, &b) else {
            panic!("Expr::call must construct Expr::Call");
        };
        assert_eq!(a, b);
        assert!(std::ptr::eq(a.as_str(), b.as_str()));
    }

    #[test]
    fn expr_call_function_bincode_wire_is_byte_identical_to_string_issue_10184() {
        let call = Expr::call("sqrt", Vec::new(), span());
        let Expr::Call { function, .. } = &call else {
            unreachable!()
        };
        let interned_bytes =
            bincode::serialize(&function).expect("serialize InternedStr call function field");
        let string_bytes =
            bincode::serialize(&"sqrt".to_string()).expect("serialize String call function field");
        assert_eq!(interned_bytes, string_bytes);
    }

    #[test]
    fn expr_remaining_identifier_fields_are_interned_and_string_wire_compatible_issue_10184() {
        let call = Expr::Call {
            function: "dup".into(),
            args: Vec::new(),
            kwargs: vec![("dup".into(), lit(1))],
            splat_mask: Vec::new(),
            kwargs_splat_mask: vec![false],
            span: span(),
        };
        let typed_empty = Expr::TypedEmptyArray {
            element_type: "dup".into(),
            span: span(),
        };
        let comprehension = Expr::Comprehension {
            body: Box::new(lit(1)),
            var: "dup".into(),
            iter: Box::new(lit(2)),
            filter: None,
            span: span(),
        };
        let multi_comprehension = Expr::MultiComprehension {
            body: Box::new(lit(1)),
            iterations: vec![("dup".into(), lit(2))],
            filter: None,
            flatten: false,
            span: span(),
        };
        let generator = Expr::Generator {
            body: Box::new(lit(1)),
            var: "dup".into(),
            iter: Box::new(lit(2)),
            filter: None,
            span: span(),
        };
        let field_access = Expr::FieldAccess {
            object: Box::new(Expr::var("obj", span())),
            field: "dup".into(),
            span: span(),
        };
        let named_tuple = Expr::NamedTupleLiteral {
            fields: vec![("dup".into(), lit(1))],
            span: span(),
        };
        let let_block = Expr::LetBlock {
            bindings: vec![("dup".into(), lit(1))],
            body: empty_block(),
            span: span(),
        };
        let module_call = Expr::ModuleCall {
            module: "dup".into(),
            function: "dup".into(),
            args: Vec::new(),
            kwargs: vec![("dup".into(), lit(1))],
            splat_mask: Vec::new(),
            kwargs_splat_mask: vec![false],
            span: span(),
        };
        let dynamic_type = Expr::DynamicTypeConstruct {
            base: "dup".into(),
            base_expr: None,
            type_args: Vec::new(),
            splat_mask: Vec::new(),
            span: span(),
        };
        let assign_expr = Expr::AssignExpr {
            var: "dup".into(),
            value: Box::new(lit(1)),
            span: span(),
        };

        let Expr::Call {
            function: call_function,
            kwargs: call_kwargs,
            ..
        } = &call
        else {
            unreachable!()
        };
        let Expr::TypedEmptyArray { element_type, .. } = &typed_empty else {
            unreachable!()
        };
        let Expr::Comprehension { var: comp_var, .. } = &comprehension else {
            unreachable!()
        };
        let Expr::MultiComprehension { iterations, .. } = &multi_comprehension else {
            unreachable!()
        };
        let Expr::Generator {
            var: generator_var, ..
        } = &generator
        else {
            unreachable!()
        };
        let Expr::FieldAccess { field, .. } = &field_access else {
            unreachable!()
        };
        let Expr::NamedTupleLiteral { fields, .. } = &named_tuple else {
            unreachable!()
        };
        let Expr::LetBlock { bindings, .. } = &let_block else {
            unreachable!()
        };
        let Expr::ModuleCall {
            module,
            function: module_function,
            kwargs: module_kwargs,
            ..
        } = &module_call
        else {
            unreachable!()
        };
        let Expr::DynamicTypeConstruct { base, .. } = &dynamic_type else {
            unreachable!()
        };
        let Expr::AssignExpr {
            var: assign_var, ..
        } = &assign_expr
        else {
            unreachable!()
        };

        let names = [
            call_function,
            &call_kwargs[0].0,
            element_type,
            comp_var,
            &iterations[0].0,
            generator_var,
            field,
            &fields[0].0,
            &bindings[0].0,
            module,
            module_function,
            &module_kwargs[0].0,
            base,
            assign_var,
        ];

        for name in names {
            assert_interned_same_allocation(call_function, name);
            assert_string_wire(name, "dup");
        }
    }

    #[test]
    fn expr_var_bincode_wire_is_byte_identical_to_string_8631_style() {
        let interned = Expr::var("cache-10124", span());
        // Hand-build the pre-migration shape (`Var(String, Span)`) via a
        // throwaway local type with the same `#[derive(Serialize)]` shape,
        // so this test does not need to special-case the real `Expr` enum's
        // discriminant/variant count.
        #[derive(serde::Serialize)]
        struct LegacyVar(String, Span);

        let Expr::Var(name, sp) = &interned else {
            unreachable!()
        };
        let legacy = LegacyVar(name.to_string(), *sp);

        let interned_field_bytes = bincode::serialize(name).expect("serialize InternedStr field");
        let legacy_field_bytes = bincode::serialize(&legacy.0).expect("serialize String field");
        assert_eq!(
            interned_field_bytes, legacy_field_bytes,
            "Expr::Var's InternedStr field must serialize identically to a \
             plain String — this is what keeps existing prelude/Base bincode \
             caches loadable without a CACHE_VERSION bump (Issue #10124)"
        );
    }

    #[test]
    fn expr_function_ref_bincode_wire_is_byte_identical_to_string() {
        let a = Expr::function_ref("retry", span());
        let Expr::FunctionRef { name, .. } = &a else {
            unreachable!()
        };
        let interned_bytes = bincode::serialize(name).expect("serialize InternedStr field");
        let string_bytes =
            bincode::serialize(&"retry".to_string()).expect("serialize String field");
        assert_eq!(interned_bytes, string_bytes);
    }
}
