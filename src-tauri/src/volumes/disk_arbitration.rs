use std::ffi::c_void;
use std::ptr;

use core_foundation_sys::array::CFArrayRef;
use core_foundation_sys::base::{kCFAllocatorDefault, CFAllocatorRef, CFRelease, CFTypeRef};
use core_foundation_sys::dictionary::CFDictionaryRef;
use dispatch2::{DispatchQueue, DispatchRetained};

use super::registry::RegistryReconcileBridge;

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
    // SAFETY: `context` is `Arc::as_ptr(&bridge)`, kept alive by `Session` for
    // as long as the session (and therefore this callback) can fire.
    unsafe { &*(context as *const RegistryReconcileBridge) }.reconcile();
}

extern "C" fn disk_disappeared(_disk: DADiskRef, context: *mut c_void) {
    // SAFETY: see `disk_appeared`.
    unsafe { &*(context as *const RegistryReconcileBridge) }.reconcile();
}

extern "C" fn disk_description_changed(_disk: DADiskRef, _keys: CFArrayRef, context: *mut c_void) {
    // SAFETY: see `disk_appeared`.
    unsafe { &*(context as *const RegistryReconcileBridge) }.reconcile();
}

pub struct Session {
    session: DASessionRef,
    _queue: DispatchRetained<DispatchQueue>,
    context: *const RegistryReconcileBridge,
}

// SAFETY: `DASessionRef` is a Core Foundation object reference; DiskArbitration
// sessions are safe to hand off between threads (only the callbacks, which
// run on the dispatch queue, ever touch the pointee), and `Session` never
// exposes the raw pointer for concurrent mutation.
unsafe impl Send for Session {}

impl Session {
    pub fn start_for_registry(
        bridge: std::sync::Arc<RegistryReconcileBridge>,
    ) -> Result<Self, String> {
        // SAFETY: `kCFAllocatorDefault` is a valid CF allocator constant.
        let session = unsafe { DASessionCreate(kCFAllocatorDefault) };
        if session.is_null() {
            return Err("DASessionCreate returned null".to_string());
        }

        let queue = DispatchQueue::new("com.axopane.volume-registry", None);
        let context = std::sync::Arc::into_raw(bridge) as *mut c_void;

        // SAFETY: `session` is a freshly created, non-null DA session; the
        // callbacks are `extern "C" fn`s matching DiskArbitration's expected
        // signatures, and `context` stays valid for the session's lifetime
        // because `Session` owns the `Arc` (via `Arc::into_raw`) and only
        // releases it in `Drop`, after the queue has been unscheduled.
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
            context: context as *const RegistryReconcileBridge,
        })
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // SAFETY: `self.session` was created by `DASessionCreate` in `start_for_registry`
        // and is released exactly once, here. The dispatch queue is
        // unscheduled first so no callback can fire using `self.context`
        // after it is reconstructed and dropped below.
        unsafe {
            DASessionSetDispatchQueue(self.session, ptr::null_mut());
            CFRelease(self.session as CFTypeRef);
            drop(std::sync::Arc::from_raw(self.context));
        }
    }
}
