//! Real OpenSSH server for tests. Not part of the product.
//!
//! Requires root: it creates a test account and runs `sshd`. It panics when
//! run without root instead of skipping, so a green run means SSH was used.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::{
    fs,
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::OnceLock,
    time::{Duration, Instant},
};

/// Login name of the account the fixture creates and authenticates.
pub const USER: &str = "selai-test";

/// Key and password material shared by every test in the process.
pub struct Material {
    pub dir: PathBuf,
    pub password: String,
    /// `ssh-ed25519 AAAA... comment` for the server's real host key.
    pub host_pub: String,
    /// Public line of a different key, for changed-key tests.
    pub other_host_pub: String,
    pub user_key: PathBuf,
    pub encrypted_key: PathBuf,
    pub encrypted_passphrase: String,
    /// RSA key, used to prove RSA private keys are refused before authentication.
    pub rsa_key: PathBuf,
}

static MATERIAL: OnceLock<Material> = OnceLock::new();

pub fn material() -> &'static Material {
    MATERIAL.get_or_init(build_material)
}

fn build_material() -> Material {
    assert!(
        is_root(),
        "SSH fixtures need root: they create test account '{USER}' and run sshd"
    );
    let dir = tempfile::Builder::new()
        .prefix("selai-ssh-fixture-")
        .tempdir()
        .expect("tempdir")
        .keep();

    let host_key = dir.join("host_ed25519");
    let other_host_key = dir.join("other_host_ed25519");
    let user_key = dir.join("user_ed25519");
    let encrypted_key = dir.join("user_encrypted_ed25519");
    let encrypted_passphrase = "selai-test-passphrase".to_owned();

    keygen(&host_key, "");
    keygen(&other_host_key, "");
    keygen(&user_key, "");
    keygen(&encrypted_key, &encrypted_passphrase);
    let rsa_key = dir.join("user_rsa");
    let rsa = Command::new("ssh-keygen")
        .args(["-q", "-t", "rsa", "-b", "2048", "-N", "", "-f"])
        .arg(&rsa_key)
        .status()
        .expect("ssh-keygen rsa");
    assert!(rsa.success(), "ssh-keygen rsa failed");

    ensure_account();
    let password = random_hex(16);
    run_with_stdin("chpasswd", &format!("{USER}:{password}\n"));
    install_authorized_keys(&[pub_line(&user_key), pub_line(&encrypted_key)]);

    Material {
        host_pub: pub_line(&host_key),
        other_host_pub: pub_line(&other_host_key),
        dir,
        password,
        user_key,
        encrypted_key,
        encrypted_passphrase,
        rsa_key,
    }
}

fn is_root() -> bool {
    Command::new("id")
        .arg("-u")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "0")
        .unwrap_or(false)
}

fn keygen(path: &Path, passphrase: &str) {
    let status = Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", passphrase, "-f"])
        .arg(path)
        .status()
        .expect("run ssh-keygen");
    assert!(status.success(), "ssh-keygen failed for {}", path.display());
}

fn pub_line(private: &Path) -> String {
    let mut p = private.as_os_str().to_owned();
    p.push(".pub");
    fs::read_to_string(PathBuf::from(p))
        .expect("read public key")
        .trim()
        .to_owned()
}

fn ensure_account() {
    let exists = Command::new("id")
        .arg(USER)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !exists {
        let status = Command::new("useradd")
            .args(["-m", "-s", "/bin/bash", USER])
            .status()
            .expect("useradd");
        assert!(status.success(), "useradd {USER} failed");
    }
}

fn run_with_stdin(program: &str, input: &str) {
    use std::io::Write;
    let mut child = Command::new(program)
        .stdin(Stdio::piped())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(input.as_bytes())
        .expect("write stdin");
    assert!(child.wait().expect("wait").success(), "{program} failed");
}

fn install_authorized_keys(lines: &[String]) {
    let ssh_dir = PathBuf::from(format!("/home/{USER}/.ssh"));
    fs::create_dir_all(&ssh_dir).expect("mkdir .ssh");
    let keys = ssh_dir.join("authorized_keys");
    fs::write(&keys, lines.join("\n") + "\n").expect("write keys");
    let owner = format!("{USER}:{USER}");
    let ssh_dir_s = ssh_dir.display().to_string();
    let keys_s = keys.display().to_string();
    for (cmd, args) in [
        ("chown", vec!["-R", owner.as_str(), ssh_dir_s.as_str()]),
        ("chmod", vec!["700", ssh_dir_s.as_str()]),
        ("chmod", vec!["600", keys_s.as_str()]),
    ] {
        assert!(Command::new(cmd).args(&args).status().expect(cmd).success());
    }
}

fn random_hex(bytes: usize) -> String {
    use std::io::Read;
    let mut buf = vec![0u8; bytes];
    fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut buf))
        .expect("read urandom");
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

/// A running `sshd` on a free loopback port, stopped when dropped.
pub struct Sshd {
    child: Child,
    pub port: u16,
    pub log: PathBuf,
}

impl Sshd {
    pub fn start() -> Self {
        let m = material();
        let port = free_port();
        let conf = m.dir.join(format!("sshd_{port}.conf"));
        let log = m.dir.join(format!("sshd_{port}.log"));
        let host_key = m.dir.join("host_ed25519");
        fs::write(
            &conf,
            format!(
                "Port {port}\nListenAddress 127.0.0.1\nHostKey {}\nPidFile {}\n\
                 AuthorizedKeysFile .ssh/authorized_keys\nPasswordAuthentication yes\n\
                 KbdInteractiveAuthentication no\nPubkeyAuthentication yes\nUsePAM yes\n\
                 PermitRootLogin no\nAllowUsers {USER}\nLogLevel VERBOSE\n\
                 Subsystem sftp internal-sftp\n",
                host_key.display(),
                m.dir.join(format!("sshd_{port}.pid")).display(),
            ),
        )
        .expect("write sshd config");
        fs::create_dir_all("/run/sshd").expect("privilege separation directory");
        let child = Command::new("/usr/sbin/sshd")
            .args(["-D", "-e", "-f"])
            .arg(&conf)
            .arg("-E")
            .arg(&log)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start sshd");
        let mut server = Self { child, port, log };
        server.wait_until_listening();
        server
    }

    fn wait_until_listening(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if TcpStream::connect(("127.0.0.1", self.port)).is_ok() {
                return;
            }
            if let Ok(Some(status)) = self.child.try_wait() {
                panic!("sshd exited early with {status}");
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("sshd did not start listening on port {}", self.port);
    }

    /// Log lines recording an authentication decision. Any line here means the
    /// server reached the authentication stage.
    pub fn auth_decisions(&self) -> Vec<String> {
        fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .filter(|l| l.contains("Accepted ") || l.contains("Failed "))
            .map(str::to_owned)
            .collect()
    }
}

impl Drop for Sshd {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .expect("free port")
}
