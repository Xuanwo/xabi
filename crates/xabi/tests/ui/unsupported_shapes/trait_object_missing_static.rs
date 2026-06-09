#[xabi::xabi(id = "xabi.test.Child", version = 1)]
pub trait Child {
    fn name(&self) -> String;
}

#[xabi::xabi(id = "xabi.test.Bad", version = 1)]
pub trait Bad {
    async fn make(&self) -> xabi::Result<impl Child>;
}

fn main() {}
