#[xabi::xabi(id = "xabi.test.Bad", version = 1)]
pub trait Bad {
    fn name(&self) -> &str;
}

fn main() {}
