//! OS lock signals. No subprocess, secret, or retained session data.
#[cfg(windows)]
#[allow(unsafe_code)]
mod windows;

pub fn start() {
    #[cfg(windows)]
    windows::start();
}
pub fn must_lock() -> bool {
    #[cfg(windows)]
    {
        windows::must_lock()
    }
    #[cfg(not(windows))]
    {
        false
    }
}
pub fn acknowledge_unlock() -> bool {
    #[cfg(windows)]
    {
        windows::acknowledge_unlock()
    }
    #[cfg(not(windows))]
    {
        true
    }
}
