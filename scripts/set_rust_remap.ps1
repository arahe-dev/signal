param(
    [string]$RepoRoot
)

if (-not $RepoRoot) {
    $RepoRoot = Split-Path -Parent $PSScriptRoot
}

$prefixes = @()
$resolvedRepo = (Resolve-Path -LiteralPath $RepoRoot).Path
$prefixes += [pscustomobject]@{ From = $resolvedRepo; To = "<workspace>" }

if ($env:USERPROFILE) {
    $prefixes += [pscustomobject]@{ From = $env:USERPROFILE; To = "<home>" }
}

$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } elseif ($env:USERPROFILE) { Join-Path $env:USERPROFILE ".cargo" } else { $null }
if ($cargoHome -and (Test-Path $cargoHome)) {
    $prefixes += [pscustomobject]@{ From = (Resolve-Path -LiteralPath $cargoHome).Path; To = "<cargo-home>" }
}

$rustupHome = if ($env:RUSTUP_HOME) { $env:RUSTUP_HOME } elseif ($env:USERPROFILE) { Join-Path $env:USERPROFILE ".rustup" } else { $null }
if ($rustupHome -and (Test-Path $rustupHome)) {
    $prefixes += [pscustomobject]@{ From = (Resolve-Path -LiteralPath $rustupHome).Path; To = "<rustup-home>" }
}

$flags = @()
foreach ($prefix in ($prefixes | Sort-Object From -Descending -Unique)) {
    $flags += "--remap-path-prefix=$($prefix.From)=$($prefix.To)"
}

$existing = @()
if (-not [string]::IsNullOrWhiteSpace($env:RUSTFLAGS)) {
    $existing = $env:RUSTFLAGS -split "\s+"
}

$env:RUSTFLAGS = (@($existing) + $flags | Where-Object { $_ } | Select-Object -Unique) -join " "
Write-Host "Rust path remapping enabled for release artifacts."
