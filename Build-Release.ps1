$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

$cargoToml = Get-Content -Path (Join-Path $PSScriptRoot 'Cargo.toml') -Raw
if ($cargoToml -match '(?m)^version\s*=\s*"([^"]+)"') {
    $Version = $Matches[1]
} else {
    throw 'Could not determine package version from Cargo.toml'
}

Write-Host "=== Building Switcheroo v$Version (100% Native Rust) ===" -ForegroundColor Cyan

# 1. Build main Switcheroo executable
cargo build --release --bin Switcheroo
if ($LASTEXITCODE -ne 0) { throw 'cargo build --release --bin Switcheroo failed' }

$AppExe = Join-Path $PSScriptRoot 'target\release\Switcheroo.exe'
if (-not (Test-Path $AppExe)) { throw "Missing $AppExe" }

# 2. Gzip-compress Switcheroo.exe for embedding inside Switcheroo-Setup.exe
$PayloadGz = Join-Path $PSScriptRoot 'target\Switcheroo.payload.gz'
$inStream = [System.IO.File]::OpenRead($AppExe)
try {
    $outStream = [System.IO.File]::Create($PayloadGz)
    try {
        $gzStream = New-Object System.IO.Compression.GZipStream(
            $outStream,
            [System.IO.Compression.CompressionLevel]::Optimal
        )
        try {
            $inStream.CopyTo($gzStream)
        } finally {
            $gzStream.Dispose()
        }
    } finally {
        $outStream.Dispose()
    }
} finally {
    $inStream.Dispose()
}

# 3. Build native Rust installer with embedded payload
$env:SWITCHEROO_PAYLOAD_GZ = $PayloadGz
try {
    cargo build --release --bin Switcheroo-Setup
    if ($LASTEXITCODE -ne 0) { throw 'cargo build --release --bin Switcheroo-Setup failed' }
} finally {
    Remove-Item Env:\SWITCHEROO_PAYLOAD_GZ -ErrorAction SilentlyContinue
}

$SetupExe = Join-Path $PSScriptRoot 'target\release\Switcheroo-Setup.exe'
if (-not (Test-Path $SetupExe)) { throw "Missing $SetupExe" }

# 4. Assemble clean dist/ directory
$DistDir = Join-Path $PSScriptRoot 'dist'
if (Test-Path $DistDir) {
    Remove-Item -Path $DistDir -Recurse -Force
}
New-Item -ItemType Directory -Path $DistDir | Out-Null

$OutSetup = Join-Path $DistDir "Switcheroo-Setup-v$Version.exe"
$OutPortable = Join-Path $DistDir "Switcheroo-Portable-v$Version.exe"
$OutZip = Join-Path $DistDir "Switcheroo-v$Version-win64.zip"

Copy-Item -Path $SetupExe -Destination $OutSetup -Force
Copy-Item -Path $AppExe -Destination $OutPortable -Force

$StageZip = Join-Path $PSScriptRoot 'target\zip_stage'
if (Test-Path $StageZip) {
    Remove-Item -Path $StageZip -Recurse -Force
}
New-Item -ItemType Directory -Path $StageZip | Out-Null
Copy-Item -Path $AppExe -Destination (Join-Path $StageZip 'Switcheroo.exe') -Force
Copy-Item -Path (Join-Path $PSScriptRoot 'README.md') -Destination (Join-Path $StageZip 'README.md') -Force
if (Test-Path (Join-Path $PSScriptRoot 'LICENSE')) {
    Copy-Item -Path (Join-Path $PSScriptRoot 'LICENSE') -Destination (Join-Path $StageZip 'LICENSE') -Force
}
Compress-Archive -Path (Join-Path $StageZip '*') -DestinationPath $OutZip -CompressionLevel Optimal -Force
Remove-Item -Path $StageZip -Recurse -Force

# 5. Generate SHA256SUMS.txt & print VirusTotal links
$Artifacts = @($OutSetup, $OutPortable, $OutZip)
$SumLines = @()
Write-Host ""
Write-Host "=== Release Artifacts & VirusTotal (by Google) Links ===" -ForegroundColor Green
foreach ($f in $Artifacts) {
    $hash = (Get-FileHash -Path $f -Algorithm SHA256).Hash.ToLowerInvariant()
    $name = [System.IO.Path]::GetFileName($f)
    $sizeMb = [Math]::Round((Get-Item $f).Length / 1MB, 2)
    $SumLines += "$hash  $name"
    Write-Host ("{0} ({1:N2} MB)`n  SHA-256:    {2}`n  VirusTotal: https://www.virustotal.com/gui/file/{2}" -f $name, $sizeMb, $hash)
}

$SumsFile = Join-Path $DistDir 'SHA256SUMS.txt'
Set-Content -Path $SumsFile -Value ($SumLines -join "`r`n") -Encoding ASCII
Write-Host ""
Write-Host "Checksums written to: $SumsFile" -ForegroundColor Cyan
