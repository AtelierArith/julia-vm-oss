//! Named pipeline phases for `compile_core_program_internal` (Issue #6333).
//!
//! Mirrors upstream Julia's `run_passes_ipo_safe` (julia/Compiler/src/optimize.jl)
//! where the compilation pipeline is an explicit sequence of named phases.

use super::*;

/// Named result of `compile_core_program_internal`, replacing the previous
/// 4-element tuple (Issue #6333).
#[derive(Debug)]
pub(crate) struct CoreCompileOutput {
    pub compiled: CompiledProgram,
    pub method_tables: HashMap<String, MethodTable>,
    pub closure_captures: HashMap<String, HashSet<String>>,
    pub inference_results: Vec<(InferenceCacheKey, CachedReturn)>,
}

fn apply_peephole_index_mapping(
    function_infos: &mut [FunctionInfo],
    entry: usize,
    index_mapping: &[usize],
    reused_base: &[bool],
) -> usize {
    for (idx, func_info) in function_infos.iter_mut().enumerate() {
        if reused_base.get(idx).copied().unwrap_or(false) {
            continue;
        }
        if func_info.code_start < index_mapping.len() {
            func_info.code_start = index_mapping[func_info.code_start];
        }
        if func_info.code_end < index_mapping.len() {
            func_info.code_end = index_mapping[func_info.code_end];
        }
        if func_info.entry < index_mapping.len() {
            func_info.entry = index_mapping[func_info.entry];
        }
    }

    if entry < index_mapping.len() {
        index_mapping[entry]
    } else {
        entry
    }
}

/// Struct tables built by [`CorePipeline::build_struct_tables`] and consumed
/// by [`CorePipeline::init_shared_context`] when creating the shared
/// compilation context.
struct StructTables {
    struct_table: HashMap<String, StructInfo>,
    parametric_structs: HashMap<String, ParametricStructDef>,
    struct_defs: Vec<StructDefInfo>,
    next_type_id: usize,
    cached_instantiation_table: HashMap<InstantiationKey, usize>,
}

/// An inner constructor collected from a struct definition. Registered with
/// the struct name, allowing `Point(x, y)` to call the inner constructor.
struct InnerCtorInfo {
    struct_name: String,
    type_id: usize,
    ctor: crate::ir::core::InnerConstructor,
    func_info_idx: usize, // Index in function_infos where this ctor is registered
    /// Dotted path of the module that defines this struct (`None` at top level).
    /// The constructor body's name lookups must be resolved in this defining
    /// module so a module-private helper/type/const is visible without the
    /// caller doing `using .Mod` (Issue #8069).
    module_path: Option<String>,
}

fn compact_type_name(name: &str) -> String {
    name.chars().filter(|c| !c.is_whitespace()).collect()
}

fn type_alias_runtime_target(alias: &crate::ir::core::TypeAliasDef) -> String {
    if alias.params.is_empty() {
        alias.target_type.clone()
    } else {
        match alias.target_type.split_once('{') {
            Some((base, _)) => base.trim().to_string(),
            None => alias.target_type.clone(),
        }
    }
}

fn type_expr_contains_type_param(expr: &TypeExpr, type_param_names: &HashSet<&str>) -> bool {
    match expr {
        TypeExpr::TypeVar(name) => type_param_names.contains(name.as_str()),
        TypeExpr::Parameterized { params, .. } => params
            .iter()
            .any(|param| type_expr_contains_type_param(param, type_param_names)),
        TypeExpr::Concrete(_) | TypeExpr::RuntimeExpr(_) => false,
    }
}

fn register_type_alias(
    shared_ctx: &mut SharedCompileContext,
    alias: &crate::ir::core::TypeAliasDef,
) {
    shared_ctx
        .type_aliases
        .insert(alias.name.clone(), type_alias_runtime_target(alias));
}

fn register_module_type_aliases(
    shared_ctx: &mut SharedCompileContext,
    module: &crate::ir::core::Module,
    prefix: &str,
) {
    let module_path = if prefix.is_empty() {
        module.name.clone()
    } else {
        format!("{}.{}", prefix, module.name)
    };

    for alias in &module.type_aliases {
        let target = type_alias_runtime_target(alias);
        shared_ctx
            .type_aliases
            .insert(format!("{}.{}", module_path, alias.name), target);
    }

    for submodule in &module.submodules {
        register_module_type_aliases(shared_ctx, submodule, &module_path);
    }
}

fn module_imports_base_symbol(
    module_path: Option<&String>,
    module_usings_map: &HashMap<String, Vec<UsingImport>>,
    symbol: &str,
) -> bool {
    let Some(path) = module_path else {
        return false;
    };
    module_usings_map.get(path).is_some_and(|usings| {
        usings.iter().any(|using_import| {
            !using_import.is_relative
                && using_import.module == "Base"
                && using_import
                    .symbols
                    .as_ref()
                    .is_some_and(|symbols| symbols.iter().any(|imported| imported == symbol))
        })
    })
}

fn should_defer_module_return_inference(
    func: &Function,
    module_path: Option<&String>,
    is_base_function: bool,
) -> bool {
    if is_base_function || func.return_type.is_some() {
        return false;
    }
    let Some(path) = module_path else {
        return false;
    };
    path != "Core"
        && !path.starts_with("Core.")
        && path != "Base"
        && !path.starts_with("Base.")
        && path != "Main"
        && !path.starts_with("Main.")
}

/// Pipeline state threaded between the named phases of
/// `compile_core_program_internal` (Issue #6333). Borrowed fields point at the
/// merged/optimized source IR prepared by the source phases; owned fields are
/// the tables and bytecode accumulated by the build/compile phases and finally
/// consumed by [`CorePipeline::finalize`].
struct CorePipeline<'a> {
    // Source IR (merged Base + user, after inline/optimize passes)
    program: &'a Program,
    opt_user_functions: &'a [Function],
    opt_modules: &'a [crate::ir::core::Module],
    opt_main: &'a Block,
    /// Program modules chained with `using`-loaded stdlib modules.
    all_modules: Vec<&'a crate::ir::core::Module>,
    /// Inline (nested) functions with their parent function names.
    inline_functions: &'a [(Function, Option<String>)],
    base_function_count: usize,
    // REPL session globals (resolved once struct tables are built)
    global_types: &'a HashMap<String, ValueType>,
    global_struct_names: &'a HashMap<String, String>,
    // Optional cache inputs (Issue #2933)
    precompiled_base: Option<&'a CompiledProgram>,
    cached_method_tables: Option<&'a HashMap<String, MethodTable>>,
    cached_closure_captures: Option<&'a HashMap<String, HashSet<String>>>,
    cached_inference_results: Option<&'a [(InferenceCacheKey, CachedReturn)]>,
    // Type definitions
    all_structs: Vec<(&'a crate::ir::core::StructDef, Option<String>)>,
    module_struct_names: HashMap<String, HashSet<String>>,
    abstract_types: Vec<AbstractTypeDefInfo>,
    abstract_type_names: HashSet<String>,
    abstract_type_parents: HashMap<String, Option<String>>,
    primitive_types: Vec<PrimitiveTypeDefInfo>,
    shared_ctx: SharedCompileContext,
    // Pending REPL globals, resolved after struct_table is built
    pending_global_types: HashMap<String, ValueType>,
    pending_global_struct_names: HashMap<String, String>,
    // Method tables and function metadata
    method_tables: HashMap<String, MethodTable>,
    function_infos: Vec<FunctionInfo>,
    global_index: usize,
    cached_base_len: usize,
    /// Maps all_functions index -> function_infos index.
    func_index_map: Vec<usize>,
    show_methods: Vec<ShowMethodEntry>,
    /// Lazy AoT: functions that need specialization.
    specializable_functions: Vec<SpecializableFunction>,
    // Module metadata
    module_functions: HashMap<String, HashSet<String>>,
    module_exports: HashMap<String, HashSet<String>>,
    /// Module-level constants (variables assigned in module body).
    module_constants: HashMap<String, HashSet<String>>,
    imported_functions: HashSet<String>,
    usings_set: HashSet<String>,
    module_imports_map: HashMap<String, HashSet<String>>,
    module_usings_map: HashMap<String, Vec<UsingImport>>,
    /// Top-level selective-import name -> source module(s)
    /// (`import M: f` / `using M: f`). A later top-level `function f(...)` extends
    /// `M.f`, so its method must also join the `M.f` table — not just `f`
    /// (Issue #8052).
    toplevel_import_sources: HashMap<String, Vec<String>>,
    // Function universe (Base + module + user + inline functions)
    base_function_names: HashSet<String>,
    user_function_names: HashSet<String>,
    all_functions: Vec<(&'a Function, Option<String>)>,
    first_user_function_idx: usize,
    inline_start_idx: usize,
    func_idx_to_parent: HashMap<usize, String>,
    callable_typeof_aliases: HashMap<String, String>,
    // Inference bookkeeping
    has_seeded_inference_results: bool,
    shadowed_user_globals: HashSet<String>,
    // Code generation state
    inner_ctors: Vec<InnerCtorInfo>,
    code: Vec<Instr>,
    reused_base: Vec<bool>,
    base_main_entry: Option<usize>,
    deferred_shadowed_global_types: Vec<(String, Option<ValueType>)>,
    modules_entry: usize,
    entry: usize,
}

/// Internal compilation with optional precompiled Base cache and method tables.
/// Returns a [`CoreCompileOutput`] carrying the compiled program plus the
/// method tables, closure captures, and inference results for caching.
pub(crate) fn compile_core_program_internal(
    program: &Program,
    global_types: &HashMap<String, ValueType>,
    global_struct_names: &HashMap<String, String>,
    cache_input: CompilerCacheInput<'_>,
) -> CResult<CoreCompileOutput> {
    let (program_ref, base_function_count) = merge_precompiled_base(program);
    let (inlined_program, optimized_user_segment) =
        inline_and_optimize_ir(program_ref.as_ref(), base_function_count);
    // The user-only optimization pass rewrites just user functions, modules,
    // and main; everything else (Base function prefix, structs, abstract
    // types, usings, ...) is read from the unmodified input program so the
    // Base IR is never deep-cloned per run (Issue #6348).
    let program = inlined_program.as_ref();
    let opt_user_functions: &Vec<Function> = &optimized_user_segment.user_functions;
    let opt_modules: &Vec<crate::ir::core::Module> = &optimized_user_segment.modules;
    let opt_main: &Block = &optimized_user_segment.main;

    let loaded_modules = load_stdlib_modules(program, opt_modules);

    // Combine program modules with loaded stdlib modules
    let all_modules: Vec<&crate::ir::core::Module> =
        opt_modules.iter().chain(loaded_modules.iter()).collect();

    // Collect inline functions from top-level statements (with parent function tracking)
    // inline_functions: Vec<(Function, Option<parent_func_name>)>
    let inline_functions: Vec<(Function, Option<String>)> = collect_top_level_inline_functions(
        program,
        base_function_count,
        opt_user_functions,
        opt_main,
        &all_modules,
    );

    let mut p = CorePipeline::new(
        program,
        opt_user_functions,
        opt_modules,
        opt_main,
        all_modules,
        &inline_functions,
        base_function_count,
        global_types,
        global_struct_names,
        cache_input,
    );

    let struct_tables = p.build_struct_tables();
    p.init_shared_context(struct_tables);
    p.seed_outputs_from_cache();

    let method_table_setup_timer = profile::start("compile.method_table_setup");
    profile::time("compile.collect_module_metadata", || {
        p.collect_module_metadata()
    });
    p.validate_using_imports()?;
    profile::time("compile.build_function_universe", || {
        p.build_function_universe()
    });
    profile::time("compile.prepopulate_closure_captures", || {
        p.prepopulate_closure_captures()
    });
    profile::time("compile.preinstantiate_parametric_types", || {
        p.preinstantiate_parametric_types()
    });
    profile::time("compile.resolve_global_types", || p.resolve_global_types());
    profile::time("compile.resolve_module_imports", || {
        p.resolve_module_imports()
    });
    let mut inference_engine = profile::time("compile.build_inference_engine", || {
        p.build_inference_engine()
    });
    profile::time("compile.build_method_tables", || {
        p.build_method_tables(&mut inference_engine)
    });
    profile::finish(method_table_setup_timer);

    p.register_inner_constructors(&mut inference_engine);
    p.project_method_table_hierarchy();
    p.analyze_module_lambda_captures();

    p.compile_functions()?;
    p.compile_inner_constructors()?;
    p.compile_base_main_prefix()?;
    p.compile_modules()?;
    p.compile_main()?;
    p.finalize(&inference_engine)
}

impl<'a> CorePipeline<'a> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        program: &'a Program,
        opt_user_functions: &'a [Function],
        opt_modules: &'a [crate::ir::core::Module],
        opt_main: &'a Block,
        all_modules: Vec<&'a crate::ir::core::Module>,
        inline_functions: &'a [(Function, Option<String>)],
        base_function_count: usize,
        global_types: &'a HashMap<String, ValueType>,
        global_struct_names: &'a HashMap<String, String>,
        cache_input: CompilerCacheInput<'a>,
    ) -> Self {
        let CompilerCacheInput {
            precompiled_base,
            method_tables: cached_method_tables,
            closure_captures: cached_closure_captures,
            inference_results: cached_inference_results,
        } = cache_input;

        CorePipeline {
            program,
            opt_user_functions,
            opt_modules,
            opt_main,
            all_modules,
            inline_functions,
            base_function_count,
            global_types,
            global_struct_names,
            precompiled_base,
            cached_method_tables,
            cached_closure_captures,
            cached_inference_results,
            all_structs: Vec::new(),
            module_struct_names: HashMap::new(),
            abstract_types: Vec::new(),
            abstract_type_names: HashSet::new(),
            abstract_type_parents: HashMap::new(),
            primitive_types: Vec::new(),
            shared_ctx: SharedCompileContext::new(
                HashMap::new(),
                Vec::new(),
                HashMap::new(),
                Vec::new(),
                0,
            ),
            pending_global_types: HashMap::new(),
            pending_global_struct_names: HashMap::new(),
            method_tables: HashMap::new(),
            function_infos: Vec::new(),
            global_index: 0,
            cached_base_len: 0,
            func_index_map: Vec::new(),
            show_methods: Vec::new(),
            specializable_functions: Vec::new(),
            module_functions: HashMap::new(),
            module_exports: HashMap::new(),
            module_constants: HashMap::new(),
            imported_functions: HashSet::new(),
            usings_set: HashSet::new(),
            module_imports_map: HashMap::new(),
            module_usings_map: HashMap::new(),
            toplevel_import_sources: HashMap::new(),
            base_function_names: HashSet::new(),
            user_function_names: HashSet::new(),
            all_functions: Vec::new(),
            first_user_function_idx: 0,
            inline_start_idx: 0,
            func_idx_to_parent: HashMap::new(),
            callable_typeof_aliases: HashMap::new(),
            has_seeded_inference_results: false,
            shadowed_user_globals: HashSet::new(),
            inner_ctors: Vec::new(),
            code: Vec::new(),
            reused_base: Vec::new(),
            base_main_entry: None,
            deferred_shadowed_global_types: Vec::new(),
            modules_entry: 0,
            entry: 0,
        }
    }

    fn build_struct_tables(&mut self) -> StructTables {
        let program = self.program;
        let opt_modules = self.opt_modules;
        let precompiled_base = self.precompiled_base;
        let all_modules = &self.all_modules;

        // Build struct table from struct definitions
        // Separate parametric structs from concrete structs
        let mut struct_table: HashMap<String, StructInfo> = HashMap::new();
        let mut parametric_structs: HashMap<String, ParametricStructDef> = HashMap::new();

        // When using cache, initialize struct_defs from cached base to maintain consistent type_ids.
        // This is critical because cached bytecode contains NewStruct instructions with type_ids
        // that must match the struct_defs indices.
        //
        // Also build instantiation_table for parametric instantiations like Complex{Float64}
        // to prevent re-instantiation with different type_ids.
        let mut cached_instantiation_table: HashMap<InstantiationKey, usize> = HashMap::new();
        let (mut struct_defs, mut next_type_id): (Vec<StructDefInfo>, usize) =
            profile::time("compile.cached_struct_defs_init", || {
                if let Some(base_cache) = precompiled_base {
                    let cached_len = base_cache.struct_defs.len();
                    // Also rebuild struct_table for cached structs so we can look them up
                    for (idx, def) in base_cache.struct_defs.iter().enumerate() {
                        struct_table.insert(
                            def.name.clone(),
                            StructInfo {
                                type_id: idx,
                                is_mutable: def.is_mutable,
                                fields: def.fields.clone(),
                                // Base structs with inner constructors are already compiled;
                                // the method_tables cache handles their dispatch.
                                has_inner_constructor: false,
                            },
                        );

                        // For parametric instantiations like "Complex{Float64}", build instantiation_table entry
                        if let Some(brace_idx) = def.name.find('{') {
                            let base_name = def.name[..brace_idx].to_string();
                            let type_args_str = &def.name[brace_idx + 1..def.name.len() - 1];
                            let Some(type_args) = parse_type_args_recursive(type_args_str) else {
                                continue;
                            };
                            let key = InstantiationKey {
                                base_name,
                                type_args,
                            };
                            cached_instantiation_table.insert(key, idx);
                        }
                    }
                    (base_cache.struct_defs.clone(), cached_len)
                } else {
                    (Vec::new(), 0)
                }
            });

        // Collect all structs: top-level (None) + module structs (Some(module_path))
        let mut all_structs: Vec<(&crate::ir::core::StructDef, Option<String>)> =
            program.structs.iter().map(|s| (s, None)).collect();

        for module in all_modules {
            let mut module_structs = Vec::new();
            collect_module_structs(module, "", &mut module_structs);
            for (struct_def, module_path) in module_structs {
                all_structs.push((struct_def, Some(module_path)));
            }
        }

        // Build a map of module_path -> set of struct names defined in that module.
        // This is used to qualify struct type names in function parameters for module functions.
        let mut module_struct_names: HashMap<String, HashSet<String>> = HashMap::new();
        for (struct_def, module_path) in &all_structs {
            if let Some(path) = module_path {
                module_struct_names
                    .entry(path.clone())
                    .or_default()
                    .insert(struct_def.name.clone());
            }
        }

        // Process all structs (top-level and module structs)
        let struct_tables_build_timer = profile::start("compile.struct_tables_build");
        for (struct_def, module_path) in &all_structs {
            // Determine the struct name (qualified for module structs)
            let struct_name = match module_path {
                Some(path) => format!("{}.{}", path, struct_def.name),
                None => struct_def.name.clone(),
            };

            // When using cache, skip Base structs that are already registered.
            // This prevents re-assigning type_ids and breaking cached bytecode.
            if precompiled_base.is_some() && struct_table.contains_key(&struct_name) {
                // For parametric structs, still register them in parametric_structs
                // but don't modify struct_table or struct_defs
                if struct_def.is_parametric() {
                    parametric_structs.insert(
                        struct_name.clone(),
                        ParametricStructDef {
                            def: (*struct_def).clone(),
                        },
                    );
                }
                continue;
            }

            if struct_def.is_parametric() {
                // Store parametric struct definition for later instantiation
                // All parametric structs (including Complex) are handled the same way
                parametric_structs.insert(
                    struct_name.clone(),
                    ParametricStructDef {
                        def: (*struct_def).clone(),
                    },
                );
                // Also register with short name for module structs
                // This allows `Point(...)` syntax after `using .MyGeometry`
                if module_path.is_some() {
                    parametric_structs.insert(
                        struct_def.name.clone(),
                        ParametricStructDef {
                            def: (*struct_def).clone(),
                        },
                    );
                }
            } else {
                // Concrete struct - register immediately with sequential type_id
                let type_id = next_type_id;
                next_type_id += 1;

                let fields: Vec<(String, ValueType)> = struct_def
                    .fields
                    .iter()
                    .map(|f| {
                        // Issue #4856: `StructField::as_julia_type` only returns
                        // `Some` for `TypeExpr::Concrete`, so a user-struct-typed
                        // field like `inner::InnerProbe` (parsed as
                        // `TypeExpr::Named`) was falling through to
                        // `ValueType::Any`. As a result the inference engine saw
                        // `OuterProbe.inner` as `Any`, and `x.inner.value` widened
                        // to `Any` because the nested struct identity was lost on
                        // the way into the lattice struct table. Resolve any
                        // typed field through `TypeExpr::to_julia_type_lossy`
                        // so struct-typed fields land as
                        // `ValueType::Struct(type_id)` whenever the field's
                        // struct is already registered in `struct_table`.
                        let jt = f
                            .as_julia_type()
                            .or_else(|| f.type_expr.as_ref().map(TypeExpr::to_julia_type_lossy));
                        let vt = jt
                            .as_ref()
                            .map(|jt| julia_type_to_value_type_with_table(jt, &struct_table))
                            .unwrap_or(ValueType::Any); // Untyped fields are Any (Julia semantics)
                                                        // Issue #5125: the reflection `Method` struct exposes a
                                                        // `.module::Module` field, but `module` is a reserved keyword
                                                        // the parser cannot accept as a field name, so the pure-Julia
                                                        // definition declares it as `mod`. Canonicalize it to
                                                        // `module` here (once, at the single field-table build site)
                                                        // so `m.module` field access resolves through `struct_table`
                                                        // and `struct_defs` consistently and `fieldnames(Method)`
                                                        // reports `:module`, matching upstream.
                        let field_name = if struct_name == "Method" && f.name == "mod" {
                            "module".to_string()
                        } else {
                            f.name.clone()
                        };
                        (field_name, vt)
                    })
                    .collect();
                let field_julia_types: Vec<JuliaType> = struct_def
                    .fields
                    .iter()
                    .map(|f| {
                        f.as_julia_type()
                            .or_else(|| f.type_expr.as_ref().map(TypeExpr::to_julia_type_lossy))
                            .unwrap_or(JuliaType::Any)
                    })
                    .collect();

                let has_inner_ctor = !struct_def.inner_constructors.is_empty();
                struct_table.insert(
                    struct_name.clone(),
                    StructInfo {
                        type_id,
                        is_mutable: struct_def.is_mutable,
                        fields: fields.clone(),
                        has_inner_constructor: has_inner_ctor,
                    },
                );

                // Also register with short name for module structs
                if module_path.is_some() {
                    struct_table.insert(
                        struct_def.name.clone(),
                        StructInfo {
                            type_id,
                            is_mutable: struct_def.is_mutable,
                            fields: fields.clone(),
                            has_inner_constructor: has_inner_ctor,
                        },
                    );
                }

                // Push to struct_defs for all structs
                // Complex is already at index 0, so update it; others get new indices
                if struct_def.name == "Complex" {
                    // Update the placeholder at index 0 with actual definition
                    // Use "Complex{Float64}" as the name for proper runtime dispatch matching
                    // Methods like +(::Real, ::Complex{Float64}) need to match correctly
                    struct_defs[0] = StructDefInfo {
                        name: "Complex{Float64}".to_string(),
                        is_mutable: struct_def.is_mutable,
                        fields,
                        field_julia_types,
                        parent_type: struct_def.parent_type.clone(),
                    };
                } else {
                    struct_defs.push(StructDefInfo {
                        name: struct_name,
                        is_mutable: struct_def.is_mutable,
                        fields,
                        field_julia_types,
                        parent_type: struct_def.parent_type.clone(),
                    });
                }
            }
        }
        profile::finish(struct_tables_build_timer);

        // Build abstract type definitions (Issue #2523: preserve type_params at runtime).
        // Abstract types declared inside modules / bundled packages (Issues #7263 /
        // #7265) live only on `Module.abstract_types`; collect them alongside the
        // top-level ones so a module-local abstract annotation (`f(d::Distribution)`)
        // resolves to the abstract type instead of a concrete `Struct("Distribution")`
        // that no value satisfies.
        let mut all_abstract_type_defs: Vec<crate::ir::core::AbstractTypeDef> =
            program.abstract_types.clone();
        collect_module_abstract_types(opt_modules, &mut all_abstract_type_defs);

        let abstract_types: Vec<AbstractTypeDefInfo> = all_abstract_type_defs
            .iter()
            .map(|at| AbstractTypeDefInfo {
                name: at.name.clone(),
                parent: at.parent.clone(),
                type_params: at.type_params.iter().map(|tp| tp.name.clone()).collect(),
            })
            .collect();

        // Build set of abstract type names for compiler
        let abstract_type_names: HashSet<String> = all_abstract_type_defs
            .iter()
            .map(|at| at.name.clone())
            .collect();

        // Build user-declared primitive types (`primitive type Name Bits end`, Issue #5058).
        // These carry the declared bit width and optional abstract supertype so the
        // runtime type-reflection layer can answer isprimitivetype/isbitstype/sizeof/
        // supertype/<: for them. Modules can also declare primitive types, so collect
        // those too.
        let mut primitive_types: Vec<PrimitiveTypeDefInfo> = program
            .primitive_types
            .iter()
            .map(|pt| PrimitiveTypeDefInfo {
                name: pt.name.clone(),
                parent: pt.parent.clone(),
                bits: pt.bits,
            })
            .collect();
        collect_module_primitive_types(opt_modules, &mut primitive_types);

        self.all_structs = all_structs;
        self.module_struct_names = module_struct_names;
        self.abstract_types = abstract_types;
        self.abstract_type_names = abstract_type_names;
        self.primitive_types = primitive_types;
        StructTables {
            struct_table,
            parametric_structs,
            struct_defs,
            next_type_id,
            cached_instantiation_table,
        }
    }

    fn init_shared_context(&mut self, tables: StructTables) {
        let StructTables {
            struct_table,
            parametric_structs,
            struct_defs,
            next_type_id,
            cached_instantiation_table,
        } = tables;
        let program = self.program;
        let opt_modules = self.opt_modules;
        let opt_main = self.opt_main;
        let all_modules = &self.all_modules;
        let global_types = self.global_types;
        let global_struct_names = self.global_struct_names;
        let cached_closure_captures = self.cached_closure_captures;
        let abstract_types = &self.abstract_types;
        let primitive_types = &self.primitive_types;

        // Create shared compilation context
        // When using cache, pass the cached instantiation table to prevent re-instantiation
        let shared_ctx_init_timer = profile::start("compile.shared_ctx_init");
        self.shared_ctx = if !cached_instantiation_table.is_empty() {
            SharedCompileContext::with_instantiation_table(
                struct_table,
                struct_defs,
                parametric_structs,
                abstract_types.clone(),
                next_type_id,
                cached_instantiation_table,
            )
        } else {
            SharedCompileContext::new(
                struct_table,
                struct_defs,
                parametric_structs,
                abstract_types.clone(),
                next_type_id,
            )
        };
        let shared_ctx = &mut self.shared_ctx;

        // `@enum` pre-pass (Issue #5139): collect every enum definition up front so
        // that bare references to an enum type name or its members resolve no matter
        // where they appear relative to the `@enum`, and so call sites can recognize
        // `Color(value)` construction and `instances(Color)`. Enum defs lower to
        // `Stmt::EnumDef` inside `main` (or a module body), so scan blocks directly.
        collect_enum_types(opt_main, &mut shared_ctx.enum_types);
        for module in opt_modules {
            collect_enum_types_in_module(module, &mut shared_ctx.enum_types);
        }

        // Register user-declared primitive types so bare references resolve to a
        // DataType value and type reflection can answer isprimitivetype/sizeof/
        // supertype (Issue #5058).
        shared_ctx.set_primitive_types(primitive_types.clone());

        // Populate type aliases from program. A *bare* reference to a parametric
        // alias (`MyVec` for `MyVec{T} = Vector{T}`) resolves to the target's bare
        // base type (`Vector`), matching upstream which prints/compares the alias as
        // the underlying `UnionAll`. Parametric *uses* (`MyVec{Int}`) are expanded
        // during lowering instead (Issue #5055).
        //
        // Base-level const type aliases (e.g. `const Bottom = Union{}` from
        // essentials.jl) live in the prelude program, which is independent of the
        // base bytecode cache. Register them FIRST so bare references like `Bottom`
        // resolve as DataType values; user aliases of the same name register
        // afterwards and override (later definition wins, matching upstream).
        // Without this, base const aliases were dropped on the cached-base path,
        // leaving `Bottom` unresolved even though `Union{}` worked everywhere
        // (Issue #5065).
        if let Some(prelude) = crate::get_prelude_program() {
            for alias in &prelude.type_aliases {
                register_type_alias(shared_ctx, alias);
            }
        }
        for alias in &program.type_aliases {
            register_type_alias(shared_ctx, alias);
        }
        for module in all_modules {
            register_module_type_aliases(shared_ctx, module, "");
        }

        // Pre-populate closure captures from cache (Issue #2100)
        // When using the compilation cache, outer Base functions are skipped (cached bytecode).
        // But their inner/nested functions still need to be compiled, and they reference
        // captured variables from the outer scope. Without this, those inner functions
        // would get empty closure_captures and fail with "Undefined variable" errors.
        if let Some(cached_captures) = cached_closure_captures {
            shared_ctx.closure_captures = cached_captures.clone();
        }

        profile::finish(shared_ctx_init_timer);

        // Store global_types temporarily - will resolve after struct_table is built
        self.pending_global_types = global_types.clone();
        self.pending_global_struct_names = global_struct_names.clone();
    }

    fn seed_outputs_from_cache(&mut self) {
        let precompiled_base = self.precompiled_base;
        let cached_method_tables = self.cached_method_tables;

        // Build method tables from functions (including module functions)
        // Start with cached Base method tables if available (Option A optimization)
        self.method_tables = profile::time("compile.cached_method_tables_clone", || {
            if let Some(cached) = cached_method_tables {
                cached
                    .iter()
                    .map(|(name, table)| (name.clone(), table.clone_for_reprojection()))
                    .collect()
            } else {
                HashMap::new()
            }
        });

        // When using cache, initialize function_infos from cache to maintain consistent indices.
        // This is critical because cached bytecode contains Call instructions with indices that
        // must match function_infos. User functions are appended at the end.
        //
        // func_index_map: maps all_functions index -> function_infos index
        // - For Base functions (when using cache): identity mapping (0->0, 1->1, etc.)
        // - For user functions: maps to end of cache (e.g., 678->682 if cache has 682 entries)
        let (function_infos, global_index, cached_base_len): (Vec<FunctionInfo>, usize, usize) =
            profile::time("compile.cached_function_infos_clone", || {
                if let Some(base_cache) = precompiled_base {
                    let len = base_cache.functions.len();
                    (base_cache.functions.clone(), len, len)
                } else {
                    (Vec::new(), 0, 0)
                }
            });
        self.function_infos = function_infos;
        self.global_index = global_index;
        self.cached_base_len = cached_base_len;
        // When using cache, initialize show_methods from cached Base (Issue #2489).
        // Base show methods (e.g., show(io, Complex)) are skipped during the function loop
        // when using cache, so they must be pre-populated from the cached compilation.
        self.show_methods = profile::time("compile.cached_show_methods_clone", || {
            if let Some(base_cache) = precompiled_base {
                base_cache.show_methods.clone()
            } else {
                Vec::new()
            }
        });
    }

    fn collect_module_metadata(&mut self) {
        let program = self.program;
        let base_function_count = self.base_function_count;
        let opt_user_functions = self.opt_user_functions;
        let all_modules = &self.all_modules;
        let module_functions = &mut self.module_functions;
        let module_exports = &mut self.module_exports;
        let module_constants = &mut self.module_constants;
        let imported_functions = &mut self.imported_functions;
        let toplevel_import_sources = &mut self.toplevel_import_sources;

        // Build module function mapping: module_path -> set of function names
        // For nested modules, path is "A.B.C"

        // Collect info from all top-level modules (including precompiled stdlib)
        for module in all_modules {
            collect_module_info(
                module,
                "",
                module_functions,
                module_exports,
                module_constants,
            );
        }

        // Build set of function names that are imported via `using`
        // This respects both export restrictions and selective imports
        for using_import in &program.usings {
            let module_name = resolve_using_module_name(using_import, "", module_functions);

            // Get the functions available in this module
            if let Some(module_funcs) = module_name
                .as_deref()
                .and_then(|name| module_functions.get(name))
            {
                // Get the exported functions (empty = all exported)
                let exports = module_name
                    .as_deref()
                    .and_then(|name| module_exports.get(name));
                match &using_import.symbols {
                    // Selective import: `using Module: func1, func2`
                    Some(symbols) => {
                        for sym in symbols {
                            imported_functions.insert(sym.clone());
                            // Record the source module so a later top-level
                            // `function sym(...)` extends `Module.sym` (joins the
                            // qualified table too), not just shadows the bare `sym`
                            // (Issue #8052).
                            if let Some(src) = module_name.as_deref() {
                                toplevel_import_sources
                                    .entry(sym.clone())
                                    .or_default()
                                    .push(src.to_string());
                            }
                        }
                    }
                    // Import all exported: `using Module`
                    None => {
                        if let Some(exports) = exports.filter(|exports| !exports.is_empty()) {
                            imported_functions.extend(exports.iter().cloned());
                        } else {
                            for func_name in module_funcs {
                                imported_functions.insert(func_name.clone());
                            }
                        }
                    }
                }
            }
        }

        // Add top-level functions to imported_functions (they're always available)
        for func in program
            .functions
            .iter()
            .take(base_function_count)
            .chain(opt_user_functions.iter())
        {
            imported_functions.insert(func.name.clone());
        }

        // For backward compatibility, also keep track of used module names.
        self.usings_set = program.usings.iter().map(|u| u.module.clone()).collect();
    }

    fn validate_using_imports(&self) -> CResult<()> {
        validate_scope_using_imports(&self.program.usings, &self.module_functions)?;
        for module in &self.all_modules {
            validate_module_using_imports(module, &self.module_functions)?;
        }
        Ok(())
    }

    fn build_function_universe(&mut self) {
        let program = self.program;
        let base_function_count = self.base_function_count;
        let opt_user_functions = self.opt_user_functions;
        let opt_main = self.opt_main;
        let inline_functions = self.inline_functions;
        let all_modules = &self.all_modules;
        let imported_functions = &mut self.imported_functions;

        // Collect all functions in Julia evaluation order for `using`-loaded stdlib:
        // prelude/Base first, loaded module methods next, then user top-level
        // methods. This lets user methods written after `using LinearAlgebra`
        // replace same-signature stdlib methods in the method table.
        let base_function_names: HashSet<String> = program
            .functions
            .iter()
            .take(base_function_count)
            .map(|func| func.name.clone())
            .collect();
        let mut user_function_names: HashSet<String> = opt_user_functions
            .iter()
            .map(|func| func.name.clone())
            .collect();
        let mut user_module_functions = Vec::new();
        for module in self.opt_modules {
            collect_module_functions(module, "", &mut user_module_functions);
        }
        user_function_names.extend(
            user_module_functions
                .iter()
                .map(|(func, _)| func.name.clone()),
        );
        let base_functions = program.functions.iter().take(base_function_count);
        let user_functions = opt_user_functions.iter();
        let mut all_functions: Vec<(&Function, Option<String>)> =
            base_functions.map(|f| (f, None)).collect();

        for module in all_modules {
            collect_module_functions(module, "", &mut all_functions);
        }

        // Map each module-level function name to its owning module path, so that
        // nested/closure functions lifted from a module function body can inherit
        // the same module scope for name resolution (Issue #7180). Without this, a
        // closure passed to a Base HOF inside a module (e.g.
        // `findfirst(x -> help(x, 2), v)`) is registered with `module_path = None`
        // and fails to resolve the module-private helper `help`.
        let mut function_module_paths: HashMap<String, String> = all_functions
            .iter()
            .filter_map(|(func, module_path)| {
                module_path
                    .as_ref()
                    .map(|path| (func.name.clone(), path.clone()))
            })
            .collect();
        let first_user_function_idx = all_functions.len();
        all_functions.extend(user_functions.map(|f| (f, None)));

        // Build maps for nested function tracking (Issue #1743)
        // 1. nested_function_parents: qualified_name -> parent_name (for general reference)
        // 2. func_to_parent: function_name -> parent_name (for lookup during compilation)
        //    Note: When multiple parents have same-named nested functions, we track the index
        let mut nested_function_parents: HashMap<String, String> = HashMap::new();

        // Track inline function indices to their parent functions
        // We use the index in inline_functions as a unique identifier
        let mut inline_func_parent_by_idx: HashMap<usize, String> = HashMap::new();
        for (idx, (func, parent_name)) in inline_functions.iter().enumerate() {
            if let Some(parent) = parent_name {
                // Create qualified name: "parent#nested"
                let qualified_name = format!("{}#{}", parent, func.name);
                nested_function_parents.insert(qualified_name, parent.clone());
                inline_func_parent_by_idx.insert(idx, parent.clone());
            }
        }

        // Track the index in all_functions where inline functions start
        let inline_start_idx = all_functions.len();

        // Add inline functions to all_functions and imported_functions
        for (func, parent_name) in inline_functions {
            // Inherit the parent's module scope (if any) so closures lifted from a
            // module function body resolve module-private helpers (Issue #7180).
            // Propagate that inherited scope through qualified nested parents as
            // well, e.g. f#__do_block_0 -> f#__do_block_0#__lambda_0 (Issue #7591).
            let inline_module_path = parent_name
                .as_ref()
                .and_then(|parent| function_module_paths.get(parent).cloned());
            all_functions.push((func, inline_module_path.clone()));
            // Mark inline functions as imported so they can be called
            // For nested functions, use qualified name for disambiguation
            let inline_name = if let Some(parent) = parent_name {
                let qualified_name = format!("{}#{}", parent, func.name);
                if let Some((_, Some(path))) = all_functions.last() {
                    function_module_paths.insert(qualified_name.clone(), path.clone());
                }
                imported_functions.insert(qualified_name.clone());
                qualified_name
            } else {
                imported_functions.insert(func.name.clone());
                func.name.clone()
            };
            if let Some(module_path) = inline_module_path {
                function_module_paths.insert(inline_name, module_path);
            }
        }

        // Build a map from function index in all_functions to parent name (for inline functions only)
        let mut func_idx_to_parent: HashMap<usize, String> = HashMap::new();
        for (inline_idx, parent) in inline_func_parent_by_idx.iter() {
            let all_funcs_idx = inline_start_idx + inline_idx;
            func_idx_to_parent.insert(all_funcs_idx, parent.clone());
        }

        let callable_typeof_aliases =
            collect_callable_typeof_aliases(&opt_main.stmts, &all_functions);

        self.base_function_names = base_function_names;
        self.user_function_names = user_function_names;
        self.first_user_function_idx = first_user_function_idx;
        self.inline_start_idx = inline_start_idx;
        self.func_idx_to_parent = func_idx_to_parent;
        self.callable_typeof_aliases = callable_typeof_aliases;
        self.all_functions = all_functions;
    }

    fn prepopulate_closure_captures(&mut self) {
        let program = self.program;
        let base_function_count = self.base_function_count;
        let opt_user_functions = self.opt_user_functions;
        let inline_functions = self.inline_functions;
        let shared_ctx = &mut self.shared_ctx;

        // Pre-populate closure captures for nested functions (Issue #2100)
        //
        // When using prelude cache, parent functions are skipped during compilation,
        // so Stmt::FunctionDef in parent bodies never runs and closure captures are
        // never analyzed. This causes "Undefined variable" errors for captured variables
        // in nested functions that act as closures (e.g., curried string search functions).
        //
        // Fix: analyze free variables for all nested functions upfront by examining
        // each parent function's parameters as the outer scope.
        profile::time("compile.prepopulate_closure_captures", || {
            let parent_params_by_name: HashMap<String, HashSet<String>> =
                if inline_functions.iter().any(|(_, parent)| parent.is_some()) {
                    let mut parent_params_by_name = HashMap::new();
                    for parent_func in program
                        .functions
                        .iter()
                        .take(base_function_count)
                        .chain(opt_user_functions.iter())
                    {
                        parent_params_by_name
                            .entry(parent_func.name.clone())
                            .or_insert_with(|| {
                                parent_func.params.iter().map(|p| p.name.clone()).collect()
                            });
                    }
                    parent_params_by_name
                } else {
                    HashMap::new()
                };

            for (nested_func, parent_name) in inline_functions {
                if let Some(parent) = parent_name {
                    if let Some(outer_vars) = parent_params_by_name.get(parent) {
                        let free_vars = analyze_free_variables(nested_func, outer_vars);
                        if !free_vars.is_empty() {
                            let qname = format!("{}#{}", parent, nested_func.name);
                            shared_ctx.closure_captures.insert(qname, free_vars);
                        }
                    }
                }
            }
        });
    }

    fn preinstantiate_parametric_types(&mut self) {
        let base_function_count = self.base_function_count;
        let opt_main = self.opt_main;
        let precompiled_base = self.precompiled_base;
        let all_functions = &self.all_functions;
        let shared_ctx = &mut self.shared_ctx;

        // Pre-instantiate parametric struct types used in function parameters
        // This ensures that types like Complex{Float64} are in struct_table
        // BEFORE we infer function return types for method tables
        profile::time("compile.preinstantiate_parametric_params", || {
            for (idx, (func, _)) in all_functions.iter().enumerate() {
                if precompiled_base.is_some() && idx < base_function_count {
                    continue;
                }

                // Collect type parameter names from the function's where clause
                let type_param_names: HashSet<&str> =
                    func.type_params.iter().map(|tp| tp.name.as_str()).collect();

                for param in &func.params {
                    let param_ty = param.effective_type();
                    if let JuliaType::Struct(name) = &param_ty {
                        if let Some(brace_idx) = name.find('{') {
                            let base_name = &name[..brace_idx];
                            let type_args_str = &name[brace_idx + 1..name.len() - 1];

                            // Check if any type argument is a type parameter from where clause
                            // e.g., Rational{T} where T - T is a type parameter, not a concrete type
                            let Some(type_args) = parse_type_args_recursive(type_args_str) else {
                                continue;
                            };
                            let has_type_param = type_args
                                .iter()
                                .any(|arg| type_expr_contains_type_param(arg, &type_param_names));

                            // Skip instantiation if any type arg is a where clause type parameter
                            // These will be instantiated at call sites with concrete types
                            if has_type_param {
                                continue;
                            }

                            // Instantiate the parametric struct type
                            let _ = shared_ctx
                                .resolve_instantiation_with_type_expr(base_name, &type_args);
                        }
                    }
                }
            }
        });

        // Collect struct literal types from main block and function bodies
        let struct_literal_names: HashSet<String> =
            profile::time("compile.collect_struct_literals", || {
                let mut struct_literal_names = HashSet::new();
                collect_struct_literal_types(&opt_main.stmts, &mut struct_literal_names);
                for (idx, (func, _)) in all_functions.iter().enumerate() {
                    if precompiled_base.is_some() && idx < base_function_count {
                        continue;
                    }
                    collect_struct_literal_types(&func.body.stmts, &mut struct_literal_names);
                }
                struct_literal_names
            });

        // Instantiate parametric struct types from literals
        profile::time("compile.instantiate_struct_literals", || {
            for struct_name in &struct_literal_names {
                if let Some(brace_idx) = struct_name.find('{') {
                    let base_name = &struct_name[..brace_idx];
                    let type_args_str = &struct_name[brace_idx + 1..struct_name.len() - 1];
                    let Some(type_args) = parse_type_args_recursive(type_args_str) else {
                        continue;
                    };
                    // Instantiate the type (ignore errors - may already exist)
                    let _ = shared_ctx.resolve_instantiation_with_type_expr(base_name, &type_args);
                }
            }
        });
    }

    fn resolve_global_types(&mut self) {
        let opt_main = self.opt_main;
        let all_modules = &self.all_modules;
        let pending_global_types = &self.pending_global_types;
        let pending_global_struct_names = &self.pending_global_struct_names;
        let shared_ctx = &mut self.shared_ctx;

        // Now that struct_table is fully built, resolve global_types from REPL session
        // Pre-collect global variable types from main block before function compilation.
        // This allows functions to reference top-level const/global variables with proper types.
        // Also collects const struct constructors for inlining in functions.
        {
            let mut global_types_map = std::mem::take(&mut shared_ctx.global_types);
            // Merge with provided global_types (from REPL session)
            // Resolve struct type_ids from struct_names using struct_table (now fully built)
            for (name, ty) in pending_global_types {
                if let ValueType::Struct(_) = ty {
                    // Resolve struct type_id from struct_name
                    if let Some(struct_name) = pending_global_struct_names.get(name) {
                        if let Some(struct_info) = shared_ctx.struct_table.get(struct_name) {
                            global_types_map
                                .insert(name.clone(), ValueType::Struct(struct_info.type_id));
                            continue;
                        }
                        // Try to find parametric struct instance (e.g., "Rational{Int64}")
                        if let Some(brace_idx) = struct_name.find('{') {
                            let base_name = &struct_name[..brace_idx];
                            let prefix = format!("{}{{", base_name);
                            let mut resolved_type = None;
                            for (table_name, struct_info) in &shared_ctx.struct_table {
                                if table_name.starts_with(&prefix) || table_name == struct_name {
                                    resolved_type = Some(ValueType::Struct(struct_info.type_id));
                                    break;
                                }
                            }
                            if let Some(resolved_type) = resolved_type {
                                global_types_map.insert(name.clone(), resolved_type);
                                continue;
                            }
                        }
                    }
                }
                // For non-struct types or if struct resolution failed, use the provided type
                global_types_map.insert(name.clone(), ty.clone());
            }
            let mut global_const_structs = std::mem::take(&mut shared_ctx.global_const_structs);
            collect_global_types_for_inference(
                &opt_main.stmts,
                &mut global_types_map,
                &shared_ctx.struct_table,
                &mut global_const_structs,
            );
            shared_ctx.global_types = global_types_map;
            shared_ctx.global_const_structs = global_const_structs;
        }

        // Also collect global types from module bodies (for module-level constants like SHIFTEDMONTHDAYS).
        // This ensures module-level constants are registered before function compilation so they're
        // not flagged as "undefined variable" when referenced from module functions.
        {
            let mut global_types_map = std::mem::take(&mut shared_ctx.global_types);
            let mut global_const_structs = std::mem::take(&mut shared_ctx.global_const_structs);
            for module in all_modules {
                collect_global_types_for_inference(
                    &module.body.stmts,
                    &mut global_types_map,
                    &shared_ctx.struct_table,
                    &mut global_const_structs,
                );
            }
            shared_ctx.global_types = global_types_map;
            shared_ctx.global_const_structs = global_const_structs;
        }
    }

    fn resolve_module_imports(&mut self) {
        let all_modules = &self.all_modules;
        let module_functions = &self.module_functions;
        let module_exports = &self.module_exports;
        let module_imports_map = &mut self.module_imports_map;
        let module_usings_map = &mut self.module_usings_map;
        let module_imported_bindings = &mut self.shared_ctx.module_imported_bindings;

        // Collect module-level using statements to support module-local imports.
        let mut module_usings: HashMap<String, Vec<UsingImport>> = HashMap::new();

        for module in all_modules {
            collect_module_usings(module, "", &mut module_usings);
        }

        // Resolve module-local imports based on their using statements.
        for (module_path, usings) in &module_usings {
            let mut imported = HashSet::new();
            for using_import in usings {
                if let Some(using_module) =
                    resolve_using_module_name(using_import, module_path, module_functions)
                {
                    if let Some(module_funcs) = module_functions.get(using_module.as_str()) {
                        let exports = module_exports.get(using_module.as_str());
                        let all_exported = exports.is_none_or(|e| e.is_empty());

                        match &using_import.symbols {
                            // Selective import: `using Module: func1, func2`
                            Some(symbols) => {
                                for sym in symbols {
                                    imported.insert(sym.clone());
                                    // Record the qualified re-export so a later
                                    // `ImportingModule.sym` resolves to its source
                                    // `using_module.sym` (Issue #8053). Only the
                                    // selective form is recorded: the IR does not
                                    // distinguish non-selective `using M` (which
                                    // exposes M's exports via getproperty) from
                                    // `import M` (which does not), so registering
                                    // every export for the `None` case would risk
                                    // wrongly exposing `import M` members.
                                    module_imported_bindings.insert(
                                        format!("{}.{}", module_path, sym),
                                        format!("{}.{}", using_module, sym),
                                    );
                                }
                            }
                            // Import all exported functions: `using Module`
                            None => {
                                for func_name in module_funcs {
                                    if all_exported
                                        || exports.is_some_and(|e| e.contains(func_name))
                                    {
                                        imported.insert(func_name.clone());
                                    }
                                }
                            }
                        }
                    }
                }
            }
            module_imports_map.insert(module_path.clone(), imported);
        }
        *module_usings_map = module_usings;
    }

    fn build_inference_engine(&mut self) -> abstract_interp::InferenceEngine {
        let base_function_count = self.base_function_count;
        let opt_main = self.opt_main;
        let all_modules = &self.all_modules;
        let all_functions = &self.all_functions;
        let func_idx_to_parent = &self.func_idx_to_parent;
        let precompiled_base = self.precompiled_base;
        let cached_inference_results = self.cached_inference_results;
        let shared_ctx = &self.shared_ctx;

        // Build a map from abstract type name to its parent for converting Struct
        // to AbstractUser. Sourced from `shared_ctx.abstract_types`, which (unlike
        // the bare `program.abstract_types`) also carries abstract types declared
        // inside modules / bundled packages (Issues #7263 / #7265) so a
        // module-local annotation like `f(d::Distribution)` is resolved to the
        // abstract type rather than a concrete `Struct("Distribution")`.
        let abstract_type_parents: HashMap<String, Option<String>> = shared_ctx
            .abstract_types
            .iter()
            .map(|at| (at.name.clone(), at.parent.clone()))
            .collect();

        let _total_functions = all_functions.len();

        // Build a shared inference engine ONCE before the loop.
        // Rebuilding one-shot inference engines inside the loop used to clone all
        // ~5000 functions on every iteration (O(n^2)).
        // This shared engine clones functions once (O(n)) and reuses the return-type cache.
        let inference_functions: Vec<Function> =
            profile::time("compile.inference_functions_clone", || {
                let clone_with_rename =
                    |(idx, (func, _)): (usize, &(&Function, Option<String>))| {
                        let mut func = (*func).clone();
                        if let Some(parent) = func_idx_to_parent.get(&idx) {
                            func.name = format!("{}#{}", parent, func.name);
                        }
                        func
                    };

                // Issue #6348: on the cached-Base path the first
                // `base_function_count` entries of `all_functions` are exactly the
                // prelude functions (the cache is bypassed when a user definition
                // replaces a Base signature), and nested-function renames only
                // apply to inline entries past the Base segment. Reuse the clone
                // a background prefetch thread prepared during pipeline load
                // instead of deep-cloning 4577 Base bodies here.
                if precompiled_base.is_some() && base_function_count <= all_functions.len() {
                    if let Some(mut funcs) =
                        cache::take_prefetched_base_inference_functions(base_function_count)
                    {
                        funcs.reserve(all_functions.len() - base_function_count);
                        funcs.extend(
                            all_functions
                                .iter()
                                .enumerate()
                                .skip(base_function_count)
                                .map(|(idx, entry)| clone_with_rename((idx, entry))),
                        );
                        return funcs;
                    }
                }

                all_functions
                    .iter()
                    .enumerate()
                    .map(|(idx, entry)| clone_with_rename((idx, entry)))
                    .collect()
            });
        let mut inference_global_types = shared_ctx.global_types.clone();
        widen_non_const_globals_for_binding_inference(&opt_main.stmts, &mut inference_global_types);
        for module in all_modules {
            widen_non_const_globals_for_binding_inference(
                &module.body.stmts,
                &mut inference_global_types,
            );
        }
        let boundary_idx = opt_main.stmts.iter().position(is_base_user_main_boundary);
        let mut shadowed_user_globals = HashSet::new();
        if let Some(idx) = boundary_idx {
            collect_assigned_binding_names(&opt_main.stmts[idx + 1..], &mut shadowed_user_globals);
        }
        shadowed_user_globals.extend(self.user_function_names.iter().cloned());
        for name in &shadowed_user_globals {
            inference_global_types.remove(name);
        }

        let mut inference_engine = profile::time("compile.build_inference_engine", || {
            build_shared_inference_engine_owned(
                &shared_ctx.struct_table,
                &inference_global_types,
                inference_functions,
            )
        });
        let has_seeded_inference_results =
            cached_inference_results.is_some_and(|entries| !entries.is_empty());
        profile::time("compile.seed_inference_results", || {
            if let Some(entries) = cached_inference_results {
                inference_engine.seed_return_cache(entries.iter().cloned());
            }
        });

        // Issue #6538: on the cached-Base path, `build_method_tables` below
        // short-circuits every cached Base function (`is_cached_base_function`)
        // without registering its `MethodSig`s into the inference engine, so a
        // user function calling a multi-method Base function got NO inference
        // information from the engine method tables (and `add_function` had
        // already dropped multi-signature names as ambiguous from the function
        // table). Such calls fell through to the tfunc registry and inferred
        // `Any` where the uncached path infers precisely. Seed the engine's
        // method tables wholesale from the cached Base tables — `Arc`-shared
        // method vectors make this O(#tables) — so both compile paths resolve
        // calls through the same method-table snapshot channel. The gate
        // mirrors `is_cached_base_function` in `build_method_tables`.
        if self.precompiled_base.is_some() {
            if let Some(cached_tables) = self.cached_method_tables {
                profile::time("compile.seed_engine_method_tables", || {
                    inference_engine.seed_initial_method_tables(cached_tables.iter());
                });
            }
        }

        self.abstract_type_parents = abstract_type_parents;
        self.shadowed_user_globals = shadowed_user_globals;
        self.has_seeded_inference_results = has_seeded_inference_results;
        inference_engine
    }

    fn build_method_tables(&mut self, inference_engine: &mut abstract_interp::InferenceEngine) {
        let base_function_count = self.base_function_count;
        let precompiled_base = self.precompiled_base;
        let cached_method_tables = self.cached_method_tables;
        let has_seeded_inference_results = self.has_seeded_inference_results;
        let all_functions = &self.all_functions;
        let func_idx_to_parent = &self.func_idx_to_parent;
        let module_struct_names = &self.module_struct_names;
        let abstract_type_parents = &self.abstract_type_parents;
        let callable_typeof_aliases = &self.callable_typeof_aliases;
        let toplevel_import_sources = &self.toplevel_import_sources;
        let module_usings_map = &self.module_usings_map;
        let shared_ctx = &mut self.shared_ctx;
        let method_tables = &mut self.method_tables;
        let function_infos = &mut self.function_infos;
        let func_index_map = &mut self.func_index_map;
        let show_methods = &mut self.show_methods;
        let specializable_functions = &mut self.specializable_functions;
        let global_index = &mut self.global_index;
        let cached_base_specializations = precompiled_base
            .filter(|_| cached_method_tables.is_some())
            .filter(|base| !base.specializable_functions.is_empty());
        if let Some(base) = cached_base_specializations {
            debug_assert!(
                specializable_functions.is_empty(),
                "cached Base specializable functions must stay at the front so cached CallSpecialize indices remain valid"
            );
            profile::time("compile.cached_base_specializations_restore", || {
                specializable_functions.extend(base.specializable_functions.iter().cloned());
                for &(fallback_index, spec_index) in &base.runtime_specialization_map {
                    if fallback_index < base_function_count
                        && spec_index < base.specializable_functions.len()
                    {
                        shared_ctx
                            .spec_func_mapping
                            .insert(fallback_index, spec_index);
                    }
                }
            });
        }

        let mut cached_base_fast_count = 0usize;
        let mut cached_base_rebuild_count = 0usize;
        let mut non_cached_function_count = 0usize;
        for (all_funcs_idx, (func, module_path)) in all_functions.iter().enumerate() {
            // Fast path for cached Base functions: function_infos[all_funcs_idx]
            // already holds params/kwparams/return_type from the cache, method tables
            // are pre-populated, and show_methods are pre-populated. The only work
            // that must still happen is identity push into func_index_map and
            // specialization registration so cached CallSpecialized instructions
            // resolve. Without this short-circuit, the loop below calls
            // inference_engine.infer_function for every cached Base function and
            // throws the result away, dominating startup
            // (~1.3 s of 1.4 s total for `println(1+1)` on Mac M1).
            let is_cached_base_function = (all_funcs_idx + 1) <= base_function_count
                && cached_method_tables.is_some()
                && precompiled_base.is_some();
            if is_cached_base_function {
                let func_info_idx = all_funcs_idx;
                func_index_map.push(func_info_idx);

                if cached_base_specializations.is_some() {
                    cached_base_fast_count += 1;
                    continue;
                }
                cached_base_rebuild_count += 1;

                let is_specializable = if let Some(path) = module_path {
                    path != "Core" && !path.starts_with("Core.")
                } else {
                    true
                };
                let specialization_lookup_name =
                    if let Some(parent) = func_idx_to_parent.get(&all_funcs_idx) {
                        format!("{}#{}", parent, func.name)
                    } else if let Some(module_path) = module_path {
                        format!("{}.{}", module_path, func.name)
                    } else {
                        func.name.clone()
                    };
                let has_recorded_closure_captures = shared_ctx
                    .closure_captures
                    .contains_key(&specialization_lookup_name);
                let runtime_specialize =
                    needs_specialization(func) || has_recorded_closure_captures;
                // Issue #5003: also register where/value-parametrized methods so
                // reflection-time inference can find them, but do NOT add them to
                // spec_func_mapping (which drives CallSpecialize emission) unless they
                // truly need runtime specialization — that would bypass dispatch.
                let reflection_register = needs_reflection_registration(func);
                if is_specializable && (runtime_specialize || reflection_register) {
                    let spec_idx = specializable_functions.len();
                    specializable_functions.push(SpecializableFunction {
                        ir: (*func).clone(),
                        name: func.name.clone(),
                        fallback_index: func_info_idx,
                    });
                    if runtime_specialize {
                        shared_ctx.spec_func_mapping.insert(func_info_idx, spec_idx);
                    }
                }
                continue;
            }
            non_cached_function_count += 1;

            // Build params early (needed for both method tables and show methods)
            // For module functions, qualify struct type names to match the qualified struct instances.
            // Also convert Struct types to AbstractUser when the type is actually an abstract type.
            let params: Vec<(String, JuliaType)> = func
                .params
                .iter()
                .map(|p| {
                    let ty = p.effective_type();
                    let qualified_ty =
                        qualify_type_for_module(ty, module_path.as_ref(), module_struct_names);
                    let resolved_ty = resolve_abstract_type(qualified_ty, abstract_type_parents);
                    // Resolve type aliases (Issue #2527): const IntWrapper = Wrapper{Int64}
                    let alias_resolved = resolve_type_alias(resolved_ty, &shared_ctx.type_aliases);
                    (p.name.clone(), alias_resolved)
                })
                .collect();

            // Build vm_params, vm_kwparams, and return_type (needed for FunctionInfo)
            let vm_params: Vec<(String, ValueType)> = params
                .iter()
                .map(|(name, jt)| {
                    (
                        name.clone(),
                        julia_type_to_value_type_with_table(jt, &shared_ctx.struct_table),
                    )
                })
                .collect();

            let vm_kwparams: Vec<KwParamInfo> = func
                .kwparams
                .iter()
                .map(|kw| {
                    let required = is_required_kwarg(&kw.default);
                    // For varargs kwargs (kwargs...), type is always Pairs (Julia's Base.Pairs)
                    // For required kwargs, use type annotation if available; otherwise use Any
                    // For optional kwargs, infer from default value
                    let ty = if kw.is_varargs {
                        ValueType::Pairs
                    } else if required {
                        kw.type_annotation
                            .as_ref()
                            .map(|jt| {
                                julia_type_to_value_type_with_table(jt, &shared_ctx.struct_table)
                            })
                            .unwrap_or(ValueType::Any)
                    } else if kw.body_evaluated_default {
                        // The default is re-evaluated inside the body (Issue #5121).
                        // The kwsorter binds the `Undef` sentinel to the slot for an
                        // omitted keyword, and the body prologue overwrites it with the
                        // real default (any type), so the slot must be `Any`.
                        ValueType::Any
                    } else if is_unannotated_optional_kwparam(kw) {
                        // An unannotated optional kwarg accepts any value, so the single
                        // compiled body must treat it as `Any` — the default's type must
                        // not constrain the slot (Issue #5425, generalizing #5416).
                        ValueType::Any
                    } else {
                        infer_default_type(&kw.default)
                    };
                    KwParamInfo {
                        name: kw.name.clone(),
                        // For body-evaluated defaults the kwsorter must bind the
                        // `Undef` sentinel so the prologue's `kw === Undef` guard
                        // fires; the real default lives in the body (Issue #5121).
                        default: if kw.body_evaluated_default {
                            Value::Undef
                        } else {
                            eval_literal_default(&kw.default)
                        },
                        default_expr: if required || kw.is_varargs || kw.body_evaluated_default {
                            None
                        } else {
                            Some(kw.default.clone())
                        },
                        ty,
                        slot: 0,
                        required,
                        is_varargs: kw.is_varargs,
                    }
                })
                .collect();

            // Skip Base functions if we're using cached method tables (Option A optimization)
            // Base methods are already in the cached method tables
            // When using cache, global_index starts at base_function_count, so we use loop counter instead
            // Note: all_funcs_idx is 0-indexed, so we use <= to match 1-indexed behavior
            let is_base_function = (all_funcs_idx + 1) <= base_function_count;

            // Use declared return type if available, otherwise infer from function body
            // Using the shared inference engine (created once before the loop) for
            // abstract interpretation. The engine caches return types across calls.
            let (mut return_type, return_julia_type) = if let Some(ref declared_rt) =
                func.return_type
            {
                let vt = julia_type_to_value_type_with_table(declared_rt, &shared_ctx.struct_table);
                // Declared return types already carry parametric info via JuliaType
                let jt = if matches!(declared_rt, JuliaType::TupleOf(_)) {
                    Some(declared_rt.clone())
                } else {
                    None
                };
                (vt, jt)
            } else if should_defer_module_return_inference(
                func,
                module_path.as_ref(),
                is_base_function,
            ) {
                // Package/module methods without declared return types dominate
                // `using Package` startup when every method is inferred eagerly.
                // Keep dispatch safe by recording an `Any` snapshot, while
                // preserving cheap syntactic type-parameter/direct-parameter
                // snapshots used by reflection and datatype-return call sites
                // (Issue #8463).
                let type_param_jt = type_parameter_return_snapshot(func);
                let jt = type_param_jt
                    .clone()
                    .or_else(|| direct_parameter_return_snapshot(func));
                let vt = if type_param_jt.is_some() {
                    ValueType::DataType
                } else {
                    ValueType::Any
                };
                (vt, jt)
            } else {
                let rt = inference_engine.infer_function(func);
                let inferred_vt = bridge::lattice_to_value_type(&rt);
                // Extract parametric tuple type that ValueType::Tuple would lose (Issue #2317)
                let type_param_jt = type_parameter_return_snapshot(func);
                let jt = type_param_jt
                    .clone()
                    .or_else(|| direct_parameter_return_snapshot(func))
                    .or_else(|| bridge::lattice_to_parametric_julia_type(&rt));
                let mut vt = if jt.is_some() {
                    if type_param_jt.is_some() {
                        ValueType::DataType
                    } else {
                        inferred_vt
                    }
                } else {
                    inferred_vt
                };
                if has_abstract_numeric_param(&params) && is_concrete_numeric_return_type(&vt) {
                    // Abstract numeric parameters (`x::Number`, `x::Real`, ...)
                    // accept BigInt/BigFloat and primitive numeric values. A single
                    // concrete numeric return type inferred from the method body is
                    // therefore a storage hazard for VM calls: callers would emit a
                    // typed StoreSlot and reject valid runtime results (Issue #4337).
                    vt = ValueType::Any;
                };
                if returns_untyped_param_power_value(func) {
                    // `^` over an untyped parameter must preserve the runtime
                    // `DynamicPow` result (`Int^Int -> Int`, negative exponents ->
                    // Float64) instead of pinning the single compiled body to F64
                    // (Issue #5608).
                    vt = ValueType::Any;
                }
                if matches!(vt, ValueType::Nothing)
                    && directly_returns_unannotated_optional_kwparam(func)
                {
                    // A `nothing`-default kwarg returned directly must not pin the
                    // function's snapshot return type to the `Nothing` singleton
                    // (Issue #5416). Note we keep a *non-`Nothing`* concrete snapshot
                    // (e.g. `Int64` for `g(; n = 0) = n`) intact so reflection stays
                    // precise; the compiled-body / call-site widening for those is
                    // applied separately (Issue #5425).
                    vt = ValueType::Any;
                }
                (vt, jt)
            };
            if func.name == "Dict" || func.name.starts_with("Dict{") {
                // Public Dict constructors now return the pure-Julia
                // `Dict{K,V}` struct. Do not compile those method bodies with
                // the legacy `ValueType::Dict` carrier / ReturnDict path
                // (Issue #6619).
                return_type = ValueType::Any;
            }
            if let Some(JuliaType::Struct(name)) = &return_julia_type {
                let base = name.rsplit('.').next().unwrap_or(name);
                if base.split('{').next() == Some("Dict") {
                    let compact_name = compact_type_name(name);
                    if let Some(info) = shared_ctx.struct_table.get(name).or_else(|| {
                        shared_ctx
                            .struct_table
                            .iter()
                            .find(|(struct_name, _)| compact_type_name(struct_name) == compact_name)
                            .map(|(_, info)| info)
                    }) {
                        return_type = ValueType::Struct(info.type_id);
                    }
                }
            }
            if func.name == "copy" || func.name == "Base.copy" {
                // Mirrors tfunc_copy's #5867 guard: the current `copy(::Dict)`
                // implementation is a legacy/migration surface and must not
                // be compiled with ReturnDict when public Dict() now creates a
                // struct-backed value (Issue #6619).
                return_type = ValueType::Any;
            }

            let skip_method_table_update = is_base_function && cached_method_tables.is_some();
            // When using cache, skip function_infos.push() for Base functions (already in cache)
            let skip_function_info_push = is_base_function && precompiled_base.is_some();
            let is_runtime_eval_function = func.is_runtime_eval;

            // Detect varargs parameter early (needed for both MethodSig and FunctionInfo)
            let vararg_param_index = func.params.iter().position(|p| p.is_varargs);
            // For Vararg{T, N}: extract fixed count N (Issue #2525)
            let vararg_fixed_count = func
                .params
                .iter()
                .find(|p| p.is_varargs)
                .and_then(|p| p.vararg_count);

            if is_runtime_eval_function {
                shared_ctx
                    .runtime_eval_function_names
                    .insert(func.name.clone());
                shared_ctx
                    .runtime_eval_function_indices
                    .insert(*global_index);
            }

            if !skip_method_table_update && !is_runtime_eval_function {
                if !is_base_function {
                    // Issue #7643: user-written Base extensions such as
                    // `import Base: ==; ==(::S, ::S) = ...` need the same IR
                    // metadata as other user methods so dynamic dispatch
                    // candidate builders can see their declared argument types.
                    // Only real Base/prelude functions are excluded here.
                    shared_ctx
                        .function_ir_by_global_index
                        .insert(*global_index, (*func).clone());
                }
                // A nested (inner) function is lexically scoped to its parent, so
                // register it under its qualified `parent#name` table ONLY — never
                // the bare short name. `function_infos`/`function_indices` already
                // key nested functions by this qualified name. Sharing the bare
                // short-name table with a same-named GLOBAL would let the inner
                // definition's signature DEDUP-REPLACE the global's method
                // (`MethodTable::add_method` dedups by signature), so a value
                // reference to the global (`f = g; f()`) — which resolves via the
                // bare table — would pick up the inner function's body instead of
                // the global's (Issue #8105). The module/import/typeof aliases below
                // only apply to top-level / module functions, never inner ones.
                let nested_qualified_name = func_idx_to_parent
                    .get(&all_funcs_idx)
                    .map(|parent| format!("{}#{}", parent, func.name));
                let mut table_names = vec![nested_qualified_name
                    .clone()
                    .unwrap_or_else(|| func.name.clone())];
                // A nested ANONYMOUS function (compiler-generated `__lambda_*` /
                // `__do_block_*`) carries a unique name that cannot collide with a
                // user global, so the qualified-table-only restriction above does
                // not apply to it. It must ALSO stay in the bare short-name table:
                // the higher-order-function return-type specialization resolves the
                // lambda by its bare name, and dropping that registration broke
                // `reduce`/`mapreduce` result-type inference (Issue #5094 regression
                // from #8105; fixed #8129).
                if nested_qualified_name.is_some()
                    && crate::compile::ir_inline::is_anonymous_generated_name(&func.name)
                {
                    table_names.push(func.name.clone());
                }
                if nested_qualified_name.is_none() {
                    if let Some(short_name) = func.name.strip_prefix("Base.") {
                        table_names.push(short_name.to_string());
                    } else if !is_base_function {
                        // A user method explicitly defined on another module's function
                        // (`function Inner.f(...)`) carries a module-qualified name. Also
                        // register it under the bare function table (`f`) so the
                        // unqualified `f(2.0)` brought in by `using .Inner` dispatches
                        // across the module-owned methods, while the qualified
                        // `Inner.f(2.0)` resolves via `func.name` above (Issue #8052).
                        // Only user (non-Base) qualified names get this bare alias so
                        // stdlib `Core.*`/`Base.*` names are unaffected.
                        if let Some((_, bare)) = func.name.rsplit_once('.') {
                            table_names.push(bare.to_string());
                        } else if module_path.is_none() {
                            // A top-level `function f(...)` whose bare name `f` was
                            // selectively imported (`import M: f`) extends `M.f`: also
                            // register the method under the qualified table so a later
                            // `M.f(2.0)` sees it, matching Julia (Issue #8052).
                            if let Some(sources) = toplevel_import_sources.get(&func.name) {
                                for src in sources {
                                    table_names.push(format!("{}.{}", src, func.name));
                                }
                            }
                        }
                    }
                    add_callable_typeof_method_table_aliases(
                        &func.name,
                        callable_typeof_aliases,
                        &mut table_names,
                    );
                    if let Some(module_path) = module_path {
                        table_names.push(format!("{}.{}", module_path, func.name));
                    }
                }
                // Issue #5425 / #5466: a function that returns an unannotated optional
                // kwarg — directly (`g(; n = 0) = n`) or derived through a computation
                // (`g2(; n = 0) = n + 1`) — returns whatever the caller passes for that
                // kwarg, so its *dispatch* return type must be `Any`. Every compile-time
                // call-type inference (binary-op operand typing, discard/assign stores,
                // call-result typing) reads `MethodSig.return_type`; a concrete
                // default-derived type (e.g. `Int64`) would drive a typed
                // comparison/store that rejects a differently-typed passed value.
                // `FunctionInfo.return_type` (set below) stays precise so reflection
                // (`Base.infer_return_type`) keeps the omitted-kwarg signature's type.
                let method_return_type = if returns_unannotated_optional_kwparam_value(func)
                    || returns_untyped_param_power_value(func)
                {
                    ValueType::Any
                } else {
                    return_type.clone()
                };
                let normalized_type_params = shared_ctx.expand_type_param_bounds(&func.type_params);
                for table_name in table_names {
                    // Issue #8079: a user (non-Base) method whose bare name
                    // collides with a Base library function REPLACES the
                    // same-signature base method in the shared short-name table
                    // (`MethodTable::add_method` dedups by signature). An explicit
                    // `Base.<name>(...)` call would then re-dispatch to the user
                    // shadow instead of Base, which self-recurses when the shadow
                    // forwards to `Base.<name>` (e.g. NaNMath.log2 → Base.log2 →
                    // NaNMath.log2 → …) and overflows the call stack. Snapshot the
                    // bare table's base methods *before* adding the user method so
                    // that, if the add actually replaces a base method (only when
                    // the signatures collide — a typed base `log(::Float64)` is
                    // untouched by an untyped `log(::Any)` shadow), they can be
                    // preserved under a dedicated `Base.<name>` table for the
                    // qualified call to dispatch through.
                    let preserve_candidate: Option<Vec<MethodSig>> = if !is_base_function
                        && !table_name.contains('.')
                        && !method_tables.contains_key(&format!("Base.{}", table_name))
                    {
                        method_tables.get(&table_name).map(|existing| {
                            existing
                                .methods
                                .iter()
                                .filter(|m| m.is_base_program_method(base_function_count))
                                .cloned()
                                .collect::<Vec<_>>()
                        })
                    } else {
                        None
                    };
                    let base_before = preserve_candidate.as_ref().map_or(0, |v| v.len());

                    let sig = {
                        let table = method_tables
                            .entry(table_name.clone())
                            .or_insert_with(|| MethodTable::new(table_name.clone()));

                        let method_index = table.methods.len();
                        let sig = MethodSig::from_julia_projections(
                            method_index,
                            *global_index,
                            params.clone(),
                            method_return_type.clone(),
                            return_julia_type.clone(),
                            func.is_base_extension,
                            normalized_type_params.clone(),
                            vararg_param_index,
                            vararg_fixed_count,
                        );
                        table.add_method(sig.clone());
                        sig
                    };
                    if has_seeded_inference_results && !is_base_function {
                        inference_engine.add_method(table_name.clone(), sig);
                    } else {
                        inference_engine.add_initial_method(table_name.clone(), sig);
                    }

                    // The user method genuinely clobbered a base method iff the
                    // bare table now holds fewer base-program methods than before.
                    // Preserve the pre-clobber base methods under `Base.<name>`.
                    if let Some(base_methods) = preserve_candidate {
                        if base_before > 0 {
                            let base_after = method_tables.get(&table_name).map_or(0, |t| {
                                t.methods
                                    .iter()
                                    .filter(|m| m.is_base_program_method(base_function_count))
                                    .count()
                            });
                            if base_after < base_before {
                                let qualified_base = format!("Base.{}", table_name);
                                let mut snapshot = MethodTable::new(qualified_base.clone());
                                snapshot.set_base_function_count(base_function_count);
                                for m in base_methods {
                                    snapshot.add_method(m.clone());
                                    inference_engine.add_initial_method(qualified_base.clone(), m);
                                }
                                method_tables.insert(qualified_base, snapshot);
                            }
                        }
                    }
                }
            }

            // Detect show methods: function Base.show(io::IO, x::SomeStruct)
            // Also detect show methods defined within base library files (e.g., io.jl)
            // Skip for cached Base functions — their show_methods are pre-populated from cache (Issue #2489)
            let is_show_name = func.name == "show" || func.name.rsplit('.').next() == Some("show");
            let extends_base_show = func.is_base_extension
                || is_base_function
                || module_imports_base_symbol(module_path.as_ref(), module_usings_map, "show");
            if !skip_function_info_push && extends_base_show && is_show_name && params.len() >= 2 {
                // First param must be IO type
                if let JuliaType::IO = &params[0].1 {
                    // Second param must be a Struct type.
                    if let JuliaType::Struct(struct_name) = &params[1].1 {
                        // Register under the exact name as written in the signature.
                        show_methods.push(ShowMethodEntry {
                            type_name: struct_name.clone(),
                            func_index: *global_index,
                        });
                        // For a parametric signature such as
                        // `show(io::IO, b::Box{T}) where T`, the second param's
                        // JuliaType is `Struct("Box{T}")`, carrying the type-var name
                        // in the braces. The runtime lookup (`user_show_method_for`)
                        // keys on the value's concrete struct name (e.g.
                        // "Box{Int64}") and only falls back to the bare base name
                        // ("Box"), never to the typevar form. Also register the bare
                        // base name so parametric `where T` show methods are found,
                        // while exact concrete instantiations (e.g.
                        // `show(io::IO, ::Box{Int64})`) still take precedence via the
                        // exact-name entry above (Issue #4853).
                        if let Some(brace_idx) = struct_name.find('{') {
                            let base_name = struct_name[..brace_idx].to_string();
                            show_methods.push(ShowMethodEntry {
                                type_name: base_name,
                                func_index: *global_index,
                            });
                        }
                    }
                }
            }

            // Build func_index_map and function_infos
            // When using cache, Base functions are already in function_infos (from cache clone)
            let func_info_idx = if skip_function_info_push {
                // Base function using cache: identity mapping (index in all_functions = index in function_infos)
                // all_funcs_idx is 0-indexed, same as function_infos
                func_index_map.push(all_funcs_idx);
                all_funcs_idx
            } else {
                // User function or no cache: push to function_infos, map to new index
                let idx = function_infos.len();
                func_index_map.push(idx);

                // Preserve original JuliaTypes for type parameter binding
                let param_julia_types: Vec<JuliaType> =
                    params.iter().map(|(_, jt)| jt.clone()).collect();

                // Retain representative reflection metadata from the leading
                // @inline/@noinline/@propagate_inbounds/@constprop/@nospecialize(infer)/
                // @assume_effects markers (Issues #4977/#4978/#4979/#4980/#4981/
                // #4983/#4984).
                let reflection_meta = function_reflection_meta(func);

                // For nested functions, use qualified name (parent#nested) to avoid collisions
                // when multiple parent functions have nested functions with the same name (Issue #1743)
                let function_name = if let Some(parent) = func_idx_to_parent.get(&all_funcs_idx) {
                    format!("{}#{}", parent, func.name)
                } else if let Some(module_path) = module_path {
                    format!("{}.{}", module_path, func.name)
                } else {
                    func.name.clone()
                };

                function_infos.push(FunctionInfo {
                    name: function_name,
                    params: vm_params,
                    kwparams: vm_kwparams,
                    entry: 0,
                    return_type,
                    return_julia_type,
                    is_base_extension: func.is_base_extension,
                    is_generated: reflection_meta.is_generated,
                    min_world: if is_runtime_eval_function {
                        u64::MAX
                    } else {
                        1
                    },
                    type_params: shared_ctx.expand_type_param_bounds(&func.type_params),
                    param_julia_types,
                    code_start: 0, // Will be set during compilation
                    code_end: 0,   // Will be set during compilation
                    slot_names: Vec::new(),
                    slot_types: Vec::new(),
                    local_slot_count: 0,
                    param_slots: Vec::new(),
                    vararg_param_index,
                    vararg_fixed_count,
                    inlining_meta: reflection_meta.inlining,
                    constprop_meta: reflection_meta.constprop,
                    nospecialize_meta: reflection_meta.nospecialize,
                    propagate_inbounds_meta: reflection_meta.propagate_inbounds,
                    nospecializeinfer_meta: reflection_meta.nospecializeinfer,
                    purity_meta: reflection_meta.purity,
                    direct_return_type_param: direct_return_type_param(func),
                    // 1-based source line of the definition, surfaced as
                    // `Method.line` (Issue #5125).
                    def_line: func.span.start_line as u32,
                });

                // Register function index for Stmt::FunctionDef lookups
                // Use qualified name for nested functions to avoid collisions (Issue #1743)
                let registration_name = if let Some(parent) = func_idx_to_parent.get(&all_funcs_idx)
                {
                    // This is a nested function - use qualified name
                    format!("{}#{}", parent, func.name)
                } else if let Some(module_path) = module_path {
                    format!("{}.{}", module_path, func.name)
                } else {
                    // Top-level or module function - use original name
                    func.name.clone()
                };
                shared_ctx.function_indices.insert(registration_name, idx);

                *global_index += 1;
                idx
            };

            // Lazy AoT: Register function if it needs specialization
            // This must be done for ALL functions (including Base when using cache)
            // because cached bytecode may contain CallSpecialized instructions
            // Lazy AoT specialization enabled for:
            // - Base functions: enabled
            // - User functions: enabled
            // - Stdlib modules: enabled (Statistics, etc.)
            // - Core module: DISABLED (intrinsic wrappers like add_int)
            let is_specializable = if let Some(path) = module_path {
                // Module functions: enable for Stdlib, disable for Core
                path != "Core" && !path.starts_with("Core.")
            } else {
                // Non-module functions (Base + User): all enabled
                true
            };
            let specialization_lookup_name =
                if let Some(parent) = func_idx_to_parent.get(&all_funcs_idx) {
                    format!("{}#{}", parent, func.name)
                } else if let Some(module_path) = module_path {
                    format!("{}.{}", module_path, func.name)
                } else {
                    func.name.clone()
                };
            let has_recorded_closure_captures = shared_ctx
                .closure_captures
                .contains_key(&specialization_lookup_name);
            let runtime_specialize = needs_specialization(func) || has_recorded_closure_captures;
            // Issue #5003: register where/value-parametrized methods for reflection-time
            // inference too, but only map to spec_func_mapping (which drives
            // CallSpecialize emission) when runtime specialization is actually needed,
            // so multiple dispatch is preserved for generic methods like promote_rule.
            let reflection_register = needs_reflection_registration(func);
            if is_specializable && (runtime_specialize || reflection_register) {
                let spec_idx = specializable_functions.len();
                specializable_functions.push(SpecializableFunction {
                    ir: (*func).clone(),
                    name: func.name.clone(),
                    fallback_index: func_info_idx,
                });
                if runtime_specialize {
                    // Map function global_index to specializable index
                    shared_ctx.spec_func_mapping.insert(func_info_idx, spec_idx);
                }
            }
        }

        // Debug assertion: verify cache alignment after function merging (Issue #2726).
        // When using precompiled cache, all_functions[i] and function_infos[i] must have the same
        // name for all Base functions. A mismatch indicates that exact signature matching in
        // base function filtering has regressed, which would cause Call instructions in cached
        // bytecode to invoke the wrong function.
        #[cfg(debug_assertions)]
        if precompiled_base.is_some() {
            for i in 0..base_function_count
                .min(all_functions.len())
                .min(function_infos.len())
            {
                let all_func_name = &all_functions[i].0.name;
                let info_name = &function_infos[i].name;
                debug_assert_eq!(
                    all_func_name, info_name,
                    "Cache alignment mismatch at index {}: all_functions has '{}' but function_infos has '{}'. \
                     Base function filtering must use exact signature matching (Issue #2726).",
                    i, all_func_name, info_name
                );
            }
        }
        profile::note("compile.build_method_tables.counts", || {
            format!(
                "cached_base_fast={} cached_base_rebuild={} non_cached={} all_functions={} base_function_count={} restored_specializable={} restored_runtime_map={}",
                cached_base_fast_count,
                cached_base_rebuild_count,
                non_cached_function_count,
                all_functions.len(),
                base_function_count,
                cached_base_specializations
                    .map(|base| base.specializable_functions.len())
                    .unwrap_or(0),
                cached_base_specializations
                    .map(|base| base.runtime_specialization_map.len())
                    .unwrap_or(0)
            )
        });
    }

    fn register_inner_constructors(
        &mut self,
        inference_engine: &mut abstract_interp::InferenceEngine,
    ) {
        let precompiled_base = self.precompiled_base;
        // The ORIGINAL cached Base method tables (`method_tables` below is a
        // mutable *clone* of these — see `build_inference_engine`). Used to
        // decide whether a struct's constructors genuinely came from the Base
        // cache, independent of any user methods added in this compilation
        // (Issue #8121).
        let cached_method_tables = self.cached_method_tables;
        let all_structs = &self.all_structs;
        let imported_functions = &mut self.imported_functions;
        let method_tables = &mut self.method_tables;
        let shared_ctx = &mut self.shared_ctx;
        let function_infos = &mut self.function_infos;
        let specializable_functions = &mut self.specializable_functions;
        let inner_ctors = &mut self.inner_ctors;
        let global_index = &mut self.global_index;

        // Collect inner constructors from struct definitions (both top-level and module structs)
        // These are registered with the struct name, allowing Point(x, y) to call the inner constructor
        let inner_ctors_timer = profile::start("compile.inner_ctors_collect");
        // Use all_structs to include module structs (e.g., Dates.Date, Dates.DateTime)
        for (struct_def, module_path) in all_structs {
            if struct_def.inner_constructors.is_empty() {
                continue;
            }

            let qualified_struct_name = module_path
                .as_ref()
                .map(|path| format!("{}.{}", path, struct_def.name))
                .unwrap_or_else(|| struct_def.name.clone());

            // Always add struct name to imported_functions (needed for name resolution)
            imported_functions.insert(struct_def.name.clone());

            // When using cache, skip inner constructors that are already in cache
            // (i.e., Base struct inner constructors). User-defined inner constructors
            // need to be registered even when using cache.
            //
            // Issue #8121: the signal must be the ORIGINAL cached Base tables, NOT
            // the working `method_tables`. The working tables are a clone of the
            // cache plus every method registered earlier in `build_method_tables`,
            // so a USER parametric struct `Foo{T}` that also defines an outer
            // constructor `Foo(...)` makes `method_tables["Foo"]` non-empty and was
            // misclassified as a cached Base struct — its inner constructors were
            // then skipped, leaving the bare/braces call to fall back to raw
            // default field construction instead of the user inner/outer ctor.
            // Checking `cached_method_tables` skips genuine Base structs only.
            let skip_this_struct = if precompiled_base.is_some() {
                let is_cached_base_struct = precompiled_base
                    .map(|base| {
                        base.struct_defs.iter().any(|def| {
                            if module_path.is_some() {
                                def.name == qualified_struct_name
                            } else {
                                def.name == struct_def.name
                            }
                        })
                    })
                    .unwrap_or(false);
                is_cached_base_struct
                    && cached_method_tables
                        .and_then(|cached| cached.get(&struct_def.name))
                        .map(|t| !t.methods.is_empty())
                        .unwrap_or(false)
            } else {
                false
            };
            if skip_this_struct {
                continue;
            }

            // Get the type_id for this struct (or handle parametric struct)
            // Use short name since both short and qualified names are registered in struct_table
            let (type_id, is_parametric) =
                if let Some(info) = shared_ctx.struct_table.get(&struct_def.name) {
                    (info.type_id, false)
                } else if shared_ctx.parametric_structs.contains_key(&struct_def.name) {
                    // Parametric struct: use placeholder type_id, actual type resolved at call site
                    (0, true)
                } else {
                    continue;
                };

            for ctor in &struct_def.inner_constructors {
                let table = method_tables
                    .entry(struct_def.name.clone())
                    .or_insert_with(|| MethodTable::new(struct_def.name.clone()));

                // Add struct name to imported_functions immediately when registering inner constructor
                imported_functions.insert(struct_def.name.clone());

                let params: Vec<(String, JuliaType)> = ctor
                    .params
                    .iter()
                    .map(|p| (p.name.clone(), p.effective_type()))
                    .collect();

                let vm_params: Vec<(String, ValueType)> = params
                    .iter()
                    .map(|(name, jt)| {
                        (
                            name.clone(),
                            julia_type_to_value_type_with_table(jt, &shared_ctx.struct_table),
                        )
                    })
                    .collect();

                // Inner constructors return the struct type
                // For parametric structs, use Any since actual type is determined at call site
                let return_type = if is_parametric {
                    ValueType::Any
                } else {
                    ValueType::Struct(type_id)
                };

                // Preserve original JuliaTypes for type parameter binding (before params is moved)
                let param_julia_types: Vec<JuliaType> =
                    params.iter().map(|(_, jt)| jt.clone()).collect();

                // Use type params from the inner constructor's where clause
                let ctor_type_params: Vec<TypeParam> =
                    shared_ctx.expand_type_param_bounds(&ctor.type_params);

                let method_index = table.methods.len();
                let return_julia_type = if is_parametric {
                    None
                } else {
                    Some(JuliaType::Struct(struct_def.name.clone()))
                };
                let sig = MethodSig::from_julia_projections(
                    method_index,
                    *global_index,
                    params,
                    return_type.clone(),
                    return_julia_type,
                    false,
                    ctor_type_params.clone(),
                    // Inner constructors don't have varargs.
                    None,
                    None,
                );

                // Issue #8121: an inner constructor `Foo{T}(args) where {T}` is a
                // DISTINCT method from an outer constructor `Foo(args)` even when
                // their value-parameter signatures coincide — upstream Julia tells
                // them apart by the implicit `Type{Foo{T}}` vs `Type{Foo}` self
                // argument, which sjulia does not model. Both therefore project to
                // the same value-param signature, so `add_method`'s dedup would let
                // this inner ctor REPLACE the already-registered outer (the outer
                // is registered first, in `build_method_tables`). After that, a
                // bare `Foo(args)` call dispatches to the inner — whose `where`
                // type parameters are unbound — instead of the user outer (e.g.
                // `Angle2d{T}(theta::Number) where {T} = new{T}(T(theta))` vs
                // `Angle2d(theta::Number) = Angle2d{...}(theta)` → `UndefVarError:
                // T`). When such a collision is detected, keep BOTH methods (via
                // `add_method_keep_existing`) so dispatch routes a bare `Foo(args)`
                // call to the outer (selection tie-breaker 4 prefers the
                // fewer-`where`-param method) and `Foo{T}(args)` to the inner.
                let sig_canonical = sig.core_signature.canonicalize_signature_for_dedup();
                let collides_with_outer = sig.has_where_params()
                    && table.methods.iter().any(|existing| {
                        !existing.has_where_params()
                            && existing.vararg_param_index == sig.vararg_param_index
                            && existing.vararg_fixed_count == sig.vararg_fixed_count
                            && (existing.core_signature == sig.core_signature
                                || existing.core_signature.canonicalize_signature_for_dedup()
                                    == sig_canonical)
                    });

                if collides_with_outer {
                    table.add_method_keep_existing(sig.clone());
                } else {
                    table.add_method(sig.clone());
                }
                inference_engine.add_initial_method(struct_def.name.clone(), sig);

                // Record the index where this inner constructor will be stored
                let func_info_idx = function_infos.len();

                function_infos.push(FunctionInfo {
                    name: struct_def.name.clone(),
                    params: vm_params,
                    kwparams: vec![],
                    entry: 0,
                    return_type,
                    return_julia_type: None,
                    is_base_extension: false,
                    is_generated: false,
                    min_world: 1,
                    type_params: ctor_type_params,
                    param_julia_types,
                    code_start: 0, // Will be set during compilation
                    code_end: 0,   // Will be set during compilation
                    slot_names: Vec::new(),
                    slot_types: Vec::new(),
                    local_slot_count: 0,
                    param_slots: Vec::new(),
                    vararg_param_index: None, // Inner constructors don't have varargs
                    vararg_fixed_count: None,
                    inlining_meta: 0,
                    constprop_meta: 0,
                    nospecialize_meta: 0,
                    propagate_inbounds_meta: false,
                    nospecializeinfer_meta: false,
                    purity_meta: 0,
                    direct_return_type_param: None,
                    // Inner constructors report the struct definition's source line
                    // (Issue #5125).
                    def_line: struct_def.span.start_line as u32,
                });

                inner_ctors.push(InnerCtorInfo {
                    struct_name: struct_def.name.clone(),
                    type_id,
                    ctor: ctor.clone(),
                    func_info_idx,
                    module_path: module_path.clone(),
                });

                // Issue #4848: retain the inner constructor IR in
                // `specializable_functions` so reflection-time inference can analyze
                // the constructor body (e.g. `new(x, "x")`) and recover
                // PartialStruct-style field facts across the constructor return
                // boundary. Only non-parametric immutable constructors are
                // registered: parametric constructors resolve their concrete type at
                // the call site (return Any here), and mutable structs do not
                // preserve field-value facts. This does NOT add the constructor to
                // `spec_func_mapping`, so dispatch/codegen is unaffected.
                if !is_parametric && !struct_def.is_mutable {
                    let ctor_ir = crate::ir::core::Function {
                        name: struct_def.name.clone(),
                        params: ctor.params.clone(),
                        kwparams: ctor.kwparams.clone(),
                        type_params: shared_ctx.expand_type_param_bounds(&ctor.type_params),
                        return_type: None,
                        body: ctor.body.clone(),
                        is_base_extension: false,
                        is_runtime_eval: false,
                        span: ctor.span,
                    };
                    specializable_functions.push(SpecializableFunction {
                        ir: ctor_ir,
                        name: struct_def.name.clone(),
                        fallback_index: func_info_idx,
                    });
                }

                *global_index += 1;
            }
        }
        // Also add struct names to imported_functions so they can be called
        // Use all_structs to include module structs
        for (struct_def, _module_path) in all_structs {
            if !struct_def.inner_constructors.is_empty() {
                imported_functions.insert(struct_def.name.clone());
            }
        }
        profile::finish(inner_ctors_timer);
    }

    fn project_method_table_hierarchy(&mut self) {
        let base_function_count = self.base_function_count;
        let shared_ctx = &self.shared_ctx;
        let method_tables = &mut self.method_tables;

        // Populate struct_parents on all method tables for abstract dispatch tie-breaking (Issue #3144).
        // Build a map from concrete struct name to its declared parent abstract type.
        // This enables `dispatch()` to correctly prefer f(::MotorVehicle) over f(::NonMotorVehicle)
        // when the argument is Car where `struct Car <: MotorVehicle`.
        {
            let hierarchy_projection_timer =
                profile::start("compile.method_table_hierarchy_projection");
            let struct_hierarchy = build_struct_hierarchy_from_context(shared_ctx);
            let concrete_struct_names: Vec<String> = shared_ctx
                .struct_defs
                .iter()
                .map(|def| def.name.clone())
                .collect();
            let parametric_struct_names: Vec<String> =
                shared_ctx.parametric_structs.keys().cloned().collect();
            let abstract_type_names: Vec<String> = shared_ctx
                .abstract_types
                .iter()
                .map(|at| at.name.clone())
                .collect();

            // Issue #5646: parametric user structs (`struct Circle{T} <: Shape`) are
            // NOT in `struct_defs` — they instantiate lazily and live in
            // `parametric_structs` (Issue #5052). Without their declared parent here,
            // a `where {T<:Shape}` method failed to match a parametric argument
            // (`Circle{Float64}`): the struct-parent fallback fell into the
            // "conservatively accept unknown struct" branch, which is then either
            // rejected by the missing match arm or would wrongly accept an unrelated
            // struct. Seed every parametric struct's (base name -> declared parent
            // base name), including parentless ones (mapped to `None`), so the chain
            // walk in `struct_is_subtype_of_abstract` accepts `Circle <: Shape` and
            // rejects an unrelated `Box{T}`.

            // Issue #5056: user *abstract* type → declared parent links, kept in a
            // separate map so `struct_parents` stays struct-only (Issue #3144
            // tie-breaking). The dispatch subtype walk consults this to follow a
            // multi-level chain through intermediate user abstracts before reaching
            // a built-in abstract (`struct Tiny <: MyInt`, `abstract type MyInt <:
            // MyNum`, `abstract type MyNum <: Number` ⇒ `Tiny` dispatches `::Number`).

            // Issue #6348: the projection only depends on the program-wide
            // struct/abstract definitions, so build it ONCE and share the same
            // `Arc` across all (1100+) method tables instead of rebuilding and
            // cloning the full hierarchy per table (~37 ms per warm run).
            let shared_projection =
                std::sync::Arc::new(method_table::MethodTableProjection::build(
                    &struct_hierarchy,
                    &concrete_struct_names,
                    &parametric_struct_names,
                    &abstract_type_names,
                ));
            for table in method_tables.values_mut() {
                table.set_base_function_count(base_function_count);
                table.set_shared_projection(std::sync::Arc::clone(&shared_projection));
            }

            // Issue #5920: MethodTable keeps the shared hierarchy explicitly; do
            // not seed the inference thread-local registry from compile.
            profile::finish(hierarchy_projection_timer);
        }
    }

    fn analyze_module_lambda_captures(&mut self) {
        let program = self.program;
        let base_function_count = self.base_function_count;
        let opt_user_functions = self.opt_user_functions;
        let opt_main = self.opt_main;
        let shared_ctx = &mut self.shared_ctx;

        // Pre-analyze closure captures for lambda functions defined at module level (Issue #2358)
        //
        // Lambda functions (e.g., `f = () -> x + 1`) in @testset or other module-level blocks
        // are lifted to top-level functions named __lambda_N. They need to capture variables
        // from the outer scope. This must be done BEFORE the function compilation loop.
        //
        // First, collect the module-level local binding *names* to know what
        // variables are available. Capture analysis only consumes the name set,
        // so the legacy typed pre-scan (which also computed a ValueType per
        // binding and mixed-type tracking, all discarded here) is replaced by
        // the name-only walker (Issue #5922).
        {
            let lambda_captures_timer = profile::start("compile.module_lambda_captures");
            let mut module_scope_vars: HashSet<String> = HashSet::new();
            collect_local_binding_names_for_capture(&opt_main.stmts, &mut module_scope_vars);
            collect_testset_local_binding_names_for_capture(
                &opt_main.stmts,
                &mut module_scope_vars,
            );

            // Index all lifted `__lambda_N` functions by name. Do-block / arrow
            // lambdas are lifted FLAT to the top level (Issue #7600): a do-block
            // nested inside another do-block becomes two sibling top-level
            // functions, with the outer one referencing the inner one by name.
            // The nesting relationship — needed so the inner lambda can capture
            // the outer lambda's params / locals — is recovered from those
            // references below.
            let lambda_funcs: HashMap<&str, &Function> = program
                .functions
                .iter()
                .take(base_function_count)
                .chain(opt_user_functions.iter())
                .filter(|f| f.name.starts_with("__lambda_"))
                .map(|f| (f.name.as_str(), f))
                .collect();

            // The local scope each lambda contributes to its nested lambdas:
            // its own parameters plus the names it binds in its body.
            let lambda_local_scope: HashMap<&str, HashSet<String>> = lambda_funcs
                .iter()
                .map(|(&name, &func)| {
                    let mut scope: HashSet<String> =
                        func.params.iter().map(|p| p.name.clone()).collect();
                    collect_local_binding_names_for_capture(&func.body.stmts, &mut scope);
                    (name, scope)
                })
                .collect();

            // parent[child] = the lambda whose body references `child`. A
            // nested do-block is referenced from exactly one enclosing lambda.
            let mut parent_of: HashMap<&str, &str> = HashMap::new();
            for (&name, &func) in &lambda_funcs {
                for referenced in collect_referenced_names(func) {
                    if let Some((&child_name, _)) = lambda_funcs.get_key_value(referenced.as_str())
                    {
                        if child_name != name {
                            parent_of.entry(child_name).or_insert(name);
                        }
                    }
                }
            }

            // Depth of each lambda in the parent forest (root = 0), and the
            // direct free variables it references from an outer scope. The outer
            // scope is the module bindings plus the local scope of every
            // enclosing lambda, so a nested do-block can reference the outer
            // do-block's params / locals.
            let depth_of = |start: &str| -> usize {
                let mut depth = 0usize;
                let mut cur = parent_of.get(start).copied();
                let mut guard = 0usize;
                while let Some(anc) = cur {
                    depth += 1;
                    cur = parent_of.get(anc).copied();
                    guard += 1;
                    if guard > lambda_funcs.len() {
                        break; // defensive bound against a malformed cycle
                    }
                }
                depth
            };

            let mut direct_free: HashMap<&str, HashSet<String>> = HashMap::new();
            for (&name, &func) in &lambda_funcs {
                let mut outer_scope_vars = module_scope_vars.clone();
                let mut ancestor = parent_of.get(name).copied();
                let mut guard = 0usize;
                while let Some(anc) = ancestor {
                    if let Some(scope) = lambda_local_scope.get(anc) {
                        outer_scope_vars.extend(scope.iter().cloned());
                    }
                    ancestor = parent_of.get(anc).copied();
                    guard += 1;
                    if guard > lambda_funcs.len() {
                        break;
                    }
                }
                direct_free.insert(name, analyze_free_variables(func, &outer_scope_vars));
            }

            // Propagate captures bottom-up (children before parents) so that an
            // intermediate lambda also captures any variable a *descendant*
            // lambda needs from a scope above it — the "capture to pass it down"
            // chain that mirrors the named-nested-function deep-nesting analysis
            // (Issue #1744), but for flat lifted do-block / arrow lambdas
            // (Issue #7600). A name bound in the lambda's own scope is available
            // directly when it builds the child closure, so it is dropped from
            // the lambda's own capture set.
            let mut names_by_depth_desc: Vec<&str> = lambda_funcs.keys().copied().collect();
            names_by_depth_desc.sort_by_key(|n| std::cmp::Reverse(depth_of(n)));

            let mut captures: HashMap<&str, HashSet<String>> = HashMap::new();
            for &name in &names_by_depth_desc {
                let mut caps = direct_free.get(name).cloned().unwrap_or_default();
                // Pull up any still-unsatisfied captures of direct children.
                for (&child, &parent) in &parent_of {
                    if parent == name {
                        if let Some(child_caps) = captures.get(child) {
                            caps.extend(child_caps.iter().cloned());
                        }
                    }
                }
                // Drop names bound in this lambda's own scope: they resolve from
                // this frame, not from a capture.
                if let Some(scope) = lambda_local_scope.get(name) {
                    caps.retain(|n| !scope.contains(n));
                }
                captures.insert(name, caps);
            }

            for (&name, caps) in &captures {
                if !caps.is_empty() {
                    shared_ctx
                        .closure_captures
                        .insert(name.to_string(), caps.clone());
                }
            }
            profile::finish(lambda_captures_timer);
        }
    }

    fn compile_functions(&mut self) -> CResult<()> {
        let program = self.program;
        self.reused_base = if self.precompiled_base.is_some() {
            vec![true; self.function_infos.len()]
        } else {
            vec![false; self.function_infos.len()]
        };
        let precompiled_base = self.precompiled_base;
        let base_function_count = self.base_function_count;
        let cached_base_len = self.cached_base_len;
        let inline_start_idx = self.inline_start_idx;
        let first_user_function_idx = self.first_user_function_idx;
        let all_functions = &self.all_functions;
        let func_idx_to_parent = &self.func_idx_to_parent;
        let func_index_map = &self.func_index_map;
        let user_function_names = &self.user_function_names;
        let base_function_names = &self.base_function_names;
        let shadowed_user_globals = &self.shadowed_user_globals;
        let imported_functions = &self.imported_functions;
        let module_functions = &self.module_functions;
        let module_imports_map = &self.module_imports_map;
        let module_usings_map = &self.module_usings_map;
        let method_tables = &self.method_tables;
        let module_exports = &self.module_exports;
        let usings_set = &self.usings_set;
        let abstract_type_names = &self.abstract_type_names;
        let module_constants = &self.module_constants;
        let function_infos = &mut self.function_infos;
        let reused_base = &mut self.reused_base;
        let code = &mut self.code;
        let shared_ctx = &mut self.shared_ctx;

        // Compile each function.
        //
        // Keep cached Base bytecode out of the mutable suffix while compiling user
        // functions/main. The final `CompiledProgram.code` is still a single Vec,
        // but deferring the Base prefix copy lets slotization and peephole passes run
        // on the user/main suffix only instead of repeatedly copying the cached Base
        // prefix through protected ranges (Issue #6348).
        let emit_functions_timer = profile::start("compile.emit_functions");
        let shadowed_global_types = shadowed_user_globals
            .iter()
            .map(|name| (name.clone(), shared_ctx.global_types.remove(name)))
            .collect::<Vec<_>>();
        for (idx, (func, module_path)) in all_functions.iter().enumerate() {
            let is_user_function_scope = if idx >= inline_start_idx {
                func_idx_to_parent
                    .get(&idx)
                    .map(|parent| user_function_names.contains(parent))
                    .unwrap_or(true)
            } else {
                idx >= first_user_function_idx
            };
            if is_user_function_scope {
                for (name, ty) in &shadowed_global_types {
                    if let Some(ty) = ty {
                        shared_ctx.global_types.insert(name.clone(), ty.clone());
                    }
                }
            } else {
                for name in shadowed_user_globals {
                    shared_ctx.global_types.remove(name);
                }
            }
            let hides_user_globals = idx < self.base_function_count
                || func_idx_to_parent
                    .get(&idx)
                    .is_some_and(|parent| base_function_names.contains(parent));
            // Map all_functions index to function_infos index
            let func_info_idx = func_index_map[idx];

            // When using cache, check if this function already has bytecode from cache
            // A function has valid cache bytecode if its code_start != code_end
            if precompiled_base.is_some() && func_info_idx < cached_base_len {
                let fi = &function_infos[func_info_idx];
                if fi.code_start != fi.code_end {
                    // Function has valid bytecode from cache, skip compilation
                    continue;
                }
            }

            let entry = code.len();
            function_infos[func_info_idx].entry = entry;
            reused_base[func_info_idx] = false; // This is a user function, not reused from cache

            let mut function_imports = imported_functions.clone();
            function_imports.insert(func.name.clone());
            if let Some(module_path) = module_path {
                if let Some(module_funcs) = module_functions.get(module_path) {
                    function_imports.extend(module_funcs.iter().cloned());
                }
                if let Some(module_imports) = module_imports_map.get(module_path) {
                    function_imports.extend(module_imports.iter().cloned());
                }
            }
            let function_scope_usings = module_path
                .as_ref()
                .and_then(|path| module_usings_map.get(path))
                .map(Vec::as_slice)
                .unwrap_or(program.usings.as_slice());
            let resolved_usings = resolve_scope_using_imports(
                function_scope_usings,
                module_path.as_deref().unwrap_or(""),
                module_functions,
            );
            // Check if this function is a closure with captured variables
            // Clone the captures before creating the compiler (to avoid borrow conflicts)
            //
            // For nested functions, closure_captures uses qualified names like "parent#nested"
            // We use func_idx_to_parent to find the exact parent for this function index,
            // which allows disambiguating between multiple nested functions with the same name
            // from different parents (Issue #1743).
            let closure_captures = if let Some(parent) = func_idx_to_parent.get(&idx) {
                // This is a nested function - look up by qualified name
                let qualified_name = format!("{}#{}", parent, func.name);
                shared_ctx
                    .closure_captures
                    .get(&qualified_name)
                    .cloned()
                    .unwrap_or_default()
            } else {
                // Top-level or module function - look up by simple name
                shared_ctx
                    .closure_captures
                    .get(&func.name)
                    .cloned()
                    .unwrap_or_default()
            };
            let normalized_type_params = shared_ctx.expand_type_param_bounds(&func.type_params);

            let mut compiler = CoreCompiler::new_for_function(
                method_tables,
                module_functions,
                module_exports,
                &function_imports,
                usings_set,
                resolved_usings,
                shared_ctx,
                abstract_type_names,
                module_constants,
            );
            if hides_user_globals {
                compiler.hidden_user_globals = shadowed_user_globals.clone();
            }

            // Set captured_vars so that load_local emits LoadCaptured for those variables
            compiler.captured_vars = closure_captures;

            // Set the current function name for nested function disambiguation
            // For nested functions, use the qualified name (parent#nested) so that
            // deeper nesting levels can build the full qualified path (Issue #1744)
            let current_func_name = if let Some(parent) = func_idx_to_parent.get(&idx) {
                format!("{}#{}", parent, func.name)
            } else {
                func.name.clone()
            };
            compiler.current_function_name = Some(current_func_name);

            // Set module path for resolving unqualified struct names inside module functions
            compiler.current_module_path = module_path.clone();
            // Names imported into this module via `using`/`import` keep cross-module
            // dispatch pooling and must NOT be redirected to the module-owned table
            // (Issue #7575).
            compiler.current_module_imports = module_path
                .as_ref()
                .and_then(|path| module_imports_map.get(path))
                .cloned()
                .unwrap_or_default();
            compiler.in_base_function_scope = idx < base_function_count
                || func_idx_to_parent
                    .get(&idx)
                    .is_some_and(|parent| base_function_names.contains(parent));

            // Set type parameters from where clause for type binding support
            compiler.current_type_params = normalized_type_params.clone();
            compiler.current_type_param_index = normalized_type_params
                .iter()
                .enumerate()
                .map(|(i, tp)| (tp.name.clone(), i))
                .collect();

            // Collect type parameter names from the function's where clause
            let func_type_param_names: HashSet<&str> = normalized_type_params
                .iter()
                .map(|tp| tp.name.as_str())
                .collect();

            // Detect Val{N} patterns and mark N as a value parameter
            // For parameters like ::Val{N} where N, N should be treated as I64, not DataType
            for param in &func.params {
                if let JuliaType::Struct(type_name) = param.effective_type() {
                    if type_name.starts_with("Val{") && type_name.ends_with("}") {
                        // Extract the type argument (e.g., "N" from "Val{N}")
                        let type_arg = &type_name[4..type_name.len() - 1];
                        // If this type arg is a where clause type parameter, it's a value parameter
                        if func_type_param_names.contains(type_arg) {
                            compiler.val_type_params.insert(type_arg.to_string());
                        }
                    } else if type_name.starts_with("NTuple{") && type_name.ends_with("}") {
                        // Collect every length value parameter, including those of
                        // nested NTuple element types such as `NTuple{N,NTuple{M,T}}`
                        // where both N and M are value parameters (Issue #4842).
                        collect_ntuple_value_params(
                            &type_name,
                            &func_type_param_names,
                            &mut compiler.val_type_params,
                        );
                    } else {
                        collect_array_rank_value_params(
                            &type_name,
                            &func_type_param_names,
                            &mut compiler.val_type_params,
                        );
                    }
                }
            }

            // Set up parameter types in locals
            for param in &func.params {
                let param_ty = param.effective_type();
                // Ensure parametric struct instantiations exist (e.g., Complex{Float64})
                if let JuliaType::Struct(name) = &param_ty {
                    if name.contains('{') && !compiler.shared_ctx.struct_table.contains_key(name) {
                        // Parse type arguments and create instantiation
                        if let Some(brace_idx) = name.find('{') {
                            let base_name = &name[..brace_idx];
                            let type_args_str = &name[brace_idx + 1..name.len() - 1];

                            // Check if any type arg is a where clause type parameter
                            let Some(type_args) = parse_type_args_recursive(type_args_str) else {
                                continue;
                            };
                            let has_type_param = type_args.iter().any(|arg| {
                                type_expr_contains_type_param(arg, &func_type_param_names)
                            });

                            // Skip instantiation if any type arg is a where clause type parameter
                            if !has_type_param {
                                let _ = compiler
                                    .shared_ctx
                                    .resolve_instantiation_with_type_expr(base_name, &type_args);
                            }
                        }
                    }
                }
                // Varargs parameters are bound as a Tuple collector at runtime,
                // even when each accepted argument has a typed annotation such as
                // `xs::Vector{Int64}...` (Issue #3914).
                let vt = if param.is_varargs {
                    ValueType::Tuple
                } else if matches!(param_ty, JuliaType::Dict) {
                    // Bare `::Dict` is the `Dict{K,V}` UnionAll family after
                    // Value::Dict carrier removal. Keep parameter storage
                    // dynamic so method bodies use pure-Julia Dict dispatch
                    // instead of legacy Dict slot/builtin paths. (Issue #7632)
                    ValueType::Any
                } else {
                    compiler.julia_type_to_value_type_with_ctx(&param_ty)
                };
                compiler.locals.insert(param.name.clone(), vt.clone());
                compiler.initialized_locals.insert(param.name.clone());
                // Track parameters with JuliaTypes that ValueType cannot represent
                // precisely, so infer_julia_type can recover the dispatch type.
                // This includes narrow integers (e.g., Int32 instead of ValueType::I64)
                // and parametric arrays (e.g., Vector{Int32} instead of ValueType::Array).
                // This is needed for correct compile-time dispatch of calls like
                // gcd(num, den) where num::Int32 and HOF inference for map(f, xs, ys).
                if param.is_varargs {
                    compiler
                        .julia_type_locals
                        .insert(param.name.clone(), JuliaType::Tuple);
                } else if param_ty.is_narrow_integer()
                    || matches!(param_ty, JuliaType::VectorOf(_) | JuliaType::MatrixOf(_))
                    || matches!(param_ty, JuliaType::Dict)
                    || matches!(&param_ty, JuliaType::Struct(name)
                        if !name.contains('{')
                            && compiler.shared_ctx.parametric_structs.contains_key(name))
                {
                    compiler
                        .julia_type_locals
                        .insert(param.name.clone(), param_ty.clone());
                }
                // Track parameters with TypeVar type annotations (e.g., x::T where T<:Integer)
                // so that variable references resolve to the bound type for proper
                // dispatch (Issue #2556).
                if let JuliaType::TypeVar(_, Some(bound_name)) = &param_ty {
                    if let Some(bound_type) = JuliaType::from_name(bound_name) {
                        if bound_type.is_abstract_numeric() {
                            // Abstract numeric bounds (`T<:Integer`, `T<:Real`, ...) accept many
                            // concrete runtime types (Int32, Int64, BigInt, ...). Storing the
                            // abstract bound in `julia_type_locals` would make calls such as
                            // `div(x, y)` statically dispatch to the generic `Any` fallback
                            // (`floor(x / y)` → Float64) instead of runtime-dispatching to the
                            // concrete integer method. Mirror a direct `x::Integer` annotation,
                            // which leaves the variable inferred as `Any` and relies on the
                            // `abstract_numeric_params` set plus runtime dispatch (Issue #5398).
                            compiler.abstract_numeric_params.insert(param.name.clone());
                        } else {
                            compiler
                                .julia_type_locals
                                .insert(param.name.clone(), bound_type.clone());
                        }
                    }
                }
                // Track parameters with Any type - these should preserve Any on reassignment
                if matches!(param_ty, JuliaType::Any) {
                    compiler.any_params.insert(param.name.clone());
                }
                // Track parameters with abstract numeric type annotations (Number, Real, etc.)
                // Binary operations on these must use runtime dispatch (Issue #2498)
                if param_ty.is_abstract_numeric() {
                    compiler.abstract_numeric_params.insert(param.name.clone());
                }
            }

            // Set up kwparam types in locals
            // For varargs kwargs (kwargs...), type is always NamedTuple
            // For required kwargs (Undef default), use type annotation if available
            // For unannotated optional kwargs, use Any since they can receive any type
            // at runtime regardless of the default's type (Issue #5425)
            for kwparam in &func.kwparams {
                let vt = if kwparam.is_varargs {
                    // Varargs kwargs collects all remaining kwargs as Pairs (Julia's Base.Pairs)
                    ValueType::Pairs
                } else {
                    let is_required = is_required_kwarg(&kwparam.default);
                    if is_required {
                        // Required kwarg - use type annotation if available
                        kwparam
                            .type_annotation
                            .as_ref()
                            .map(|jt| {
                                julia_type_to_value_type_with_table(
                                    jt,
                                    &compiler.shared_ctx.struct_table,
                                )
                            })
                            .unwrap_or(ValueType::Any)
                    } else if kwparam.body_evaluated_default {
                        // Slot is initialized to the `Undef` sentinel by the kwsorter
                        // and overwritten by the body prologue with the real default of
                        // any type, so the slot must be `Any` (Issue #5121).
                        ValueType::Any
                    } else if is_unannotated_optional_kwparam(kwparam) {
                        // Same rationale as `KwParamInfo.ty` above: an unannotated optional
                        // kwarg must be `Any` in the compiled body so a passed value of any
                        // type flows through `return kw` without a typed-slot rejection
                        // (Issue #5425, generalizing #5416).
                        ValueType::Any
                    } else {
                        infer_default_type(&kwparam.default)
                    }
                };
                compiler.locals.insert(kwparam.name.clone(), vt);
                compiler.initialized_locals.insert(kwparam.name.clone());
            }

            // Register type parameters from where clause as DataType locals
            // This enables T(x) calls where T is a type parameter: function f(x::T) where T; T(1); end
            for tp in &normalized_type_params {
                // Skip Val{N} value parameters - they are I64, not DataType
                if !compiler.val_type_params.contains(&tp.name) {
                    compiler.locals.insert(tp.name.clone(), ValueType::DataType);
                    compiler.initialized_locals.insert(tp.name.clone());
                }
            }

            // Pre-populate locals with inferred types to ensure consistent type usage
            // This prevents bugs where a variable is first assigned as I64 then used as F64
            // Protect function parameters (and kwargs) from being overwritten by local assignments
            // This fixes the bug where parameter reassignment (e.g., a = abs(a)) causes type mismatch
            let protected: HashSet<String> = func
                .params
                .iter()
                .map(|p| p.name.clone())
                .chain(func.kwparams.iter().map(|k| k.name.clone()))
                .collect();
            collect_local_types_with_mixed_tracking(
                &func.body.stmts,
                &mut compiler.locals,
                &protected,
                &compiler.shared_ctx.struct_table,
                &compiler.shared_ctx.global_types,
                &mut compiler.mixed_type_vars,
            );

            // Compile function body with implicit return handling
            // In Julia, the last expression in a function is its return value.
            // Issue #5425 / #5466: when the body returns an unannotated optional kwarg
            // — directly (`g(; n = 0) = n`) or derived through a computation
            // (`g2(; n = 0) = n + 1`) — the runtime value can be any type, so emit the
            // body against `Any` to force `ReturnAny` instead of a typed return that
            // would reject a differently-typed result. `FunctionInfo`'s own
            // `return_type` stays precise for reflection (`Base.infer_return_type`).
            let body_return_type = if returns_unannotated_optional_kwparam_value(func)
                || returns_untyped_param_power_value(func)
            {
                ValueType::Any
            } else {
                function_infos[func_info_idx].return_type.clone()
            };
            compiler.compile_function_body(&func.body, body_return_type)?;
            // Patch @goto jumps after function body compilation
            compiler.patch_goto_jumps()?;

            if hides_user_globals {
                for name in shadowed_user_globals {
                    compiler.shared_ctx.global_types.remove(name);
                }
            }

            let code_start = entry;
            let mut func_code = compiler.code;
            relocate_jumps(&mut func_code, 0, entry);
            code.extend(func_code);
            let code_end = code.len();

            // Update function boundaries for future caching
            function_infos[func_info_idx].code_start = code_start;
            function_infos[func_info_idx].code_end = code_end;
        }
        for (name, ty) in &shadowed_global_types {
            if let Some(ty) = ty {
                shared_ctx.global_types.insert(name.clone(), ty.clone());
            }
        }
        profile::finish(emit_functions_timer);
        Ok(())
    }

    fn compile_inner_constructors(&mut self) -> CResult<()> {
        let program = self.program;
        let inner_ctors = &self.inner_ctors;
        let method_tables = &self.method_tables;
        let module_functions = &self.module_functions;
        let module_exports = &self.module_exports;
        let imported_functions = &self.imported_functions;
        let module_imports_map = &self.module_imports_map;
        let module_usings_map = &self.module_usings_map;
        let usings_set = &self.usings_set;
        let abstract_type_names = &self.abstract_type_names;
        let module_constants = &self.module_constants;
        let function_infos = &mut self.function_infos;
        let reused_base = &mut self.reused_base;
        let code = &mut self.code;
        let shared_ctx = &mut self.shared_ctx;

        // Compile inner constructors
        // These run with current_struct_type_id set so new() creates the correct struct type
        let emit_inner_constructors_timer = profile::start("compile.emit_inner_constructors");
        for ctor_info in inner_ctors.iter() {
            let entry = code.len();
            let func_info_idx = ctor_info.func_info_idx;
            function_infos[func_info_idx].entry = entry;

            // Resolve the constructor body's name lookups in the struct's DEFINING
            // module, not at the call site. Upstream Julia always evaluates a
            // method body's names in its definition module, so a module-private
            // helper function, type, or const referenced inside an inner
            // constructor must be visible without the caller doing `using .Mod`
            // (Issue #8069). Mirror the module-scope setup that ordinary module
            // functions get in `compile_functions`.
            let module_path = ctor_info.module_path.as_deref();
            let mut ctor_imports = imported_functions.clone();
            if let Some(path) = module_path {
                if let Some(module_funcs) = module_functions.get(path) {
                    ctor_imports.extend(module_funcs.iter().cloned());
                }
                if let Some(module_imports) = module_imports_map.get(path) {
                    ctor_imports.extend(module_imports.iter().cloned());
                }
            }
            let ctor_scope_usings = module_path
                .and_then(|path| module_usings_map.get(path))
                .map(Vec::as_slice)
                .unwrap_or(program.usings.as_slice());
            let resolved_usings = resolve_scope_using_imports(
                ctor_scope_usings,
                module_path.unwrap_or(""),
                module_functions,
            );
            let normalized_type_params =
                shared_ctx.expand_type_param_bounds(&ctor_info.ctor.type_params);

            let mut compiler = CoreCompiler::new_for_function(
                method_tables,
                module_functions,
                module_exports,
                &ctor_imports,
                usings_set,
                resolved_usings,
                shared_ctx,
                abstract_type_names,
                module_constants,
            );

            // Resolve unqualified module-private struct names in the defining
            // module and keep cross-module dispatch pooling consistent with it,
            // exactly as `compile_functions` does for module methods (#8069).
            compiler.current_module_path = ctor_info.module_path.clone();
            compiler.current_module_imports = module_path
                .and_then(|path| module_imports_map.get(path))
                .cloned()
                .unwrap_or_default();

            // Set current_struct_type_id so new() creates the correct struct type
            compiler.current_struct_type_id = Some(ctor_info.type_id);

            // For parametric structs (type_id=0), set the base name for dynamic struct creation
            if ctor_info.type_id == 0 {
                compiler.current_parametric_struct_name = Some(ctor_info.struct_name.clone());
            }

            // Set type parameters from the constructor's where clause (e.g., where T)
            compiler.current_type_params = normalized_type_params.clone();
            compiler.current_type_param_index = normalized_type_params
                .iter()
                .enumerate()
                .map(|(i, tp)| (tp.name.clone(), i))
                .collect();

            // Set up parameter types in locals
            for param in &ctor_info.ctor.params {
                let param_ty = param.effective_type();
                let vt = if matches!(param_ty, JuliaType::Dict) {
                    // See function-parameter setup above. (Issue #7632)
                    ValueType::Any
                } else {
                    compiler.julia_type_to_value_type_with_ctx(&param_ty)
                };
                compiler.locals.insert(param.name.clone(), vt);
                compiler.initialized_locals.insert(param.name.clone());
                // Track parameters with Any type - these should preserve Any on reassignment
                if matches!(param_ty, JuliaType::Any) {
                    compiler.any_params.insert(param.name.clone());
                }
                // Track parameters with abstract numeric type annotations (Issue #2498)
                if param_ty.is_abstract_numeric() {
                    compiler.abstract_numeric_params.insert(param.name.clone());
                }
            }

            // Determine which `where`-clause type parameters are recoverable from a
            // constructor argument at runtime (they appear in some parameter's type
            // annotation, e.g. `Bar(x::T)`). Only those can be safely materialized
            // by `new{...}` from the constructor frame; explicit-only parameters
            // (`Foo{T}(x)` with an untyped `x`) need call-site type-arg plumbing
            // that does not yet exist, so they fall back to the legacy runtime path
            // (Issue #5059).
            {
                let where_names: HashSet<&str> = ctor_info
                    .ctor
                    .type_params
                    .iter()
                    .map(|tp| tp.name.as_str())
                    .collect();
                for param in &ctor_info.ctor.params {
                    collect_referenced_type_var_names(
                        &param.effective_type(),
                        &where_names,
                        &mut compiler.ctor_arg_bound_type_vars,
                    );
                }
            }

            // Register type parameters from constructor's where clause as DataType locals
            // This enables T(x) calls inside inner constructors: function Foo{T}(x) where T; T(1); end
            for tp in &ctor_info.ctor.type_params {
                compiler.locals.insert(tp.name.clone(), ValueType::DataType);
                compiler.initialized_locals.insert(tp.name.clone());
            }

            // Protect constructor parameters from being overwritten by local assignments
            // This fixes the bug where parameter reassignment (e.g., num = div(num, g)) causes type mismatch
            let protected: HashSet<String> = ctor_info
                .ctor
                .params
                .iter()
                .map(|p| p.name.clone())
                .collect();
            collect_local_types_with_mixed_tracking(
                &ctor_info.ctor.body.stmts,
                &mut compiler.locals,
                &protected,
                &compiler.shared_ctx.struct_table,
                &compiler.shared_ctx.global_types,
                &mut compiler.mixed_type_vars,
            );

            // Compile constructor body
            let return_type = ValueType::Struct(ctor_info.type_id);
            compiler.compile_function_body(&ctor_info.ctor.body, return_type)?;
            // Patch @goto jumps after constructor body compilation
            compiler.patch_goto_jumps()?;

            let code_start = entry;
            let mut func_code = compiler.code;
            relocate_jumps(&mut func_code, 0, entry);
            code.extend(func_code);
            let code_end = code.len();

            // Update constructor function boundaries
            function_infos[func_info_idx].code_start = code_start;
            function_infos[func_info_idx].code_end = code_end;

            // Mark this inner constructor as not reused from cache (needs slot transformation)
            reused_base[func_info_idx] = false;
        }
        profile::finish(emit_inner_constructors_timer);
        Ok(())
    }

    fn compile_modules(&mut self) -> CResult<()> {
        // Record where modules start (this will be the entry point if there are modules)
        self.modules_entry = self.code.len();
        let all_modules = self.all_modules.clone();

        // Compile modules (execute their bodies before main)
        let emit_modules_timer = profile::start("compile.emit_modules");
        for module in all_modules {
            self.compile_module_recursive(module, &module.name)?;
        }
        profile::finish(emit_modules_timer);
        Ok(())
    }

    fn compile_module_recursive(
        &mut self,
        module: &crate::ir::core::Module,
        module_path: &str,
    ) -> CResult<()> {
        for submodule in &module.submodules {
            let submodule_path = format!("{}.{}", module_path, submodule.name);
            self.compile_module_recursive(submodule, &submodule_path)?;
        }

        {
            let module_offset = self.code.len();
            let module_imports_map = &self.module_imports_map;
            let method_tables = &self.method_tables;
            let module_functions = &self.module_functions;
            let module_exports = &self.module_exports;
            let imported_functions = &self.imported_functions;
            let usings_set = &self.usings_set;
            let abstract_type_names = &self.abstract_type_names;
            let module_constants = &self.module_constants;
            let code = &mut self.code;
            let shared_ctx = &mut self.shared_ctx;

            // Create module-local imported functions set: includes all functions defined in this module
            // and functions imported via `using` statements in this module
            let mut module_imported_functions = imported_functions.clone();
            for func in &module.functions {
                module_imported_functions.insert(func.name.clone());
            }

            // Add functions imported via module-local using statements
            if let Some(module_imports) = module_imports_map.get(module_path) {
                module_imported_functions.extend(module_imports.iter().cloned());
            }
            let resolved_usings =
                resolve_scope_using_imports(&module.usings, module_path, module_functions);

            let mut module_compiler = CoreCompiler::new(
                method_tables,
                module_functions,
                module_exports,
                &module_imported_functions,
                usings_set,
                resolved_usings,
                shared_ctx,
                abstract_type_names,
                module_constants,
            );

            // Set module path for qualified constant storage
            module_compiler.current_module_path = Some(module_path.to_string());

            // Compile module body
            module_compiler.compile_block(&module.body)?;

            // After compiling the module body, create a ModuleValue and store it
            // This makes the module accessible as a variable (e.g., TestMod)
            module_compiler.emit(Instr::PushModule(Box::new(
                crate::vm::instr::ModuleOperands {
                    name: module.name.clone(),
                    exports: module.exports.clone(),
                    publics: module.publics.clone(),
                },
            )));
            module_compiler.emit(Instr::StoreAny(module.name.clone()));

            // Don't emit ReturnUnit - let execution flow through to next module or main

            // Patch @goto jumps after module body compilation
            module_compiler.patch_goto_jumps()?;

            let mut module_code = module_compiler.code;
            relocate_jumps(&mut module_code, 0, module_offset);
            code.extend(module_code);
        }

        Ok(())
    }

    fn compile_base_main_prefix(&mut self) -> CResult<()> {
        let program = self.program;
        let opt_main = self.opt_main;
        let method_tables = &self.method_tables;
        let module_functions = &self.module_functions;
        let module_exports = &self.module_exports;
        let imported_functions = &self.imported_functions;
        let usings_set = &self.usings_set;
        let abstract_type_names = &self.abstract_type_names;
        let module_constants = &self.module_constants;
        let code = &mut self.code;
        let shared_ctx = &mut self.shared_ctx;

        let stmts = &opt_main.stmts;
        let boundary_idx = stmts.iter().position(is_base_user_main_boundary);
        let (base_main_stmts, user_main_stmts) = if let Some(idx) = boundary_idx {
            (&stmts[..idx], &stmts[idx + 1..])
        } else {
            (&[][..], stmts.as_slice())
        };

        if boundary_idx.is_some() {
            let mut shadowed_user_globals = HashSet::new();
            collect_assigned_binding_names(user_main_stmts, &mut shadowed_user_globals);
            self.deferred_shadowed_global_types = shadowed_user_globals
                .iter()
                .map(|name| (name.clone(), shared_ctx.global_types.remove(name)))
                .collect();
        }

        if base_main_stmts.is_empty() {
            return Ok(());
        }

        let emit_base_main_timer = profile::start("compile.emit_base_main_prefix");
        let base_main_entry = code.len();
        self.base_main_entry = Some(base_main_entry);
        let resolved_usings = resolve_scope_using_imports(&program.usings, "", module_functions);

        let mut base_main_compiler = CoreCompiler::new(
            method_tables,
            module_functions,
            module_exports,
            imported_functions,
            usings_set,
            resolved_usings,
            shared_ctx,
            abstract_type_names,
            module_constants,
        );

        let protected: HashSet<String> = HashSet::new();
        collect_local_types_with_mixed_tracking(
            base_main_stmts,
            &mut base_main_compiler.locals,
            &protected,
            &base_main_compiler.shared_ctx.struct_table,
            &base_main_compiler.shared_ctx.global_types,
            &mut base_main_compiler.mixed_type_vars,
        );
        for stmt in base_main_stmts {
            base_main_compiler.compile_stmt(stmt)?;
        }
        base_main_compiler.patch_goto_jumps()?;

        let mut base_main_code = base_main_compiler.code;
        relocate_jumps(&mut base_main_code, 0, base_main_entry);
        code.extend(base_main_code);
        profile::finish(emit_base_main_timer);
        Ok(())
    }

    fn compile_main(&mut self) -> CResult<()> {
        let program = self.program;
        let opt_main = self.opt_main;
        let modules_entry = self.modules_entry;
        let all_modules = &self.all_modules;
        let method_tables = &self.method_tables;
        let module_functions = &self.module_functions;
        let module_exports = &self.module_exports;
        let imported_functions = &self.imported_functions;
        let usings_set = &self.usings_set;
        let abstract_type_names = &self.abstract_type_names;
        let module_constants = &self.module_constants;
        let code = &mut self.code;
        let shared_ctx = &mut self.shared_ctx;

        // Compile main block
        let emit_main_timer = profile::start("compile.emit_main");
        let main_entry = code.len();
        // Entry point: Base top-level initializers must run before user modules,
        // because module bodies may call Base functions whose internal constants
        // are initialized in the Base main prefix (Issue #7570).
        self.entry = if let Some(base_main_entry) = self.base_main_entry {
            base_main_entry
        } else if !all_modules.is_empty() {
            modules_entry
        } else {
            main_entry
        };
        let resolved_usings = resolve_scope_using_imports(&program.usings, "", module_functions);
        let mut main_compiler = CoreCompiler::new(
            method_tables,
            module_functions,
            module_exports,
            imported_functions,
            usings_set,
            resolved_usings,
            shared_ctx,
            abstract_type_names,
            module_constants,
        );

        let stmts = &opt_main.stmts;
        let protected: HashSet<String> = HashSet::new();
        let boundary_idx = stmts.iter().position(is_base_user_main_boundary);
        let user_main_stmts = if let Some(idx) = boundary_idx {
            (&stmts[..idx], &stmts[idx + 1..])
        } else {
            (&[][..], stmts.as_slice())
        }
        .1;
        for (name, ty) in self.deferred_shadowed_global_types.drain(..) {
            if let Some(ty) = ty {
                main_compiler.shared_ctx.global_types.insert(name, ty);
            }
        }

        // Pre-populate user-main locals only after Base main has compiled. Scanning
        // the merged Base+user block at once lets a user binding like `idx = [...]`
        // change the static type of Base's own `idx` temporaries before those Base
        // statements compile (Issue #5590).
        collect_local_types_with_mixed_tracking(
            user_main_stmts,
            &mut main_compiler.locals,
            &protected,
            &main_compiler.shared_ctx.struct_table,
            &main_compiler.shared_ctx.global_types,
            &mut main_compiler.mixed_type_vars,
        );

        // Compile all statements except the last one
        if !user_main_stmts.is_empty() {
            for stmt in &user_main_stmts[..user_main_stmts.len() - 1] {
                main_compiler.compile_stmt(stmt)?;
            }

            // For the last statement, if it's an expression, return its value
            // In Julia, assignment is also an expression that returns the assigned value
            let last_stmt = &user_main_stmts[user_main_stmts.len() - 1];
            match last_stmt {
                Stmt::Expr { expr, .. } => {
                    let ty = main_compiler.compile_expr(expr)?;
                    main_compiler.emit_return_for_type(ty);
                }
                // Assignment as last statement returns the assigned value (Julia semantics)
                Stmt::Assign { var, value, .. } => {
                    if main_compiler.const_bindings.contains(var)
                        && !main_compiler.pending_const_bindings.remove(var)
                        && !main_compiler.strict_undefined_check
                    {
                        main_compiler.emit(Instr::PushStr(format!(
                            "invalid assignment to constant Main.{}",
                            var
                        )));
                        main_compiler.emit(Instr::ThrowError);
                        main_compiler.emit_return_for_type(ValueType::Nothing);
                    } else {
                        let was_pending_const = main_compiler.pending_const_bindings.remove(var);
                        let folded_const_value =
                            if was_pending_const && !main_compiler.strict_undefined_check {
                                crate::compile::const_prop::fold_expr_const_value(value, &|name| {
                                    main_compiler.const_values.get(name).cloned()
                                })
                            } else {
                                None
                            };
                        // Check for wider type as in compile_stmt
                        let target_ty = main_compiler.locals.get(var).cloned();
                        let ty = main_compiler.compile_expr(value)?;

                        // Handle widening for consistency with compile_stmt
                        // For mixed-type variables, use dynamic typing (don't convert I64 to F64)
                        let is_mixed_type = main_compiler.mixed_type_vars.contains(var);
                        let final_ty = match (target_ty, ty.clone()) {
                            // For mixed-type variables, preserve the actual type
                            (Some(ValueType::Any), ValueType::I64)
                            | (Some(ValueType::Any), ValueType::F64)
                                if is_mixed_type =>
                            {
                                ValueType::Any
                            }
                            (Some(target), incoming)
                                if is_mixed_type
                                    && !static_assignment_types_compatible(&target, &incoming) =>
                            {
                                ValueType::Any
                            }
                            (Some(ValueType::F64), ValueType::I64) if is_mixed_type => ty,
                            (Some(ValueType::I64), ValueType::F64) if is_mixed_type => ty,
                            // For non-mixed variables, apply widening
                            (Some(ValueType::F64), ValueType::I64) => {
                                main_compiler.emit(Instr::ToF64);
                                ValueType::F64
                            }
                            _ => ty,
                        };

                        // Duplicate the value before storing (for supported types)
                        // For other types, store and then load back
                        let needs_load_back = !matches!(final_ty, ValueType::I64 | ValueType::F64);

                        if !needs_load_back {
                            // For I64 and F64, we have Dup instructions
                            let dup_instr = match final_ty {
                                ValueType::I64 => Instr::DupI64,
                                ValueType::F64 => Instr::DupF64,
                                _ => {
                                    return err(format!(
                                        "internal: unexpected type {:?} in Dup path",
                                        final_ty
                                    ))
                                }
                            };
                            main_compiler.emit(dup_instr);
                            main_compiler.store_local(var, final_ty.clone());
                        } else {
                            // For other types, store first then load back
                            main_compiler.store_local(var, final_ty.clone());
                            main_compiler.load_local(var)?;
                        }
                        if was_pending_const && !main_compiler.strict_undefined_check {
                            main_compiler.const_bindings.insert(var.clone());
                            if let Some(value) = folded_const_value {
                                main_compiler.const_values.insert(var.clone(), value);
                            } else {
                                main_compiler.const_values.remove(var);
                            }
                        } else if !main_compiler.const_bindings.contains(var) {
                            main_compiler.const_values.remove(var);
                        }

                        main_compiler.emit_return_for_type(final_ty);
                    }
                }
                other => {
                    main_compiler.compile_stmt(other)?;
                    main_compiler.emit(Instr::ReturnNothing);
                }
            }
        } else {
            main_compiler.emit(Instr::ReturnNothing);
        }

        // Patch @goto jumps after main code compilation
        main_compiler.patch_goto_jumps()?;

        let mut main_code = main_compiler.code;
        // Use main_entry (where main code actually starts) instead of entry (modules_entry)
        // for jump relocation. This ensures jumps point to correct addresses when modules exist.
        relocate_jumps(&mut main_code, 0, main_entry);
        code.extend(main_code);
        profile::finish(emit_main_timer);
        Ok(())
    }

    fn finalize(
        self,
        inference_engine: &abstract_interp::InferenceEngine,
    ) -> CResult<CoreCompileOutput> {
        let CorePipeline {
            program,
            precompiled_base,
            base_function_count,
            shared_ctx,
            abstract_types,
            primitive_types,
            method_tables,
            mut function_infos,
            show_methods,
            specializable_functions,
            reused_base,
            code,
            entry,
            module_functions,
            imported_functions,
            ..
        } = self;
        // Keep cached Base bytecode out of the mutable suffix while compiling
        // (Issue #6348): the prefix is prepended only here, after the user/main
        // suffix has been optimized and slotized.
        let base_code_prefix = precompiled_base.map(|base_cache| base_cache.code.as_slice());
        let base_code_prefix_len = base_code_prefix.map_or(0, <[_]>::len);

        let (mut code, index_mapping) =
            profile::time("compile.peephole_pre_slotize", || peephole::optimize(code));

        // Update all function boundaries and entry point after optimization.
        // The index_mapping includes one extra entry for the end position.
        let entry =
            apply_peephole_index_mapping(&mut function_infos, entry, &index_mapping, &reused_base);

        let slotize_timer = profile::start("compile.slotize");
        for (idx, func_info) in function_infos.iter_mut().enumerate() {
            if reused_base[idx] {
                continue;
            }
            let code_start = func_info.code_start;
            let code_end = func_info.code_end;
            if code_start >= code_end || code_end > code.len() {
                continue;
            }
            let slot_info = build_slot_info(
                &func_info.params,
                &func_info.kwparams,
                &code[code_start..code_end],
            );
            slotize_code(
                &mut code[code_start..code_end],
                &slot_info.name_to_slot,
                &slot_info.slot_types,
            );
            func_info.slot_names = slot_info.slot_names;
            func_info.slot_types = slot_info.slot_types;
            func_info.local_slot_count = func_info.slot_names.len();
            func_info.param_slots = slot_info.param_slots;
            for (kw, slot) in func_info.kwparams.iter_mut().zip(slot_info.kwparam_slots) {
                kw.slot = slot;
            }
        }

        let global_slot_info = if entry < code.len() {
            let slot_info = build_slot_info(&[], &[], &code[entry..]);
            slotize_code(
                &mut code[entry..],
                &slot_info.name_to_slot,
                &slot_info.slot_types,
            );
            slot_info
        } else {
            build_slot_info(&[], &[], &[])
        };
        let global_slot_names = global_slot_info.slot_names;
        let global_slot_types = global_slot_info.slot_types;
        let global_slot_count = global_slot_names.len();
        profile::finish(slotize_timer);

        // Slotization can expose `LoadSlotI64; AddI64; StoreSlotI64` patterns that
        // the earlier name-based peephole pass cannot see. Run a second pass after
        // slot assignment so Issue #5091 superinstructions are emitted from the
        // final slotized bytecode.
        let (optimized_code, index_mapping) =
            profile::time("compile.peephole_post_slotize", || peephole::optimize(code));
        let code = optimized_code;
        let entry =
            apply_peephole_index_mapping(&mut function_infos, entry, &index_mapping, &reused_base);

        let (code, entry) = profile::time("compile.cached_code_prefix_assemble", || {
            if let Some(base_code_prefix) = base_code_prefix {
                let mut suffix = code;
                relocate_jumps(&mut suffix, 0, base_code_prefix_len);
                for (idx, func_info) in function_infos.iter_mut().enumerate() {
                    if reused_base.get(idx).copied().unwrap_or(false) {
                        continue;
                    }
                    func_info.entry += base_code_prefix_len;
                    func_info.code_start += base_code_prefix_len;
                    func_info.code_end += base_code_prefix_len;
                }

                let mut merged = Vec::with_capacity(base_code_prefix_len + suffix.len());
                merged.extend_from_slice(base_code_prefix);
                merged.extend(suffix);
                (merged, entry + base_code_prefix_len)
            } else {
                (code, entry)
            }
        });

        // Lazy AoT: Build RuntimeCompileContext for specialization
        let final_assembly_timer = profile::start("compile.final_assembly");
        // Issue #6657: detect a user `getindex` override on a native array-like
        // receiver so the runtime specializer skips its native-indexing fast path
        // for scalar `xs[i]` (which would bypass the override). User-origin is
        // `global_index >= base_function_count`; Base array `getindex` methods are
        // excluded so the common no-override program is unaffected.
        let disable_array_getindex_specialization =
            ["getindex", "Base.getindex"].iter().any(|name| {
                method_tables.get(*name).is_some_and(|table| {
                    table.methods.iter().any(|m| {
                        !m.is_base_program_method(base_function_count)
                            && m.param_matches_at(0, method_table::core_type_is_array_like)
                    })
                })
            });
        // Issue #6806: the same detection for a user `setindex!` override on a
        // native array-like receiver (param 0) so the IndexStore write fast path
        // is refused, reaching the override via dispatch.
        let disable_array_setindex_specialization =
            ["setindex!", "Base.setindex!"].iter().any(|name| {
                method_tables.get(*name).is_some_and(|table| {
                    table.methods.iter().any(|m| {
                        !m.is_base_program_method(base_function_count)
                            && m.param_matches_at(0, method_table::core_type_is_array_like)
                    })
                })
            });
        // Issue #8127: detect any user `getproperty` override so the function
        // specializer refuses its direct-`GetField` fast path for `obj.field`
        // reads (which would bypass the override). User-origin is `global_index
        // >= base_function_count`; the Base default `getproperty(x, ::Symbol)` is
        // excluded so the common no-override program keeps the field fast path.
        let disable_field_access_specialization =
            ["getproperty", "Base.getproperty"].iter().any(|name| {
                method_tables.get(*name).is_some_and(|table| {
                    table
                        .methods
                        .iter()
                        .any(|m| !m.is_base_program_method(base_function_count))
                })
            });
        // NB: `disable_field_access_specialization` is intentionally NOT a
        // context-activation trigger — a getproperty override alone must not
        // newly enable specialization. When no other trigger fires, the context
        // stays `None`, no function is specialized, and the interpreter's
        // `getproperty` routing (compile/expr/struct_.rs) already reaches the
        // override. The flag only matters once specialization is otherwise active.
        let compile_context = if !specializable_functions.is_empty()
            || !shared_ctx.parametric_structs.is_empty()
            || !shared_ctx.type_aliases.is_empty()
            || !primitive_types.is_empty()
        {
            Some(RuntimeCompileContext {
                struct_table: shared_ctx.struct_table.clone(),
                struct_defs: shared_ctx.struct_defs.clone(),
                parametric_structs: shared_ctx.parametric_structs.clone(),
                type_aliases: shared_ctx.type_aliases.clone(),
                primitive_types: primitive_types.clone(),
                disable_array_getindex_specialization,
                disable_array_setindex_specialization,
                disable_field_access_specialization,
            })
        } else {
            None
        };
        let mut runtime_specialization_map: Vec<(usize, usize)> = shared_ctx
            .spec_func_mapping
            .iter()
            .map(|(&fallback_index, &spec_index)| (fallback_index, spec_index))
            .collect();
        runtime_specialization_map
            .sort_unstable_by_key(|&(fallback_index, spec_index)| (spec_index, fallback_index));

        // Macro binding table for function-form `isdefined(::Module, Symbol("@m"))`
        // reflection (Issue #7948). Macros are expanded away during lowering, so the
        // VM has no macro registry at runtime; record per-module which macro names
        // are visible so the reflection path can answer correctly.
        let mut macro_bindings: HashMap<String, HashSet<String>> = HashMap::new();
        // Module-qualified surface: `isdefined(AbstractAlgebra, Symbol("@alias"))`.
        // `module_functions` already carries each module's `@name` macro entries.
        for (module_path, names) in &module_functions {
            let macros: HashSet<String> = names
                .iter()
                .filter(|n| n.starts_with('@'))
                .cloned()
                .collect();
            if !macros.is_empty() {
                macro_bindings
                    .entry(module_path.clone())
                    .or_default()
                    .extend(macros);
            }
        }
        // Main-visible surface: `isdefined(Main, Symbol("@alias"))`. Top-level
        // (Main-owned) macros plus macros pulled in by `using` (already collected,
        // export-respecting, in `imported_functions`).
        {
            let main_macros = macro_bindings.entry("Main".to_string()).or_default();
            for m in &program.macros {
                main_macros.insert(format!("@{}", m.name));
            }
            for name in &imported_functions {
                if name.starts_with('@') {
                    main_macros.insert(name.clone());
                }
            }
            if main_macros.is_empty() {
                macro_bindings.remove("Main");
            }
        }

        let compiled = CompiledProgram {
            code,
            functions: function_infos,
            struct_defs: shared_ctx.struct_defs,
            abstract_types,
            primitive_types,
            show_methods,
            entry,
            specializable_functions,
            runtime_specialization_map,
            compile_context,
            base_function_count,
            macro_bindings,
            global_slot_names,
            global_slot_types,
            global_slot_count,
        };

        let inference_results = inference_engine.snapshot_return_cache();
        profile::finish(final_assembly_timer);

        Ok(CoreCompileOutput {
            compiled,
            method_tables,
            closure_captures: shared_ctx.closure_captures,
            inference_results,
        })
    }
}

/// Phase 1: use `base_function_count` from the program if Base was already
/// merged by lib.rs, otherwise merge with the precompiled Base prelude now
/// (for JSON IR input that doesn't use the lib.rs pipeline).
fn merge_precompiled_base(program: &Program) -> (std::borrow::Cow<'_, Program>, usize) {
    profile::time("compile.merge_precompiled_base", || {
        if program.base_function_count > 0 {
            // Already merged by lib.rs - use as-is
            (
                std::borrow::Cow::Borrowed(program),
                program.base_function_count,
            )
        } else {
            // Not merged yet (e.g., JSON IR) - merge now
            let merged = merge_with_precompiled_base(program);
            (
                std::borrow::Cow::Owned(merged.program),
                merged.base_function_count,
            )
        }
    })
}

/// Phase 2: inline small pure user functions into the IR, then run the
/// pure-expression optimization pass over the user segment only.
fn inline_and_optimize_ir(
    program: &Program,
    base_function_count: usize,
) -> (std::borrow::Cow<'_, Program>, ir_opt::UserSegmentOptimized) {
    let inlined_program = profile::time("compile.ir_inline", || {
        ir_inline::inline_small_pure_functions_cow(program, base_function_count)
    });
    let optimized_user_segment = profile::time("compile.ir_opt", || {
        ir_opt::optimize_pure_expressions_user_only(inlined_program.as_ref(), base_function_count)
    });
    (inlined_program, optimized_user_segment)
}

/// Phase 3: load stdlib modules for any using statements
/// that reference stdlib modules not already in program.modules.
fn load_stdlib_modules(
    program: &Program,
    opt_modules: &[crate::ir::core::Module],
) -> Vec<crate::ir::core::Module> {
    let existing_module_names: HashSet<String> =
        profile::time("compile.stdlib_existing_module_names", || {
            opt_modules.iter().map(|m| m.name.clone()).collect()
        });
    profile::time("compile.stdlib_load", || {
        // Collect all using imports from top-level and from within modules
        let mut all_usings: Vec<&UsingImport> = program.usings.iter().collect();

        for module in opt_modules {
            collect_module_usings_recursive(module, &mut all_usings);
        }

        // Use pure Rust stdlib loader for WASM builds
        let usings_to_load: Vec<UsingImport> = all_usings
            .iter()
            .filter(|u| !u.is_relative)
            .filter(|u| !existing_module_names.contains(&u.module))
            .filter(|u| !matches!(u.module.as_str(), "Base" | "Core" | "Main" | "Pkg"))
            .map(|u| (*u).clone())
            .collect();
        crate::stdlib_loader::load_stdlib_modules(&usings_to_load)
    })
}

fn resolve_using_module_name(
    using_import: &UsingImport,
    current_module_path: &str,
    module_functions: &HashMap<String, HashSet<String>>,
) -> Option<String> {
    if !using_import.is_relative {
        if let Some(base_submodule) = using_import.module.strip_prefix("Base.") {
            if module_functions.contains_key(base_submodule)
                && !super::constants::is_stdlib_module(base_submodule)
            {
                return Some(base_submodule.to_string());
            }
        }
        return Some(using_import.module.clone());
    }

    let relative_level = using_import.relative_level.max(1);
    let mut base_parts: Vec<&str> = if current_module_path.is_empty() {
        Vec::new()
    } else {
        current_module_path.split('.').collect()
    };

    let parent_hops = relative_level.saturating_sub(1).min(base_parts.len());
    for _ in 0..parent_hops {
        base_parts.pop();
    }

    let candidate = if base_parts.is_empty() {
        using_import.module.clone()
    } else {
        format!("{}.{}", base_parts.join("."), using_import.module)
    };

    if module_functions.contains_key(candidate.as_str()) {
        return Some(candidate);
    }

    // Julia permits parent modules to refer to themselves by name, e.g.
    // `import ..LinearAlgebra: inv` inside `LinearAlgebra.LAPACK`.
    if module_functions.contains_key(using_import.module.as_str()) {
        return Some(using_import.module.clone());
    }

    None
}

fn validate_scope_using_imports(
    usings: &[UsingImport],
    module_functions: &HashMap<String, HashSet<String>>,
) -> CResult<()> {
    for using_import in usings {
        validate_using_import(using_import, module_functions)?;
    }
    Ok(())
}

fn validate_module_using_imports(
    module: &crate::ir::core::Module,
    module_functions: &HashMap<String, HashSet<String>>,
) -> CResult<()> {
    validate_scope_using_imports(&module.usings, module_functions)?;
    for submodule in &module.submodules {
        validate_module_using_imports(submodule, module_functions)?;
    }
    Ok(())
}

fn validate_using_import(
    using_import: &UsingImport,
    module_functions: &HashMap<String, HashSet<String>>,
) -> CResult<()> {
    if using_import.is_relative {
        return Ok(());
    }

    let Some(base_submodule) = using_import.module.strip_prefix("Base.") else {
        return Ok(());
    };

    if module_functions.contains_key(base_submodule)
        && !super::constants::is_stdlib_module(base_submodule)
    {
        return Ok(());
    }

    if module_functions.contains_key(using_import.module.as_str()) {
        return Ok(());
    }

    err(format!(
        "UndefVarError: `{base_submodule}` not defined in `Base`"
    ))
}

fn resolve_scope_using_imports(
    usings: &[UsingImport],
    current_module_path: &str,
    module_functions: &HashMap<String, HashSet<String>>,
) -> Vec<ResolvedUsingImport> {
    usings
        .iter()
        .filter_map(|using_import| {
            let module =
                resolve_using_module_name(using_import, current_module_path, module_functions)?;
            let symbols = using_import
                .symbols
                .as_ref()
                .map(|names| names.iter().cloned().collect());
            Some((module, symbols))
        })
        .collect()
}

/// Phase 4: collect inline (nested) functions from top-level statements and
/// function bodies, with parent function tracking.
fn collect_top_level_inline_functions(
    program: &Program,
    base_function_count: usize,
    opt_user_functions: &[Function],
    opt_main: &Block,
    all_modules: &[&crate::ir::core::Module],
) -> Vec<(Function, Option<String>)> {
    profile::time("compile.collect_inline_functions", || {
        let mut inline_functions = Vec::new();
        for stmt in &opt_main.stmts {
            collect_stmt_functions(stmt, &mut inline_functions, None);
        }
        // Also collect from each top-level function's body. Keep scanning
        // Base bodies on the cached path because some cached Base entries
        // still rely on nested-function alignment and closure metadata.
        for func in program
            .functions
            .iter()
            .take(base_function_count)
            .chain(opt_user_functions.iter())
        {
            collect_block_functions(&func.body, &mut inline_functions, Some(&func.name));
        }
        // Also collect from module functions
        for module in all_modules {
            collect_from_module(module, &mut inline_functions);
        }
        inline_functions
    })
}
