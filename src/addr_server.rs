use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::net::{SocketAddr, ToSocketAddrs, IpAddr};
use std::io;
use dns_lookup::lookup_addr;

const MAX_CONNS: usize = 256;
const IN_BUF_SIZE: usize = 1024;
const OUT_BUF_SIZE: usize = 80;

enum ConnectionState {
    Closed,
    Open,
}

struct Connection {
    stream: Option<TcpStream>,
    state: ConnectionState,
    addr: Option<SocketAddr>,
    sname: String,
    buf: [u8; IN_BUF_SIZE],
    leftover: usize,
}

impl Connection {
    fn new() -> Self {
        Connection {
            stream: None,
            state: ConnectionState::Closed,
            addr: None,
            sname: String::new(),
            buf: [0; IN_BUF_SIZE],
            leftover: 0,
        }
    }
}

pub async fn start_addr_server(port: u16, ip_address: &str) -> io::Result<()> {
    let addr = format!("{}:{}", ip_address, port);
    let listener = TcpListener::bind(&addr).await?;
    println!("Listening on: {}", addr);

    loop {
        let (socket, _) = listener.accept().await?;
        tokio::spawn(async move {
            if let Err(e) = handle_connection(socket).await {
                eprintln!("Error handling connection: {}", e);
            }
        });
    }
}

async fn handle_connection(mut stream: TcpStream) -> io::Result<()> {
    let mut buf = [0; IN_BUF_SIZE];
    loop {
        let n = stream.read(&mut buf).await?;
        if n == 0 {
            return Ok(());
        }

        let mut cursor = 0;
        while cursor < n {
            let (msg_type, msg_len) = parse_message_header(&buf[cursor..n]);
            if msg_len == 0 {
                // Incomplete header, need more data
                break;
            }

            if (cursor + msg_len) > n {
                // Incomplete message, need more data
                break;
            }

            let msg_data = &buf[cursor + std::mem::size_of::<u32>()..cursor + msg_len];
            let response = match msg_type {
                1 => ip_by_name(msg_data).await,
                2 => name_by_ip(msg_data).await,
                _ => "Unknown message type".to_string(),
            };

            stream.write_all(response.as_bytes()).await?;
            cursor += msg_len;
        }
    }
}

fn parse_message_header(buf: &[u8]) -> (u32, usize) {
    if buf.len() < std::mem::size_of::<u32>() * 2 {
        return (0, 0);
    }
    let msg_type = u32::from_ne_bytes(buf[0..4].try_into().unwrap());
    let msg_len = u32::from_ne_bytes(buf[4..8].try_into().unwrap()) as usize;
    (msg_type, msg_len)
}

async fn ip_by_name(data: &[u8]) -> String {
    let name = String::from_utf8_lossy(data);
    match (name.to_string(), 0).to_socket_addrs() {
        Ok(mut addrs) => {
            if let Some(addr) = addrs.next() {
                format!("{} {}\n", name, addr.ip())
            } else {
                format!("{} 0\n", name)
            }
        }
        Err(_) => format!("{} 0\n", name),
    }
}

async fn name_by_ip(data: &[u8]) -> String {
    let ip_str = String::from_utf8_lossy(data);
    match ip_str.parse::<IpAddr>() {
        Ok(ip) => {
            if let Ok(hostname) = lookup_addr(&ip) {
                return format!("{} {}\n", ip_str, hostname);
            }
            format!("{} 0\n", ip_str)
        }
        Err(_) => format!("{} 0\n", ip_str),
    }
}
