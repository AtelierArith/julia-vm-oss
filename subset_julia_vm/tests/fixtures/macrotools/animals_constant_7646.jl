using MacroTools

animals = MacroTools.animals

animals isa Vector{Symbol} || error("MacroTools.animals should be Vector{Symbol}")
length(animals) == 214 || error("unexpected animals length")
animals[1] == :wombat || error("unexpected first animal")
animals[length(animals)] == :fly || error("unexpected last animal")

true
