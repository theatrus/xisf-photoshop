[CmdletBinding()]
param(
    [string]$PhotoshopSdk = $env:PHOTOSHOP_SDK,
    [switch]$BackendOnly
)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot -Parent
Push-Location $projectRoot
try {
    if (-not $BackendOnly -and (-not $PhotoshopSdk -or -not (Test-Path -LiteralPath $PhotoshopSdk))) {
        throw 'Supply -PhotoshopSdk <extracted Adobe Photoshop C++ SDK folder> or set PHOTOSHOP_SDK. Use -BackendOnly to test/build the Rust library without the SDK.'
    }
    $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
    $vs = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if (-not $vs) { throw 'Install Visual Studio C++ build tools and a Windows SDK.' }
    & (Join-Path $vs 'Common7\Tools\Launch-VsDevShell.ps1') -Arch amd64 -HostArch amd64 -SkipAutomaticLocation | Out-Null

    cargo fmt --check
    if ($LASTEXITCODE) { throw 'Rust formatting check failed' }
    cargo test --locked
    if ($LASTEXITCODE) { throw 'Rust tests failed' }
    cargo build --release --locked --target x86_64-pc-windows-msvc
    if ($LASTEXITCODE) { throw 'Rust release build failed' }
    New-Item -ItemType Directory -Force build\native | Out-Null
    $rustLib = Join-Path $projectRoot 'target\x86_64-pc-windows-msvc\release\seiza_photoshop.lib'
    & cl.exe /nologo /std:c++17 /EHsc /O2 /MD native\codec_smoke.cpp /Fobuild\native\codec_smoke.obj /Febuild\native\codec_smoke.exe /link $rustLib ws2_32.lib userenv.lib bcrypt.lib ntdll.lib advapi32.lib
    if ($LASTEXITCODE) { throw 'C++/Rust ABI smoke test compilation failed' }
    & .\build\native\codec_smoke.exe
    if ($LASTEXITCODE) { throw 'C++/Rust ABI smoke test failed' }
    if ($BackendOnly) { return }

    $PhotoshopSdk = (Resolve-Path -LiteralPath $PhotoshopSdk).Path
    $header = @(Get-ChildItem -LiteralPath $PhotoshopSdk -Recurse -Filter PIFormat.h)
    if ($header.Count -ne 1) { throw 'SDK folder must contain exactly one photoshopapi/Photoshop/PIFormat.h' }
    $apiRoot = Split-Path $header[0].DirectoryName -Parent
    $sdkRoot = Split-Path $apiRoot -Parent
    $includeDirs = @('Photoshop','PICA_SP','Resources') | ForEach-Object { Join-Path $apiRoot $_ }
    $includeDirs += Join-Path $sdkRoot 'samplecode\common\includes'
    $includeDirs += Join-Path $sdkRoot 'samplecode\common\resources'
    $includeArgs = $includeDirs | ForEach-Object { '/I' + $_ }
    $converter = @(Get-ChildItem -LiteralPath $sdkRoot -Recurse -Filter cnvtpipl.exe)
    if ($converter.Count -eq 0) { throw 'SDK cnvtpipl.exe resource compiler was not found.' }
    $converterPath = $converter[0].FullName
    New-Item -ItemType Directory -Force build\native,dist | Out-Null
    & rc.exe /nologo /fobuild\native\options.res native\options.rc
    if ($LASTEXITCODE) { throw 'Windows options dialog compilation failed' }
    foreach ($format in @(@{Name='FITS'; Id=1}, @{Name='XISF'; Id=2})) {
        $name = 'Seiza' + $format.Name
        $base = Join-Path $projectRoot "build\native\$name"
        $preprocessed = & cl.exe /nologo /EP /DMSWindows=1 "/DSEIZA_FORMAT=$($format.Id)" @includeArgs /Tcnative\Seiza.r
        if ($LASTEXITCODE) { throw "Resource preprocessing failed: $name" }
        [IO.File]::WriteAllLines("$base.rr", [string[]]$preprocessed, [Text.Encoding]::ASCII)
        & $converterPath "$base.rr" "$base.rc"
        if ($LASTEXITCODE) { throw "PiPL compilation failed: $name" }
        & rc.exe /nologo "/fo$base.res" "$base.rc"
        if ($LASTEXITCODE) { throw "Windows resource compilation failed: $name" }
        & cl.exe /nologo /std:c++17 /EHsc /O2 /MD /W4 /LD /DWIN32=1 /DMSWindows=1 "/DSEIZA_FORMAT=$($format.Id)" @includeArgs native\plugin.cpp "/Fo$base.obj" /link "$base.res" build\native\options.res $rustLib ws2_32.lib userenv.lib bcrypt.lib ntdll.lib advapi32.lib user32.lib "/OUT:dist\$name.8bi" "/IMPLIB:$base.lib"
        if ($LASTEXITCODE) { throw "Plug-in compilation failed: $name" }
    }
    # MSVC can hit C1001 optimizing this large test harness; shipped plugins stay /O2.
    & cl.exe /nologo /std:c++17 /EHsc /Od /MD /DWIN32=1 /DMSWindows=1 @includeArgs native\host_smoke.cpp /Fobuild\native\host_smoke.obj /Febuild\native\host_smoke.exe /link $rustLib ws2_32.lib userenv.lib bcrypt.lib ntdll.lib advapi32.lib
    if ($LASTEXITCODE) { throw 'Adobe SDK host harness compilation failed' }
    & .\build\native\host_smoke.exe
    if ($LASTEXITCODE) { throw 'Adobe SDK host harness failed' }
    Copy-Item README.md,NOTICE,LICENSE -Destination dist
    Compress-Archive -Path dist\*.8bi,dist\README.md,dist\NOTICE,dist\LICENSE -DestinationPath dist\Seiza-Photoshop-Windows-x64.zip -Force
    Write-Host 'Built dist\SeizaFITS.8bi and dist\SeizaXISF.8bi'
} finally { Pop-Location }
