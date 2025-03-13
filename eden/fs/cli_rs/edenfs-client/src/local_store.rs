/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use anyhow::anyhow;
use edenfs_error::EdenFsError;
use edenfs_error::Result;

use crate::client::EdenFsClient;

impl<'a> EdenFsClient<'a> {
    pub async fn debug_clear_local_store_caches(&self) -> Result<()> {
        self.with_thrift(|thrift| thrift.debugClearLocalStoreCaches())
            .await
            .map_err(|_| EdenFsError::Other(anyhow!("failed to call debugClearLocalStoreCaches")))
    }

    pub async fn debug_compact_local_storage(&self) -> Result<()> {
        self.with_thrift(|thrift| thrift.debugCompactLocalStorage())
            .await
            .map_err(|_| EdenFsError::Other(anyhow!("failed to call debugCompactLocalStorage")))
    }

    pub async fn clear_and_compact_local_store(&self) -> Result<()> {
        self.with_thrift(|thrift| thrift.clearAndCompactLocalStore())
            .await
            .map_err(|_| EdenFsError::Other(anyhow!("failed to call clearAndCompactLocalStore")))
    }
}
