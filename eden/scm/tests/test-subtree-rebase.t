  $ setconfig diff.git=True
  $ setconfig subtree.copy-reuse-tree=False
  $ enable rebase

test rebase subtree copy commit and keep the subtree copy metadata
  $ newclientrepo
  $ drawdag <<'EOS'
  > B C  # B/foo/x = 1a\n2\n3\n
  > |/   # C/foo/x = 1\n2\n3a\n
  > A    # A/foo/x = 1\n2\n3\n
  > EOS
  $ hg go -q $C
  $ hg subtree copy -r $A --from-path foo --to-path foo2 -m "subtree copy foo to foo2"
  copying foo to foo2
  $ hg rebase -r . -d $B
  rebasing 0e2c86cd5c8f "subtree copy foo to foo2"
tofix: rebase should keep the subtree copy metadata
  $ hg dbsh -c 'print(repo["."].extra())'
  {'branch': 'default', 'rebase_source': '0e2c86cd5c8fccc8a8922025e4aa211765b3a770'}
