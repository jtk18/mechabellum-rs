# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a **Rust desktop automation tool** for testing the Mechabellum game on Steam, running on Windows 11. The tool provides:
- **Game Automation**: Launches Mechabellum via Steam and interacts with UI elements
- **OCR Text Extraction**: Uses Tesseract to extract and verify text from game regions
- **Template Matching**: Automatically finds UI elements in screenshots and adds them to config
- **Resource File Updates**: Updates JSON files with extracted game data

## Architecture

### **Technology Stack**
- **Language**: Rust (edition 2021)
- **UI Automation**: `enigo` crate for mouse/keyboard control
- **Screenshots**: `screenshots` crate for screen capture
- **OCR**: Direct `tesseract` executable calls (not the tesseract crate to avoid vcpkg issues)
- **Image Processing**: `image` crate for template matching and image manipulation
- **CLI**: `clap` crate for command line argument parsing
- **Windows Integration**: `windows` crate for native Windows API calls

### **Key Dependencies**
```toml
enigo = "0.5.0"           # Mouse/keyboard automation (with Mouse trait)
image = "0.25"            # Image processing and template matching
screenshots = "0.8.10"    # Screen capture
clap = { version = "4.0", features = ["derive"] }  # CLI argument parsing
windows = "0.58"          # Windows API integration
```

## Project Structure

### **Main Source File (`src/main.rs`)**
- **CLI Commands**: Two modes - `run` (normal testing) and `add-region` (template matching)
- **MechabellumTester**: Main struct handling game automation
- **Template Matching**: Functions for finding UI elements in screenshots
- **Config Management**: JSON-based configuration for buttons, text regions, and settings

### **Configuration (`config.json`)**
```json
{
  "steam_app_id": "669330",
  "window_title": "Mechabellum", 
  "buttons": {
    "button_name": {
      "x": 960, "y": 540, "width": 200, "height": 60,
      "description": "Human readable description"
    }
  },
  "text_regions": {
    "region_name": {
      "x": 50, "y": 950, "width": 200, "height": 30,
      "expected_text": "Expected text content",
      "extract_pattern": "Regex pattern for extraction"
    }
  },
  "resource_files": ["game_data.json", "player_stats.json"]
}
```

## Development Commands

### **Build and Run**
```bash
# Clean build
cargo clean && cargo build --release

# Normal testing mode
cargo run
# or explicitly 
cargo run -- run

# Template matching mode
cargo run -- add-region \
  --screenshot "screenshot.png" \
  --template "button.png" \
  --name "new_button" \
  --description "Button description" \
  --expected-text "PLAY" \
  --pattern "PLAY.*"
```

### **Debug Mode**
```bash
RUST_LOG=debug cargo run
```

## Key Features

### **1. Game Automation**
- **Steam Launch**: Uses `steam://rungameid/669330` URLs to launch Mechabellum
- **Window Focus**: Windows API integration to bring game window to foreground
- **Mouse Control**: Precise clicking at configured coordinates
- **Screenshot Capture**: Full screen and region-specific capture

### **2. OCR Text Extraction**
- **Tesseract Integration**: Direct executable calls (avoids vcpkg/leptonica-sys issues)
- **Region-based**: Extracts text from specific screen coordinates
- **Pattern Matching**: Regex support for extracting specific data
- **Debug Screenshots**: Saves full and cropped images for troubleshooting

### **3. Template Matching**
- **Automatic Detection**: Finds smaller images within larger screenshots
- **Pixel-perfect Accuracy**: Sliding window algorithm with RGB difference scoring
- **Config Integration**: Automatically adds detected regions to config.json
- **Visual Verification**: Creates marked screenshots showing detected regions

### **4. Resource File Management**
- **JSON Updates**: Modifies game data files with extracted information
- **Flexible Structure**: Supports any JSON key-value updates
- **File Creation**: Creates new files if they don't exist

## Important Implementation Details

### **Tesseract OCR Approach**
- **Direct Executable**: Calls `tesseract` command directly via `std::process::Command`
- **Temporary Files**: Uses permanent debug files instead of `NamedTempFile` for troubleshooting
- **Error Handling**: Comprehensive error messages for OCR failures

### **Windows API Integration**
- **FindWindowA**: Locates game window by title
- **Result Handling**: Proper error handling for Windows API Result types
- **Window Focus**: `ShowWindow` and `SetForegroundWindow` for bringing game to front

### **Enigo Mouse Control**
- **Trait Import**: Must import `Mouse` trait for `move_mouse` and `button` methods
- **API Usage**: `button(Button::Left, Direction::Click)` for clicking
- **Settings**: `Enigo::new(&Settings::default())` for initialization

### **Template Matching Algorithm**
- **RGB Difference**: Calculates pixel-by-pixel color differences
- **Scoring**: Lower scores indicate better matches (threshold: 50.0)
- **Progress Tracking**: Shows progress for large image searches
- **Quality Validation**: Rejects poor matches with error messages

## Debugging and Troubleshooting

### **Debug File Outputs**
- `debug_full_screenshot_{region_name}.png` - Complete screen capture
- `debug_cropped_region_{region_name}.png` - Extracted region for OCR
- `marked_screenshot_{region_name}.png` - Screenshot with red border showing detected region

### **Common Issues and Solutions**

1. **Build Errors**:
   - Run `cargo clean` before rebuilding
   - Ensure Tesseract is in PATH: `tesseract --version`
   - Check Visual Studio Build Tools are installed

2. **Template Matching**:
   - Use high-quality, exact screenshots for templates
   - Ensure template is smaller than screenshot
   - Check marked screenshots to verify detection accuracy

3. **OCR Issues**:
   - Examine debug screenshots for image quality
   - Adjust text region coordinates if needed
   - Ensure game text is clear and readable

4. **Windows Focus Issues**:
   - Verify `window_title` matches exactly
   - Ensure game is running before automation
   - Check Windows permissions for automation

## Code Conventions

### **Error Handling**
- Use `anyhow::Result` for all fallible functions
- Provide context with `.context()` for meaningful error messages
- Log important operations with `info!`, `warn!`, and `error!` macros

### **Configuration**
- All coordinates and settings are externalized to `config.json`
- Use descriptive names for buttons and regions
- Include human-readable descriptions for maintainability

### **Image Processing**
- Convert images to RGBA8 format for consistency
- Use u32 for coordinates to match image crate conventions
- Validate image dimensions before processing

### **Testing**
- Debug mode provides extensive logging
- Save intermediate images for visual verification
- Use descriptive region names for easy identification

## Deployment Notes

### **Prerequisites**
- Rust toolchain (latest stable)
- Tesseract OCR executable in PATH
- Visual Studio Build Tools (Windows)
- Steam with Mechabellum installed

### **Target Platform**
- Primary: Windows 11
- Window focusing uses Windows-specific APIs
- Steam integration via standard URLs (cross-platform)

### **Performance Considerations**
- Template matching is CPU-intensive for large images
- OCR processing time depends on region size
- Screenshot capture is optimized for primary display

## Future Enhancement Areas

1. **Multi-Monitor Support**: Currently uses primary screen only
2. **Template Caching**: Could cache template matching results
3. **Batch Operations**: Process multiple regions simultaneously
4. **GUI Interface**: Command-line only currently
5. **Game State Detection**: More sophisticated game state awareness

## Security Notes
- Only performs legitimate automation tasks
- Does not modify game files or memory
- Respects game's terms of service
- All operations are external to the game process