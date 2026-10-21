use crate::error::TutaBridgeError;
use crate::logger::log;
use crate::smtp::{MailAddress, PendingMail};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::Path;

/// Default path of the Unix socket the external sender service listens on.

pub trait Mailer {
    async fn send_mail(&self, mail: &PendingMail) -> Result<(), TutaBridgeError>;
}

#[derive(Serialize, Deserialize)]
#[allow(non_snake_case)]
struct SocketMessageReturn {
    messageId: String,
    recipient: String,
}

pub struct SocketMailer {
    client: Client,
}

impl SocketMailer {
    pub fn new(socket_path: impl AsRef<Path>) -> Result<Self, TutaBridgeError> {
        let client = Client::builder()
            .unix_socket(socket_path.as_ref())
            .build()
            .map_err(|e| {
                TutaBridgeError::SocketError(format!("Failed to create socket: {:?}", e))
            })?;
        Ok(Self { client })
    }
}

impl Mailer for SocketMailer {
    async fn send_mail(&self, mail: &PendingMail) -> Result<(), TutaBridgeError> {
        let response = self
            .client
            .post("http://localhost/")
            .json(&json!(mail))
            .send()
            .await
            .map_err(|e| {
                TutaBridgeError::SendMessageError(format!("Failed to send message: {:?}", e))
            })?;

        let json: SocketMessageReturn = response.json().await.map_err(|e| {
            TutaBridgeError::ResponseError(format!("Failed to parse response: {:?}", e))
        })?;

        log::info!(
            "Successfully sent mail to {}, with MessageId: {}",
            json.recipient,
            json.messageId
        );
        Ok(())
    }
}
