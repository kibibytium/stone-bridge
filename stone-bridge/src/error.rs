#[derive(Debug)]
pub enum TutaBridgeError {
    SocketError(String),
    SendMessageError(String),
    ResponseError(String),
}