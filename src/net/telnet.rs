use anyhow::Result;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

/// Outbound telnet traffic: text or echo negotiation.
#[derive(Clone, Debug)]
pub enum TelnetOut {
    Text(String),
    /// `false` = hide local echo (password); `true` = restore echo.
    Echo(bool),
}

pub struct TelnetSession;

impl TelnetSession {
    pub async fn run(
        socket: TcpStream,
        mut outbound_rx: mpsc::UnboundedReceiver<TelnetOut>,
        mut on_line: impl FnMut(String) + Send,
    ) -> Result<()> {
        let (reader, mut writer) = socket.into_split();
        let mut reader = BufReader::new(reader);
        let mut line_buf = Vec::new();

        loop {
            tokio::select! {
                biased;
                msg = outbound_rx.recv() => {
                    match msg {
                        Some(TelnetOut::Text(text)) => {
                            let out = format_telnet_text(&text);
                            writer.write_all(out.as_bytes()).await?;
                            writer.flush().await?;
                        }
                        Some(TelnetOut::Echo(enable)) => {
                            // IAC WILL ECHO (server echoes → client stops local echo)
                            // IAC WONT ECHO (restore client local echo)
                            let seq: [u8; 3] = if enable {
                                [255, 252, 1] // IAC WONT ECHO
                            } else {
                                [255, 251, 1] // IAC WILL ECHO
                            };
                            writer.write_all(&seq).await?;
                            writer.flush().await?;
                        }
                        None => break,
                    }
                }
                read = reader.read_until(b'\n', &mut line_buf) => {
                    let n = read?;
                    if n == 0 {
                        break;
                    }
                    let raw = strip_telnet_iac(&line_buf);
                    line_buf.clear();
                    let line = String::from_utf8_lossy(&raw);
                    let line = line.trim_end_matches(['\r', '\n']).to_string();
                    on_line(line);
                }
            }
        }
        Ok(())
    }
}

/// Telnet payload: `\n` → `\r\n`. Add a line break only when the LPC string
/// has none. `receive_message` wraps with a trailing `\n` then appends color
/// RESET, so the bytes often look like `"text\n<ESC>[0m"` — that already had a
/// newline; adding another made a blank line after every colored message.
pub(crate) fn format_telnet_text(text: &str) -> String {
    let mut out = text.to_owned();
    if !out.contains('\n') {
        out.push('\n');
    }
    out.replace('\n', "\r\n")
}

fn strip_telnet_iac(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len());
    let mut i = 0;
    while i < input.len() {
        if input[i] == 255 && i + 1 < input.len() {
            let cmd = input[i + 1];
            if cmd == 255 {
                out.push(255);
                i += 2;
                continue;
            }
            if (cmd == 251 || cmd == 252 || cmd == 253 || cmd == 254) && i + 2 < input.len() {
                i += 3;
                continue;
            }
            i += 2;
            continue;
        }
        out.push(input[i]);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::format_telnet_text;

    #[test]
    fn color_reset_after_wrap_does_not_add_blank_line() {
        let wrapped = "There are five obvious exits: south.\n\u{1b}[0;37;40m";
        assert_eq!(
            format_telnet_text(wrapped),
            "There are five obvious exits: south.\r\n\u{1b}[0;37;40m"
        );
    }

    #[test]
    fn write_without_newline_still_gets_one() {
        assert_eq!(format_telnet_text("ok"), "ok\r\n");
    }
}
