# Sapling

Sapling is a fast, easy to use, distributed revision control tool for software
developers.


# Basic install

```
$ make install-oss
$ sl debuginstall # sanity-check setup
$ sl --help       # see help
```


Running without installing:

```
$ make oss        # build for inplace usage
$ ./sl --version  # should show the latest version
```

See <https://sapling-scm.com/> for detailed installation instructions,
platform-specific notes, and Sapling user information.

# Thrift enabled build for use by Mononoke or EdenFS

Mononoke and EdenFS need the thrift enabled sapling CLI built via getdeps. Check github actions to see current OS version the Sapling CLI Getdeps CI runs with.

This build also provides a way to run the sapling .t tests in an open source environment.

make sure system packages are installed
`./build/fbcode_builder/getdeps.py install-system-deps --recursive sapling`

build sapling:
`./build/fbcode_builder/getdeps.py build --allow-system-packages --no-facebook-internal --src-dir=. sapling`

run the tests.  48 jobs was about max concurrency on a 64GB RAM personal linux machine,  CI runs specify less.
`./build/fbcode_builder/getdeps.py --allow-system-packages test --src-dir=. sapling --num-jobs=48`

to iterate on one test run with --retry 0 --filter:
`./build/fbcode_builder/getdeps.py --allow-system-packages test --src-dir=. sapling --num-jobs=48 --retry 0 --filter test-check-execute.t`