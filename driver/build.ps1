[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
$revision = 'f4c1ff842b6a0001152f44cb61788b216a1c7627'
$sdkVersion = '10.0.26100.0'
$kmdfVersion = '1.31'
$kit = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits/10'
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
$vs = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (!$vs) { throw 'Se requieren Visual Studio 2022 Build Tools con C++ x64.' }
$msvcVersion = (Get-Content -LiteralPath (Join-Path $vs 'VC/Auxiliary/Build/Microsoft.VCToolsVersion.default.txt') -Raw).Trim()
$msvc = Join-Path $vs "VC/Tools/MSVC/$msvcVersion"
$kitInclude = Join-Path $kit "Include/$sdkVersion"
$km = Join-Path $kit "Lib/$sdkVersion/km/x64"
$wdf = Join-Path $kit "Lib/wdf/kmdf/x64/$kmdfVersion"
$bin = Join-Path $kit "bin/$sdkVersion"
$cl = Join-Path $msvc 'bin/Hostx64/x64/cl.exe'
$link = Join-Path $msvc 'bin/Hostx64/x64/link.exe'
$rc = Join-Path $bin 'x64/rc.exe'
$stamp = Join-Path $bin 'x64/stampinf.exe'
$catalog = Join-Path $bin 'x86/Inf2Cat.exe'
foreach ($required in @($cl, $link, $rc, $stamp, $catalog, (Join-Path $km 'portcls.lib'), (Join-Path $wdf 'WdfDriverEntry.lib'))) {
    if (!(Test-Path -LiteralPath $required -PathType Leaf)) { throw "Falta un componente de Visual Studio, SDK o WDK: $required" }
}

$out = Join-Path $PSScriptRoot 'out'
$checkout = Join-Path $out "source-$([guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Path $checkout -Force | Out-Null
git -C $checkout init --quiet
git -C $checkout remote add origin https://github.com/microsoft/Windows-driver-samples.git
git -C $checkout sparse-checkout set audio/simpleaudiosample
git -C $checkout -c protocol.version=2 fetch --quiet --depth 1 --filter=blob:none origin $revision
git -C $checkout -c core.autocrlf=false checkout --quiet --detach FETCH_HEAD
git -C $checkout apply --whitespace=error --check (Join-Path $PSScriptRoot 'sysvad-voice.patch')
git -C $checkout apply --whitespace=error (Join-Path $PSScriptRoot 'sysvad-voice.patch')
$source = Join-Path $checkout 'audio/simpleaudiosample/Source'
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'voice-bridge.h') -Destination (Join-Path $source 'Inc')

$package = Join-Path $checkout 'unsigned'
$objects = Join-Path $checkout 'objects'
New-Item -ItemType Directory -Path $package,$objects | Out-Null
$includes = @(
    (Join-Path $kitInclude 'km/crt'),
    (Join-Path $kitInclude 'km'),
    (Join-Path $kitInclude 'shared'),
    (Join-Path $msvc 'include'),
    (Join-Path $kit "Include/wdf/kmdf/$kmdfVersion"),
    (Join-Path $source 'Inc'),
    (Join-Path $source 'Main'),
    (Join-Path $source 'Utilities'),
    (Join-Path $source 'Filters')
) | ForEach-Object { "/I$_" }
$definitions = '/D_WIN32','/D_WIN64','/D_AMD64_','/DAMD64','/DUNICODE','/D_UNICODE','/DPC_IMPLEMENTATION',
    '/D_NEW_DELETE_OPERATORS_','/D_WIN32_WINNT=0x0A00','/DNTDDI_VERSION=0x0A000008','/DKMDF_VERSION_MAJOR=1','/DKMDF_VERSION_MINOR=31'
$flags = '/nologo','/c','/kernel','/O2','/Z7','/GS','/guard:cf','/Zc:wchar_t','/Zp8','/GR-','/W4','/WX','/wd4324','/std:c++17'
$files = Get-ChildItem -Path "$source/Main/*.cpp","$source/Utilities/*.cpp","$source/Filters/*.cpp" |
    Where-Object Name -NotIn @('savedata.cpp', 'ToneGenerator.cpp') | Sort-Object Name
$objectFiles = foreach ($file in $files) {
    $object = Join-Path $objects "$($file.BaseName).obj"
    & $cl @flags @definitions @includes "/Fo$object" $file.FullName | Write-Host
    $object
}
$res = Join-Path $objects 'MPKDeckVoice.res'
& $rc /nologo "/I$(Join-Path $kitInclude 'um')" "/I$(Join-Path $kitInclude 'shared')" "/fo$res" (Join-Path $source 'Main/SimpleAudioSample.rc')

$libs = @('ntoskrnl','hal','wmilib','wdmsec','portcls','stdunk','libcntpr','BufferOverflowFastFailK') |
    ForEach-Object { Join-Path $km "$_.lib" }
$libs += @('WdfLdr','WdfDriverEntry') | ForEach-Object { Join-Path $wdf "$_.lib" }
$sys = Join-Path $package 'MPKDeckVoice.sys'
$pdb = Join-Path $checkout 'MPKDeckVoice.pdb'
& $link /nologo /kernel /driver /subsystem:native,10.0 /machine:x64 /entry:FxDriverEntry /nodefaultlib /guard:cf /dynamicbase /nxcompat `
    /debug:full /opt:ref /opt:icf /pdbaltpath:MPKDeckVoice.pdb "/pdb:$pdb" "/out:$sys" @objectFiles $res @libs

$inf = Join-Path $package 'MPKDeckVoice.inf'
Copy-Item -LiteralPath (Join-Path $source 'Main/SimpleAudioSample.inx') -Destination $inf
& $stamp -f $inf -a amd64 -k $kmdfVersion -d '11/05/2026' -v '0.2.0.0'
& $catalog "/driver:$package" /os:10_VB_X64,10_CO_X64,10_NI_X64,10_GE_X64
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'LICENSE-MS-PL.txt') -Destination $package

# Partner Center attestation submissions use a CAB with the driver and symbols in a subfolder.
$ddf = Join-Path $checkout 'submission.ddf'
@'
.OPTION EXPLICIT
.Set CabinetNameTemplate=MPKDeckVoice-x64.cab
.Set DiskDirectoryTemplate=.
.Set Cabinet=on
.Set Compress=on
.Set DestinationDir=MPKDeckVoice
"unsigned\MPKDeckVoice.inf"
"unsigned\MPKDeckVoice.sys"
"unsigned\MPKDeckVoice.cat"
"MPKDeckVoice.pdb"
'@ | Set-Content -LiteralPath $ddf -Encoding ascii
Push-Location $checkout
try { & (Join-Path $env:SystemRoot 'System32/makecab.exe') /f $ddf }
finally { Pop-Location }
Write-Host "Paquete de desarrollo sin firmar: $package"
Write-Host "CAB de atestación pendiente de firma EV y envío: $(Join-Path $checkout 'MPKDeckVoice-x64.cab')"
