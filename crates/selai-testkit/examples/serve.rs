//! Runs the test OpenSSH server until stdin closes, so a manual UI check can
//! connect to it. Not shipped. Needs root, like the test suites.
//!
//!     sleep 900 | cargo run -p selai-testkit --example serve

use std::io::Read;

fn main() {
    let m = selai_testkit::material();
    let sshd = selai_testkit::Sshd::start();
    println!(
        "PORT={} USER={} PASSWORD={}",
        sshd.port,
        selai_testkit::USER,
        m.password
    );
    let mut sink = Vec::new();
    let _ = std::io::stdin().read_to_end(&mut sink);
}
