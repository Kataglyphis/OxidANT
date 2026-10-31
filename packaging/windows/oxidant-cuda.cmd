@echo off
setlocal
if not defined KATAGLYPHIS_ORT_DEVICE set "KATAGLYPHIS_ORT_DEVICE=cuda"
"%~dp0kataglyphis_cli.exe" %*
exit /b %errorlevel%
