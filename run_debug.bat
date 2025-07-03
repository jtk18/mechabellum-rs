@echo off
echo Starting Mechabellum Tester in Debug Mode...
echo.
echo Make sure:
echo 1. Steam is running
echo 2. Mechabellum is installed
echo 3. You have edited config.json with correct coordinates
echo.
pause

set RUST_LOG=debug
cargo run

echo.
echo Press any key to exit...
pause > nul