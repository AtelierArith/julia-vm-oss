"""
    @alias alias_name real_name

Define `alias_name` as a constant alias for `real_name`.
"""
macro alias(alias_name::Symbol, real_name::Symbol)
    result = quote
        if isdefined($__module__, $(QuoteNode(alias_name)))
            $alias_name === $real_name || error("Alias '$($(string(alias_name)))' is already defined with a different value")
        else
            @doc """
                $($(string(alias_name)))

            Alias for `$($(string(real_name)))`.
            """
            const $alias_name = $real_name
        end
        if $(QuoteNode(real_name)) in names($__module__)
            export $alias_name
        elseif $(QuoteNode(alias_name)) in names($__module__)
            export $real_name
        end
    end
    return esc(result)
end
