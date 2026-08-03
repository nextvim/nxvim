use std::{error::Error, fmt, io};

use crate::{Editor, EditorError, EventSource, Presenter};

pub struct Application<S, P> {
    editor: Editor,
    events: S,
    presenter: P,
}

impl<S: EventSource, P: Presenter> Application<S, P> {
    pub const fn new(editor: Editor, events: S, presenter: P) -> Self {
        Self {
            editor,
            events,
            presenter,
        }
    }

    pub fn editor(&self) -> &Editor {
        &self.editor
    }

    pub fn run(&mut self) -> Result<(), AppError> {
        self.draw()?;
        while self.editor.is_running() {
            let event = self.events.next_event()?;
            for command in self.editor.take_script_commands() {
                self.editor.apply_host_command(command)?;
                if !self.editor.is_running() {
                    break;
                }
            }
            if self.editor.is_running() {
                self.editor.handle_event(event)?;
            }
            if self.editor.is_running() {
                self.draw()?;
            }
        }
        Ok(())
    }

    fn draw(&mut self) -> Result<(), AppError> {
        let frame = self.editor.frame()?;
        self.presenter.draw(frame)?;
        Ok(())
    }
}

#[derive(Debug)]
pub enum AppError {
    Io(io::Error),
    Editor(EditorError),
}

impl fmt::Display for AppError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "terminal I/O error: {error}"),
            Self::Editor(error) => error.fmt(formatter),
        }
    }
}

impl Error for AppError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Editor(error) => Some(error),
        }
    }
}

impl From<io::Error> for AppError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<EditorError> for AppError {
    fn from(error: EditorError) -> Self {
        Self::Editor(error)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use crate::{AppEvent, Lifecycle, NoopPresenter, ScreenSize};

    use super::*;

    struct FakeEvents(VecDeque<AppEvent>);

    impl EventSource for FakeEvents {
        fn next_event(&mut self) -> io::Result<AppEvent> {
            Ok(self.0.pop_front().unwrap_or(AppEvent::EndOfInput))
        }
    }

    #[test]
    fn fake_events_drive_resize_then_clean_exit() {
        let editor = Editor::new(ScreenSize::new(80, 24)).unwrap();
        let events = FakeEvents(VecDeque::from([
            AppEvent::Resize(ScreenSize::new(100, 30)),
            AppEvent::EndOfInput,
        ]));
        let mut app = Application::new(editor, events, NoopPresenter);

        app.run().unwrap();

        assert_eq!(app.editor().screen(), ScreenSize::new(100, 30));
        assert_eq!(app.editor().lifecycle(), Lifecycle::ExitRequested);
    }
}
