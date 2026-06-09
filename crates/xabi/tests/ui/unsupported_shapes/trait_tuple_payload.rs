#[xabi::xabi(id = "xabi.test.Bad", version = 1)]
pub trait Bad {
    fn values(&self) -> xabi::Result<(u32, u32)>;
}

fn main() {}
