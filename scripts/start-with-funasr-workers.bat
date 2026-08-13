@echo off
REM Launch desktop app with multi-process FunASR (default 2 workers).
REM Memory tip: each worker loads a full model. Use 2 on 16GB RAM, try 3~4 only if RAM is ample.
set "BSR_FUNASR_ROOT=d:\git_work\bili-shadowreplay-worktrees\obsidian-vault-read-sync"
set "BSR_FUNASR_WORKERS=2"
set "EXE=C:\Users\10230\.cargo\diandian-tauri-target\release\bili-shadowreplay.exe"
if not exist "%EXE%" (
  echo Missing %EXE%
  exit /b 1
)
start "" "%EXE%"
