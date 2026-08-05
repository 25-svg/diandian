# Wrapper for scripts/doudian_order_events.py
# Example:
#   .\scripts\doudian_order_events.ps1 `
#     -Orders "d:\Desktop\order-searchList-20260729-135146.json" `
#     -StartedAt "2026-07-29 08:15:49" `
#     -ShopName "金典拍拍相机专卖店" `
#     -PayTimeStart "2026/07/28 08:15:49" `
#     -PayTimeEnd "2026/07/28 15:44:39" `
#     -Output "d:\Desktop\payment-events.json"

param(
    [Parameter(Mandatory = $true)]
    [string]$Orders,
    [Parameter(Mandatory = $true)]
    [string]$StartedAt,
    [string]$Output = "",
    [int]$InWindow = 0,
    [string]$ShopId = "",
    [string]$ShopName = "",
    [string]$PayTimeStart = "",
    [string]$PayTimeEnd = ""
)

$repoRoot = Split-Path -Parent $PSScriptRoot
$pythonArgs = @(
    "$repoRoot\scripts\doudian_order_events.py",
    "--orders", $Orders,
    "--started-at", $StartedAt
)

if ($Output) { $pythonArgs += @("--output", $Output) }
if ($InWindow -gt 0) { $pythonArgs += @("--in-window", "$InWindow") }
if ($ShopId) { $pythonArgs += @("--shop-id", $ShopId) }
if ($ShopName) { $pythonArgs += @("--shop-name", $ShopName) }
if ($PayTimeStart) { $pythonArgs += @("--pay-time-start", $PayTimeStart) }
if ($PayTimeEnd) { $pythonArgs += @("--pay-time-end", $PayTimeEnd) }

python @pythonArgs
