[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string] $SignedPackage
)

$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
$root = Split-Path -Parent $PSScriptRoot
$kit = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits/10'
$sdkVersion = '10.0.26100.0'
$signTool = Join-Path $kit "bin/$sdkVersion/x64/signtool.exe"
$package = (Resolve-Path -LiteralPath $SignedPackage).Path
$inf = Join-Path $package 'MPKDeckVoice.inf'
$sys = Join-Path $package 'MPKDeckVoice.sys'
$catalog = Join-Path $package 'MPKDeckVoice.cat'

# Verify kernel-mode trust and membership of both files in the signed catalog.
foreach ($file in @($inf, $sys)) {
    & $signTool verify /kp /v /c $catalog $file
}
if ((Get-Item -LiteralPath $sys).VersionInfo.ProductVersion -ne '0.2.0.0' -or
    !(Select-String -LiteralPath $inf -SimpleMatch 'ROOT\MPKDeckVoice' -Quiet)) {
    throw 'El paquete firmado no corresponde a MPK Deck Virtual Audio 0.2.0.'
}

$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
$vs = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (!$vs) { throw 'Se requieren Visual Studio 2022 Build Tools con C++ x64.' }
$version = (Get-Content -LiteralPath (Join-Path $vs 'VC/Auxiliary/Build/Microsoft.VCToolsVersion.default.txt') -Raw).Trim()
$msvc = Join-Path $vs "VC/Tools/MSVC/$version"
$include = Join-Path $kit "Include/$sdkVersion"
$lib = Join-Path $kit "Lib/$sdkVersion"
$cl = Join-Path $msvc 'bin/Hostx64/x64/cl.exe'

$stage = Join-Path $PSScriptRoot "out/signed-$([guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Path $stage -Force | Out-Null
Copy-Item -LiteralPath $inf,$sys,$catalog,(Join-Path $PSScriptRoot 'LICENSE-MS-PL.txt') -Destination $stage
$helper = Join-Path $stage 'mpk-voice-register.exe'
& $cl /nologo /std:c++20 /EHsc /W4 /WX /O2 /MT /guard:cf /DUNICODE /D_UNICODE `
    "/I$msvc/include" "/I$include/ucrt" "/I$include/um" "/I$include/shared" `
    "/Fo$stage/register.obj" "/Fe$helper" (Join-Path $PSScriptRoot 'register.cpp') `
    /link /guard:cf "/LIBPATH:$msvc/lib/x64" "/LIBPATH:$lib/ucrt/x64" "/LIBPATH:$lib/um/x64" Setupapi.lib Newdev.lib Cfgmgr32.lib

$resources = @{}
foreach ($file in 'MPKDeckVoice.inf','MPKDeckVoice.sys','MPKDeckVoice.cat','LICENSE-MS-PL.txt','mpk-voice-register.exe') {
    $resources[(Join-Path $stage $file)] = "voice-driver/$file"
}
$override = Join-Path $stage 'tauri.signed.json'
@{
    bundle = @{
        resources = $resources
        windows = @{ nsis = @{ installMode = 'perMachine'; installerHooks = (Join-Path $PSScriptRoot 'installer-hooks.nsh') } }
    }
} | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $override -Encoding utf8

# Keep the driver bundle separate from the standard per-user development installer.
$previousTarget = $env:CARGO_TARGET_DIR
try {
    $env:CARGO_TARGET_DIR = Join-Path $stage 'tauri-target'
    Push-Location $root
    try { npm run tauri build -- --ci --bundles nsis --config $override -- --locked }
    finally { Pop-Location }
} finally {
    $env:CARGO_TARGET_DIR = $previousTarget
}
