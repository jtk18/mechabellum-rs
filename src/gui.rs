use anyhow::Result;

use iced::widget::{button, column, container, text};
use iced::{Element, Length, Task};
use rfd::FileDialog;

use std::path::PathBuf;

pub fn start_gui() -> Result<()> {
    iced::application("Mechabellum Replays", App::update, App::view).run()?;
    Ok(())
}

#[derive(Debug, Clone)]
pub enum Message {
    OpenFileDialog,
}

#[derive(Debug, Default)]
pub struct App {
    replay_dir: Option<PathBuf>,
}

impl App {
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::OpenFileDialog => {
                let pick = FileDialog::new().set_directory("/").pick_folder();
                if let Some(path) = pick {
                    self.replay_dir = Some(path);
                }

                Task::none()
            }
        }
    }

    fn view(&self) -> Element<Message> {
        let mut content = column![]
            .width(Length::Fill)
            .height(Length::Fill)
            .spacing(20);
        if self.replay_dir.is_some() {
            content = content.push(text(format!("You entered: {:?}", self.replay_dir.as_ref())));
        } else {
            let open_button =
                button(text("Select Replay Directory")).on_press(Message::OpenFileDialog);
            content = content.push(text("Welcome!"));
            content = content.push(text("Please select your replay directory to continue:"));
            content = content.push(open_button);
        }
        container(content)
            .center_x(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}
