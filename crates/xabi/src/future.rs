use std::ffi::c_void;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

use crate::{
    catch_unwind_code, validate_abi_version, validate_size, Error, Result, XabiOwnedBytes,
    XabiResult, ABI_VERSION, ERR_EXPORT, ERR_INVALID_ARGUMENT, ERR_PANIC, OK, POLL_PENDING,
    POLL_READY,
};

/// Waker handle passed into the xabi future poll ABI.
///
/// `XabiWaker` mirrors Rust's [`Waker`] behavior with C ABI function pointers.
/// Hosts usually construct it with [`XabiWaker::from_waker_ref`]; plugins turn
/// it back into a Rust waker with [`XabiWaker::to_waker`].
#[repr(C)]
#[derive(Clone, Copy)]
pub struct XabiWaker {
    /// Size of this structure in bytes.
    pub size: usize,
    /// ABI version for this structure.
    pub abi_version: u32,
    /// Opaque waker state pointer.
    pub instance: *mut c_void,
    /// Clone the waker state and return an owned `XabiWaker`.
    pub clone: unsafe extern "C" fn(*mut c_void) -> XabiWaker,
    /// Wake and consume an owned waker state.
    pub wake: unsafe extern "C" fn(*mut c_void),
    /// Wake without consuming the waker state.
    pub wake_by_ref: unsafe extern "C" fn(*mut c_void),
    /// Release an owned waker state.
    pub release: unsafe extern "C" fn(*mut c_void),
}

// The ABI waker is an opaque handle around Rust's `Waker`, which is `Send + Sync`.
// Exporters that construct custom wakers must uphold the same contract.
unsafe impl Send for XabiWaker {}
unsafe impl Sync for XabiWaker {}

impl XabiWaker {
    /// ABI version expected by this structure.
    pub const ABI_VERSION: u32 = ABI_VERSION;

    /// Validate the waker layout and required fields.
    ///
    /// ```
    /// unsafe extern "C" fn clone(_: *mut std::ffi::c_void) -> xabi::XabiWaker {
    ///     unreachable!()
    /// }
    ///
    /// unsafe extern "C" fn noop(_: *mut std::ffi::c_void) {}
    ///
    /// let waker = xabi::XabiWaker {
    ///     size: 0,
    ///     abi_version: xabi::XabiWaker::ABI_VERSION,
    ///     instance: std::ptr::null_mut(),
    ///     clone,
    ///     wake: noop,
    ///     wake_by_ref: noop,
    ///     release: noop,
    /// };
    /// assert!(waker.validate().is_err());
    /// ```
    pub fn validate(&self) -> Result<()> {
        validate_size(self.size, std::mem::size_of::<Self>(), "XabiWaker")?;
        validate_abi_version(self.abi_version, Self::ABI_VERSION, "XabiWaker")?;
        if self.instance.is_null() {
            return Err(Error::NullPointer("XabiWaker::instance"));
        }
        Ok(())
    }

    /// Borrow a Rust [`Waker`] as an ABI waker for one poll call.
    ///
    /// ```
    /// use std::sync::Arc;
    /// use std::task::{Wake, Waker};
    ///
    /// struct Noop;
    /// impl Wake for Noop {
    ///     fn wake(self: Arc<Self>) {}
    /// }
    ///
    /// let rust_waker = Waker::from(Arc::new(Noop));
    /// let waker = xabi::XabiWaker::from_waker_ref(&rust_waker);
    /// waker.validate().unwrap();
    /// ```
    pub fn from_waker_ref(waker: &Waker) -> Self {
        Self {
            size: std::mem::size_of::<Self>(),
            abi_version: ABI_VERSION,
            instance: waker as *const Waker as *mut c_void,
            clone: clone_borrowed_waker,
            wake: wake_borrowed_waker,
            wake_by_ref: wake_borrowed_waker,
            release: release_borrowed_waker,
        }
    }

    /// Convert this ABI waker into an owned Rust [`Waker`].
    ///
    /// # Safety
    ///
    /// The waker must follow the xabi waker ownership contract. The returned Rust
    /// waker owns a cloned xabi waker and will release it when dropped.
    pub unsafe fn to_waker(&self) -> Result<Waker> {
        self.validate()?;
        let owned = unsafe { (self.clone)(self.instance) };
        owned.validate()?;
        let boxed = Box::new(owned);
        let raw = RawWaker::new(Box::into_raw(boxed) as *const (), &XABI_WAKER_VTABLE);
        Ok(unsafe { Waker::from_raw(raw) })
    }
}

unsafe extern "C" fn clone_borrowed_waker(instance: *mut c_void) -> XabiWaker {
    let waker = unsafe { &*(instance as *const Waker) };
    let owned = Box::new(waker.clone());
    XabiWaker {
        size: std::mem::size_of::<XabiWaker>(),
        abi_version: ABI_VERSION,
        instance: Box::into_raw(owned) as *mut c_void,
        clone: clone_owned_waker,
        wake: wake_owned_waker,
        wake_by_ref: wake_by_ref_owned_waker,
        release: release_owned_waker,
    }
}

unsafe extern "C" fn wake_borrowed_waker(instance: *mut c_void) {
    let waker = unsafe { &*(instance as *const Waker) };
    waker.wake_by_ref();
}

unsafe extern "C" fn release_borrowed_waker(_instance: *mut c_void) {}

unsafe extern "C" fn clone_owned_waker(instance: *mut c_void) -> XabiWaker {
    let waker = unsafe { &*(instance as *const Waker) };
    let owned = Box::new(waker.clone());
    XabiWaker {
        size: std::mem::size_of::<XabiWaker>(),
        abi_version: ABI_VERSION,
        instance: Box::into_raw(owned) as *mut c_void,
        clone: clone_owned_waker,
        wake: wake_owned_waker,
        wake_by_ref: wake_by_ref_owned_waker,
        release: release_owned_waker,
    }
}

unsafe extern "C" fn wake_owned_waker(instance: *mut c_void) {
    let waker = unsafe { &*(instance as *const Waker) };
    waker.wake_by_ref();
}

unsafe extern "C" fn wake_by_ref_owned_waker(instance: *mut c_void) {
    let waker = unsafe { &*(instance as *const Waker) };
    waker.wake_by_ref();
}

unsafe extern "C" fn release_owned_waker(instance: *mut c_void) {
    if !instance.is_null() {
        drop(unsafe { Box::from_raw(instance as *mut Waker) });
    }
}

static XABI_WAKER_VTABLE: RawWakerVTable = RawWakerVTable::new(
    raw_waker_clone,
    raw_waker_wake,
    raw_waker_wake_by_ref,
    raw_waker_drop,
);

unsafe fn raw_waker_clone(data: *const ()) -> RawWaker {
    let waker = unsafe { &*(data as *const XabiWaker) };
    let cloned = unsafe { (waker.clone)(waker.instance) };
    RawWaker::new(
        Box::into_raw(Box::new(cloned)) as *const (),
        &XABI_WAKER_VTABLE,
    )
}

unsafe fn raw_waker_wake(data: *const ()) {
    let waker = unsafe { Box::from_raw(data as *mut XabiWaker) };
    unsafe {
        (waker.wake)(waker.instance);
        (waker.release)(waker.instance);
    }
}

unsafe fn raw_waker_wake_by_ref(data: *const ()) {
    let waker = unsafe { &*(data as *const XabiWaker) };
    unsafe { (waker.wake_by_ref)(waker.instance) };
}

unsafe fn raw_waker_drop(data: *const ()) {
    let waker = unsafe { Box::from_raw(data as *mut XabiWaker) };
    unsafe { (waker.release)(waker.instance) };
}

/// Future handle returned by async xabi vtable methods.
///
/// Hosts poll this handle through [`XabiFutureHandle`]. Exporters can construct a
/// handle from a Rust future with [`XabiFuture::from_result_bytes`].
#[repr(C)]
pub struct XabiFuture {
    /// Size of this structure in bytes.
    pub size: usize,
    /// ABI version for this structure.
    pub abi_version: u32,
    /// Opaque future state pointer.
    pub instance: *mut c_void,
    /// Poll the future.
    pub poll: unsafe extern "C" fn(*mut c_void, *const XabiWaker, *mut XabiResult) -> i32,
    /// Release the future state.
    pub release: unsafe extern "C" fn(*mut c_void),
}

// The async ABI allows hosts to move foreign futures across executor threads.
// `from_result_bytes` enforces `Send` for Rust futures created by xabi.
unsafe impl Send for XabiFuture {}

impl XabiFuture {
    /// ABI version expected by this structure.
    pub const ABI_VERSION: u32 = ABI_VERSION;

    /// Create an empty invalid future placeholder.
    ///
    /// ```
    /// let future = xabi::XabiFuture::empty();
    /// assert!(future.validate().is_err());
    /// ```
    pub fn empty() -> Self {
        Self {
            size: std::mem::size_of::<Self>(),
            abi_version: ABI_VERSION,
            instance: std::ptr::null_mut(),
            poll: poll_missing_future,
            release: release_missing_future,
        }
    }

    /// Validate the future layout and required fields.
    ///
    /// ```
    /// let future = xabi::XabiFuture::from_result_bytes(async {
    ///     Ok::<_, String>(b"ready".to_vec())
    /// });
    /// future.validate().unwrap();
    /// unsafe { (future.release)(future.instance) };
    /// ```
    pub fn validate(&self) -> Result<()> {
        validate_size(self.size, std::mem::size_of::<Self>(), "XabiFuture")?;
        validate_abi_version(self.abi_version, Self::ABI_VERSION, "XabiFuture")?;
        if self.instance.is_null() {
            return Err(Error::NullPointer("XabiFuture::instance"));
        }
        Ok(())
    }

    /// Convert a Rust future returning bytes into an xabi future handle.
    ///
    /// The future must be `Send` so host executors may move the foreign future
    /// between worker threads.
    ///
    /// ```
    /// use std::future::Future;
    /// use std::pin::pin;
    /// use std::sync::Arc;
    /// use std::task::{Context, Poll, Wake, Waker};
    ///
    /// struct Noop;
    /// impl Wake for Noop {
    ///     fn wake(self: Arc<Self>) {}
    /// }
    ///
    /// let future = xabi::XabiFuture::from_result_bytes(async {
    ///     Ok::<_, String>(b"hello".to_vec())
    /// });
    /// let mut future = pin!(xabi::XabiFutureHandle::new(future).unwrap());
    /// let waker = Waker::from(Arc::new(Noop));
    /// let mut cx = Context::from_waker(&waker);
    ///
    /// match Future::poll(future.as_mut(), &mut cx) {
    ///     Poll::Ready(Ok(bytes)) => assert_eq!(bytes, b"hello"),
    ///     other => panic!("unexpected poll result: {other:?}"),
    /// }
    /// ```
    pub fn from_result_bytes<F, E>(future: F) -> Self
    where
        F: Future<Output = std::result::Result<Vec<u8>, E>> + Send + 'static,
        E: ToString + 'static,
    {
        let state = Box::new(XabiFutureState {
            future: Some(Box::pin(future)),
        });
        Self {
            size: std::mem::size_of::<Self>(),
            abi_version: ABI_VERSION,
            instance: Box::into_raw(state) as *mut c_void,
            poll: poll_result_bytes_future::<F, E>,
            release: release_result_bytes_future::<F, E>,
        }
    }
}

unsafe extern "C" fn poll_missing_future(
    _instance: *mut c_void,
    _waker: *const XabiWaker,
    _out: *mut XabiResult,
) -> i32 {
    ERR_INVALID_ARGUMENT
}

unsafe extern "C" fn release_missing_future(_instance: *mut c_void) {}

struct XabiFutureState<F> {
    future: Option<Pin<Box<F>>>,
}

unsafe extern "C" fn poll_result_bytes_future<F, E>(
    instance: *mut c_void,
    waker: *const XabiWaker,
    out: *mut XabiResult,
) -> i32
where
    F: Future<Output = std::result::Result<Vec<u8>, E>> + Send + 'static,
    E: ToString + 'static,
{
    catch_unwind_code(|| {
        let Some(state) = (unsafe { (instance as *mut XabiFutureState<F>).as_mut() }) else {
            return ERR_INVALID_ARGUMENT;
        };
        let Some(out) = (unsafe { out.as_mut() }) else {
            return ERR_INVALID_ARGUMENT;
        };
        let Some(waker) = (unsafe { waker.as_ref() }) else {
            return ERR_INVALID_ARGUMENT;
        };
        let rust_waker = match unsafe { waker.to_waker() } {
            Ok(waker) => waker,
            Err(_) => return ERR_INVALID_ARGUMENT,
        };
        let mut cx = Context::from_waker(&rust_waker);
        let Some(future) = state.future.as_mut() else {
            return ERR_INVALID_ARGUMENT;
        };

        match future.as_mut().poll(&mut cx) {
            Poll::Pending => POLL_PENDING,
            Poll::Ready(Ok(bytes)) => {
                state.future = None;
                *out = XabiResult::ok(XabiOwnedBytes::from_vec(bytes));
                POLL_READY
            }
            Poll::Ready(Err(err)) => {
                state.future = None;
                *out = XabiResult::error(ERR_EXPORT, err.to_string());
                POLL_READY
            }
        }
    })
}

unsafe extern "C" fn release_result_bytes_future<F, E>(instance: *mut c_void)
where
    F: Future<Output = std::result::Result<Vec<u8>, E>> + Send + 'static,
    E: ToString + 'static,
{
    if !instance.is_null() {
        drop(unsafe { Box::from_raw(instance as *mut XabiFutureState<F>) });
    }
}

/// Rust [`Future`] wrapper around a foreign [`XabiFuture`].
///
/// ```
/// use std::future::Future;
/// use std::pin::pin;
/// use std::sync::Arc;
/// use std::task::{Context, Poll, Wake, Waker};
///
/// struct Noop;
/// impl Wake for Noop {
///     fn wake(self: Arc<Self>) {}
/// }
///
/// let future = xabi::XabiFuture::from_result_bytes(async {
///     Ok::<_, String>(b"ok".to_vec())
/// });
/// let mut future = pin!(xabi::XabiFutureHandle::new(future).unwrap());
/// let waker = Waker::from(Arc::new(Noop));
/// let mut cx = Context::from_waker(&waker);
///
/// assert!(matches!(
///     Future::poll(future.as_mut(), &mut cx),
///     Poll::Ready(Ok(bytes)) if bytes == b"ok"
/// ));
/// ```
pub struct XabiFutureHandle {
    future: XabiFuture,
}

impl XabiFutureHandle {
    /// Validate and wrap an [`XabiFuture`].
    ///
    /// ```
    /// let future = xabi::XabiFuture::empty();
    /// assert!(xabi::XabiFutureHandle::new(future).is_err());
    /// ```
    pub fn new(future: XabiFuture) -> Result<Self> {
        future.validate()?;
        Ok(Self { future })
    }
}

impl Future for XabiFutureHandle {
    type Output = Result<Vec<u8>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let waker = XabiWaker::from_waker_ref(cx.waker());
        let mut out = XabiResult::empty();
        let code = unsafe { (this.future.poll)(this.future.instance, &waker, &mut out) };
        match code {
            POLL_PENDING => Poll::Pending,
            POLL_READY => {
                if out.code == OK {
                    Poll::Ready(unsafe { out.payload.to_vec_and_free() })
                } else {
                    let message = unsafe {
                        out.payload
                            .to_string_and_free()
                            .unwrap_or_else(|err| err.to_string())
                    };
                    Poll::Ready(Err(Error::Export(message)))
                }
            }
            ERR_PANIC => Poll::Ready(Err(Error::Export(
                "future poll panicked across xabi boundary".to_string(),
            ))),
            other => Poll::Ready(Err(Error::Export(format!(
                "future poll returned xabi code {other}"
            )))),
        }
    }
}

impl Drop for XabiFutureHandle {
    fn drop(&mut self) {
        unsafe { (self.future.release)(self.future.instance) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use std::task::{Wake, Waker};

    struct CountingWaker(Arc<AtomicUsize>);

    impl Wake for CountingWaker {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }

        fn wake_by_ref(self: &Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn context() -> (Arc<AtomicUsize>, Waker) {
        let count = Arc::new(AtomicUsize::new(0));
        let waker = Waker::from(Arc::new(CountingWaker(Arc::clone(&count))));
        (count, waker)
    }

    #[test]
    fn xabi_waker_roundtrips_to_rust_waker() {
        let (count, rust_waker) = context();
        let waker = XabiWaker::from_waker_ref(&rust_waker);
        let rust_waker = unsafe { waker.to_waker() }.unwrap();

        rust_waker.wake_by_ref();

        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn xabi_future_handle_returns_ready_bytes() {
        let future = XabiFuture::from_result_bytes(async { Ok::<_, String>(b"ready".to_vec()) });
        let mut future = Box::pin(XabiFutureHandle::new(future).unwrap());
        let (_count, waker) = context();
        let mut cx = Context::from_waker(&waker);

        match Future::poll(future.as_mut(), &mut cx) {
            Poll::Ready(Ok(bytes)) => assert_eq!(bytes, b"ready"),
            other => panic!("unexpected poll result: {other:?}"),
        }
    }

    #[test]
    fn xabi_future_handle_returns_export_error_payload() {
        let future = XabiFuture::from_result_bytes(async { Err::<Vec<u8>, _>("failed") });
        let mut future = Box::pin(XabiFutureHandle::new(future).unwrap());
        let (_count, waker) = context();
        let mut cx = Context::from_waker(&waker);

        match Future::poll(future.as_mut(), &mut cx) {
            Poll::Ready(Err(err)) => assert_eq!(err.to_string(), "failed"),
            other => panic!("unexpected poll result: {other:?}"),
        }
    }

    #[test]
    fn raw_future_poll_rejects_null_arguments() {
        let future = XabiFuture::from_result_bytes(async { Ok::<_, String>(Vec::new()) });

        let code = unsafe {
            (future.poll)(
                future.instance,
                std::ptr::null(),
                std::ptr::null_mut::<XabiResult>(),
            )
        };
        unsafe { (future.release)(future.instance) };

        assert_eq!(code, ERR_INVALID_ARGUMENT);
    }
}
