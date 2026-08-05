# Resume fetch for camera shop 7/28 (narrow window, retries on 系统繁忙).
param(
    [string]$EnvFile = "d:\Desktop\.env",
    [string]$TokenFile = "d:\Desktop\doudian_token.env",
    [string]$SdkPath = "d:\Desktop\doudian-sdk-python-1.1.0-20260724091610\sdk-python",
    [string]$MergeInput = "d:\Desktop\order-searchList-all-20260729-152019.json",
    [string]$Output = "d:\Desktop\order-searchList-camera-20260728-resumed.json",
    [string]$ShopId = "212709966",
    [string]$CreateTimeStart = "2026/07/28 00:00:00",
    [string]$CreateTimeEnd = "2026/07/29 00:00:00",
    [int]$StartPage = 0,
    [int]$PageSize = 100,
    [int]$MaxRetries = 6
)

$repoRoot = Split-Path -Parent $PSScriptRoot
python "$repoRoot\scripts\doudian_resume_shop_fetch.py" `
    --env-file $EnvFile `
    --token-file $TokenFile `
    --sdk-path $SdkPath `
    --merge-input $MergeInput `
    --output $Output `
    --shop-id $ShopId `
    --create-time-start $CreateTimeStart `
    --create-time-end $CreateTimeEnd `
    --start-page $StartPage `
    --page-size $PageSize `
    --max-retries $MaxRetries
