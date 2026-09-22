[CmdletBinding()]
param([string]$Compiler, [string]$TestRoot)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
if (-not $Compiler) { $Compiler = & "$PSScriptRoot/get-inno.ps1" }
$versionLine = Select-String -LiteralPath (Join-Path $root 'Cargo.toml') -Pattern '^version = "([0-9]+\.[0-9]+\.[0-9]+)"$' | Select-Object -First 1
if (-not $versionLine) { throw 'Cargo package version is missing' }
$version = $versionLine.Matches[0].Groups[1].Value
foreach ($name in 'SeizaFITS.8bi','SeizaXISF.8bi') {
    if (-not (Test-Path -LiteralPath (Join-Path $root "dist/$name"))) { throw "Build $name first" }
}
$output = Join-Path $root 'dist'
$defines = @("/DPluginVersion=$version", "/DSourceRoot=$root")
if ($TestRoot) {
    $TestRoot = [IO.Path]::GetFullPath($TestRoot)
    $testBase = [IO.Path]::GetFullPath((Join-Path $root 'build/installer-tests')) + [IO.Path]::DirectorySeparatorChar
    if (-not $TestRoot.StartsWith($testBase, [StringComparison]::OrdinalIgnoreCase)) { throw 'Installer tests must stay under build/installer-tests' }
    $output = Join-Path $TestRoot 'Output'
    $defines += "/DTestRoot=$TestRoot"
}
New-Item -ItemType Directory -Force -Path $output | Out-Null
& $Compiler @defines "/DOutputRoot=$output" (Join-Path $root 'installer/windows.iss')
if ($LASTEXITCODE) { throw 'Windows installer compilation failed' }
if (-not $TestRoot) {
    $installer = Join-Path $output "Seiza-Photoshop-Windows-x64-Setup-$version.exe"
    $hash = (Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash.ToLowerInvariant()
    [IO.File]::WriteAllText("$installer.sha256", "$hash  $([IO.Path]::GetFileName($installer))`n")
    Write-Host "Built $installer"
}
