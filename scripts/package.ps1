# Builds a release binary and packs it for distribution.
#
#   pwsh -File scripts/package.ps1 [-Target win|linux|macos] [-SkipBuild]
#
# The result is dist/aurora-export-<version>-<target>.zip containing the
# executable, the README, the license and the icon.

param(
    [string]$Target = "win",
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
Write-Host "packaging AuroraExport v$version for $Target"

if (-not $SkipBuild) {
    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
}

$binaryName = switch ($Target) {
    "win" { "aurora-export.exe" }
    default { "aurora-export" }
}
$binary = Join-Path $root "target/release/$binaryName"
if (-not (Test-Path $binary)) { throw "binary not found: $binary" }

$dist = Join-Path $root "dist"
$stage = Join-Path $dist "aurora-export-$version-$Target"
$stagePath = [System.IO.Path]::GetFullPath($stage)
$distPath = [System.IO.Path]::GetFullPath($dist).TrimEnd([System.IO.Path]::DirectorySeparatorChar)
if (-not $stagePath.StartsWith($distPath + [System.IO.Path]::DirectorySeparatorChar, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "staging directory is outside dist: $stagePath"
}
if (Test-Path -LiteralPath $stagePath) { Remove-Item -LiteralPath $stagePath -Recurse -Force }
New-Item -ItemType Directory -Force $stage | Out-Null

Copy-Item $binary $stage
Copy-Item README.md $stage
Copy-Item LICENSE $stage
New-Item -ItemType Directory -Force (Join-Path $stage "icon") | Out-Null
Copy-Item assets/icon.png (Join-Path $stage "icon")

$size = (Get-Item (Join-Path $stage $binaryName)).Length / 1MB
Write-Host ("binary size: {0:N2} MB" -f $size)

$zip = Join-Path $dist "aurora-export-$version-$Target.zip"
if (Test-Path $zip) { Remove-Item -Force $zip }
Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zip
Write-Host "wrote $zip"
