using Test

repo_root_path = joinpath("subset_julia_vm", "packages", "MacroTools", "animals.txt")
crate_root_path = joinpath("packages", "MacroTools", "animals.txt")
animals_path = isfile(repo_root_path) ? repo_root_path : crate_root_path

lines = collect(eachline(animals_path))
@test length(lines) == 214
@test lines[1] == "wombat"
@test lines[end] == "fly"

symbols = map(Symbol, eachline(animals_path))
@test length(symbols) == 214
@test string(symbols[1]) == "wombat"
@test string(symbols[end]) == "fly"

true
