#[xabi::data]
#[derive(Clone, Copy)]
pub struct BuildInput {
    pub rows_seen: u64,
}

#[xabi::data]
#[derive(Debug, PartialEq, Eq)]
pub struct AbiError {
    pub message: String,
}

impl From<xabi::Error> for AbiError {
    fn from(value: xabi::Error) -> Self {
        Self::new(value.to_string())
    }
}

impl From<xabi::XabiCallError<AbiError>> for AbiError {
    fn from(value: xabi::XabiCallError<AbiError>) -> Self {
        match value {
            xabi::XabiCallError::Runtime(err) => Self::from(err),
            xabi::XabiCallError::Export(err) => err,
        }
    }
}

impl std::fmt::Display for AbiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for AbiError {}

#[xabi::xabi(id = "xabi.test.ShapePlugin", version = 1)]
pub trait ShapePlugin {
    fn name(&self) -> String;

    fn version(&self) -> u32;

    fn enabled(&self) -> bool;

    fn put(&self, details: &[u8]) -> xabi::Result<()>;

    fn optional_json(&self, details: &[u8]) -> xabi::Result<Option<Vec<u8>>>;

    fn build_sync(&self, input: BuildInput) -> xabi::Result<Vec<u8>>;

    async fn build_async(&self, input: BuildInput) -> xabi::Result<Vec<u8>>;

    async fn load_async(&self, details: &[u8]) -> xabi::Result<()>;
}

#[xabi::xabi(id = "xabi.test.Callback", version = 1)]
pub trait Callback {
    async fn record(&self, key: &str, payload: &[u8]) -> std::result::Result<(), AbiError>;
}

#[xabi::xabi(id = "xabi.test.Child", version = 1)]
pub trait Child {
    async fn describe(&self, query: &str) -> std::result::Result<String, AbiError>;
}

#[xabi::xabi(id = "xabi.test.Factory", version = 1)]
pub trait Factory {
    async fn make(
        &self,
        callback: XabiV1BorrowedTraitCallback<'_>,
        name: &str,
    ) -> std::result::Result<impl Child + 'static, AbiError>;

    async fn make_with_input(
        &self,
        input: BuildInput,
        name: &str,
    ) -> std::result::Result<(BuildInput, impl Child + 'static), AbiError>;
}

#[xabi::xabi(id = "xabi.test.Cancellation", version = 1)]
pub trait Cancellation {
    async fn complete(
        &self,
        callback: XabiV1BorrowedTraitCallback<'_>,
    ) -> std::result::Result<(), AbiError>;

    async fn wait(
        &self,
        callback: XabiV1BorrowedTraitCallback<'_>,
    ) -> std::result::Result<(), AbiError>;
}

#[xabi::xabi(id = "xabi.test.WideInteger", version = 1)]
pub trait WideInteger {
    fn current(&self) -> u128;

    fn current_signed(&self) -> i128;

    fn echo(&self, value: u128) -> xabi::Result<u128>;

    fn echo_signed(&self, value: i128) -> xabi::Result<i128>;

    async fn echo_async(&self, value: u128) -> xabi::Result<u128>;

    async fn echo_signed_async(&self, value: i128) -> xabi::Result<i128>;
}

#[xabi::xabi(id = "xabi.test.Service", version = 1)]
pub trait Service {
    fn call(&self, value: u32) -> std::result::Result<u32, AbiError>;
}

#[xabi::xabi(id = "xabi.test.Layer", version = 1)]
pub trait Layer {
    fn apply(
        &self,
        inner: XabiV1OwnedTraitService,
    ) -> std::result::Result<impl Service + 'static, AbiError>;
}

#[xabi::xabi(id = "xabi.test.OwnershipProbe", version = 1)]
pub trait OwnershipProbe {
    fn reject(&self, inner: XabiV1OwnedTraitService) -> std::result::Result<(), AbiError>;

    fn panic_after_decode(
        &self,
        inner: XabiV1OwnedTraitService,
    ) -> std::result::Result<(), AbiError>;

    async fn pending(&self, inner: XabiV1OwnedTraitService) -> std::result::Result<(), AbiError>;
}

type EventLog = std::sync::Arc<std::sync::Mutex<Vec<(String, Vec<u8>)>>>;

fn event_log() -> EventLog {
    std::sync::Arc::new(std::sync::Mutex::new(Vec::new()))
}

#[test]
fn native_128_bit_integers_cross_generated_sync_and_async_handles() {
    let unsigned = 0x0123_4567_89ab_cdef_fedc_ba98_7654_3210_u128;
    let signed = -0x0123_4567_89ab_cdef_0123_4567_89ab_cdef_i128;
    let integer = XabiV1OwnedTraitWideInteger::new(TestWideInteger(unsigned, signed));

    assert_eq!(integer.xabi_borrow().current().unwrap(), unsigned);
    assert_eq!(integer.xabi_borrow().current_signed().unwrap(), signed);
    assert_eq!(integer.xabi_borrow().echo(u128::MAX).unwrap(), u128::MAX);
    assert_eq!(
        integer.xabi_borrow().echo_signed(i128::MIN).unwrap(),
        i128::MIN
    );
    assert_eq!(
        futures::executor::block_on(integer.xabi_borrow().echo_async(unsigned)).unwrap(),
        unsigned
    );
    assert_eq!(
        futures::executor::block_on(integer.xabi_borrow().echo_signed_async(signed)).unwrap(),
        signed
    );
}

#[test]
fn ownership_transferring_trait_handle_can_be_retained_by_layer() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let drops = std::sync::Arc::new(AtomicUsize::new(0));
    let inner =
        XabiV1OwnedTraitService::new(TrackedService::new(40, std::sync::Arc::clone(&drops)));
    let layer = XabiV1OwnedTraitLayer::new(AddLayer(2));

    let decorated = layer
        .xabi_borrow()
        .apply(inner)
        .expect("layer accepts ownership of the inner service");
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    assert_eq!(decorated.xabi_borrow().call(1).unwrap(), 43);

    drop(decorated);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[test]
fn ownership_transfer_releases_once_on_validation_failure() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let drops = std::sync::Arc::new(AtomicUsize::new(0));
    let inner = XabiV1OwnedTraitService::new(TrackedService::new(0, std::sync::Arc::clone(&drops)));
    unsafe {
        let vtable = inner.xabi_as_ptr() as *mut XabiV1VtableTraitService;
        (*vtable).abi_version = XabiV1VtableTraitService::ABI_VERSION + 1;
    }
    let probe = XabiV1OwnedTraitOwnershipProbe::new(TestOwnershipProbe);

    let err = probe
        .xabi_borrow()
        .reject(inner)
        .expect_err("invalid transferred vtable must fail decoding");
    assert!(matches!(err, xabi::XabiCallError::Runtime(_)));
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[test]
fn ownership_transfer_releases_once_when_export_returns_before_decode() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let drops = std::sync::Arc::new(AtomicUsize::new(0));
    let inner = XabiV1OwnedTraitService::new(TrackedService::new(0, std::sync::Arc::clone(&drops)));
    let probe = XabiV1OwnedTraitOwnershipProbe::new(TestOwnershipProbe);
    unsafe {
        let vtable = probe.xabi_as_ptr() as *mut XabiV1VtableTraitOwnershipProbe;
        (*vtable).instance = std::ptr::null_mut();
    }

    let err = probe
        .xabi_borrow()
        .reject(inner)
        .expect_err("invalid export instance must fail before argument decoding");
    assert!(matches!(err, xabi::XabiCallError::Runtime(_)));
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[test]
fn ownership_transfer_releases_once_on_export_error_and_panic() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let probe = XabiV1OwnedTraitOwnershipProbe::new(TestOwnershipProbe);

    let error_drops = std::sync::Arc::new(AtomicUsize::new(0));
    let inner =
        XabiV1OwnedTraitService::new(TrackedService::new(0, std::sync::Arc::clone(&error_drops)));
    let err = probe
        .xabi_borrow()
        .reject(inner)
        .expect_err("probe returns an export error");
    assert!(matches!(err, xabi::XabiCallError::Export(_)));
    assert_eq!(error_drops.load(Ordering::SeqCst), 1);

    let panic_drops = std::sync::Arc::new(AtomicUsize::new(0));
    let inner =
        XabiV1OwnedTraitService::new(TrackedService::new(0, std::sync::Arc::clone(&panic_drops)));
    let err = probe
        .xabi_borrow()
        .panic_after_decode(inner)
        .expect_err("probe panic must be contained by the ABI thunk");
    assert!(matches!(err, xabi::XabiCallError::Runtime(_)));
    assert_eq!(panic_drops.load(Ordering::SeqCst), 1);
}

#[test]
fn ownership_transfer_releases_once_when_async_call_is_cancelled() {
    use std::future::Future;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::task::{Context, Poll};

    let probe = XabiV1OwnedTraitOwnershipProbe::new(TestOwnershipProbe);
    let borrowed = probe.xabi_borrow();

    let before_poll_drops = std::sync::Arc::new(AtomicUsize::new(0));
    let inner = XabiV1OwnedTraitService::new(TrackedService::new(
        0,
        std::sync::Arc::clone(&before_poll_drops),
    ));
    let future = borrowed.pending(inner);
    drop(future);
    assert_eq!(before_poll_drops.load(Ordering::SeqCst), 1);

    let after_poll_drops = std::sync::Arc::new(AtomicUsize::new(0));
    let inner = XabiV1OwnedTraitService::new(TrackedService::new(
        0,
        std::sync::Arc::clone(&after_poll_drops),
    ));
    let mut future = Box::pin(borrowed.pending(inner));
    let waker = futures::task::noop_waker();
    let mut cx = Context::from_waker(&waker);
    assert!(matches!(future.as_mut().poll(&mut cx), Poll::Pending));
    assert_eq!(after_poll_drops.load(Ordering::SeqCst), 0);

    drop(future);
    assert_eq!(after_poll_drops.load(Ordering::SeqCst), 1);
}

#[test]
fn async_callback_can_return_xabi_trait_object() {
    futures::executor::block_on(async {
        let events = event_log();
        let callback = XabiV1OwnedTraitCallback::new(TestCallback {
            events: std::sync::Arc::clone(&events),
        });
        let factory = XabiV1OwnedTraitFactory::new(TestFactory);

        let child = factory
            .xabi_borrow()
            .make(callback.xabi_borrow(), "demo")
            .await
            .expect("factory returns child");
        let description = child
            .xabi_borrow()
            .describe("needle")
            .await
            .expect("child responds");

        assert_eq!(description, "demo:needle");
        assert_eq!(
            *events.lock().unwrap(),
            vec![("factory".to_string(), b"demo".to_vec())]
        );

        let (input, child) = factory
            .xabi_borrow()
            .make_with_input(BuildInput::new(42), "pair")
            .await
            .expect("factory returns input and child");
        assert_eq!(input.rows_seen, 42);
        let description = child
            .xabi_borrow()
            .describe("needle")
            .await
            .expect("child responds");
        assert_eq!(description, "pair:needle");
    });
}

#[test]
fn completed_call_releases_the_borrow_before_the_callback_owner() {
    use std::sync::atomic::{AtomicBool, Ordering};

    let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let callback_alive = std::sync::Arc::new(AtomicBool::new(true));
    let completion_saw_live_callback = std::sync::Arc::new(AtomicBool::new(false));
    let callback = XabiV1OwnedTraitCallback::new(DroppingCallback {
        events: std::sync::Arc::clone(&events),
        alive: std::sync::Arc::clone(&callback_alive),
    });
    let cancellation = XabiV1OwnedTraitCancellation::new(TestCancellation {
        events: std::sync::Arc::clone(&events),
        callback_alive: std::sync::Arc::clone(&callback_alive),
        future_drop_saw_live_callback: std::sync::Arc::clone(&completion_saw_live_callback),
    });

    futures::executor::block_on(cancellation.xabi_borrow().complete(callback.xabi_borrow()))
        .unwrap();

    assert!(completion_saw_live_callback.load(Ordering::SeqCst));
    assert!(callback_alive.load(Ordering::SeqCst));
    assert_eq!(
        *events.lock().unwrap(),
        vec!["callback-called", "future-completed"]
    );

    drop(callback);
    assert!(!callback_alive.load(Ordering::SeqCst));
    assert_eq!(
        *events.lock().unwrap(),
        vec!["callback-called", "future-completed", "callback-dropped"]
    );
}

#[test]
fn cancelled_call_releases_the_future_before_the_callback_owner() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::task::{Context, Poll};

    let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let callback_alive = std::sync::Arc::new(AtomicBool::new(true));
    let cancellation_saw_live_callback = std::sync::Arc::new(AtomicBool::new(false));
    let callback = XabiV1OwnedTraitCallback::new(DroppingCallback {
        events: std::sync::Arc::clone(&events),
        alive: std::sync::Arc::clone(&callback_alive),
    });
    let cancellation = XabiV1OwnedTraitCancellation::new(TestCancellation {
        events: std::sync::Arc::clone(&events),
        callback_alive: std::sync::Arc::clone(&callback_alive),
        future_drop_saw_live_callback: std::sync::Arc::clone(&cancellation_saw_live_callback),
    });
    let cancellation = cancellation.xabi_borrow();
    let callback_borrow = callback.xabi_borrow();
    let mut future = Box::pin(cancellation.wait(callback_borrow));
    let waker = futures::task::noop_waker();
    let mut context = Context::from_waker(&waker);

    assert!(matches!(future.as_mut().poll(&mut context), Poll::Pending));
    assert_eq!(*events.lock().unwrap(), vec!["callback-called"]);
    drop(future);

    assert!(cancellation_saw_live_callback.load(Ordering::SeqCst));
    assert!(callback_alive.load(Ordering::SeqCst));
    assert_eq!(
        *events.lock().unwrap(),
        vec!["callback-called", "future-cancelled"]
    );

    drop(callback);
    assert!(!callback_alive.load(Ordering::SeqCst));
    assert_eq!(
        *events.lock().unwrap(),
        vec!["callback-called", "future-cancelled", "callback-dropped"]
    );
}

#[test]
fn short_vtable_reports_missing_method_instead_of_reading_tail() {
    futures::executor::block_on(async {
        let events = event_log();
        let callback = XabiV1OwnedTraitCallback::new(TestCallback {
            events: std::sync::Arc::clone(&events),
        });
        let factory = XabiV1OwnedTraitFactory::new(TestFactory);

        unsafe {
            let vtable = factory.xabi_as_ptr() as *mut XabiV1VtableTraitFactory;
            (*vtable).size = XabiV1VtableTraitFactory::MIN_SIZE;
        }

        let err = match factory
            .xabi_borrow()
            .make(callback.xabi_borrow(), "demo")
            .await
        {
            Ok(_) => panic!("short vtable must not expose the make method"),
            Err(err) => err,
        };

        assert!(err.to_string().contains("not available in this vtable"));
    });
}

struct TestCallback {
    events: EventLog,
}

struct DroppingCallback {
    events: std::sync::Arc<std::sync::Mutex<Vec<&'static str>>>,
    alive: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl Callback for DroppingCallback {
    async fn record(&self, _key: &str, _payload: &[u8]) -> std::result::Result<(), AbiError> {
        self.events.lock().unwrap().push("callback-called");
        Ok(())
    }
}

impl Drop for DroppingCallback {
    fn drop(&mut self) {
        self.alive.store(false, std::sync::atomic::Ordering::SeqCst);
        self.events.lock().unwrap().push("callback-dropped");
    }
}

struct TestCancellation {
    events: std::sync::Arc<std::sync::Mutex<Vec<&'static str>>>,
    callback_alive: std::sync::Arc<std::sync::atomic::AtomicBool>,
    future_drop_saw_live_callback: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

struct CancellationGuard<'a> {
    cancellation: &'a TestCancellation,
    event: &'static str,
}

impl Drop for CancellationGuard<'_> {
    fn drop(&mut self) {
        self.cancellation.future_drop_saw_live_callback.store(
            self.cancellation
                .callback_alive
                .load(std::sync::atomic::Ordering::SeqCst),
            std::sync::atomic::Ordering::SeqCst,
        );
        self.cancellation.events.lock().unwrap().push(self.event);
    }
}

impl Cancellation for TestCancellation {
    async fn complete(
        &self,
        callback: XabiV1BorrowedTraitCallback<'_>,
    ) -> std::result::Result<(), AbiError> {
        let _guard = CancellationGuard {
            cancellation: self,
            event: "future-completed",
        };
        callback.record("started", &[]).await?;
        Ok(())
    }

    async fn wait(
        &self,
        callback: XabiV1BorrowedTraitCallback<'_>,
    ) -> std::result::Result<(), AbiError> {
        let _guard = CancellationGuard {
            cancellation: self,
            event: "future-cancelled",
        };
        callback.record("started", &[]).await?;
        std::future::pending().await
    }
}

impl Callback for TestCallback {
    async fn record(&self, key: &str, payload: &[u8]) -> std::result::Result<(), AbiError> {
        self.events
            .lock()
            .unwrap()
            .push((key.to_string(), payload.to_vec()));
        Ok(())
    }
}

struct TestFactory;

struct TestWideInteger(u128, i128);

struct TrackedService {
    base: u32,
    drops: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl TrackedService {
    fn new(base: u32, drops: std::sync::Arc<std::sync::atomic::AtomicUsize>) -> Self {
        Self { base, drops }
    }
}

impl Drop for TrackedService {
    fn drop(&mut self) {
        self.drops.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}

impl Service for TrackedService {
    fn call(&self, value: u32) -> std::result::Result<u32, AbiError> {
        Ok(self.base + value)
    }
}

struct AddLayer(u32);

impl Layer for AddLayer {
    fn apply(
        &self,
        inner: XabiV1OwnedTraitService,
    ) -> std::result::Result<impl Service + 'static, AbiError> {
        Ok(LayeredService {
            inner,
            increment: self.0,
        })
    }
}

struct LayeredService {
    inner: XabiV1OwnedTraitService,
    increment: u32,
}

impl Service for LayeredService {
    fn call(&self, value: u32) -> std::result::Result<u32, AbiError> {
        let value = self.inner.xabi_borrow().call(value)?;
        Ok(value + self.increment)
    }
}

struct TestOwnershipProbe;

impl OwnershipProbe for TestOwnershipProbe {
    fn reject(&self, _inner: XabiV1OwnedTraitService) -> std::result::Result<(), AbiError> {
        Err(AbiError::new("rejected"))
    }

    fn panic_after_decode(
        &self,
        _inner: XabiV1OwnedTraitService,
    ) -> std::result::Result<(), AbiError> {
        panic!("ownership probe panic")
    }

    async fn pending(&self, inner: XabiV1OwnedTraitService) -> std::result::Result<(), AbiError> {
        std::future::pending::<()>().await;
        drop(inner);
        Ok(())
    }
}

impl WideInteger for TestWideInteger {
    fn current(&self) -> u128 {
        self.0
    }

    fn current_signed(&self) -> i128 {
        self.1
    }

    fn echo(&self, value: u128) -> xabi::Result<u128> {
        Ok(value)
    }

    fn echo_signed(&self, value: i128) -> xabi::Result<i128> {
        Ok(value)
    }

    async fn echo_async(&self, value: u128) -> xabi::Result<u128> {
        Ok(value)
    }

    async fn echo_signed_async(&self, value: i128) -> xabi::Result<i128> {
        Ok(value)
    }
}

impl Factory for TestFactory {
    async fn make(
        &self,
        callback: XabiV1BorrowedTraitCallback<'_>,
        name: &str,
    ) -> std::result::Result<impl Child + 'static, AbiError> {
        callback.record("factory", name.as_bytes()).await?;
        Ok(TestChild {
            name: name.to_string(),
        })
    }

    async fn make_with_input(
        &self,
        input: BuildInput,
        name: &str,
    ) -> std::result::Result<(BuildInput, impl Child + 'static), AbiError> {
        Ok((
            input,
            TestChild {
                name: name.to_string(),
            },
        ))
    }
}

struct TestChild {
    name: String,
}

impl Child for TestChild {
    async fn describe(&self, query: &str) -> std::result::Result<String, AbiError> {
        Ok(format!("{}:{query}", self.name))
    }
}
