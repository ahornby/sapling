#!/usr/bin/env python3
#
# Copyright (c) 2016, Facebook, Inc.
# All rights reserved.
#
# This source code is licensed under the BSD-style license found in the
# LICENSE file in the root directory of this source tree. An additional grant
# of patent rights can be found in the PATENTS file in the same directory.

import os
import stat
from .lib import testcase


@testcase.eden_repo_test
class MaterializedQueryTest:
    '''Check that materialization is represented correctly.'''

    def populate_repo(self):
        self.repo.write_file('hello', 'hola\n')
        self.repo.write_file('adir/file', 'foo!\n')
        self.repo.write_file('bdir/test.sh', '#!/bin/bash\necho test\n',
                             mode=0o755)
        self.repo.write_file('bdir/noexec.sh', '#!/bin/bash\necho test\n')
        self.repo.symlink('slink', 'hello')
        self.repo.commit('Initial commit.')

    def setUp(self):
        super().setUp()
        self.client = self.get_thrift_client()
        self.client.open()

    def tearDown(self):
        self.client.close()
        super().tearDown()

    def test_noEntries(self):
        items = self.client.getMaterializedEntries(self.mount)
        self.assertEqual({}, items.fileInfo)

        pos = self.client.getCurrentJournalPosition(self.mount)
        self.assertEqual(1, pos.sequenceNumber)
        self.assertNotEqual(0, pos.mountGeneration)

    def test_addFile(self):
        pos = self.client.getCurrentJournalPosition(self.mount)
        self.assertEqual(1, pos.sequenceNumber)

        name = os.path.join(self.mount, 'overlaid')
        with open(name, 'w+') as f:
            pos = self.client.getCurrentJournalPosition(self.mount)
            self.assertEqual(2, pos.sequenceNumber,
                             msg='creating a file bumps the journal')

            f.write('NAME!\n')

        pos = self.client.getCurrentJournalPosition(self.mount)
        self.assertEqual(3, pos.sequenceNumber, msg='writing bumps the journal')

        info = self.client.getMaterializedEntries(self.mount)
        self.assertEqual(pos, info.currentPosition,
                         msg='consistent with getCurrentJournalPosition')

        items = info.fileInfo
        self.assertEqual(2, len(items))

        self.assertTrue(stat.S_ISDIR(items[''].mode))

        self.assertTrue(stat.S_ISREG(items['overlaid'].mode))
        self.assertEqual(6, items['overlaid'].size)
        self.assertNotEqual(0, items['overlaid'].mtime.seconds)

        name = os.path.join(self.mount, 'adir', 'file')
        with open(name, 'a') as f:
            pos = self.client.getCurrentJournalPosition(self.mount)
            self.assertEqual(3, pos.sequenceNumber,
                             msg='journal still in same place for append')
            f.write('more stuff on the end\n')

        pos = self.client.getCurrentJournalPosition(self.mount)
        self.assertEqual(4, pos.sequenceNumber,
                         msg='appending bumps the journal')

        info = self.client.getMaterializedEntries(self.mount)
        self.assertEqual(pos, info.currentPosition,
                         msg='consistent with getCurrentJournalPosition')
        items = info.fileInfo
        self.assertEqual(4, len(items))

        self.assertTrue(stat.S_ISDIR(items[''].mode))

        self.assertTrue(stat.S_ISREG(items['overlaid'].mode))
        self.assertEqual(6, items['overlaid'].size)
        self.assertNotEqual(0, items['overlaid'].mtime.seconds)
