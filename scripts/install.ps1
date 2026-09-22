[CmdletBinding()]
param([Parameter(Mandatory)][string]$PluginDirectory)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
if (Get-Process Photoshop -ErrorAction SilentlyContinue) {
    throw 'Close Photoshop before installing the plug-ins.'
}
foreach ($name in 'SeizaFITS.8bi','SeizaXISF.8bi') {
    if (-not (Test-Path -LiteralPath (Join-Path $root "dist\$name"))) { throw "Build $name first using scripts/build.ps1" }
}
$destination = Join-Path $PluginDirectory 'Seiza'
New-Item -ItemType Directory -Force -Path $destination | Out-Null
foreach ($name in 'SeizaFITS.8bi','SeizaXISF.8bi') {
    $target = Join-Path $destination $name
    if (Test-Path -LiteralPath $target) { Copy-Item -LiteralPath $target -Destination "$target.bak" -Force }
    Copy-Item -LiteralPath (Join-Path $root "dist\$name") -Destination $target -Force
}
Write-Host "Installed in $destination. Start Photoshop to load the formats."
