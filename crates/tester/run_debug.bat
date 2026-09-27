@echo off
echo Starting Mechabellum Tester in Debug Mode with Screenshots...
echo.
echo Make sure:
echo 1. Steam is running
echo 2. Mechabellum is installed
echo 3. You have edited config.json with correct coordinates
echo.
echo This will save debug screenshots to help you see what the program is capturing.
echo.
pause

set RUST_LOG=debug
cargo run -- --save-screenshots

echo.
echo Check the directory for debug screenshots:
echo - debug_full_screenshot_*.png (complete screen captures)
echo - debug_cropped_region_*.png (regions sent to OCR)
echo.
echo Press any key to exit...
pause > nul