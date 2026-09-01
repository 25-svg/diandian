param(
    [Parameter(Mandatory = $true)]
    [string]$ZipPath,
    [string]$ExtractRoot = "D:\典典直播切片交付验证"
)

$ErrorActionPreference = "Stop"

$resolvedZip = (Resolve-Path -LiteralPath $ZipPath).Path
Add-Type -AssemblyName System.IO.Compression.FileSystem

$archive = [System.IO.Compression.ZipFile]::OpenRead($resolvedZip)
try {
    $fileEntries = @($archive.Entries | Where-Object { -not [string]::IsNullOrWhiteSpace($_.Name) })
    if ($fileEntries.Count -eq 0) {
        throw "压缩包中没有文件。"
    }

    $rootNames = @(
        $fileEntries |
            ForEach-Object { ($_.FullName -replace "\\", "/").Split("/")[0] } |
            Sort-Object -Unique
    )
    if ($rootNames.Count -ne 1) {
        throw "压缩包必须只有一个根目录，当前发现：$($rootNames -join '、')"
    }

    $rootName = $rootNames[0]
    $requiredEntries = @(
        "$rootName/INSTALL.cmd",
        "$rootName/01-安装并初始化.cmd",
        "$rootName/install.ps1",
        "$rootName/app/典典直播切片.exe",
        "$rootName/app/ffmpeg.exe",
        "$rootName/app/ffprobe.exe",
        "$rootName/app/msvcp140.dll",
        "$rootName/app/VCOMP140.DLL",
        "$rootName/app/vcruntime140.dll",
        "$rootName/app/vcruntime140_1.dll",
        "$rootName/app/funasr-runtime/funasr-service.exe",
        "$rootName/app/doudian-runtime/doudian-fetch-payment-events.exe",
        "$rootName/runtime/MicrosoftEdgeWebView2RuntimeInstallerX64.exe",
        "$rootName/seed/Conf.template.toml",
        "$rootName/seed/data_v2.db",
        "$rootName/seed/ggml-small-q5_1.bin"
    )
    $entryNames = @($fileEntries | ForEach-Object { $_.FullName -replace "\\", "/" })
    foreach ($requiredEntry in $requiredEntries) {
        if ($entryNames -notcontains $requiredEntry) {
            throw "压缩包结构错误，缺少：$requiredEntry"
        }
    }

    $installEntry = $entryNames | Where-Object { $_ -eq "$rootName/install.ps1" }
    if (@($installEntry).Count -ne 1) {
        throw "install.ps1 必须直接位于唯一根目录下。"
    }

    foreach ($launcherName in @("INSTALL.cmd", "01-安装并初始化.cmd")) {
        $launcherEntry = $fileEntries | Where-Object {
            ($_.FullName -replace "\\", "/") -eq "$rootName/$launcherName"
        } | Select-Object -First 1
        $launcherStream = $launcherEntry.Open()
        try {
            $buffer = New-Object byte[] $launcherEntry.Length
            [void]$launcherStream.Read($buffer, 0, $buffer.Length)
            if (@($buffer | Where-Object { $_ -gt 127 }).Count -gt 0) {
                throw "$launcherName 必须只包含 ASCII 字符，避免 Windows cmd 编码损坏。"
            }
        }
        finally {
            $launcherStream.Dispose()
        }
    }
}
finally {
    $archive.Dispose()
}

$extractBase = [System.IO.Path]::GetFullPath($ExtractRoot).TrimEnd("\")
if (-not $extractBase.StartsWith("D:\", [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "验证目录必须位于 D 盘，当前为：$extractBase"
}

$runId = [System.IO.Path]::GetFileNameWithoutExtension($resolvedZip) + "-" + (Get-Date -Format "yyyyMMdd-HHmmss")
$extractPath = Join-Path $extractBase $runId
New-Item -ItemType Directory -Force -Path $extractPath | Out-Null

try {
    [System.IO.Compression.ZipFile]::ExtractToDirectory($resolvedZip, $extractPath)
    $packageRoot = Join-Path $extractPath $rootName
    $installScript = Join-Path $packageRoot "install.ps1"
    $launcher = Join-Path $packageRoot "INSTALL.cmd"

    & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $installScript -ValidateOnly
    if ($LASTEXITCODE -ne 0) {
        throw "安装包结构验证脚本执行失败，退出码：$LASTEXITCODE"
    }

    & cmd.exe /d /c call $launcher -ValidateOnly
    if ($LASTEXITCODE -ne 0) {
        throw "INSTALL.cmd 启动验证失败，退出码：$LASTEXITCODE"
    }

    $previousErrorAction = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $installScript -UnsupportedInstallerFlag 2>$null
    $unsupportedParameterExitCode = $LASTEXITCODE
    $ErrorActionPreference = $previousErrorAction
    if ($unsupportedParameterExitCode -eq 0) {
        throw "安装器必须拒绝未知参数，避免测试参数触发正式安装。"
    }

    $testInstallRoot = Join-Path $extractPath "_isolated-install"
    & cmd.exe /d /c call $launcher -TestInstall -TestRoot $testInstallRoot
    if ($LASTEXITCODE -ne 0) {
        throw "INSTALL.cmd 隔离安装测试失败，退出码：$LASTEXITCODE"
    }

    foreach ($installedFile in @(
        (Join-Path $testInstallRoot "storage\应用\典典直播切片.exe"),
        (Join-Path $testInstallRoot "storage\应用\ffmpeg.exe"),
        (Join-Path $testInstallRoot "storage\应用\vcruntime140.dll"),
        (Join-Path $testInstallRoot "storage\应用\vcruntime140_1.dll"),
        (Join-Path $testInstallRoot "storage\应用\funasr-runtime\funasr-service.exe"),
        (Join-Path $testInstallRoot "storage\应用\doudian-runtime\doudian-fetch-payment-events.exe"),
        (Join-Path $testInstallRoot "appdata\cn.vjoi.bili-shadowreplay\Conf.toml"),
        (Join-Path $testInstallRoot "appdata\cn.vjoi.bili-shadowreplay\models\whisper\ggml-small-q5_1.bin"),
        (Join-Path $testInstallRoot "appdata\cn.vjoi.bilishadowreplay\data_v2.db")
    )) {
        if (-not (Test-Path -LiteralPath $installedFile -PathType Leaf)) {
            throw "隔离安装结果不完整，缺少：$installedFile"
        }
    }

    $installedDatabase = Join-Path $testInstallRoot "appdata\cn.vjoi.bilishadowreplay\data_v2.db"
    [System.IO.File]::WriteAllText(
        $installedDatabase,
        "existing-user-database-must-be-preserved",
        (New-Object System.Text.UTF8Encoding($false))
    )
    $installedConfig = Join-Path $testInstallRoot "appdata\cn.vjoi.bili-shadowreplay\Conf.toml"
    [System.IO.File]::WriteAllText(
        $installedConfig,
        "existing-user-config-must-be-preserved",
        (New-Object System.Text.UTF8Encoding($false))
    )
    $knowledgeMarker = Join-Path $testInstallRoot "storage\知识库\existing-user-knowledge.md"
    [System.IO.File]::WriteAllText(
        $knowledgeMarker,
        "existing-user-knowledge-must-be-preserved",
        (New-Object System.Text.UTF8Encoding($false))
    )
    $databaseHashBeforeReinstall = (Get-FileHash -LiteralPath $installedDatabase -Algorithm SHA256).Hash
    $configHashBeforeReinstall = (Get-FileHash -LiteralPath $installedConfig -Algorithm SHA256).Hash

    & cmd.exe /d /c call $launcher -TestInstall -TestRoot $testInstallRoot
    if ($LASTEXITCODE -ne 0) {
        throw "INSTALL.cmd 隔离重复安装测试失败，退出码：$LASTEXITCODE"
    }

    $databaseHashAfterReinstall = (Get-FileHash -LiteralPath $installedDatabase -Algorithm SHA256).Hash
    if ($databaseHashAfterReinstall -ne $databaseHashBeforeReinstall) {
        throw "重复安装覆盖了已有用户数据库。"
    }
    $configHashAfterReinstall = (Get-FileHash -LiteralPath $installedConfig -Algorithm SHA256).Hash
    if ($configHashAfterReinstall -ne $configHashBeforeReinstall) {
        throw "重复安装覆盖了已有用户配置。"
    }
    if (-not (Test-Path -LiteralPath $knowledgeMarker -PathType Leaf)) {
        throw "重复安装覆盖了已有用户知识库。"
    }
}
finally {
    if (Test-Path -LiteralPath $extractPath) {
        Remove-Item -LiteralPath $extractPath -Recurse -Force
    }
}

Write-Host "交付包检查通过：$resolvedZip" -ForegroundColor Green
