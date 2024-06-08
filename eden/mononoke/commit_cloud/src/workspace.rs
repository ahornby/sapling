/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use crate::references::heads::WorkspaceHead;
use crate::references::history::WorkspaceHistory;
use crate::references::local_bookmarks::WorkspaceLocalBookmark;
use crate::references::remote_bookmarks::WorkspaceRemoteBookmark;

#[allow(unused)]
pub(crate) struct WorkspaceContents {
    heads: Vec<WorkspaceHead>,
    local_bookmarks: Vec<WorkspaceLocalBookmark>,
    remote_bookmarks: Vec<WorkspaceRemoteBookmark>,
    history: WorkspaceHistory,
}
