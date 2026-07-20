using Test

const CONST_PROP_FLAG_8443 = true

function const_prop_flow_sensitive_8443()
    if CONST_PROP_FLAG_8443
        return typeof(1) === Int64
    else
        return typeof(1.0) === Float64
    end
end

@test const_prop_flow_sensitive_8443()

true
