/*
 * Copyright (c) Facebook, Inc. and its affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use blobstore::impl_loadable_storable;

use crate::thrift::{Tree as ThriftTree, TreeHandle as ThriftTreeHandle};
use crate::{Tree, TreeHandle};

impl_loadable_storable! {
    handle_type => TreeHandle,
    handle_thrift_type => ThriftTreeHandle,
    value_type => Tree,
    value_thrift_type => ThriftTree,
}
