#[xabi::xabi(id = "xabi.test.Bad", version = 1)]
pub trait Bad {
    async fn make(&self) -> xabi::Result<impl Iterator<Item = u8> + 'static>;
}

fn main() {}
