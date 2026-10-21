use crate::smtp::error::SmtpError;

pub enum Command {
    Helo(String),
    Ehlo(String),
    Auth,
    Mail { from: String },
    Rcpt { to: String },
    Data,
    Quit,
    Unknown(String),
}

impl TryFrom<&[u8]> for Command {
    type Error = SmtpError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        let s = std::str::from_utf8(bytes)
            .map_err(|_| SmtpError::CommandSyntaxError("Command is not valid UTF-8".to_string()))?
            .trim();
        let (verb, args) = match s.split_once(|c: char| c.is_ascii_whitespace()) {
            Some((verb, args)) => (verb, args),
            None => (s, ""),
        };
        let verb = verb.to_uppercase();
        match verb.as_str() {
            "HELO" => Ok(Command::Helo(args.to_string())),
            "EHLO" => Ok(Command::Ehlo(args.to_string())),
            "MAIL" => Ok(Command::Mail {
                from: args.to_string(),
            }),
            "RCPT" => Ok(Command::Rcpt {
                to: args.to_string(),
            }),
            "DATA" => {
                if !args.is_empty() {
                    return Err(SmtpError::CommandSyntaxError(format!(
                        "Got args for DATA command: {}",
                        args
                    )));
                }
                Ok(Command::Data)
            }
            "QUIT" => Ok(Command::Quit),
            "AUTH" => Ok(Command::Auth),
            other => Ok(Command::Unknown(other.to_string())),
        }
    }
}
