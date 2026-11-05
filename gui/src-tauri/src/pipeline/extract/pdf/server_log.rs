//! What the managed llama-server last said. Keeps a bounded tail of its error
//! output, so a card that runs out of memory or is lost mid-run can be named.

use super::super::core::ANSI_ESCAPE_SEQUENCE;
use std::collections::VecDeque;
use std::io::{BufRead as _, BufReader, Read};
use std::sync::{Arc, Mutex};

const KEPT_LINES: usize = 200;
const LINE_BYTES: u64 = 2048;
/// How much of it a failure message shows.
const REASON_LINES: usize = 8;
const FAILURE_WORDS: [&str; 8] = [
    "error",
    "fail",
    "out of memory",
    "device lost",
    "exception",
    "abort",
    "terminate",
    "vk::",
];

#[derive(Clone, Default)]
pub(in crate::pipeline::extract) struct ServerLog(Arc<Mutex<VecDeque<String>>>);

impl ServerLog {
    /// Reads `output` until it closes. The server must not fill the pipe and
    /// stall, so this drains everything and keeps only the tail. `secret` never
    /// reaches a message.
    pub(super) fn capture(output: impl Read + Send + 'static, secret: String) -> Self {
        let log = Self::default();
        let tail = log.clone();
        std::thread::spawn(move || tail.read(output, &secret));
        log
    }

    fn read(&self, output: impl Read, secret: &str) {
        let mut reader = BufReader::new(output);
        let mut line = Vec::new();
        loop {
            line.clear();
            // A line past the limit carries on as the next one.
            match reader
                .by_ref()
                .take(LINE_BYTES)
                .read_until(b'\n', &mut line)
            {
                Ok(0) | Err(_) => return,
                Ok(_) => self.keep(&line, secret),
            }
        }
    }

    fn keep(&self, line: &[u8], secret: &str) {
        let line = String::from_utf8_lossy(line).replace(secret, "[hidden]");
        let line = ANSI_ESCAPE_SEQUENCE.replace_all(line.trim(), "");
        if line.is_empty() {
            return;
        }
        let mut lines = self.0.lock().unwrap_or_else(|error| error.into_inner());
        if lines.len() == KEPT_LINES {
            lines.pop_front();
        }
        lines.push_back(line.into_owned());
    }

    /// The latest lines that name a failure, or failing that the latest lines.
    pub(in crate::pipeline::extract) fn reason(&self) -> Option<String> {
        let lines = self.0.lock().unwrap_or_else(|error| error.into_inner());
        let failure = |line: &&String| {
            let line = line.to_lowercase();
            FAILURE_WORDS.iter().any(|word| line.contains(word))
        };
        let mut chosen: Vec<&String> = lines.iter().filter(failure).collect();
        if chosen.is_empty() {
            chosen = lines.iter().collect();
        }
        let shown = chosen.len().saturating_sub(REASON_LINES);
        let text = chosen[shown..]
            .iter()
            .map(|line| line.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        (!text.is_empty()).then_some(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn read(output: &str) -> ServerLog {
        let log = ServerLog::default();
        log.read(Cursor::new(output.to_owned()), "secret-key");
        log
    }

    #[test]
    fn a_vulkan_failure_stands_out_from_routine_lines() {
        let log = read(
            "slot launch_slot_: id 0 | task 12 | processing task\n\
             ggml_vulkan: Device memory allocation of size 2147483648 failed.\n\
             terminate called after throwing an instance of 'vk::DeviceLostError'\n\
             slot update_slots: id 0 | task 12 | prompt done\n",
        );
        let reason = log.reason().unwrap();
        assert!(reason.contains("allocation of size"), "{reason}");
        assert!(reason.contains("vk::DeviceLostError"), "{reason}");
        assert!(!reason.contains("prompt done"), "{reason}");
    }

    #[test]
    fn with_no_failure_named_the_last_lines_are_kept() {
        let lines: String = (0..20).map(|line| format!("step {line}\n")).collect();
        let reason = read(&lines).reason().unwrap();
        assert!(reason.starts_with("step 12\n"), "{reason}");
        assert!(reason.ends_with("step 19"), "{reason}");
    }

    #[test]
    fn a_silent_server_has_no_reason() {
        assert_eq!(read("\n\n").reason(), None);
    }

    #[test]
    fn the_server_key_and_color_codes_never_reach_a_message() {
        let log = read("\u{1b}[31merror: bad key secret-key\u{1b}[0m\n");
        assert_eq!(log.reason().unwrap(), "error: bad key [hidden]");
    }

    #[test]
    fn the_tail_stays_bounded_however_much_the_server_says() {
        let noisy = format!("{}\n", "x".repeat(5000)).repeat(1000);
        let log = read(&noisy);
        let lines = log.0.lock().unwrap();
        assert_eq!(lines.len(), KEPT_LINES);
        assert!(lines.iter().all(|line| line.len() as u64 <= LINE_BYTES));
    }

    #[test]
    fn capture_drains_a_real_pipe_and_ends_with_it() {
        let mut child = std::process::Command::new(if cfg!(windows) { "cmd" } else { "sh" })
            .args(if cfg!(windows) {
                ["/C", "echo error: out of memory 1>&2"]
            } else {
                ["-c", "echo 'error: out of memory' >&2"]
            })
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let log = ServerLog::capture(child.stderr.take().unwrap(), "secret-key".into());
        child.wait().unwrap();
        for _ in 0..200 {
            if log.reason().is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(log.reason().unwrap().contains("out of memory"));
    }
}
