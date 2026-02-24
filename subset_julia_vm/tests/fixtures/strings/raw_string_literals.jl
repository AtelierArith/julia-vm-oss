# Test raw string literals (Issue #554)
# Tests the raw"..." string macro literal
# In Julia, raw strings process \\ (to single \) and \" (to ")
# but keep other escape sequences like \n as literal backslash+letter

using Test

# Test 1: raw string preserves backslash+letter sequences
@test raw"\n\t" == "\\n\\t"
@test length(raw"\n\t") == 4

# Test 2: raw string processes double backslash to single backslash
# raw"\\\\" has 4 backslashes in source -> 2 backslashes
@test raw"\\\\" == "\\\\"
@test length(raw"\\\\") == 2

# Test 3: raw string with normal text
@test raw"hello" == "hello"
@test raw"hello world" == "hello world"

# Test 4: raw string with special characters that would normally be escapes
@test raw"\a\b\f\r\v" == "\\a\\b\\f\\r\\v"

# Test 5: raw string in function
function get_raw_string()
    return raw"\path\to\file"
end
@test get_raw_string() == "\\path\\to\\file"

# Test 6: raw string comparison
x = raw"\n"
y = "\\n"
@test x == y

# Test 7: single backslash
@test length(raw"\\") == 1

# Return true to indicate success
true
