# Wrapper for scripts/doudian_get_token.py
# Example (tool app, test shop):
#   .\scripts\doudian_get_token.ps1 -Mode code -Code "paste-auth-code-here"

param(
    [ValidateSet("code", "self", "refresh")]
    [string]$Mode = "code",
    [string]$Code,
    [string]$ShopId,
    [string]$RefreshToken,
    [string]$EnvFile = "d:\Desktop\.env"
)

$repoRoot = Split-Path -Parent $PSScriptRoot
$pythonArgs = @(
    "$repoRoot\scripts\doudian_get_token.py",
    "--mode", $Mode,
    "--env-file", $EnvFile
)

if ($Code) { $pythonArgs += @("--code", $Code) }
if ($ShopId) { $pythonArgs += @("--shop-id", $ShopId) }
if ($RefreshToken) { $pythonArgs += @("--refresh-token", $RefreshToken) }

python @pythonArgs
