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
serde_yaml = "0.9"        # YAML parsing for DSL test scripts
```

## Project Structure

### **Main Source File (`src/main.rs`)**
- **CLI Commands**: Five modes - `run` (normal testing), `add-region` (direct coordinate input), `add-button` (direct coordinate input), `find-regions` (batch template matching), and `execute-script` (YAML-based test sequences)
- **MechabellumTester**: Main struct handling game automation
- **Template Matching**: Optimized functions for finding UI elements in screenshots with single-pass batch processing
- **Config Management**: JSON-based configuration for buttons, text regions, and settings
- **DSL Engine**: YAML-based Domain Specific Language for defining test sequences

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
# Format code (ALWAYS run before committing)
cargo fmt

# Clean build
cargo clean && cargo build --release

# Normal testing mode
cargo run
# or explicitly 
cargo run -- run

# Normal testing mode with debug screenshots
cargo run -- --save-screenshots run

# Add region with direct coordinates
cargo run -- add-region \
  --name "new_button" \
  --x 100 --y 200 \
  --width 50 --height 30 \
  --description "Button description" \
  --expected-text "PLAY" \
  --pattern "PLAY.*"

# Add button with direct coordinates (note: -H for height to avoid conflict with -h for help)
cargo run -- add-button \
  --name "play_button" \
  --x 794 --y 44 \
  --width 84 --height 48 \
  --description "Play button"

# Find multiple templates in a single efficient pass
cargo run -- find-regions \
  --screenshot "main_menu.png" \
  --template "button1.png" \
  --template "button2.png" \
  --template "ui_element.png"

# Find templates with debug screenshots
cargo run -- --save-screenshots find-regions \
  --screenshot "main_menu.png" \
  --template "button1.png" \
  --template "button2.png"

# Execute a YAML test script
cargo run -- execute-script --script example_test.yml

# Execute test script with debug screenshots
cargo run -- --save-screenshots execute-script --script example_test.yml
```

### **Debug Mode**
```bash
# Debug logging
RUST_LOG=debug cargo run

# Debug logging with screenshot saving
RUST_LOG=debug cargo run -- --save-screenshots run
```

## Key Features

### **1. Game Automation**
- **Steam Launch**: Uses `steam://rungameid/669330` URLs to launch Mechabellum
- **Window Focus**: Windows API integration to bring game window to foreground
- **Mouse Control**: Precise clicking at configured coordinates
- **Screenshot Capture**: Full screen and region-specific capture
- **Graceful Shutdown**: Automatically closes game window after test completion

### **2. OCR Text Extraction**
- **Tesseract Integration**: Direct executable calls (avoids vcpkg/leptonica-sys issues)
- **Region-based**: Extracts text from specific screen coordinates
- **Pattern Matching**: Regex support for extracting specific data
- **Conditional Debug Screenshots**: `--save-screenshots` flag saves full and cropped images for troubleshooting
- **Temporary Files**: Uses temporary files by default for cleaner operation

### **3. Template Matching**
- **Batch Processing**: Optimized single-pass algorithm that searches for multiple templates simultaneously
- **Automatic Detection**: Finds smaller images within larger screenshots
- **Pixel-perfect Accuracy**: Sliding window algorithm with RGB difference scoring
- **Command Output**: Generates ready-to-use CLI commands for adding detected regions
- **Visual Verification**: Creates marked screenshots showing detected regions
- **Performance Optimized**: O(W×H + N) complexity instead of O(N×W×H) for multiple templates

### **4. Resource File Management**
- **JSON Updates**: Modifies game data files with extracted information
- **Flexible Structure**: Supports any JSON key-value updates
- **File Creation**: Creates new files if they don't exist

### **5. YAML-Based Test DSL**
- **Declarative Syntax**: Define test sequences in human-readable YAML format
- **Five Action Types**: `screenshot`, `click`, `read`, `verify`, `wait`
- **Flexible Parameters**: Optional descriptions, wait times, file outputs
- **Config Integration**: References buttons/regions defined in config.json
- **Error Handling**: Fails fast with descriptive error messages
- **Debug Support**: Works with `--save-screenshots` flag for troubleshooting

## Important Implementation Details

### **Tesseract OCR Approach**
- **Direct Executable**: Calls `tesseract` command directly via `std::process::Command`
- **Temporary Files**: Uses permanent debug files instead of `NamedTempFile` for troubleshooting
- **Error Handling**: Comprehensive error messages for OCR failures

### **Windows API Integration**
- **FindWindowA**: Locates game window by title
- **Result Handling**: Proper error handling for Windows API Result types
- **Window Focus**: `ShowWindow` and `SetForegroundWindow` for bringing game to front
- **Window Closing**: `PostMessageA` with `WM_CLOSE` for graceful game shutdown

### **Enigo Mouse Control**
- **Trait Import**: Must import `Mouse` trait for `move_mouse` and `button` methods
- **API Usage**: `button(Button::Left, Direction::Click)` for clicking
- **Settings**: `Enigo::new(&Settings::default())` for initialization

### **Template Matching Algorithm**
- **Single-Pass Efficiency**: `find_multiple_templates_in_image` scans the screenshot once and tests all templates at each position
- **RGB Difference**: Calculates pixel-by-pixel color differences
- **Scoring**: Lower scores indicate better matches (threshold: 50.0)
- **Progress Tracking**: Shows progress for large image searches
- **Quality Validation**: Rejects poor matches with error messages
- **Batch Results**: Returns results for all templates in a single operation

## Debugging and Troubleshooting

### **Debug File Outputs**
When using `--save-screenshots` flag:
- `debug_full_screenshot_{region_name}.png` - Complete screen capture
- `debug_cropped_region_{region_name}.png` - Extracted region for OCR
- `marked_screenshot_{region_name}.png` - Screenshot with red border showing detected region (template matching only)

Without the flag, temporary files are used and automatically cleaned up.

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

### **Code Formatting**
- **MANDATORY: Run `cargo fmt` after EVERY code change and before presenting your changes to a human**
- Use standard Rust formatting conventions (handled automatically by `cargo fmt`)
- Maintain consistent indentation and line wrapping
- Format long function signatures and parameter lists properly
- Claude Code must run `cargo fmt` immediately after making any modifications to Rust files

### **DSL Maintenance**
- **MANDATORY: Keep DSL documentation synchronized with code changes**
- When modifying TestStep enum or DSL functionality, update the YAML Test DSL Syntax section
- When adding new action types, document them with examples in this file
- Ensure example_test.yml demonstrates current DSL capabilities
- Update CLI command documentation when DSL commands change

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
- Template matching is CPU-intensive for large images but optimized for batch processing
- Single-pass algorithm provides 5x+ speedup when searching for multiple templates
- OCR processing time depends on region size
- Screenshot capture is optimized for primary display

## Future Enhancement Areas

1. **Multi-Monitor Support**: Currently uses primary screen only
2. **Template Caching**: Could cache template matching results
3. **GUI Interface**: Command-line only currently
4. **Game State Detection**: More sophisticated game state awareness
5. **Parallel Template Matching**: Could parallelize template matching across CPU cores

## Security Notes
- Only performs legitimate automation tasks
- Does not modify game files or memory
- Respects game's terms of service
- All operations are external to the game process\n\n## CLI Command Reference\n\n### **find-regions** (Optimized Batch Template Matching)\n```bash\ncargo run -- find-regions --screenshot SCREENSHOT --template TEMPLATE1 --template TEMPLATE2 ...\n```\n- Efficiently searches for multiple templates in a single pass through the screenshot\n- Outputs ready-to-use `add-region` commands for each found template\n- Template names automatically derived from filenames (without extension)\n- Use `--save-screenshots` flag to generate marked screenshots showing detection results\n\n### **add-region** (Direct Coordinate Input)\n```bash\ncargo run -- add-region --name NAME --x X --y Y --width WIDTH --height HEIGHT [OPTIONS]\n```\n- Adds a text region directly to config.json with specified coordinates\n- Optional: `--description DESC`, `--expected-text TEXT`, `--pattern REGEX`\n- Use `-H` for height (uppercase) to avoid conflict with help flag `-h`\n\n### **add-button** (Direct Coordinate Input)\n```bash\ncargo run -- add-button --name NAME --x X --y Y --width WIDTH --height HEIGHT [OPTIONS]\n```\n- Adds a button directly to config.json with specified coordinates\n- Optional: `--description DESC`\n- Use `-H` for height (uppercase) to avoid conflict with help flag `-h`\n\n### **run** (Normal Game Testing)\n```bash\ncargo run [-- --save-screenshots] [run]\n```\n- Executes the main game automation sequence\n- Optional: `--save-screenshots` saves debug images for troubleshooting\n\n### **execute-script** (YAML-Based Test Sequences)\n```bash\ncargo run -- execute-script --script TEST_SCRIPT.yml\n```\n- Executes a YAML-defined test sequence instead of hardcoded logic\n- References buttons and regions from config.json\n- Optional: `--save-screenshots` saves debug images for each step\n\n## YAML Test DSL Syntax\n\n### **DSL Structure**\n```yaml\nname: "Test Script Name"\ndescription: "Optional description"\nsteps:\n  - action: ACTION_TYPE\n    # action-specific parameters\n```\n\n### **Action Types**\n\n#### **screenshot** - Capture Screen\n```yaml\n- action: screenshot\n  name: "screenshot_name"           # Required: filename suffix\n  description: "Optional description"\n```\n\n#### **click** - Click Button\n```yaml\n- action: click\n  target: "button_name"            # Required: button name from config.json\n  description: "Optional description"\n  wait: 2000                       # Optional: milliseconds to wait after click\n```\n\n#### **read** - Extract Text\n```yaml\n- action: read\n  target: "region_name"            # Required: text region name from config.json\n  description: "Optional description"\n  save_to: "output.json"           # Optional: JSON file to update\n  key: "extracted_value"           # Optional: JSON key for saved value\n```\n\n#### **verify** - Check Text\n```yaml\n- action: verify\n  target: "region_name"            # Required: text region name from config.json\n  expect: "Expected Text"          # Required: text that must be present\n  description: "Optional description"\n```\n\n#### **wait** - Pause Execution\n```yaml\n- action: wait\n  duration: 1000                   # Required: milliseconds to wait\n  description: "Optional description"\n```\n\n### **Example Test Script**\n```yaml\nname: "Mechabellum Basic Test"\ndescription: "Example test sequence"\nsteps:\n  - action: screenshot\n    name: "main_menu"\n    description: "Capture main menu"\n  \n  - action: click\n    target: "main_menu"\n    description: "Click main menu button"\n    wait: 2000\n  \n  - action: read\n    target: "version_info"\n    description: "Extract version"\n    save_to: "game_data.json"\n    key: "version"\n  \n  - action: verify\n    target: "version_info"\n    expect: "Version"\n    description: "Verify version text"\n```\n\n## New Workflow Advantages\n\n1. **Performance**: Single-pass template matching is 5x+ faster for multiple templates\n2. **Flexibility**: Separate detection from configuration allows custom parameters\n3. **Reliability**: Automatic template naming prevents naming conflicts\n4. **Scalability**: Easily process dozens of templates efficiently\n5. **Debugging**: Optional marked screenshots verify detection accuracy\n6. **Maintainability**: YAML test scripts are easier to read and modify than hardcoded sequences\n7. **Reusability**: Different test scenarios can be defined in separate YAML files