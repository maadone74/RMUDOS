#[tokio::test]
async fn test_login() {
    use tokio::net::TcpStream;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut stream = TcpStream::connect("127.0.0.1:6666").await.unwrap();

    // Read the welcome message
    let mut buf = [0; 1024];
    let n = stream.read(&mut buf).await.unwrap();
    let msg = String::from_utf8_lossy(&buf[..n]);
    assert!(msg.contains("Welcome to the MUD!"));

    // Send the username
    stream.write_all(b"testuser\n").await.unwrap();

    // Read the password prompt
    let n = stream.read(&mut buf).await.unwrap();
    let msg = String::from_utf8_lossy(&buf[..n]);
    assert!(msg.contains("Please enter your password:"));

    // Send the password
    stream.write_all(b"password123\n").await.unwrap();

    // Read the success message
    let n = stream.read(&mut buf).await.unwrap();
    let msg = String::from_utf8_lossy(&buf[..n]);
    assert!(msg.contains("Login successful!"));
}
