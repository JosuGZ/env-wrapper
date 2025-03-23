use std::io::{self, stdin, stdout, Read, Write};
use std::env::var;
use std::os::fd::AsRawFd;
use std::process::{Command, Stdio};
use std::thread::{sleep, JoinHandle};
use std::fs::File;
use std::time::Duration;

use termios::{Termios, tcsetattr, TCSANOW, ICANON, ECHO};

mod pseudoterminal;
use pseudoterminal::*;

fn main() {
  run_in_pseudoterminal(master, slave);
}

/// Disables echo and allows to read characters without waiting for and end of
/// line
fn configure_terminal() -> Termios {
  let stdin = libc::STDIN_FILENO;
  let mut termios = Termios::from_fd(stdin).unwrap();
  termios.c_lflag &= !(ICANON | ECHO); // no echo and canonical mode
  tcsetattr(stdin, TCSANOW, &termios).unwrap();
  termios
}

/// Pipes data from a reader to a writer, spawning a thread.
pub fn pipe<Reader, Writer>(mut from: Reader, mut to: Writer) -> JoinHandle<io::Error>
  where
    Reader: Read + Send + 'static,
    Writer: Write + Send + 'static
{
  std::thread::spawn(move || {
    let mut buf = [0u8; 1];
    let buf = &mut buf;
    loop {
      match from.read_exact(buf) {
        Ok(()) => {
          let data = &buf[0..1];
          if let Err(error) = to.write_all(data) { break error; }
          if let Err(error) = to.flush() { break error; }
        }
        Err(error) => break error
      }
    }
  })
}

/// Here we handle the terminal. All the data from the slave will be output to
/// stdout, but data from stdin will be altered to replace environment
/// variables.
fn master(master_file_descriptor: File) {
  enum State {
    ForwardingInput,
    ReadingVariable
  }

  configure_terminal();

  let mut buf = [0_u8; 1];
  let mut state = State::ForwardingInput;
  let mut variable_name = String::new();
  let mut slave_sink = master_file_descriptor;
  let slave_source = slave_sink.try_clone().unwrap();

  // We forward from the slave to the terminal
  pipe(slave_source, stdout());

  /// Outputs a char and flushes it
  fn output(c: char) {
    print!("{c}");
    stdout().flush().unwrap();
  }

  /// Deletes n characters from the terminal
  fn delete(n: usize) {
    print!("\x1B[{n}D\x1B[K");
    stdout().flush().unwrap();
  }

  let mfd = slave_sink.as_raw_fd();
  resize_terminal(libc::STDIN_FILENO, mfd);
  start_window_resize_task(libc::STDIN_FILENO, mfd);

  // We send data from the terminal to the slave, after performing subtitutions
  loop {
    stdin().read_exact(&mut buf).unwrap();
    let readed = buf[0] as char;

    match state {

      State::ForwardingInput => {
        match readed {
          '$' => {
            output('$');
            variable_name.clear();
            state = State::ReadingVariable;
          },
          _ => {
            slave_sink.write_all(&buf).unwrap();
            slave_sink.flush().unwrap();
          }
        }
      }

      State::ReadingVariable => {
        match readed {
          '{' => output('{'),
          '}' => {
            output(readed);
            let replacement_length = 3 + variable_name.len(); // TODO: Review pattern, handle things like ${{{}}}
            delete(replacement_length);

            let variable_value = var(&variable_name).unwrap_or_default();
            slave_sink.write_all(variable_value.as_bytes()).unwrap();
            slave_sink.flush().unwrap();

            state = State::ForwardingInput;
          },
          '\x7F' => { // Handle deletion, TODO: Handle begin of line, arrows...
            if !variable_name.is_empty() {
              delete(1);
              variable_name.remove(variable_name.len() - 1);
            }
          }
          character => {
            output(character);
            variable_name.push(character);
          }
        }
      }
    }
  }
}

/// As the slave, for now we just run "screen" (it has to be installed). Screen
/// will receive altered data from the master side of the pseudoterminal.
fn slave() {
  let command_name = std::env::args().nth(1).unwrap_or("screen".into());

  let mut command = Command::new(command_name);
  command.stdin(Stdio::inherit());
  command.stdout(Stdio::inherit());
  command.stderr(Stdio::inherit());
  command.spawn().unwrap().wait().unwrap();
}

/// This checks if the window has been resized, to forward it to the slave. It
/// does it every second. This should be donw with the SIGWINCH terminal but
/// aparently doesn't work with the integrated terminal form VS Code.
fn start_window_resize_task(fd_from: i32, fd_to: i32) {
  std::thread::spawn(move || {
    loop {
      resize_terminal(fd_from, fd_to);
      sleep(Duration::from_secs(1));
    }
  });
}
