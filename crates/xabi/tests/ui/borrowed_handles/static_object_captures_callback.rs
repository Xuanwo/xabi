#[xabi::xabi(id = "xabi.test.ui.Callback", version = 1)]
trait Callback {
    fn call(&self) -> xabi::Result<()>;
}

#[xabi::xabi(id = "xabi.test.ui.Child", version = 1)]
trait Child {
    fn call(&self) -> xabi::Result<()>;
}

#[xabi::xabi(id = "xabi.test.ui.Factory", version = 1)]
trait Factory {
    async fn make(
        &self,
        callback: XabiV1BorrowedTraitCallback<'_>,
    ) -> xabi::Result<impl Child + 'static>;
}

struct StoredCallback<'a> {
    callback: XabiV1BorrowedTraitCallback<'a>,
}

impl Child for StoredCallback<'static> {
    fn call(&self) -> xabi::Result<()> {
        let _ = self.callback.call();
        Ok(())
    }
}

struct FactoryImpl;

impl Factory for FactoryImpl {
    async fn make(
        &self,
        callback: XabiV1BorrowedTraitCallback<'_>,
    ) -> xabi::Result<impl Child + 'static> {
        Ok(StoredCallback { callback })
    }
}

fn main() {}
