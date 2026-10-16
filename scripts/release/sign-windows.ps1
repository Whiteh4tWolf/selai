# Signs the Windows installers (MSI and NSIS EXE) in a bundle directory with
# signtool. The PFX comes from the WINDOWS_CERTIFICATE (base64) and
# WINDOWS_CERTIFICATE_PASSWORD environment variables. The workflow only calls
# this script when WINDOWS_CERTIFICATE is set. The PFX file and the imported
# certificate are removed afterwards.
#
# Usage: pwsh scripts/release/sign-windows.ps1 -BundleDir target/release/bundle
param([Parameter(Mandatory)][string]$BundleDir)
$ErrorActionPreference = 'Stop'

if (-not $env:WINDOWS_CERTIFICATE -or -not $env:WINDOWS_CERTIFICATE_PASSWORD) {
  throw 'WINDOWS_CERTIFICATE and WINDOWS_CERTIFICATE_PASSWORD must both be set'
}

$files = @(Get-ChildItem -Path (Join-Path $BundleDir 'msi\*.msi'), (Join-Path $BundleDir 'nsis\*.exe') -File)
if ($files.Count -eq 0) { throw "no installers to sign under $BundleDir" }

$signtool = Get-ChildItem -Path "${env:ProgramFiles(x86)}\Windows Kits\10\bin" -Recurse -Filter signtool.exe -ErrorAction SilentlyContinue |
  Where-Object { $_.FullName -like '*\x64\*' } |
  Sort-Object FullName -Descending |
  Select-Object -First 1
if (-not $signtool) { throw 'signtool.exe not found on this runner' }

$pfxPath = Join-Path $env:RUNNER_TEMP ('codesign-' + [guid]::NewGuid().ToString('N') + '.pfx')
$thumbprint = $null
try {
  [IO.File]::WriteAllBytes($pfxPath, [Convert]::FromBase64String($env:WINDOWS_CERTIFICATE))
  $password = ConvertTo-SecureString -String $env:WINDOWS_CERTIFICATE_PASSWORD -AsPlainText -Force
  $cert = Import-PfxCertificate -FilePath $pfxPath -CertStoreLocation Cert:\CurrentUser\My -Password $password
  $thumbprint = $cert.Thumbprint

  foreach ($f in $files) {
    # Timestamp server is a public RFC 3161 service. Change it here if the release
    # process uses a different one.
    & $signtool.FullName sign /fd SHA256 /sha1 $thumbprint /tr http://timestamp.digicert.com /td SHA256 $f.FullName
    if ($LASTEXITCODE -ne 0) { throw "signtool sign failed for $($f.Name)" }
    & $signtool.FullName verify /pa /q $f.FullName
    if ($LASTEXITCODE -ne 0) { throw "signtool verify failed for $($f.Name)" }
    Write-Host "Signed $($f.Name)"
  }
}
finally {
  if ($thumbprint) { Remove-Item -Path "Cert:\CurrentUser\My\$thumbprint" -ErrorAction SilentlyContinue }
  Remove-Item -Path $pfxPath -Force -ErrorAction SilentlyContinue
}
