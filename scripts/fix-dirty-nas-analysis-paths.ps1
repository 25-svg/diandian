# Wrapper around the Python repair tool.
# Usage:
#   powershell -ExecutionPolicy Bypass -File scripts\fix-dirty-nas-analysis-paths.ps1
#   powershell -ExecutionPolicy Bypass -File scripts\fix-dirty-nas-analysis-paths.ps1 -Apply

param(
  [switch]$Apply,
  [string]$DbPath = ""
)

$ErrorActionPreference = "Stop"
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$py = Join-Path $scriptDir "fix-dirty-nas-analysis-paths.py"
$args = @($py)
if ($DbPath) { $args += @("--db", $DbPath) }
if ($Apply) { $args += "--apply" }
python @args
exit $LASTEXITCODE
