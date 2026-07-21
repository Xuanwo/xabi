#[xabi::xabi(id = "xabi.test.ui.Callback", version = 1)]
trait Callback {
    fn call(&self) -> xabi::Result<()>;
}

#[xabi::xabi(id = "xabi.test.ui.Consumer", version = 1)]
trait Consumer {
    async fn consume(
        &self,
        callback: XabiV1BorrowedTraitCallback<'_>,
    ) -> xabi::Result<()>;
}

struct CallbackImpl;

impl Callback for CallbackImpl {
    fn call(&self) -> xabi::Result<()> {
        Ok(())
    }
}

struct ConsumerImpl;

impl Consumer for ConsumerImpl {
    async fn consume(
        &self,
        callback: XabiV1BorrowedTraitCallback<'_>,
    ) -> xabi::Result<()> {
        let _ = callback.call();
        Ok(())
    }
}

fn main() {
    let consumer = XabiV1OwnedTraitConsumer::new(ConsumerImpl);
    let consumer = consumer.xabi_borrow();
    let future;
    {
        let callback = XabiV1OwnedTraitCallback::new(CallbackImpl);
        future = consumer.consume(callback.xabi_borrow());
    }
    drop(future);
}
