use anyhow::{Context, Result};
use enigo::{Button, Coordinate, Enigo, Mouse};
use image::{DynamicImage, ImageBuffer, Rgba};
use log::{debug, error, info, warn};
use regex::Regex;
use screenshots::Screen;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::Duration;
use tempfile::NamedTempFile;

#[cfg(target_os = "windows")]
use windows::core::PCSTR;
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::HWND;
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowA, SetForegroundWindow, ShowWindow, SW_RESTORE,
};

#[derive(Debug, Serialize, Deserialize)]
struct GameConfig {
    steam_app_id: String,
    window_title: String,
    buttons: HashMap<String, ButtonConfig>,
    text_regions: HashMap<String, TextRegion>,
    resource_files: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ButtonConfig {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    description: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct TextRegion {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    expected_text: Option<String>,
    extract_pattern: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ResourceUpdate {
    file_path: String,
    key: String,
    value: String,
}

struct MechabellumTester {
    config: GameConfig,
    enigo: Enigo,
}

impl MechabellumTester {
    fn new(config_path: &str) -> Result<Self> {
        let config_content =
            fs::read_to_string(config_path).context("Failed to read config file")?;
        let config: GameConfig =
            serde_json::from_str(&config_content).context("Failed to parse config JSON")?;

        let enigo = Enigo::new().context("Failed to initialize Enigo")?;

        // Test tesseract installation
        Self::test_tesseract()?;

        Ok(MechabellumTester { config, enigo })
    }

    fn test_tesseract() -> Result<()> {
        info!("Testing Tesseract installation...");

        let output = Command::new("tesseract")
            .arg("--version")
            .output()
            .context(
            "Failed to run tesseract command. Make sure Tesseract is installed and in your PATH",
        )?;

        if output.status.success() {
            let version_info = String::from_utf8_lossy(&output.stderr);
            info!(
                "Tesseract found: {}",
                version_info.lines().next().unwrap_or("Unknown version")
            );
        } else {
            return Err(anyhow::anyhow!("Tesseract command failed"));
        }

        Ok(())
    }

    fn launch_game(&self) -> Result<()> {
        info!("Launching Mechabellum via Steam");

        let steam_url = format!("steam://rungameid/{}", self.config.steam_app_id);

        #[cfg(target_os = "windows")]
        {
            Command::new("cmd")
                .args(["/c", "start", &steam_url])
                .spawn()
                .context("Failed to launch Steam game")?;
        }

        #[cfg(not(target_os = "windows"))]
        {
            Command::new("xdg-open")
                .arg(&steam_url)
                .spawn()
                .context("Failed to launch Steam game")?;
        }

        info!("Waiting for game to start...");
        thread::sleep(Duration::from_secs(30));

        self.focus_game_window()?;
        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn focus_game_window(&self) -> Result<()> {
        unsafe {
            let window_title = std::ffi::CString::new(self.config.window_title.clone())?;
            let hwnd = FindWindowA(PCSTR::null(), PCSTR(window_title.as_ptr() as *const u8));

            if hwnd.0 != 0 {
                ShowWindow(hwnd, SW_RESTORE);
                SetForegroundWindow(hwnd);
                info!("Focused game window: {}", self.config.window_title);
            } else {
                warn!("Could not find game window: {}", self.config.window_title);
            }
        }
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    fn focus_game_window(&self) -> Result<()> {
        info!("Window focusing not implemented for non-Windows platforms");
        Ok(())
    }

    fn take_screenshot(&self) -> Result<DynamicImage> {
        let screens = Screen::all().context("Failed to get screens")?;
        let primary_screen = screens.first().context("No screens found")?;

        let screenshot = primary_screen
            .capture()
            .context("Failed to capture screen")?;

        // Convert screenshot to DynamicImage
        let image = DynamicImage::ImageRgba8(
            ImageBuffer::from_raw(
                screenshot.width() as u32,
                screenshot.height() as u32,
                screenshot.as_raw().to_vec(),
            )
            .context("Failed to create image buffer")?,
        );

        Ok(image)
    }

    fn click_button(&mut self, button_name: &str) -> Result<()> {
        let button = self
            .config
            .buttons
            .get(button_name)
            .context(format!("Button '{}' not found in config", button_name))?;

        info!(
            "Clicking button: {} at ({}, {})",
            button_name, button.x, button.y
        );

        // Move mouse to button position
        self.enigo
            .move_mouse(button.x, button.y, Coordinate::Abs)
            .context("Failed to move mouse")?;

        thread::sleep(Duration::from_millis(100));

        // Click the left mouse button
        self.enigo
            .click(Button::Left)
            .context("Failed to click button")?;

        thread::sleep(Duration::from_millis(500));
        Ok(())
    }

    fn extract_text_from_region(&self, region_name: &str) -> Result<String> {
        let region = self
            .config
            .text_regions
            .get(region_name)
            .context(format!("Text region '{}' not found in config", region_name))?;

        let screenshot = self.take_screenshot()?;
        let cropped = screenshot.crop_imm(
            region.x as u32,
            region.y as u32,
            region.width as u32,
            region.height as u32,
        );

        // Create a temporary file for the cropped image
        let temp_file =
            NamedTempFile::with_suffix(".png").context("Failed to create temporary file")?;
        let temp_path = temp_file.path();

        cropped
            .save(temp_path)
            .context("Failed to save temporary image")?;

        // Run tesseract on the temporary file
        let output = Command::new("tesseract")
            .arg(temp_path)
            .arg("stdout")
            .arg("-l")
            .arg("eng")
            .output()
            .context("Failed to run tesseract command")?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Tesseract failed: {}", error_msg));
        }

        let extracted_text = String::from_utf8_lossy(&output.stdout);
        let cleaned_text = extracted_text.trim().to_string();

        info!(
            "Extracted text from region '{}': '{}'",
            region_name, cleaned_text
        );

        // Apply regex pattern if specified
        if let Some(pattern) = &region.extract_pattern {
            let regex = Regex::new(pattern).context("Failed to compile regex pattern")?;

            if let Some(captures) = regex.captures(&cleaned_text) {
                if let Some(matched) = captures.get(1) {
                    let pattern_result = matched.as_str().to_string();
                    info!("Pattern matched: '{}'", pattern_result);
                    return Ok(pattern_result);
                }
            }
        }

        Ok(cleaned_text)
    }

    fn verify_text(&self, region_name: &str, expected_text: &str) -> Result<bool> {
        let extracted_text = self.extract_text_from_region(region_name)?;
        let matches = extracted_text.contains(expected_text);

        if matches {
            info!(
                "Text verification passed for region '{}': found '{}'",
                region_name, expected_text
            );
        } else {
            warn!(
                "Text verification failed for region '{}': expected '{}', found '{}'",
                region_name, expected_text, extracted_text
            );
        }

        Ok(matches)
    }

    fn update_resource_file(&self, file_path: &str, key: &str, value: &str) -> Result<()> {
        info!(
            "Updating resource file: {} with key: {} = {}",
            file_path, key, value
        );

        let path = Path::new(file_path);
        let mut content = if path.exists() {
            fs::read_to_string(path).context("Failed to read resource file")?
        } else {
            "{}".to_string()
        };

        // Try to parse as JSON
        let mut json_data: serde_json::Value =
            serde_json::from_str(&content).context("Failed to parse resource file as JSON")?;

        // Update the value
        json_data[key] = serde_json::Value::String(value.to_string());

        // Write back to file
        let updated_content =
            serde_json::to_string_pretty(&json_data).context("Failed to serialize JSON")?;

        fs::write(path, updated_content).context("Failed to write updated resource file")?;

        info!("Successfully updated resource file: {}", file_path);
        Ok(())
    }

    fn run_test_sequence(&mut self) -> Result<()> {
        info!("Starting Mechabellum test sequence");

        // Launch the game
        self.launch_game()?;

        // Wait a bit more for the game to fully load
        thread::sleep(Duration::from_secs(10));

        // Example test sequence - customize based on your needs
        info!("Running test sequence...");

        // Click main menu button
        if self.config.buttons.contains_key("main_menu") {
            self.click_button("main_menu")?;
            thread::sleep(Duration::from_secs(2));
        }

        // Extract and verify some text
        if self.config.text_regions.contains_key("version_info") {
            let version_text = self.extract_text_from_region("version_info")?;

            // Update resource file with extracted version
            self.update_resource_file("game_data.json", "version", &version_text)?;
        }

        // More test actions can be added here
        // For example: navigating menus, checking game state, etc.

        info!("Test sequence completed successfully");
        Ok(())
    }
}

fn main() -> Result<()> {
    env_logger::init();

    info!("Starting Mechabellum Tester");

    let config_path = "config.json";
    if !Path::new(config_path).exists() {
        create_default_config(config_path)?;
        info!("Created default config file: {}", config_path);
        info!("Please edit the config file and run again");
        return Ok(());
    }

    let mut tester = MechabellumTester::new(config_path)?;
    tester.run_test_sequence()?;

    Ok(())
}

fn create_default_config(config_path: &str) -> Result<()> {
    let default_config = GameConfig {
        steam_app_id: "669330".to_string(), // Mechabellum Steam App ID
        window_title: "Mechabellum".to_string(),
        buttons: HashMap::from([
            (
                "main_menu".to_string(),
                ButtonConfig {
                    x: 960,
                    y: 540,
                    width: 200,
                    height: 60,
                    description: "Main menu button".to_string(),
                },
            ),
            (
                "play_button".to_string(),
                ButtonConfig {
                    x: 960,
                    y: 400,
                    width: 150,
                    height: 50,
                    description: "Play button".to_string(),
                },
            ),
        ]),
        text_regions: HashMap::from([
            (
                "version_info".to_string(),
                TextRegion {
                    x: 50,
                    y: 950,
                    width: 200,
                    height: 30,
                    expected_text: Some("Version".to_string()),
                    extract_pattern: Some(r"Version\s+(.+)".to_string()),
                },
            ),
            (
                "player_name".to_string(),
                TextRegion {
                    x: 100,
                    y: 100,
                    width: 300,
                    height: 40,
                    expected_text: None,
                    extract_pattern: None,
                },
            ),
        ]),
        resource_files: vec![
            "game_data.json".to_string(),
            "player_stats.json".to_string(),
        ],
    };

    let config_json = serde_json::to_string_pretty(&default_config)?;
    fs::write(config_path, config_json)?;

    Ok(())
}
