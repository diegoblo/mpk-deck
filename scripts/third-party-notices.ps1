$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$manifest = Join-Path $root 'src-tauri/Cargo.toml'
$metadata = cargo metadata --manifest-path $manifest --locked --format-version 1 --filter-platform x86_64-pc-windows-msvc | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw 'No se pudieron resolver las licencias de Cargo.' }

$nodes = @{}
foreach ($node in $metadata.resolve.nodes) { $nodes[$node.id] = $node }
$stack = [Collections.Generic.Stack[string]]::new()
$used = [Collections.Generic.HashSet[string]]::new()
$stack.Push($metadata.resolve.root)
while ($stack.Count) {
    $id = $stack.Pop()
    if (!$used.Add($id)) { continue }
    foreach ($dep in $nodes[$id].deps) {
        if ($dep.dep_kinds | Where-Object { $null -eq $_.kind }) { $stack.Push($dep.pkg) }
    }
}

$upstream = @{
    'alloc-stdlib@0.3.0' = 'https://raw.githubusercontent.com/dropbox/rust-alloc-no-stdlib/0a81fd6928ea3b33c8cd484aa4575d50ffb98012/LICENSE'
    'dasp_sample@0.11.0' = 'https://raw.githubusercontent.com/RustAudio/dasp/97c3bb9b2363c0b46ac1633858bf1054fd02a980/LICENSE-MIT'
    'webview2-com@0.39.1' = 'https://raw.githubusercontent.com/wravery/webview2-rs/edc2caf886175ccaebe86078c9cfe1ae2a187328/LICENSE'
    'webview2-com-sys@0.39.1' = 'https://raw.githubusercontent.com/wravery/webview2-rs/edc2caf886175ccaebe86078c9cfe1ae2a187328/LICENSE'
    'webview2-com-macros@0.8.1' = 'https://raw.githubusercontent.com/wravery/webview2-rs/dffa41a8a46d3f5565eefbff2de57d38d399f158/LICENSE'
}
$pattern = '^(license|licence|copying|notice|copyright|unlicense|blessing)($|[._-])'
$notices = [Collections.Generic.List[string]]::new()
$index = [Collections.Generic.List[string]]::new()
function Add-Notice([string] $value) {
    $value = $value.Replace("`r`n", "`n").Trim()
    $position = $notices.IndexOf($value)
    if ($position -lt 0) {
        $position = $notices.Count
        $notices.Add($value)
    }
    return $position + 1
}

$packages = $metadata.packages | Where-Object { $_.id -ne $metadata.resolve.root -and $used.Contains($_.id) } | Sort-Object name, version
foreach ($package in $packages) {
    $key = "$($package.name)@$($package.version)"
    $files = Get-ChildItem -LiteralPath (Split-Path $package.manifest_path) -File | Where-Object Name -Match $pattern | Sort-Object Name
    $blocks = @($files | ForEach-Object { Add-Notice (Get-Content -LiteralPath $_.FullName -Raw -Encoding utf8) })
    if ($upstream.ContainsKey($key)) { $blocks += Add-Notice (Invoke-WebRequest $upstream[$key]).Content }
    if (!$blocks.Count -and $package.license -ne 'MPL-2.0') { throw "Falta revisar la licencia de $key." }
    $source = "https://static.crates.io/crates/$($package.name)/$($package.name)-$($package.version).crate"
    $index.Add("$key | $($package.license) | [$($blocks -join ', ')] | $source")
}
foreach ($name in 'react','react-dom','scheduler','@tauri-apps/api') {
    $dir = Join-Path $root "node_modules/$name"
    $package = Get-Content -LiteralPath (Join-Path $dir 'package.json') -Raw | ConvertFrom-Json
    $blocks = Get-ChildItem -LiteralPath $dir -File | Where-Object Name -Match $pattern | Sort-Object Name | ForEach-Object {
        Add-Notice (Get-Content -LiteralPath $_.FullName -Raw -Encoding utf8)
    }
    $index.Add("$name@$($package.version) | $($package.license) | [$($blocks -join ', ')] | https://www.npmjs.com/package/$name/v/$($package.version)")
}
$header = @"
MPK Deck — third-party notices for Windows x64

The exact source for unmodified MPL-2.0 crates is available from their listed
.crate archives. MPL-2.0 terms are at https://www.mozilla.org/MPL/2.0/ and
included below. All Rust source archives are also listed for attribution.
WebView2 Runtime is distributed by Microsoft under its own terms:
https://developer.microsoft.com/microsoft-edge/webview2/
MPK Deck does not include any third-party virtual audio driver.

PACKAGES (name | declared license | notice references | source)
"@
$sections = for ($i = 0; $i -lt $notices.Count; $i++) { "[$($i + 1)]`n$($notices[$i])" }
$resource = Join-Path $root 'src-tauri/resources'
New-Item -ItemType Directory -Force -Path $resource | Out-Null
$output = $header + "`n" + ($index -join "`n") + "`n`nLICENSE TEXTS`n`n" + ($sections -join "`n`n")
$output | Set-Content -LiteralPath (Join-Path $resource 'third-party-notices.txt') -Encoding utf8
