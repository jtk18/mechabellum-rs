@echo off
echo Building and running Mechabellum Tester...
echo.
echo Make sure:
echo 1. Steam is running
echo 2. Mechabellum is installed
echo 3. You have edited config.json with correct coordinates
echo.
pause

echo Building release version...
cargo build --release

echo.
echo Running tester...
.\target\release\mechabellum-tester.exe

echo.
echo Press any key to exit...
pause > nul