#[xabi::xabi(id = "xabi.test.Bad", version = 1)]
pub trait Bad {
    fn write(&self, value: &mut [u8]) -> xabi::Result<()>;
}

fn main() {}
