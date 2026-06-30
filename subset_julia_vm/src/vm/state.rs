//! VM lifecycle, value/state accessors, and runtime-support helpers.
//!
//! Split out of `vm/mod.rs` (Issue #6826). These `impl Vm<R>` methods cover the
//! `Vm` constructors (`new`, `new_program`), local/global/output accessors,
//! value type queries, error handling/`raise`, the call-site inline/dispatch
//! caches, and the small stack/compare execution helpers. The `Vm` struct
//! definition itself stays in `vm/mod.rs`.

use super::*;

impl<R: RngLike> Vm<R> {
    pub(crate) fn debug_current_instruction(&self) -> Option<(usize, Instr)> {
        self.code
            .get(self.ip)
            .cloned()
            .map(|instr| (self.ip, instr))
    }

    pub(crate) fn debug_instruction_at(&self, ip: usize) -> Option<Instr> {
        self.code.get(ip).cloned()
    }

    /// Create a new VM with a flat instruction list and an RNG instance.
    ///
    /// Use this constructor when you have a raw `Vec<Instr>` (e.g., from incremental
    /// compilation). For compiled programs with entry points and metadata, prefer
    /// [`Vm::new_program`].
    pub fn new(code: Vec<Instr>, rng: R) -> Self {
        let call_site_caches = vec![CallSiteCache::default(); code.len()];
        Self {
            ip: 0,
            stack: Vec::with_capacity(256),
            frames: vec![Frame::new()],
            frame_pool: Vec::new(),
            return_ips: Vec::new(),
            handlers: Vec::new(),
            code: Rc::new(code),
            executable: executable::ExecutableProgram::empty(),
            next_executable_ip: executable::NO_EXECUTABLE_IP,
            functions: Vec::new(),
            base_function_count: 0,
            native_array_exempt_functions: Vec::new(),
            function_slot_maps: Vec::new(),
            binary_signature_cache: HashMap::new(),
            typed_signature_cache: HashMap::new(),
            struct_defs: Vec::new(),
            abstract_types: Vec::new(),
            show_methods: std::collections::HashMap::new(),
            struct_heap: Vec::new(),
            rng,
            output: String::new(),
            stderr_output: String::new(),
            output_callback: None,
            output_callback_context: std::ptr::null_mut(),
            broadcast_states: Vec::new(),
            composed_call_state: None,
            generator_iterate_state: Vec::new(),
            sprint_state: None,
            pending_error: None,
            pending_exception_value: None,
            caught_exceptions: Vec::new(),
            rethrow_on_finally: false,
            test_pass_count: 0,
            test_fail_count: 0,
            test_broken_count: 0,
            current_testset: None,
            any_test_failed: false,
            test_throws_state: None,
            // Lazy AoT fields
            specializable_functions: Vec::new(),
            specialization_cache: HashMap::new(),
            specialization_i64_cache: HashMap::new(),
            i64_function_cache: HashMap::new(),
            binary_method_cache: HashMap::new(),
            compile_context: None,
            macro_bindings: HashMap::new(),
            global_slot_names: Vec::new(),
            global_slot_map: HashMap::new(),
            gensym_counter: 0,
            runtime_typevar_counter: 0,
            runtime_typevar_identities: HashMap::new(),
            cached_cartesian_index_type_id: Cell::new(None),
            cached_pair_type_id: Cell::new(None),
            cached_complex_type_id: Cell::new(None),
            cached_array_type_id: Cell::new(None),
            struct_def_name_index: HashMap::new(),
            abstract_type_name_index: HashMap::new(),
            dispatch_cache: HashMap::new(),
            binary_both_dispatch_cache: HashMap::new(),
            call_site_caches,
            method_dispatch_cache: HashMap::new(),
            generated_expr_cache: HashMap::new(),
            generated_expr_pending_keys: HashMap::new(),
            generated_expr_pending_eval_frames: HashMap::new(),
            function_name_index: HashMap::new(),
            current_world: 1,
            source_map: Vec::new(),
            last_error_ip: None,
            type_ancestors: HashMap::new(),
            struct_hierarchy: StructHierarchy::new(),
            eval_dispatch_depth: 0,
            eval_dispatch_floor: None,
            call_depth_overflow_pending: false,
        }
    }

    /// Create a new VM from a fully compiled program.
    ///
    /// `CompiledProgram` carries the entry point IP, all function/struct definitions,
    /// global slot layout, and optional lazy-AoT context produced by the compiler.
    /// This is the primary constructor used after calling [`compile_and_run_str`] or
    /// the two-phase compile pipeline.
    pub fn new_program(mut program: CompiledProgram, rng: R) -> Self {
        // Build show_methods HashMap from the CompiledProgram's Vec
        let show_methods = program
            .show_methods
            .iter()
            .map(|entry| (entry.type_name.clone(), entry.func_index))
            .collect();

        let global_slot_map = program
            .global_slot_names
            .iter()
            .enumerate()
            .map(|(idx, name)| (name.clone(), idx))
            .collect::<HashMap<_, _>>();

        // Pre-compute the native-array boundary exemptions once at install so
        // runtime dispatch consults a flag instead of re-matching function name
        // strings per candidate (Issue #6336).
        let native_array_exempt_functions = program
            .functions
            .iter()
            .map(|func| base_function_accepts_native_array_value(&func.name))
            .collect::<Vec<_>>();

        // Pre-compute per-function `name -> slot` maps so the string-keyed
        // `Load*/Store*` paths resolve `name -> slot` in O(1) instead of a
        // linear scan over `slot_names` on every execution (Issue #5179).
        let function_slot_maps = program
            .functions
            .iter()
            .map(|func| {
                func.slot_names
                    .iter()
                    .enumerate()
                    .map(|(idx, name)| (name.clone(), idx))
                    .collect::<HashMap<String, usize>>()
            })
            .collect::<Vec<_>>();

        let struct_def_name_index = program
            .struct_defs
            .iter()
            .enumerate()
            .map(|(idx, def)| (def.name.clone(), idx))
            .collect::<HashMap<_, _>>();

        let abstract_type_name_index = program
            .abstract_types
            .iter()
            .enumerate()
            .map(|(idx, at)| (at.name.clone(), idx))
            .collect::<HashMap<_, _>>();

        let struct_hierarchy = build_struct_hierarchy_from_program(&program);
        let base_function_count = program.base_function_count;
        let entry_ip = program.entry;
        let executable =
            executable::ExecutableProgram::from_bytecode(&program.code, &program.functions);
        let next_executable_ip = executable.next_ip_from(entry_ip);
        let call_site_caches = vec![CallSiteCache::default(); program.code.len()];

        // Parametric user structs are not in `struct_defs` (they instantiate
        // lazily), so surface their base names while the declared parents stay
        // centralized in `struct_hierarchy` (Issue #5052, #5920).
        let parametric_struct_names: Vec<String> = program
            .compile_context
            .as_ref()
            .map(|ctx| ctx.parametric_structs.keys().cloned().collect())
            .unwrap_or_default();

        // Pre-compute transitive closure of abstract type hierarchy (Issue #3356)
        let type_ancestors = compute_type_ancestors(
            &program.struct_defs,
            &program.abstract_types,
            &abstract_type_name_index,
            &struct_hierarchy,
            &parametric_struct_names,
        );

        // Build function name → indices lookup for O(1) dispatch (Issue #3361)
        let mut function_name_index: HashMap<String, Vec<usize>> = HashMap::new();
        for (idx, func) in program.functions.iter().enumerate() {
            function_name_index
                .entry(func.name.clone())
                .or_default()
                .push(idx);
            if let Some((_, short_name)) = func.name.rsplit_once('.') {
                function_name_index
                    .entry(short_name.to_string())
                    .or_default()
                    .push(idx);
            }
        }

        // The reflection `Method` struct exposes a `.module::Module` field, but
        // `module` is a reserved keyword the parser cannot accept as a field
        // name, so the pure-Julia definition declares it as `mod`. Rename it to
        // `module` here so `m.module` field access (compiled to a
        // `GetFieldByName("module")`) resolves and `fieldnames(Method)` reports
        // `:module`, matching upstream (Issue #5125).
        normalize_method_struct_def(&mut program.struct_defs);

        Self {
            ip: entry_ip,
            stack: Vec::with_capacity(256),
            frames: vec![Frame::new_with_slots(program.global_slot_count, None)],
            frame_pool: Vec::new(),
            return_ips: Vec::new(),
            handlers: Vec::new(),
            code: Rc::new(program.code),
            executable,
            next_executable_ip,
            functions: program.functions.into_iter().map(Rc::new).collect(),
            base_function_count,
            native_array_exempt_functions,
            function_slot_maps,
            binary_signature_cache: HashMap::new(),
            typed_signature_cache: HashMap::new(),
            struct_defs: program.struct_defs,
            abstract_types: program.abstract_types,
            show_methods,
            struct_heap: Vec::new(),
            rng,
            output: String::new(),
            stderr_output: String::new(),
            output_callback: None,
            output_callback_context: std::ptr::null_mut(),
            broadcast_states: Vec::new(),
            composed_call_state: None,
            generator_iterate_state: Vec::new(),
            sprint_state: None,
            pending_error: None,
            pending_exception_value: None,
            caught_exceptions: Vec::new(),
            rethrow_on_finally: false,
            test_pass_count: 0,
            test_fail_count: 0,
            test_broken_count: 0,
            current_testset: None,
            any_test_failed: false,
            test_throws_state: None,
            // Lazy AoT fields
            specializable_functions: program.specializable_functions,
            specialization_cache: HashMap::new(),
            specialization_i64_cache: HashMap::new(),
            i64_function_cache: HashMap::new(),
            binary_method_cache: HashMap::new(),
            compile_context: program.compile_context,
            macro_bindings: program.macro_bindings,
            global_slot_names: program.global_slot_names,
            global_slot_map,
            gensym_counter: 0,
            runtime_typevar_counter: 0,
            runtime_typevar_identities: HashMap::new(),
            cached_cartesian_index_type_id: Cell::new(None),
            cached_pair_type_id: Cell::new(None),
            cached_complex_type_id: Cell::new(None),
            cached_array_type_id: Cell::new(None),
            struct_def_name_index,
            abstract_type_name_index,
            dispatch_cache: HashMap::new(),
            binary_both_dispatch_cache: HashMap::new(),
            call_site_caches,
            method_dispatch_cache: HashMap::new(),
            generated_expr_cache: HashMap::new(),
            generated_expr_pending_keys: HashMap::new(),
            generated_expr_pending_eval_frames: HashMap::new(),
            function_name_index,
            current_world: 1,
            source_map: Vec::new(),
            last_error_ip: None,
            type_ancestors,
            struct_hierarchy,
            eval_dispatch_depth: 0,
            eval_dispatch_floor: None,
            call_depth_overflow_pending: false,
        }
    }

    /// Inject an `Int64` variable into the current frame before execution.
    ///
    /// If `name` maps to a slot in the global slot layout, the slot is updated
    /// directly; otherwise the value is stored in `locals_any`.
    /// This is used by the REPL and FFI layer to pass integer inputs into Julia code
    /// without going through compilation.
    pub fn set_local_i64(&mut self, name: &str, v: i64) {
        if let Some(&slot) = self.global_slot_map.get(name) {
            if let Some(frame) = self.frames.last_mut() {
                if frame.set_slot_i64(slot, v) {
                    return;
                }
            }
        }
        if let Some(frame) = self.frames.last_mut() {
            frame.locals_any.insert(name.to_string(), Value::I64(v));
            frame
                .var_types
                .insert(name.to_string(), frame::VarTypeTag::I64);
        }
    }

    /// Inject a `Float64` variable into the current frame before execution.
    ///
    /// Mirrors [`Vm::set_local_i64`] but for floating-point values. The slot-based
    /// fast path is tried first; `locals_any` is used as fallback.
    pub fn set_local_f64(&mut self, name: &str, v: f64) {
        if let Some(&slot) = self.global_slot_map.get(name) {
            if let Some(frame) = self.frames.last_mut() {
                if frame.set_slot_f64(slot, v) {
                    return;
                }
            }
        }
        if let Some(frame) = self.frames.last_mut() {
            frame.locals_any.insert(name.to_string(), Value::F64(v));
            frame
                .var_types
                .insert(name.to_string(), frame::VarTypeTag::F64);
        }
    }

    /// Get the accumulated output from println calls
    pub fn get_output(&self) -> &str {
        &self.output
    }

    /// Get the accumulated stderr output from `print(stderr, ...)` calls (Issue #3573).
    pub fn get_stderr_output(&self) -> &str {
        &self.stderr_output
    }

    /// Set the source map that maps instruction IPs to source spans (Issue #2856).
    ///
    /// The source map is a parallel vector to `code` — `source_map[ip]` gives the
    /// source span for instruction at `ip`. Populated by the compiler; empty by default.
    pub fn set_source_map(&mut self, source_map: Vec<Option<crate::span::Span>>) {
        self.source_map = source_map;
    }

    /// Get the source span for the instruction that caused the last error (Issue #2856).
    ///
    /// Returns `None` if no error has occurred, or if the source map is not populated.
    pub fn last_error_span(&self) -> Option<crate::span::Span> {
        self.last_error_ip
            .and_then(|ip| self.source_map.get(ip).copied().flatten())
    }

    /// Create a [`SpannedVmError`] from a `VmError`, attaching the source span
    /// of the last error instruction if available (Issue #2856).
    pub fn spanned_error(&self, error: VmError) -> SpannedVmError {
        SpannedVmError {
            error,
            span: self.last_error_span(),
        }
    }

    /// Get the type_id for Complex struct from struct_defs (cached).
    /// Returns the first type_id for a struct named "Complex" or "Complex{...}"
    pub(super) fn get_complex_type_id(&self) -> usize {
        if let Some(id) = self.cached_complex_type_id.get() {
            return id;
        }
        let id = self
            .struct_defs
            .iter()
            .enumerate()
            .find_map(|(idx, def)| {
                if is_complex_type_name(&def.name) {
                    Some(idx)
                } else {
                    None
                }
            })
            .unwrap_or(0);
        self.cached_complex_type_id.set(Some(id));
        id
    }

    /// Create a Complex struct instance with the correct struct_name from struct_defs.
    /// This ensures the struct_name matches what's registered in struct_defs for proper dispatch.
    pub(super) fn create_complex(&mut self, type_id: usize, re: f64, im: f64) -> Value {
        let struct_name = self
            .struct_defs
            .get(type_id)
            .map(|def| def.name.clone())
            .unwrap_or_else(|| "Complex{Float64}".to_string());
        let s =
            StructInstance::with_name(type_id, struct_name, vec![Value::F64(re), Value::F64(im)]);
        let idx = self.struct_heap.len();
        self.struct_heap.push(s);
        Value::StructRef(idx)
    }

    /// Get the ValueType from a Value (for Lazy AoT specialization)
    pub(super) fn get_value_type(&self, val: &Value) -> ValueType {
        // Route the legacy native-array carrier through the shared
        // `native_array_value_ref` helper so the match below no longer holds
        // a native-array arm (Issue #3908). The catch-all
        // `_ => ValueType::Any` would otherwise map the native carrier to
        // `Any`, so the early return is required for behavior preservation.
        if let Some(arr) = native_array_value_ref(val) {
            return ValueType::ArrayOf(arr.borrow().element_type(), None);
        }
        match val {
            Value::I64(_) => ValueType::I64,
            Value::F64(_) => ValueType::F64,
            Value::Str(_) => ValueType::Str,
            Value::Char(_) => ValueType::Char,
            Value::Bool(_) => ValueType::Bool,
            Value::Nothing => ValueType::Nothing,
            Value::Missing => ValueType::Missing,
            Value::Memory(mem) => ValueType::ArrayOf(mem.borrow().element_type().clone(), None),
            Value::MemoryRef(_) => ValueType::Any,
            Value::StructRef(idx) => {
                if let Some(s) = self.struct_heap.get(*idx) {
                    value_type_for_struct_instance(s)
                } else {
                    ValueType::Any
                }
            }
            Value::Struct(s) => value_type_for_struct_instance(s),
            Value::Tuple(_) => ValueType::Tuple,
            Value::NamedTuple(_) => ValueType::Tuple,
            Value::Range(_) => ValueType::Range,
            Value::DataType(_) | Value::RuntimeTypeVar(_) => ValueType::DataType,
            Value::Rng(_) => ValueType::Rng,
            Value::Generator(_) => ValueType::Generator,
            _ => ValueType::Any,
        }
    }

    /// Get the JuliaType for a Value (for type parameter binding)
    pub(super) fn get_value_julia_type(&self, val: &Value) -> crate::types::JuliaType {
        // Route the legacy native-array carrier through the shared
        // `native_array_value_ref` helper so the match below no longer holds
        // a native-array arm (Issue #3908). The catch-all
        // `_ => crate::types::JuliaType::Any` would otherwise map the native
        // carrier to `Any`, so the early return is required for behavior
        // preservation.
        if let Some(arr) = native_array_value_ref(val) {
            let arr_ref = arr.borrow();
            if let Some(container_type) = arr_ref.array_type_override() {
                return crate::types::JuliaType::Struct(container_type.to_string());
            }
            let elem_jtype = self.array_value_logical_element_julia_type(&arr_ref);
            return julia_array_type_for_ndims(elem_jtype, arr_ref.shape.len());
        }
        match val {
            Value::I8(_) => crate::types::JuliaType::Int8,
            Value::I16(_) => crate::types::JuliaType::Int16,
            Value::I32(_) => crate::types::JuliaType::Int32,
            Value::I64(_) => crate::types::JuliaType::Int64,
            Value::I128(_) => crate::types::JuliaType::Int128,
            Value::BigInt(_) => crate::types::JuliaType::BigInt,
            Value::U8(_) => crate::types::JuliaType::UInt8,
            Value::U16(_) => crate::types::JuliaType::UInt16,
            Value::U32(_) => crate::types::JuliaType::UInt32,
            Value::U64(_) => crate::types::JuliaType::UInt64,
            Value::U128(_) => crate::types::JuliaType::UInt128,
            Value::F16(_) => crate::types::JuliaType::Float16,
            Value::F32(_) => crate::types::JuliaType::Float32,
            Value::F64(_) => crate::types::JuliaType::Float64,
            Value::BigFloat(_) => crate::types::JuliaType::BigFloat,
            Value::Str(_) => crate::types::JuliaType::String,
            Value::Char(_) => crate::types::JuliaType::Char,
            Value::Bool(_) => crate::types::JuliaType::Bool,
            Value::Nothing => crate::types::JuliaType::Nothing,
            Value::Missing => crate::types::JuliaType::Missing,
            Value::Regex(_) => crate::types::JuliaType::Struct("Regex".to_string()),
            Value::RegexMatch(_) => crate::types::JuliaType::Struct("RegexMatch".to_string()),
            // Report the concrete RNG struct type so method dispatch on
            // `::Xoshiro`, `::StableRNG`, `::AbstractRNG` selects the right
            // method when a Value::Rng is passed as an argument (Issue #7231).
            // The global handle (default_rng()/GLOBAL_RNG) reports as
            // TaskLocalRNG (Issue #7230).
            Value::Rng(rng) => crate::types::JuliaType::Struct(
                match rng {
                    crate::rng::RngInstance::Stable(_) => "StableRNG",
                    crate::rng::RngInstance::Xoshiro(_) => "Xoshiro",
                    crate::rng::RngInstance::Mersenne(_) => "MersenneTwister",
                    crate::rng::RngInstance::Global => "TaskLocalRNG",
                }
                .to_string(),
            ),
            Value::StructRef(idx) => {
                if let Some(s) = self.struct_heap.get(*idx) {
                    // Issue #8025: resolve a user-struct array element type
                    // (`StructOf(type_id)`) to its concrete struct name through
                    // `struct_defs`, mirroring `typeof`/reflection (Issue #7304).
                    // The registry-free `array_wrapper_julia_type()` reports `Any`
                    // for such an eltype, so dispatch saw a `Matrix{MyNum}` as
                    // `Matrix{Any}` and a parametric `AbstractMatrix{<:MyNum}`
                    // method failed to match against its bare `AbstractMatrix`
                    // sibling.
                    self.array_wrapper_julia_type_resolved(s)
                        .unwrap_or_else(|| self.get_parametric_struct_name(s))
                } else {
                    crate::types::JuliaType::Any
                }
            }
            Value::Struct(s) => self
                .array_wrapper_julia_type_resolved(s)
                .unwrap_or_else(|| self.get_parametric_struct_name(s)),
            Value::DataType(jt) => *jt.clone(),
            Value::RuntimeTypeVar(tv) => tv.projection(),
            Value::Memory(mem) => {
                let mem = mem.borrow();
                let elem_type_name = self.memory_element_type_name(mem.element_type());
                crate::types::JuliaType::Struct(format!("Memory{{{}}}", elem_type_name))
            }
            Value::MemoryRef(memref) => crate::types::JuliaType::Struct(memref.julia_type_name()),
            Value::Tuple(items) => crate::types::JuliaType::TupleOf(
                items
                    .elements
                    .iter()
                    .map(|item| self.get_value_julia_type(item))
                    .collect(),
            ),
            Value::NamedTuple(_) => crate::types::JuliaType::NamedTuple,
            Value::SimpleVector(_) => {
                crate::types::JuliaType::Struct("Core.SimpleVector".to_string())
            }
            // Keep runtime dispatch in sync with `typeof` / reflection for index
            // wrappers. Official Julia dispatches range and colon indexing through
            // `getindex(A::Array, I::AbstractUnitRange)` / `getindex(A::Array, ::Colon)`.
            Value::Range(_) => val.runtime_type(),
            Value::SliceAll => crate::types::JuliaType::Struct("Colon".to_string()),
            // Base.RefValue{T}: report the concrete struct type so method dispatch
            // on `::Ref`, `::RefValue`, and `::Ref{T}` selects correctly (Issue #5130).
            Value::Ref(inner) => {
                let inner_ty = self.get_value_julia_type(&inner.borrow());
                crate::types::JuliaType::Struct(format!("Base.RefValue{{{}}}", inner_ty))
            }
            Value::IO(_) => crate::types::JuliaType::IOBuffer,
            Value::Closure(_) => crate::types::JuliaType::Function,
            Value::ComposedFunction(_) => crate::types::JuliaType::Function,
            Value::Generator(_) => crate::types::JuliaType::Generator,
            Value::Module(_) => crate::types::JuliaType::Module,
            Value::Symbol(_) => crate::types::JuliaType::Symbol,
            Value::Expr(_) => crate::types::JuliaType::Expr,
            Value::QuoteNode(_) => crate::types::JuliaType::QuoteNode,
            Value::LineNumberNode(_) => crate::types::JuliaType::LineNumberNode,
            Value::GlobalRef(_) => crate::types::JuliaType::GlobalRef,
            Value::Pairs(_) => crate::types::JuliaType::Pairs,
            Value::Enum { type_name, .. } => crate::types::JuliaType::Enum(type_name.clone()),
            // Each generic function has its own singleton type `typeof(f)`, a
            // subtype of `Function` (Issue #5128). Report it here so a
            // `where {F}` / `where {F<:Function}` parameter matched against a
            // function value binds `F` to `typeof(f)` instead of falling
            // through to the `Any` catch-all. This mirrors the `typeof(f)`
            // projection in `BuiltinId::TypeOf` (builtins_types.rs).
            Value::Function(f) => crate::types::JuliaType::Struct(format!("typeof({})", f.name)),
            // StaticArray variants: report the concrete parametric type name so
            // where-clause binding (e.g. `size(x::SMatrix{M,N,T}) where {M,N,T}`)
            // can extract M, N, T from the type string (Issue #7964).
            Value::StaticArray(sv) => {
                crate::types::JuliaType::Struct(sv.julia_type_name().to_string())
            }
            Value::StaticArrayInline(sv) => {
                crate::types::JuliaType::Struct(sv.julia_type_name_owned().to_string())
            }
            _ => crate::types::JuliaType::Any,
        }
    }

    pub(in crate::vm) fn array_value_logical_element_julia_type(
        &self,
        arr: &ArrayValue,
    ) -> crate::types::JuliaType {
        match arr.element_type() {
            ArrayElementType::StructOf(type_id) | ArrayElementType::StructInlineOf(type_id, _) => {
                self.struct_defs
                    .get(type_id)
                    .map(|def| crate::types::JuliaType::Struct(def.name.clone()))
                    .unwrap_or(crate::types::JuliaType::Any)
            }
            ArrayElementType::Struct => {
                if let ArrayData::StructRefs(refs) = &arr.data {
                    refs.first()
                        .and_then(|idx| self.struct_heap.get(*idx))
                        .map(|s| crate::types::JuliaType::Struct(s.struct_name.to_string()))
                        .unwrap_or(crate::types::JuliaType::Any)
                } else {
                    crate::types::JuliaType::Any
                }
            }
            ArrayElementType::Any => {
                if let ArrayData::Any(values) = &arr.data {
                    values
                        .first()
                        .and_then(|first| match first {
                            Value::StructRef(idx) => self.struct_heap.get(*idx).map(|s| {
                                crate::types::JuliaType::Struct(s.struct_name.to_string())
                            }),
                            Value::Struct(s) => {
                                Some(crate::types::JuliaType::Struct(s.struct_name.to_string()))
                            }
                            _ => None,
                        })
                        .unwrap_or(crate::types::JuliaType::Any)
                } else {
                    crate::types::JuliaType::Any
                }
            }
            element_type => array_element_type_to_julia_type(&element_type),
        }
    }

    pub(in crate::vm) fn array_value_declared_element_julia_type(
        &self,
        arr: &ArrayValue,
    ) -> crate::types::JuliaType {
        match arr.element_type() {
            ArrayElementType::StructOf(type_id) | ArrayElementType::StructInlineOf(type_id, _) => {
                self.struct_defs
                    .get(type_id)
                    .map(|def| crate::types::JuliaType::Struct(def.name.clone()))
                    .unwrap_or(crate::types::JuliaType::Any)
            }
            element_type => array_element_type_to_julia_type(&element_type),
        }
    }

    /// Get the full parametric struct name for a struct instance.
    /// Preserves actual type parameters (e.g., "Complex{Bool}", "Complex{Int64}").
    pub(super) fn get_parametric_struct_name(&self, s: &StructInstance) -> crate::types::JuliaType {
        // Preserve the actual struct name including type parameters
        crate::types::JuliaType::Struct(s.struct_name.to_string())
    }

    /// Try to load a variable from a specific frame index.
    /// Returns true if the variable was found and pushed onto the stack.
    pub(super) fn try_load_from_frame(&mut self, name: &str, frame_idx: usize) -> bool {
        if let Some(frame) = self.frames.get(frame_idx) {
            // 1. Check slot-based locals first
            if let Some(val) = self.load_slot_value_by_name(frame, name) {
                self.stack.push(val);
                return true;
            }
            // 2. O(1) tag-dispatched lookup across all typed maps
            if let Some(val) = frame.get_local(name) {
                self.stack.push(val);
                return true;
            }
            // 3. Check type_bindings for type parameters from where clause
            if let Some(julia_type) = frame.type_bindings.get(name) {
                self.stack
                    .push(Value::DataType(Box::new(julia_type.clone())));
                return true;
            }
        }
        false
    }

    /// Get a variable value from a specific frame without pushing to stack.
    /// Returns None if the variable is not found.
    pub(super) fn get_value_from_frame(&self, name: &str, frame_idx: usize) -> Option<Value> {
        let frame = self.frames.get(frame_idx)?;
        // 1. Check slot-based locals first
        if let Some(val) = self.load_slot_value_by_name(frame, name) {
            return Some(val);
        }
        // 2. Issue #1744: Check captured variables for deeply nested closures
        if let Some(v) = frame.captured_vars.get(name) {
            return Some(v.clone());
        }
        // 3. O(1) tag-dispatched lookup across all typed maps
        frame.get_local(name)
    }

    /// Check if a variable is defined in a specific frame.
    /// Returns true if the variable exists in that frame.
    pub(super) fn is_var_defined_in_frame(&self, name: &str, frame_idx: usize) -> bool {
        self.get_value_from_frame(name, frame_idx).is_some()
    }

    /// Get a variable value by name, checking current frame first, then global.
    /// Used by eval to resolve symbols at runtime.
    pub fn get_variable_value(&self, name: &str) -> Option<Value> {
        // First check current frame
        let current_frame_idx = self.frames.len().saturating_sub(1);
        if let Some(val) = self.get_value_from_frame(name, current_frame_idx) {
            return Some(val);
        }
        // Try global frame if not in current frame
        if self.frames.len() > 1 {
            if let Some(val) = self.get_value_from_frame(name, 0) {
                return Some(val);
            }
        }
        None
    }

    /// Set a variable value by name in the current frame.
    /// Used by eval to support assignment expressions.
    pub fn set_variable_value(&mut self, name: &str, val: Value) {
        if let Some(frame) = self.frames.last_mut() {
            util::bind_value_to_frame(frame, name, ValueType::Any, val, &mut self.struct_heap);
        }
    }

    pub(super) fn slot_index_for_frame(&self, frame: &Frame, name: &str) -> Option<usize> {
        if let Some(func_index) = frame.func_index {
            // Fast path: O(1) probe of the pre-computed `name -> slot` map
            // (Issue #5179). Falls back to scanning `slot_names` only when the
            // map is absent — e.g. functions appended after construction by
            // unit-test harnesses that do not refresh `function_slot_maps`.
            if let Some(slot_map) = self.function_slot_maps.get(func_index) {
                return slot_map.get(name).copied();
            }
            self.functions.get(func_index).and_then(|func| {
                func.slot_names
                    .iter()
                    .position(|slot_name| slot_name == name)
            })
        } else {
            self.global_slot_map.get(name).copied()
        }
    }

    pub(super) fn load_slot_value_by_name(&self, frame: &Frame, name: &str) -> Option<Value> {
        let slot = self.slot_index_for_frame(frame, name)?;
        frame.locals_slots.get(slot).and_then(|v| v.clone())
    }

    pub(super) fn slot_name_for_frame(&self, frame: &Frame, slot: usize) -> String {
        if let Some(func_index) = frame.func_index {
            if let Some(name) = self
                .functions
                .get(func_index)
                .and_then(|func| func.slot_names.get(slot))
            {
                return name.clone();
            }
        } else if let Some(name) = self.global_slot_names.get(slot) {
            return name.clone();
        }
        format!("slot {}", slot)
    }

    /// Set the output callback for streaming output.
    /// The callback will be called for each output line with the context pointer.
    pub fn set_output_callback(&mut self, callback: OutputCallback, context: *mut c_void) {
        self.output_callback = Some(callback);
        self.output_callback_context = context;
    }

    /// Emit output to the buffer and optionally to the callback.
    /// This is the central method for all output operations.
    ///
    /// When inside a sprint call, output is redirected to the sprint's IOBuffer
    /// instead of stdout/the main output buffer.
    pub(super) fn emit_output(&mut self, s: &str, newline: bool) {
        // Check if we're inside a sprint call - if so, redirect output to the sprint buffer
        if let Some(ref state) = self.sprint_state {
            let mut io = state.io.borrow_mut();
            io.buffer.push_str(s);
            if newline {
                io.buffer.push('\n');
            }
            return;
        }

        // Normal output path - buffer for get_output() (REPL and non-streaming use cases)
        self.output.push_str(s);
        if newline {
            self.output.push('\n');
        }

        // Call the streaming callback if set
        if let Some(callback) = self.output_callback {
            let line = if newline {
                format!("{}\n", s)
            } else {
                s.to_string()
            };
            if let Ok(cstr) = CString::new(line) {
                callback(self.output_callback_context, cstr.as_ptr());
            }
        }
    }

    /// Emit captured stderr output (Issue #3573).
    ///
    /// Mirrors `emit_output` but writes to a separate buffer that the runner
    /// (or FFI consumer) is expected to forward to the user's actual stderr
    /// on exit. Inside a `sprint` call we route to the sprint buffer too so
    /// that `sprint(io -> print(stderr, x))` is well-defined.
    pub(super) fn emit_stderr(&mut self, s: &str, newline: bool) {
        if let Some(ref state) = self.sprint_state {
            let mut io = state.io.borrow_mut();
            io.buffer.push_str(s);
            if newline {
                io.buffer.push('\n');
            }
            return;
        }
        self.stderr_output.push_str(s);
        if newline {
            self.stderr_output.push('\n');
        }
    }

    /// Get a global variable by name from the top-level frame.
    /// Used by REPL session to extract variables after execution.
    pub fn get_global(&self, name: &str) -> Option<Value> {
        // Look in the first (global) frame
        let frame = self.frames.first()?;

        if let Some(&slot) = self.global_slot_map.get(name) {
            if let Some(Some(val)) = frame.locals_slots.get(slot) {
                return Some(val.clone());
            }
        }

        // Check fallback locals for TypedArray, StructRef, and other dynamic types.
        if let Some(v) = frame.locals_any.get(name) {
            return Some(v.clone());
        }

        None
    }

    /// Get a reference to the struct heap (for REPL display)
    pub fn get_struct_heap(&self) -> &[StructInstance] {
        &self.struct_heap
    }

    /// Pop a numeric value as f64 from stack, handling Rational and BigInt.
    /// Uses StackOpsExt with struct_heap context.
    #[inline]
    pub fn pop_f64_or_i64(&mut self) -> Result<f64, VmError> {
        StackOpsExt::pop_f64_or_i64(&mut self.stack, &self.struct_heap)
    }

    /// Pop a numeric-or-Char value as f64. For `Value::Char(c)`, returns
    /// the Unicode codepoint as `f64`. Used by `Instr::MakeRangeLazy`
    /// so Char ranges (`'a':'e'`) work — the resulting `RangeValue`
    /// stores the codepoint and `RangeElementType::Char` converts back
    /// via `char::from_u32` on element materialization (Issue #4795).
    #[inline]
    pub fn pop_f64_or_i64_or_char(&mut self) -> Result<f64, VmError> {
        if let Some(crate::vm::Value::Char(c)) = self.stack.last() {
            let cp = *c as u32 as f64;
            self.stack.pop();
            return Ok(cp);
        }
        StackOpsExt::pop_f64_or_i64(&mut self.stack, &self.struct_heap)
    }

    /// Pop a complex number from stack, handling promotion from real numbers.
    /// Uses StackOpsExt with struct_heap context.
    #[inline]
    pub fn pop_complex(&mut self) -> Result<(f64, f64), VmError> {
        StackOpsExt::pop_complex(&mut self.stack, &self.struct_heap)
    }

    /// Pop exception handlers that were pushed by the current function.
    /// This should be called before returning from a function to clean up
    /// any try-catch handlers that are still active.
    ///
    /// Handlers store `return_ip_len` which is the length of return_ips
    /// when the handler was pushed. After a callee returns, its handlers have
    /// a greater return_ip_len than the caller's current return_ips length.
    /// Caller handlers have the same length and must remain active.
    pub(crate) fn pop_handlers_for_return(&mut self) {
        let current_return_ip_len = self.return_ips.len();
        // Pop handlers that were pushed in the current function frame
        // (their return_ip_len >= current_return_ip_len means they were
        // pushed after we entered this function)
        while let Some(handler) = self.handlers.last() {
            if handler.return_ip_len > current_return_ip_len {
                self.handlers.pop();
            } else {
                break;
            }
        }
    }

    pub(super) fn handle_error(&mut self, err: VmError) -> Result<(), VmError> {
        // During an `eval`-driven nested dispatch, do not route an error to a
        // handler installed by an *ancestor* frame (Issue #5972). Such a handler
        // (`frame_len <= eval_dispatch_floor`) lives in a `try` opened *outside*
        // the nested `run_until_frame_return` loop; catching it here would
        // truncate `self.frames` below the floor and make that loop return
        // mid-catch, swallowing the exception. Propagate the error as `Err`
        // instead: it unwinds out of `run_until_frame_return`/`eval_dispatch_call`
        // and the outer `run()` loop's `CallBuiltin` arm re-`raise`s it (with the
        // floor restored to the ancestor's level), routing it correctly. Handlers
        // installed *within* the nested dispatch (`frame_len > floor`) are caught
        // here as usual, so a `try`/`catch` inside the eval'd code still works.
        if let Some(floor) = self.eval_dispatch_floor {
            if self.handlers.last().is_some_and(|h| h.frame_len <= floor) {
                return Err(err);
            }
        }
        if let Some(handler) = self.handlers.pop() {
            self.pending_error = Some(err);
            self.rethrow_on_finally = handler.catch_ip.is_none() && handler.finally_ip.is_some();
            self.stack.truncate(handler.stack_len);
            self.frames.truncate(handler.frame_len);
            self.caught_exceptions
                .truncate(handler.caught_exception_len);
            self.generated_expr_pending_keys
                .retain(|depth, _| *depth < handler.frame_len);
            self.generated_expr_pending_eval_frames
                .retain(|depth, _| *depth < handler.frame_len);
            self.return_ips.truncate(handler.return_ip_len);

            if let Some(catch_ip) = handler.catch_ip {
                self.ip = catch_ip;
            } else if let Some(finally_ip) = handler.finally_ip {
                self.ip = finally_ip;
            } else {
                let err = self
                    .pending_error
                    .take()
                    .unwrap_or(VmError::InvalidInstruction);
                self.rethrow_on_finally = false;
                return Err(err);
            }
            Ok(())
        } else {
            Err(err)
        }
    }

    pub(super) fn error_code(err: &VmError) -> i64 {
        match err {
            VmError::ErrorException(_) => 0, // User-thrown error
            VmError::AssertionFailed(_) => 1,
            VmError::Cancelled => 17,
            VmError::DivisionByZero => 2,
            VmError::StackOverflow => 3,
            VmError::StackUnderflow => 4,
            VmError::InvalidInstruction => 5,
            VmError::IndexOutOfBounds { .. } => 6,
            VmError::DimensionMismatch { .. } => 7,
            VmError::MatMulDimensionMismatch { .. } => 8,
            VmError::BroadcastDimensionMismatch { .. } => 9,
            VmError::EmptyArrayPop => 10,
            VmError::TypeError(_) => 11,
            VmError::DomainError(_) => 12,
            VmError::UnknownBroadcastOp(_) => 13,
            VmError::FieldIndexOutOfBounds { .. } => 14,
            VmError::ImmutableFieldAssign(_) => 15,
            VmError::NotImplemented(_) => 16,
            // Tuple/NamedTuple/Dict errors
            VmError::TupleIndexOutOfBounds { .. } => 18,
            VmError::EmptyTuple => 19,
            VmError::TupleDestructuringMismatch { .. } => 20,
            VmError::NamedTupleFieldNotFound(_) => 21,
            VmError::NamedTupleLengthMismatch { .. } => 22,
            VmError::DictKeyNotFound(_) => 23,
            VmError::InvalidDictKey(_) => 24,
            VmError::RangeIndexOutOfBounds { .. } => 25,
            VmError::EmptyRange => 26,
            VmError::UndefVarError(_) => 27,
            VmError::StringIndexError { .. } => 28,
            VmError::MethodError(_) => 29,
            VmError::InexactError(_) => 30,
            VmError::UndefKeywordError(_) => 31,
            VmError::OverflowError(_) => 32,
            VmError::InternalError(_) => 33,
        }
    }

    pub(super) fn raise(&mut self, err: VmError) -> Result<(), VmError> {
        if self.handle_error(err.clone()).is_ok() {
            Ok(())
        } else {
            Err(err)
        }
    }

    /// Clone the top `n` values of the operand stack, preserving their
    /// bottom-to-top order. Returns fewer than `n` values if the stack is
    /// shorter. Used to build diagnostics (e.g. argument type names for a
    /// `MethodError`) without consuming the stack (Issue #5493).
    pub(crate) fn peek_stack_top(&self, n: usize) -> Vec<Value> {
        let len = self.stack.len();
        let start = len.saturating_sub(n);
        self.stack[start..].to_vec()
    }

    pub(super) fn try_or_handle<T>(
        &mut self,
        result: Result<T, VmError>,
    ) -> Result<Option<T>, VmError> {
        match result {
            Ok(val) => Ok(Some(val)),
            Err(err) => {
                if self.handle_error(err.clone()).is_ok() {
                    Ok(None)
                } else {
                    Err(err)
                }
            }
        }
    }

    /// Get a function by index, returning an error if the index is out of bounds.
    ///
    /// This is the single source of truth for function index lookups.
    /// All execution modules should use this method instead of raw
    /// `self.functions.get(idx).ok_or_else(...)` or `match self.functions.get(idx)`.
    pub(super) fn get_function_checked(&self, index: usize) -> Result<&FunctionInfo, VmError> {
        self.functions
            .get(index)
            .map(|func| func.as_ref())
            .ok_or_else(|| {
                VmError::InternalError(format!(
                    "Function index {} out of bounds (have {} functions)",
                    index,
                    self.functions.len()
                ))
            })
    }

    /// Get function indices by name using the pre-built index (Issue #3361).
    /// Returns an empty slice if no functions match.
    #[inline]
    pub(crate) fn get_function_indices_by_name(&self, name: &str) -> &[usize] {
        self.function_name_index
            .get(name)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    pub(crate) fn current_dispatch_world(&self) -> u64 {
        self.frames
            .last()
            .and_then(|frame| frame.func_index.map(|_| frame.world_age))
            .unwrap_or(self.current_world)
    }

    pub(crate) fn function_visible_in_world(&self, index: usize, world: u64) -> bool {
        self.functions
            .get(index)
            .is_some_and(|func| func.min_world <= world)
    }

    pub(crate) fn activate_eval_function(&mut self, index: usize) {
        self.current_world = self.current_world.saturating_add(1);
        let func_name = self.functions.get(index).map(|func| func.name.clone());
        if let Some(func) = self.functions.get_mut(index) {
            std::rc::Rc::make_mut(func).min_world = self.current_world;
        }
        if let Some(func_name) = func_name {
            if let Some(frame) = self.frames.first_mut() {
                util::bind_value_to_frame(
                    frame,
                    &func_name,
                    ValueType::Any,
                    Value::Function(FunctionValue::new(func_name.clone())),
                    &mut self.struct_heap,
                );
            }
        }
        self.method_dispatch_cache.clear();
        self.dispatch_cache.clear();
        self.binary_both_dispatch_cache.clear();
    }

    /// Return a lightweight exact-type fingerprint for L1 call-site caching.
    ///
    /// Only scalar/runtime singleton types whose dispatch identity is exactly
    /// represented by the value tag participate here. Parametric containers,
    /// structs, `Type{T}`, and function singletons keep using the L2 cache so
    /// L1 hits never collapse distinct Julia dispatch identities.
    #[inline]
    pub(crate) fn call_site_arg_fingerprint(&self, value: &Value) -> Option<u64> {
        exact_call_site_fingerprint(&[value])
    }

    #[inline]
    pub(crate) fn call_site_arg_fingerprints(&self, values: &[&Value]) -> Option<u64> {
        exact_call_site_fingerprint(values)
    }

    /// Look up the monomorphic L1 call-site cache directly by bytecode IP.
    #[inline]
    pub(crate) fn lookup_call_site_inline_cache(
        &self,
        call_site_ip: usize,
        arg_fingerprint: u64,
    ) -> Option<usize> {
        let cached = self
            .call_site_caches
            .get(call_site_ip)
            .and_then(|cache| cache.lookup(arg_fingerprint));
        if cached.is_some() {
            crate::vm::profiler::record_event("CallSiteDispatchCacheHit");
        }
        cached
    }

    /// Store the monomorphic L1 call-site cache directly by bytecode IP.
    #[inline]
    pub(crate) fn store_call_site_inline_cache(
        &mut self,
        call_site_ip: usize,
        arg_fingerprint: Option<u64>,
        func_index: usize,
    ) {
        let Some(arg_fingerprint) = arg_fingerprint else {
            return;
        };
        if let Some(cache) = self.call_site_caches.get_mut(call_site_ip) {
            cache.store(arg_fingerprint, func_index);
        }
    }

    /// Look up a call-site polymorphic inline cache entry (Issue #5079).
    ///
    /// The cache stores `usize::MAX` as a negative/sentinel entry for call
    /// forms that fall back to a builtin or native boundary.
    #[inline]
    pub(crate) fn lookup_call_site_dispatch_cache(
        &self,
        call_site_ip: usize,
        type_hash: u64,
    ) -> Option<usize> {
        let cached = self
            .dispatch_cache
            .get(&call_site_ip)
            .and_then(|m| m.get(&type_hash))
            .copied();
        match cached {
            Some(usize::MAX) => {
                crate::vm::profiler::record_event("CallSiteDispatchNegativeCacheHit");
                Some(usize::MAX)
            }
            Some(idx) => {
                crate::vm::profiler::record_event("CallSiteDispatchCacheHit");
                Some(idx)
            }
            None => {
                crate::vm::profiler::record_event("CallSiteDispatchCacheMiss");
                None
            }
        }
    }

    /// Store a call-site polymorphic inline cache entry (Issue #5079).
    #[inline]
    pub(crate) fn store_call_site_dispatch_cache(
        &mut self,
        call_site_ip: usize,
        type_hash: u64,
        func_index: usize,
    ) {
        crate::vm::profiler::record_event("CallSiteDispatchCacheFill");
        self.dispatch_cache
            .entry(call_site_ip)
            .or_default()
            .insert(type_hash, func_index);
    }

    /// Convert a value to the representation used by local slots (Issue #5173).
    ///
    /// Mutable structs keep Julia reference identity, so slots store a
    /// `StructRef` into `struct_heap`. Immutable structs have value semantics and
    /// can stay inline in the slot, avoiding the unbounded heap growth that
    /// tight loops saw when every `StoreSlot` cloned an immutable value into
    /// `struct_heap`.
    #[inline]
    pub(crate) fn value_for_slot_storage(&mut self, val: Value) -> Value {
        match val {
            Value::Struct(s)
                if self
                    .struct_defs
                    .get(s.type_id)
                    .map(|def| def.is_mutable)
                    .unwrap_or(false) =>
            {
                let idx = self.struct_heap.len();
                self.struct_heap.push(s);
                Value::StructRef(idx)
            }
            other => other,
        }
    }

    /// Get a cloned function by index, raising through error handling if not found.
    ///
    /// Returns `Ok(Some(func))` if the function was found, `Ok(None)` if the index
    /// was invalid but the error was caught by a try-catch handler, or `Err` if
    /// the error propagated.
    /// Clone the `Rc` handle (O(1) refcount bump) for the function at `index`,
    /// routing an out-of-bounds index through the VM's exception handling.
    ///
    /// The returned `Rc<FunctionInfo>` lets the caller drop its borrow of
    /// `self.functions` and take `&mut self` for frame setup without cloning the
    /// whole (multi-`Vec`/`String`) `FunctionInfo` on every dynamic call (Issue
    /// #6853).
    pub(super) fn get_function_cloned_or_raise(
        &mut self,
        index: usize,
    ) -> Result<Option<Rc<FunctionInfo>>, VmError> {
        let result = self.functions.get(index).cloned().ok_or_else(|| {
            VmError::InternalError(format!(
                "Function index {} out of bounds (have {} functions)",
                index,
                self.functions.len()
            ))
        });
        self.try_or_handle(result)
    }

    // ==================== Boolean Context Helpers ====================

    /// Check if a value is a boolean and return its value.
    /// Returns Err(TypeError) if the value is not a boolean (Julia semantics).
    #[inline]
    pub(super) fn expect_bool(&self, v: &Value) -> Result<bool, VmError> {
        match v {
            Value::Bool(b) => Ok(*b),
            _ => {
                let type_name = self.get_type_name(v);
                Err(VmError::TypeError(format!(
                    "non-boolean ({}) used in boolean context",
                    type_name
                )))
            }
        }
    }

    /// Execute JumpIfZero instruction: jump to target if condition is false.
    /// Returns Some(target) if should jump, None if should continue.
    /// Returns Err if condition is not a boolean.
    #[inline]
    pub(super) fn execute_jump_if_zero(&mut self, target: usize) -> Result<Option<usize>, VmError> {
        let v = self.stack.pop_value()?;
        let cond = self.expect_bool(&v)?;
        Ok(if !cond { Some(target) } else { None })
    }

    // ==================== Comparison Helpers (return Bool) ====================

    /// Execute floating-point comparison, returns Bool
    #[inline]
    pub(super) fn cmp_f64<F: Fn(f64, f64) -> bool>(&mut self, op: F) -> Result<(), VmError> {
        let b = self.pop_f64_or_i64()?;
        let a = self.pop_f64_or_i64()?;
        self.stack.push(Value::Bool(op(a, b)));
        Ok(())
    }

    /// Execute integer comparison, returns Bool
    #[inline]
    pub(super) fn cmp_i64<F: Fn(i64, i64) -> bool>(&mut self, op: F) -> Result<(), VmError> {
        let b = self.stack.pop_i64()?;
        let a = self.stack.pop_i64()?;
        self.stack.push(Value::Bool(op(a, b)));
        Ok(())
    }
}
