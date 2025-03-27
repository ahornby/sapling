/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum FetchCause {
    // Unknown orginination from EdenFS
    EdenUnknown,
    // The fetch originated from a Eden Thrift prefetch endpoint
    EdenPrefetch,
    // The fetch originated from a Eden Thrift endpoint
    EdenThrift,
    // The fetch originated from FUSE/NFS/PrjFS
    EdenFs,
    // The fetch originated from a mixed EdenFS causes
    EdenMixed,
    // The fetch originated from a Sapling prefetch
    SaplingPrefetch,
    // Unknown orginination from Sapling
    SaplingUnknown,
    // Unknown originiation, usually from Sapling (the default)
    Unspecified,
}

impl FetchCause {
    pub fn to_str(&self) -> &str {
        match self {
            FetchCause::EdenUnknown => "edenfs-unknown",
            FetchCause::EdenPrefetch => "edenfs-prefetch",
            FetchCause::EdenThrift => "edenfs-thrift",
            FetchCause::EdenFs => "edenfs-fs",
            FetchCause::EdenMixed => "edenfs-mixed",
            FetchCause::SaplingPrefetch => "sl-prefetch",
            FetchCause::SaplingUnknown => "sl-unknown",
            FetchCause::Unspecified => "unspecified",
        }
    }
}

impl std::str::FromStr for FetchCause {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "edenfs-unknown" => Ok(FetchCause::EdenUnknown),
            "edenfs-prefetch" => Ok(FetchCause::EdenPrefetch),
            "edenfs-thrift" => Ok(FetchCause::EdenThrift),
            "edenfs-fs" => Ok(FetchCause::EdenFs),
            "edenfs-mixed" => Ok(FetchCause::EdenMixed),
            "sl-prefetch" => Ok(FetchCause::SaplingPrefetch),
            "sl-unknown" => Ok(FetchCause::SaplingUnknown),
            "unspecified" => Ok(FetchCause::Unspecified),
            _ => Err(anyhow::anyhow!("Invalid FetchCause string")),
        }
    }
}
