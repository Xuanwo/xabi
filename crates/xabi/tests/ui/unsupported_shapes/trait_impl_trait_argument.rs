pub trait Callback {}

#[xabi::xabi(id = "xabi.test.Bad", version = 1)]
pub trait Bad {
    fn call(&self, callback: impl Callback) -> xabi::Result<()>;
}

fn main() {}
