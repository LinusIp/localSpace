@echo off
setlocal
cd /d "%~dp0"
if "%~1"=="" (
  echo usage:  measure ^<model id^>      for example:  measure qwen3.5-9b-q4_k_m
  echo         measure --list         names the models
  echo.
  localspace.exe measure --list
  exit /b 2
)
if "%~1"=="--list" (
  localspace.exe measure --list
  exit /b %errorlevel%
)
localspace.exe measure --model %1 --out "%~dp0" --download %2 %3 %4
set result=%errorlevel%
echo.
if "%result%"=="0" (
  echo Finished. Please send back the file named on the line above.
) else (
  echo The run did not finish. Please send the lines above, and the file if one was named.
)
pause
exit /b %result%
