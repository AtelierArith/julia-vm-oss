get_quote_value(x) = x.value

get_quote_value(QuoteNode(:field)) === :field
