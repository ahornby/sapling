/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use anyhow::Result;
use async_trait::async_trait;
use context::CoreContext;
use mononoke_types::RepositoryId;
use sql_construct::SqlConstruct;
use sql_construct::SqlConstructFromMetadataDatabaseConfig;
use sql_ext::mononoke_queries;
use sql_ext::SqlConnections;

use crate::PushRedirectionConfig;
use crate::PushRedirectionConfigEntry;
use crate::RowId;

mononoke_queries! {
    read TestGet(id: RowId) -> (
        RowId,
        RepositoryId,
        bool,
        bool,
    ) {
        "SELECT id,
            repo_id,
            draft_push,
            public_push
         FROM pushredirect
         WHERE id = {id}"
    }

    read Get(repo_id: RepositoryId) -> (
        RowId,
        RepositoryId,
        bool,
        bool,
    ) {
        "SELECT id,
            repo_id,
            draft_push,
            public_push
         FROM pushredirect
         WHERE repo_id = {repo_id}"
    }

    write Set(repo_id: RepositoryId, draft_push: bool, public_push: bool) {
        none,
        mysql("INSERT INTO pushredirect (repo_id, draft_push, public_push) VALUES ({repo_id}, {draft_push}, {public_push}) ON DUPLICATE KEY UPDATE draft_push = {draft_push}, public_push = {public_push}")
        sqlite("REPLACE INTO pushredirect (repo_id, draft_push, public_push) VALUES ({repo_id}, {draft_push}, {public_push})")
    }
}

fn row_to_entry(row: (RowId, RepositoryId, bool, bool)) -> PushRedirectionConfigEntry {
    let (id, repo_id, draft_push, public_push) = row;
    PushRedirectionConfigEntry {
        id,
        repo_id,
        draft_push,
        public_push,
    }
}

pub struct SqlPushRedirectionConfig {
    connections: SqlConnections,
    repo_id: RepositoryId,
}

#[derive(Clone)]
pub struct SqlPushRedirectionConfigBuilder {
    connections: SqlConnections,
}

impl SqlConstruct for SqlPushRedirectionConfigBuilder {
    const LABEL: &'static str = "pushredirect";

    const CREATION_QUERY: &'static str = include_str!("../schemas/sqlite-pushredirect.sql");

    fn from_sql_connections(connections: SqlConnections) -> Self {
        Self { connections }
    }
}

impl SqlPushRedirectionConfigBuilder {
    pub fn build(self, repo_id: RepositoryId) -> SqlPushRedirectionConfig {
        let SqlPushRedirectionConfigBuilder { connections } = self;

        SqlPushRedirectionConfig {
            connections,
            repo_id,
        }
    }
}

impl SqlConstructFromMetadataDatabaseConfig for SqlPushRedirectionConfigBuilder {}

#[async_trait]
impl PushRedirectionConfig for SqlPushRedirectionConfig {
    async fn set(&self, _ctx: &CoreContext, draft_push: bool, public_push: bool) -> Result<()> {
        Set::query(
            &self.connections.write_connection,
            &self.repo_id,
            &draft_push,
            &public_push,
        )
        .await?;
        Ok(())
    }

    async fn get(&self, _ctx: &CoreContext) -> Result<Option<PushRedirectionConfigEntry>> {
        let rows = Get::query(&self.connections.read_connection, &self.repo_id).await?;
        Ok(rows.into_iter().next().map(row_to_entry))
    }
}

#[cfg(test)]
mod test {
    use fbinit::FacebookInit;

    use super::*;

    #[fbinit::test]
    async fn test_set(fb: FacebookInit) -> Result<()> {
        let ctx = CoreContext::test_mock(fb);
        let builder = SqlPushRedirectionConfigBuilder::with_sqlite_in_memory()?;
        let push = builder.clone().build(RepositoryId::new(1));

        // insert one
        push.set(&ctx, true, false).await?;
        let entry = push.get(&ctx).await?;
        assert!(entry.is_some());
        let entry = entry.unwrap();
        assert!(entry.draft_push);
        assert!(!entry.public_push);

        let push = builder.build(RepositoryId::new(2));

        // insert another
        push.set(&ctx, false, true).await?;
        let entry = push.get(&ctx).await?;
        assert!(entry.is_some());
        let entry = entry.unwrap();
        assert!(!entry.draft_push);
        assert!(entry.public_push);

        // update it
        push.set(&ctx, true, true).await?;
        let entry = push.get(&ctx).await?;
        assert!(entry.is_some());
        let entry = entry.unwrap();
        assert!(entry.draft_push);
        assert!(entry.public_push);

        Ok(())
    }

    #[fbinit::test]
    async fn test_get(fb: FacebookInit) -> Result<()> {
        let ctx = CoreContext::test_mock(fb);
        let builder = SqlPushRedirectionConfigBuilder::with_sqlite_in_memory()?;
        let push = builder.build(RepositoryId::new(3));

        let entry = push.get(&ctx).await?;
        assert!(entry.is_none());

        push.set(&ctx, true, true).await?;
        let entry = push.get(&ctx).await?;
        assert!(entry.is_some());
        let entry = entry.unwrap();
        assert!(entry.draft_push);
        assert!(entry.public_push);

        Ok(())
    }
}
