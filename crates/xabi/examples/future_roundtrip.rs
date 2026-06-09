use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

fn main() -> xabi::Result<()> {
    let future = xabi::XabiFuture::from_result_bytes(async {
        Ok::<_, xabi::Error>(b"async-payload".to_vec())
    });
    let mut future = pin!(xabi::XabiFutureHandle::new(future)?);

    let waker = Waker::noop();
    let mut cx = Context::from_waker(waker);

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
