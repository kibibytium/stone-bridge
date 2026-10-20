use crate::logger::log;
use crate::mail_dispatcher::Mailer;
use crate::mime::{Attachment, MimeError, MimeMessage, ParsedMimeMessage};
use crate::smtp::session::{Action, MessageOutcome, Reply, Session};
use chrono::format::parse;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;

mod command;
mod error;
mod session;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope {
    pub from: String,
    pub recipients: Vec<String>,
    /// Message lines exactly as received, each with its line ending. The
    /// terminating `.` line is not included. Dot-stuffing is not removed yet.
    pub lines: Vec<u8>,
}

struct PendingMail {
    recipients: Vec<String>,
    subject: String,
    body: String,
    attachments: Vec<Attachment>,
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

            if session.expects_command() {
                log::debug!("{}", String::from_utf8_lossy(&raw));
            }

            let reply = match session.handle_line(&raw) {
                Action::Continue => continue,
                Action::Reply(reply) => reply,
                Action::Close(reply) => {
                    let _ = send(&mut writer, &reply);
                    break;
                }
                Action::Message(envelope) => {
                    let outcome = match prepare_mail(envelope) {
                        Ok(mail) => {
                            pending.push(mail);
                            MessageOutcome::Accepted
                        }
                        Err(reason) => MessageOutcome::PermanentFailure(reason.to_string()),
                    };
                    session.finish_message(outcome)
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

/// Dumps the message to `mime.txt` and extracts subject and body.
fn prepare_mail(envelope: Envelope) -> Result<PendingMail, MimeError> {
    //FIXME test if this writes utf8
    match File::create("mime.txt").and_then(|mut f| f.write_all(&envelope.lines)) {
        Ok(()) => log::debug!("Done reading DATA and wrote data to file."),
        Err(e) => log::warning!("Could not write mime.txt: {}", e),
    }
    match ParsedMimeMessage::try_from(envelope.lines.as_slice()) {
        Ok(parsed) => {
            let body = match parsed.html {
                Some(html) => html,
                None => parsed.text.unwrap_or_else(|| "".to_string()),
            };

            log::info!("parsing done, preparing mail");
            Ok(PendingMail {
                recipients: envelope.recipients,
                subject: parsed.subject.unwrap_or("".to_string()),
                body,
                attachments: parsed.attachments,
            })
        }
        Err(err) => {
            return Err(err);
        }
    }
}

pub fn parse_cmd_address(line: String) -> String {
    let trimmed = line.trim();
    let upper = trimmed.to_uppercase();

    let rest = ["TO:", "FROM:"]
        .iter()
        .find(|p| upper.starts_with(*p))
        .map(|p| &trimmed[p.len()..])
        .unwrap_or(line.as_str());

    let addr = rest
        .trim()
        .trim_start_matches('<')
        .trim_end_matches('>')
        .trim();

    addr.to_string()
}

#[cfg(test)]
mod tests {
    use crate::smtp::parse_cmd_address;

    #[test]
    fn can_parse_address_from_smtp_cmd() {
        let line = String::from("FROM: <test@localhost>");
        let parsed = parse_cmd_address(line.clone());

        assert_eq!(parsed, "test@localhost");
    }
}
