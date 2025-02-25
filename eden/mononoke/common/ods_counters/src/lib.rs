/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

//! This library is used to query ODS counters
//! It should not be used for counters that are available locally
//! Those should be queried from the local host via fb303
use async_trait::async_trait;
use tokio::time::Duration;

#[cfg(fbcode_build)]
mod facebook;
#[cfg(not(fbcode_build))]
mod oss;

#[async_trait]
pub trait CounterManager {
    async fn add_counter(&mut self, entity: String, key: String);

    async fn run_periodic_fetch(&mut self, interval_duration: Duration);

    async fn get_counter_value(&self, entity: &str, key: &str) -> Option<f64>;
}
