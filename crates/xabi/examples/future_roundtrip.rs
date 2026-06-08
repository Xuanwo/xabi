use std::future::Future;
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

struct Noop;

impl Wake for Noop {
    fn wake(self: Arc<Self>) {}
}

fn main() -> xabi::Result<()> {
    let future =
        xabi::XabiFuture::from_result_bytes(async { Ok::<_, String>(b"async-payload".to_vec()) });
    let mut future = pin!(xabi::XabiFutureHandle::new(future)?);

    let waker = Waker::from(Arc::new(Noop));
    let mut cx = Context::from_waker(&waker);

    match Future::poll(future.as_mut(), &mut cx) {
        Poll::Ready(Ok(bytes)) => {
            assert_eq!(bytes, b"async-payload");
            Ok(())
        }
        Poll::Ready(Err(err)) => Err(err),
        Poll::Pending => Err(xabi::Error::Export(
            "future unexpectedly pending".to_string(),
        )),
    }
}
