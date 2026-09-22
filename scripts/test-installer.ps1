[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$testRoot = Join-Path $root ('build/installer-tests/' + [guid]::NewGuid().ToString('N'))
$legacy = Join-Path $testRoot 'Adobe/Adobe Photoshop Test/Plug-ins'
$shared = Join-Path $testRoot 'Shared/Seiza'
New-Item -ItemType Directory -Force -Path $legacy, $shared | Out-Null
$fits = Join-Path $legacy 'SeizaFITS.8bi'
$xisf = Join-Path $legacy 'SeizaXISF.8bi'
[IO.File]::WriteAllText($fits, 'old FITS')
[IO.File]::WriteAllText($xisf, 'old XISF')
$sentinel = Join-Path $shared 'Unrelated.8bi'
[IO.File]::WriteAllText($sentinel, 'keep me')
& "$PSScriptRoot/build-installer.ps1" -TestRoot $testRoot
$setup = Join-Path $testRoot 'Output/InstallerTest.exe'
$uninstall = Join-Path $testRoot 'App/unins000.exe'
function Assert($condition, [string]$message) { if (-not $condition) { throw $message } }
function Run-Installer([string]$exe, [string]$stage, [bool]$success) {
    $process = Start-Process -FilePath $exe -WindowStyle Hidden -PassThru -ArgumentList @(
        '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', ('/LOG="{0}"' -f (Join-Path $testRoot "$stage.log"))
    )
    if (-not $process.WaitForExit(60000)) { $process.Kill(); throw "$stage timed out" }
    Assert (($process.ExitCode -eq 0) -eq $success) "$stage returned $($process.ExitCode); see $testRoot/$stage.log"
}
function Assert-Installed {
    foreach ($name in 'SeizaFITS.8bi', 'SeizaXISF.8bi') {
        Assert (Test-Path -LiteralPath (Join-Path $shared $name)) "Missing installed $name"
        Assert ((Get-FileHash (Join-Path $shared $name)).Hash -eq (Get-FileHash (Join-Path $root "dist/$name")).Hash) "Incorrect installed $name"
    }
    Assert ([IO.File]::ReadAllText($sentinel) -eq 'keep me') 'Unrelated plugin changed'
}
$helper = $null
$lock = $null
try {
    # A denied delete after the first migration must restore the first file.
    $lock = [IO.File]::Open($xisf, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    Run-Installer $setup 'rollback' $false
    $lock.Dispose(); $lock = $null
    Assert ([IO.File]::ReadAllText($fits) -eq 'old FITS') 'Rollback did not restore FITS'
    Assert ([IO.File]::ReadAllText($xisf) -eq 'old XISF') 'Rollback changed XISF'
    Assert (-not (Test-Path (Join-Path $shared 'SeizaFITS.8bi'))) 'Failed install left an active plugin'

    # Test process checks without closing the real Photoshop application.
    $source = Join-Path $testRoot 'Host.cs'
    $hostExe = Join-Path $testRoot 'SeizaInstallerTestHost.exe'
    [IO.File]::WriteAllText($source, 'class Host { static void Main() { System.Threading.Thread.Sleep(600000); } }')
    & "$env:WINDIR/Microsoft.NET/Framework64/v4.0.30319/csc.exe" /nologo "/out:$hostExe" $source
    if ($LASTEXITCODE) { throw 'Cannot compile installer test host' }
    $helper = Start-Process -FilePath $hostExe -WindowStyle Hidden -PassThru
    Run-Installer $setup 'blocked-install' $false
    Assert (Test-Path $fits) 'Blocked install changed legacy plugins'
    Stop-Process -Id $helper.Id; $helper.WaitForExit(); $helper = $null

    Run-Installer $setup 'install' $true
    Assert-Installed
    Assert (-not (Test-Path $fits)) 'Legacy FITS remains active'
    Assert (-not (Test-Path $xisf)) 'Legacy XISF remains active'
    $backups = @(Get-ChildItem (Join-Path $testRoot 'Backups') -Recurse -Filter '*.8bi')
    Assert ($backups.Count -ge 2) 'Missing legacy backups'
    Assert (@($backups | Where-Object { [IO.File]::ReadAllText($_.FullName) -eq 'old FITS' }).Count -gt 0) 'FITS backup changed'
    Assert (@($backups | Where-Object { [IO.File]::ReadAllText($_.FullName) -eq 'old XISF' }).Count -gt 0) 'XISF backup changed'

    Run-Installer $setup 'upgrade' $true
    Assert-Installed
    $helper = Start-Process -FilePath $hostExe -WindowStyle Hidden -PassThru
    Run-Installer $setup 'blocked-upgrade' $false
    Run-Installer $uninstall 'blocked-uninstall' $false
    Assert-Installed
    Stop-Process -Id $helper.Id; $helper.WaitForExit(); $helper = $null

    Run-Installer $uninstall 'uninstall' $true
    Assert (-not (Test-Path (Join-Path $shared 'SeizaFITS.8bi'))) 'Uninstall left FITS'
    Assert (-not (Test-Path (Join-Path $shared 'SeizaXISF.8bi'))) 'Uninstall left XISF'
    Assert ([IO.File]::ReadAllText($sentinel) -eq 'keep me') 'Uninstall removed unrelated plugin'
    foreach ($backup in $backups) { Assert (Test-Path -LiteralPath $backup.FullName) 'Uninstall removed backup' }
    Assert (-not (Test-Path 'HKCU:/Software/Microsoft/Windows/CurrentVersion/Uninstall/Seiza.Photoshop.Formats.InstallerTest_is1')) 'Uninstall left app registration'
    Write-Host "Installer tests passed: rollback, process blocking, migration, upgrade, uninstall. Logs: $testRoot"
} finally {
    if ($lock) { $lock.Dispose() }
    if ($helper -and -not $helper.HasExited) { Stop-Process -Id $helper.Id }
}
