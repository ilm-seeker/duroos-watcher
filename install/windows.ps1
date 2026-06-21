param(
  [string]$Tag = $(if ($env:DUROOS_WATCHER_TAG) { $env:DUROOS_WATCHER_TAG } else { "v0.1.0-alpha.3" }),
  [string]$Repo = $(if ($env:DUROOS_WATCHER_REPO) { $env:DUROOS_WATCHER_REPO } else { "ilm-seeker/duroos-watcher" }),
  [ValidateSet("exe", "msi")]
  [string]$Package = $(if ($env:DUROOS_WATCHER_PACKAGE) { $env:DUROOS_WATCHER_PACKAGE } else { "exe" })
)

$ErrorActionPreference = "Stop"

function Fail($Message) {
  Write-Error $Message
  exit 1
}

function Confirm-UnsignedAlpha {
  if ($env:DUROOS_WATCHER_ACCEPT_UNSIGNED -eq "1") {
    return
  }

  Write-Host "Duroos Watcher Windows installers are unsigned alpha/testing artifacts."
  Write-Host "Only continue if you trust the repository and are comfortable testing unsigned software."
  Write-Host "Set DUROOS_WATCHER_ACCEPT_UNSIGNED=1 to skip this prompt."

  $answer = Read-Host "Continue installing the unsigned alpha? [y/N]"
  if ($answer -notin @("y", "Y", "yes", "YES")) {
    Fail "install cancelled"
  }
}

if (-not [Environment]::Is64BitOperatingSystem) {
  Fail "current Windows alpha assets are x64 only."
}

Confirm-UnsignedAlpha

$baseUrl = "https://github.com/$Repo/releases/download/$Tag"
$checksumName = "SHA256SUMS-$Tag-windows.txt"
if ($Package -eq "msi") {
  $assetName = "Duroos-Watcher-$Tag-windows-unsigned-Duroos.Watcher_0.1.0_x64_en-US.msi"
} else {
  $assetName = "Duroos-Watcher-$Tag-windows-unsigned-Duroos.Watcher_0.1.0_x64-setup.exe"
}

Write-Host "Duroos Watcher $Tag Windows unsigned alpha"
Write-Host "Download: $baseUrl/$assetName"
Write-Host "Package: $Package"

if ($env:DUROOS_WATCHER_DRY_RUN -eq "1") {
  Write-Host "Dry run only; no files were downloaded or installed."
  exit 0
}

$tempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("duroos-watcher-install-" + [System.Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force $tempDir | Out-Null

try {
  $assetPath = Join-Path $tempDir $assetName
  $checksumPath = Join-Path $tempDir $checksumName

  Invoke-WebRequest -Uri "$baseUrl/$assetName" -OutFile $assetPath
  Invoke-WebRequest -Uri "$baseUrl/$checksumName" -OutFile $checksumPath

  $checksumLine = Get-Content $checksumPath | Where-Object { $_ -match "\s+$([regex]::Escape($assetName))$" } | Select-Object -First 1
  if (-not $checksumLine) {
    Fail "checksum file does not include $assetName"
  }

  $expectedHash = (($checksumLine -split "\s+")[0]).ToLowerInvariant()
  $actualHash = (Get-FileHash -Algorithm SHA256 $assetPath).Hash.ToLowerInvariant()
  if ($expectedHash -ne $actualHash) {
    Fail "checksum mismatch for $assetName"
  }

  Write-Host "Checksum OK: $assetName"

  if ($Package -eq "msi") {
    Start-Process -FilePath "msiexec.exe" -ArgumentList @("/i", "`"$assetPath`"") -Wait
  } else {
    Start-Process -FilePath $assetPath -Wait
  }
} finally {
  Remove-Item -Recurse -Force $tempDir -ErrorAction SilentlyContinue
}
