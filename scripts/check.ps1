<#
验证入口：一个脚本收口全部验证动作。

  pwsh scripts/check.ps1            改后端 / 日常：格式 → 编译 → 全量测试
  pwsh scripts/check.ps1 -Frontend  改前端：i18n 审计 + 前端构建
  pwsh scripts/check.ps1 -Full      提交前：两侧全跑 + clippy -D warnings
  pwsh scripts/check.ps1 -List      列出全部集成测试名

只跑一条测试（不必等全量）：
  cargo test --manifest-path src-tauri/Cargo.toml --test chain_tests 关键字
#>
[CmdletBinding()]
param(
    [switch]$Full,
    [switch]$Frontend,
    [switch]$List
)

$ErrorActionPreference = 'Stop'
Set-Location (Split-Path -Parent $PSScriptRoot)
$TestLog = '.check/test-output.log'
if (-not (Test-Path '.check')) { New-Item -ItemType Directory -Path '.check' | Out-Null }

# 失败时只摘出测试名与出错位置：通过就保持安静。
function Write-FailureSummary {
    $log = Get-Content $TestLog -ErrorAction SilentlyContinue
    if (-not $log) { return }
    $marks = $log | Select-String -Pattern '^test .* FAILED|^---- |^thread .* panicked|\.rs:\d+:'
    $body = if ($marks) { $marks | Out-String } else { ($log | Select-Object -Last 30) -join "`n" }
    if (-not $body.Trim()) { return }
    Write-Host ''
    Write-Host '=== 失败摘要 ===' -ForegroundColor Red
    Write-Host $body
}

function Invoke-Step {
    param([string]$Name, [scriptblock]$Body, [switch]$Tests)
    Write-Host "==> $Name" -ForegroundColor Cyan
    & $Body
    if ($LASTEXITCODE -ne 0) {
        Write-Host "   FAILED: $Name（退出码 $LASTEXITCODE）" -ForegroundColor Red
        # 只有测试步骤才有可摘的测试日志，别的步骤失败时别去读旧日志
        if ($Tests) { Write-FailureSummary }
        exit $LASTEXITCODE
    }
}

if ($List) {
    # 函数名就是它守护的那件坏事；坏法对照表见 specs/12-testing.md。
    Get-ChildItem 'src-tauri/tests' -Filter '*.rs' | Sort-Object Name | ForEach-Object {
        Write-Host $_.Name -ForegroundColor Cyan
        $text = [IO.File]::ReadAllText($_.FullName)
        [regex]::Matches($text, '#\[test\]\s*(?://[^\r\n]*\s*)*fn\s+(\w+)\s*\(') |
            ForEach-Object { '  {0}' -f $_.Groups[1].Value }
    }
    exit 0
}

# 前端：缺 key、zh/en 不对齐、模板裸中文都是编译期抓不到的
if ($Full -or $Frontend) {
    Invoke-Step 'i18n audit' { node scripts/i18n-audit.mjs }
    if (-not (Test-Path 'frontend/node_modules')) {
        Invoke-Step 'frontend deps' { npm ci --prefix frontend }
    }
    Invoke-Step 'frontend build' { npm --prefix frontend run build }
}

# 后端：rustfmt 必须无输出（.gitattributes 把 *.rs 钉成 LF）
if (-not $Frontend -or $Full) {
    Invoke-Step 'rustfmt' {
        cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
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
        # 只跑两个目标：没有测试文件的包不喂给 cargo test，免得空跑一遍编译
        cargo test --manifest-path src-tauri/Cargo.toml --lib --test chain_tests 2>&1 |
            Tee-Object -FilePath $TestLog | Out-Null
    } -Tests
}

Write-Host ''
Write-Host '全部通过' -ForegroundColor Green
