// comm.rs

use std::net::SocketAddr;
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use log::{info, warn, error};
use crate::user::{User, LoginState};

const MAX_TEXT: usize = 2048;
const MESSAGE_BUF_SIZE: usize = 2048;

pub struct Connection {
    pub stream: Option<TcpStream>,
    pub addr: SocketAddr,
    pub last_time: i64,
    pub prompt: String,
    pub text: [u8; MAX_TEXT],
    pub text_end: usize,
    pub text_start: usize,
    pub message_buf: [u8; MESSAGE_BUF_SIZE],
    pub message_producer: usize,
    pub message_consumer: usize,
    pub message_length: usize,
    pub iflags: u32,
    pub user: Option<User>,
}

impl Connection {
    pub fn new(stream: TcpStream) -> Self {
        let addr = stream.peer_addr().unwrap();
        Connection {
            stream: Some(stream),
            addr,
            last_time: chrono::Utc::now().timestamp(),
            prompt: "> ".to_string(),
            text: [0; MAX_TEXT],
            text_end: 0,
            text_start: 0,
            message_buf: [0; MESSAGE_BUF_SIZE],
            message_producer: 0,
            message_consumer: 0,
            message_length: 0,
            iflags: 0,
            user: None,
        }
    }
}

pub struct Shared {
    pub connections: Mutex<HashMap<SocketAddr, Arc<Mutex<Connection>>>>,
}

impl Shared {
    pub fn new() -> Self {
        Shared {
            connections: Mutex::new(HashMap::new()),
        }
    }
}

pub async fn handle_connection(
    stream: TcpStream,
    shared: Arc<Shared>,
    addr: SocketAddr,
) {
    info!("New connection from {}", addr);
    let conn_arc = Arc::new(Mutex::new(Connection::new(stream)));
    shared.connections.lock().unwrap().insert(addr, conn_arc.clone());

    let mut stream = {
        let mut conn = conn_arc.lock().unwrap();
        conn.stream.take().unwrap()
    };

    // Hardcoded user database
    let mut users = HashMap::new();
    users.insert("testuser".to_string(), User::new("testuser", "password123"));

    stream.write_all(b"Welcome to the MUD!\nPlease enter your username: ").await.unwrap();

    let mut buf = [0; 1024];
    let mut current_user: Option<User> = None;
    let mut login_state = LoginState::EnteringUsername;

    loop {
        match stream.read(&mut buf).await {
            Ok(0) => {
                info!("Connection closed by {}", addr);
                break;
            }
            Ok(n) => {
                let msg = String::from_utf8_lossy(&buf[..n]).trim().to_string();
                match login_state {
                    LoginState::EnteringUsername => {
                        if let Some(user) = users.get(&msg) {
                            current_user = Some(User::new(&user.username, &user.password_hash));
                            login_state = LoginState::EnteringPassword;
                            stream.write_all(b"Please enter your password: ").await.unwrap();
                        } else {
                            warn!("Failed login attempt for username: {}", msg);
                            stream.write_all(b"Invalid username. Please try again: ").await.unwrap();
                        }
                    }
                    LoginState::EnteringPassword => {
                        if let Some(user) = &current_user {
                            // Password verification should be done here, after the user enters the password.
                            if user.verify_password(&msg) {
                                login_state = LoginState::LoggedIn;
                                stream.write_all(b"Login successful!\n").await.unwrap();
                                info!("User {} logged in from {}", user.username, addr);
                                // Store user in connection
                                {
                                    let mut conn = conn_arc.lock().unwrap();
                                    conn.user = Some(User::new(&user.username, &user.password_hash));
                                }
                            } else {
                                warn!("Failed login attempt for user: {}", user.username);
                                // Ask for password again, but don't reset the username
                                stream.write_all(b"Invalid password. Please try again: ").await.unwrap();
                            }
                        }
                    }
                    LoginState::LoggedIn => {
                        // Process game commands
                        info!("Received from {}: {}", addr, msg);
                        stream.write_all(format!("You said: {}\n", msg).as_bytes()).await.unwrap();
                    }
                    _ => {}
                }
            }
            Err(e) => {
                error!("Error reading from {}: {}", addr, e);
                break;
            }
        }
    }

    shared.connections.lock().unwrap().remove(&addr);
    info!("Connection from {} removed", addr);
}
