use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use enigo::{Button, Coordinate, Direction, Enigo, Mouse, Settings};
use image::{DynamicImage, GenericImageView, ImageBuffer, Rgba};
use log::{info, warn};
use regex::Regex;
use screenshots::Screen;
use serde::{Deserialize, Serialize};
use serde_yaml;
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
use windows::Win32::Foundation::{LPARAM, WPARAM};
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowA, PostMessageA, SetForegroundWindow, ShowWindow, SW_RESTORE, WM_CLOSE,
};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Save debug screenshots to permanent files instead of temporary files
    #[arg(long, global = true)]
    save_screenshots: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the normal testing sequence
    Run,
    /// Add a new region to config with specified coordinates
    AddRegion {
        /// Name for the new region
        #[arg(short, long)]
        name: String,
        /// X coordinate
        #[arg(short = 'x', long)]
        x: i32,
        /// Y coordinate
        #[arg(short = 'y', long)]
        y: i32,
        /// Width
        #[arg(short = 'w', long)]
        width: i32,
        /// Height
        #[arg(short = 'H', long)]
        height: i32,
        /// Optional description for the region
        #[arg(short, long)]
        description: Option<String>,
        /// Optional expected text pattern
        #[arg(short, long)]
        expected_text: Option<String>,
        /// Optional regex pattern for text extraction
        #[arg(short = 'p', long)]
        pattern: Option<String>,
    },
    /// Add a new button to config with specified coordinates
    AddButton {
        /// Name for the new button
        #[arg(short, long)]
        name: String,
        /// X coordinate
        #[arg(short = 'x', long)]
        x: i32,
        /// Y coordinate
        #[arg(short = 'y', long)]
        y: i32,
        /// Width
        #[arg(short = 'w', long)]
        width: i32,
        /// Height
        #[arg(short = 'H', long)]
        height: i32,
        /// Optional description for the button
        #[arg(short, long)]
        description: Option<String>,
    },
    /// Find multiple template images within a screenshot and output add-region commands
    FindRegions {
        /// Path to the full screenshot image
        #[arg(short, long)]
        screenshot: String,
        /// Paths to template images to find (specify multiple times)
        #[arg(short, long)]
        template: Vec<String>,
    },
    /// Execute a test sequence from a YAML file
    ExecuteScript {
        /// Path to the YAML test script file
        #[arg(short, long)]
        script: String,
    },
    /// Crop a template from a larger screenshot
    CropTemplate {
        /// Path to the source screenshot image
        #[arg(short, long)]
        screenshot: String,
        /// Name for the cropped template file (without extension)
        #[arg(short, long)]
        name: String,
        /// X coordinate for cropping
        #[arg(short = 'x', long)]
        x: i32,
        /// Y coordinate for cropping
        #[arg(short = 'y', long)]
        y: i32,
        /// Width of the crop region
        #[arg(short = 'w', long)]
        width: i32,
        /// Height of the crop region
        #[arg(short = 'H', long)]
        height: i32,
    },
}

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

#[derive(Debug, Serialize, Deserialize)]
struct TestScript {
    name: String,
    steps: Vec<TestStep>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "action")]
enum TestStep {
    #[serde(rename = "screenshot")]
    Screenshot {
        name: String,
        description: Option<String>,
    },
    #[serde(rename = "click")]
    Click {
        target: String,
        description: Option<String>,
        wait: Option<u64>,
    },
    #[serde(rename = "read")]
    Read {
        target: String,
        description: Option<String>,
        save_to: Option<String>,
        key: Option<String>,
    },
    #[serde(rename = "verify")]
    Verify {
        target: String,
        expect: String,
        description: Option<String>,
    },
    #[serde(rename = "wait")]
    Wait {
        duration: u64,
        description: Option<String>,
    },
}

struct MechabellumTester {
    config: GameConfig,
    enigo: Enigo,
    save_screenshots: bool,
}

impl MechabellumTester {
    fn new(config_path: &str, save_screenshots: bool) -> Result<Self> {
        let config_content =
            fs::read_to_string(config_path).context("Failed to read config file")?;
        let config: GameConfig =
            serde_json::from_str(&config_content).context("Failed to parse config JSON")?;

        let enigo = Enigo::new(&Settings::default()).context("Failed to initialize Enigo")?;

        // Test tesseract installation
        Self::test_tesseract()?;

        Ok(MechabellumTester {
            config,
            enigo,
            save_screenshots,
        })
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
            let version_info = String::from_utf8_lossy(&output.stdout);
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

            if let Ok(handle) = hwnd {
                if !handle.is_invalid() {
                    let _ = ShowWindow(handle, SW_RESTORE);
                    let _ = SetForegroundWindow(handle);
                    info!("Focused game window: {}", self.config.window_title);
                } else {
                    warn!("Invalid window handle for: {}", self.config.window_title);
                }
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

    #[cfg(target_os = "windows")]
    fn close_game_window(&self) -> Result<()> {
        unsafe {
            let window_title = std::ffi::CString::new(self.config.window_title.clone())?;
            let hwnd = FindWindowA(PCSTR::null(), PCSTR(window_title.as_ptr() as *const u8));

            if let Ok(handle) = hwnd {
                if !handle.is_invalid() {
                    // Send WM_CLOSE message to gracefully close the window
                    let result = PostMessageA(handle, WM_CLOSE, WPARAM(0), LPARAM(0));
                    if result.is_ok() {
                        info!(
                            "Sent close message to game window: {}",
                            self.config.window_title
                        );
                        // Give the game a moment to close gracefully
                        thread::sleep(Duration::from_secs(3));
                    } else {
                        warn!("Failed to send close message to game window");
                    }
                } else {
                    warn!(
                        "Invalid game window handle for: {}",
                        self.config.window_title
                    );
                }
            } else {
                warn!(
                    "Could not find game window to close: {}",
                    self.config.window_title
                );
            }
        }
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    fn close_game_window(&self) -> Result<()> {
        info!("Window closing not implemented for non-Windows platforms");
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

        // Calculate center position of the button
        let center_x = button.x + button.width / 2;
        let center_y = button.y + button.height / 2;

        info!(
            "Clicking button: {} at center ({}, {}) of region ({}, {}) {}x{}",
            button_name, center_x, center_y, button.x, button.y, button.width, button.height
        );

        // Move mouse to button center position
        self.enigo
            .move_mouse(center_x, center_y, Coordinate::Abs)
            .context("Failed to move mouse")?;

        thread::sleep(Duration::from_millis(100));

        // Click the left mouse button
        self.enigo
            .button(Button::Left, Direction::Click)
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

        let output = if self.save_screenshots {
            // Save debug screenshots for troubleshooting
            let full_screenshot_path = format!("debug_full_screenshot_{}.png", region_name);
            screenshot
                .save(&full_screenshot_path)
                .context("Failed to save full screenshot")?;
            info!("Saved full screenshot to: {}", full_screenshot_path);

            // Save cropped region for debugging (permanent file)
            let cropped_path = format!("debug_cropped_region_{}.png", region_name);
            cropped
                .save(&cropped_path)
                .context("Failed to save cropped region")?;
            info!("Saved cropped region to: {}", cropped_path);

            // Use the permanent cropped file for tesseract
            Command::new("tesseract")
                .arg(&cropped_path)
                .arg("stdout")
                .arg("-l")
                .arg("eng")
                .output()
                .context("Failed to run tesseract command")?
        } else {
            // Use temporary file approach for normal operation
            let temp_file =
                NamedTempFile::with_suffix(".png").context("Failed to create temporary file")?;
            let temp_path = temp_file.path();

            cropped
                .save(temp_path)
                .context("Failed to save temporary image")?;

            // Run tesseract on the temporary file
            Command::new("tesseract")
                .arg(temp_path)
                .arg("stdout")
                .arg("-l")
                .arg("eng")
                .output()
                .context("Failed to run tesseract command")?
        };

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
        let content = if path.exists() {
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

        // Close the game window gracefully
        info!("Closing game window...");
        self.close_game_window()?;
        info!("Game window closed");

        Ok(())
    }

    fn execute_script(&mut self, script_path: &str) -> Result<()> {
        info!("Loading test script: {}", script_path);

        let script_content =
            fs::read_to_string(script_path).context("Failed to read test script file")?;

        let script: TestScript =
            serde_yaml::from_str(&script_content).context("Failed to parse test script YAML")?;

        info!("Executing test script: {}", script.name);

        // Launch the game
        self.launch_game()?;

        // Wait a bit more for the game to fully load
        thread::sleep(Duration::from_secs(10));

        // Execute steps with proper cleanup - use a closure to ensure cleanup happens
        let execution_result = (|| -> Result<()> {
            // Execute each step
            for (step_index, step) in script.steps.iter().enumerate() {
                info!("Executing step {}: {:?}", step_index + 1, step);

                match step {
                    TestStep::Screenshot { name, description } => {
                        if let Some(desc) = description {
                            info!("Taking screenshot '{}': {}", name, desc);
                        }
                        let screenshot = self.take_screenshot()?;
                        let screenshot_path = format!("script_screenshot_{}.png", name);
                        screenshot.save(&screenshot_path)?;
                        info!("Screenshot saved to: {}", screenshot_path);
                    }

                    TestStep::Click {
                        target,
                        description,
                        wait,
                    } => {
                        if let Some(desc) = description {
                            info!("Clicking '{}': {}", target, desc);
                        }
                        self.click_button(target)?;

                        if let Some(wait_ms) = wait {
                            thread::sleep(Duration::from_millis(*wait_ms));
                        }
                    }

                    TestStep::Read {
                        target,
                        description,
                        save_to,
                        key,
                    } => {
                        if let Some(desc) = description {
                            info!("Reading from '{}': {}", target, desc);
                        }
                        let text = self.extract_text_from_region(target)?;

                        if let (Some(file_path), Some(key_name)) = (save_to, key) {
                            self.update_resource_file(file_path, key_name, &text)?;
                        }
                    }

                    TestStep::Verify {
                        target,
                        expect,
                        description,
                    } => {
                        if let Some(desc) = description {
                            info!("Verifying '{}': {}", target, desc);
                        }
                        let success = self.verify_text(target, expect)?;
                        if !success {
                            return Err(anyhow::anyhow!(
                                "Verification failed for '{}': expected text '{}' not found",
                                target,
                                expect
                            ));
                        }
                    }

                    TestStep::Wait {
                        duration,
                        description,
                    } => {
                        if let Some(desc) = description {
                            info!("Waiting {} ms: {}", duration, desc);
                        } else {
                            info!("Waiting {} ms", duration);
                        }
                        thread::sleep(Duration::from_millis(*duration));
                    }
                }
            }
            Ok(())
        })();

        // ALWAYS close the game window, regardless of success or failure
        info!("Closing game window...");
        if let Err(close_error) = self.close_game_window() {
            warn!("Failed to close game window: {}", close_error);
        } else {
            info!("Game window closed");
        }

        // Handle the execution result after cleanup
        match execution_result {
            Ok(()) => {
                info!("Test script '{}' completed successfully", script.name);
                Ok(())
            }
            Err(e) => {
                warn!("Test script '{}' failed: {}", script.name, e);
                Err(e)
            }
        }
    }
}

// Template matching functionality
fn find_multiple_templates_in_image(
    screenshot_path: &str,
    template_paths: &[String],
) -> Result<Vec<Result<(u32, u32, u32, u32)>>> {
    info!("Loading screenshot: {}", screenshot_path);
    let screenshot = image::open(screenshot_path)
        .context("Failed to load screenshot image")?
        .to_rgba8();

    let (screenshot_width, screenshot_height) = screenshot.dimensions();
    info!(
        "Screenshot dimensions: {} x {}",
        screenshot_width, screenshot_height
    );

    // Load all templates
    let mut templates = Vec::new();
    for template_path in template_paths {
        info!("Loading template: {}", template_path);
        match image::open(template_path) {
            Ok(template_img) => {
                let template = template_img.to_rgba8();
                let (template_width, template_height) = template.dimensions();

                if template_width > screenshot_width || template_height > screenshot_height {
                    templates.push(Err(anyhow::anyhow!(
                        "Template image ({} x {}) is larger than screenshot ({} x {})",
                        template_width,
                        template_height,
                        screenshot_width,
                        screenshot_height
                    )));
                } else {
                    info!(
                        "Template '{}' loaded: {} x {}",
                        template_path, template_width, template_height
                    );
                    templates.push(Ok((template, template_width, template_height)));
                }
            }
            Err(e) => {
                templates.push(Err(anyhow::anyhow!(
                    "Failed to load template '{}': {}",
                    template_path,
                    e
                )));
            }
        }
    }

    // Initialize best match tracking for each template
    let mut best_matches: Vec<Option<(f64, u32, u32, u32, u32)>> = vec![None; templates.len()];

    info!(
        "Starting single-pass template matching across {} templates",
        templates.len()
    );

    // Single pass through the screenshot
    for y in 0..screenshot_height {
        for x in 0..screenshot_width {
            // Check each template at this position
            for (template_idx, template_result) in templates.iter().enumerate() {
                if let Ok((template, template_width, template_height)) = template_result {
                    // Check if template fits at this position
                    if x + template_width <= screenshot_width
                        && y + template_height <= screenshot_height
                    {
                        let score = calculate_match_score(&screenshot, template, x, y);

                        // Update best match if this is better
                        match &best_matches[template_idx] {
                            None => {
                                best_matches[template_idx] =
                                    Some((score, x, y, *template_width, *template_height));
                            }
                            Some((best_score, _, _, _, _)) => {
                                if score < *best_score {
                                    best_matches[template_idx] =
                                        Some((score, x, y, *template_width, *template_height));
                                }
                            }
                        }
                    }
                }
            }
        }

        // Progress indicator for large images
        if y % 50 == 0 {
            info!("Progress: {} / {} rows", y, screenshot_height);
        }
    }

    // Convert results
    let mut results = Vec::new();
    for (template_idx, template_result) in templates.iter().enumerate() {
        match template_result {
            Ok(_) => match &best_matches[template_idx] {
                Some((score, x, y, width, height)) => {
                    if *score < 50.0 {
                        info!(
                            "Template {} best match at ({}, {}) with score: {:.2}",
                            template_idx, x, y, score
                        );
                        results.push(Ok((*x, *y, *width, *height)));
                    } else {
                        results.push(Err(anyhow::anyhow!(
                                "No good match found for template {}. Best score was {:.2} (threshold: 50.0)",
                                template_idx, score
                            )));
                    }
                }
                None => {
                    results.push(Err(anyhow::anyhow!(
                        "No match found for template {}",
                        template_idx
                    )));
                }
            },
            Err(e) => {
                results.push(Err(anyhow::anyhow!(
                    "Template {} error: {}",
                    template_idx,
                    e
                )));
            }
        }
    }

    Ok(results)
}

fn find_template_in_image(
    screenshot_path: &str,
    template_path: &str,
) -> Result<(u32, u32, u32, u32)> {
    info!("Loading screenshot: {}", screenshot_path);
    let screenshot = image::open(screenshot_path)
        .context("Failed to load screenshot image")?
        .to_rgba8();

    info!("Loading template: {}", template_path);
    let template = image::open(template_path)
        .context("Failed to load template image")?
        .to_rgba8();

    let (screenshot_width, screenshot_height) = screenshot.dimensions();
    let (template_width, template_height) = template.dimensions();

    if template_width > screenshot_width || template_height > screenshot_height {
        return Err(anyhow::anyhow!(
            "Template image ({} x {}) is larger than screenshot ({} x {})",
            template_width,
            template_height,
            screenshot_width,
            screenshot_height
        ));
    }

    info!(
        "Searching for template ({} x {}) in screenshot ({} x {})",
        template_width, template_height, screenshot_width, screenshot_height
    );

    let mut best_match_score = f64::MAX;
    let mut best_match_pos = (0u32, 0u32);

    // Slide the template across the screenshot
    for y in 0..=(screenshot_height - template_height) {
        for x in 0..=(screenshot_width - template_width) {
            let score = calculate_match_score(&screenshot, &template, x, y);

            if score < best_match_score {
                best_match_score = score;
                best_match_pos = (x, y);
            }
        }

        // Progress indicator for large images
        if y % 50 == 0 {
            info!(
                "Progress: {} / {} rows",
                y,
                screenshot_height - template_height
            );
        }
    }

    info!(
        "Best match found at position ({}, {}) with score: {:.2}",
        best_match_pos.0, best_match_pos.1, best_match_score
    );

    // If the match score is reasonable (lower is better), return the coordinates
    if best_match_score < 50.0 {
        // Threshold for acceptable match
        Ok((
            best_match_pos.0,
            best_match_pos.1,
            template_width,
            template_height,
        ))
    } else {
        Err(anyhow::anyhow!(
            "No good match found. Best score was {:.2} (threshold: 50.0)",
            best_match_score
        ))
    }
}

fn calculate_match_score(
    screenshot: &ImageBuffer<Rgba<u8>, Vec<u8>>,
    template: &ImageBuffer<Rgba<u8>, Vec<u8>>,
    start_x: u32,
    start_y: u32,
) -> f64 {
    let (template_width, template_height) = template.dimensions();
    let mut total_diff = 0f64;
    let mut pixel_count = 0;

    for y in 0..template_height {
        for x in 0..template_width {
            let screenshot_pixel = screenshot.get_pixel(start_x + x, start_y + y);
            let template_pixel = template.get_pixel(x, y);

            // Calculate RGB difference (ignoring alpha)
            let r_diff = (screenshot_pixel[0] as f64 - template_pixel[0] as f64).abs();
            let g_diff = (screenshot_pixel[1] as f64 - template_pixel[1] as f64).abs();
            let b_diff = (screenshot_pixel[2] as f64 - template_pixel[2] as f64).abs();

            total_diff += r_diff + g_diff + b_diff;
            pixel_count += 1;
        }
    }

    total_diff / pixel_count as f64
}

fn add_region_to_config(
    config_path: &str,
    region_name: &str,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    _description: Option<String>,
    expected_text: Option<String>,
    pattern: Option<String>,
) -> Result<()> {
    info!(
        "Adding region '{}' at ({}, {}) {}x{} to config",
        region_name, x, y, width, height
    );

    // Load existing config or create new one
    let mut config = if Path::new(config_path).exists() {
        let config_content =
            fs::read_to_string(config_path).context("Failed to read existing config file")?;
        serde_json::from_str::<GameConfig>(&config_content)
            .context("Failed to parse existing config JSON")?
    } else {
        // Create a basic config if none exists
        GameConfig {
            steam_app_id: "669330".to_string(),
            window_title: "Mechabellum".to_string(),
            buttons: HashMap::new(),
            text_regions: HashMap::new(),
            resource_files: vec!["game_data.json".to_string()],
        }
    };

    // Add the new region
    let new_region = TextRegion {
        x: x as i32,
        y: y as i32,
        width: width as i32,
        height: height as i32,
        expected_text,
        extract_pattern: pattern,
    };

    config
        .text_regions
        .insert(region_name.to_string(), new_region);

    // Save updated config
    let config_json =
        serde_json::to_string_pretty(&config).context("Failed to serialize updated config")?;
    fs::write(config_path, config_json).context("Failed to write updated config file")?;

    info!(
        "Successfully added region '{}' to config file: {}",
        region_name, config_path
    );
    Ok(())
}

fn add_button_to_config(
    config_path: &str,
    button_name: &str,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    description: Option<String>,
) -> Result<()> {
    info!(
        "Adding button '{}' at ({}, {}) {}x{} to config",
        button_name, x, y, width, height
    );

    // Load existing config or create new one
    let mut config = if Path::new(config_path).exists() {
        let config_content =
            fs::read_to_string(config_path).context("Failed to read existing config file")?;
        serde_json::from_str::<GameConfig>(&config_content)
            .context("Failed to parse existing config JSON")?
    } else {
        // Create a basic config if none exists
        GameConfig {
            steam_app_id: "669330".to_string(),
            window_title: "Mechabellum".to_string(),
            buttons: HashMap::new(),
            text_regions: HashMap::new(),
            resource_files: vec!["game_data.json".to_string()],
        }
    };

    // Add the new button
    let new_button = ButtonConfig {
        x: x as i32,
        y: y as i32,
        width: width as i32,
        height: height as i32,
        description: description.unwrap_or_else(|| format!("Button at ({}, {})", x, y)),
    };

    config.buttons.insert(button_name.to_string(), new_button);

    // Save updated config
    let config_json =
        serde_json::to_string_pretty(&config).context("Failed to serialize updated config")?;
    fs::write(config_path, config_json).context("Failed to write updated config file")?;

    info!(
        "Successfully added button '{}' to config file: {}",
        button_name, config_path
    );
    Ok(())
}

fn save_marked_screenshot(
    screenshot_path: &str,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    output_path: &str,
) -> Result<()> {
    let mut screenshot = image::open(screenshot_path)
        .context("Failed to load screenshot for marking")?
        .to_rgba8();

    // Draw a red rectangle around the found region
    let red = Rgba([255u8, 0u8, 0u8, 255u8]);

    // Draw top and bottom borders
    for px in x..=(x + width) {
        if px < screenshot.width() {
            if y < screenshot.height() {
                screenshot.put_pixel(px, y, red);
            }
            if y + height < screenshot.height() {
                screenshot.put_pixel(px, y + height, red);
            }
        }
    }

    // Draw left and right borders
    for py in y..=(y + height) {
        if py < screenshot.height() {
            if x < screenshot.width() {
                screenshot.put_pixel(x, py, red);
            }
            if x + width < screenshot.width() {
                screenshot.put_pixel(x + width, py, red);
            }
        }
    }

    screenshot
        .save(output_path)
        .context("Failed to save marked screenshot")?;

    Ok(())
}

fn crop_template_from_screenshot(
    screenshot_path: &str,
    template_name: &str,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
) -> Result<()> {
    info!(
        "Cropping template '{}' from screenshot '{}' at ({}, {}) with size {}x{}",
        template_name, screenshot_path, x, y, width, height
    );

    let screenshot = image::open(screenshot_path).context("Failed to load screenshot")?;

    let (screenshot_width, screenshot_height) = screenshot.dimensions();

    // Validate crop region
    if x + width > screenshot_width || y + height > screenshot_height {
        return Err(anyhow::anyhow!(
            "Crop region ({}, {}) {}x{} exceeds screenshot dimensions {}x{}",
            x,
            y,
            width,
            height,
            screenshot_width,
            screenshot_height
        ));
    }

    // Crop the region
    let cropped = screenshot.crop_imm(x, y, width, height);

    // Save the cropped template
    let template_path = format!("{}.png", template_name);
    cropped
        .save(&template_path)
        .context("Failed to save cropped template")?;

    info!("Successfully saved cropped template to: {}", template_path);
    Ok(())
}

fn main() -> Result<()> {
    env_logger::init();

    let cli = Cli::parse();

    match cli.command {
        Some(Commands::AddRegion {
            name,
            x,
            y,
            width,
            height,
            description,
            expected_text,
            pattern,
        }) => {
            info!(
                "Adding region '{}' with coordinates ({}, {}) {}x{}",
                name, x, y, width, height
            );

            // Add region to config
            let config_path = "config.json";
            add_region_to_config(
                config_path,
                &name,
                x as u32,
                y as u32,
                width as u32,
                height as u32,
                description,
                expected_text,
                pattern,
            )?;

            info!("Region '{}' successfully added to config!", name);
        }
        Some(Commands::AddButton {
            name,
            x,
            y,
            width,
            height,
            description,
        }) => {
            info!(
                "Adding button '{}' with coordinates ({}, {}) {}x{}",
                name, x, y, width, height
            );

            // Add button to config
            let config_path = "config.json";
            add_button_to_config(
                config_path,
                &name,
                x as u32,
                y as u32,
                width as u32,
                height as u32,
                description,
            )?;

            info!("Button '{}' successfully added to config!", name);
        }
        Some(Commands::FindRegions {
            screenshot,
            template,
        }) => {
            info!(
                "Finding {} templates in screenshot '{}'",
                template.len(),
                screenshot
            );

            // Find all templates in a single pass
            let results = find_multiple_templates_in_image(&screenshot, &template)?;

            // Process results
            for (template_path, result) in template.iter().zip(results.iter()) {
                // Extract filename without extension as the region name
                let template_name = Path::new(template_path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown");

                match result {
                    Ok((x, y, width, height)) => {
                        info!(
                            "Template '{}' found at position: ({}, {}) with size: {}x{}",
                            template_name, x, y, width, height
                        );

                        // Output the add-region command for this template
                        println!(
                            "cargo run -- add-region --name {} --x {} --y {} --width {} --height {}",
                            template_name, x, y, width, height
                        );

                        // Save a marked version of the screenshot showing the found region
                        if cli.save_screenshots {
                            let marked_screenshot_path =
                                format!("marked_screenshot_{}.png", template_name);
                            save_marked_screenshot(
                                &screenshot,
                                *x,
                                *y,
                                *width,
                                *height,
                                &marked_screenshot_path,
                            )?;
                            info!("Saved marked screenshot to: {}", marked_screenshot_path);
                        }
                    }
                    Err(e) => {
                        warn!(
                            "Failed to find template '{}' in screenshot: {}",
                            template_name, e
                        );
                    }
                }
            }
        }
        Some(Commands::ExecuteScript { script }) => {
            info!("Executing test script: {}", script);

            let config_path = "config.json";
            if !Path::new(config_path).exists() {
                create_default_config(config_path)?;
                info!("Created default config file: {}", config_path);
                info!("Please edit the config file and run again");
                return Ok(());
            }

            let mut tester = MechabellumTester::new(config_path, cli.save_screenshots)?;
            tester.execute_script(&script)?;
        }
        Some(Commands::CropTemplate {
            screenshot,
            name,
            x,
            y,
            width,
            height,
        }) => {
            info!(
                "Cropping template '{}' from screenshot '{}' at ({}, {}) with size {}x{}",
                name, screenshot, x, y, width, height
            );

            crop_template_from_screenshot(
                &screenshot,
                &name,
                x as u32,
                y as u32,
                width as u32,
                height as u32,
            )?;

            info!("Template '{}' successfully cropped and saved!", name);
        }
        Some(Commands::Run) | None => {
            info!("Starting Mechabellum Tester");

            let config_path = "config.json";
            if !Path::new(config_path).exists() {
                create_default_config(config_path)?;
                info!("Created default config file: {}", config_path);
                info!("Please edit the config file and run again");
                return Ok(());
            }

            let mut tester = MechabellumTester::new(config_path, cli.save_screenshots)?;
            tester.run_test_sequence()?;
        }
    }

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
