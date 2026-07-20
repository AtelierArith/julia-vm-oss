//! Source-ordered runtime activation for bindings imported through `using`.

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::bytecode::{Instr, ValueType};
use crate::ir::core::Expr;
use crate::span::Span;
use crate::types::{builtin_type_binding_authority, BuiltinTypeBindingAuthority, TypeExpr};

use super::{types, CResult, CoreCompiler};

#[derive(Debug, Clone)]
pub(super) struct ResolvedUsingImport {
    /// Stable identity in the owning `Program.usings` vector. Runtime recovery
    /// records this statement index rather than inferring reachability from the
    /// binding names it happened to change (Issue #11748).
    pub(super) program_index: usize,
    pub(super) module: String,
    pub(super) source_module: String,
    pub(super) is_import: bool,
    pub(super) is_relative: bool,
    pub(super) selected_symbols: Option<HashSet<String>>,
    pub(super) alias_bindings: Vec<(String, String)>,
    pub(super) span: Span,
    /// Whether the statement binds the source module's own short name. A
    /// whole-module rename (`import P as D`) binds only `D`, not `P`.
    pub(super) binds_module_root: bool,
    pub(super) has_renames: bool,
    /// Every lowered rename assignment, retaining its target, the source path
    /// carried by the lowered expression, its canonical source, and the import
    /// statement span. The lowered path disambiguates repeated aliases within
    /// one import statement, whose generated assignments share a span.
    pub(super) alias_assignments: Vec<(String, String, String, Span)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ModuleAliasProvenance {
    /// Selective `using`/`import` and module-valued `as` aliases are explicit
    /// bindings. They take precedence over names made visible by exports.
    Explicit,
    /// A module-valued name made visible by a nonselective `using M` export.
    Exported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ImportBindingKind {
    Module,
    NonModule,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ModuleAliasState {
    Bound {
        canonical_target: String,
        provenance: ModuleAliasProvenance,
        kind: ImportBindingKind,
    },
    /// Conflicting nonselective exports remain ambiguous until an explicit
    /// import replaces them. This tombstone prevents a later export or raw
    /// import scan from silently restoring a first-wins binding.
    Ambiguous,
}

fn insert_module_alias_state(
    aliases: &mut HashMap<String, ModuleAliasState>,
    alias: String,
    canonical_target: String,
    provenance: ModuleAliasProvenance,
    kind: ImportBindingKind,
) {
    let Some(current) = aliases.get_mut(&alias) else {
        aliases.insert(
            alias,
            ModuleAliasState::Bound {
                canonical_target,
                provenance,
                kind,
            },
        );
        return;
    };

    if let ModuleAliasState::Bound {
        canonical_target: current_target,
        provenance: current_provenance,
        kind: current_kind,
    } = current
    {
        if current_target == &canonical_target {
            if provenance == ModuleAliasProvenance::Explicit {
                *current_provenance = ModuleAliasProvenance::Explicit;
            }
            if kind == ImportBindingKind::Module {
                *current_kind = ImportBindingKind::Module;
            }
            return;
        }
    }

    match current {
        ModuleAliasState::Bound {
            provenance: ModuleAliasProvenance::Exported,
            ..
        } => match provenance {
            ModuleAliasProvenance::Exported => {
                *current = ModuleAliasState::Ambiguous;
            }
            ModuleAliasProvenance::Explicit => {
                *current = ModuleAliasState::Bound {
                    canonical_target,
                    provenance,
                    kind,
                };
            }
        },
        ModuleAliasState::Ambiguous => {
            if provenance == ModuleAliasProvenance::Explicit {
                *current = ModuleAliasState::Bound {
                    canonical_target,
                    provenance,
                    kind,
                };
            }
        }
        ModuleAliasState::Bound {
            provenance: ModuleAliasProvenance::Explicit,
            ..
        } => {}
    }
}

fn build_module_alias_states_impl(
    resolved_usings: &[ResolvedUsingImport],
    module_functions: &HashMap<String, HashSet<String>>,
    module_exports: &HashMap<String, HashSet<String>>,
    module_imported_bindings: &HashMap<String, String>,
    materialize_unique_exported_bindings: bool,
) -> HashMap<String, ModuleAliasState> {
    let mut aliases = HashMap::new();
    // Unique exported non-module names keep using the established lightweight
    // lookup path. Materialize them in the authoritative table only when two
    // providers export the same name (where ambiguity is semantic); module
    // candidates are always materialized because qualified-path resolution
    // needs their canonical target. This avoids cloning every Base export into
    // every function compiler merely to detect the rare collision.
    let mut first_exported_target: HashMap<String, String> = HashMap::new();
    let mut exported_conflicts = HashSet::new();
    for resolved_using in resolved_usings {
        if resolved_using.is_import
            || resolved_using.selected_symbols.is_some()
            || !resolved_using.binds_module_root
        {
            continue;
        }
        if let Some(exports) = module_exports.get(&resolved_using.module) {
            for name in exports {
                let candidate = format!("{}.{}", resolved_using.module, name);
                let candidate = resolve_import_binding_chain(module_imported_bindings, &candidate)
                    .unwrap_or(candidate);
                match first_exported_target.get(name) {
                    Some(first) if first != &candidate => {
                        exported_conflicts.insert(name.clone());
                    }
                    None => {
                        first_exported_target.insert(name.clone(), candidate);
                    }
                    _ => {}
                }
            }
        }
    }
    for resolved_using in resolved_usings {
        if resolved_using.binds_module_root {
            let root = resolved_using
                .module
                .rsplit('.')
                .next()
                .unwrap_or(&resolved_using.module);
            insert_module_alias_state(
                &mut aliases,
                root.to_string(),
                resolved_using.module.clone(),
                ModuleAliasProvenance::Explicit,
                ImportBindingKind::Module,
            );
        }
        for (alias, _, target, _) in &resolved_using.alias_assignments {
            // Classify by the canonical target's registered module identity,
            // so functions, values, and types never become `Module` values
            // merely because they used `as` (`import Base: sin as mysin`,
            // Issue #11176 hardening).
            let canonical_target = resolve_import_binding_chain(module_imported_bindings, target)
                .unwrap_or_else(|| target.clone());
            let is_module = module_functions.contains_key(&canonical_target)
                || module_exports.contains_key(&canonical_target);
            let kind = if is_module {
                ImportBindingKind::Module
            } else {
                ImportBindingKind::NonModule
            };
            insert_module_alias_state(
                &mut aliases,
                alias.clone(),
                canonical_target,
                ModuleAliasProvenance::Explicit,
                kind,
            );
        }

        let (mut visible_names, provenance): (Vec<String>, ModuleAliasProvenance) =
            match &resolved_using.selected_symbols {
                Some(symbols) => (
                    symbols.iter().cloned().collect(),
                    ModuleAliasProvenance::Explicit,
                ),
                None if resolved_using.binds_module_root && !resolved_using.is_import => {
                    let exports = module_exports
                        .get(&resolved_using.module)
                        .map(|exports| exports.iter().cloned().collect())
                        .unwrap_or_default();
                    (exports, ModuleAliasProvenance::Exported)
                }
                None => (Vec::new(), ModuleAliasProvenance::Exported),
            };
        // HashSet iteration must never choose a winner. Conflicts for one
        // alias key are resolved only between the ordered UsingImport entries.
        visible_names.sort();
        visible_names.dedup();
        for name in visible_names {
            let candidate = format!("{}.{}", resolved_using.module, name);
            let candidate = resolve_import_binding_chain(module_imported_bindings, &candidate)
                .unwrap_or(candidate);
            let kind = if module_functions.contains_key(&candidate)
                || module_exports.contains_key(&candidate)
            {
                ImportBindingKind::Module
            } else {
                ImportBindingKind::NonModule
            };
            if !materialize_unique_exported_bindings
                && provenance == ModuleAliasProvenance::Exported
                && kind == ImportBindingKind::NonModule
                && !exported_conflicts.contains(&name)
            {
                continue;
            }
            insert_module_alias_state(&mut aliases, name, candidate, provenance, kind);
        }
    }
    aliases
}

pub(super) fn build_canonical_module_alias_states(
    resolved_usings: &[ResolvedUsingImport],
    module_functions: &HashMap<String, HashSet<String>>,
    module_exports: &HashMap<String, HashSet<String>>,
    module_imported_bindings: &HashMap<String, String>,
) -> HashMap<String, ModuleAliasState> {
    build_module_alias_states_impl(
        resolved_usings,
        module_functions,
        module_exports,
        module_imported_bindings,
        false,
    )
}

pub(super) fn build_live_import_binding_states(
    resolved_usings: &[ResolvedUsingImport],
    module_functions: &HashMap<String, HashSet<String>>,
    module_exports: &HashMap<String, HashSet<String>>,
    module_imported_bindings: &HashMap<String, String>,
) -> HashMap<String, ModuleAliasState> {
    build_module_alias_states_impl(
        resolved_usings,
        module_functions,
        module_exports,
        module_imported_bindings,
        true,
    )
}

pub(super) fn bound_module_aliases(
    states: &HashMap<String, ModuleAliasState>,
) -> HashMap<String, String> {
    states
        .iter()
        .filter_map(|(alias, state)| match state {
            ModuleAliasState::Bound {
                canonical_target,
                kind: ImportBindingKind::Module,
                ..
            } => Some((alias.clone(), canonical_target.clone())),
            ModuleAliasState::Bound { .. } | ModuleAliasState::Ambiguous => None,
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AliasProvenance {
    Exported,
    Explicit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ImportedBindingResolution {
    Resolved {
        source_module: String,
        source_name: Option<String>,
        provenance: AliasProvenance,
    },
    Ambiguous,
}

/// Follow the live-import metadata to the binding that owns the Julia object.
///
/// Import/re-export edges deliberately remain source qualified in
/// `module_imported_bindings` (`Optim.NonDifferentiable ->
/// NLSolversBase.NonDifferentiable`).  Consumers must compare the end of that
/// chain, not the spelling of the immediate provider: two facade modules can
/// re-export the same generic/type without making the name ambiguous.
pub(super) fn resolve_import_binding_chain(
    bindings: &HashMap<String, String>,
    qualified_name: &str,
) -> Option<String> {
    let mut current = bindings.get(qualified_name)?.clone();
    let mut seen = HashSet::from([qualified_name.to_string()]);
    loop {
        if !seen.insert(current.clone()) {
            return None;
        }
        match bindings.get(&current) {
            Some(next) => current = next.clone(),
            None => return Some(current),
        }
    }
}

fn direct_imported_bindings(
    _module_functions: &HashMap<String, HashSet<String>>,
    module_exports: &HashMap<String, HashSet<String>>,
    _module_constants: &HashMap<String, HashSet<String>>,
    using_import: &ResolvedUsingImport,
) -> Vec<(String, String, Option<String>, AliasProvenance)> {
    let using_module = &using_import.module;
    let mut bindings = Vec::new();

    if using_import.binds_module_root {
        let root = using_module.rsplit('.').next().unwrap_or(using_module);
        bindings.push((
            root.to_string(),
            using_module.clone(),
            None,
            AliasProvenance::Explicit,
        ));
    }

    // Renames bind only their target spelling. Keep the source module and
    // source field separate so each read can go back through Module.getfield
    // instead of freezing a value snapshot (Issue #11203 hardening).
    for (source, target) in &using_import.alias_bindings {
        let source_leaf = source.rsplit('.').next().unwrap_or(source);
        let whole_module = source == using_module
            || source_leaf == using_module.rsplit('.').next().unwrap_or(using_module);
        bindings.push((
            target.clone(),
            using_module.clone(),
            (!whole_module).then(|| source_leaf.to_string()),
            AliasProvenance::Explicit,
        ));
    }

    let (mut visible_names, provenance) = match &using_import.selected_symbols {
        Some(symbols) => (
            symbols.iter().cloned().collect::<HashSet<_>>(),
            AliasProvenance::Explicit,
        ),
        None if !using_import.is_import => {
            let mut names = module_exports
                .get(using_module)
                .cloned()
                .unwrap_or_default();
            // Base is represented by the merged top-level prelude rather than
            // an ordinary IR module, so use its canonical export metadata for
            // an explicit `using Base`. Restrict this fallback to Base: an
            // ordinary module with no exports must introduce no bare names.
            if crate::module_names::is_base(using_module) {
                names.extend(crate::julia::base::exported_names().iter().cloned());
            }
            (names, AliasProvenance::Exported)
        }
        None => (HashSet::new(), AliasProvenance::Explicit),
    };
    for name in visible_names.drain() {
        // Macro imports are consumed by lowering's macro environment and have
        // no runtime value binding to activate. Treating `@name` like a value
        // would make Module.getfield probe an erased macro global (#7948).
        if name.starts_with('@')
            || (using_module
                .strip_prefix("Random")
                .is_some_and(str::is_empty)
                && super::is_random_function(&name))
        {
            continue;
        }
        bindings.push((name.clone(), using_module.clone(), Some(name), provenance));
    }
    bindings.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    bindings
}

pub(super) fn imported_binding_names(
    module_functions: &HashMap<String, HashSet<String>>,
    module_exports: &HashMap<String, HashSet<String>>,
    module_constants: &HashMap<String, HashSet<String>>,
    resolved_usings: &[ResolvedUsingImport],
) -> HashSet<String> {
    resolved_usings
        .iter()
        .flat_map(|using_import| {
            direct_imported_bindings(
                module_functions,
                module_exports,
                module_constants,
                using_import,
            )
        })
        .map(|(name, ..)| name)
        .collect()
}

impl CoreCompiler<'_> {
    fn canonical_imported_binding_source(
        &self,
        source_module: String,
        source_name: Option<String>,
    ) -> (String, Option<String>) {
        let Some(source_name) = source_name else {
            return (source_module, None);
        };
        let qualified = format!("{source_module}.{source_name}");
        let canonical =
            resolve_import_binding_chain(&self.shared_ctx.module_imported_bindings, &qualified)
                .or_else(|| {
                    // Ordinary modules implicitly see Base.  A compatibility facade
                    // can therefore export a Base binding without declaring a local
                    // function (the bundled Broadcast module is one example).  Only
                    // use the bare/Base table when the provider has no local surface
                    // declaration; an independently owned same-leaf binding remains
                    // distinct.
                    let provider_owns_name = self.module_has_binding(&source_module, &source_name);
                    let base_owns_generic = super::is_base_function(&source_name)
                        || self
                            .method_tables
                            .get(&source_name)
                            .is_some_and(|table| table.base_function_count() > 0)
                        || self
                            .method_tables
                            .contains_key(&format!("Base.{source_name}"));
                    (!provider_owns_name && base_owns_generic)
                        .then(|| format!("Base.{source_name}"))
                })
                .unwrap_or(qualified);

        canonical.rsplit_once('.').map_or_else(
            || ("Base".to_string(), Some(canonical.clone())),
            |(module, name)| (module.to_string(), Some(name.to_string())),
        )
    }

    pub(super) fn emit_runtime_callable_call(
        &mut self,
        callable_temp: String,
        args: &[Expr],
        kwargs: &[(crate::ir::core::InternedStr, Expr)],
        splat_mask: &[bool],
        kwargs_splat_mask: &[bool],
    ) -> CResult<ValueType> {
        for arg in args {
            self.compile_expr(arg)?;
        }
        let kwarg_names: Vec<String> = kwargs.iter().map(|(name, _)| name.to_string()).collect();
        for (_, value) in kwargs {
            self.compile_expr(value)?;
        }
        self.emit(Instr::LoadAny(callable_temp));
        if !kwargs.is_empty()
            || splat_mask.iter().any(|is_splat| *is_splat)
            || kwargs_splat_mask.iter().any(|is_splat| *is_splat)
        {
            self.emit(Instr::CallFunctionVariableWithKwargsSplat(Box::new(
                crate::bytecode::CallVarKwargsSplat {
                    arg_count: args.len(),
                    pos_splat_mask: splat_mask.to_vec(),
                    kwarg_names,
                    kwargs_splat_mask: kwargs_splat_mask.to_vec(),
                },
            )));
        } else {
            self.emit(Instr::CallFunctionVariable(args.len()));
        }
        Ok(ValueType::Any)
    }

    pub(super) fn compile_imported_module_alias_call(
        &mut self,
        module: &str,
        function: &str,
        args: &[Expr],
        kwargs: &[(crate::ir::core::InternedStr, Expr)],
        splat_mask: &[bool],
        kwargs_splat_mask: &[bool],
    ) -> CResult<ValueType> {
        let root = module.split_once('.').map_or(module, |(root, _)| root);
        self.emit_load_imported_binding(root);
        for segment in module.split('.').skip(1) {
            self.emit(Instr::GetFieldByName(segment.to_string()));
        }
        self.emit(Instr::GetFieldByName(function.to_string()));
        let callable_temp = self.new_temp("imported_module_callable");
        self.emit(Instr::StoreAny(callable_temp.clone()));
        self.emit_runtime_callable_call(callable_temp, args, kwargs, splat_mask, kwargs_splat_mask)
    }

    pub(super) fn compile_local_module_syntax_call(
        &mut self,
        module: &str,
        function: &str,
        args: &[Expr],
        kwargs: &[(crate::ir::core::InternedStr, Expr)],
        splat_mask: &[bool],
        kwargs_splat_mask: &[bool],
    ) -> CResult<ValueType> {
        let root = module.split_once('.').map_or(module, |(root, _)| root);
        self.load_local(root)?;
        for segment in module.split('.').skip(1) {
            self.emit(Instr::GetFieldByName(segment.to_string()));
        }
        self.emit(Instr::GetFieldByName(function.to_string()));
        let callable_temp = self.new_temp("local_module_syntax_callable");
        self.emit(Instr::StoreAny(callable_temp.clone()));
        self.emit_runtime_callable_call(callable_temp, args, kwargs, splat_mask, kwargs_splat_mask)
    }

    pub(super) fn resolved_module_alias(&self, name: &str) -> Option<&str> {
        self.module_aliases.get(name).map(String::as_str)
    }

    pub(super) fn imported_binding_root<'a>(&self, path: &'a str) -> Option<&'a str> {
        if self.in_base_function_scope {
            return None;
        }
        let root = path.split_once('.').map_or(path, |(root, _)| root);
        (self.imported_bindings.contains(root)
            && (self.imported_binding_has_non_base_source(root)
                || self.imported_binding_is_renamed(root)))
        .then_some(root)
    }

    pub(super) fn imported_binding_has_non_base_source(&self, name: &str) -> bool {
        self.resolved_usings
            .iter()
            .flat_map(|using_import| {
                direct_imported_bindings(
                    self.module_functions,
                    self.module_exports,
                    self.module_constants,
                    using_import,
                )
            })
            .any(|(binding_name, source_module, _, _)| {
                binding_name == name && !crate::module_names::is_base(&source_module)
            })
    }

    pub(super) fn imported_binding_is_renamed(&self, name: &str) -> bool {
        self.resolved_usings.iter().any(|using_import| {
            using_import
                .alias_bindings
                .iter()
                .any(|(_, target)| target == name)
        })
    }

    pub(super) fn set_resolved_module_alias(&mut self, name: String, module_path: String) {
        self.module_aliases.insert(name, module_path);
    }

    pub(super) fn resolved_active_imported_module_alias(&self, name: &str) -> Option<String> {
        match self.active_imported_bindings.get(name) {
            Some(ImportedBindingResolution::Resolved {
                source_module,
                source_name: Some(source_name),
                ..
            }) if source_name == name
                && (self
                    .module_functions
                    .contains_key(&format!("{source_module}.{name}"))
                    || self
                        .module_exports
                        .contains_key(&format!("{source_module}.{name}"))) =>
            {
                Some(format!("{source_module}.{source_name}"))
            }
            Some(ImportedBindingResolution::Resolved {
                source_module,
                source_name: None,
                ..
            }) => Some(source_module.clone()),
            Some(ImportedBindingResolution::Resolved { .. })
            | Some(ImportedBindingResolution::Ambiguous)
            | None => None,
        }
    }

    pub(super) fn resolved_active_imported_type_name(&self, name: &str) -> Option<String> {
        let ImportedBindingResolution::Resolved {
            source_module,
            source_name: Some(source_name),
            ..
        } = self.active_imported_bindings.get(name)?
        else {
            return None;
        };
        let qualified = format!("{source_module}.{source_name}");
        self.shared_ctx
            .type_aliases
            .get(&qualified)
            .cloned()
            .or_else(|| {
                self.type_object_binding_exists(&qualified)
                    .then_some(qualified)
            })
            .or_else(|| self.resolve_visible_type_alias(name))
            .or_else(|| self.resolve_parametric_struct_name(name))
            .or_else(|| self.resolve_visible_type_object_name(name))
    }

    fn runtime_module_alias_binding_name(&self, name: &str) -> String {
        self.current_module_path
            .as_ref()
            .map_or_else(|| name.to_string(), |module| format!("{module}.{name}"))
    }

    fn runtime_imported_binding_source_storage(&self, name: &str) -> String {
        let scope = self.current_module_path.as_deref().unwrap_or("Main");
        format!("#sjulia_imported_binding_source#{scope}#{name}")
    }

    fn runtime_module_alias_ambiguity_flag(&self, name: &str) -> String {
        let scope = self.current_module_path.as_deref().unwrap_or("Main");
        format!("#sjulia_module_alias_ambiguous#{scope}#{name}")
    }

    fn runtime_imported_binding_renamed_flag(&self, name: &str) -> String {
        let scope = self.current_module_path.as_deref().unwrap_or("Main");
        format!("#sjulia_imported_binding_renamed#{scope}#{name}")
    }

    fn runtime_imported_binding_whole_module_flag(&self, name: &str) -> String {
        let scope = self.current_module_path.as_deref().unwrap_or("Main");
        format!("#sjulia_imported_binding_whole_module#{scope}#{name}")
    }

    fn renamed_import_source_name(&self, target: &str) -> Option<String> {
        self.resolved_usings.iter().find_map(|using_import| {
            using_import
                .alias_bindings
                .iter()
                .find(|(_, alias)| alias == target)
                .and_then(|(source, _)| source.rsplit('.').next().map(str::to_string))
        })
    }

    /// Load a source-ordered imported submodule alias. The hidden ambiguity
    /// flag is absent before the first `using`, false for a resolved binding,
    /// and true after two distinct exported bindings conflict. Guarding the
    /// flag lookup preserves the ordinary `UndefVarError(name)` before the
    /// first `using` executes (Issues #11203/#11216).
    pub(super) fn emit_load_imported_binding(&mut self, name: &str) {
        let flag = self.runtime_module_alias_ambiguity_flag(name);
        self.emit(Instr::IsDefined(flag.clone()));
        let no_flag = self.here();
        self.emit(Instr::JumpIfZero(usize::MAX));
        self.emit(Instr::LoadGlobalAny(flag));
        let not_ambiguous = self.here();
        self.emit(Instr::JumpIfZero(usize::MAX));
        self.emit(Instr::ThrowUndefVarError(name.to_string()));
        let load_binding = self.here();
        self.patch_jump(not_ambiguous, load_binding);

        let whole_module_flag = self.runtime_imported_binding_whole_module_flag(name);
        self.emit(Instr::IsDefined(whole_module_flag.clone()));
        let no_shape_flags = self.here();
        self.emit(Instr::JumpIfZero(usize::MAX));
        self.emit(Instr::LoadGlobalAny(whole_module_flag));
        let not_whole_module = self.here();
        self.emit(Instr::JumpIfZero(usize::MAX));
        self.emit(Instr::LoadGlobalAny(
            self.runtime_imported_binding_source_storage(name),
        ));
        let whole_done = self.here();
        self.emit(Instr::Jump(usize::MAX));

        let renamed_check = self.here();
        self.patch_jump(not_whole_module, renamed_check);
        self.emit(Instr::LoadGlobalAny(
            self.runtime_imported_binding_renamed_flag(name),
        ));
        let ordinary_source = self.here();
        self.emit(Instr::JumpIfZero(usize::MAX));
        self.emit(Instr::LoadGlobalAny(
            self.runtime_imported_binding_source_storage(name),
        ));
        if let Some(source_name) = self.renamed_import_source_name(name) {
            self.emit(Instr::GetFieldByName(source_name));
        } else {
            // The flag can only be true for an `as` binding retained in
            // resolved_usings. Keep malformed cached metadata fail-safe.
            self.emit(Instr::GetFieldByName(name.to_string()));
        }
        let renamed_done = self.here();
        self.emit(Instr::Jump(usize::MAX));

        let ordinary_load = self.here();
        self.patch_jump(ordinary_source, ordinary_load);
        self.emit(Instr::LoadGlobalAny(
            self.runtime_imported_binding_source_storage(name),
        ));
        self.emit(Instr::GetFieldByName(name.to_string()));
        let ordinary_done = self.here();
        self.emit(Instr::Jump(usize::MAX));

        // Persistent REPL programs compiled before the shape flags were added
        // can still carry the ordinary visible binding and ambiguity flag.
        // Preserve that cached binding instead of probing a missing hidden flag.
        let cached_binding = self.here();
        self.patch_jump(no_flag, cached_binding);
        self.patch_jump(no_shape_flags, cached_binding);
        self.emit(Instr::LoadGlobalAny(
            self.runtime_module_alias_binding_name(name),
        ));
        let done = self.here();
        self.patch_jump(whole_done, done);
        self.patch_jump(renamed_done, done);
        self.patch_jump(ordinary_done, done);
    }

    fn emit_runtime_imported_binding_state(
        &mut self,
        name: &str,
        resolution: &ImportedBindingResolution,
    ) {
        match resolution {
            ImportedBindingResolution::Resolved {
                source_module,
                source_name,
                ..
            } => {
                let submodule_path = source_name
                    .as_ref()
                    .map(|source_name| format!("{source_module}.{source_name}"))
                    .filter(|path| {
                        self.module_functions.contains_key(path)
                            || self.module_exports.contains_key(path)
                    });
                let stored_module = submodule_path.as_deref().unwrap_or(source_module);
                let source_is_base = source_module
                    .strip_prefix("Base")
                    .is_some_and(str::is_empty);
                // A Julia type binding is itself callable through its
                // constructors. When both the type registry and method table
                // contain the spelling, the binding value is the type object,
                // never a plain Function (e.g. `import Base: OneTo`).
                let base_direct_type = source_is_base
                    && source_name.as_ref().is_some_and(|source_name| {
                        super::type_helpers::is_builtin_type_name(source_name)
                            || self.type_object_binding_exists(source_name)
                    });
                let imported_type_name = source_name.as_ref().and_then(|source_name| {
                    let qualified = format!("{source_module}.{source_name}");
                    if let Some(target) = self.shared_ctx.type_aliases.get(&qualified) {
                        Some(target.clone())
                    } else if self.type_object_binding_exists(&qualified) {
                        Some(qualified)
                    } else {
                        None
                    }
                });
                let base_direct_function = source_is_base
                    && !base_direct_type
                    && source_name.as_ref().is_some_and(|source_name| {
                        super::is_base_function(source_name)
                            || self.method_tables.contains_key(source_name)
                    });
                let imported_function_name = if submodule_path.is_none() {
                    source_name.as_ref().and_then(|source_name| {
                        let qualified = format!("{source_module}.{source_name}");
                        (imported_type_name.is_none()
                            && self.method_tables.contains_key(&qualified))
                        .then_some(qualified)
                    })
                } else {
                    None
                };
                let direct_value = source_name.is_none()
                    || submodule_path.is_some()
                    || base_direct_function
                    || imported_function_name.is_some()
                    || base_direct_type
                    || imported_type_name.is_some();
                if let Some(type_name) = &imported_type_name {
                    self.emit(Instr::PushDataType(type_name.clone()));
                } else if base_direct_type {
                    if let Some(source_name) = source_name {
                        self.emit(Instr::PushDataType(source_name.clone()));
                    }
                } else if base_direct_function {
                    if let Some(source_name) = source_name {
                        self.emit_function_value(source_name);
                    }
                } else if let Some(qualified) = &imported_function_name {
                    if let Some(source_name) = source_name {
                        // A module can add methods to a Base/prelude-owned generic.
                        // Its qualified table is then only that module's shard;
                        // importing the binding must retain the shared bare pool so
                        // methods added by other modules remain reachable. For an
                        // independently owned generic, keep the source-qualified
                        // candidates so sibling declarations with the same bare
                        // spelling stay distinct (Issues #8052/#11088/#11203).
                        let shared_generic =
                            self.method_tables.get(source_name).is_some_and(|table| {
                                let base_count = table.base_function_count();
                                table.methods.iter().any(|method| {
                                    method.is_base_extension
                                        || method.is_base_program_method(base_count)
                                })
                            });
                        let lookup_name = if shared_generic {
                            source_name.as_str()
                        } else {
                            qualified.as_str()
                        };
                        self.emit_function_value_named(source_name, lookup_name);
                    }
                } else {
                    let root = stored_module
                        .split_once('.')
                        .map_or(stored_module, |(root, _)| root);
                    if self.current_module_path.is_none()
                        && self.toplevel_module_bindings.contains(root)
                    {
                        // Follow the declared runtime ModuleValue so imports and
                        // renames preserve exports, publics, and nested module
                        // identity instead of synthesizing a metadata-poor copy
                        // (Issue #11240 review regression). Module bodies run
                        // before Main initializes top-level module slots, so
                        // they must use the reflection-complete compile-time
                        // descriptor instead of loading an uninitialized slot.
                        self.emit(Instr::LoadAny(root.to_string()));
                        for segment in stored_module.split('.').skip(1) {
                            self.emit(Instr::GetFieldByName(segment.to_string()));
                        }
                    } else {
                        self.emit_module_value(stored_module);
                    }
                }
                if direct_value {
                    self.emit(Instr::Dup);
                    self.emit(Instr::StoreGlobalAny(
                        self.runtime_module_alias_binding_name(name),
                    ));
                    self.emit(Instr::StoreGlobalAny(
                        self.runtime_imported_binding_source_storage(name),
                    ));
                } else {
                    self.emit(Instr::StoreGlobalAny(
                        self.runtime_imported_binding_source_storage(name),
                    ));
                    // Keep ordinary imported values as live source bindings.
                    // Reading the field here would eagerly fail for a declared
                    // but uninitialized source global; Julia accepts the import
                    // and raises only when the imported binding is read. Runtime
                    // loads and reflection both follow the recorded source.
                }
                self.emit(Instr::PushBool(
                    source_name.as_ref().is_some_and(|source| source != name),
                ));
                self.emit(Instr::StoreGlobalAny(
                    self.runtime_imported_binding_renamed_flag(name),
                ));
                self.emit(Instr::PushBool(direct_value));
                self.emit(Instr::StoreGlobalAny(
                    self.runtime_imported_binding_whole_module_flag(name),
                ));
                self.emit(Instr::PushBool(false));
            }
            ImportedBindingResolution::Ambiguous => self.emit(Instr::PushBool(true)),
        }
        self.emit(Instr::StoreGlobalAny(
            self.runtime_module_alias_ambiguity_flag(name),
        ));
    }

    pub(super) fn compile_using_alias_activation(
        &mut self,
        source_module: &str,
        span: Span,
    ) -> CResult<()> {
        let Some((index, using_import)) = self
            .resolved_usings
            .iter()
            .enumerate()
            .skip(self.using_import_cursor)
            .find(|(_, import)| import.source_module == source_module && import.span == span)
            .map(|(index, import)| (index, import.clone()))
        else {
            // A pre-lowered/cached program may not retain executable `using`
            // markers. Its compile-time import metadata remains sufficient for
            // non-submodule names, so keep this marker a no-op.
            return Ok(());
        };
        self.using_import_cursor = index + 1;

        for (name, source_module, source_name, provenance) in direct_imported_bindings(
            self.module_functions,
            self.module_exports,
            self.module_constants,
            &using_import,
        ) {
            let imported_whole_module = source_name.is_none()
                && name == source_module.rsplit('.').next().unwrap_or(&source_module);
            let locally_owned_module_root = imported_whole_module
                && using_import.is_relative
                && self.module_path_in_current_scope(&name).is_some();
            // A module owned by this lexical scope is a real binding and wins
            // over imported names, including an imported ambiguity.
            // A whole-module using/import is itself the declaration of that
            // module root. Suppress it only for a module actually declared by
            // this lexical scope; the merged module-constant surface also
            // contains imported stdlibs such as Test/LinearAlgebra and is not
            // ownership evidence. This keeps a pre-existing local module from
            // being overwritten while still activating ordinary stdlib roots
            // (Issues #11132/#11216/#11240).
            if locally_owned_module_root
                || (!imported_whole_module
                    && (self.module_path_in_current_scope(&name).is_some()
                        || self
                            .current_module_path
                            .as_ref()
                            .is_some_and(|scope| self.module_has_binding(scope, &name))))
            {
                continue;
            }
            let (source_module, source_name) =
                self.canonical_imported_binding_source(source_module, source_name);
            let incoming = ImportedBindingResolution::Resolved {
                source_module,
                source_name,
                provenance,
            };
            let next = match self.active_imported_bindings.get(&name) {
                None => incoming,
                Some(ImportedBindingResolution::Resolved {
                    source_module: existing_module,
                    source_name: existing_name,
                    provenance: existing_provenance,
                }) if matches!(&incoming, ImportedBindingResolution::Resolved { source_module, source_name, .. } if source_module == existing_module && source_name == existing_name) => {
                    ImportedBindingResolution::Resolved {
                        source_module: existing_module.clone(),
                        source_name: existing_name.clone(),
                        provenance: if provenance == AliasProvenance::Explicit {
                            AliasProvenance::Explicit
                        } else {
                            *existing_provenance
                        },
                    }
                }
                Some(
                    existing @ ImportedBindingResolution::Resolved {
                        provenance: AliasProvenance::Explicit,
                        ..
                    },
                ) => existing.clone(),
                Some(ImportedBindingResolution::Resolved {
                    provenance: AliasProvenance::Exported,
                    ..
                }) if provenance == AliasProvenance::Explicit => incoming,
                Some(ImportedBindingResolution::Resolved {
                    provenance: AliasProvenance::Exported,
                    ..
                }) => ImportedBindingResolution::Ambiguous,
                Some(ImportedBindingResolution::Ambiguous)
                    if provenance == AliasProvenance::Explicit =>
                {
                    incoming
                }
                Some(ImportedBindingResolution::Ambiguous) => ImportedBindingResolution::Ambiguous,
            };
            let changed = self.active_imported_bindings.get(&name) != Some(&next);
            if changed {
                self.emit_runtime_imported_binding_state(&name, &next);
                self.active_imported_bindings.insert(name, next);
            }
        }
        self.emit(Instr::ActivateUsing {
            owner_module: self.current_module_path.clone().unwrap_or_default(),
            program_index: using_import.program_index,
        });
        Ok(())
    }

    /// Activate import metadata for a pre-lowered/package module whose body has
    /// no executable `Stmt::Using` markers. Included package sources can retain
    /// `Module.usings` while omitting every marker; without entry initialization,
    /// imported-binding reads fall back to nonexistent qualified globals such as
    /// `DataStructures.Forward`. Marker-bearing modules stay on the source-ordered
    /// statement path above (Issues #11203/#11216).
    pub(super) fn compile_markerless_using_alias_activations(&mut self) -> CResult<()> {
        let imports: Vec<(String, Span)> = self
            .resolved_usings
            .iter()
            .map(|using| (using.source_module.clone(), using.span))
            .collect();
        for (source_module, span) in imports {
            self.compile_using_alias_activation(&source_module, span)?;
        }
        Ok(())
    }
}

pub(super) fn is_runtime_import_metadata_binding(name: &str) -> bool {
    [
        "#sjulia_imported_binding_source#",
        "#sjulia_module_alias_ambiguous#",
        "#sjulia_imported_binding_renamed#",
        "#sjulia_imported_binding_whole_module#",
    ]
    .iter()
    .any(|prefix| name.starts_with(prefix))
}

impl CoreCompiler<'_> {
    pub(super) fn module_has_binding(&self, module: &str, name: &str) -> bool {
        let qualified = format!("{}.{}", module, name);
        self.module_functions
            .get(module)
            .is_some_and(|bindings| bindings.contains(name))
            || self
                .module_constants
                .get(module)
                .is_some_and(|constants| constants.contains(name))
            || self.shared_ctx.type_aliases.contains_key(&qualified)
            || self.type_object_binding_exists(&qualified)
    }

    pub(super) fn type_object_binding_exists(&self, name: &str) -> bool {
        self.abstract_type_names.contains(name)
            || self.shared_ctx.enum_types.contains_key(name)
            || self.shared_ctx.is_primitive_type_name(name)
            || self.shared_ctx.struct_table.contains_key(name)
            || self.shared_ctx.parametric_structs.contains_key(name)
    }

    /// The qualified module value binding that shadowed a conflicting import
    /// of `name` (upstream's warn-and-ignore, Issue #11426): Some when the
    /// current module has a plain body assignment of `name` that precedes
    /// every explicit (selective/renamed) import of the same name. The
    /// ignored import must not resolve `name` as a static type; the value
    /// binding stays authoritative.
    pub(super) fn conflict_winning_module_value_binding(&self, name: &str) -> Option<String> {
        if name.contains('.') {
            return None;
        }
        let scope = self.current_module_path.as_deref()?;
        let qualified = format!("{scope}.{name}");
        let binding_order = *self
            .shared_ctx
            .module_value_binding_positions
            .get(&qualified)?;
        let mut explicit_import_orders = self
            .resolved_usings
            .iter()
            .filter(|using_import| {
                using_import
                    .selected_symbols
                    .as_ref()
                    .is_some_and(|symbols| symbols.contains(name))
                    || using_import
                        .alias_assignments
                        .iter()
                        .any(|(alias, ..)| alias == name)
            })
            .map(|using_import| using_import.span.definition_order)
            .peekable();
        explicit_import_orders.peek()?;
        explicit_import_orders
            .all(|import_order| binding_order < import_order)
            .then_some(qualified)
    }

    /// Whether a source-declared type with this bare name exists somewhere in
    /// the program but is not a binding of the current lexical module.
    ///
    /// `type_definition_positions` records user-source declarations (including
    /// their temporary bare registry aliases), not inherited Base/Core types.
    /// Using it as the provenance gate prevents a parent/sibling type from
    /// leaking through the global identity registry without routing ordinary
    /// Base helpers such as `HasEltype` through the caller module (Issue #11168).
    pub(super) fn source_type_binding_is_hidden(&self, name: &str) -> bool {
        // Qualified names carry their owner explicitly. This guard exists only
        // for globally cached *bare* aliases; treating an imported canonical
        // target such as `P.Source.T` as hidden would incorrectly require `P`
        // to be a lexical root in the importing module (Issue #11168).
        if name.contains('.') {
            return false;
        }
        let Some(scope) = self.current_module_path.as_deref() else {
            return false;
        };
        if self.in_base_function_scope
            || self.module_has_binding(scope, name)
            || matches!(
                self.module_alias_states.get(name),
                Some(ModuleAliasState::Bound { .. })
            )
        {
            return false;
        }
        self.shared_ctx.type_definition_positions.contains_key(name)
            && self.type_object_binding_exists(name)
            && self.resolve_visible_type_object_name(name).is_none()
    }

    /// Resolve the registry spelling of an imported type object.
    ///
    /// Base imports carry a canonical source such as `Base.Rational`, while
    /// modeled Base types are intentionally stored under their ordinary short
    /// registry key (`Rational`). User-module types retain their qualified
    /// nominal identity (Issue #11176).
    fn imported_type_object_target(&self, canonical_target: &str) -> Option<String> {
        if self.type_object_binding_exists(canonical_target) {
            return Some(canonical_target.to_string());
        }
        let base_name = canonical_target.strip_prefix("Base.")?;
        self.type_object_binding_exists(base_name)
            .then(|| base_name.to_string())
    }

    fn imported_type_alias_target(&self, canonical_target: &str) -> Option<String> {
        self.shared_ctx
            .type_aliases
            .get(canonical_target)
            .cloned()
            .or_else(|| {
                canonical_target
                    .strip_prefix("Base.")
                    .and_then(|base_name| self.shared_ctx.type_aliases.get(base_name).cloned())
            })
    }

    pub(super) fn resolve_visible_type_object_name(&self, name: &str) -> Option<String> {
        // Selective imports share one source-ordered lexical binding table
        // across modules, functions, values, and types.  Consult that table
        // before the eagerly collected type registries: a later imported type
        // must not turn a name whose first explicit owner was a function (or a
        // module) back into a type object (Issue #11176).
        if let Some(state) = self.module_alias_states.get(name) {
            return match state {
                ModuleAliasState::Bound {
                    canonical_target,
                    kind: ImportBindingKind::NonModule,
                    ..
                } => self.imported_type_object_target(canonical_target),
                ModuleAliasState::Bound { .. } | ModuleAliasState::Ambiguous => None,
            };
        }

        if let Some(module_path) = &self.current_module_path {
            let qualified = format!("{}.{}", module_path, name);
            if self.type_object_binding_exists(&qualified) {
                return Some(qualified);
            }
        }

        for using_module in self.visible_using_modules_for_name(name) {
            let qualified = format!("{}.{}", using_module, name);
            if self.type_object_binding_exists(&qualified) {
                return Some(qualified);
            }
            if crate::module_names::is_base(&using_module)
                && builtin_type_binding_authority(name) == Some(BuiltinTypeBindingAuthority::Base)
                && self.type_object_binding_exists(name)
            {
                return Some(name.to_string());
            }
        }

        // Every Julia module receives Core, including `baremodule`. Keep the
        // lexical authority check separate from the compiler's type projection
        // so a registered spelling is not itself treated as a binding (#11419).
        if builtin_type_binding_authority(name) == Some(BuiltinTypeBindingAuthority::Core)
            && self.type_object_binding_exists(name)
        {
            return Some(name.to_string());
        }

        // Base is itself a baremodule, but it owns rather than imports its
        // Base-qualified bindings.
        if self
            .current_module_path
            .as_deref()
            .is_some_and(crate::module_names::is_base)
            && builtin_type_binding_authority(name) == Some(BuiltinTypeBindingAuthority::Base)
            && self.type_object_binding_exists(name)
        {
            return Some(name.to_string());
        }

        // Ordinary modules have an implicit `using Base` even though Base is
        // compiled as the merged top-level prelude rather than an IR module in
        // `resolved_usings`. `baremodule` deliberately has no such fallback.
        if !self.current_module_is_bare
            && crate::julia::base::is_exported(name)
            && self.type_object_binding_exists(name)
        {
            return Some(name.to_string());
        }

        if self.current_module_path.is_some() {
            return None;
        }

        self.type_object_binding_exists(name)
            .then(|| name.to_string())
    }

    /// Resolve a type binding owned by the current lexical module or by Main,
    /// without consulting any `using` metadata. Definition-time signature
    /// probes use this before source-activated imports so a future `using`
    /// cannot expose a whole-program type-table entry early (Issues
    /// #11203/#11216).
    pub(super) fn resolve_owned_type_object_name(&self, name: &str) -> Option<String> {
        if let Some(module_path) = &self.current_module_path {
            let qualified = format!("{module_path}.{name}");
            if self.type_object_binding_exists(&qualified) {
                return Some(qualified);
            }
        }

        // Module types are registered under a bare alias before source emission
        // so constructor/type inference can resolve them after `using`. That
        // structural alias is not a Main-owned binding. Reject it when its
        // current-source position is exactly the position of an eventual
        // imported qualified type. This whole-program scan only identifies the
        // alias's provenance; it never grants visibility (Issues #11025/#11216).
        let bare_position = self.shared_ctx.type_definition_positions.get(name);
        let bare_is_import_alias = self.imported_bindings.contains(name)
            && bare_position.is_some_and(|bare_position| {
                self.visible_using_modules_for_name(name)
                    .into_iter()
                    .any(|module| {
                        self.shared_ctx
                            .type_definition_positions
                            .get(&format!("{module}.{name}"))
                            .is_some_and(|qualified_position| {
                                qualified_position.definition_order
                                    == bare_position.definition_order
                                    && qualified_position.source_start == bare_position.source_start
                            })
                    })
            });
        if bare_is_import_alias {
            return None;
        }

        self.type_object_binding_exists(name)
            .then(|| name.to_string())
    }

    pub(super) fn canonical_module_path(&self, module: &str) -> String {
        if let Some(main_module) = module.strip_prefix("Main.") {
            if self.is_known_module_path(main_module) {
                return main_module.to_string();
            }
        }
        if let Some(base_submodule) = module.strip_prefix("Base.") {
            if self.module_functions.contains_key(base_submodule)
                && !super::constants::is_stdlib_module(base_submodule)
            {
                return base_submodule.to_string();
            }
        }
        module.to_string()
    }

    pub(super) fn is_known_module_path(&self, module: &str) -> bool {
        self.module_functions.contains_key(module)
            || self.module_exports.contains_key(module)
            || super::constants::is_stdlib_module(module)
            || crate::module_names::is_language_root(module)
    }

    /// Ordinary modules receive `Base`, `Core`, and `Main` directly from the
    /// language, plus module-valued bindings exported by their implicit
    /// `using Base`.  Require export metadata plus either a known module path
    /// or a declaration in the bundled Base source, so isolated cache-backed
    /// compilations retain Base submodules without exposing unrelated stdlibs.
    fn is_implicit_base_module_binding(&self, name: &str) -> bool {
        matches!(
            crate::module_names::classify_builtin_module(name),
            crate::module_names::BuiltinModule::Core | crate::module_names::BuiltinModule::Main
        ) || (!self.current_module_is_bare
            && (crate::module_names::is_base(name)
                || (crate::julia::base::is_exported(name)
                    && (self.is_known_module_path(name)
                        || crate::julia::base::declares_module(name)))))
    }

    /// Resolve a syntactic module path only when its root is lexically bound in
    /// the module currently being compiled.
    ///
    /// The global module registry records every module in the combined program,
    /// but Julia modules do not inherit their parent's or `Main`'s bindings.  A
    /// child `P.C` therefore cannot use bare `P` (or an unrelated root `Q`)
    /// merely because those registry entries exist.  The visible roots are the
    /// current module's own short name, a direct nested module, an explicit
    /// alias/non-selective import, and Core/Base's implicit module exports.
    pub(super) fn resolve_visible_module_path(&self, module: &str) -> Option<String> {
        let root = module.split('.').next().unwrap_or(module);
        let rest = module.strip_prefix(root).unwrap_or_default();
        // Imported submodule names have source-ordered provenance semantics.
        // A bound target owns the bare root and receives the dotted suffix
        // exactly once. An ambiguous export collision is a tombstone: never
        // resurrect either provider through a raw `resolved_usings` scan.
        if !self.in_base_function_scope {
            if let Some(state) = self.module_alias_states.get(root) {
                return match state {
                    ModuleAliasState::Bound {
                        canonical_target,
                        kind: ImportBindingKind::Module,
                        ..
                    } => {
                        let candidate = format!("{canonical_target}{rest}");
                        self.is_known_module_path(&candidate)
                            .then(|| self.canonical_module_path(&candidate))
                    }
                    ModuleAliasState::Bound { .. } | ModuleAliasState::Ambiguous => None,
                };
            }
        }

        let canonical = self.resolve_module_alias_path(module);

        // An alias is itself a lexical binding (`const A = M`, `import M as A`).
        if canonical != module && self.is_known_module_path(&canonical) {
            return Some(self.canonical_module_path(&canonical));
        }

        if self.is_implicit_base_module_binding(root) {
            return Some(self.canonical_module_path(&canonical));
        }

        // Main/top-level code owns only root modules defined by the user
        // program. Merely loading a stdlib/dependency into the global registry
        // does not create a Main binding for it.
        let Some(current) = self.current_module_path.as_deref() else {
            if self.toplevel_module_bindings.contains(root) && self.is_known_module_path(&canonical)
            {
                return Some(self.canonical_module_path(&canonical));
            }

            for resolved_using in &self.resolved_usings {
                if resolved_using.binds_module_root
                    && resolved_using
                        .module
                        .rsplit('.')
                        .next()
                        .unwrap_or(&resolved_using.module)
                        == root
                    && self.is_known_module_path(&canonical)
                {
                    return Some(self.canonical_module_path(&canonical));
                }
            }
            return None;
        };

        let current_short = current.rsplit('.').next().unwrap_or(current);

        // A module binds its own name, and it binds its direct submodules.
        if root == current_short {
            let candidate = format!("{current}{rest}");
            if self.is_known_module_path(&candidate) {
                return Some(self.canonical_module_path(&candidate));
            }
        }
        let nested_candidate = format!("{current}.{module}");
        if self.is_known_module_path(&nested_candidate) {
            return Some(self.canonical_module_path(&nested_candidate));
        }

        // `using/import M` binds M.  A selective `using/import M: x` binds x,
        // not the source module name, so only the non-selective form qualifies.
        if self.in_base_function_scope {
            return None;
        }
        for resolved_using in &self.resolved_usings {
            if !resolved_using.binds_module_root
                || resolved_using
                    .module
                    .rsplit('.')
                    .next()
                    .unwrap_or(&resolved_using.module)
                    != root
            {
                continue;
            }
            let candidate = format!("{}{rest}", resolved_using.module);
            if self.is_known_module_path(&candidate) {
                return Some(self.canonical_module_path(&candidate));
            }
        }

        None
    }

    pub(super) fn is_ambiguous_module_alias_root(&self, module: &str) -> bool {
        let root = module.split('.').next().unwrap_or(module);
        matches!(
            self.module_alias_states.get(root),
            Some(ModuleAliasState::Ambiguous)
        )
    }

    pub(super) fn is_bound_module_alias_root(&self, module: &str) -> bool {
        let root = module.split('.').next().unwrap_or(module);
        matches!(
            self.module_alias_states.get(root),
            Some(ModuleAliasState::Bound {
                kind: ImportBindingKind::Module,
                ..
            })
        )
    }

    /// Emit a runtime module-scope lookup for a statically-known module name
    /// that is not lexically bound in the current module.  `GetFieldByName` on
    /// a Module raises the catchable, scope-aware `UndefVarError` used by Julia.
    pub(super) fn emit_unbound_module_name(&mut self, module: &str) -> ValueType {
        let root = module.split('.').next().unwrap_or(module);
        let scope = self.current_module_path.clone().unwrap_or_default();
        let exports = self
            .module_exports
            .get(scope.as_str())
            .map(|set| {
                let mut exports: Vec<String> = set.iter().cloned().collect();
                exports.sort();
                exports
            })
            .unwrap_or_default();
        self.emit(Instr::PushModule(Box::new(
            crate::bytecode::ModuleOperands {
                name: scope,
                exports,
                publics: vec![],
                base_exports_visible: !self.current_module_is_bare,
                implicit_standard_bindings: !self.current_module_is_bare,
            },
        )));
        self.emit(Instr::GetFieldByName(root.to_string()));
        ValueType::Any
    }

    pub(super) fn module_path_in_current_scope(&self, path: &str) -> Option<String> {
        let candidate = self
            .current_module_path
            .as_ref()
            .map_or_else(|| path.to_string(), |current| format!("{current}.{path}"));
        // `module_functions` also contains qualified extension namespaces such
        // as `OrdinaryDiffEq.SciMLBase` for methods defined as
        // `SciMLBase.f(...)`; that does not declare a nested module binding.
        // `module_exports` is populated only by the real module tree, including
        // modules with an empty export list, so it is the ownership authority
        // here (Issues #11132/#11216).
        self.module_exports
            .contains_key(&candidate)
            .then_some(candidate)
    }

    pub(super) fn visible_using_modules_for_name(&self, name: &str) -> Vec<String> {
        // Issue #10220/#10294 (follow-up to #10078/#10257): `self.usings` /
        // `self.resolved_usings` are fed from `CorePipeline::usings_set`,
        // which is collected from the WHOLE combined program's top-level
        // `using` statements — Base/prelude's own top level and the USER
        // SCRIPT's own top level both have `module_path == None`, so they
        // are indistinguishable there, and the same flat set is passed to
        // EVERY `CoreCompiler` instance regardless of scope.
        //
        // While compiling Base/prelude's OWN function bodies
        // (`in_base_function_scope`, reachable only when
        // `should_skip_base_cache_for_program` bypasses the frozen Base
        // cache), a bare name must never be qualified against a module the
        // USER script `using`-imported — Base's own source has zero textual
        // relationship to that `using` statement. Concretely: Base's own
        // `partition()` (subset_julia_vm/src/julia/base/iterators.jl)
        // constructs `Partition(itr, Int64(n))`; if a user module also
        // exports a same-named `Partition` via `using .MyPkg`, this method
        // must not qualify Base's own bare reference to `MyPkg.Partition`
        // (wrong field layout -> runtime `DynamicToI64` corruption) — it
        // must return empty so callers fall through to the origin-aware
        // bare-name path (`resolve_struct_info_scoped`, `resolve_struct_name`,
        // `resolve_parametric_struct_name`).
        if self.in_base_function_scope {
            return Vec::new();
        }

        let mut modules = Vec::new();
        let mut seen = HashSet::new();

        if self.resolved_usings.is_empty() {
            for using_module in self.usings {
                if self
                    .module_exports
                    .get(using_module)
                    .is_some_and(|exports| exports.contains(name))
                    && seen.insert(using_module.clone())
                {
                    modules.push(using_module.clone());
                }
            }
            return modules;
        }

        for resolved_using in &self.resolved_usings {
            let mut visible = match &resolved_using.selected_symbols {
                Some(symbols) => {
                    symbols.contains(name) && self.module_has_binding(&resolved_using.module, name)
                }
                None => {
                    !resolved_using.is_import
                        && resolved_using.binds_module_root
                        && self
                            .module_exports
                            .get(&resolved_using.module)
                            .is_some_and(|exports| {
                                let candidate = format!("{}.{}", resolved_using.module, name);
                                if self.is_known_module_path(&candidate) {
                                    exports.contains(name)
                                } else if exports.is_empty() {
                                    self.module_has_binding(&resolved_using.module, name)
                                } else {
                                    exports.contains(name)
                                }
                            })
                }
            };
            if visible {
                let candidate = format!("{}.{}", resolved_using.module, name);
                visible = match self.module_alias_states.get(name) {
                    Some(ModuleAliasState::Bound {
                        canonical_target, ..
                    }) => canonical_target == &candidate,
                    Some(ModuleAliasState::Ambiguous) => false,
                    None => true,
                };
            }
            if visible && seen.insert(resolved_using.module.clone()) {
                modules.push(resolved_using.module.clone());
            }
        }
        modules
    }

    pub(super) fn is_renamed_only_module_root(&self, name: &str) -> bool {
        if !self.renamed_only_module_roots.contains(name) {
            return false;
        }
        // Renaming a binding from a module does not erase an independently
        // existing module binding in this scope (`module P ... end;
        // import .P: x as y; P.x = 2`, Issue #11176).
        if self.current_module_path.is_none() && self.toplevel_module_bindings.contains(name) {
            return false;
        }
        if self.strict_undefined_check && self.initialized_locals.contains(name) {
            return false;
        }
        !self
            .current_module_path
            .as_deref()
            .and_then(|path| self.module_constants.get(path))
            .is_some_and(|constants| constants.contains(name))
    }

    fn resolve_visible_type_alias_exact(&self, name: &str) -> Option<String> {
        // Imported aliases are registered eagerly so signatures can resolve
        // forward references, but lexical ownership is source ordered.  Only
        // the canonical winner may contribute type meaning for this bare name;
        // metadata from a later losing type import is not visible (Issue
        // #11176).
        if let Some(state) = self.module_alias_states.get(name) {
            return match state {
                ModuleAliasState::Bound {
                    canonical_target,
                    kind: ImportBindingKind::NonModule,
                    ..
                } => self
                    .imported_type_alias_target(canonical_target)
                    .or_else(|| self.imported_type_object_target(canonical_target)),
                ModuleAliasState::Bound { .. } | ModuleAliasState::Ambiguous => None,
            };
        }

        if let Some(module_path) = &self.current_module_path {
            let qualified = format!("{}.{}", module_path, name);
            if let Some(target) = self.shared_ctx.type_aliases.get(&qualified) {
                return Some(target.clone());
            }
        }

        for using_module in self.visible_using_modules_for_name(name) {
            let qualified = format!("{}.{}", using_module, name);
            if let Some(target) = self.shared_ctx.type_aliases.get(&qualified) {
                return Some(target.clone());
            }
        }

        self.shared_ctx.type_aliases.get(name).cloned()
    }

    /// Whether an alias-table entry for a builtin spelling is also a lexical
    /// binding in the current module. The general alias resolver intentionally
    /// retains its legacy global short-key fallback; authority checks must not
    /// mistake that storage fallback for visibility (Issue #11419).
    fn builtin_type_alias_binding_is_visible(&self, name: &str) -> bool {
        if matches!(
            self.active_imported_bindings.get(name),
            Some(ImportedBindingResolution::Resolved { .. })
        ) {
            return true;
        }
        if self.current_module_path.as_ref().is_some_and(|module| {
            self.shared_ctx
                .type_aliases
                .contains_key(&format!("{module}.{name}"))
        }) {
            return true;
        }
        if self.in_base_function_scope
            || self
                .current_module_path
                .as_deref()
                .is_some_and(crate::module_names::is_base)
        {
            return self.shared_ctx.type_aliases.contains_key(name);
        }
        if !self.current_module_is_bare
            && builtin_type_binding_authority(name) == Some(BuiltinTypeBindingAuthority::Base)
            && crate::julia::base::is_exported(name)
        {
            return self.shared_ctx.type_aliases.contains_key(name);
        }
        self.current_module_path.is_none() && self.shared_ctx.type_aliases.contains_key(name)
    }

    /// Whether the canonical Core/Base owner makes a builtin spelling
    /// lexically visible in this scope, independent of whether that type is
    /// represented by a materialized type object or by a compiler projection.
    fn builtin_type_owner_is_visible(
        &self,
        name: &str,
        authority: BuiltinTypeBindingAuthority,
    ) -> bool {
        match authority {
            BuiltinTypeBindingAuthority::Core => true,
            BuiltinTypeBindingAuthority::Base => {
                self.in_base_function_scope
                    || self
                        .current_module_path
                        .as_deref()
                        .is_some_and(crate::module_names::is_base)
                    || self.current_module_path.is_none()
                    || (!self.current_module_is_bare && crate::julia::base::is_exported(name))
                    || matches!(
                        self.active_imported_bindings.get(name),
                        Some(ImportedBindingResolution::Resolved {
                            source_module,
                            source_name: Some(source_name),
                            ..
                        }) if crate::module_names::is_base(source_module)
                            && source_name == name
                    )
            }
        }
    }

    fn first_hidden_builtin_type_expr_binding(&self, expr: &TypeExpr) -> Option<String> {
        match expr {
            TypeExpr::Concrete(ty) => self.first_hidden_builtin_type_binding(ty.name().as_ref()),
            TypeExpr::TypeVar(name) => self.first_hidden_builtin_type_binding(name),
            TypeExpr::Parameterized { base, params } => {
                self.first_hidden_builtin_type_binding(base).or_else(|| {
                    params
                        .iter()
                        .find_map(|param| self.first_hidden_builtin_type_expr_binding(param))
                })
            }
            TypeExpr::RuntimeExpr(_) => None,
        }
    }

    /// Return the first Core/Base-owned builtin name whose lexical binding is
    /// absent from the current module. Grammar-only containers such as
    /// `Union{...}` have no binding authority entry, but their type arguments
    /// are still checked recursively (Issue #11419).
    pub(super) fn first_hidden_builtin_type_binding(&self, name: &str) -> Option<String> {
        if let Some((base, params)) = types::parse_parametric_call(name) {
            return self.first_hidden_builtin_type_binding(&base).or_else(|| {
                params
                    .iter()
                    .find_map(|param| self.first_hidden_builtin_type_expr_binding(param))
            });
        }
        let authority = builtin_type_binding_authority(name)?;
        // Some compiler-projected builtins (for example Core.Nothing and
        // Base.Regex) do not need a materialized type-object table entry.
        // Lexical authority therefore cannot depend on storage representation.
        if self.builtin_type_owner_is_visible(name, authority) {
            return None;
        }
        (!self.builtin_type_alias_binding_is_visible(name)
            && self.resolve_visible_type_object_name(name).is_none())
        .then(|| name.to_string())
    }

    pub(super) fn resolve_visible_type_alias(&self, name: &str) -> Option<String> {
        if let Some(target) = self.resolve_visible_type_alias_exact(name) {
            return Some(target);
        }

        let (base, params) = types::parse_parametric_call(name)?;
        let target = self.resolve_visible_type_alias_exact(&base)?;
        let rendered_params = params
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        Some(format!("{target}{{{rendered_params}}}"))
    }

    /// Whether some statically collected module has `name` as its short name.
    ///
    /// The registry is intentionally global, while module bindings are
    /// lexical.  This predicate is used only after lexical resolution failed,
    /// so it diagnoses a known-but-unbound nested module instead of letting the
    /// short name fall through to `LoadAny` (Issue #11132/#11176).
    pub(super) fn is_known_module_short_name(&self, name: &str) -> bool {
        let suffix = format!(".{name}");
        self.module_functions
            .keys()
            .chain(self.module_exports.keys())
            .any(|module| module == name || module.ends_with(&suffix))
    }

    /// Collect every method belonging to an imported generic function, including
    /// methods defined by modules that imported and extended that binding. Those
    /// extensions are registered under the importing module's qualified table
    /// (`Symbolics.det`) while `module_imported_bindings` records that the binding
    /// is the provider's generic (`LinearAlgebra.det`). A runtime function value
    /// must carry both tables; otherwise dynamic calls skip extension methods and
    /// fall through to the provider's generic fallback (Issue #11216).
    pub(super) fn imported_generic_candidate_indices(&self, lookup_name: &str) -> Vec<usize> {
        let canonical = self
            .resolve_reexport_chain(lookup_name)
            .unwrap_or_else(|| lookup_name.to_string());
        let mut table_names = BTreeSet::from([lookup_name.to_string(), canonical.clone()]);
        for alias in self.shared_ctx.module_imported_bindings.keys() {
            if self.resolve_reexport_chain(alias).as_deref() == Some(canonical.as_str()) {
                table_names.insert(alias.clone());
            }
        }

        table_names
            .iter()
            .filter_map(|name| self.method_tables.get(name))
            .flat_map(|table| table.methods.iter().map(|method| method.global_index))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    /// Follow a selective-import/re-export chain to its ultimate qualified
    /// source. Returns `None` when `qualified_name` is not an imported binding or
    /// when malformed metadata contains a cycle (Issue #8053).
    pub(super) fn resolve_reexport_chain(&self, qualified_name: &str) -> Option<String> {
        resolve_import_binding_chain(&self.shared_ctx.module_imported_bindings, qualified_name)
    }

    /// Choose the runtime type identity name for a resolved function value
    /// (Issue #11088). See `emit_function_value_named`'s doc comment for the
    /// full rationale.
    pub(super) fn function_value_identity_name(
        &self,
        display_name: &str,
        lookup_name: &str,
    ) -> String {
        // The owning module implied by this access: `lookup_name` itself,
        // minus the trailing `.{display_name}`, when it is already
        // qualified; otherwise the module (if exactly one) that a `using`
        // actually resolves `display_name` to.
        let owner: Option<&str> = if display_name == lookup_name {
            self.unique_using_owner(display_name)
        } else {
            lookup_name.strip_suffix(&format!(".{}", display_name))
        };
        let Some(owner) = owner else {
            // No module ownership could be established at all (an ordinary
            // top-level/Main-scoped function) — nothing to disambiguate.
            return display_name.to_string();
        };

        // `display_name` denotes THIS SAME owner unambiguously through a
        // real `using` — the bare spelling is safe for both access paths
        // (Issue #10077's invariant), regardless of any OTHER, unrelated
        // module that also happens to declare the same bare name but was
        // never brought into scope.
        if self.unique_using_owner(display_name) == Some(owner) {
            return display_name.to_string();
        }

        // Otherwise, look for another module's qualified table for the same
        // bare name (`"M2x.f"` when this access's owner is `"M1x"`). This
        // checks table KEY existence, not the shared bare table's method
        // LIST — the list can have one sibling's same-signature entry
        // evicted by the other's (Issue #8079's dedup), which would make a
        // candidate-set comparison miss the collision for whichever sibling
        // compiled second.
        let suffix = format!(".{}", display_name);
        let sibling_declaration_exists = self
            .method_tables
            .keys()
            .any(|key| key != lookup_name && key.ends_with(&suffix));
        if sibling_declaration_exists {
            // Some OTHER declaration also shares the bare name (a sibling
            // module's same-named function, most commonly) — keep the
            // qualified spelling so the two remain distinct runtime types.
            format!("{}.{}", owner, display_name)
        } else {
            display_name.to_string()
        }
    }
}
