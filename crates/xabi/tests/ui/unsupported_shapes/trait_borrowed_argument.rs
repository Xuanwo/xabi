#[xabi::xabi(id = "xabi.test.Bad", version = 1)]
pub trait Bad {
    fn read(&self, value: &u64) -> xabi::Result<()>;
}

fn main() {}
