use async_trait::async_trait;
use std::time::Duration;
use tokio::time::Instant;

use crate::dashboard::models::ProviderError;

pub mod claude;
pub mod codex;
pub mod cursor;
pub mod github;
pub mod grok;

#[async_trait]
pub trait DataProvider<T>: Send + Sync {
    async fn fetch(&self) -> Result<T, ProviderError>;
}

pub(super) fn remaining_timeout(deadline: Instant) -> Result<Duration, ProviderError> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|remaining| !remaining.is_zero())
        .ok_or(ProviderError::Timeout)
}
