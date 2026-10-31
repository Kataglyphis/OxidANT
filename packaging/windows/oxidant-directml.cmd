@echo off
setlocal
if not defined KATAGLYPHIS_ORT_DEVICE set "KATAGLYPHIS_ORT_DEVICE=directml"
"%~dp0kataglyphis_cli.exe" %*
exit /b %errorlevel%
