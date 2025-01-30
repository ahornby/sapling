/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::fs::read_to_string;
use std::path::Path;

use tokio::process::Command;

pub fn is_fbsource_checkout(mount_point: &Path) -> bool {
    let project_id_path = mount_point.join(".projectid");
    let project_id = read_to_string(project_id_path).ok();
    match project_id {
        Some(project_id) => project_id.trim() == "fbsource",
        None => false,
    }
}

pub async fn get_current_commit_id() -> anyhow::Result<String> {
    let output = Command::new("sl")
        .env("HGPLAIN", "1")
        .args(["whereami", "--traceback"])
        .output()
        .await?;
    Ok(String::from_utf8(output.stdout)?)
}

pub async fn get_commit_timestamp(commit_id: &str) -> anyhow::Result<u64> {
    let output = Command::new("sl")
        .env("HGPLAIN", "1")
        .args(["log", "--traceback", "-T", "{date}", "-r", commit_id])
        .output()
        .await?;
    let date = String::from_utf8(output.stdout)?;
    let date = date.parse::<f64>().map_err(anyhow::Error::msg)?;
    // NOTE: Sapling returns fractional seconds, but we only want whole seconds.
    //       Truncate to the nearest second.
    Ok(date as u64)
}

pub async fn is_commit_in_repo(commit_id: &str) -> anyhow::Result<bool> {
    let output = Command::new("sl")
        .env("HGPLAIN", "1")
        .args([
            "log",
            "--traceback",
            "-r",
            commit_id,
            // Disable fbsource <-> www sync during lookup
            "--config",
            "megarepo.transparent-lookup=",
        ])
        .output()
        .await?;
    Ok(output.status.success())
}

#[cfg(test)]
mod tests {
    use crate::sapling::*;
    use crate::utils::get_mount_point;

    const FBSOURCE_COMMIT_ID: &str = "5496dd87e5fe7430a1a399530cc339a479097524";
    const WWW_COMMIT_ID: &str = "1061662d6db2072dd30308d1626a45ac11db3467";

    #[fbinit::test]
    pub async fn test_current_commit_id() -> anyhow::Result<()> {
        let commit_id = get_current_commit_id().await?;
        assert!(!commit_id.is_empty());
        Ok(())
    }

    #[fbinit::test]
    pub async fn test_is_commit_in_repo() -> anyhow::Result<()> {
        let mount_point = get_mount_point(&None)?;
        let commit_id = get_current_commit_id().await?;
        assert!(is_commit_in_repo(&commit_id).await?);
        assert_eq!(
            is_commit_in_repo(FBSOURCE_COMMIT_ID).await?,
            is_fbsource_checkout(&mount_point)
        );
        assert_eq!(
            is_commit_in_repo(WWW_COMMIT_ID).await?,
            !is_fbsource_checkout(&mount_point)
        );

        Ok(())
    }

    #[fbinit::test]
    pub async fn test_is_fbsource_checkout() -> anyhow::Result<()> {
        let mount_point = get_mount_point(&None)?;
        assert!(is_fbsource_checkout(&mount_point));
        Ok(())
    }

    #[fbinit::test]
    pub async fn test_get_commit_timestamp() -> anyhow::Result<()> {
        // sl log of commit in fbsource:
        //   changeset:   5496dd87e5fe7430a1a399530cc339a479097524  D68746950
        //   user:        John Elliott <jdelliot@fb.com>
        //   date:        Tue, 28 Jan 2025 10:35:17 -0800
        //   summary:     [meerkat] Improve saved state support
        // timestamp should be 1738089317.028800, but we truncate to the nearest second
        let timestamp = get_commit_timestamp(FBSOURCE_COMMIT_ID).await?;
        assert_eq!(timestamp, 1738089317);
        Ok(())
    }
}
