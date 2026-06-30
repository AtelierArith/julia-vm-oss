#pragma once
#include <stdint.h>
#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

// ==================== Ownership Summary ====================
//
// - char* returned directly from compile_to_ir, compile_and_run_with_output,
//   split_expressions, unicode_* and *_json copy accessors is owned by the
//   caller and must be released with free_string().
// - CExecutionResult* returned by compile_and_run_detailed and
//   compile_and_run_streaming is owned by the caller and must be released with
//   free_execution_result().
// - char* fields inside CExecutionResult and CREPLResult are owned by their
//   parent result. Read them before freeing the result; do not pass them to
//   free_string().
// - const char* returned by execution_result_value_json() and
//   execution_result_artifact_*() is borrowed from CExecutionResult and remains
//   valid only until free_execution_result().
// - const char* returned by repl_result_artifact_*() is borrowed from
//   CREPLResult and remains valid only until free_repl_result().

// Run JSON Core IR programs.
int64_t run_ir_json_f_N_seed(const char* json, int64_t n, uint64_t seed);
double run_ir_json_f64_N_seed(const char* json, int64_t n, uint64_t seed);
double run_ir_json_f64(const char* json);

// ==================== Detailed Error FFI Structs ====================

// Source span for error location (byte offsets and 1-indexed line/column)
typedef struct {
    uint32_t start;
    uint32_t end;
    uint32_t start_line;
    uint32_t end_line;
    uint32_t start_column;
    uint32_t end_column;
} CSpan;

// Error kind enumeration
typedef enum {
    CErrorKind_None = 0,
    CErrorKind_Syntax = 1,
    CErrorKind_Unsupported = 2,
    CErrorKind_Runtime = 3,
    CErrorKind_Compile = 4,
} CErrorKind;

// Stable type tags for CExecutionResult.value_json.
typedef enum {
    CValueKind_Unknown = 0,
    CValueKind_Nothing = 1,
    CValueKind_Missing = 2,
    CValueKind_Bool = 3,
    CValueKind_Int = 4,
    CValueKind_UInt = 5,
    CValueKind_Float = 6,
    CValueKind_String = 7,
    CValueKind_Char = 8,
    CValueKind_Complex = 9,
    CValueKind_Array = 10,
    CValueKind_Dict = 11,
    CValueKind_Tuple = 12,
    CValueKind_NamedTuple = 13,
    CValueKind_Struct = 14,
    CValueKind_Symbol = 15,
    CValueKind_Range = 16,
    CValueKind_Enum = 17,
    CValueKind_Artifact = 18,
    CValueKind_Opaque = 19,
} CValueKind;

// Error struct with detailed information
typedef struct {
    CErrorKind kind;
    CSpan span;
    char* message;  // Owned by parent CExecutionResult, may be NULL
    char* hint;     // Owned by parent CExecutionResult, may be NULL
} CError;

// Execution result with detailed error information
typedef struct {
    bool success;
    // Legacy numeric projection: Int/Float values, real part for Complex,
    // 0.0 for nothing, NaN for non-scalar values.
    double result_value;
    char* output;   // Owned by this result; println output, may be NULL
    CError error;
    char* artifact_mime; // Owned by this result; MIME string, may be NULL
    char* artifact_data; // Owned by this result; UTF-8 artifact data, may be NULL
    char* value_json;    // Owned by this result; structured typed value JSON
} CExecutionResult;

// Cancel execution of the currently running VM (best-effort).
void vm_request_cancel(void);
// Clear any pending cancellation before starting a new run.
void vm_reset_cancel(void);

// Compile Julia subset source to IR JSON.
// Returns a heap-allocated string that must be freed with free_string.
// Returns NULL on error.
char* compile_to_ir(const char* src);
void free_string(char* ptr);

// Compile and run Julia subset source with function definition and call.
// e.g., "function f(N) ... end\nf(1000)"
// Returns the result as f64. Returns NaN on error.
double compile_and_run(const char* src, uint64_t seed);

// Compile and run Julia subset source (auto-detect: function or simple program).
// Supports both:
// - "function f(N) ... end\nf(1000)"
// - "println(\"Hello world\")"
// Returns the result as f64. Returns NaN on error or 0.0 for void results.
double compile_and_run_auto(const char* src, uint64_t seed);

// Compile and run Julia subset source, returning output as a string.
// Returns a heap-allocated string that must be freed with free_string.
// The output includes both println output and the result value.
// Returns NULL on error.
char* compile_and_run_with_output(const char* src, uint64_t seed);

// Compile and run with detailed error information.
// Returns a heap-allocated CExecutionResult that must be freed with free_execution_result.
CExecutionResult* compile_and_run_detailed(const char* src, uint64_t seed);

// Output callback function for compile_and_run_streaming.
// `output` is borrowed and valid only for the duration of the callback.
typedef void (*OutputCallback)(void* context, const char* output);

// Compile and run with streaming output via callback.
// Returns a heap-allocated CExecutionResult that must be freed with free_execution_result.
CExecutionResult* compile_and_run_streaming(
    const char* src,
    uint64_t seed,
    void* context,
    OutputCallback output_callback
);

// Borrow the structured typed value JSON. Do not free the returned pointer.
const char* execution_result_value_json(const CExecutionResult* result);
CValueKind execution_result_value_kind(const CExecutionResult* result);

// Complex accessors. Return NaN if the result is not a complex value.
double execution_result_complex_real(const CExecutionResult* result);
double execution_result_complex_imag(const CExecutionResult* result);

// Array accessors. Indexes are 0-based. JSON element copies must be freed with free_string().
uint64_t execution_result_array_len(const CExecutionResult* result);
CValueKind execution_result_array_element_kind(const CExecutionResult* result, uint64_t index);
double execution_result_array_element_f64(const CExecutionResult* result, uint64_t index);
char* execution_result_array_element_json(const CExecutionResult* result, uint64_t index);

// Dictionary accessors. Indexes are 0-based slot iteration order.
// Key/value JSON copies must be freed with free_string().
uint64_t execution_result_dict_len(const CExecutionResult* result);
char* execution_result_dict_key_json(const CExecutionResult* result, uint64_t index);
char* execution_result_dict_value_json(const CExecutionResult* result, uint64_t index);

// Borrow artifact strings. Do not free the returned pointers.
const char* execution_result_artifact_mime(const CExecutionResult* result);
const char* execution_result_artifact_data(const CExecutionResult* result);

// Free a CExecutionResult allocated by compile_and_run_detailed or compile_and_run_streaming.
void free_execution_result(CExecutionResult* result);

// Check if a Julia expression is complete (can be evaluated).
// Returns 1 if complete, 0 if incomplete (e.g., unclosed brackets, unfinished blocks).
int32_t is_expression_complete(const char* src);

// Split Julia source code into top-level expressions.
// Returns a JSON array of expression strings, or NULL on error.
// The result must be freed with free_string.
char* split_expressions(const char* src);

// ==================== REPL Session API ====================

// REPL result struct returned by repl_session_eval
typedef struct {
    bool success;
    char* output;   // Heap-allocated, may be NULL (println/print output only)
    char* value;    // Heap-allocated, may be NULL (formatted result value)
    char* error;    // Heap-allocated, may be NULL
    char* artifact_mime; // Owned by this result; MIME string, may be NULL
    char* artifact_data; // Owned by this result; UTF-8 artifact data, may be NULL
} CREPLResult;

// Create a new REPL session with the given random seed.
// Returns an opaque pointer to the session.
void* repl_session_new(uint64_t seed);

// Evaluate Julia code in a REPL session.
// Returns a heap-allocated CREPLResult that must be freed with free_repl_result.
CREPLResult* repl_session_eval(void* session, const char* src);

// Reset a REPL session (clears all state).
void repl_session_reset(void* session);

// Free a REPL session.
void repl_session_free(void* session);

// Free a REPL result.
void free_repl_result(CREPLResult* result);

// Borrow REPL artifact strings. Do not free the returned pointers.
const char* repl_result_artifact_mime(const CREPLResult* result);
const char* repl_result_artifact_data(const CREPLResult* result);

void subset_julia_vm_demo(void);

// ==================== Unicode Completion API ====================

// Look up a LaTeX command and return its Unicode representation.
// Returns a heap-allocated string that must be freed with free_string, or NULL if not found.
// Example: unicode_lookup("\\alpha") returns "α"
char* unicode_lookup(const char* latex);

// Get completions for a LaTeX prefix.
// Returns a JSON array of [latex, unicode] pairs, or NULL on error.
// The result must be freed with free_string.
// Example: unicode_completions("\\alph") returns [["\\alpha", "α"]]
char* unicode_completions(const char* prefix);

// Expand all LaTeX sequences in a string to their Unicode equivalents.
// Returns a heap-allocated string that must be freed with free_string.
// Example: unicode_expand("x\\^2 + y\\^2") returns "x² + y²"
char* unicode_expand(const char* input);

// Reverse lookup: get LaTeX for a Unicode character.
// Returns a heap-allocated string that must be freed with free_string, or NULL if not found.
// Example: unicode_reverse_lookup("α") returns "\\alpha"
char* unicode_reverse_lookup(const char* unicode);

#ifdef __cplusplus
} // extern "C"
#endif
