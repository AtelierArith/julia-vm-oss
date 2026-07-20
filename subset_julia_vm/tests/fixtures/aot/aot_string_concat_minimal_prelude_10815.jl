# vm_aot equivalence corpus widening (Issue #10815): String parameters,
# concatenation (`*`), and return values under the AoT minimal-prelude
# codegen path.
function greet(name::String)::String
    "hello " * name
end

println(greet("world"))

greet("world") == "hello world"
