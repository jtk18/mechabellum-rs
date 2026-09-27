# Mechabellum Tester

A Rust-based automated testing tool for the Mechabellum game on Steam, running on Windows 11. This tool can launch the game, interact with UI elements, extract and verify text using OCR, and update resource files based on extracted data.

## Features

- **Steam Integration**: Automatically launches Mechabellum via Steam
- **UI Automation**: Click buttons and interact with game elements using coordinates
- **OCR Text Extraction**: Extract text from specific screen regions using Tesseract
- **Text Verification**: Verify extracted text matches expected values
- **Resource File Updates**: Update JSON resource files with extracted data
- **Windows 11 Support**: Native Windows API integration for window management
- **Configurable**: JSON-based configuration for buttons, text regions, and game settings

## Prerequisites

### 1. Install Rust

Download and install Rust from [rustup.rs](https://rustup.rs/):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Restart your terminal and verify installation:
```bash
rustc --version
cargo --version
```

### 2. Install Tesseract OCR

**IMPORTANT**: This project uses the Tesseract executable directly, so you only need to install Tesseract itself - no additional development libraries required.

#### Option A: Using Chocolatey (Recommended)
```bash
# Install Chocolatey first if you don't have it
Set-ExecutionPolicy Bypass -Scope Process -Force; [System.Net.ServicePointManager]::SecurityProtocol = [System.Net.ServicePointManager]::SecurityProtocol -bor 3072; iex ((New-Object System.Net.WebClient).DownloadString('https://community.chocolatey.org/install.ps1'))

# Install Tesseract
choco install tesseract
```

#### Option B: Manual Installation
1. Download Tesseract from [GitHub releases](https://github.com/UB-Mannheim/tesseract/wiki)
2. Install to default location (`C:\Program Files\Tesseract-OCR`)
3. Add `C:\Program Files\Tesseract-OCR` to your PATH environment variable

#### Option C: Using winget
```bash
winget install UB-Mannheim.TesseractOCR
```

### 3. Install Visual Studio Build Tools

Install Microsoft C++ Build Tools for Windows:

#### Option A: Visual Studio Installer
1. Download [Visual Studio Installer](https://visualstudio.microsoft.com/downloads/)
2. Install "Build Tools for Visual Studio"
3. Select "C++ build tools" workload

#### Option B: Using Chocolatey
```bash
choco install visualstudio2022buildtools --package-parameters "--add Microsoft.VisualStudio.Workload.VCTools"
```

### 4. Install Steam and Mechabellum

1. Install [Steam](https://store.steampowered.com/about/) if not already installed
2. Purchase and install [Mechabellum](https://store.steampowered.com/app/669330/Mechabellum/) from Steam
3. Launch the game at least once to ensure it works properly

## Installation

1. **Clone or download this repository**:
   ```bash
   git clone <repository-url>
   cd mechabellum-tester
   ```

2. **Build the project**:
   ```bash
   cargo build --release
   ```

3. **Verify Tesseract installation**:
   ```bash
   tesseract --version
   ```
   Should output version information. If not, check your PATH environment variable.

## Configuration

On first run, the program will create a default `config.json` file. Edit this file to customize:

### Sample Configuration Structure:

```json
{
  "steam_app_id": "669330",
  "window_title": "Mechabellum",
  "buttons": {
    "main_menu": {
      "x": 960,
      "y": 540,
      "width": 200,
      "height": 60,
      "description": "Main menu button"
    },
    "play_button": {
      "x": 960,
      "y": 400,
      "width": 150,
      "height": 50,
      "description": "Play button"
    }
  },
  "text_regions": {
    "version_info": {
      "x": 50,
      "y": 950,
      "width": 200,
      "height": 30,
      "expected_text": "Version",
      "extract_pattern": "Version\\s+(.+)"
    },
    "player_name": {
      "x": 100,
      "y": 100,
      "width": 300,
      "height": 40,
      "expected_text": null,
      "extract_pattern": null
    }
  },
  "resource_files": [
    "game_data.json",
    "player_stats.json"
  ]
}
```

### Configuration Parameters:

- **steam_app_id**: Steam Application ID for Mechabellum (669330)
- **window_title**: Window title to search for when focusing the game
- **buttons**: Dictionary of clickable buttons with coordinates and descriptions
- **text_regions**: Dictionary of screen regions for text extraction
- **resource_files**: List of JSON files to update with extracted data

### Finding Coordinates:

To find button coordinates and text regions:

1. **Use Windows built-in tools**:
   - Press `Windows + Shift + S` to open Snipping Tool
   - Select area and note pixel coordinates from the ruler

2. **Use PowerToys (Recommended)**:
   - Install [PowerToys](https://github.com/microsoft/PowerToys)
   - Use the "Screen Ruler" tool to measure coordinates

3. **Use third-party tools**:
   - [PixelRuler](http://www.mobilefish.com/services/pixelruler/pixelruler.html)
   - [Universal Screen Ruler](https://sourceforge.net/projects/screenruler/)

## Usage

### 1. Run the Program

```bash
# Debug mode (shows detailed logging)
RUST_LOG=debug cargo run

# Release mode
cargo run --release

# Or run the built executable
./target/release/mechabellum-tester.exe
```

### 2. Program Flow

1. **Initial Setup**: Creates default config if none exists
2. **Game Launch**: Launches Mechabellum via Steam URL
3. **Window Focus**: Brings game window to foreground
4. **Test Sequence**: Executes configured test actions:
   - Click buttons based on configuration
   - Extract text from defined regions
   - Verify text matches expected values
   - Update resource files with extracted data

### 3. Customizing Test Sequences

Edit the `run_test_sequence` method in `src/main.rs` to customize your test flow:

```rust
fn run_test_sequence(&mut self) -> Result<()> {
    info!("Starting Mechabellum test sequence");
    
    // Launch the game
    self.launch_game()?;
    
    // Your custom test actions here:
    
    // Click a button
    self.click_button("main_menu")?;
    
    // Extract text from a region
    let version = self.extract_text_from_region("version_info")?;
    
    // Verify text contains expected content
    if self.verify_text("version_info", "Version")? {
        println!("Version verification passed!");
    }
    
    // Update resource file
    self.update_resource_file("game_data.json", "last_version", &version)?;
    
    Ok(())
}
```

## Troubleshooting

### Common Issues

1. **Tesseract not found error**:
   - Ensure Tesseract is in your PATH
   - Try running `tesseract --version` in Command Prompt
   - Reinstall Tesseract if necessary

2. **Build errors related to Windows crates**:
   - Ensure you have Visual Studio Build Tools installed
   - Try running `cargo clean` then `cargo build` again

3. **`leptonica-sys` or `vcpkg` build errors**:
   - This error has been fixed in the latest version - we now use Tesseract executable directly
   - If you still encounter this, make sure you're using the updated `Cargo.toml` and `main.rs`
   - Run `cargo clean` and `cargo build --release` again

4. **Outdated dependency API errors**:
   - All dependencies have been updated to latest versions (enigo 0.5.0, screenshots 0.8.10)
   - Run `cargo clean` and `cargo build --release` if you have compilation issues
   - The API usage has been updated for the latest versions

5. **Game window not found**:
   - Check that the `window_title` in config matches exactly
   - Ensure the game is actually running
   - Try running the game manually first

6. **Steam launch fails**:
   - Verify Steam is installed and running
   - Check that the `steam_app_id` is correct (669330 for Mechabellum)
   - Ensure you own the game on Steam

7. **Mouse clicks not working**:
   - Verify coordinates are correct for your screen resolution
   - Check that the game window is in focus
   - Try adjusting coordinates in the config file

8. **OCR text extraction poor quality**:
   - Ensure text regions are properly sized
   - Check that the game is running at stable FPS
   - Try different screen regions or larger capture areas

### Debug Mode

Run with debug logging to see detailed execution:

```bash
RUST_LOG=debug cargo run
```

This will show:
- Detailed coordinate information
- Screenshot capture status
- Text extraction results
- File update operations

### Performance Tips

- **Screen Resolution**: Higher resolution provides better OCR accuracy
- **Game Settings**: Ensure stable FPS for consistent screenshots
- **Antivirus**: Add project directory to antivirus exceptions
- **Windows Settings**: Disable Windows scaling if coordinates seem off

## Development

### Adding New Features

1. **New Button Actions**: Add button configurations to `config.json`
2. **Text Regions**: Define new text extraction areas
3. **Resource Files**: Add new JSON files to update
4. **Custom Logic**: Modify `run_test_sequence` method

### Testing

Run tests with:
```bash
cargo test
```

### Building for Distribution

```bash
cargo build --release
```

The executable will be located at `target/release/mechabellum-tester.exe`.

## License

This project is provided as-is for educational and testing purposes. Ensure you comply with Steam's Terms of Service and Mechabellum's End User License Agreement when using this tool.

## Support

For issues or questions:
1. Check the troubleshooting section above
2. Ensure all prerequisites are properly installed
3. Run in debug mode to identify specific issues
4. Check that your configuration matches your game setup

## Disclaimer

This tool is designed for legitimate testing purposes only. Use responsibly and in accordance with the game's terms of service. The authors are not responsible for any misuse of this software.