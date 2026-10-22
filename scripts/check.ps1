<#
验证入口：一个脚本收口全部验证动作。
  pwsh scripts/check.ps1            日常快门：格式 → 编译 → 全量测试
  pwsh scripts/check.ps1 -Full      提交前门禁：+ clippy + 前端构建
  pwsh scripts/check.ps1 -List      列出全部测试链
#>
[CmdletBinding()]
param(
    [switch]$Full,
    [switch]$List,
    [switch]$Detail
)

$ErrorActionPreference = 'Stop'
Set-Location (Split-Path -Parent $PSScriptRoot)

$TestLog = '.check/test-output.log'
if (-not (Test-Path '.check')) { New-Item -ItemType Directory -Path '.check' | Out-Null }

function Write-FailureSummary {
    if (-not (Test-Path $TestLog)) { return }
    $log = Get-Content $TestLog -ErrorAction SilentlyContinue
    if (-not $log) { return }
    $marks = $log | Select-String -Pattern '^test .* FAILED|^---- |^thread .* panicked|\.rs:\d+:' -Context 0, 4
    $body = if ($marks) { ($marks | Out-String) } else { ($log | Select-Object -Last 30) -join "`n" }
    if (-not $body.Trim()) { return }
    Write-Host ''
    Write-Host '=== 失败摘要 ===' -ForegroundColor Red
    Write-Host $body
    if ($env:GITHUB_STEP_SUMMARY) {
        $fence = ([string][char]96) * 3
        Add-Content -Path $env:GITHUB_STEP_SUMMARY -Value "## 验证失败摘要`n`n${fence}text`n$body`n${fence}" -Encoding utf8
    }
}

function Invoke-Step {
    param([string]$Name, [scriptblock]$Body)
    Write-Host "==> $Name" -ForegroundColor Cyan
    & $Body
    if ($LASTEXITCODE -ne 0) {
        Write-Host "    FAILED: $Name（退出码 $LASTEXITCODE）" -ForegroundColor Red
        Write-FailureSummary
        exit $LASTEXITCODE
    }
}

if ($List) {
    Get-ChildItem -Path 'src-tauri/tests' -Filter '*_chain.rs' | Sort-Object Name | ForEach-Object {
        Write-Host $_.Name -ForegroundColor Cyan
        Get-Content $_.FullName | Select-String -Pattern '^fn (test_\w+)\(' |
            ForEach-Object { '  {0}' -f $_.Matches[0].Groups[1].Value }
    }
    exit 0
}

if ($Full) {
    if (-not (Test-Path 'frontend/node_modules')) {
        Invoke-Step 'frontend deps' {
            Push-Location frontend
            try { npm ci } finally { Pop-Location }
        }
    }
    Invoke-Step 'frontend build' {
        Push-Location frontend
        try { npm run build } finally { Pop-Location }
    }
}

# rustfmt 必须无输出：.gitattributes 把 *.rs 钉成 LF，CRLF 会被一律报成未格式化。
Invoke-Step 'rustfmt' {
    $bad = @(cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check 2>&1 |
        Where-Object { $_ -and $_ -notmatch '^Diff in' })
    foreach ($f in $bad) { Write-Host "      需要格式化：$f" -ForegroundColor Yellow }
    $global:LASTEXITCODE = [int][bool]$bad
}

Invoke-Step 'cargo check' {
    cargo check --manifest-path src-tauri/Cargo.toml --all-targets
}

if ($Full) {
    Invoke-Step 'clippy' {
        cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
    }
}

Invoke-Step 'cargo test' {
    cargo test --manifest-path src-tauri/Cargo.toml 2>&1 |
        Tee-Object -FilePath $TestLog | Out-Null
    if ($Detail) { Get-Content $TestLog | Select-String -Pattern '^test ' }
}

Write-Host ''
Write-Host '全部通过' -ForegroundColor Green
