#[derive(Debug)]
pub enum SmtpError {
    CommandSyntaxError(String),
}