use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use crate::logger::log;
use crate::mail_dispatcher::Mailer;
use crate::smtp::session::{Action, Reply, Session};

mod command;
mod session;
mod error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope {
    pub from: String,
    pub recipients: Vec<String>,
    /// Message lines exactly as received, each with its line ending. The
    /// terminating `.` line is not included. Dot-stuffing is not removed yet.
    pub lines: Vec<String>,
}

struct PendingMail {
    recipients: Vec<String>,
    subject: String,
    body: String,
}
pub struct TutaMailTransferAgent<M: Mailer> {
    mailer: Arc<M>,
    bind_address: String,
    hostname: String,
}
impl<M: Mailer> TutaMailTransferAgent<M> {
    pub fn new(
        mailer: Arc<M>,
        bind_address: impl Into<String>,
        hostname: impl Into<String>,
    ) -> Self {
        Self {
            mailer,
            bind_address: bind_address.into(),
            hostname: hostname.into(),
        }
    }

    pub async fn start(&self) {
        let listener = TcpListener::bind(&self.bind_address).unwrap();

        for stream in listener.incoming().flatten() {
            let pending = self.serve_connection(stream);
            for mail in pending {
                self.deliver(mail).await;
            }
        }
    }
    fn serve_connection(&self, stream: TcpStream) -> Vec<PendingMail> {
        let mut pending = Vec::new();
        let mut session = Session::new(self.hostname.clone());
        let mut writer = match stream.try_clone() {
            Ok(writer) => writer,
            Err(e) => {
                log::error!("Could not clone connection stream: {}", e);
                return pending;
            }
        };
        let mut reader = BufReader::new(stream);

        if send(&mut writer, &session.greeting()).is_err() {
            return pending;
        }

        let mut raw = Vec::new();
        loop {
            raw.clear();
            match reader.read_until(b'\n', &mut raw) {
                Ok(0) => {
                    log::debug!("Connection closed by client");
                    break;
                }
                Ok(_) => {}
                Err(e) => {
                    log::error!("Read error: {}", e);
                    break;
                }
            }
            let line = String::from_utf8_lossy(&raw);
            if session.expects_command() {
                let line = line.trim();
                log::debug!("{}", line);
            }

            let reply = match session.handle_line(&line) {
                Action::Continue => continue,
                Action::Reply(reply) => reply,
                Action::Close(reply) => {
                    let _ = send(&mut writer, &reply);
                    break;
                }
                Action::Message(envelope) => {
                    todo!()
                }
            };
            if send(&mut writer, &reply).is_err() {
                break;
            }
        }
        pending
    }

    async fn deliver(&self, mail: PendingMail) {
        for recipient in &mail.recipients {
            log::debug!("Sending mail to {}", recipient);
            if let Err(e) = self
                .mailer
                .send_mail(recipient, &mail.subject, &mail.body)
                .await
            {
                log::error!("Could not send mail to {}: {:?}", recipient, e);
            }
        }
    }
}

fn send(writer: &mut TcpStream, reply: &Reply) -> std::io::Result<()> {
    let result = writer
        .write_all(&reply.to_bytes())
        .and_then(|()| writer.flush());
    if let Err(e) = &result {
        log::error!("Write error: {}", e);
    }
    result
}