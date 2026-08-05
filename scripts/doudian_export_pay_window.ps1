# Export pay_time window orders from an existing order.searchList JSON.
param(
    [string]$InputFile = "d:\Desktop\order-searchList-all-20260729-152019.json",
    [string]$Output = "d:\Desktop\orders-pay-window-20260728-camera.json",
    [string]$ShopId = "212709966",
    [string]$PayTimeStart = "2026/07/28 08:15:00",
    [string]$PayTimeEnd = "2026/07/28 15:44:39",
    [switch]$PaidOnly
)

$repoRoot = Split-Path -Parent $PSScriptRoot
$pythonArgs = @(
    "$repoRoot\scripts\doudian_export_pay_window.py",
    "--input", $InputFile,
    "--output", $Output,
    "--shop-id", $ShopId,
    "--pay-time-start", $PayTimeStart,
    "--pay-time-end", $PayTimeEnd
)
if ($PaidOnly) { $pythonArgs += "--paid-only" }

python @pythonArgs
exit $LASTEXITCODE
