use std::{error::Error, sync::Arc};

use greetd_ipc::Request;
use termion::event::Key;
use tokio::sync::RwLock;

use crate::{
  event::{Event, Events},
  info::write_last_session,
  ipc::Ipc,
  power::power,
  ui::POWER_OPTIONS,
  Greeter, Mode,
};

pub async fn handle(greeter: Arc<RwLock<Greeter>>, events: &mut Events, ipc: Ipc) -> Result<(), Box<dyn Error>> {
  if let Some(Event::Input(input)) = events.next().await {
    let mut greeter = greeter.write().await;

    match input {
      Key::Esc => {
        Ipc::cancel(&mut greeter).await;
        greeter.reset().await;
      }

      Key::Left => greeter.cursor_offset -= 1,
      Key::Right => greeter.cursor_offset += 1,

      Key::F(2) => {
        greeter.previous_mode = match greeter.mode {
          Mode::Command | Mode::Sessions | Mode::Power => greeter.previous_mode,
          _ => greeter.mode,
        };

        greeter.new_command = greeter.command.clone().unwrap_or_default();
        greeter.mode = Mode::Command;
      }

      Key::F(3) => {
        greeter.previous_mode = match greeter.mode {
          Mode::Command | Mode::Sessions | Mode::Power => greeter.previous_mode,
          _ => greeter.mode,
        };

        greeter.mode = Mode::Sessions;
      }

      Key::F(12) => {
        greeter.previous_mode = match greeter.mode {
          Mode::Command | Mode::Sessions | Mode::Power => greeter.previous_mode,
          _ => greeter.mode,
        };

        greeter.mode = Mode::Power;
      }

      Key::Up => {
        if let Mode::Sessions = greeter.mode {
          if greeter.selected_session > 0 {
            greeter.selected_session -= 1;
          }
        }

        if let Mode::Power = greeter.mode {
          if greeter.selected_power_option > 0 {
            greeter.selected_power_option -= 1;
          }
        }
      }

      Key::Down => {
        if let Mode::Sessions = greeter.mode {
          if greeter.selected_session < greeter.sessions.len() - 1 {
            greeter.selected_session += 1;
          }
        }

        if let Mode::Power = greeter.mode {
          if greeter.selected_power_option < POWER_OPTIONS.len() - 1 {
            greeter.selected_power_option += 1;
          }
        }
      }

      Key::Ctrl('a') => {
        let value = {
          match greeter.mode {
            Mode::Username => greeter.username.clone(),
            _ => greeter.answer.clone(),
          }
        };

        greeter.cursor_offset = -(value.chars().count() as i16);
      }

      Key::Ctrl('e') => greeter.cursor_offset = 0,

      Key::Char('\n') | Key::Char('\t') => match greeter.mode {
        Mode::Username => {
          greeter.working = true;
          greeter.message = None;

          ipc.send(Request::CreateSession { username: greeter.username.clone() }).await;
          greeter.answer = String::new();
        }

        Mode::Password => {
          greeter.working = true;
          greeter.message = None;

          ipc
            .send(Request::PostAuthMessageResponse {
              response: Some(greeter.answer.clone()),
            })
            .await;

          greeter.answer = String::new();
        }

        Mode::Command => {
          let cmd = greeter.command.clone();

          greeter.command = Some(greeter.new_command.clone());
          greeter.selected_session = greeter.sessions.iter().position(|(_, command)| Some(command) == cmd.as_ref()).unwrap_or(0);

          if greeter.remember_session {
            write_last_session(&greeter.new_command);
          }

          greeter.mode = greeter.previous_mode;
        }

        Mode::Sessions => {
          let session = match greeter.sessions.get(greeter.selected_session) {
            Some((_, command)) => Some(command.clone()),
            _ => None,
          };

          if let Some(command) = session {
            greeter.command = Some(command.clone());

            if greeter.remember_session {
              write_last_session(&command);
            }
          }

          greeter.mode = greeter.previous_mode;
        }

        Mode::Power => {
          if let Some((option, _)) = POWER_OPTIONS.get(greeter.selected_power_option) {
            match power(&greeter, *option) {
              Ok(status) if status.success() => {}
              Ok(status) => greeter.message = Some(format!("Command exited with {}", status)),
              Err(err) => greeter.message = Some(format!("Command failed: {}", err)),
            }
          }

          greeter.mode = greeter.previous_mode;
        }
      },

      Key::Char(c) => insert_key(&mut greeter, c).await,

      Key::Backspace | Key::Delete => delete_key(&mut greeter, input).await,

      Key::Ctrl('u') => match greeter.mode {
        Mode::Username => greeter.username = String::new(),
        Mode::Password => greeter.answer = String::new(),
        Mode::Command => greeter.new_command = String::new(),
        _ => {}
      },

      #[cfg(debug_assertions)]
      Key::Ctrl('x') => {
        use crate::greeter::AuthStatus;

        crate::exit(&mut greeter, AuthStatus::Cancel).await;
      }

      _ => {}
    }
  }

  Ok(())
}

async fn insert_key(greeter: &mut Greeter, c: char) {
  let value = match greeter.mode {
    Mode::Username => greeter.username.clone(),
    Mode::Password => greeter.answer.clone(),
    Mode::Command => greeter.new_command.clone(),
    Mode::Sessions | Mode::Power => return,
  };

  let index = (value.chars().count() as i16 + greeter.cursor_offset) as usize;
  let left = value.chars().take(index);
  let right = value.chars().skip(index);

  let value = left.chain(vec![c].into_iter()).chain(right).collect();
  let mode = greeter.mode;

  match mode {
    Mode::Username => greeter.username = value,
    Mode::Password => greeter.answer = value,
    Mode::Command => greeter.new_command = value,
    _ => {}
  };
}

async fn delete_key(greeter: &mut Greeter, key: Key) {
  let value = match greeter.mode {
    Mode::Username => greeter.username.clone(),
    Mode::Password => greeter.answer.clone(),
    Mode::Command => greeter.new_command.clone(),
    Mode::Sessions | Mode::Power => return,
  };

  let index = match key {
    Key::Backspace => (value.chars().count() as i16 + greeter.cursor_offset - 1) as usize,
    Key::Delete => (value.chars().count() as i16 + greeter.cursor_offset) as usize,
    _ => 0,
  };

  if value.chars().nth(index as usize).is_some() {
    let left = value.chars().take(index);
    let right = value.chars().skip(index + 1);

    let value = left.chain(right).collect();

    match greeter.mode {
      Mode::Username => greeter.username = value,
      Mode::Password => greeter.answer = value,
      Mode::Command => greeter.new_command = value,
      Mode::Sessions | Mode::Power => return,
    };

    if let Key::Delete = key {
      greeter.cursor_offset += 1;
    }
  }
}
