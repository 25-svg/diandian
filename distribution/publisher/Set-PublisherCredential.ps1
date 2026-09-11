# ASCII source is compatible with Windows PowerShell 5 regardless of code page.
# No param block: reject arguments ourselves without PowerShell printing secret values.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$mode = 'Set'
if ($args.Count -gt 0) {
    if ($args.Count -ne 1 -or $args[0] -cnotin @('-Read', '-ValidateOnly', '-Publish')) {
        [Console]::Error.WriteLine('ARGUMENTS_INVALID: credentials must be entered interactively.')
        exit 2
    }
    $mode = $args[0].Substring(1)
}
if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
    [Console]::Error.WriteLine('WINDOWS_REQUIRED')
    exit 1
}
if ($mode -eq 'ValidateOnly') {
    [Console]::WriteLine('Publisher PowerShell validation passed.')
    exit 0
}
function Read-Secret([string]$Prompt) {
    $secure = Read-Host -Prompt $Prompt -AsSecureString
    $pointer = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($secure)
    try { return [Runtime.InteropServices.Marshal]::PtrToStringBSTR($pointer) }
    finally { [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($pointer); $secure.Dispose() }
}
function Set-PrivateAcl([string]$Path, [bool]$Directory) {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent().User
    if ($Directory) {
        $acl = New-Object Security.AccessControl.DirectorySecurity
        $inherit = [Security.AccessControl.InheritanceFlags]'ContainerInherit, ObjectInherit'
        $rule = New-Object Security.AccessControl.FileSystemAccessRule($identity, 'FullControl', $inherit, 'None', 'Allow')
    } else {
        $acl = New-Object Security.AccessControl.FileSecurity
        $rule = New-Object Security.AccessControl.FileSystemAccessRule($identity, 'FullControl', 'Allow')
    }
    $acl.SetOwner($identity)
    $acl.SetAccessRuleProtection($true, $false)
    $acl.AddAccessRule($rule)
    Set-Acl -LiteralPath $Path -AclObject $acl
}
try {
    if ($mode -eq 'Publish') {
        $entry = Join-Path $PSScriptRoot 'dist\publish.js'
        if (-not (Test-Path -LiteralPath $entry -PathType Leaf)) { throw 'BUILD_REQUIRED' }
        $version = Read-Host 'Version (for example 2.21.1)'
        $bundle = Read-Host 'Installer EXE path'
        $signature = Read-Host 'Tauri SIG path'
        $notes = Read-Host 'UTF-8 release notes TXT path'
        & node $entry --version $version --bundle $bundle --signature $signature --notes $notes
        exit $LASTEXITCODE
    }
    Add-Type -AssemblyName System.Security
    $directory = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'DiandianPublisher'
    $path = Join-Path $directory 'publisher.dpapi'
    $utf8 = New-Object Text.UTF8Encoding($false, $true)
    if ($mode -eq 'Read') {
        $item = Get-Item -LiteralPath $path
        if ($item.Length -gt 65536 -or $item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'CREDENTIALS_INVALID' }
        $encrypted = [IO.File]::ReadAllBytes($path)
        $plain = [Security.Cryptography.ProtectedData]::Unprotect($encrypted, $null, [Security.Cryptography.DataProtectionScope]::CurrentUser)
        try {
            [Console]::OutputEncoding = $utf8
            [Console]::Write($utf8.GetString($plain))
        } finally { [Array]::Clear($plain, 0, $plain.Length) }
        exit 0
    }
    Write-Host 'Use an R2 S3 key restricted to ONE target bucket, and a service token restricted to the publisher Access application.'
    Write-Host 'Do not enter account-wide Cloudflare API tokens or an administrator login token.'
    $data = [ordered]@{
        bucket = (Read-Host 'Target private R2 bucket')
        endpoint = (Read-Host 'R2 S3 HTTPS endpoint (origin only)')
        region = (Read-Host 'R2 region (normally auto)')
        accessKeyId = (Read-Secret 'Bucket-scoped S3 access key ID')
        secretAccessKey = (Read-Secret 'Bucket-scoped S3 secret access key')
        serviceTokenId = (Read-Secret 'Publisher Access service token client ID')
        serviceTokenSecret = (Read-Secret 'Publisher Access service token client secret')
        apiBase = (Read-Host 'Publisher API HTTPS base (origin only)')
    }
    foreach ($value in $data.Values) { if ([string]::IsNullOrWhiteSpace($value) -or $value.Length -gt 4096 -or $value -match '[\x00-\x20\x7f-\x9f]') { throw 'CREDENTIALS_INVALID' } }
    $plain = $utf8.GetBytes(($data | ConvertTo-Json -Compress))
    try { $encrypted = [Security.Cryptography.ProtectedData]::Protect($plain, $null, [Security.Cryptography.DataProtectionScope]::CurrentUser) }
    finally { [Array]::Clear($plain, 0, $plain.Length); $data = $null }
    [IO.Directory]::CreateDirectory($directory) | Out-Null
    if ((Get-Item -LiteralPath $directory).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'CREDENTIALS_INVALID' }
    Set-PrivateAcl $directory $true
    if (Test-Path -LiteralPath $path) { throw 'CREDENTIALS_ALREADY_EXIST: remove the old encrypted file deliberately before replacing it.' }
    $stream = New-Object IO.FileStream($path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try { $stream.Write($encrypted, 0, $encrypted.Length) } finally { $stream.Dispose() }
    Set-PrivateAcl $path $false
    Write-Host 'Publisher credentials encrypted for the current Windows user.'
} catch {
    [Console]::Error.WriteLine('PUBLISHER_FAILED: check setup, permissions, credentials, or build. No sensitive details logged.')
    exit 1
}
