$ErrorActionPreference = "Stop"
$repo = if ($env:SKILLDO_REPO) { $env:SKILLDO_REPO } else { "yancongya/skilldo" }
$installDir = if ($env:SKILLDO_INSTALL_DIR) { $env:SKILLDO_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA "SkillDo\bin" }
$arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
$asset = switch ($arch) {
  "X64" { "skilldo-cli-windows-x64.zip" }
  "Arm64" { "skilldo-cli-windows-arm64.zip" }
  default { throw "Unsupported Windows architecture: $arch" }
}
$base = if ($env:SKILLDO_DOWNLOAD_BASE) { $env:SKILLDO_DOWNLOAD_BASE.TrimEnd('/') } else { "https://github.com/$repo/releases/latest/download" }
$tempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("skilldo-cli-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $tempDir | Out-Null
try {
  $archive = Join-Path $tempDir $asset
  $checksum = "$archive.sha256"
  Invoke-WebRequest -Uri "$base/$asset" -OutFile $archive
  Invoke-WebRequest -Uri "$base/$asset.sha256" -OutFile $checksum
  $expected = ((Get-Content $checksum -Raw).Trim() -split '\s+')[0].ToLowerInvariant()
  $actual = (Get-FileHash -Algorithm SHA256 $archive).Hash.ToLowerInvariant()
  if ($expected -ne $actual) { throw "SHA-256 verification failed." }
  Expand-Archive -Path $archive -DestinationPath $tempDir -Force
  New-Item -ItemType Directory -Path $installDir -Force | Out-Null
  Copy-Item (Join-Path $tempDir "skilldo.exe") (Join-Path $installDir "skilldo.exe") -Force
  $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
  if (($userPath -split ';') -notcontains $installDir) {
    [Environment]::SetEnvironmentVariable("Path", (($userPath.TrimEnd(';') + ';' + $installDir).TrimStart(';')), "User")
  }
  if (($env:Path -split ';') -notcontains $installDir) { $env:Path += ";$installDir" }
  & (Join-Path $installDir "skilldo.exe") --version
  Write-Host "Installed: $(Join-Path $installDir 'skilldo.exe')"
} finally {
  Remove-Item -LiteralPath $tempDir -Recurse -Force -ErrorAction SilentlyContinue
}
