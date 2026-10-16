//! Narrow Win32 boundary. Every buffer and registration has one owner.
use std::{
    ffi::c_void,
    ptr,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Once,
    },
    time::Duration,
};
use windows_sys::Win32::{
    System::{Power::*, RemoteDesktop::*},
    UI::WindowsAndMessaging::{
        DEVICE_NOTIFY_CALLBACK, PBT_APMRESUMEAUTOMATIC, PBT_APMRESUMESUSPEND, PBT_APMSUSPEND,
    },
};
static GENERATION: AtomicU64 = AtomicU64::new(0);
static ACKNOWLEDGED: AtomicU64 = AtomicU64::new(0);
static READY: AtomicBool = AtomicBool::new(false);
static START: Once = Once::new();

fn screen_unlocked() -> bool {
    let mut session = 0;
    let mut buffer = ptr::null_mut();
    let mut size = 0;
    // SAFETY: outputs point to initialized stack locals. The OS owns buffer
    // until WTSFreeMemory; its size and level are checked before the union read.
    unsafe {
        if ProcessIdToSessionId(std::process::id(), &mut session) == 0 {
            return false;
        }
        if WTSQuerySessionInformationW(
            ptr::null_mut(),
            session,
            WTSSessionInfoEx,
            &mut buffer,
            &mut size,
        ) == 0
        {
            return false;
        }
        let unlocked = if !buffer.is_null() && size as usize >= std::mem::size_of::<WTSINFOEXW>() {
            let info = ptr::read_unaligned(buffer.cast::<WTSINFOEXW>());
            info.Level == 1
                && info.Data.WTSInfoExLevel1.SessionFlags == WTS_SESSIONSTATE_UNLOCK as i32
        } else {
            false
        };
        WTSFreeMemory(buffer.cast());
        unlocked
    }
}
unsafe extern "system" fn power_event(_: *const c_void, kind: u32, _: *const c_void) -> u32 {
    if [PBT_APMSUSPEND, PBT_APMRESUMEAUTOMATIC, PBT_APMRESUMESUSPEND].contains(&kind) {
        GENERATION.fetch_add(1, Ordering::SeqCst);
    }
    0
}
pub fn start() {
    START.call_once(|| {
        std::thread::spawn(|| {
            let mut params = DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS {
                Callback: Some(power_event),
                Context: ptr::null_mut(),
            };
            let mut registration = ptr::null_mut();
            // SAFETY: params remains on this thread's stack for the full registration
            // lifetime. The callback accesses only static atomics, never its context.
            let result = unsafe {
                PowerRegisterSuspendResumeNotification(
                    DEVICE_NOTIFY_CALLBACK,
                    (&mut params as *mut DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS).cast(),
                    &mut registration,
                )
            };
            if result != 0 {
                return;
            }
            READY.store(true, Ordering::SeqCst);
            loop {
                if !screen_unlocked() {
                    GENERATION.fetch_add(1, Ordering::SeqCst);
                }
                std::thread::sleep(Duration::from_millis(250));
            }
            // Registration lasts until process exit; Windows releases it at that point.
        });
    });
}
pub fn must_lock() -> bool {
    !READY.load(Ordering::SeqCst)
        || GENERATION.load(Ordering::SeqCst) != ACKNOWLEDGED.load(Ordering::SeqCst)
}
pub fn acknowledge_unlock() -> bool {
    let generation = GENERATION.load(Ordering::SeqCst);
    if !READY.load(Ordering::SeqCst) || !screen_unlocked() {
        return false;
    }
    ACKNOWLEDGED.store(generation, Ordering::SeqCst);
    true
}
