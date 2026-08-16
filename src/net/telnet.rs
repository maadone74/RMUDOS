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
                            let mut out = text;
                            if !out.ends_with('\n') {
                                out.push('\n');
                            }
                            let out = out.replace('\n', "\r\n");
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
