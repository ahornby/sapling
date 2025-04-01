# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License found in the LICENSE file in the root
# directory of this source tree.

  $ . "${TEST_FIXTURES}/library.sh"

  $ cat >> "$ACL_FILE" << ACLS
  > {
  >   "repos": {
  >     "orig": {
  >       "actions": {
  >         "read": ["$CLIENT0_ID_TYPE:$CLIENT0_ID_DATA", "X509_SUBJECT_NAME:CN=localhost,O=Mononoke,C=US,ST=CA", "X509_SUBJECT_NAME:CN=client0,O=Mononoke,C=US,ST=CA"],
  >         "write": ["$CLIENT0_ID_TYPE:$CLIENT0_ID_DATA", "X509_SUBJECT_NAME:CN=localhost,O=Mononoke,C=US,ST=CA", "X509_SUBJECT_NAME:CN=client0,O=Mononoke,C=US,ST=CA"],
  >         "bypass_readonly": ["$CLIENT0_ID_TYPE:$CLIENT0_ID_DATA", "X509_SUBJECT_NAME:CN=localhost,O=Mononoke,C=US,ST=CA", "X509_SUBJECT_NAME:CN=client0,O=Mononoke,C=US,ST=CA"]
  >       }
  >     },
  >     "dest": {
  >       "actions": {
  >         "read": ["$CLIENT0_ID_TYPE:$CLIENT0_ID_DATA","SERVICE_IDENTITY:server", "X509_SUBJECT_NAME:CN=localhost,O=Mononoke,C=US,ST=CA", "X509_SUBJECT_NAME:CN=client0,O=Mononoke,C=US,ST=CA"],
  >         "write": ["$CLIENT0_ID_TYPE:$CLIENT0_ID_DATA","SERVICE_IDENTITY:server", "X509_SUBJECT_NAME:CN=localhost,O=Mononoke,C=US,ST=CA", "X509_SUBJECT_NAME:CN=client0,O=Mononoke,C=US,ST=CA"],
  >          "bypass_readonly": ["$CLIENT0_ID_TYPE:$CLIENT0_ID_DATA","SERVICE_IDENTITY:server", "X509_SUBJECT_NAME:CN=localhost,O=Mononoke,C=US,ST=CA", "X509_SUBJECT_NAME:CN=client0,O=Mononoke,C=US,ST=CA"]
  >       }
  >     }
  >   },
  >   "tiers": {
  >     "mirror_commit_upload": {
  >       "actions": {
  >         "mirror_upload": ["$CLIENT0_ID_TYPE:$CLIENT0_ID_DATA","SERVICE_IDENTITY:server", "X509_SUBJECT_NAME:CN=localhost,O=Mononoke,C=US,ST=CA", "X509_SUBJECT_NAME:CN=client0,O=Mononoke,C=US,ST=CA"]
  >       }
  >     }
  >   }
  > }
  > ACLS

  $ REPOID=0 REPONAME=orig ACL_NAME=orig setup_common_config
  $ REPOID=1 REPONAME=dest ACL_NAME=dest setup_common_config

  $ start_and_wait_for_mononoke_server

  $ hg clone -q mono:orig orig
  $ cd orig

Force push of unrelated commit stack containing empty tree
  $ hg update -q null
  $ mkcommit unrelated
  $ hg rm unrelated
  $ hg commit --amend
  $ hg push -r . --to master_bookmark --non-forward-move --create --force -q
  $ hg log -p
  commit:      f8f8a958c69f
  bookmark:    remote/master_bookmark
  hoistedname: master_bookmark
  user:        test
  date:        Thu Jan 01 00:00:00 1970 +0000
  summary:     unrelated
  
  
Sync all bookmarks moves
  $ mononoke_modern_sync "" sync-once orig dest --start-id 0 2>&1 | grep -v "Uploaded"
  [INFO] Running sync-once loop
  [INFO] Connecting to https://localhost:$LOCAL_PORT/edenapi/
  [INFO] Established EdenAPI connection
  [INFO] Initialized channels
  [INFO] mononoke_host="*" dogfooding=false (glob)
  [INFO] Calculating segments for entry 1, from changeset None to changeset ChangesetId(Blake2(87d9f6e52bc2c5b123a938f090abba9b3ab691d53c51ea2496f93ec138740106)), to generation 1
  [INFO] Done calculating segments for entry 1, from changeset None to changeset ChangesetId(Blake2(87d9f6e52bc2c5b123a938f090abba9b3ab691d53c51ea2496f93ec138740106)), to generation 1 in *ms (glob)
  [INFO] Resuming from latest entry checkpoint 0
  [INFO] Skipping 0 batches from entry 1
  [INFO] Starting sync of 1 missing commits, 0 were already synced
  [INFO] Changeset 87d9f6e52bc2c5b123a938f090abba9b3ab691d53c51ea2496f93ec138740106 has no content
  [INFO] Setting checkpoint from entry 1 to 0
  [INFO] Setting bookmark master_bookmark from None to Some(HgChangesetId(HgNodeHash(Sha1(f8f8a958c69f2b383a6901cc91885d6dd3043f2c))))
  [INFO] Moved bookmark with result SetBookmarkResponse { data: Ok(()) }
  [INFO] Marking entry 1 as done
