# Packages the Windows release into out\: a folder with the game, which the setup wraps, and a
# zip of it. target\release\robigo.exe has to be built with ROBIGO_VERSION set to the version given.
#
# usage: dist\windows\package.ps1 <version>
param([Parameter(Mandatory)][string]$Version)

$ErrorActionPreference = 'Stop'

$root = Resolve-Path (Join-Path $PSScriptRoot '..\..')
$binary = Join-Path $root 'target\release\robigo.exe'
$out = Join-Path $root 'out'
$name = "robigo-$Version-windows-x86_64"
$stage = Join-Path $out $name

$built = & $binary --version
if ($built -ne "robigo $Version") {
    throw "target\release\robigo.exe is '$built', and the release is $Version"
}

# The notes are copied by name, so a game a developer keeps in dist\RETAIL stays out.
New-Item -ItemType Directory -Force (Join-Path $stage 'RETAIL\PATCH') | Out-Null
Copy-Item $binary $stage
Copy-Item (Join-Path $root 'LICENSE') (Join-Path $stage 'LICENSE.txt')
Copy-Item (Join-Path $root 'dist\RETAIL\RETAIL.txt') (Join-Path $stage 'RETAIL')
Copy-Item (Join-Path $root 'dist\RETAIL\PATCH\PATCH.txt') (Join-Path $stage 'RETAIL\PATCH')
Compress-Archive -Path $stage -DestinationPath (Join-Path $out "$name.zip") -Force

Get-ChildItem $out
