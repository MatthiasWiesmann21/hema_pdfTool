@echo off
rem Build wrapper: sets up the MSVC environment first, because rustc's
rem vswhere probe does not detect this (prerelease) VS 2026 installation
rem and would otherwise fall back to Git's GNU link.exe.
rem Usage: build.cmd build|run|clippy|test [extra cargo args]
call "C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=x64 >nul
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
cargo %*
