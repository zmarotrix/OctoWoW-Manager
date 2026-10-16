@echo off
title OctoWoW Manager (dev)
rem Launches the app in dev mode with hot reload. Close the app window or press Ctrl+C to stop.
pushd "%~dp0"

if not exist node_modules (
	echo Installing dependencies...
	call npm install || goto :error
)

call npm run tauri dev || goto :error
popd
exit /b 0

:error
echo.
echo Launch failed. See the messages above.
pause
popd
exit /b 1
