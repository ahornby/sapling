/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

//! Define common EdenFS errors

use std::path::PathBuf;
use std::result::Result as StdResult;

use thiserror::Error;
#[allow(unused_imports)]
use thrift_streaming_clients::errors::StreamJournalChangedError;
#[allow(unused_imports)]
use thrift_streaming_clients::errors::StreamStartStatusError;
use thrift_thriftclients::thrift::errors::AddBindMountError;
use thrift_thriftclients::thrift::errors::ChangesSinceV2Error;
use thrift_thriftclients::thrift::errors::ClearAndCompactLocalStoreError;
use thrift_thriftclients::thrift::errors::DebugClearLocalStoreCachesError;
use thrift_thriftclients::thrift::errors::DebugCompactLocalStorageError;
use thrift_thriftclients::thrift::errors::EnsureMaterializedError;
use thrift_thriftclients::thrift::errors::FlushStatsNowError;
use thrift_thriftclients::thrift::errors::GetAttributesFromFilesError;
use thrift_thriftclients::thrift::errors::GetAttributesFromFilesV2Error;
use thrift_thriftclients::thrift::errors::GetConfigError;
use thrift_thriftclients::thrift::errors::GetCurrentJournalPositionError;
use thrift_thriftclients::thrift::errors::GetCurrentSnapshotInfoError;
use thrift_thriftclients::thrift::errors::GetDaemonInfoError;
use thrift_thriftclients::thrift::errors::GetSHA1Error;
use thrift_thriftclients::thrift::errors::GetScmStatusV2Error;
use thrift_thriftclients::thrift::errors::GlobFilesError;
use thrift_thriftclients::thrift::errors::ListMountsError;
use thrift_thriftclients::thrift::errors::ReaddirError;
use thrift_thriftclients::thrift::errors::RemoveBindMountError;
use thrift_thriftclients::thrift::errors::RemoveRecursivelyError;
use thrift_thriftclients::thrift::errors::SetPathObjectIdError;
#[cfg(target_os = "macos")]
use thrift_thriftclients::thrift::errors::StartFileAccessMonitorError;
use thrift_thriftclients::thrift::errors::StartRecordingBackingStoreFetchError;
#[cfg(target_os = "macos")]
use thrift_thriftclients::thrift::errors::StopFileAccessMonitorError;
use thrift_thriftclients::thrift::errors::StopRecordingBackingStoreFetchError;
use thrift_thriftclients::thrift::errors::SynchronizeWorkingCopyError;
use thrift_thriftclients::thrift::errors::UnmountError;
use thrift_thriftclients::thrift::errors::UnmountV2Error;
use tokio::time::error::Elapsed;

pub type ExitCode = i32;
pub type Result<T, E = EdenFsError> = std::result::Result<T, E>;

#[derive(Error, Debug)]
pub enum EdenFsError {
    #[error("Timed out when connecting to EdenFS daemon: {0:?}")]
    ThriftConnectionTimeout(PathBuf),

    #[error("IO error when connecting to EdenFS daemon: {0:?}")]
    ThriftIoError(#[source] std::io::Error),

    #[error("Error when loading configurations: {0}")]
    ConfigurationError(String),

    #[error("EdenFS did not respond within set timeout: {0}")]
    RequestTimeout(Elapsed),

    #[error("The running version of the EdenFS daemon doesn't know that method.")]
    UnknownMethod(String),

    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

pub trait ResultExt<T> {
    /// Convert any error in a `Result` type into [`EdenFsError`]. Use this when ?-operator can't
    /// automatically infer the type.
    ///
    /// Note: This method will unconditionally convert everything into [`EdenFsError::Other`]
    /// variant even if there is a better match.
    fn from_err(self) -> StdResult<T, EdenFsError>;
}

impl<T, E: std::error::Error + Send + Sync + 'static> ResultExt<T> for StdResult<T, E> {
    fn from_err(self) -> StdResult<T, EdenFsError> {
        self.map_err(|e| EdenFsError::Other(e.into()))
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum ErrorHandlingStrategy {
    Reconnect,
    Retry,
    Abort,
}

pub trait HasErrorHandlingStrategy {
    fn get_error_handling_strategy(&self) -> ErrorHandlingStrategy;
}

macro_rules! impl_has_error_handling_strategy {
    ($err: ident) => {
        impl HasErrorHandlingStrategy for $err {
            fn get_error_handling_strategy(&self) -> ErrorHandlingStrategy {
                match self {
                    Self::ThriftError(..) => ErrorHandlingStrategy::Reconnect,
                    Self::ApplicationException(..) => ErrorHandlingStrategy::Retry,
                    Self::ex(..) => ErrorHandlingStrategy::Abort,
                }
            }
        }
    };
}

impl_has_error_handling_strategy!(AddBindMountError);
impl_has_error_handling_strategy!(ChangesSinceV2Error);
impl_has_error_handling_strategy!(ClearAndCompactLocalStoreError);
impl_has_error_handling_strategy!(DebugClearLocalStoreCachesError);
impl_has_error_handling_strategy!(DebugCompactLocalStorageError);
impl_has_error_handling_strategy!(EnsureMaterializedError);
impl_has_error_handling_strategy!(FlushStatsNowError);
impl_has_error_handling_strategy!(GetAttributesFromFilesError);
impl_has_error_handling_strategy!(GetAttributesFromFilesV2Error);
impl_has_error_handling_strategy!(GetConfigError);
impl_has_error_handling_strategy!(GetCurrentJournalPositionError);
impl_has_error_handling_strategy!(GetCurrentSnapshotInfoError);
impl_has_error_handling_strategy!(GetDaemonInfoError);
impl_has_error_handling_strategy!(GetScmStatusV2Error);
impl_has_error_handling_strategy!(GetSHA1Error);
impl_has_error_handling_strategy!(GlobFilesError);
impl_has_error_handling_strategy!(ListMountsError);
impl_has_error_handling_strategy!(ReaddirError);
impl_has_error_handling_strategy!(RemoveBindMountError);
impl_has_error_handling_strategy!(RemoveRecursivelyError);
impl_has_error_handling_strategy!(SetPathObjectIdError);
#[cfg(target_os = "macos")]
impl_has_error_handling_strategy!(StartFileAccessMonitorError);
impl_has_error_handling_strategy!(StartRecordingBackingStoreFetchError);
#[cfg(target_os = "macos")]
impl_has_error_handling_strategy!(StopFileAccessMonitorError);
impl_has_error_handling_strategy!(StopRecordingBackingStoreFetchError);
impl_has_error_handling_strategy!(SynchronizeWorkingCopyError);
impl_has_error_handling_strategy!(UnmountError);
impl_has_error_handling_strategy!(UnmountV2Error);

// TODO: Add error handling strategy for streaming endpoints
//impl_has_error_handling_strategy!(StreamJournalChangedError);
