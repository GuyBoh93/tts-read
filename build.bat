@echo off
setlocal

:: Prepend cargo bin dir in case it's not on PATH
set PATH=%USERPROFILE%\.cargo\bin;%PATH%

set CMD=%1
if "%CMD%"=="" set CMD=installer

if "%CMD%"=="installer" (
    :: Ensure cargo-packager is available
    where cargo-packager >nul 2>&1
    if errorlevel 1 (
        echo Installing cargo-packager...
        cargo install cargo-packager --locked
        if errorlevel 1 exit /b 1
    )

    :: Ensure the icon exists; generate it once if not
    if not exist assets\icon.png (
        echo Generating assets\icon.png...
        cargo run --example gen_icon
        if errorlevel 1 exit /b 1
    )

    echo.
    echo Building release binary...
    cargo build --release
    if errorlevel 1 exit /b 1

    echo.
    echo Packaging Windows installer...
    cargo packager --release
    if errorlevel 1 exit /b 1

    echo.
    echo ========================================
    echo  Installer built. Files in dist\:
    echo ========================================
    dir /b dist\*.exe 2>nul
    dir /b dist\*.msi 2>nul
    echo.
    echo Run the .exe in dist\ to install TTS Read on this machine.
    echo It will register itself to autostart on login.
    goto :eof
)

if "%CMD%"=="dev" (
    cargo build
    if errorlevel 1 exit /b 1
    echo.
    echo Debug build OK. Running...
    cargo run
    goto :eof
)

if "%CMD%"=="release" (
    cargo build --release
    if errorlevel 1 exit /b 1
    echo Release binary: target\release\tts-read.exe
    goto :eof
)

if "%CMD%"=="clean" (
    cargo clean
    goto :eof
)

echo Usage: build.bat [installer^|dev^|release^|clean]
echo   installer (default) build release + Windows .exe installer in dist\
echo   dev                 debug build + run (for local testing)
echo   release             optimized release binary, no installer
echo   clean               cargo clean
