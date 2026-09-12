# Register Datum as a Windows Service. Requires an elevated prompt.
# Datum never owns the database (D5 / ADR 0003 as amended). PostgreSQL is a
# separate Windows Service the shop already runs.
#
# Usage:
#   .\install-windows-service.ps1 -BinPath "C:\Program Files\Datum\datum.exe" -Profile regulated-device

param(
  [string]$Name = "DatumServer",
  [string]$BinPath = "C:\Program Files\Datum\datum.exe",
  [string]$Profile = "regulated-device",
  [string]$Bind = "0.0.0.0:8080"
)

$display = "Datum ERP HTTP API"
$bin = "`"$BinPath`" serve --profile $Profile --bind $Bind"

sc.exe create $Name binPath= $bin start= auto DisplayName= $display
if ($LASTEXITCODE -ne 0) {
  Write-Error "sc create failed with $LASTEXITCODE"
  exit $LASTEXITCODE
}

sc.exe description $Name "Datum HTTP API. Never owns the database — PostgreSQL is a separate service."

# Datum never owns the database. These URLs must exist in the service environment.
$envKey = "HKLM:\SYSTEM\CurrentControlSet\Services\$Name"
New-ItemProperty -Path $envKey -Name Environment -PropertyType MultiString -Force -Value @(
  "DATUM_DATABASE_URL=postgres://datum_app@127.0.0.1:5432/datum?sslmode=disable",
  "DATUM_MIGRATE_DATABASE_URL=postgres://datum_migrate@127.0.0.1:5432/datum?sslmode=disable",
  "DATUM_BOOTSTRAP_URL=postgres://127.0.0.1:5432/postgres?sslmode=disable"
) | Out-Null

Write-Host "Created service $Name. Start with: sc start $Name"
Write-Host "Environment includes DATUM_DATABASE_URL, DATUM_MIGRATE_DATABASE_URL, DATUM_BOOTSTRAP_URL."
