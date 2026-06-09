#[cfg(feature = "python")]
use pyo3::prelude::*;
#[cfg(feature = "python")]
use scalar_index_abi::TRAIT_ID;
use scalar_index_abi::{
    Error, IndexBuildProgress, IndexStore, Result, ScalarIndexAbi, ScalarIndexPluginAbi,
    TrainInput, TrainOutput, drain_arrow_stream,
};

#[derive(Default)]
struct DemoPlugin;

impl DemoPlugin {
    fn name(&self) -> String {
        "demo-scalar-index".to_string()
    }

    fn version(&self) -> u32 {
        1
    }

    async fn train_index(&self, input: TrainInput) -> Result<TrainOutput> {
        let rows_seen = drain_arrow_stream(input.data)?;
        let progress_events = input.op.requested_partitions.max(1);
        for _ in 0..progress_events {
            IndexBuildProgress::update(&input.progress, rows_seen).await?;
        }
        IndexStore::put(
            &input.store,
            "index.details",
            format!("rows={rows_seen}").as_bytes(),
        )
        .await?;

        Ok(TrainOutput {
            rows_seen,
            progress_events,
            details: format!("demo:index:rows={rows_seen}").into_bytes(),
        })
    }

    async fn load_index(
        &self,
        details: &[u8],
        store: scalar_index_abi::BorrowedIndexStore,
    ) -> Result<DemoIndex> {
        IndexStore::put(&store, "index.loaded", details).await?;
        let details = String::from_utf8(details.to_vec())
            .map_err(|err| Error::new(format!("invalid details: {err}")))?;
        Ok(DemoIndex { details })
    }

    async fn load_statistics(&self, details: &[u8]) -> Result<Option<String>> {
        Ok(Some(format!("statistics:{}", details.len())))
    }
}

struct DemoIndex {
    details: String,
}

impl ScalarIndexAbi for DemoIndex {
    async fn search(&self, query: &str) -> std::result::Result<String, Error> {
        Ok(format!("{}|query={query}", self.details))
    }
}

#[xabi::module]
mod exports {
    use super::*;

    #[xabi::xabi(name = "demo-scalar-index", version = 1)]
    impl ScalarIndexPluginAbi for DemoPlugin {
        fn name(&self) -> String {
            DemoPlugin::name(self)
        }

        fn version(&self) -> u32 {
            DemoPlugin::version(self)
        }

        async fn train_index(&self, input: TrainInput) -> std::result::Result<TrainOutput, Error> {
            DemoPlugin::train_index(self, input).await
        }

        async fn load_index(
            &self,
            details: &[u8],
            store: scalar_index_abi::BorrowedIndexStore,
        ) -> std::result::Result<impl ScalarIndexAbi + 'static, Error> {
            DemoPlugin::load_index(self, details, store).await
        }

        async fn load_statistics(
            &self,
            details: &[u8],
        ) -> std::result::Result<Option<String>, Error> {
            DemoPlugin::load_statistics(self, details).await
        }
    }
}

#[cfg(feature = "python")]
#[pyfunction]
fn abi_id() -> String {
    TRAIT_ID.to_string()
}

#[cfg(feature = "python")]
#[pyfunction]
fn native_plugin_name() -> String {
    "demo-scalar-index".to_string()
}

#[cfg(feature = "python")]
#[pyfunction]
fn export_version() -> u32 {
    1
}

#[cfg(feature = "python")]
#[pymodule]
fn _scalar_index_plugin(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(abi_id, m)?)?;
    m.add_function(wrap_pyfunction!(native_plugin_name, m)?)?;
    m.add_function(wrap_pyfunction!(export_version, m)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn abi_is_stable() {
        xabi_assert::assert_abi!(super::exports);
    }
}
