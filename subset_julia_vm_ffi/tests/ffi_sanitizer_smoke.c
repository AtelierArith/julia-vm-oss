#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "subset_vm.h"

typedef struct {
    int calls;
    bool saw_line;
} StreamState;

static void fail(const char* message) {
    fprintf(stderr, "ffi_sanitizer_smoke: %s\n", message);
    abort();
}

static void stream_callback(void* context, const char* output) {
    StreamState* state = (StreamState*)context;
    if (state == NULL || output == NULL) {
        fail("stream callback received null state/output");
    }
    state->calls += 1;
    if (strstr(output, "hello") != NULL) {
        state->saw_line = true;
    }
}

static void expect_success_result(CExecutionResult* result) {
    if (result == NULL) {
        fail("expected non-null CExecutionResult");
    }
    if (!result->success) {
        fail("expected successful CExecutionResult");
    }
    const char* value_json = execution_result_value_json(result);
    if (value_json == NULL || strlen(value_json) == 0) {
        fail("successful result should expose borrowed value JSON");
    }
    (void)execution_result_value_kind(result);
    (void)execution_result_artifact_mime(result);
    (void)execution_result_artifact_data(result);
    (void)execution_result_artifact_count(result);
    (void)execution_result_artifact_mime_at(result, 0);
    (void)execution_result_artifact_data_at(result, 0);
}

int main(void) {
    char* ir = compile_to_ir("1 + 1");
    if (ir == NULL || strlen(ir) == 0) {
        fail("compile_to_ir returned null/empty");
    }
    free_string(ir);

    CExecutionResult* result = compile_and_run_detailed("1 + 1", 42);
    expect_success_result(result);
    free_execution_result(result);

    result = compile_and_run_detailed(NULL, 42);
    if (result == NULL || result->success || result->error.message == NULL) {
        fail("null source should return an owned error result");
    }
    free_execution_result(result);

    StreamState state = {0, false};
    result = compile_and_run_streaming("println(\"hello\")\n2 + 3", 42, &state, stream_callback);
    expect_success_result(result);
    if (state.calls == 0 || !state.saw_line) {
        fail("streaming callback did not observe println output");
    }
    free_execution_result(result);

    void* session = repl_session_new(7);
    if (session == NULL) {
        fail("repl_session_new returned null");
    }
    CREPLResult* repl = repl_session_eval(session, "x = 10\nx + 5");
    if (repl == NULL || !repl->success || repl->value == NULL) {
        fail("repl_session_eval should return a successful value");
    }
    (void)repl_result_artifact_mime(repl);
    (void)repl_result_artifact_data(repl);
    (void)repl_result_artifact_count(repl);
    (void)repl_result_artifact_mime_at(repl, 0);
    (void)repl_result_artifact_data_at(repl, 0);
    free_repl_result(repl);
    repl_session_reset(session);
    repl_session_free(session);

#ifdef SJULIA_FFI_PANIC_TEST
    result = subset_julia_vm_ffi_debug_panic_detailed();
    if (result == NULL || result->success || result->error.message == NULL) {
        fail("detailed panic probe should return an error result");
    }
    free_execution_result(result);

    repl = subset_julia_vm_ffi_debug_panic_repl();
    if (repl == NULL || repl->success || repl->error == NULL) {
        fail("REPL panic probe should return an error result");
    }
    free_repl_result(repl);
#endif

    return 0;
}
