# Fetch orders for a live session.
# By placed orders during live (recommended vs old status=2-only pull):
#   .\scripts\doudian_fetch_orders.ps1 `
#     -LiveStartedAt "2026/07/28 08:15:49" `
#     -LiveEndedAt "2026/07/28 15:44:39" `
#     -PlacedInLive `
#     -ShopId "212709966" `
#     -Output "d:\Desktop\order-searchList-camera-20260728-placed.json"

param(
    [string]$EnvFile = "d:\Desktop\.env",
    [string]$TokenFile = "d:\Desktop\doudian_token.env",
    [string]$SdkPath = "d:\Desktop\doudian-sdk-python-1.1.0-20260724091610\sdk-python",
    [Parameter(Mandatory = $true)]
    [string]$Output,
    [string]$LiveStartedAt = "",
    [string]$LiveEndedAt = "",
    [string]$PayTimeStart = "",
    [string]$PayTimeEnd = "",
    [string]$CreateTimeStart = "",
    [string]$CreateTimeEnd = "",
    [int]$CreateBufferBefore = 259200,
    [int]$CreateBufferAfter = 86400,
    [string]$OrderStatus = "",
    [string]$ShopId = "",
    [switch]$PlacedInLive,
    [switch]$KeepUnpaid
)

$repoRoot = Split-Path -Parent $PSScriptRoot
$pythonArgs = @(
    "$repoRoot\scripts\doudian_fetch_orders.py",
    "--env-file", $EnvFile,
    "--token-file", $TokenFile,
    "--sdk-path", $SdkPath,
    "--output", $Output,
    "--create-buffer-before", "$CreateBufferBefore",
    "--create-buffer-after", "$CreateBufferAfter"
)

if ($LiveStartedAt) { $pythonArgs += @("--live-started-at", $LiveStartedAt) }
if ($LiveEndedAt) { $pythonArgs += @("--live-ended-at", $LiveEndedAt) }
if ($PayTimeStart) { $pythonArgs += @("--pay-time-start", $PayTimeStart) }
if ($PayTimeEnd) { $pythonArgs += @("--pay-time-end", $PayTimeEnd) }
if ($CreateTimeStart) { $pythonArgs += @("--create-time-start", $CreateTimeStart) }
if ($CreateTimeEnd) { $pythonArgs += @("--create-time-end", $CreateTimeEnd) }
if ($OrderStatus) { $pythonArgs += @("--order-status", $OrderStatus) }
if ($ShopId) { $pythonArgs += @("--shop-id", $ShopId) }
if ($PlacedInLive) { $pythonArgs += @("--placed-in-live") }
if ($KeepUnpaid) { $pythonArgs += @("--keep-unpaid") }

python @pythonArgs
