use std::env;
use std::ffi::CString;
use std::os::fd::{AsFd, AsRawFd, OwnedFd};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;

use nix::pty::{ForkptyResult, Winsize, forkpty};
use nix::sys::wait::waitpid;
use nix::unistd::{execvp, read, write};

pub struct TerminalSession {
    master_fd: Arc<OwnedFd>,
    receiver: Mutex<Receiver<Vec<u8>>>,
}

impl TerminalSession {
    pub fn spawn() -> Result<Self, String> {
        let shell = default_shell_path();
        let shell_cstr = CString::new(shell.as_str())
            .map_err(|_| "shell path contains an interior null byte".to_string())?;
        let argv = vec![
            shell_cstr.clone(),
            CString::new("-l").expect("static string is valid"),
        ];
        let winsize = Winsize {
            ws_row: 32,
            ws_col: 100,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };

        let fork_result = unsafe { forkpty(Some(&winsize), None) }
            .map_err(|error| format!("failed to create PTY: {error}"))?;

        match fork_result {
            ForkptyResult::Child => {
                let error = execvp(&shell_cstr, &argv).expect_err("execvp only returns on error");
                Err(format!("failed to exec shell: {error}"))
            }
            ForkptyResult::Parent { master, child } => {
                let master_fd = Arc::new(master);
                let (sender, receiver) = mpsc::channel();
                spawn_reader_loop(Arc::clone(&master_fd), sender);
                spawn_reaper(child);

                Ok(Self {
                    master_fd,
                    receiver: Mutex::new(receiver),
                })
            }
        }
    }

    pub fn try_read(&self) -> Vec<Vec<u8>> {
        let mut chunks = Vec::new();
        let Ok(receiver) = self.receiver.lock() else {
            return chunks;
        };

        while let Ok(chunk) = receiver.try_recv() {
            chunks.push(chunk);
        }

        chunks
    }

    pub fn write_input(&self, bytes: &[u8]) {
        let _ = write(self.master_fd.as_fd(), bytes);
    }

    pub fn resize(&self, rows: u16, cols: u16, pixel_width: u16, pixel_height: u16) {
        let winsize = Winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: pixel_width,
            ws_ypixel: pixel_height,
        };

        let _ = unsafe { libc::ioctl(self.master_fd.as_raw_fd(), libc::TIOCSWINSZ, &winsize) };
    }
}

fn default_shell_path() -> String {
    env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string())
}

fn spawn_reader_loop(master_fd: Arc<OwnedFd>, sender: mpsc::Sender<Vec<u8>>) {
    thread::spawn(move || {
        let mut buffer = [0_u8; 4096];

        loop {
            match read(master_fd.as_fd(), &mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    if sender.send(buffer[..count].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
}

fn spawn_reaper(child: nix::unistd::Pid) {
    thread::spawn(move || {
        let _ = waitpid(child, None);
    });
}
