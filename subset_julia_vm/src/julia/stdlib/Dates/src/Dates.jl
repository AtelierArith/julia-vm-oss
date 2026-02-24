# Dates module for SubsetJuliaVM
# Simplified implementation compatible with the VM's supported features
# Note: Abstract types are not supported in stdlib modules, so we use concrete types only

module Dates

# ============================================================================
# Period Types
# ============================================================================

struct Year
    value::Int64
end
# Note: Int == Int64 on 64-bit systems, so no conversion constructor needed

struct Quarter
    value::Int64
end

struct Month
    value::Int64
end

struct Week
    value::Int64
end

struct Day
    value::Int64
end

struct Hour
    value::Int64
end

struct Minute
    value::Int64
end

struct Second
    value::Int64
end

struct Millisecond
    value::Int64
end

struct Microsecond
    value::Int64
end

struct Nanosecond
    value::Int64
end

# Period value accessor
value(x::Year) = x.value
value(x::Quarter) = x.value
value(x::Month) = x.value
value(x::Week) = x.value
value(x::Day) = x.value
value(x::Hour) = x.value
value(x::Minute) = x.value
value(x::Second) = x.value
value(x::Millisecond) = x.value
value(x::Microsecond) = x.value
value(x::Nanosecond) = x.value

# Period negation - extend Base.- for Period types
Base.:-(x::Year) = Year(-value(x))
Base.:-(x::Quarter) = Quarter(-value(x))
Base.:-(x::Month) = Month(-value(x))
Base.:-(x::Week) = Week(-value(x))
Base.:-(x::Day) = Day(-value(x))
Base.:-(x::Hour) = Hour(-value(x))
Base.:-(x::Minute) = Minute(-value(x))
Base.:-(x::Second) = Second(-value(x))
Base.:-(x::Millisecond) = Millisecond(-value(x))
Base.:-(x::Microsecond) = Microsecond(-value(x))
Base.:-(x::Nanosecond) = Nanosecond(-value(x))

# Period addition - extend Base.+ for Period types
Base.:+(x::Year, y::Year) = Year(value(x) + value(y))
Base.:+(x::Quarter, y::Quarter) = Quarter(value(x) + value(y))
Base.:+(x::Month, y::Month) = Month(value(x) + value(y))
Base.:+(x::Week, y::Week) = Week(value(x) + value(y))
Base.:+(x::Day, y::Day) = Day(value(x) + value(y))
Base.:+(x::Hour, y::Hour) = Hour(value(x) + value(y))
Base.:+(x::Minute, y::Minute) = Minute(value(x) + value(y))
Base.:+(x::Second, y::Second) = Second(value(x) + value(y))
Base.:+(x::Millisecond, y::Millisecond) = Millisecond(value(x) + value(y))
Base.:+(x::Microsecond, y::Microsecond) = Microsecond(value(x) + value(y))
Base.:+(x::Nanosecond, y::Nanosecond) = Nanosecond(value(x) + value(y))

# Period subtraction - extend Base.- for Period types
Base.:-(x::Year, y::Year) = Year(value(x) - value(y))
Base.:-(x::Quarter, y::Quarter) = Quarter(value(x) - value(y))
Base.:-(x::Month, y::Month) = Month(value(x) - value(y))
Base.:-(x::Week, y::Week) = Week(value(x) - value(y))
Base.:-(x::Day, y::Day) = Day(value(x) - value(y))
Base.:-(x::Hour, y::Hour) = Hour(value(x) - value(y))
Base.:-(x::Minute, y::Minute) = Minute(value(x) - value(y))
Base.:-(x::Second, y::Second) = Second(value(x) - value(y))
Base.:-(x::Millisecond, y::Millisecond) = Millisecond(value(x) - value(y))
Base.:-(x::Microsecond, y::Microsecond) = Microsecond(value(x) - value(y))
Base.:-(x::Nanosecond, y::Nanosecond) = Nanosecond(value(x) - value(y))

# Period multiplication - extend Base.* for Period types
Base.:*(x::Year, y::Int) = Year(value(x) * y)
Base.:*(x::Quarter, y::Int) = Quarter(value(x) * y)
Base.:*(x::Month, y::Int) = Month(value(x) * y)
Base.:*(x::Week, y::Int) = Week(value(x) * y)
Base.:*(x::Day, y::Int) = Day(value(x) * y)
Base.:*(x::Hour, y::Int) = Hour(value(x) * y)
Base.:*(x::Minute, y::Int) = Minute(value(x) * y)
Base.:*(x::Second, y::Int) = Second(value(x) * y)
Base.:*(x::Millisecond, y::Int) = Millisecond(value(x) * y)
Base.:*(x::Microsecond, y::Int) = Microsecond(value(x) * y)
Base.:*(x::Nanosecond, y::Int) = Nanosecond(value(x) * y)

Base.:*(y::Int, x::Year) = x * y
Base.:*(y::Int, x::Quarter) = x * y
Base.:*(y::Int, x::Month) = x * y
Base.:*(y::Int, x::Week) = x * y
Base.:*(y::Int, x::Day) = x * y
Base.:*(y::Int, x::Hour) = x * y
Base.:*(y::Int, x::Minute) = x * y
Base.:*(y::Int, x::Second) = x * y
Base.:*(y::Int, x::Millisecond) = x * y
Base.:*(y::Int, x::Microsecond) = x * y
Base.:*(y::Int, x::Nanosecond) = x * y

# Period comparison - extend Base.== and Base.< for Period types
Base.:(==)(x::Year, y::Year) = value(x) == value(y)
Base.:(==)(x::Quarter, y::Quarter) = value(x) == value(y)
Base.:(==)(x::Month, y::Month) = value(x) == value(y)
Base.:(==)(x::Week, y::Week) = value(x) == value(y)
Base.:(==)(x::Day, y::Day) = value(x) == value(y)
Base.:(==)(x::Hour, y::Hour) = value(x) == value(y)
Base.:(==)(x::Minute, y::Minute) = value(x) == value(y)
Base.:(==)(x::Second, y::Second) = value(x) == value(y)
Base.:(==)(x::Millisecond, y::Millisecond) = value(x) == value(y)
Base.:(==)(x::Microsecond, y::Microsecond) = value(x) == value(y)
Base.:(==)(x::Nanosecond, y::Nanosecond) = value(x) == value(y)

Base.:(<)(x::Year, y::Year) = value(x) < value(y)
Base.:(<)(x::Quarter, y::Quarter) = value(x) < value(y)
Base.:(<)(x::Month, y::Month) = value(x) < value(y)
Base.:(<)(x::Week, y::Week) = value(x) < value(y)
Base.:(<)(x::Day, y::Day) = value(x) < value(y)
Base.:(<)(x::Hour, y::Hour) = value(x) < value(y)
Base.:(<)(x::Minute, y::Minute) = value(x) < value(y)
Base.:(<)(x::Second, y::Second) = value(x) < value(y)
Base.:(<)(x::Millisecond, y::Millisecond) = value(x) < value(y)
Base.:(<)(x::Microsecond, y::Microsecond) = value(x) < value(y)
Base.:(<)(x::Nanosecond, y::Nanosecond) = value(x) < value(y)

# ============================================================================
# Date/Time Types
# ============================================================================

struct Date
    value::Int64  # Rata Die days
end

struct DateTime
    value::Int64  # Milliseconds since epoch
end

struct Time
    value::Int64  # Nanoseconds since midnight
end

# ============================================================================
# Date/Time Helper Functions
# ============================================================================

# Convert y,m,d to Rata Die days
const SHIFTEDMONTHDAYS = (306, 337, 0, 31, 61, 92, 122, 153, 184, 214, 245, 275)

function totaldays(y::Int64, m::Int64, d::Int64)
    z = m < 3 ? y - 1 : y
    mdays = SHIFTEDMONTHDAYS[m]
    return d + mdays + 365*z + fld(z, 4) - fld(z, 100) + fld(z, 400) - 306
end

# Check if year is a leap year
isleapyear(y::Integer) = (y % 4 == 0) && ((y % 100 != 0) || (y % 400 == 0))

# Days in each month
const DAYSINMONTH = (31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31)
daysinmonth(y, m) = DAYSINMONTH[m] + (m == 2 && isleapyear(y) ? 1 : 0)

# ============================================================================
# Date/Time Constructors
# ============================================================================

function Date(y::Int64, m::Int64, d::Int64)
    return Date(totaldays(y, m, d))
end
# Note: Removed Date(y::Int, m::Int, d::Int) as Int == Int64 on 64-bit systems

Date(y::Int64, m::Int64) = Date(y, m, 1)
Date(y::Int64) = Date(y, 1, 1)

function DateTime(y::Int64, m::Int64, d::Int64, h::Int64, mi::Int64, s::Int64, ms::Int64)
    rata = ms + 1000 * (s + 60*mi + 3600*h + 86400 * totaldays(y, m, d))
    return DateTime(rata)
end

DateTime(y::Int64, m::Int64, d::Int64, h::Int64, mi::Int64, s::Int64) =
    DateTime(y, m, d, h, mi, s, 0)
DateTime(y::Int64, m::Int64, d::Int64, h::Int64, mi::Int64) =
    DateTime(y, m, d, h, mi, 0, 0)
DateTime(y::Int64, m::Int64, d::Int64, h::Int64) =
    DateTime(y, m, d, h, 0, 0, 0)
DateTime(y::Int64, m::Int64, d::Int64) =
    DateTime(y, m, d, 0, 0, 0, 0)
# Note: Removed Int->Int64 conversion constructors as Int == Int64 on 64-bit systems

function Time(h::Int64, mi::Int64, s::Int64, ms::Int64, us::Int64, ns::Int64)
    total_ns = ns + 1000*us + 1000000*ms + 1000000000*s + 60000000000*mi + 3600000000000*h
    return Time(mod(total_ns, 86400000000000))
end

Time(h::Int64, mi::Int64, s::Int64, ms::Int64, us::Int64) = Time(h, mi, s, ms, us, 0)
Time(h::Int64, mi::Int64, s::Int64, ms::Int64) = Time(h, mi, s, ms, 0, 0)
Time(h::Int64, mi::Int64, s::Int64) = Time(h, mi, s, 0, 0, 0)
Time(h::Int64, mi::Int64) = Time(h, mi, 0, 0, 0, 0)
Time(h::Int64) = Time(h, 0, 0, 0, 0, 0)
# Note: Removed Int->Int64 conversion constructor as Int == Int64 on 64-bit systems

# ============================================================================
# Value Accessors for Date/DateTime/Time
# ============================================================================

value(dt::Date) = dt.value
value(dt::DateTime) = dt.value
value(t::Time) = t.value

# Days from DateTime
days(dt::Date) = value(dt)
days(dt::DateTime) = fld(value(dt), 86400000)

# ============================================================================
# Date Component Accessors
# ============================================================================

# Convert Rata Die days to year, month, day
function yearmonthday(days::Int64)
    z = days + 306
    h = 100*z - 25
    a = fld(h, 3652425)
    b = a - fld(a, 4)
    y = fld(100*b + h, 36525)
    c = b + z - 365*y - fld(y, 4)
    m = div(5*c + 456, 153)
    d = c - div(153*m - 457, 5)
    return m > 12 ? (y + 1, m - 12, d) : (y, m, d)
end

function year(days::Int64)
    z = days + 306
    h = 100*z - 25
    a = fld(h, 3652425)
    b = a - fld(a, 4)
    y = fld(100*b + h, 36525)
    c = b + z - 365*y - fld(y, 4)
    m = div(5*c + 456, 153)
    return m > 12 ? y + 1 : y
end

function month(days::Int64)
    z = days + 306
    h = 100*z - 25
    a = fld(h, 3652425)
    b = a - fld(a, 4)
    y = fld(100*b + h, 36525)
    c = b + z - 365*y - fld(y, 4)
    m = div(5*c + 456, 153)
    return m > 12 ? m - 12 : m
end

function day(days::Int64)
    z = days + 306
    h = 100*z - 25
    a = fld(h, 3652425)
    b = a - fld(a, 4)
    y = fld(100*b + h, 36525)
    c = b + z - 365*y - fld(y, 4)
    m = div(5*c + 456, 153)
    return c - div(153*m - 457, 5)
end

function yearmonth(days::Int64)
    z = days + 306
    h = 100*z - 25
    a = fld(h, 3652425)
    b = a - fld(a, 4)
    y = fld(100*b + h, 36525)
    c = b + z - 365*y - fld(y, 4)
    m = div(5*c + 456, 153)
    return m > 12 ? (y + 1, m - 12) : (y, m)
end

function monthday(days::Int64)
    z = days + 306
    h = 100*z - 25
    a = fld(h, 3652425)
    b = a - fld(a, 4)
    y = fld(100*b + h, 36525)
    c = b + z - 365*y - fld(y, 4)
    m = div(5*c + 456, 153)
    d = c - div(153*m - 457, 5)
    return m > 12 ? (m - 12, d) : (m, d)
end

# Date accessors
year(dt::Date) = year(days(dt))
month(dt::Date) = month(days(dt))
day(dt::Date) = day(days(dt))
yearmonthday(dt::Date) = yearmonthday(days(dt))
yearmonth(dt::Date) = yearmonth(days(dt))
monthday(dt::Date) = monthday(days(dt))

# DateTime accessors
year(dt::DateTime) = year(days(dt))
month(dt::DateTime) = month(days(dt))
day(dt::DateTime) = day(days(dt))
yearmonthday(dt::DateTime) = yearmonthday(days(dt))
yearmonth(dt::DateTime) = yearmonth(days(dt))
monthday(dt::DateTime) = monthday(days(dt))

hour(dt::DateTime) = mod(fld(value(dt), 3600000), 24)
minute(dt::DateTime) = mod(fld(value(dt), 60000), 60)
second(dt::DateTime) = mod(fld(value(dt), 1000), 60)
millisecond(dt::DateTime) = mod(value(dt), 1000)

# Time accessors
hour(t::Time) = mod(fld(value(t), 3600000000000), Int64(24))
minute(t::Time) = mod(fld(value(t), 60000000000), Int64(60))
second(t::Time) = mod(fld(value(t), 1000000000), Int64(60))
millisecond(t::Time) = mod(fld(value(t), Int64(1000000)), Int64(1000))
microsecond(t::Time) = mod(fld(value(t), Int64(1000)), Int64(1000))
nanosecond(t::Time) = mod(value(t), Int64(1000))

dayofmonth(dt::Date) = day(dt)
dayofmonth(dt::DateTime) = day(dt)

# ============================================================================
# Query Functions
# ============================================================================

# Day of week (Monday = 1, Sunday = 7)
dayofweek(days::Int64) = mod(days - 1, 7) + 1
dayofweek(dt::Date) = dayofweek(days(dt))
dayofweek(dt::DateTime) = dayofweek(days(dt))

# Quarter
function quarter(days::Int64)
    m = month(days)
    return m < 4 ? 1 : m < 7 ? 2 : m < 10 ? 3 : 4
end
quarter(dt::Date) = quarter(days(dt))
quarter(dt::DateTime) = quarter(days(dt))
quarterofyear(dt::Date) = quarter(dt)
quarterofyear(dt::DateTime) = quarter(dt)

# ISO week
const WEEK_INDEX = (15, 23, 3, 11)
function week(days::Int64)
    w = div(abs(days - 1), 7) % 20871
    c, w = divrem((w + (w >= 10435 ? 1 : 0)), 5218)
    w = (w * 28 + WEEK_INDEX[c + 1]) % 1461
    return div(w, 28) + 1
end
week(dt::Date) = week(days(dt))
week(dt::DateTime) = week(days(dt))

# Days in year
daysinyear(y) = 365 + (isleapyear(y) ? 1 : 0)
daysinyear(dt::Date) = daysinyear(year(dt))
daysinyear(dt::DateTime) = daysinyear(year(dt))

# Day of year
const MONTHDAYS = (0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334)
dayofyear(y, m, d) = MONTHDAYS[m] + d + (m > 2 && isleapyear(y) ? 1 : 0)
function dayofyear(dt::Date)
    y, m, d = yearmonthday(dt)
    return dayofyear(y, m, d)
end
function dayofyear(dt::DateTime)
    y, m, d = yearmonthday(dt)
    return dayofyear(y, m, d)
end

# Day of quarter
const QUARTERDAYS = (0, 31, 59, 0, 30, 61, 0, 31, 62, 0, 31, 61)
function dayofquarter(dt::Date)
    y, m, d = yearmonthday(dt)
    return QUARTERDAYS[m] + d + (m == 3 && isleapyear(y) ? 1 : 0)
end
function dayofquarter(dt::DateTime)
    y, m, d = yearmonthday(dt)
    return QUARTERDAYS[m] + d + (m == 3 && isleapyear(y) ? 1 : 0)
end

# Days in month for Date/DateTime
daysinmonth(dt::Date) = daysinmonth(year(dt), month(dt))
daysinmonth(dt::DateTime) = daysinmonth(year(dt), month(dt))

# isleapyear for Date/DateTime
isleapyear(dt::Date) = isleapyear(year(dt))
isleapyear(dt::DateTime) = isleapyear(year(dt))

# Day of week of month (1st Monday, 2nd Monday, etc.)
function dayofweekofmonth(dt::Date)
    d = day(dt)
    return d < 8 ? 1 : d < 15 ? 2 : d < 22 ? 3 : d < 29 ? 4 : 5
end
function dayofweekofmonth(dt::DateTime)
    d = day(dt)
    return d < 8 ? 1 : d < 15 ? 2 : d < 22 ? 3 : d < 29 ? 4 : 5
end

# Day constants
const Monday = 1
const Tuesday = 2
const Wednesday = 3
const Thursday = 4
const Friday = 5
const Saturday = 6
const Sunday = 7

const Mon = 1
const Tue = 2
const Wed = 3
const Thu = 4
const Fri = 5
const Sat = 6
const Sun = 7

# Month constants
const January = 1
const February = 2
const March = 3
const April = 4
const May = 5
const June = 6
const July = 7
const August = 8
const September = 9
const October = 10
const November = 11
const December = 12

const Jan = 1
const Feb = 2
const Mar = 3
const Apr = 4
const Jun = 6
const Jul = 7
const Aug = 8
const Sep = 9
const Oct = 10
const Nov = 11
const Dec = 12

# Day name functions
const DAYNAMES = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"]
const DAYABBRS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]

dayname(day::Int) = DAYNAMES[day]
dayname(dt::Date) = dayname(dayofweek(dt))
dayname(dt::DateTime) = dayname(dayofweek(dt))
dayabbr(day::Int) = DAYABBRS[day]
dayabbr(dt::Date) = dayabbr(dayofweek(dt))
dayabbr(dt::DateTime) = dayabbr(dayofweek(dt))

# Month name functions
const MONTHNAMES = ["January", "February", "March", "April", "May", "June",
                    "July", "August", "September", "October", "November", "December"]
const MONTHABBRS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun",
                    "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]

monthname(month::Int) = MONTHNAMES[month]
monthname(dt::Date) = monthname(month(dt))
monthname(dt::DateTime) = monthname(month(dt))
monthabbr(month::Int) = MONTHABBRS[month]
monthabbr(dt::Date) = monthabbr(month(dt))
monthabbr(dt::DateTime) = monthabbr(month(dt))

# ============================================================================
# Comparison Functions
# ============================================================================

Base.:(==)(x::Date, y::Date) = value(x) == value(y)
Base.:(==)(x::DateTime, y::DateTime) = value(x) == value(y)
Base.:(==)(x::Time, y::Time) = value(x) == value(y)

Base.:(<)(x::Date, y::Date) = value(x) < value(y)
Base.:(<)(x::DateTime, y::DateTime) = value(x) < value(y)
Base.:(<)(x::Time, y::Time) = value(x) < value(y)

Base.:(<=)(x::Date, y::Date) = value(x) <= value(y)
Base.:(<=)(x::DateTime, y::DateTime) = value(x) <= value(y)
Base.:(<=)(x::Time, y::Time) = value(x) <= value(y)

Base.:(>)(x::Date, y::Date) = value(x) > value(y)
Base.:(>)(x::DateTime, y::DateTime) = value(x) > value(y)
Base.:(>)(x::Time, y::Time) = value(x) > value(y)

Base.:(>=)(x::Date, y::Date) = value(x) >= value(y)
Base.:(>=)(x::DateTime, y::DateTime) = value(x) >= value(y)
Base.:(>=)(x::Time, y::Time) = value(x) >= value(y)

# ============================================================================
# Arithmetic Functions
# ============================================================================

# Date/DateTime difference
Base.:-(x::Date, y::Date) = Day(value(x) - value(y))
Base.:-(x::DateTime, y::DateTime) = Millisecond(value(x) - value(y))
Base.:-(x::Time, y::Time) = Nanosecond(value(x) - value(y))

# Month wrap helpers
monthwrap(m1, m2) = mod(m1 + m2 - 1, 12) + 1
yearwrap(y, m1, m2) = y + fld(m1 + m2 - 1, 12)

# Year arithmetic for DateTime
function Base.:+(dt::DateTime, y::Year)
    oy, m, d = yearmonthday(dt)
    ny = oy + value(y)
    ld = daysinmonth(ny, m)
    return DateTime(ny, m, d <= ld ? d : ld, hour(dt), minute(dt), second(dt), millisecond(dt))
end

function Base.:-(dt::DateTime, y::Year)
    oy, m, d = yearmonthday(dt)
    ny = oy - value(y)
    ld = daysinmonth(ny, m)
    return DateTime(ny, m, d <= ld ? d : ld, hour(dt), minute(dt), second(dt), millisecond(dt))
end

# Year arithmetic for Date
function Base.:+(dt::Date, y::Year)
    oy, m, d = yearmonthday(dt)
    ny = oy + value(y)
    ld = daysinmonth(ny, m)
    return Date(ny, m, d <= ld ? d : ld)
end

function Base.:-(dt::Date, y::Year)
    oy, m, d = yearmonthday(dt)
    ny = oy - value(y)
    ld = daysinmonth(ny, m)
    return Date(ny, m, d <= ld ? d : ld)
end

# Month arithmetic for DateTime
function Base.:+(dt::DateTime, z::Month)
    y, m, d = yearmonthday(dt)
    ny = yearwrap(y, m, value(z))
    mm = monthwrap(m, value(z))
    ld = daysinmonth(ny, mm)
    return DateTime(ny, mm, d <= ld ? d : ld, hour(dt), minute(dt), second(dt), millisecond(dt))
end

function Base.:-(dt::DateTime, z::Month)
    y, m, d = yearmonthday(dt)
    ny = yearwrap(y, m, -value(z))
    mm = monthwrap(m, -value(z))
    ld = daysinmonth(ny, mm)
    return DateTime(ny, mm, d <= ld ? d : ld, hour(dt), minute(dt), second(dt), millisecond(dt))
end

# Month arithmetic for Date
function Base.:+(dt::Date, z::Month)
    y, m, d = yearmonthday(dt)
    ny = yearwrap(y, m, value(z))
    mm = monthwrap(m, value(z))
    ld = daysinmonth(ny, mm)
    return Date(ny, mm, d <= ld ? d : ld)
end

function Base.:-(dt::Date, z::Month)
    y, m, d = yearmonthday(dt)
    ny = yearwrap(y, m, -value(z))
    mm = monthwrap(m, -value(z))
    ld = daysinmonth(ny, mm)
    return Date(ny, mm, d <= ld ? d : ld)
end

# Quarter arithmetic
Base.:+(x::Date, y::Quarter) = x + Month(3 * value(y))
Base.:-(x::Date, y::Quarter) = x - Month(3 * value(y))
Base.:+(x::DateTime, y::Quarter) = x + Month(3 * value(y))
Base.:-(x::DateTime, y::Quarter) = x - Month(3 * value(y))

# Week arithmetic
Base.:+(x::Date, y::Week) = Date(value(x) + 7 * value(y))
Base.:-(x::Date, y::Week) = Date(value(x) - 7 * value(y))

# Day arithmetic for Date
Base.:+(x::Date, y::Day) = Date(value(x) + value(y))
Base.:-(x::Date, y::Day) = Date(value(x) - value(y))

# TimePeriod conversions for DateTime arithmetic
toms(c::Nanosecond) = div(value(c), 1000000)
toms(c::Microsecond) = div(value(c), 1000)
toms(c::Millisecond) = value(c)
toms(c::Second) = 1000 * value(c)
toms(c::Minute) = 60000 * value(c)
toms(c::Hour) = 3600000 * value(c)
toms(c::Day) = 86400000 * value(c)
toms(c::Week) = 604800000 * value(c)

# DateTime + TimePeriod
Base.:+(x::DateTime, y::Week) = DateTime(value(x) + toms(y))
Base.:-(x::DateTime, y::Week) = DateTime(value(x) - toms(y))
Base.:+(x::DateTime, y::Day) = DateTime(value(x) + toms(y))
Base.:-(x::DateTime, y::Day) = DateTime(value(x) - toms(y))
Base.:+(x::DateTime, y::Hour) = DateTime(value(x) + toms(y))
Base.:-(x::DateTime, y::Hour) = DateTime(value(x) - toms(y))
Base.:+(x::DateTime, y::Minute) = DateTime(value(x) + toms(y))
Base.:-(x::DateTime, y::Minute) = DateTime(value(x) - toms(y))
Base.:+(x::DateTime, y::Second) = DateTime(value(x) + toms(y))
Base.:-(x::DateTime, y::Second) = DateTime(value(x) - toms(y))
Base.:+(x::DateTime, y::Millisecond) = DateTime(value(x) + toms(y))
Base.:-(x::DateTime, y::Millisecond) = DateTime(value(x) - toms(y))

# Nanosecond conversions for Time arithmetic
tons(c::Nanosecond) = value(c)
tons(c::Microsecond) = value(c) * 1000
tons(c::Millisecond) = value(c) * 1000000
tons(c::Second) = value(c) * 1000000000
tons(c::Minute) = value(c) * 60000000000
tons(c::Hour) = value(c) * 3600000000000

# Time + TimePeriod
Base.:+(x::Time, y::Hour) = Time(mod(value(x) + tons(y), 86400000000000))
Base.:-(x::Time, y::Hour) = Time(mod(value(x) - tons(y), 86400000000000))
Base.:+(x::Time, y::Minute) = Time(mod(value(x) + tons(y), 86400000000000))
Base.:-(x::Time, y::Minute) = Time(mod(value(x) - tons(y), 86400000000000))
Base.:+(x::Time, y::Second) = Time(mod(value(x) + tons(y), 86400000000000))
Base.:-(x::Time, y::Second) = Time(mod(value(x) - tons(y), 86400000000000))
Base.:+(x::Time, y::Millisecond) = Time(mod(value(x) + tons(y), 86400000000000))
Base.:-(x::Time, y::Millisecond) = Time(mod(value(x) - tons(y), 86400000000000))
Base.:+(x::Time, y::Microsecond) = Time(mod(value(x) + tons(y), 86400000000000))
Base.:-(x::Time, y::Microsecond) = Time(mod(value(x) - tons(y), 86400000000000))
Base.:+(x::Time, y::Nanosecond) = Time(mod(value(x) + tons(y), 86400000000000))
Base.:-(x::Time, y::Nanosecond) = Time(mod(value(x) - tons(y), 86400000000000))

# Commutative Period + DateTime/Date
Base.:+(y::Year, x::DateTime) = x + y
Base.:+(y::Year, x::Date) = x + y
Base.:+(y::Month, x::DateTime) = x + y
Base.:+(y::Month, x::Date) = x + y
Base.:+(y::Quarter, x::DateTime) = x + y
Base.:+(y::Quarter, x::Date) = x + y
Base.:+(y::Week, x::DateTime) = x + y
Base.:+(y::Week, x::Date) = x + y
Base.:+(y::Day, x::DateTime) = x + y
Base.:+(y::Day, x::Date) = x + y
Base.:+(y::Hour, x::DateTime) = x + y
Base.:+(y::Minute, x::DateTime) = x + y
Base.:+(y::Second, x::DateTime) = x + y
Base.:+(y::Millisecond, x::DateTime) = x + y
Base.:+(y::Hour, x::Time) = x + y
Base.:+(y::Minute, x::Time) = x + y
Base.:+(y::Second, x::Time) = x + y
Base.:+(y::Millisecond, x::Time) = x + y
Base.:+(y::Microsecond, x::Time) = x + y
Base.:+(y::Nanosecond, x::Time) = x + y

# Date + Time = DateTime
function Base.:+(dt::Date, t::Time)
    y, m, d = yearmonthday(dt)
    return DateTime(y, m, d, hour(t), minute(t), second(t), millisecond(t))
end
Base.:+(t::Time, dt::Date) = dt + t

# ============================================================================
# Exports
# ============================================================================

export Year, Quarter, Month, Week, Day, Hour, Minute, Second, Millisecond,
       Microsecond, Nanosecond,
       DateTime, Date, Time,
       # accessors
       yearmonthday, yearmonth, monthday, year, month, week, day,
       hour, minute, second, millisecond, dayofmonth,
       microsecond, nanosecond,
       # query
       dayofweek, isleapyear, daysinmonth, daysinyear, dayofyear, dayname, dayabbr,
       dayofweekofmonth, monthname, monthabbr,
       quarterofyear, dayofquarter,
       # day constants
       Monday, Tuesday, Wednesday, Thursday, Friday, Saturday, Sunday,
       Mon, Tue, Wed, Thu, Fri, Sat, Sun,
       # month constants
       January, February, March, April, May, June,
       July, August, September, October, November, December,
       Jan, Feb, Mar, Apr, Jun, Jul, Aug, Sep, Oct, Nov, Dec,
       # functions
       value

end # module
