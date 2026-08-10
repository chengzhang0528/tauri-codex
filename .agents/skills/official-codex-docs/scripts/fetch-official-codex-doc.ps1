param(
    [Parameter(Mandatory = $true)][string]$Url,
    [string]$OutputPath = "",
    [int]$TimeoutSeconds = 30
)

$ErrorActionPreference = "Stop"
$parsed = [Uri]$Url
if ($parsed.Scheme -ne "https" -or $parsed.Host -notin @("developers.openai.com", "learn.chatgpt.com")) {
    throw "Only official OpenAI documentation hosts are allowed."
}
if ($TimeoutSeconds -lt 1) { throw "TimeoutSeconds must be positive." }

$proxyCandidates = if ($env:CODEX_DOCS_PROXY) {
    @($env:CODEX_DOCS_PROXY)
} else {
    @("http://localhost:1080", "http://127.0.0.1:1080")
}
$temporary = Join-Path ([IO.Path]::GetTempPath()) ("codex-docs-" + [Guid]::NewGuid().ToString("N") + ".tmp")
$arguments = @(
    "--location", "--fail", "--silent", "--show-error",
    "--max-time", $TimeoutSeconds.ToString(),
    "--user-agent", "tauri-codex-official-docs/1.0",
    "--header", "Accept: text/markdown,text/html;q=0.9"
)

function Invoke-Fetch([string[]]$ExtraArguments) {
    & curl.exe @ExtraArguments --output $temporary $Url
    if ($LASTEXITCODE -ne 0) { throw "curl exited with $LASTEXITCODE" }
    if (-not (Test-Path -LiteralPath $temporary -PathType Leaf)) { throw "curl did not produce a response" }
}

try {
    $fetched = $false
    foreach ($proxy in $proxyCandidates) {
        try {
            Invoke-Fetch ($arguments + @("--proxy", $proxy))
            $fetched = $true
            break
        } catch {
            Remove-Item -LiteralPath $temporary -Force -ErrorAction SilentlyContinue
        }
    }
    if (-not $fetched) {
        Invoke-Fetch $arguments
    }

    if ($OutputPath) {
        $parent = Split-Path -Parent $OutputPath
        if ($parent) { New-Item -ItemType Directory -Force -Path $parent | Out-Null }
        Move-Item -LiteralPath $temporary -Destination $OutputPath -Force
    } else {
        Get-Content -LiteralPath $temporary -Raw
    }
} finally {
    Remove-Item -LiteralPath $temporary -Force -ErrorAction SilentlyContinue
}
