@echo off
setlocal

rem Compilar fuera del arbol del proyecto: dentro de %APPDATA% el enlazador
rem falla con LNK1104 porque Defender bloquea los ejecutables recien creados.
if "%CARGO_TARGET_DIR%"=="" set "CARGO_TARGET_DIR=%USERPROFILE%\.cargo-targets\claude-profiles"

echo Compilando Claude Profiles...
echo   salida: %CARGO_TARGET_DIR%
echo.

cargo build --release
if errorlevel 1 goto :error

echo.
echo Listo: %CARGO_TARGET_DIR%\release\claude-profiles.exe
echo.
choice /C SN /M "Abrir la aplicacion ahora"
if errorlevel 2 goto :end
start "" "%CARGO_TARGET_DIR%\release\claude-profiles.exe"
goto :end

:error
echo.
echo La compilacion fallo.
exit /b 1

:end
endlocal
