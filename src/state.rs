use std::sync::Arc;

use crate::config::Config;
use crate::upstream::NutstoreClient;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub nutstore: NutstoreClient,
}

impl AppState {
    pub fn new(config: Config, nutstore: NutstoreClient) -> Self {
        Self {
            config: Arc::new(config),
            nutstore,
        }
    }
}
