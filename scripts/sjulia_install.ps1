# Install sjulia with embedded Base bytecode and prelude Program caches.
#
# Usage:
#   pwsh -File scripts/sjulia_install.ps1
#   pwsh -File scripts/sjulia_install.ps1 --force-cache
#
# Can be run from any directory. Requires a Rust toolchain.

[CmdletBinding()]
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]] $Arguments
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$forceCache = $false
foreach ($argument in $Arguments) {
    if ($argument -eq "--force-cache") {
        $forceCache = $true
    }
    else {
        [Console]::Error.WriteLine("ERROR: unknown argument: $argument")
        [Console]::Error.WriteLine("Usage: $($MyInvocation.MyCommand.Path) [--force-cache]")
        exit 1
    }
}

$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Set-Location $root

if ([string]::IsNullOrWhiteSpace($env:CARGO_TARGET_DIR)) {
    $env:CARGO_TARGET_DIR = Join-Path $root "target"
}
elseif (-not [System.IO.Path]::IsPathRooted($env:CARGO_TARGET_DIR)) {
    # Match the shell installer: relative target directories are repository-relative.
    $env:CARGO_TARGET_DIR = [System.IO.Path]::GetFullPath(
        (Join-Path $root $env:CARGO_TARGET_DIR)
    )
}

$sjuliaBin = Join-Path $env:CARGO_TARGET_DIR "release\sjulia.exe"
$baseCache = Join-Path $env:CARGO_TARGET_DIR "base_cache.bin"
$preludeProgramCache = Join-Path $env:CARGO_TARGET_DIR "prelude_program_cache.bin"
$preludeDir = Join-Path $root "subset_julia_vm\src\julia"

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    [Console]::Error.WriteLine("ERROR: cargo not found. Install Rust: https://rustup.rs/")
    exit 1
}

function Invoke-Cargo {
    param([Parameter(Mandatory = $true)][string[]] $CargoArguments)

    & cargo @CargoArguments
    if ($LASTEXITCODE -ne 0) {
        throw "cargo exited with status $LASTEXITCODE"
    }
}

function Test-CacheNeedsRegeneration {
    param([Parameter(Mandatory = $true)][string] $CachePath)

    if ($forceCache -or -not (Test-Path -LiteralPath $CachePath -PathType Leaf)) {
        return $true
    }

    $cacheTimestamp = (Get-Item -LiteralPath $CachePath).LastWriteTimeUtc
    if ((Get-Item -LiteralPath $sjuliaBin).LastWriteTimeUtc -gt $cacheTimestamp) {
        return $true
    }

    return [bool](Get-ChildItem -LiteralPath $preludeDir -Recurse -File | Where-Object {
        $_.LastWriteTimeUtc -gt $cacheTimestamp
    } | Select-Object -First 1)
}

Write-Host "== [1/4] build host sjulia =="
# Do NOT set SJULIA_BASE_CACHE here -- this build is what generates the caches.
Invoke-Cargo -CargoArguments @(
    "build", "--release", "-p", "subset_julia_vm", "--bin", "sjulia", "--features", "repl"
)
if (-not (Test-Path -LiteralPath $sjuliaBin -PathType Leaf)) {
    throw "ERROR: missing sjulia binary: $sjuliaBin"
}

Write-Host "== [2/4] generate prelude Program cache =="
New-Item -ItemType Directory -Force -Path $env:CARGO_TARGET_DIR | Out-Null
if (Test-CacheNeedsRegeneration $preludeProgramCache) {
    & $sjuliaBin --precompile-prelude $preludeProgramCache
    if ($LASTEXITCODE -ne 0) {
        throw "sjulia --precompile-prelude exited with status $LASTEXITCODE"
    }
}
else {
    Write-Host "Prelude Program cache is up-to-date; skipping regeneration ($preludeProgramCache)"
}
if (-not (Test-Path -LiteralPath $preludeProgramCache -PathType Leaf)) {
    throw "ERROR: missing prelude Program cache: $preludeProgramCache"
}

Write-Host "== [3/4] generate Base bytecode cache =="
if (Test-CacheNeedsRegeneration $baseCache) {
    & $sjuliaBin --precompile-base $baseCache
    if ($LASTEXITCODE -ne 0) {
        throw "sjulia --precompile-base exited with status $LASTEXITCODE"
    }
}
else {
    Write-Host "Base cache is up-to-date; skipping regeneration ($baseCache)"
}
if (-not (Test-Path -LiteralPath $baseCache -PathType Leaf)) {
    throw "ERROR: missing Base cache: $baseCache"
}

Write-Host "== [4/4] cargo install with embedded caches =="
$oldBaseCache = $env:SJULIA_BASE_CACHE
$oldPreludeProgramCache = $env:SJULIA_PRELUDE_PROGRAM_CACHE
try {
    $env:SJULIA_BASE_CACHE = $baseCache
    $env:SJULIA_PRELUDE_PROGRAM_CACHE = $preludeProgramCache
    Invoke-Cargo -CargoArguments @(
        "install", "--force", "--bin", "sjulia", "--path", "subset_julia_vm", "--features", "repl"
    )
}
finally {
    $env:SJULIA_BASE_CACHE = $oldBaseCache
    $env:SJULIA_PRELUDE_PROGRAM_CACHE = $oldPreludeProgramCache
}

Write-Host "== sjulia installed successfully =="
Write-Host "Default binary location: $env:USERPROFILE\.cargo\bin\sjulia.exe (override with CARGO_INSTALL_ROOT)"
