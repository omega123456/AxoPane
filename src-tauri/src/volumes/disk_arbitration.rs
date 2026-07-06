//! Minimal raw FFI shim over macOS's DiskArbitration framework.
//!
//! There is no maintained high-level DiskArbitration crate, so this binds
//! just the handful of C entry points needed to learn about mount/unmount
//! and volume-relabel events without polling: `DASessionCreate`, the three
//! `DARegister*Callback` functions, and `DASessionSetDispatchQueue`. Driving
//! the session off a `dispatch2` serial queue means the callbacks run on a
//! GCD-owned thread that blocks until the kernel has something to report —
//! zero idle wakeups while the app is otherwise idle.

use std::ffi::c_void;
use std::ptr;

use core_foundation_sys::array::CFArrayRef;
use core_foundation_sys::base::{kCFAllocatorDefault, CFAllocatorRef, CFRelease, CFTypeRef};
use core_foundation_sys::dictionary::CFDictionaryRef;
use dispatch2::{DispatchQueue, DispatchRetained};

use super::VolumeMonitorState;

#[repr(C)]
struct OpaqueDASession {
    _private: [u8; 0],
}
type DASessionRef = *const OpaqueDASession;

#[repr(C)]
struct OpaqueDADisk {
    _private: [u8; 0],
}
type DADiskRef = *const OpaqueDADisk;

type DiskAppearedCallback = extern "C" fn(disk: DADiskRef, context: *mut c_void);
type DiskDisappearedCallback = extern "C" fn(disk: DADiskRef, context: *mut c_void);
type DiskDescriptionChangedCallback =
    extern "C" fn(disk: DADiskRef, keys: CFArrayRef, context: *mut c_void);

#[link(name = "DiskArbitration", kind = "framework")]
extern "C" {
    fn DASessionCreate(allocator: CFAllocatorRef) -> DASessionRef;
    fn DASessionSetDispatchQueue(session: DASessionRef, queue: *mut c_void);
    fn DARegisterDiskAppearedCallback(
        session: DASessionRef,
        match_dict: CFDictionaryRef,
        callback: DiskAppearedCallback,
        context: *mut c_void,
    );
    fn DARegisterDiskDisappearedCallback(
        session: DASessionRef,
        match_dict: CFDictionaryRef,
        callback: DiskDisappearedCallback,
        context: *mut c_void,
    );
    fn DARegisterDiskDescriptionChangedCallback(
        session: DASessionRef,
        match_dict: CFDictionaryRef,
        watch_keys: CFArrayRef,
        callback: DiskDescriptionChangedCallback,
        context: *mut c_void,
    );
}

extern "C" fn disk_appeared(_disk: DADiskRef, context: *mut c_void) {
    // SAFETY: `context` is `Arc::as_ptr(&state)`, kept alive by `Session` for
    // as long as the session (and therefore this callback) can fire.
    unsafe { &*(context as *const VolumeMonitorState) }.emit_if_changed();
}

extern "C" fn disk_disappeared(_disk: DADiskRef, context: *mut c_void) {
    // SAFETY: see `disk_appeared`.
    unsafe { &*(context as *const VolumeMonitorState) }.emit_if_changed();
}

extern "C" fn disk_description_changed(_disk: DADiskRef, _keys: CFArrayRef, context: *mut c_void) {
    // SAFETY: see `disk_appeared`.
    unsafe { &*(context as *const VolumeMonitorState) }.emit_if_changed();
}

/// Owns the Disk Arbitration session and the serial dispatch queue driving
/// it. Dropping this unschedules the queue and releases the session.
pub struct Session {
    session: DASessionRef,
    // Kept alive so the queue backing the session isn't deallocated out from
    // under DiskArbitration while the session is live.
    _queue: DispatchRetained<DispatchQueue>,
}

// SAFETY: `DASessionRef` is a Core Foundation object reference; DiskArbitration
// sessions are safe to hand off between threads (only the callbacks, which
// run on the dispatch queue, ever touch the pointee), and `Session` never
// exposes the raw pointer for concurrent mutation.
unsafe impl Send for Session {}

impl Session {
    pub fn start(state: &std::sync::Arc<VolumeMonitorState>) -> Result<Self, String> {
        // SAFETY: `kCFAllocatorDefault` is a valid CF allocator constant.
        let session = unsafe { DASessionCreate(kCFAllocatorDefault) };
        if session.is_null() {
            return Err("DASessionCreate returned null".to_string());
        }

        let queue = DispatchQueue::new("com.axopane.volume-monitor", None);
        let context = std::sync::Arc::as_ptr(state) as *mut c_void;

        // SAFETY: `session` is a freshly created, non-null DA session; the
        // callbacks are `extern "C" fn`s matching DiskArbitration's expected
        // signatures, and `context` stays valid for the session's lifetime
        // because the caller (`VolumeMonitorService`) keeps `state` alive at
        // least as long as this `Session`.
        unsafe {
            DARegisterDiskAppearedCallback(session, ptr::null(), disk_appeared, context);
            DARegisterDiskDisappearedCallback(session, ptr::null(), disk_disappeared, context);
            DARegisterDiskDescriptionChangedCallback(
                session,
                ptr::null(),
                ptr::null(),
                disk_description_changed,
                context,
            );

            let queue_ptr = DispatchRetained::as_ptr(&queue).as_ptr() as *mut c_void;
            DASessionSetDispatchQueue(session, queue_ptr);
        }

        Ok(Self {
            session,
            _queue: queue,
        })
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // SAFETY: `self.session` was created by `DASessionCreate` in `start`
        // and is released exactly once, here.
        unsafe {
            DASessionSetDispatchQueue(self.session, ptr::null_mut());
            CFRelease(self.session as CFTypeRef);
        }
    }
}
