/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::collections::HashMap;

use ::sql::mysql;
use ::sql::mysql_async::prelude::ConvIr;
use ::sql::mysql_async::prelude::FromValue;
use ::sql::mysql_async::FromValueError;
use ::sql::mysql_async::Value;
use anyhow::Error;
use async_trait::async_trait;
use auto_impl::auto_impl;
use context::CoreContext;
use metaconfig_types::CommitSyncConfigVersion;
use mononoke_types::ChangesetId;
use mononoke_types::RepositoryId;
use thiserror::Error;

#[derive(Debug, Eq, Error, PartialEq)]
pub enum ErrorKind {
    #[error(
        "tried to insert inconsistent small bcs id {actual_bcs_id:?} version {actual_config_version:?}, while db has {expected_bcs_id:?} version {expected_config_version:?}"
    )]
    InconsistentWorkingCopyEntry {
        expected_bcs_id: Option<ChangesetId>,
        expected_config_version: Option<CommitSyncConfigVersion>,
        actual_bcs_id: Option<ChangesetId>,
        actual_config_version: Option<CommitSyncConfigVersion>,
    },
    #[error(
        "tried to insert inconsistent version for {large_cs_id} in repo {large_repo_id}: tried to insert {expected_version_name}, found {actual_version_name}"
    )]
    InconsistentLargeRepoCommitVersion {
        large_repo_id: RepositoryId,
        large_cs_id: ChangesetId,
        expected_version_name: CommitSyncConfigVersion,
        actual_version_name: CommitSyncConfigVersion,
    },
}

// Repo that originally contained the synced commit
#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq, mysql::OptTryFromRowField)]
pub enum SyncedCommitSourceRepo {
    Large,
    Small,
}

impl ConvIr<SyncedCommitSourceRepo> for SyncedCommitSourceRepo {
    fn new(v: Value) -> Result<Self, FromValueError> {
        use SyncedCommitSourceRepo::*;

        match v {
            Value::Bytes(ref b) if b == b"large" => Ok(Large),
            Value::Bytes(ref b) if b == b"small" => Ok(Small),
            v => Err(FromValueError(v)),
        }
    }

    fn commit(self) -> SyncedCommitSourceRepo {
        self
    }

    fn rollback(self) -> Value {
        self.into()
    }
}

impl FromValue for SyncedCommitSourceRepo {
    type Intermediate = SyncedCommitSourceRepo;
}

impl From<SyncedCommitSourceRepo> for Value {
    fn from(source_repo: SyncedCommitSourceRepo) -> Self {
        use SyncedCommitSourceRepo::*;

        match source_repo {
            Small => Value::Bytes(b"small".to_vec()),
            Large => Value::Bytes(b"large".to_vec()),
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SyncedCommitMappingEntry {
    pub large_repo_id: RepositoryId,
    pub large_bcs_id: ChangesetId,
    pub small_repo_id: RepositoryId,
    pub small_bcs_id: ChangesetId,
    pub version_name: Option<CommitSyncConfigVersion>,
    pub source_repo: Option<SyncedCommitSourceRepo>,
}

impl SyncedCommitMappingEntry {
    pub fn new(
        large_repo_id: RepositoryId,
        large_bcs_id: ChangesetId,
        small_repo_id: RepositoryId,
        small_bcs_id: ChangesetId,
        version_name: CommitSyncConfigVersion,
        source_repo: SyncedCommitSourceRepo,
    ) -> Self {
        Self {
            large_repo_id,
            large_bcs_id,
            small_repo_id,
            small_bcs_id,
            version_name: Some(version_name),
            source_repo: Some(source_repo),
        }
    }

    pub(crate) fn into_equivalent_working_copy_entry(self) -> EquivalentWorkingCopyEntry {
        let Self {
            large_repo_id,
            large_bcs_id,
            small_repo_id,
            small_bcs_id,
            version_name,
            source_repo: _,
        } = self;

        EquivalentWorkingCopyEntry {
            large_repo_id,
            large_bcs_id,
            small_repo_id,
            small_bcs_id: Some(small_bcs_id),
            version_name,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct EquivalentWorkingCopyEntry {
    pub large_repo_id: RepositoryId,
    pub large_bcs_id: ChangesetId,
    pub small_repo_id: RepositoryId,
    pub small_bcs_id: Option<ChangesetId>,
    pub version_name: Option<CommitSyncConfigVersion>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum WorkingCopyEquivalence {
    /// There's no matching working copy. It can happen if a pre-big-merge commit from one small
    /// repo is mapped into another small repo
    NoWorkingCopy(CommitSyncConfigVersion),
    /// ChangesetId of matching working copy and CommitSyncConfigVersion that was used for mapping
    WorkingCopy(ChangesetId, CommitSyncConfigVersion),
}

#[async_trait]
#[auto_impl(Arc)]
#[facet::facet]
pub trait SyncedCommitMapping: Send + Sync {
    /// Given the full large, small mapping, store it in the DB.
    /// Future resolves to true if the mapping was saved, false otherwise
    async fn add(&self, ctx: &CoreContext, entry: SyncedCommitMappingEntry) -> Result<bool, Error>;

    /// Bulk insert a set of large, small mappings
    /// This is meant for blobimport and similar
    async fn add_bulk(
        &self,
        ctx: &CoreContext,
        entries: Vec<SyncedCommitMappingEntry>,
    ) -> Result<u64, Error>;

    /// Find all the mapping entries for a given source commit and target repo
    async fn get(
        &self,
        ctx: &CoreContext,
        source_repo_id: RepositoryId,
        bcs_id: ChangesetId,
        target_repo_id: RepositoryId,
    ) -> Result<
        Vec<(
            ChangesetId,
            Option<CommitSyncConfigVersion>,
            Option<SyncedCommitSourceRepo>,
        )>,
        Error,
    >;

    /// Find all the mapping entries for a given source commit and target repo.
    ///
    /// This method is similar to `get`, but it doesn't query the DB master
    /// and so can return stale data.
    async fn get_maybe_stale(
        &self,
        ctx: &CoreContext,
        source_repo_id: RepositoryId,
        bcs_id: ChangesetId,
        target_repo_id: RepositoryId,
    ) -> Result<Vec<FetchedMappingEntry>, Error>;

    /// Find all the mapping entries given many source commits and a target repo
    async fn get_many(
        &self,
        ctx: &CoreContext,
        source_repo_id: RepositoryId,
        target_repo_id: RepositoryId,
        bcs_ids: &[ChangesetId],
    ) -> Result<HashMap<ChangesetId, Vec<FetchedMappingEntry>>, Error>;

    /// Inserts equivalent working copy of a large bcs id. It's similar to mapping entry,
    /// however there are a few differences:
    /// 1) For (large repo, small repo) pair, many large commits can map to the same small commit
    /// 2) Small commit can be null
    ///
    /// If there's a mapping between small and large commits, then equivalent working copy is
    /// the same as the same as the mapping.
    async fn insert_equivalent_working_copy(
        &self,
        ctx: &CoreContext,
        entry: EquivalentWorkingCopyEntry,
    ) -> Result<bool, Error>;

    /// Same as previous command, but it overwrites existing value.
    /// This is not intended to be used in production, but just as a debug tool
    async fn overwrite_equivalent_working_copy(
        &self,
        ctx: &CoreContext,
        entry: EquivalentWorkingCopyEntry,
    ) -> Result<bool, Error>;

    /// Finds equivalent working copy
    async fn get_equivalent_working_copy(
        &self,
        ctx: &CoreContext,
        source_repo_id: RepositoryId,
        source_bcs_id: ChangesetId,
        target_repo_id: RepositoryId,
    ) -> Result<Option<WorkingCopyEquivalence>, Error>;

    /// Insert version for large repo commit without mapping to any small repo
    /// commits
    async fn insert_large_repo_commit_version(
        &self,
        ctx: &CoreContext,
        large_repo_id: RepositoryId,
        large_repo_cs_id: ChangesetId,
        version: &CommitSyncConfigVersion,
    ) -> Result<bool, Error>;

    /// Same as previous command, but it overwrites existing value.
    /// This is not intended to be used in production, but just as a debug tool
    /// commits
    async fn overwrite_large_repo_commit_version(
        &self,
        ctx: &CoreContext,
        large_repo_id: RepositoryId,
        large_repo_cs_id: ChangesetId,
        version: &CommitSyncConfigVersion,
    ) -> Result<bool, Error>;

    /// Get version for large repo commit
    async fn get_large_repo_commit_version(
        &self,
        ctx: &CoreContext,
        large_repo_id: RepositoryId,
        large_repo_cs_id: ChangesetId,
    ) -> Result<Option<CommitSyncConfigVersion>, Error>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchedMappingEntry {
    pub target_bcs_id: ChangesetId,
    pub maybe_version_name: Option<CommitSyncConfigVersion>,
    pub maybe_source_repo: Option<SyncedCommitSourceRepo>,
}