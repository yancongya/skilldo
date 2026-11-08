$ErrorActionPreference = "Stop"
# PowerShell 5.1 defaults to TLS 1.0/1.1 on some systems; GitHub requires TLS 1.2.
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$repo = if ($env:SKILLDO_REPO) { $env:SKILLDO_REPO } else { "yancongya/skilldo" }
$installDir = if ($env:SKILLDO_INSTALL_DIR) { $env:SKILLDO_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA "SkillDo\bin" }
$arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
$asset = switch ($arch) {
  "X64" { "skilldo-cli-windows-x64.zip" }
  "Arm64" { "skilldo-cli-windows-arm64.zip" }
  default { throw "Unsupported Windows architecture: $arch" }
}
$base = if ($env:SKILLDO_DOWNLOAD_BASE) { $env:SKILLDO_DOWNLOAD_BASE.TrimEnd('/') } else { "https://github.com/$repo/releases/latest/download" }
$releasesUrl = "https://github.com/$repo/releases/latest"
$apiBase = "https://api.github.com/repos/$repo"
$apiHeaders = @{ "User-Agent" = "skilldo-installer"; "Accept" = "application/vnd.github+json" }
$downloadHeaders = @{ "User-Agent" = "skilldo-installer"; "Accept" = "application/octet-stream" }

# The release CDN can fail with 5xx even when api.github.com is reachable.
# Resolve asset ids up front so every download can fall back to the API asset endpoint.
$apiAssets = $null
if (-not $env:SKILLDO_DOWNLOAD_BASE) {
  try {
    $release = Invoke-RestMethod -Uri "$apiBase/releases/latest" -Headers $apiHeaders -TimeoutSec 30
    $apiAssets = $release.assets
  } catch {
    Write-Warning "GitHub API fallback unavailable: $($_.Exception.Message)"
  }
}

function Get-AssetFallbackUrl {
  param([string]$Name)
  if (-not $apiAssets) { return $null }
  $match = $apiAssets | Where-Object { $_.name -eq $Name } | Select-Object -First 1
  if (-not $match) { return $null }
  return "$apiBase/releases/assets/$($match.id)"
}

function Save-ReleaseAsset {
  param([string]$Name, [string]$OutFile, [string]$FallbackUrl)
  $urls = @("$base/$Name")
  if ($FallbackUrl) { $urls += $FallbackUrl }
  $lastError = $null
  foreach ($url in $urls) {
    for ($attempt = 1; $attempt -le 2; $attempt++) {
      try {
        $iwrParams = @{
          Uri = $url
          OutFile = $OutFile
          Headers = $downloadHeaders
          TimeoutSec = 60
        }
        if ($PSVersionTable.PSVersion.Major -lt 6) { $iwrParams.UseBasicParsing = $true }
        Invoke-WebRequest @iwrParams
        if ((Get-Item -LiteralPath $OutFile).Length -gt 0) { return }
      } catch {
        $lastError = $_
        if (Test-Path -LiteralPath $OutFile) { Remove-Item -LiteralPath $OutFile -Force -ErrorAction SilentlyContinue }
      }
    }
  }
  throw "Failed to download '$Name' from the release CDN and the GitHub API asset endpoint. Last error: $($lastError.Exception.Message). You can download it manually from $releasesUrl"
}

$tempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("skilldo-cli-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $tempDir | Out-Null
try {
  $archive = Join-Path $tempDir $asset
  $checksum = "$archive.sha256"
  Save-ReleaseAsset -Name $asset -OutFile $archive -FallbackUrl (Get-AssetFallbackUrl $asset)
  Save-ReleaseAsset -Name "$asset.sha256" -OutFile $checksum -FallbackUrl (Get-AssetFallbackUrl "$asset.sha256")
  $expected = ((Get-Content $checksum -Raw).Trim() -split '\s+')[0].ToLowerInvariant()
  $actual = (Get-FileHash -Algorithm SHA256 $archive).Hash.ToLowerInvariant()
  if ($expected -ne $actual) { throw "SHA-256 verification failed. Expected $expected, got $actual." }
  Write-Host "SHA-256 verified."
  Expand-Archive -Path $archive -DestinationPath $tempDir -Force
  $binary = Join-Path $tempDir "skilldo.exe"
  if (-not (Test-Path -LiteralPath $binary)) { throw "skilldo.exe was not found in the downloaded archive." }
  New-Item -ItemType Directory -Path $installDir -Force | Out-Null
  Copy-Item $binary (Join-Path $installDir "skilldo.exe") -Force
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
