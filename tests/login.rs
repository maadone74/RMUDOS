use std::net::SocketAddr;
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn test_login() {
    // It's assumed the server is running on this address.
    let addr = "127.0.0.1:6666".parse::<SocketAddr>().unwrap();
    let mut stream = TcpStream::connect(addr).await.unwrap();

    // Read the welcome message
    let mut buf = [0; 1024];
    let n = stream.read(&mut buf).await.unwrap();
    let msg = String::from_utf8_lossy(&buf[..n]);
    assert!(msg.contains("Welcome to the MUD!"));

    // Send username
    stream.write_all(b"testuser\n").await.unwrap();

    // Read the password prompt
    let n = stream.read(&mut buf).await.unwrap();
    let msg = String::from_utf8_lossy(&buf[..n]);
    assert!(msg.contains("Please enter your password:"));

    // Send password
    stream.write_all(b"password123\n").await.unwrap();

    // Read the login success message
    let n = stream.read(&mut buf).await.unwrap();
    let msg = String::from_utf8_lossy(&buf[..n]);
    assert!(msg.contains("Login successful!"));
}
