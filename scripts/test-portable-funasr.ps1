param(
    [Parameter(Mandatory = $true)]
    [string]$RuntimeRoot,
    [int]$Port = 18766,
    [int]$TimeoutSeconds = 300,
    [string]$SampleAudio = ""
)

$ErrorActionPreference = "Stop"
$runtime = (Resolve-Path -LiteralPath $RuntimeRoot).Path
$serviceExe = Join-Path $runtime "funasr-service.exe"
$modelRoot = Join-Path $runtime "models"
$asrModel = Join-Path $modelRoot "iic--speech_seaco_paraformer_large_asr_nat-zh-cn-16k-common-vocab8404-pytorch"
$vadModel = Join-Path $modelRoot "iic--speech_fsmn_vad_zh-cn-16k-common-pytorch"
foreach ($path in @($serviceExe, $asrModel, $vadModel)) {
    if (-not (Test-Path -LiteralPath $path)) {
        throw "FunASR 离线运行时不完整：$path"
    }
}

$logRoot = Join-Path $runtime "_qa"
New-Item -ItemType Directory -Force -Path $logRoot | Out-Null
$stdout = Join-Path $logRoot "portable-funasr.stdout.log"
$stderr = Join-Path $logRoot "portable-funasr.stderr.log"
$previousAsr = $env:BSR_FUNASR_ASR_MODEL
$previousVad = $env:BSR_FUNASR_VAD_MODEL
$previousPath = $env:PATH
$env:BSR_FUNASR_ASR_MODEL = $asrModel
$env:BSR_FUNASR_VAD_MODEL = $vadModel
if (Test-Path -LiteralPath (Join-Path (Split-Path -Parent $runtime) "ffmpeg.exe")) {
    $env:PATH = (Split-Path -Parent $runtime) + ";" + $env:PATH
}

$process = $null
try {
    $process = Start-Process `
        -FilePath $serviceExe `
        -ArgumentList "--port", $Port `
        -WorkingDirectory $runtime `
        -WindowStyle Hidden `
        -RedirectStandardOutput $stdout `
        -RedirectStandardError $stderr `
        -PassThru
    $health = $null
    for ($attempt = 0; $attempt -lt $TimeoutSeconds; $attempt++) {
        Start-Sleep -Seconds 1
        if ($process.HasExited) { break }
        try {
            $health = Invoke-RestMethod -Uri "http://127.0.0.1:$Port/health" -TimeoutSec 2
            if ($health.ready) { break }
        }
        catch { }
    }
    if (-not $health.ready) {
        $details = @(
            Get-Content -LiteralPath $stdout -Tail 80 -ErrorAction SilentlyContinue
            Get-Content -LiteralPath $stderr -Tail 120 -ErrorAction SilentlyContinue
        ) -join [Environment]::NewLine
        throw "可移植 FunASR 健康检查失败。$([Environment]::NewLine)$details"
    }
    $result = [ordered]@{ health = $health }
    if (-not [string]::IsNullOrWhiteSpace($SampleAudio)) {
        $resolvedSample = (Resolve-Path -LiteralPath $SampleAudio).Path
        $body = @{
            audio_path = $resolvedSample
            hotwords = "Canon 70-200"
        } | ConvertTo-Json
        $transcription = Invoke-RestMethod `
            -Uri "http://127.0.0.1:$Port/transcribe" `
            -Method Post `
            -ContentType "application/json; charset=utf-8" `
            -Body $body `
            -TimeoutSec 600
        if ([string]::IsNullOrWhiteSpace($transcription.corrected_text) -and @($transcription.segments).Count -eq 0) {
            throw "FunASR started, but the sample transcription was empty."
        }
        $result.transcription = [ordered]@{
            engine = $transcription.engine
            segment_count = @($transcription.segments).Count
            inference_seconds = $transcription.inference_seconds
            text = $transcription.corrected_text
        }
    }
    $result | ConvertTo-Json -Depth 5
}
finally {
    if ($process -and -not $process.HasExited) {
        Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
    }
    $env:BSR_FUNASR_ASR_MODEL = $previousAsr
    $env:BSR_FUNASR_VAD_MODEL = $previousVad
    $env:PATH = $previousPath
}
