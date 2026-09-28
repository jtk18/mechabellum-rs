//! Recognizing which screen the game shows, from a reference-size screenshot. Each check is the
//! mean color of a small region that differs clearly between screens (measured 2026-09-28).

use crate::mean_color;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    /// Testing Grounds sandbox, between rounds: units can be placed.
    Deploying,
    /// Testing Grounds sandbox, a round is being fought.
    Battle,
    /// The in-game menu (Close, Return to lobby, Save replay...) over the sandbox.
    Menu,
    /// The Testing Grounds presets, with "Enter the Testing Grounds".
    Lobby,
    /// Standard Versus with another tab open (Multiplayer, Solo cards...): the orange mode
    /// button at the top left, but no Enter button.
    ModeTabs,
    /// The main menu: Standard Versus and Star Expedition cards, no mode button.
    MainMenu,
    /// Anything else: the main menu, loading, another tab.
    Other,
}

/// The START / PAUSE BATTLE banner at the top is gold in the sandbox.
fn banner_gold(c: [f64; 3]) -> bool {
    c[0] > 110.0 && c[1] > 80.0 && c[2] < 75.0 && c[0] > c[2] + 50.0
}

/// The orange "Standard Versus" mode button in the lobby's top-left corner.
fn orange_mode(c: [f64; 3]) -> bool {
    c[0] > 55.0 && c[0] > c[2] + 20.0
}

/// Navy button fill (Testing Grounds Tools label, Enter button, menu buttons).
fn navy(c: [f64; 3]) -> bool {
    c[2] > c[0] + 25.0 && c[2] > 55.0
}

pub fn classify(img: &image::RgbImage) -> Screen {
    let banner = mean_color(img, (1100, 60, 1460, 120));
    let tools_label = mean_color(img, (2260, 860, 2550, 892));
    let enter_button = mean_color(img, (2140, 1440, 2480, 1500));
    let menu_close = mean_color(img, (1090, 560, 1470, 610));
    let mode_button = mean_color(img, (60, 40, 500, 100));
    let main_caption = mean_color(img, (90, 700, 960, 760));
    if banner_gold(banner) {
        // The open tools panel covers the label, and it only opens while deploying.
        if navy(tools_label) || crate::tools_open(img) {
            Screen::Deploying
        } else {
            Screen::Battle
        }
    } else if navy(enter_button) && orange_mode(mode_button) {
        Screen::Lobby
    } else if orange_mode(mode_button) {
        Screen::ModeTabs
    } else if main_caption[2] > main_caption[0] && main_caption[0] > 35.0 && mode_button[0] < 40.0 {
        Screen::MainMenu
    } else if menu_close[2] > menu_close[0] + 25.0 && banner[0] > banner[2] {
        // Dimmed gold banner behind the menu, navy Close button in front.
        Screen::Menu
    } else {
        Screen::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Region = ((u32, u32, u32, u32), [u8; 3]);

    fn paint(regions: &[Region]) -> image::RgbImage {
        let mut img = image::RgbImage::from_pixel(2560, 1600, image::Rgb([20, 20, 20]));
        for &((l, t, r, b), c) in regions {
            for y in t..b {
                for x in l..r {
                    img.put_pixel(x, y, image::Rgb(c));
                }
            }
        }
        img
    }

    #[test]
    fn test_classify_measured_colors() {
        let gold = [137, 103, 51];
        let navy_label = [45, 55, 98];
        let brown = [76, 49, 37];
        assert_eq!(
            classify(&paint(&[
                ((1100, 60, 1460, 120), gold),
                ((2260, 860, 2550, 892), navy_label)
            ])),
            Screen::Deploying
        );
        assert_eq!(
            classify(&paint(&[
                ((1100, 60, 1460, 120), gold),
                ((2260, 860, 2550, 892), brown)
            ])),
            Screen::Battle
        );
        assert_eq!(
            classify(&paint(&[
                ((1100, 60, 1460, 120), [40, 45, 67]),
                ((2140, 1440, 2480, 1500), [47, 57, 98]),
                ((60, 40, 500, 100), [67, 52, 36])
            ])),
            Screen::Lobby
        );
        assert_eq!(
            classify(&paint(&[
                ((1100, 60, 1460, 120), [59, 43, 18]),
                ((1090, 560, 1470, 610), [27, 35, 73])
            ])),
            Screen::Menu
        );
        assert_eq!(classify(&paint(&[])), Screen::Other);
    }
}
