# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License found in the LICENSE file in the root
# directory of this source tree.

  $ . "${TEST_FIXTURES}/library.sh"

Set up local hgrc and Mononoke config.
  $ setup_common_config
  $ cd $TESTTMP


Setup testing repo for mononoke:
  $ hginit_treemanifest repo
  $ cd repo


  $ drawdag << EOS
  > B
  > |
  > A
  > EOS

  $ hg book -r $A alpha
  $ hg log -r alpha -T'{node}\n'
  426bada5c67598ca65036d57d9e4b64b0c1ce7a0
  $ hg book -r $B beta
  $ hg log -r beta -T'{node}\n'
  112478962961147124edd43549aedd1a335e44bf


import testing repo to mononoke
  $ cd
  $ blobimport repo/.hg repo


Start up SaplingRemoteAPI server.
  $ start_and_wait_for_mononoke_server

Clone repo
  $ hg clone -q mono:repo client
  $ cd client

Check response.
  $ hg debugapi -e bookmarks -i '["alpha", "beta", "unknown"]'
  {"beta": "112478962961147124edd43549aedd1a335e44bf",
   "alpha": "426bada5c67598ca65036d57d9e4b64b0c1ce7a0",
   "unknown": None}

Check response for error propagating endpoint 
  $ hg debugapi -e bookmarks2 -i '["alpha", "beta", "unknown"]' | sed -E 's/"hgid": bin\("([^"]+)"\)/"hgid": "\1"/g; s/None/null/g' | jq 'sort_by(.data.Ok.bookmark)'
  [
    {
      "data": {
        "Ok": {
          "hgid": "426bada5c67598ca65036d57d9e4b64b0c1ce7a0",
          "bookmark": "alpha"
        }
      }
    },
    {
      "data": {
        "Ok": {
          "hgid": "112478962961147124edd43549aedd1a335e44bf",
          "bookmark": "beta"
        }
      }
    },
    {
      "data": {
        "Ok": {
          "hgid": null,
          "bookmark": "unknown"
        }
      }
    }
  ]

Check response for slapigit.
  $ hg --config edenapi.url=https://localhost:$MONONOKE_SOCKET/slapigit/ debugapi -e bookmarks -i '["alpha", "beta", "unknown"]'
  {"beta": "6cfc1c8c1f96fda3264583d15e8ef8b2a3436dca",
   "alpha": "396d8029b77033c770d483ba57559d5161397819",
   "unknown": None}
