param(
    [switch]$ValidateOnly,
    [switch]$TestInstall,
    [string]$TestRoot
)

$ErrorActionPreference = "Stop"
if ($args.Count -gt 0) {
    throw "不支持的安装参数：$($args -join ' ')"
}
if ($TestInstall -and [string]::IsNullOrWhiteSpace($TestRoot)) {
    throw "隔离安装测试必须提供 TestRoot。"
}
$packageRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$appSource = Join-Path $packageRoot "app"
$seedRoot = Join-Path $packageRoot "seed"
$runtimeRoot = Join-Path $packageRoot "runtime"
$configTemplate = Join-Path $seedRoot "Conf.template.toml"
$seedDatabase = Join-Path $seedRoot "data_v2.db"
$seedKnowledge = Join-Path $seedRoot "knowledge-vault"
$seedWhisperModel = Join-Path $seedRoot "ggml-small-q5_1.bin"
$webView2Installer = Join-Path $runtimeRoot "MicrosoftEdgeWebView2RuntimeInstallerX64.exe"

$requiredFiles = @(
    (Join-Path $appSource "典典直播切片.exe"),
    (Join-Path $appSource "ffmpeg.exe"),
    (Join-Path $appSource "ffprobe.exe"),
    (Join-Path $appSource "funasr-runtime\funasr-service.exe"),
    $webView2Installer,
    $configTemplate,
    $seedDatabase,
    $seedWhisperModel
)

foreach ($file in $requiredFiles) {
    if (-not (Test-Path -LiteralPath $file -PathType Leaf)) {
        throw "安装包不完整，缺少文件：$file"
    }
}
if (-not (Test-Path -LiteralPath $seedKnowledge -PathType Container)) {
    throw "安装包不完整，缺少知识库目录。"
}
foreach ($modelDirectory in @(
    (Join-Path $appSource "funasr-runtime\models\iic--speech_seaco_paraformer_large_asr_nat-zh-cn-16k-common-vocab8404-pytorch"),
    (Join-Path $appSource "funasr-runtime\models\iic--speech_fsmn_vad_zh-cn-16k-common-pytorch")
)) {
    if (-not (Test-Path -LiteralPath $modelDirectory -PathType Container)) {
        throw "安装包不完整，缺少 FunASR 模型：$modelDirectory"
    }
}
if (-not [Environment]::Is64BitOperatingSystem) {
    throw "当前安装包仅支持 64 位 Windows。"
}

$templateText = Get-Content -LiteralPath $configTemplate -Raw -Encoding UTF8
foreach ($placeholder in @("__CACHE_DIR__", "__OUTPUT_DIR__", "__WHISPER_MODEL__", "__KNOWLEDGE_VAULT__")) {
    if (-not $templateText.Contains($placeholder)) {
        throw "配置模板不完整，缺少占位符：$placeholder"
    }
}

$storageRoot = if ($TestInstall) {
    Join-Path ([System.IO.Path]::GetFullPath($TestRoot).TrimEnd("\")) "storage"
} elseif (Test-Path -LiteralPath "D:\") {
    "D:\典典直播切片"
} else {
    Join-Path ([Environment]::GetFolderPath("MyDocuments")) "典典直播切片"
}
$appDir = Join-Path $storageRoot "应用"
$knowledgeDir = Join-Path $storageRoot "知识库"
$cacheDir = Join-Path $storageRoot "缓存"
$outputDir = Join-Path $storageRoot "录播与切片"
$logDir = Join-Path $storageRoot "安装日志"

if ($ValidateOnly) {
    Write-Host "安装包结构检查通过。" -ForegroundColor Green
    Write-Host "程序与业务数据将安装到：$storageRoot"
    exit 0
}

function Test-WebView2Runtime {
    $clientId = "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"
    $registryPaths = @(
        "HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients\$clientId",
        "HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\$clientId",
        "HKCU:\SOFTWARE\Microsoft\EdgeUpdate\Clients\$clientId"
    )
    foreach ($registryPath in $registryPaths) {
        $version = (Get-ItemProperty -LiteralPath $registryPath -Name "pv" -ErrorAction SilentlyContinue).pv
        if (-not [string]::IsNullOrWhiteSpace($version) -and $version -ne "0.0.0.0") {
            return $true
        }
    }
    return $false
}

if (-not $TestInstall -and -not (Test-WebView2Runtime)) {
    Write-Host "正在安装程序所需的 Microsoft WebView2 运行组件..."
    $runtimeProcess = Start-Process `
        -FilePath $webView2Installer `
        -ArgumentList "/silent", "/install" `
        -Wait `
        -PassThru
    if ($runtimeProcess.ExitCode -ne 0 -and -not (Test-WebView2Runtime)) {
        throw "Microsoft WebView2 运行组件安装失败，退出码：$($runtimeProcess.ExitCode)"
    }
}

$timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
$configBase = if ($TestInstall) {
    Join-Path ([System.IO.Path]::GetFullPath($TestRoot).TrimEnd("\")) "appdata"
} else {
    $env:APPDATA
}
$configDir = Join-Path $configBase "cn.vjoi.bili-shadowreplay"
$databaseDir = Join-Path $configBase "cn.vjoi.bilishadowreplay"
$whisperModel = Join-Path $configDir "models\whisper\ggml-small-q5_1.bin"

if (-not $TestInstall) {
    Get-Process -Name "典典直播切片", "bili-shadowreplay" -ErrorAction SilentlyContinue |
        Stop-Process -Force -ErrorAction SilentlyContinue
}

foreach ($dir in @($appDir, $configDir, $databaseDir, $cacheDir, $outputDir)) {
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
}

$configPath = Join-Path $configDir "Conf.toml"
$databasePath = Join-Path $databaseDir "data_v2.db"
$hasExistingConfig = Test-Path -LiteralPath $configPath -PathType Leaf
$hasExistingDatabase = Test-Path -LiteralPath $databasePath -PathType Leaf
$hasExistingKnowledge = Test-Path -LiteralPath $knowledgeDir -PathType Container
foreach ($path in @($configPath, $databasePath)) {
    if (Test-Path -LiteralPath $path) {
        Copy-Item -LiteralPath $path -Destination "$path.$timestamp.bak" -Force
    }
}

if (-not $hasExistingKnowledge) {
    New-Item -ItemType Directory -Force -Path $knowledgeDir | Out-Null
}

Get-ChildItem -LiteralPath $appSource -Force |
    Copy-Item -Destination $appDir -Recurse -Force
if (-not $hasExistingKnowledge) {
    Get-ChildItem -LiteralPath $seedKnowledge -Force |
        Copy-Item -Destination $knowledgeDir -Recurse -Force
}
if (-not $hasExistingDatabase) {
    Copy-Item -LiteralPath $seedDatabase -Destination $databasePath
}
if (-not (Test-Path -LiteralPath $whisperModel -PathType Leaf)) {
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $whisperModel) | Out-Null
    Copy-Item -LiteralPath $seedWhisperModel -Destination $whisperModel
}

function ConvertTo-TomlPath([string]$value) {
    return $value.Replace("'", "''")
}

$configured = $templateText
$configured = $configured.Replace("__CACHE_DIR__", (ConvertTo-TomlPath $cacheDir))
$configured = $configured.Replace("__OUTPUT_DIR__", (ConvertTo-TomlPath $outputDir))
$configured = $configured.Replace("__WHISPER_MODEL__", (ConvertTo-TomlPath $whisperModel))
$configured = $configured.Replace("__KNOWLEDGE_VAULT__", (ConvertTo-TomlPath $knowledgeDir))
if (-not $hasExistingConfig) {
    [System.IO.File]::WriteAllText($configPath, $configured, (New-Object System.Text.UTF8Encoding($false)))
}

$exePath = Join-Path $appDir "典典直播切片.exe"
if ($TestInstall) {
    Write-Host ""
    Write-Host "隔离安装测试完成。" -ForegroundColor Green
    Write-Host "测试目录：$([System.IO.Path]::GetFullPath($TestRoot))"
    exit 0
}

$shell = New-Object -ComObject WScript.Shell
$desktopShortcut = $shell.CreateShortcut((Join-Path ([Environment]::GetFolderPath("Desktop")) "典典直播切片.lnk"))
$desktopShortcut.TargetPath = $exePath
$desktopShortcut.WorkingDirectory = $appDir
$desktopShortcut.Save()

$startMenuDir = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs"
$startMenuShortcut = $shell.CreateShortcut((Join-Path $startMenuDir "典典直播切片.lnk"))
$startMenuShortcut.TargetPath = $exePath
$startMenuShortcut.WorkingDirectory = $appDir
$startMenuShortcut.Save()

New-Item -ItemType Directory -Force -Path $logDir | Out-Null
$startupStamp = Get-Date -Format "yyyyMMdd-HHmmss"
$startupStdout = Join-Path $logDir "startup-$startupStamp.log"
$startupStderr = Join-Path $logDir "startup-$startupStamp.error.log"
$quotedExe = '"' + $exePath + '"'
$quotedStdout = '"' + $startupStdout + '"'
$quotedStderr = '"' + $startupStderr + '"'
$launchCommand = "cmd.exe /d /c `"$quotedExe 1>$quotedStdout 2>$quotedStderr`""
$launchResult = Invoke-CimMethod `
    -ClassName Win32_Process `
    -MethodName Create `
    -Arguments @{
        CommandLine = $launchCommand
        CurrentDirectory = $appDir
    }
if ($launchResult.ReturnValue -ne 0) {
    throw "程序启动失败，Windows 进程创建返回码：$($launchResult.ReturnValue)"
}

Start-Sleep -Seconds 5
$process = Get-CimInstance Win32_Process |
    Where-Object {
        $_.Name -eq "典典直播切片.exe" -and
        $_.ExecutablePath -eq $exePath
    } |
    Select-Object -First 1

if (-not $process) {
    $startupDetails = @()
    if (Test-Path -LiteralPath $startupStdout) {
        $startupDetails += Get-Content -LiteralPath $startupStdout -Raw -ErrorAction SilentlyContinue
    }
    if (Test-Path -LiteralPath $startupStderr) {
        $startupDetails += Get-Content -LiteralPath $startupStderr -Raw -ErrorAction SilentlyContinue
    }
    $detailText = ($startupDetails -join [Environment]::NewLine).Trim()
    if ([string]::IsNullOrWhiteSpace($detailText)) {
        $detailText = "程序启动后立即退出，未生成详细日志。"
    }
    throw "程序启动失败。日志目录：$logDir`n$detailText"
}

Write-Host ""
Write-Host "安装完成，典典直播切片已成功启动。" -ForegroundColor Green
Write-Host "程序与业务数据目录：$storageRoot"
Write-Host "启动日志目录：$logDir"
