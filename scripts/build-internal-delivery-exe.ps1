param(
    [Parameter(Mandatory = $true)]
    [string]$PackageRoot,
    [Parameter(Mandatory = $true)]
    [string]$SevenZipPath,
    [Parameter(Mandatory = $true)]
    [string]$SfxModulePath,
    [Parameter(Mandatory = $true)]
    [string]$OutputPath,
    [string]$WorkingRoot = "D:\典典直播切片交付验证\sfx-build",
    [ValidateRange(0, 9)]
    [int]$CompressionLevel = 5,
    [ValidateRange(1, 16)]
    [int]$CompressionThreads = 4
)

$ErrorActionPreference = "Stop"

$resolvedPackageRoot = (Resolve-Path -LiteralPath $PackageRoot).Path
$resolvedSevenZip = (Resolve-Path -LiteralPath $SevenZipPath).Path
$resolvedSfxModule = (Resolve-Path -LiteralPath $SfxModulePath).Path
$resolvedOutput = [System.IO.Path]::GetFullPath($OutputPath)
$resolvedWorkingRoot = [System.IO.Path]::GetFullPath($WorkingRoot)

foreach ($path in @($resolvedOutput, ($resolvedOutput + "-SHA256.txt"))) {
    if (Test-Path -LiteralPath $path) {
        throw "输出已存在，请更换 OutputPath，避免覆盖已有交付物：$path"
    }
}
if (-not $resolvedOutput.StartsWith("D:\", [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "单 EXE 输出必须位于 D 盘，当前为：$resolvedOutput"
}
if (-not $resolvedWorkingRoot.StartsWith("D:\", [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "构建目录必须位于 D 盘，当前为：$resolvedWorkingRoot"
}

$runRoot = Join-Path $resolvedWorkingRoot (Get-Date -Format "yyyyMMdd-HHmmss")
New-Item -ItemType Directory -Path $runRoot -Force | Out-Null
$archivePath = Join-Path $runRoot "payload.7z"
$configPath = Join-Path $runRoot "sfx-config.txt"

try {
    Push-Location $resolvedPackageRoot
    try {
        & $resolvedSevenZip a -t7z $archivePath ".\*" "-mx=$CompressionLevel" -m0=lzma2 "-mmt=$CompressionThreads" | Out-Null
        if ($LASTEXITCODE -ne 0) {
            throw "生成单 EXE 内嵌压缩数据失败，退出码：$LASTEXITCODE"
        }
    }
    finally {
        Pop-Location
    }

    $config = @(
        ';!@Install@!UTF-8!'
        'Title="典典直播切片 安装程序"'
        'BeginPrompt="将安装典典直播切片及所需运行组件。已有配置、数据库和知识库不会被覆盖。"'
        'ExecuteFile="powershell.exe"'
        'ExecuteParameters="-NoLogo -NoProfile -ExecutionPolicy Bypass -File install.ps1"'
        ';!@InstallEnd@!'
    ) -join "`r`n"
    [System.IO.File]::WriteAllText($configPath, $config, (New-Object System.Text.UTF8Encoding($false)))

    New-Item -ItemType Directory -Path (Split-Path -Parent $resolvedOutput) -Force | Out-Null
    $outputStream = [System.IO.File]::Create($resolvedOutput)
    try {
        foreach ($part in @($resolvedSfxModule, $configPath, $archivePath)) {
            $inputStream = [System.IO.File]::OpenRead($part)
            try {
                $inputStream.CopyTo($outputStream)
            }
            finally {
                $inputStream.Dispose()
            }
        }
    }
    finally {
        $outputStream.Dispose()
    }

    $hash = Get-FileHash -LiteralPath $resolvedOutput -Algorithm SHA256
    $hashLine = $hash.Hash + "  " + [System.IO.Path]::GetFileName($resolvedOutput)
    [System.IO.File]::WriteAllText(
        ($resolvedOutput + "-SHA256.txt"),
        $hashLine + [Environment]::NewLine,
        (New-Object System.Text.UTF8Encoding($false))
    )
}
finally {
    if (Test-Path -LiteralPath $runRoot) {
        Remove-Item -LiteralPath $runRoot -Recurse -Force
    }
}

Write-Host "单 EXE 安装包已生成：" -ForegroundColor Green
Write-Host $resolvedOutput
Write-Host ($resolvedOutput + "-SHA256.txt")
