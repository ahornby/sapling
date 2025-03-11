/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use edenfs_error::Result;
use edenfs_error::ResultExt;
use futures::stream::BoxStream;
use thrift_streaming::EdenStartStatusUpdate;
use thrift_streaming_clients::errors::StreamStartStatusStreamError;
use thrift_types::edenfs::DaemonInfo;
use thrift_types::fb303_core::fb303_status;
use tracing::event;
use tracing::Level;

use crate::client::EdenFsClient;

pub type StartStatusStream =
    BoxStream<'static, Result<EdenStartStatusUpdate, StreamStartStatusStreamError>>;

pub trait DaemonHealthy {
    fn is_healthy(&self) -> bool;
}

impl DaemonHealthy for DaemonInfo {
    fn is_healthy(&self) -> bool {
        self.status
            .map_or_else(|| false, |val| val == fb303_status::ALIVE)
    }
}

impl<'a> EdenFsClient<'a> {
    pub async fn get_health(&self) -> Result<DaemonInfo> {
        event!(Level::DEBUG, "connected to EdenFS daemon");
        self.client.getDaemonInfo().await.from_err()
    }

    pub async fn get_health_with_startup_updates_included(
        &self,
    ) -> Result<(DaemonInfo, StartStatusStream)> {
        use edenfs_error::EdenFsError;
        use thrift_streaming_clients::errors::StreamStartStatusError;
        use thrift_types::fbthrift::ApplicationExceptionErrorCode;

        let result = self.streaming_client.streamStartStatus().await;
        match result {
            Err(StreamStartStatusError::ApplicationException(e))
                if e.type_ == ApplicationExceptionErrorCode::UnknownMethod =>
            {
                Err(EdenFsError::UnknownMethod(e.message))
            }
            r => r.from_err(),
        }
    }
}
