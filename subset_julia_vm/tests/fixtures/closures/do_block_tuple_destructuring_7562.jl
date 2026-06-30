fields = map([(:x, :T)]) do field
    fieldname, typ = field
    (fieldname, typ)
end

fields[1] == (:x, :T) || error("do-block tuple destructuring assignment failed")

true
