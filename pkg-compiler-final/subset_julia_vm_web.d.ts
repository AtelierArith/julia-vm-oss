/* tslint:disable */
/* eslint-disable */

export interface CompilerSpan { start: number; end: number; start_line: number; start_column: number; end_line: number; end_column: number; }
export interface CompilerDiagnostic { code: string; kind: string; message: string; span?: CompilerSpan; workaround?: string; }
export interface CompileExport { export_name: string; function_name: string; arg_types?: string[]; }
export interface CompileImport { module: string; name: string; function_name: string; params?: string[]; result?: string; }
export interface ResolvedWasmImport { module: string; name: string; function_name: string; params: string[]; result?: string; }
export interface CompileOptions { source_name?: string; opt_level?: 0 | 1 | 2 | 3; exports?: CompileExport[]; imports?: CompileImport[]; }
export interface PhaseTimings { source_parse_lower_ms: number; dead_code_elimination_ms: number; type_inference_ms: number; ir_conversion_ms: number; optimization_ms: number; wasm_ir_lowering_ms: number; wasm_codegen_ms: number; total_ms: number; }
export interface CompileToWasmResult { success: boolean; wasm_bytes: Uint8Array; diagnostics: CompilerDiagnostic[]; compiler_version: string; abi_version: number; imports: ResolvedWasmImport[]; phase_timings: PhaseTimings; }
export function compile_to_wasm(source: string, options?: CompileOptions): CompileToWasmResult;



/**
 * Register VM vs stack VM measurement entry for wasm32 (Issue #8559).
 *
 * Wraps one precompiled program so repeated `run` calls time VM execution
 * only (parsing/lowering/compilation happen once, in the constructor). The
 * JavaScript driver (`scripts/register_vm_wasm_bench_8559.mjs`) measures
 * wall time around `run` and reads the deterministic engine counters from
 * the returned object. wasm32-unknown-unknown has no process environment,
 * so the engine gates are toggled through the
 * `set_register_vm_forced`/`set_stack_vm_metrics_forced` process overrides
 * instead of `SJULIA_REGISTER_VM`/`SJULIA_STACK_VM_METRICS`.
 */
export class RegisterVmBench {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Parse, lower, and compile `source` once through the shared pipeline
     * (same path as `run_from_source`).
     */
    constructor(source: string);
    /**
     * Execute the precompiled program once on a fresh `Vm`.
     *
     * `register_gate` routes eligible direct calls through the register VM
     * prototype; `collect_counters` arms the stack VM metrics (leave it
     * `false` for wall-time runs so counter bookkeeping does not perturb
     * the timing).
     */
    run(register_gate: boolean, collect_counters: boolean, seed: bigint): any;
}

/**
 * Return the C ABI version baked into this WASM build (Issue #9001).
 *
 * For WASM consumers the JS glue and the WASM module are bundled together at
 * build time, so there is no runtime binary mismatch risk.  This function
 * exposes the version as an informational export so host applications can
 * log or assert it matches the version they were written against.
 */
export function abi_version(): number;

/**
 * List of supported Julia subset features
 */
export function get_supported_features(): any;

/**
 * List of unsupported features
 */
export function get_unsupported_features(): any;

/**
 * Get the version of the VM
 */
export function get_version(): string;

export function init(): void;

/**
 * Run Julia source code directly using the pure Rust parser.
 *
 * This is the recommended entry point for running Julia code in WASM.
 * It uses subset_julia_vm_parser which is a pure Rust parser that works
 * natively in WASM without requiring web-tree-sitter.
 *
 * # Arguments
 * * `source` - Julia source code to execute
 * * `seed` - Random seed for deterministic execution
 *
 * # Returns
 * An ExecutionResult object containing success status, value, output, and error message
 */
export function run_from_source(source: string, seed: bigint): any;

/**
 * Run Julia source code and return an ExecutionResult with `typed_value`
 * populated as a structured JavaScript object.
 */
export function run_from_source_typed(source: string, seed: bigint): any;

/**
 * Run Julia source with one named browser RGBA image available through `load(name)`.
 */
export function run_from_source_with_image(source: string, seed: bigint, name: string, width: number, height: number, rgba: Uint8Array): any;

/**
 * Run a Core IR JSON program and return the result.
 *
 * This function takes a JSON-serialized Core IR program and executes it.
 * The IR should be generated from Julia source code using the lowering pipeline.
 *
 * # Arguments
 * * `ir_json` - JSON string representing the Core IR program
 * * `seed` - Random seed for deterministic execution
 *
 * # Returns
 * An ExecutionResult object containing success status, value, output, and error message
 */
export function run_ir_json(ir_json: string, seed: bigint): any;

/**
 * Run IR JSON and return just the numeric result.
 * Returns NaN on error.
 */
export function run_ir_simple(ir_json: string, seed: bigint): number;

/**
 * Get completions for a LaTeX prefix.
 * Returns a JSON array of [latex, unicode] pairs.
 */
export function unicode_completions(prefix: string): any;

/**
 * Expand all LaTeX sequences in a string to their Unicode equivalents.
 */
export function unicode_expand(input: string): string;

/**
 * Look up a LaTeX command and return its Unicode representation.
 * Returns null if not found.
 */
export function unicode_lookup(latex: string): string | undefined;

/**
 * Reverse lookup: get LaTeX for a Unicode character.
 * Returns null if not found.
 */
export function unicode_reverse_lookup(unicode_char: string): string | undefined;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_registervmbench_free: (a: number, b: number) => void;
    readonly abi_version: () => number;
    readonly get_supported_features: () => any;
    readonly get_unsupported_features: () => any;
    readonly get_version: () => [number, number];
    readonly init: () => void;
    readonly registervmbench_new: (a: number, b: number) => [number, number, number];
    readonly registervmbench_run: (a: number, b: number, c: number, d: bigint) => [number, number, number];
    readonly run_from_source: (a: number, b: number, c: bigint) => any;
    readonly run_from_source_typed: (a: number, b: number, c: bigint) => any;
    readonly run_from_source_with_image: (a: number, b: number, c: bigint, d: number, e: number, f: number, g: number, h: number, i: number) => any;
    readonly run_ir_json: (a: number, b: number, c: bigint) => any;
    readonly run_ir_simple: (a: number, b: number, c: bigint) => number;
    readonly unicode_completions: (a: number, b: number) => any;
    readonly unicode_expand: (a: number, b: number) => [number, number];
    readonly unicode_lookup: (a: number, b: number) => [number, number];
    readonly unicode_reverse_lookup: (a: number, b: number) => [number, number];
    readonly compile_to_wasm: (a: number, b: number, c: number) => any;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
