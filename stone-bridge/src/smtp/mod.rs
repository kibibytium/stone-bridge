use crate::logger::log;
use crate::mail_dispatcher::Mailer;
use crate::mime::{Attachment, MimeError, MimeMessage, ParsedMimeMessage};
use crate::smtp::session::{Action, MessageOutcome, Reply, Session};
use serde::{Deserialize, Serialize};
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

#[derive(Serialize, Deserialize)]
pub struct MailAddress {
    address: String,
    name: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct PendingMail {
    sender: MailAddress,
    recipients: Vec<MailAddress>,
    subject: String,
    body: String,
    #[serde(skip)] // TODO support Attachments in Tuta
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
            log::debug!("Sending mail to {}", recipient.address);
            if let Err(e) = self.mailer.send_mail(&mail).await {
                log::error!("Could not send mail to {}: {:?}", recipient.address, e);
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
                sender: MailAddress {
                    address: envelope.from,
                    name: parsed.sender_name.clone(),
                },
                recipients: envelope
                    .recipients
                    .iter()
                    .map(|r| MailAddress {
                        address: r.to_string(),
                        name: parsed.recipient_name.clone(),
                    })
                    .collect(),
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

pub fn parse_address_name(line: String) -> String {
    let trimmed = line.trim();
    let upper = trimmed.to_uppercase();

    let rest = ["TO:", "FROM:"]
        .iter()
        .find(|p| upper.starts_with(*p))
        .map(|p| &trimmed[p.len()..])
        .unwrap_or(line.as_str());

    let name_end_idx = rest.find(|c| c == '<');
    match name_end_idx {
        Some(idx) => rest[..idx].trim().trim_matches('"').to_string().to_string(),
        None => "".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use crate::smtp::{parse_address_name, parse_cmd_address};

    #[test]
    fn can_parse_address_from_smtp_cmd() {
        let line = String::from("FROM: <test@localhost>");
        let no_angle_brackets = String::from("To: some_address@server_name.net");
        let parsed_address = parse_cmd_address(line.clone());
        let parsed_no_angle_brackets = parse_cmd_address(no_angle_brackets);

        assert_eq!(parsed_address, "test@localhost");
        assert_eq!(parsed_no_angle_brackets, "some_address@server_name.net");
    }

    #[test]
    fn can_parse_name_from_address_line() {
        let line_with_name = String::from("FROM: \"Sender Name\" <test@localhost>");
        let line_without_name = String::from("TO: <test@localhost>");
        let line_without_brackets = String::from("TO: test@localhost");

        let parsed_name = parse_address_name(line_with_name.clone());
        let parsed_empty = parse_address_name(line_without_name.clone());
        let parsed_no_brackets = parse_address_name(line_without_brackets.clone());

        assert_eq!(parsed_name, "Sender Name");
        assert_eq!(parsed_empty, "");
        assert_eq!(parsed_no_brackets, "");
    }
}
