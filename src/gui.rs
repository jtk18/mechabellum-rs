use anyhow::Result;

use iced::{Element, Task};
use iced::widget::{column, text};

pub fn start_gui() -> Result<()> {
    iced::application("Mechabellum Replays", App::update, App::view)
        .run()?;
    Ok(())
}

#[derive(Debug, Clone)]
pub enum Message {
    SplashScreen,
}

#[derive(Debug, Default)]
pub struct App {

}

impl App {
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::SplashScreen => Task::none(),
        }
    }

    fn view(&self) -> Element<Message> {
        column![
            text("Welcome!")
        ].into()
    }
}