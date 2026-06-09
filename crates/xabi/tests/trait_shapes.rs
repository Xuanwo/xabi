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
        callback: XabiV1BorrowedTraitCallback,
        name: &str,
    ) -> std::result::Result<impl Child + 'static, AbiError>;
}

#[test]
fn async_callback_can_return_xabi_trait_object() {
    futures::executor::block_on(async {
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
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
    });
}

#[test]
fn short_vtable_reports_missing_method_instead_of_reading_tail() {
    futures::executor::block_on(async {
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
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
    events: std::sync::Arc<std::sync::Mutex<Vec<(String, Vec<u8>)>>>,
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

impl Factory for TestFactory {
    async fn make(
        &self,
        callback: XabiV1BorrowedTraitCallback,
        name: &str,
    ) -> std::result::Result<impl Child + 'static, AbiError> {
        callback.record("factory", name.as_bytes()).await?;
        Ok(TestChild {
            name: name.to_string(),
        })
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
