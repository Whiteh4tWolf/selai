//! Running one command as root through `sudo`, without the model ever seeing a
//! password (ADR-0005, sudo amendment).
//!
//! The command is wrapped by local code, never by the model. When a password is
//! needed, `sudo` is told to print a prompt that contains a random nonce. The
//! password is written to stdin only after that exact prompt appears on stderr,
//! and only once; stdin is then closed. If the prompt never appears (the host
//! does not ask), nothing is written, so a command that reads stdin cannot
//! capture the password. The prompt is removed from the output afterwards.

use selai_domain::TaskId;

/// Checks whether `sudo` would run without a password, without running anything
/// as root. `-k` ignores cached credentials, `-n` never prompts.
pub const SUDO_PROBE: &str = "LC_ALL=C sudo -k -n true";

/// What [`SUDO_PROBE`] found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SudoStatus {
    /// `NOPASSWD` (or the user is root): runs without a password.
    NoPassword,
    /// `sudo` will ask for the user's password.
    PasswordRequired,
    /// The user may not use `sudo` on this host.
    NotAllowed,
    /// `sudo` is not installed.
    Unavailable,
}

impl SudoStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NoPassword => "no_password",
            Self::PasswordRequired => "password_required",
            Self::NotAllowed => "not_allowed",
            Self::Unavailable => "unavailable",
        }
    }

    /// Reads the probe's result. Unknown failures count as "password required",
    /// so the operator can still try; a wrong password then fails safely.
    pub fn from_probe(exit_status: Option<u32>, stderr: &str) -> Self {
        let text = stderr.to_ascii_lowercase();
        if exit_status == Some(0) {
            return Self::NoPassword;
        }
        if exit_status == Some(127)
            || text.contains("command not found")
            || text.contains("sudo: not found")
        {
            return Self::Unavailable;
        }
        if text.contains("not in the sudoers")
            || text.contains("is not allowed to run sudo")
            || text.contains("may not run sudo")
        {
            return Self::NotAllowed;
        }
        Self::PasswordRequired
    }
}

/// A wrapped command and the prompt that marks where the password goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SudoPlan {
    /// The full text to execute. Contains no secret.
    pub command: String,
    /// Present when a password will be written on this prompt.
    pub prompt: Option<String>,
}

/// Wraps `inner` so it runs as root through `sh -c`. With `with_password`, sudo
/// reads the password from stdin on a unique prompt; without it, sudo must not
/// prompt at all and fails instead.
pub fn sudo_plan(inner: &str, with_password: bool) -> SudoPlan {
    let quoted = shell_quote(inner);
    if with_password {
        let nonce = TaskId::new().to_string().replace('-', "");
        let prompt = format!("[selai-sudo:{nonce}]");
        SudoPlan {
            command: format!("LC_ALL=C sudo -S -k -p '{prompt}' -- sh -c {quoted}"),
            prompt: Some(prompt),
        }
    } else {
        SudoPlan {
            command: format!("LC_ALL=C sudo -n -- sh -c {quoted}"),
            prompt: None,
        }
    }
}

/// POSIX single-quote quoting: the result is one shell word equal to `s`.
pub fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Watches stderr for the prompt and says when to send the password. Bytes can
/// arrive split anywhere, so a short tail of earlier data is kept.
#[derive(Debug)]
pub struct PromptWatch {
    prompt: Vec<u8>,
    tail: Vec<u8>,
    answered: bool,
    closed: bool,
}

/// What the caller must do after feeding data to [`PromptWatch`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StdinAction {
    Nothing,
    /// Write the password and a newline, then close stdin.
    SendPasswordAndClose,
    /// Close stdin without writing anything.
    Close,
}

impl PromptWatch {
    pub fn new(prompt: &str) -> Self {
        Self {
            prompt: prompt.as_bytes().to_vec(),
            tail: Vec::new(),
            answered: false,
            closed: false,
        }
    }

    /// Call with every stderr chunk.
    pub fn on_stderr(&mut self, data: &[u8]) -> StdinAction {
        if self.closed {
            return StdinAction::Nothing;
        }
        self.tail.extend_from_slice(data);
        let found = contains(&self.tail, &self.prompt);
        let keep = self.prompt.len().saturating_sub(1);
        if self.tail.len() > keep {
            self.tail.drain(..self.tail.len() - keep);
        }
        if found && !self.answered {
            self.answered = true;
            self.closed = true;
            return StdinAction::SendPasswordAndClose;
        }
        StdinAction::Nothing
    }

    /// Call with every stdout chunk. Output before any prompt means sudo did not
    /// ask, so stdin is closed and the password is never sent.
    pub fn on_stdout(&mut self, data: &[u8]) -> StdinAction {
        if self.closed || data.is_empty() {
            return StdinAction::Nothing;
        }
        self.closed = true;
        StdinAction::Close
    }

    pub fn answered(&self) -> bool {
        self.answered
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|w| w == needle)
}

/// Removes every occurrence of the prompt from captured stderr.
pub fn strip_prompt(stderr: &[u8], prompt: &str) -> Vec<u8> {
    let needle = prompt.as_bytes();
    let mut out = Vec::with_capacity(stderr.len());
    let mut i = 0;
    while i < stderr.len() {
        if stderr[i..].starts_with(needle) {
            i += needle.len();
        } else {
            out.push(stderr[i]);
            i += 1;
        }
    }
    out
}

/// True when sudo's own messages say the password was wrong or missing.
pub fn sudo_auth_failed(stderr: &str) -> bool {
    let text = stderr.to_ascii_lowercase();
    text.contains("incorrect password")
        || text.contains("sorry, try again")
        || text.contains("no password was provided")
        || text.contains("a password is required")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting_keeps_any_text_as_one_word() {
        assert_eq!(shell_quote("echo hi"), "'echo hi'");
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
        assert_eq!(shell_quote("$(id) `x` ; rm"), "'$(id) `x` ; rm'");
    }

    #[test]
    fn plans_use_a_fresh_prompt_and_never_contain_a_secret() {
        let a = sudo_plan("systemctl restart nginx", true);
        let b = sudo_plan("systemctl restart nginx", true);
        assert_ne!(a.prompt, b.prompt, "each run gets a new nonce");
        let prompt = a.prompt.clone().expect("prompt");
        assert!(a.command.contains(&prompt));
        assert!(a.command.contains("sudo -S -k -p"));
        assert!(a.command.ends_with("-- sh -c 'systemctl restart nginx'"));

        let n = sudo_plan("id -u", false);
        assert_eq!(n.prompt, None);
        assert_eq!(n.command, "LC_ALL=C sudo -n -- sh -c 'id -u'");
    }

    #[test]
    fn the_password_is_sent_once_and_only_on_the_exact_prompt() {
        let mut w = PromptWatch::new("[selai-sudo:abc]");
        assert_eq!(w.on_stderr(b"[selai-sudo:ab"), StdinAction::Nothing);
        assert_eq!(
            w.on_stderr(b"c]"),
            StdinAction::SendPasswordAndClose,
            "split prompt"
        );
        assert_eq!(
            w.on_stderr(b"Sorry, try again.\n[selai-sudo:abc]"),
            StdinAction::Nothing
        );
        assert!(w.answered());
    }

    #[test]
    fn output_before_a_prompt_closes_stdin_without_the_password() {
        let mut w = PromptWatch::new("[selai-sudo:abc]");
        assert_eq!(w.on_stdout(b"root\n"), StdinAction::Close);
        assert_eq!(w.on_stderr(b"[selai-sudo:abc]"), StdinAction::Nothing);
        assert!(!w.answered());
    }

    #[test]
    fn a_look_alike_prompt_is_not_answered() {
        let mut w = PromptWatch::new("[selai-sudo:abc]");
        assert_eq!(
            w.on_stderr(b"[sudo] password for ops: "),
            StdinAction::Nothing
        );
        assert_eq!(w.on_stderr(b"[selai-sudo:xyz]"), StdinAction::Nothing);
        assert!(!w.answered());
    }

    #[test]
    fn the_prompt_is_removed_from_output() {
        assert_eq!(
            strip_prompt(b"[p]Sorry, try again.\n[p]sudo: 1 incorrect", "[p]"),
            b"Sorry, try again.\nsudo: 1 incorrect".to_vec()
        );
    }

    #[test]
    fn probe_results_are_classified() {
        assert_eq!(SudoStatus::from_probe(Some(0), ""), SudoStatus::NoPassword);
        assert_eq!(
            SudoStatus::from_probe(Some(1), "sudo: a password is required\n"),
            SudoStatus::PasswordRequired
        );
        assert_eq!(
            SudoStatus::from_probe(Some(1), "ops is not in the sudoers file.\n"),
            SudoStatus::NotAllowed
        );
        assert_eq!(
            SudoStatus::from_probe(Some(1), "Sorry, user ops may not run sudo on web.\n"),
            SudoStatus::NotAllowed
        );
        assert_eq!(
            SudoStatus::from_probe(Some(127), "sh: 1: sudo: not found\n"),
            SudoStatus::Unavailable
        );
    }
}
