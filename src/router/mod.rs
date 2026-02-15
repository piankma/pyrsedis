pub mod cluster;
pub mod sentinel;
pub mod standalone;

pub use cluster::ClusterRouter;
pub use sentinel::SentinelRouter;
pub use standalone::StandaloneRouter;

use crate::error::Result;
use crate::resp::types::RespValue;

/// Common interface for all Redis topology routers.
///
/// Implementations handle the details of single-server, cluster, or
/// sentinel-managed deployments behind a uniform API.
pub trait Router: Send + Sync {
    /// Execute a single command and return the response.
    fn execute(
        &self,
        args: &[&str],
    ) -> impl std::future::Future<Output = Result<RespValue>> + Send;

    /// Execute a pipeline (batch of commands) and return all responses.
    fn pipeline(
        &self,
        commands: &[Vec<String>],
    ) -> impl std::future::Future<Output = Result<Vec<RespValue>>> + Send;

    /// Number of idle connections across pools.
    fn pool_idle_count(&self) -> usize;

    /// Number of available connection slots across pools.
    fn pool_available(&self) -> usize;
}
