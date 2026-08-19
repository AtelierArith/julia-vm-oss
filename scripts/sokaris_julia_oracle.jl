#!/usr/bin/env julia

function json_string(value)
    escaped = replace(string(value), '\\' => "\\\\", '"' => "\\\"", '\n' => "\\n", '\r' => "\\r", '\t' => "\\t")
    return "\"$(escaped)\""
end

function typed_value(value)
    if value isa Bool
        return "{\"kind\":\"bool\",\"value\":$(value ? "true" : "false")}"
    elseif value isa Integer
        return "{\"kind\":\"i64\",\"value\":$(json_string(value))}"
    elseif value isa AbstractFloat
        encoded = isnan(value) ? "NaN" : value == Inf ? "Infinity" : value == -Inf ? "-Infinity" : string(value)
        json_value = isfinite(value) ? encoded : json_string(encoded)
        return "{\"kind\":\"f64\",\"value\":$(json_value)}"
    elseif value === nothing
        return "{\"kind\":\"none\",\"value\":null}"
    end
    error("unsupported oracle result type $(typeof(value))")
end

case_id = get(ENV, "SOKARIS_CASE_ID", "")
try
    expected_version = get(ENV, "SOKARIS_EXPECTED_JULIA_VERSION", "")
    isempty(case_id) && error("SOKARIS_CASE_ID is required")
    isempty(expected_version) && error("SOKARIS_EXPECTED_JULIA_VERSION is required")
    if string(VERSION) != expected_version
        print("{\"schemaVersion\":1,\"caseId\":$(json_string(case_id)),\"status\":\"failed\",\"code\":\"julia_version_mismatch\",\"message\":$(json_string("expected Julia $(expected_version), found $(VERSION)"))}")
        exit(1)
    end
    @eval using Sokaris
    expression = get(ENV, "SOKARIS_ORACLE_EXPRESSION", "")
    if isempty(expression)
        module_name = get(ENV, "SOKARIS_MODULE", "")
        symbol_name = get(ENV, "SOKARIS_SYMBOL", "")
        target_module = getfield(Sokaris, Symbol(module_name))
        isdefined(target_module, Symbol(symbol_name)) || error("$(module_name).$(symbol_name) is not defined")
        result = "{\"kind\":\"discovery\",\"value\":true}"
    else
        result = typed_value(Core.eval(Main, Meta.parse(expression)))
    end
    print("{\"schemaVersion\":1,\"caseId\":$(json_string(case_id)),\"status\":\"passed\",\"juliaVersion\":$(json_string(VERSION)),\"result\":$(result)}")
catch error_value
    message = sprint(showerror, error_value)
    print("{\"schemaVersion\":1,\"caseId\":$(json_string(case_id)),\"status\":\"failed\",\"code\":\"oracle_failure\",\"message\":$(json_string(message))}")
    exit(1)
end
