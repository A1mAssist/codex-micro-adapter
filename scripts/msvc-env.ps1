# Build environment for the MSVC target.
#
# Two local quirks, both found the hard way:
#   1. the "Visual Studio 2026" install here only ships the `onecore` CRT, so the
#      desktop headers/libs have to come from the 2022 build tools;
#   2. this shell exports CC=clang / CXX=clang++, which makes `cc` compile MSVC
#      build scripts with clang -- which has no Windows SDK headers.
#
# Dot-source it before cargo:
#     . .\scripts\msvc-env.ps1
#     cargo run -p codex-micro-desktop
$vcvars = @(
    'D:\Tools\VSBuildTools2022\VC\Auxiliary\Build\vcvars64.bat',
    'C:\Program Files\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat',
    'C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat'
) | Where-Object { Test-Path $_ } | Select-Object -First 1

if ($vcvars) {
    foreach ($line in (cmd /c "`"$vcvars`" >nul 2>&1 && set")) {
        if ($line -match '^([^=]+)=(.*)$') { Set-Item -Path "env:$($matches[1])" -Value $matches[2] }
    }
    Write-Host "MSVC $env:VCToolsVersion, Windows SDK $env:WindowsSdkVersion"
} else {
    Write-Warning 'vcvars64.bat not found; MSVC builds will probably fail'
}
Remove-Item Env:CC -ErrorAction SilentlyContinue
Remove-Item Env:CXX -ErrorAction SilentlyContinue
