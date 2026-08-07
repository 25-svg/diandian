param(
    [Parameter(Mandatory = $true)]
    [string]$InstallerPath,
    [Parameter(Mandatory = $true)]
    [string]$SevenZipPath,
    [string]$ExtractRoot = "D:\典典直播切片交付验证"
)

$ErrorActionPreference = "Stop"

$resolvedInstaller = (Resolve-Path -LiteralPath $InstallerPath).Path
$resolvedSevenZip = (Resolve-Path -LiteralPath $SevenZipPath).Path
$extractBase = [System.IO.Path]::GetFullPath($ExtractRoot).TrimEnd("\")
if (-not $extractBase.StartsWith("D:\", [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "验证目录必须位于 D 盘，当前为：$extractBase"
}

& $resolvedSevenZip t $resolvedInstaller | Out-Null
if ($LASTEXITCODE -ne 0) {
    throw "单 EXE 安装包的内嵌压缩数据校验失败，退出码：$LASTEXITCODE"
}

$runId = "single-exe-" + (Get-Date -Format "yyyyMMdd-HHmmss")
$extractPath = Join-Path $extractBase $runId
if (Test-Path -LiteralPath $extractPath) {
    throw "验证目录已存在：$extractPath"
}
New-Item -ItemType Directory -Path $extractPath | Out-Null

try {
    & $resolvedSevenZip x $resolvedInstaller "-o$extractPath" -y | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw "单 EXE 安装包解压验证失败，退出码：$LASTEXITCODE"
    }

    foreach ($requiredFile in @(
        (Join-Path $extractPath "install.ps1"),
        (Join-Path $extractPath "INSTALL.cmd"),
        (Join-Path $extractPath "app\典典直播切片.exe"),
        (Join-Path $extractPath "app\ffmpeg.exe"),
        (Join-Path $extractPath "app\ffprobe.exe"),
        (Join-Path $extractPath "app\msvcp140.dll"),
        (Join-Path $extractPath "app\VCOMP140.DLL"),
        (Join-Path $extractPath "app\vcruntime140.dll"),
        (Join-Path $extractPath "app\vcruntime140_1.dll"),
        (Join-Path $extractPath "app\funasr-runtime\funasr-service.exe"),
        (Join-Path $extractPath "runtime\MicrosoftEdgeWebView2RuntimeInstallerX64.exe"),
        (Join-Path $extractPath "seed\data_v2.db"),
        (Join-Path $extractPath "seed\ggml-small-q5_1.bin")
    )) {
        if (-not (Test-Path -LiteralPath $requiredFile -PathType Leaf)) {
            throw "单 EXE 安装包缺少文件：$requiredFile"
        }
    }

    $installScript = Join-Path $extractPath "install.ps1"
    & powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File $installScript -ValidateOnly
    if ($LASTEXITCODE -ne 0) {
        throw "单 EXE 安装包结构验证失败，退出码：$LASTEXITCODE"
    }

    $testInstallRoot = Join-Path $extractPath "_isolated-install"
    & powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File $installScript -TestInstall -TestRoot $testInstallRoot
    if ($LASTEXITCODE -ne 0) {
        throw "单 EXE 安装包隔离安装失败，退出码：$LASTEXITCODE"
    }
    foreach ($installedFile in @(
        (Join-Path $testInstallRoot "storage\应用\funasr-runtime\funasr-service.exe"),
        (Join-Path $testInstallRoot "storage\应用\vcruntime140.dll"),
        (Join-Path $testInstallRoot "storage\应用\vcruntime140_1.dll"),
        (Join-Path $testInstallRoot "appdata\cn.vjoi.bili-shadowreplay\models\whisper\ggml-small-q5_1.bin")
    )) {
        if (-not (Test-Path -LiteralPath $installedFile -PathType Leaf)) {
            throw "单 EXE 隔离安装结果不完整，缺少：$installedFile"
        }
    }
}
finally {
    if (Test-Path -LiteralPath $extractPath) {
        Remove-Item -LiteralPath $extractPath -Recurse -Force
    }
}

Write-Host "单 EXE 安装包检查通过：$resolvedInstaller" -ForegroundColor Green
