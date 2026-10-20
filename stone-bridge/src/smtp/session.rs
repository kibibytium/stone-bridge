use crate::logger::log;
use crate::smtp::command::Command;
use crate::smtp::error::SmtpError;
use crate::smtp::{Envelope, parse_cmd_address};

#[derive(Debug, PartialEq)]
enum State {
    /// Greeting sent; waiting for HELO/EHLO.
    Connected,
    /// HELO/EHLO received; no mail transaction in progress.
    Greeted,
    /// MAIL accepted; waiting for the first RCPT.
    Mail { from: String },
    /// RCPT
    Recipients {
        /// from is needed because State::Data write Envelope and needs from.
        from: String,
        recipients: Vec<String>,
    },
    /// Inside DATA, collecting lines until `.`.
    Data(Envelope),
    /// The message was handed to the MIME parser
    Awaiting,
}

pub struct Session {
    hostname: String,
    state: State,
    envelopes: Vec<Envelope>,
}

impl Session {
    pub fn new(hostname: String) -> Self {
        Self {
            hostname,
            state: State::Connected,
            envelopes: Vec::new(),
        }
    }

    /// The 220 greeting sent when the connection opens.
    pub fn greeting(&self) -> Reply {
        Reply::new(220, format!("{} ESMTP", self.hostname))
    }

    /// True when the next line is an SMTP command, false while inside DATA
    pub fn expects_command(&self) -> bool {
        !matches!(self.state, State::Data { .. })
    }

    pub fn handle_line(&mut self, line: &[u8]) -> Action {
        match std::mem::replace(&mut self.state, State::Awaiting) {
            State::Awaiting => {
                log::error!("Got Awaiting State twice in a row.");
                Action::Reply(Reply::new(
                    451,
                    "4.3.0 Previous message still being processed",
                ))
            }

            State::Data(mut envelope) => {
                if line == b".\r\n" {
                    self.state = State::Greeted;
                    return Action::Message(envelope);
                }

                envelope.lines.extend_from_slice(line);
                self.state = State::Data(envelope);
                Action::Continue
            }

            state => {
                // reset the state to what it was
                self.state = state;
                let command_result: Result<Command, SmtpError> = line.try_into();
                match command_result {
                    Ok(command) => self.handle_command(command),
                    Err(e) => {
                        Action::Reply(Reply::new(501, format!("5.4.4 Syntax error: {:?}", e)))
                    }
                }
            }
        }
    }

    pub fn handle_command(&mut self, command: Command) -> Action {
        let reply: Reply = match command {
            Command::Helo(client_msg) => {
                log::info!("HELO: {:?}", client_msg);
                self.state = State::Greeted;
                Reply::new(250, self.hostname.clone())
            }
            Command::Ehlo(host) => {
                self.state = State::Greeted;
                self.ehlo_reply(&host)
            }
            Command::Mail { from: mail_from } => match &mut self.state {
                State::Connected => bad_sequence("MAIL command is too early"),
                State::Greeted => {
                    self.state = State::Mail {
                        from: parse_cmd_address(mail_from),
                    };
                    ok()
                }
                State::Recipients { from, recipients } => bad_sequence("MAIL comes before RCPT"),
                _ => bad_sequence("Bad Sequence of Commands"),
            },
            Command::Quit => {
                return Action::Close(Reply::new(
                    221,
                    format!(
                        "2.0.0 {} Service closing transmission channel",
                        self.hostname
                    ),
                ));
            }
            Command::Data => match std::mem::replace(&mut self.state, State::Greeted) {
                State::Recipients { from, recipients } => {
                    self.state = State::Data(Envelope {
                        from,
                        recipients,
                        lines: Vec::new(),
                    });

                    Reply::new(354, "Start mail input; end with <CRLF>.<CRLF>")
                }

                state => {
                    self.state = state;
                    bad_sequence("Need RCPT before DATA")
                }
            },

            Command::Rcpt { to } => match &mut self.state {
                State::Greeted => bad_sequence("Expected MAIL before RCPT"),
                State::Mail { from } => {
                    self.state = State::Recipients {
                        from: from.to_string(),
                        recipients: vec![parse_cmd_address(to)],
                    };
                    ok()
                }
                State::Recipients { from, recipients } => {
                    recipients.push(to.clone());
                    ok()
                }
                _ => bad_sequence("RCPT too early"),
            },
            Command::Unknown(verb) => {
                Reply::new(502, format!("5.5.2 Command not implemented: {}", verb))
            }
        };
        Action::Reply(reply)
    }

    pub fn ehlo_reply(&self, host: &str) -> Reply {
        Reply::multiline(
            250,
            vec![
                format!("{} greets {}", self.hostname, host),
                "SIZE 36700160".into(),
                "ETRN".into(),
                "ENHANCEDSTATUSCODES".into(),
                "8BITMIME".into(),
                "AUTH PLAIN LOGIN".into(),
                "DSN".into(),
            ],
        )
    }
    pub fn finish_message(&mut self, outcome: MessageOutcome) -> Reply {
        self.state = State::Greeted;
        match outcome {
            MessageOutcome::Accepted => Reply::new(250, "2.0.0 Ok: Message queued"),
            MessageOutcome::TemporaryFailure(reason) => {
                Reply::new(451, format!("4.3.0 {}", reason))
            }
            MessageOutcome::PermanentFailure(reason) => {
                Reply::new(554, format!("5.0.0 {}", reason))
            }
        }
    }
}

fn ok() -> Reply {
    Reply::new(250, "2.0.0 OK")
}

fn bad_sequence(text: &str) -> Reply {
    Reply::new(503, format!("5.5.1 {}", text))
}

/// SMTP Reply
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub code: u16,
    pub lines: Vec<String>,
}

impl Reply {
    pub fn new(code: u16, text: impl Into<String>) -> Self {
        Self {
            code,
            lines: vec![text.into()],
        }
    }

    pub fn multiline(code: u16, lines: Vec<String>) -> Self {
        debug_assert!(!lines.is_empty());
        Self { code, lines }
    }

    /// Wire format: `250-first\r\n250-second\r\n250 last\r\n`.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = String::new();
        let last = self.lines.len().saturating_sub(1);
        for (i, line) in self.lines.iter().enumerate() {
            let sep = if i == last { ' ' } else { '-' };
            out.push_str(&format!("{}{}{}\r\n", self.code, sep, line));
        }
        out.into_bytes()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    /// Write the reply and keep reading.
    Reply(Reply),
    /// Write the reply and close the connection.
    Close(Reply),
    /// A message is complete.
    Message(Envelope),
    /// The line was consumed without a reply (a DATA content line).
    Continue,
}

/// What the SMTP server replies after MIME parsing
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageOutcome {
    Accepted,
    /// The client may retry later (451).
    TemporaryFailure(String),
    /// The client must not retry (554).
    PermanentFailure(String),
}

#[cfg(test)]
mod tests {
    use crate::smtp::session::{Action, Reply, Session, State};

    #[test]
    fn reply_wire_format() {
        assert_eq!(Reply::new(250, "OK").to_bytes(), b"250 OK\r\n");
        assert_eq!(
            Reply::multiline(250, vec!["a".into(), "b".into(), "c".into()]).to_bytes(),
            b"250-a\r\n250-b\r\n250 c\r\n"
        );
    }

    #[test]
    fn greeting_and_ehlo_use_hostname() {
        let mut s = Session::new("mx.example".to_string());
        assert_eq!(s.greeting().to_bytes(), b"220 mx.example ESMTP\r\n");
        match s.handle_line(b"EHLO c\r\n") {
            Action::Reply(r) => {
                assert_eq!(r.code, 250);
                assert_eq!(r.lines[0], "mx.example greets c");
                assert!(r.lines.contains(&"AUTH PLAIN LOGIN".to_string()));
            }
            other => {
                eprint!("{:?}", other);
                panic!("{:?}", other)
            }
        }
    }

    #[test]
    fn simple_happy_path() {
        let mut s = Session::new("mx.example".to_string());

        let mime_and_quit: Vec<&[u8]> = vec![
            b"Message-ID: <123456789@server_name.net>\r\n",
            b"To: some_address@server_name.net\r\n",
            b"From: Vaultwarden <vaultwarden@server_name.net>\r\n",
            b"Subject: Vaultwarden SMTP Test\r\n",
            b"MIME-Version: 1.0\r\n",
            b"Date: Sun, 21 Sep 2025 11:07:22 +0000\r\n",
            b"Content-Type: text/html; charset=utf-8\r\n",
            b" boundary=\"0nzJYn4VFgU25jBs4nJsFRwZfjURTbE5t1xI1xyN\"\r\n",
            b"Content-Transfer-Encoding: 8bit\r\n",
            b"\r\n",
            b"This is a test\r\n",
            b".\r\n",
            b"QUIT",
        ];

        let commands: Vec<&[u8]> = vec![
            b"EHLO test",
            b"MAIL FROM: sender@localhost",
            b"RCPT TO: recipient@localhost",
            b"DATA",
        ];
        for cmd in commands {
            match s.handle_line(cmd) {
                Action::Reply(a) => match cmd {
                    b"EHLO test" => {
                        assert_eq!(a.code, 250);
                    }
                    b"MAIL FROM: sender@localhost" => {
                        assert_eq!(a.code, 250);
                        assert_eq!(
                            s.state,
                            State::Mail {
                                from: "sender@localhost".to_string()
                            }
                        );
                    }
                    b"RCPT TO: recipient@localhost" => {
                        assert_eq!(a.code, 250);
                        assert_eq!(
                            s.state,
                            State::Recipients {
                                from: "sender@localhost".to_string(),
                                recipients: vec!["recipient@localhost".to_string()]
                            }
                        );
                    }
                    b"DATA" => {
                        assert_eq!(a.lines[0], "Start mail input; end with <CRLF>.<CRLF>");
                    }
                    any => {
                        panic!("No other commands are expected")
                    }
                },
                Action::Close(_) => {
                    panic!("Connection has not been closed yet")
                }
                Action::Message(_) => {
                    panic!("Should not get message yet");
                }
                Action::Continue => {}
            }
        }

        for line in mime_and_quit.iter() {
            match s.handle_line(line) {
                Action::Reply(_) => {}
                Action::Close(_) => {}
                Action::Message(msg) => {
                    assert_eq!(msg.recipients.len(), 1);
                    assert_eq!(msg.recipients[0], "recipient@localhost");
                }
                Action::Continue => {}
            }
        }
    }
}
