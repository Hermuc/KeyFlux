@echo off
setlocal enabledelayedexpansion

rem tail 模式: 软件目录删除后由 %TEMP% 副本输出完成总结
if "%~1"=="tail" (
  echo.
  echo ================================================
  echo   卸载流程结束
  echo ================================================
  echo   - 进程: 已清理
  echo   - 注册表自启项: 已处理
  echo   - 启动快捷方式: 已处理
  echo   - 软件目录: 已删除
  echo.
  echo 提示: 软件目录删除成功, 原卸载脚本已随之删除。
  echo 本窗口由临时脚本 %TEMP%\keyflux_uninstall_tail.bat 输出, 可手动删除。
  echo.
  pause
  exit
)

echo ================================================
echo   KeyFlux 卸载程序
echo ================================================
echo.
echo 本程序将执行以下操作:
echo   1. 结束 KeyFlux 相关进程
echo   2. 删除开机自启注册表项 KeyFlux
echo   3. 删除启动文件夹中的旧快捷方式 KeyFlux.lnk
echo   4. 删除整个软件目录: %~dp0
echo.
echo 该操作不可撤销, 请谨慎确认!
echo.
set /p confirm=请输入 Y 确认卸载, 输入其他任意内容将中止: 
if /i not "%confirm%"=="Y" (
  echo.
  echo 已取消卸载, 未做任何更改。
  echo.
  pause
  exit /b 0
)

echo.
echo [1/4] 正在结束 KeyFlux 相关进程...
taskkill /f /im KeyFlux.exe >nul 2>&1
taskkill /f /im KeyFlux-CommandInput.exe >nul 2>&1
taskkill /f /im settings.exe >nul 2>&1
echo       进程清理完成 (未运行的进程已自动跳过)。

echo.
echo [2/4] 正在删除开机自启注册表项...
reg delete "HKCU\Software\Microsoft\Windows\CurrentVersion\Run" /v KeyFlux /f >nul 2>&1
if %errorlevel%==0 (
  echo       注册表自启项已删除。
) else (
  echo       注册表自启项不存在或已删除, 跳过。
)

echo.
echo [3/4] 正在清理启动文件夹旧快捷方式...
set "STARTUP_LNK=%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup\KeyFlux.lnk"
if exist "%STARTUP_LNK%" (
  del /f /q "%STARTUP_LNK%" >nul 2>&1
  echo       旧快捷方式已删除。
) else (
  echo       未发现旧快捷方式, 跳过。
)

echo.
echo [4/4] 正在删除软件目录...
cd /d %TEMP%
if not exist "%~dp0KeyFlux.exe" (
  echo       [警告] 未在 %~dp0 检测到软件主程序 KeyFlux.exe, 为防止误删已跳过目录删除。
  echo       请确认本脚本位于软件目录内后重新运行。
  echo.
  pause
  exit /b 0
)
if not exist "%~dp0" (
  echo       软件目录不存在, 跳过。
  echo.
  pause
  exit /b 0
)
echo       即将删除软件目录: %~dp0
copy /y "%~f0" "%TEMP%\keyflux_uninstall_tail.bat" >nul 2>&1
rmdir /s /q "%~dp0" >nul 2>&1 && call "%TEMP%\keyflux_uninstall_tail.bat" tail || ( echo       [警告] 软件目录删除失败, 可能因文件被占用或权限不足。 & echo       请重启电脑后手动删除: %~dp0 & del /f /q "%TEMP%\keyflux_uninstall_tail.bat" >nul 2>&1 & echo. & pause )

