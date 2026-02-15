pub mod client;
pub mod config;
pub mod connection;
pub mod crc16;
pub mod error;
pub mod graph;
pub mod resp;
pub mod response;
pub mod router;
pub mod runtime;

use pyo3::prelude::*;

/// The native Python module.
#[pymodule]
fn _pyrsedis(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add_class::<client::Redis>()?;
    m.add_class::<client::Pipeline>()?;
    error::register_exceptions(m)?;
    Ok(())
}
