#require git no-windows
#debugruntest-compatible

  $ . $TESTDIR/git.sh
  $ setconfig diff.git=true ui.allowemptycommit=true

Prepare git repo

  $ git init -q -b main git-repo
  $ cd git-repo
  $ echo 'i' > .gitignore 
  $ touch a b c
  $ git add a b c .gitignore
  $ git commit -q -m commit1
  $ for i in a b c; do echo 1 >> $i; done
  $ git commit -q -a -m commit2

Ignore status

  $ touch i

  $ git status --porcelain
  $ git status --porcelain --ignored
  !! i
  $ sl status
  $ sl status --ignore
  I i

Status after changing filesystem (modify, create, remove)

  $ echo 2 > b
  $ echo 2 > d
  $ rm c

  $ git status --porcelain
   M b
   D c
  ?? d

  $ sl status
  ? b
  ? d

Status update via add or remove commands

  $ sl rm c
  warning: failed to inspect working copy parent
  abort: working directory state appears damaged!
  [255]
  $ sl add d
  abort: working directory state appears damaged!
  [255]
  $ sl status
  ? b
  ? d
