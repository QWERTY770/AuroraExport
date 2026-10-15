@echo off
rem Wrapper so the packaging script can run even when the PowerShell execution
rem policy is RemoteSigned and the file is treated as untrusted.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0package.ps1" %*
