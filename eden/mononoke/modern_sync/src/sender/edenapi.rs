/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::collections::HashMap;
use std::collections::HashSet;

use anyhow::Result;
use async_trait::async_trait;
use clientinfo::ClientEntryPoint;
use clientinfo::ClientInfo;
use context::CoreContext;
use edenapi::Client;
use edenapi::HttpClientBuilder;
use edenapi::HttpClientConfig;
use edenapi::SaplingRemoteApi;
use edenapi_types::AnyFileContentId;
use futures::stream;
use futures::StreamExt;
use futures::TryStreamExt;
use mercurial_types::blobs::HgBlobChangeset;
use mercurial_types::HgChangesetId;
use mercurial_types::HgFileNodeId;
use mercurial_types::HgManifestId;
use mononoke_app::args::TLSArgs;
use mononoke_types::FileContents;
use repo_blobstore::RepoBlobstore;
use slog::info;
use slog::Logger;
use url::Url;

mod util;

use crate::sender::Entry;
use crate::sender::ModernSyncSender;

#[allow(dead_code)]
pub struct EdenapiSender {
    client: Client,
    logger: Logger,
    ctx: CoreContext,
    repo_blobstore: RepoBlobstore,
}

impl EdenapiSender {
    pub async fn new(
        url: Url,
        reponame: String,
        logger: Logger,
        tls_args: TLSArgs,
        ctx: CoreContext,
        repo_blobstore: RepoBlobstore,
    ) -> Result<Self> {
        let ci = ClientInfo::new_with_entry_point(ClientEntryPoint::ModernSync)?.to_json()?;
        let http_config = HttpClientConfig {
            cert_path: Some(tls_args.tls_certificate.into()),
            key_path: Some(tls_args.tls_private_key.into()),
            ca_path: Some(tls_args.tls_ca.into()),
            convert_cert: false,

            client_info: Some(ci),
            disable_tls_verification: false,
            max_concurrent_requests: None,
            unix_socket_domains: HashSet::new(),
            unix_socket_path: None,
            verbose: false,
            verbose_stats: false,
        };

        info!(logger, "Connecting to {}", url.to_string());

        let client = HttpClientBuilder::new()
            .repo_name(&reponame)
            .server_url(url)
            .http_config(http_config)
            .build()?;

        let res = client.health().await;
        info!(logger, "Health check outcome: {:?}", res);
        Ok(Self {
            client,
            logger,
            ctx,
            repo_blobstore,
        })
    }
}

#[async_trait]
impl ModernSyncSender for EdenapiSender {
    async fn enqueue_entry(&self, _entry: Entry) -> Result<()> {
        // TODO: implement using mpsc channels
        Ok(())
    }

    async fn upload_contents(&self, contents: Vec<(AnyFileContentId, FileContents)>) -> Result<()> {
        info!(
            &self.logger,
            "Uploading contents: {:?}",
            contents
                .clone()
                .into_iter()
                .map(|(first, _)| first)
                .collect::<Vec<_>>()
        );

        for (id, blob) in contents {
            match blob {
                FileContents::Bytes(bytes) => {
                    info!(&self.logger, "Uploading bytes: {:?}", bytes);
                    let response = self
                        .client
                        .process_files_upload(vec![(id, bytes.into())], None, None)
                        .await?;
                    info!(
                        &self.logger,
                        "Upload response: {:?}",
                        response.entries.try_collect::<Vec<_>>().await?
                    );
                }
                _ => (),
            }
        }

        Ok(())
    }

    async fn upload_trees(&self, trees: Vec<HgManifestId>) -> Result<()> {
        let entries = stream::iter(trees)
            .map(|mf_id| {
                let ctx = self.ctx.clone();
                let repo_blobstore = self.repo_blobstore.clone();
                async move { util::from_tree_to_entry(mf_id, &ctx, &repo_blobstore).await }
            })
            .buffer_unordered(10)
            .try_collect::<Vec<_>>()
            .await?;

        let res = self.client.upload_trees_batch(entries).await?;
        info!(
            &self.logger,
            "Upload tree response: {:?}",
            res.entries.try_collect::<Vec<_>>().await?
        );
        Ok(())
    }

    async fn upload_filenodes(&self, fn_ids: Vec<HgFileNodeId>) -> Result<()> {
        let filenodes = stream::iter(fn_ids)
            .map(|file_id| {
                let ctx = self.ctx.clone();
                let repo_blobstore = self.repo_blobstore.clone();
                async move { util::from_id_to_filenode(file_id, &ctx, &repo_blobstore).await }
            })
            .buffer_unordered(10)
            .try_collect::<Vec<_>>()
            .await?;

        let res = self.client.upload_filenodes_batch(filenodes).await?;
        info!(
            &self.logger,
            "Upload filenodes response: {:?}",
            res.entries.try_collect::<Vec<_>>().await?
        );
        Ok(())
    }

    async fn upload_hg_changeset(&self, hg_css: Vec<HgBlobChangeset>) -> Result<()> {
        let entries = stream::iter(hg_css)
            .map(util::to_upload_hg_changeset)
            .try_collect::<Vec<_>>()
            .await?;

        let res = self.client.upload_changesets(entries, vec![]).await?;
        info!(
            &self.logger,
            "Upload hg changeset response: {:?}",
            res.entries.try_collect::<Vec<_>>().await?
        );
        Ok(())
    }

    async fn set_bookmark(
        &self,
        bookmark: String,
        from: Option<HgChangesetId>,
        to: Option<HgChangesetId>,
    ) -> Result<()> {
        let res = self
            .client
            .set_bookmark(
                bookmark,
                to.map(|cs| cs.into()),
                from.map(|cs| cs.into()),
                HashMap::new(),
            )
            .await?;
        info!(&self.logger, "Move bookmark response {:?}", res);
        Ok(())
    }
}
