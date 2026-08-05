param(
    [Parameter(Mandatory = $true)]
    [string]$SourcePackageRoot,
    [string]$OutputRoot = "D:\典典直播切片交付",
    [string]$PackageName = "典典直播切片-公司内部测试版-v1.0.0-20260726-跨电脑部署版"
)

$ErrorActionPreference = "Stop"

$sourceRoot = (Resolve-Path -LiteralPath $SourcePackageRoot).Path
$sourceApp = Join-Path $sourceRoot "app"
$sourceSeed = Join-Path $sourceRoot "seed"
$sourceRuntime = Join-Path $sourceRoot "runtime"
foreach ($requiredDirectory in @($sourceApp, $sourceSeed, $sourceRuntime)) {
    if (-not (Test-Path -LiteralPath $requiredDirectory -PathType Container)) {
        throw "源交付目录不完整，缺少：$requiredDirectory"
    }
}

$outputBase = [System.IO.Path]::GetFullPath($OutputRoot).TrimEnd("\")
if (-not $outputBase.StartsWith("D:\", [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "交付输出必须位于 D 盘，当前为：$outputBase"
}

$packageDir = Join-Path $outputBase $PackageName
$zipPath = "$packageDir.zip"
$hashPath = "$packageDir-SHA256.txt"
$tempRoot = Join-Path $outputBase "_temp"

foreach ($path in @($packageDir, $zipPath, $hashPath)) {
    if (Test-Path -LiteralPath $path) {
        throw "输出已存在，请更换 PackageName，避免覆盖已有交付物：$path"
    }
}

New-Item -ItemType Directory -Force -Path $packageDir, $tempRoot | Out-Null
Copy-Item -LiteralPath $sourceApp -Destination (Join-Path $packageDir "app") -Recurse
Copy-Item -LiteralPath $sourceSeed -Destination (Join-Path $packageDir "seed") -Recurse
Copy-Item -LiteralPath $sourceRuntime -Destination (Join-Path $packageDir "runtime") -Recurse

$templateRoot = Join-Path $PSScriptRoot "internal-delivery"
Copy-Item -LiteralPath (Join-Path $templateRoot "INSTALL.cmd") -Destination $packageDir
Copy-Item -LiteralPath (Join-Path $templateRoot "01-安装并初始化.cmd") -Destination $packageDir
Copy-Item -LiteralPath (Join-Path $templateRoot "install.ps1") -Destination $packageDir

foreach ($optionalFile in @(
    "02-同事操作说明.txt",
    "03-版本说明.txt",
    "04-NAS视频存储使用说明.md"
)) {
    $templateFile = Join-Path $templateRoot $optionalFile
    $sourceFile = Join-Path $sourceRoot $optionalFile
    if (Test-Path -LiteralPath $templateFile -PathType Leaf) {
        Copy-Item -LiteralPath $templateFile -Destination $packageDir
    } elseif (Test-Path -LiteralPath $sourceFile -PathType Leaf) {
        Copy-Item -LiteralPath $sourceFile -Destination $packageDir
    }
}

$noticePath = Join-Path $packageDir "00-请先全部解压再安装.txt"
[System.IO.File]::WriteAllText(
    $noticePath,
    "请先右键压缩包选择【全部解压】，进入解压后的文件夹，再双击【INSTALL.cmd】。`r`n不要直接在压缩包预览窗口中运行安装程序。",
    (New-Object System.Text.UTF8Encoding($false))
)

$previousTemp = $env:TEMP
$previousTmp = $env:TMP
try {
    $env:TEMP = $tempRoot
    $env:TMP = $tempRoot
    Compress-Archive -LiteralPath $packageDir -DestinationPath $zipPath -CompressionLevel Optimal
}
finally {
    $env:TEMP = $previousTemp
    $env:TMP = $previousTmp
}

$hash = Get-FileHash -LiteralPath $zipPath -Algorithm SHA256
"$($hash.Hash)  $([System.IO.Path]::GetFileName($zipPath))" |
    Set-Content -LiteralPath $hashPath -Encoding ASCII

Write-Host "交付包已生成：" -ForegroundColor Green
Write-Host $zipPath
Write-Host $hashPath
