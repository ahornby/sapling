/*
 * Copyright (c) Facebook, Inc. and its affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

#include "eden/fs/inodes/Overlay.h"
#include "eden/fs/inodes/overlay/FsOverlay.h"

#include <folly/Exception.h>
#include <folly/Expected.h>
#include <folly/FileUtil.h>
#include <folly/Range.h>
#include <folly/Subprocess.h>
#include <folly/experimental/TestUtil.h>
#include <folly/logging/test/TestLogHandler.h>
#include <folly/test/TestUtils.h>
#include <gtest/gtest.h>
#include <algorithm>
#include <iomanip>
#include <sstream>
#include "eden/fs/utils/PathFuncs.h"

#include "eden/fs/inodes/EdenMount.h"
#include "eden/fs/inodes/FileInode.h"
#include "eden/fs/inodes/OverlayFile.h"
#include "eden/fs/inodes/TreeInode.h"
#include "eden/fs/service/PrettyPrinters.h"
#include "eden/fs/testharness/FakeBackingStore.h"
#include "eden/fs/testharness/FakeTreeBuilder.h"
#include "eden/fs/testharness/TempFile.h"
#include "eden/fs/testharness/TestChecks.h"
#include "eden/fs/testharness/TestMount.h"
#include "eden/fs/testharness/TestUtil.h"

using namespace folly::string_piece_literals;
using folly::Subprocess;

namespace facebook {
namespace eden {

namespace {
std::string debugDumpOverlayInodes(Overlay&, InodeNumber rootInode);
} // namespace

TEST(OverlayGoldMasterTest, can_load_overlay_v2) {
  // eden/test-data/overlay-v2.tgz contains a saved copy of an overlay
  // directory generated by edenfs.  Unpack it into a temporary directory,
  // then try loading it.
  //
  // This test helps ensure that new edenfs versions can still successfully load
  // this overlay format even if we change how the overlay is saved in the
  // future.
  auto tmpdir = makeTempDir("eden_test");
  Subprocess tarProcess({"/usr/bin/tar",
                         "-xzf",
                         "eden/test-data/overlay-v2.tgz",
                         "-C",
                         tmpdir.path().string()});
  tarProcess.waitChecked();

  auto overlay =
      Overlay::create(realpath(tmpdir.path().string()) + "overlay-v2"_pc);
  overlay->initialize().get();

  Hash hash1{folly::ByteRange{"abcdabcdabcdabcdabcd"_sp}};
  Hash hash2{folly::ByteRange{"01234012340123401234"_sp}};
  Hash hash3{folly::ByteRange{"e0e0e0e0e0e0e0e0e0e0"_sp}};
  Hash hash4{folly::ByteRange{"44444444444444444444"_sp}};

  auto rootTree = overlay->loadOverlayDir(kRootNodeId);
  auto file = overlay->openFile(2_ino, FsOverlay::kHeaderIdentifierFile);
  auto subdir = overlay->loadOverlayDir(3_ino);
  auto emptyDir = overlay->loadOverlayDir(4_ino);
  auto hello = overlay->openFile(5_ino, FsOverlay::kHeaderIdentifierFile);

  ASSERT_TRUE(rootTree);
  EXPECT_EQ(2, rootTree->size());
  const auto& fileEntry = rootTree->at("file"_pc);
  EXPECT_EQ(2_ino, fileEntry.getInodeNumber());
  EXPECT_EQ(hash1, fileEntry.getHash());
  EXPECT_EQ(S_IFREG | 0644, fileEntry.getInitialMode());
  const auto& subdirEntry = rootTree->at("subdir"_pc);
  EXPECT_EQ(3_ino, subdirEntry.getInodeNumber());
  EXPECT_EQ(hash2, subdirEntry.getHash());
  EXPECT_EQ(S_IFDIR | 0755, subdirEntry.getInitialMode());

  EXPECT_TRUE(file.lseek(FsOverlay::kHeaderLength, SEEK_SET).hasValue());
  auto result = file.readFile();
  EXPECT_FALSE(result.hasError());
  EXPECT_EQ("contents", result.value());

  ASSERT_TRUE(subdir);
  EXPECT_EQ(2, subdir->size());
  const auto& emptyEntry = subdir->at("empty"_pc);
  EXPECT_EQ(4_ino, emptyEntry.getInodeNumber());
  EXPECT_EQ(hash3, emptyEntry.getHash());
  EXPECT_EQ(S_IFDIR | 0755, emptyEntry.getInitialMode());
  const auto& helloEntry = subdir->at("hello"_pc);
  EXPECT_EQ(5_ino, helloEntry.getInodeNumber());
  EXPECT_EQ(hash4, helloEntry.getHash());
  EXPECT_EQ(S_IFREG | 0644, helloEntry.getInitialMode());

  ASSERT_TRUE(emptyDir);
  EXPECT_EQ(0, emptyDir->size());

  EXPECT_TRUE(hello.lseek(FsOverlay::kHeaderLength, SEEK_SET).hasValue());
  result = file.readFile();
  EXPECT_FALSE(result.hasError());
  EXPECT_EQ("", result.value());
}

class OverlayTest : public ::testing::Test {
 protected:
  void SetUp() override {
    // Set up a directory structure that we will use for most
    // of the tests below
    FakeTreeBuilder builder;
    builder.setFiles({
        {"dir/a.txt", "This is a.txt.\n"},
    });
    mount_.initialize(builder);
  }

  // Helper method to check if two timestamps are same or not.
  static void expectTimeSpecsEqual(
      const EdenTimestamp& at,
      const EdenTimestamp& bt) {
    auto a = at.toTimespec();
    auto b = bt.toTimespec();
    EXPECT_EQ(a.tv_sec, b.tv_sec);
    EXPECT_EQ(a.tv_nsec, b.tv_nsec);
  }

  static void expectTimeStampsEqual(
      const InodeTimestamps& a,
      const InodeTimestamps& b) {
    expectTimeSpecsEqual(a.atime, b.atime);
    expectTimeSpecsEqual(a.mtime, b.mtime);
    expectTimeSpecsEqual(a.ctime, b.ctime);
  }

  TestMount mount_;
};

TEST_F(OverlayTest, testRemount) {
  mount_.addFile("dir/new.txt", "test\n");
  mount_.remount();
  // Confirm that the tree has been updated correctly.
  auto newInode = mount_.getFileInode("dir/new.txt");
  EXPECT_FILE_INODE(newInode, "test\n", 0644);
}

TEST_F(OverlayTest, testModifyRemount) {
  // inode object has to be destroyed
  // before remount is called to release the reference
  {
    auto inode = mount_.getFileInode("dir/a.txt");
    EXPECT_FILE_INODE(inode, "This is a.txt.\n", 0644);
  }

  // materialize a directory
  mount_.overwriteFile("dir/a.txt", "contents changed\n");
  mount_.remount();

  auto newInode = mount_.getFileInode("dir/a.txt");
  EXPECT_FILE_INODE(newInode, "contents changed\n", 0644);
}

// In memory timestamps should be same before and after a remount.
// (inmemory timestamps should be written to overlay on
// on unmount and should be read back from the overlay on remount)
TEST_F(OverlayTest, testTimeStampsInOverlayOnMountAndUnmount) {
  // Materialize file and directory
  // test timestamp behavior in overlay on remount.
  InodeTimestamps beforeRemountFile;
  InodeTimestamps beforeRemountDir;
  mount_.overwriteFile("dir/a.txt", "contents changed\n");

  {
    // We do not want to keep references to inode in order to remount.
    auto inodeFile = mount_.getFileInode("dir/a.txt");
    EXPECT_FILE_INODE(inodeFile, "contents changed\n", 0644);
    beforeRemountFile = inodeFile->getMetadata().timestamps;
  }

  {
    // Check for materialized files.
    mount_.remount();
    auto inodeRemount = mount_.getFileInode("dir/a.txt");
    auto afterRemount = inodeRemount->getMetadata().timestamps;
    expectTimeStampsEqual(beforeRemountFile, afterRemount);
  }

  {
    auto inodeDir = mount_.getTreeInode("dir");
    beforeRemountDir = inodeDir->getMetadata().timestamps;
  }

  {
    // Check for materialized directory
    mount_.remount();
    auto inodeRemount = mount_.getTreeInode("dir");
    auto afterRemount = inodeRemount->getMetadata().timestamps;
    expectTimeStampsEqual(beforeRemountDir, afterRemount);
  }
}

TEST_F(OverlayTest, roundTripThroughSaveAndLoad) {
  auto hash = Hash{"0123456789012345678901234567890123456789"};

  auto overlay = mount_.getEdenMount()->getOverlay();

  auto ino1 = overlay->allocateInodeNumber();
  auto ino2 = overlay->allocateInodeNumber();
  auto ino3 = overlay->allocateInodeNumber();

  DirContents dir;
  dir.emplace("one"_pc, S_IFREG | 0644, ino2, hash);
  dir.emplace("two"_pc, S_IFDIR | 0755, ino3);

  overlay->saveOverlayDir(ino1, dir);

  auto result = overlay->loadOverlayDir(ino1);
  ASSERT_TRUE(result);
  const auto* newDir = &*result;

  EXPECT_EQ(2, newDir->size());
  const auto& one = newDir->find("one"_pc)->second;
  const auto& two = newDir->find("two"_pc)->second;
  EXPECT_EQ(ino2, one.getInodeNumber());
  EXPECT_FALSE(one.isMaterialized());
  EXPECT_EQ(ino3, two.getInodeNumber());
  EXPECT_TRUE(two.isMaterialized());
}

TEST_F(OverlayTest, getFilePath) {
  InodePath path;

  path = FsOverlay::getFilePath(1_ino);
  EXPECT_EQ("01/1"_relpath, path);
  path = FsOverlay::getFilePath(1234_ino);
  EXPECT_EQ("d2/1234"_relpath, path);

  // It's slightly unfortunate that we use hexadecimal for the subdirectory
  // name and decimal for the final inode path.  That doesn't seem worth fixing
  // for now.
  path = FsOverlay::getFilePath(15_ino);
  EXPECT_EQ("0f/15"_relpath, path);
  path = FsOverlay::getFilePath(16_ino);
  EXPECT_EQ("10/16"_relpath, path);
}

enum class OverlayRestartMode {
  CLEAN,
  UNCLEAN,
};

class RawOverlayTest : public ::testing::TestWithParam<OverlayRestartMode> {
 public:
  RawOverlayTest() : testDir_{makeTempDir("eden_raw_overlay_test_")} {
    loadOverlay();
  }

  void recreate(std::optional<OverlayRestartMode> restartMode = std::nullopt) {
    unloadOverlay(restartMode);
    loadOverlay();
  }

  void unloadOverlay(
      std::optional<OverlayRestartMode> restartMode = std::nullopt) {
    overlay->close();
    overlay = nullptr;
    switch (restartMode.value_or(GetParam())) {
      case OverlayRestartMode::CLEAN:
        break;
      case OverlayRestartMode::UNCLEAN:
        if (unlink((getLocalDir() + "next-inode-number"_pc).c_str())) {
          folly::throwSystemError("removing saved inode numebr");
        }
        break;
    }
  }

  void loadOverlay() {
    overlay = Overlay::create(getLocalDir());
    overlay->initialize().get();
  }

  void corruptOverlayFile(InodeNumber inodeNumber) {
    corruptOverlayFileByTruncating(inodeNumber);
  }

  void corruptOverlayFileByTruncating(InodeNumber inodeNumber) {
    EXPECT_FALSE(overlay) << "Overlay should not be open when corrupting";
    folly::checkUnixError(
        folly::truncateNoInt(getOverlayFilePath(inodeNumber).c_str(), 0));
  }

  void corruptOverlayFileByDeleting(InodeNumber inodeNumber) {
    EXPECT_FALSE(overlay) << "Overlay should not be open when corrupting";
    folly::checkUnixError(unlink(getOverlayFilePath(inodeNumber).c_str()));
  }

  AbsolutePath getOverlayFilePath(InodeNumber inodeNumber) {
    return getLocalDir() +
        RelativePathPiece{FsOverlay::getFilePath(inodeNumber)};
  }

  AbsolutePath getLocalDir() {
    return AbsolutePath{testDir_.path().string()};
  }

  folly::test::TemporaryDirectory testDir_;
  std::shared_ptr<Overlay> overlay;
};

TEST_P(RawOverlayTest, max_inode_number_is_1_if_overlay_is_empty) {
  EXPECT_EQ(kRootNodeId, overlay->getMaxInodeNumber());
  EXPECT_EQ(2_ino, overlay->allocateInodeNumber());

  recreate(OverlayRestartMode::CLEAN);

  EXPECT_EQ(2_ino, overlay->getMaxInodeNumber());
  EXPECT_EQ(3_ino, overlay->allocateInodeNumber());

  recreate(OverlayRestartMode::UNCLEAN);

  EXPECT_EQ(kRootNodeId, overlay->getMaxInodeNumber());
  EXPECT_EQ(2_ino, overlay->allocateInodeNumber());
}

TEST_P(RawOverlayTest, remembers_max_inode_number_of_tree_inodes) {
  auto ino2 = overlay->allocateInodeNumber();
  EXPECT_EQ(2_ino, ino2);

  DirContents dir;
  overlay->saveOverlayDir(ino2, dir);

  recreate();

  EXPECT_EQ(2_ino, overlay->getMaxInodeNumber());
}

TEST_P(RawOverlayTest, remembers_max_inode_number_of_tree_entries) {
  auto ino2 = overlay->allocateInodeNumber();
  EXPECT_EQ(2_ino, ino2);
  auto ino3 = overlay->allocateInodeNumber();
  auto ino4 = overlay->allocateInodeNumber();

  DirContents dir;
  dir.emplace(PathComponentPiece{"f"}, S_IFREG | 0644, ino3);
  dir.emplace(PathComponentPiece{"d"}, S_IFDIR | 0755, ino4);
  overlay->saveOverlayDir(kRootNodeId, dir);

  recreate();

  SCOPED_TRACE("Inodes:\n" + debugDumpOverlayInodes(*overlay, kRootNodeId));
  EXPECT_EQ(4_ino, overlay->getMaxInodeNumber());
}

TEST_P(RawOverlayTest, remembers_max_inode_number_of_file) {
  auto ino2 = overlay->allocateInodeNumber();
  EXPECT_EQ(2_ino, ino2);
  auto ino3 = overlay->allocateInodeNumber();

  // When materializing, overlay data is written leaf-to-root.

  // The File is written first.
  overlay->createOverlayFile(ino3, folly::ByteRange{"contents"_sp});

  recreate();

  EXPECT_EQ(3_ino, overlay->getMaxInodeNumber());
}

TEST_P(
    RawOverlayTest,
    inode_number_scan_includes_linked_directory_despite_its_corruption) {
  auto subdirectoryIno = overlay->allocateInodeNumber();
  auto rootIno = kRootNodeId;
  ASSERT_GT(subdirectoryIno, rootIno);

  DirContents root;
  root.emplace("subdirectory"_pc, S_IFDIR | 0755, subdirectoryIno);
  overlay->saveOverlayDir(rootIno, root);

  overlay->saveOverlayDir(subdirectoryIno, DirContents{});

  unloadOverlay();
  corruptOverlayFile(subdirectoryIno);
  loadOverlay();

  EXPECT_EQ(subdirectoryIno, overlay->getMaxInodeNumber());
}

TEST_P(
    RawOverlayTest,
    inode_number_scan_continues_scanning_despite_corrupted_directory) {
  // Check that the next inode number is recomputed correctly even in the
  // presence of corrupted directory data in the overlay.
  //
  // The old scan algorithm we used to used would traverse down the directory
  // tree, so we needed to ensure that it still found orphan parts of the tree.
  // The newer OverlayChecker code uses a completely different algorithm which
  // isn't susceptible to this same problem, but it still seems worth testing
  // this behavior.
  //
  // We test with the following overlay structure:
  //
  //   /                               (rootIno)
  //     corrupted_by_truncation/      (corruptedByTruncationIno)
  //     temp/                         (tempDirIno)
  //       temp/corrupted_by_deletion  (corruptedByDeletionIno)
  //

  struct PathNames {
    PathComponentPiece corruptedByTruncationName;
    PathComponentPiece tempName;
  };

  auto rootIno = kRootNodeId;
  auto corruptedByTruncationIno = InodeNumber{};
  auto tempDirIno = InodeNumber{};
  auto corruptedByDeletionIno = InodeNumber{};

  auto setUpOverlay = [&](const PathNames& pathNames) {
    DirContents root;
    root.emplace(
        pathNames.corruptedByTruncationName,
        S_IFDIR | 0755,
        corruptedByTruncationIno);
    root.emplace(pathNames.tempName, S_IFDIR | 0755, tempDirIno);
    overlay->saveOverlayDir(rootIno, root);

    overlay->saveOverlayDir(corruptedByTruncationIno, DirContents{});

    DirContents tempDir;
    tempDir.emplace(
        "corrupted_by_deletion"_pc, S_IFDIR | 0755, corruptedByDeletionIno);
    overlay->saveOverlayDir(tempDirIno, tempDir);

    overlay->saveOverlayDir(corruptedByDeletionIno, DirContents{});
  };

  const PathNames pathNamesToTest[] = {
      // Test a few different path name variations, to ensure traversal order
      // doesn't matter.
      PathNames{.corruptedByTruncationName = "A_corrupted_by_truncation"_pc,
                .tempName = "B_temp"_pc},
      PathNames{.corruptedByTruncationName = "B_corrupted_by_truncation"_pc,
                .tempName = "A_temp"_pc},
  };

  for (auto pathNames : pathNamesToTest) {
    corruptedByTruncationIno = overlay->allocateInodeNumber();
    tempDirIno = overlay->allocateInodeNumber();
    corruptedByDeletionIno = overlay->allocateInodeNumber();
    auto maxIno = std::max(
        {tempDirIno, corruptedByTruncationIno, corruptedByDeletionIno});
    ASSERT_EQ(corruptedByDeletionIno, maxIno);

    setUpOverlay(pathNames);

    SCOPED_TRACE(
        "Inodes before corruption:\n" +
        debugDumpOverlayInodes(*overlay, rootIno));

    unloadOverlay();
    corruptOverlayFileByTruncating(corruptedByTruncationIno);
    corruptOverlayFileByDeleting(corruptedByDeletionIno);
    loadOverlay();

    EXPECT_EQ(maxIno, overlay->getMaxInodeNumber());
  }
}

TEST_P(RawOverlayTest, inode_numbers_not_reused_after_unclean_shutdown) {
  auto ino2 = overlay->allocateInodeNumber();
  EXPECT_EQ(2_ino, ino2);
  overlay->allocateInodeNumber();
  auto ino4 = overlay->allocateInodeNumber();
  auto ino5 = overlay->allocateInodeNumber();

  // When materializing, overlay data is written leaf-to-root.

  // The File is written first.
  overlay->createOverlayFile(ino5, folly::ByteRange{"contents"_sp});

  // The subdir is written next.
  DirContents subdir;
  subdir.emplace(PathComponentPiece{"f"}, S_IFREG | 0644, ino5);
  overlay->saveOverlayDir(ino4, subdir);

  // Crashed before root was written.

  recreate();

  SCOPED_TRACE(
      "Inodes from subdir:\n" + debugDumpOverlayInodes(*overlay, ino4));
  EXPECT_EQ(5_ino, overlay->getMaxInodeNumber());
}

TEST_P(RawOverlayTest, inode_numbers_after_takeover) {
  auto ino2 = overlay->allocateInodeNumber();
  EXPECT_EQ(2_ino, ino2);
  auto ino3 = overlay->allocateInodeNumber();
  auto ino4 = overlay->allocateInodeNumber();
  auto ino5 = overlay->allocateInodeNumber();

  // Write a subdir.
  DirContents subdir;
  subdir.emplace(PathComponentPiece{"f"}, S_IFREG | 0644, ino5);
  overlay->saveOverlayDir(ino4, subdir);

  // Write the root.
  DirContents dir;
  dir.emplace(PathComponentPiece{"f"}, S_IFREG | 0644, ino3);
  dir.emplace(PathComponentPiece{"d"}, S_IFDIR | 0755, ino4);
  overlay->saveOverlayDir(kRootNodeId, dir);

  recreate();

  // Rewrite the root (say, after a takeover) without the file.

  DirContents newroot;
  newroot.emplace(PathComponentPiece{"d"}, S_IFDIR | 0755, 4_ino);
  overlay->saveOverlayDir(kRootNodeId, newroot);

  recreate(OverlayRestartMode::CLEAN);

  SCOPED_TRACE("Inodes:\n" + debugDumpOverlayInodes(*overlay, kRootNodeId));
  // Ensure an inode in the overlay but not referenced by the previous session
  // counts.
  EXPECT_EQ(5_ino, overlay->getMaxInodeNumber());
}

INSTANTIATE_TEST_CASE_P(
    Clean,
    RawOverlayTest,
    ::testing::Values(OverlayRestartMode::CLEAN));

INSTANTIATE_TEST_CASE_P(
    Unclean,
    RawOverlayTest,
    ::testing::Values(OverlayRestartMode::UNCLEAN));

TEST(OverlayInodePath, defaultInodePathIsEmpty) {
  InodePath path;
  EXPECT_STREQ(path.c_str(), "");
}

class DebugDumpOverlayInodesTest : public ::testing::Test {
 public:
  DebugDumpOverlayInodesTest()
      : testDir_{makeTempDir("eden_DebugDumpOverlayInodesTest")},
        overlay{Overlay::create(AbsolutePathPiece{testDir_.path().string()})} {
    overlay->initialize().get();
  }

  folly::test::TemporaryDirectory testDir_;
  std::shared_ptr<Overlay> overlay;
};

TEST_F(DebugDumpOverlayInodesTest, dump_empty_directory) {
  auto ino = kRootNodeId;
  EXPECT_EQ(1_ino, ino);

  overlay->saveOverlayDir(ino, DirContents{});
  EXPECT_EQ(
      "/\n"
      "  Inode number: 1\n"
      "  Entries (0 total):\n",
      debugDumpOverlayInodes(*overlay, ino));
}

TEST_F(DebugDumpOverlayInodesTest, dump_directory_with_3_regular_files) {
  auto rootIno = kRootNodeId;
  EXPECT_EQ(1_ino, rootIno);
  auto fileAIno = overlay->allocateInodeNumber();
  EXPECT_EQ(2_ino, fileAIno);
  auto fileBIno = overlay->allocateInodeNumber();
  EXPECT_EQ(3_ino, fileBIno);
  auto fileCIno = overlay->allocateInodeNumber();
  EXPECT_EQ(4_ino, fileCIno);

  DirContents root;
  root.emplace("file_a"_pc, S_IFREG | 0644, fileAIno);
  root.emplace("file_b"_pc, S_IFREG | 0644, fileBIno);
  root.emplace("file_c"_pc, S_IFREG | 0644, fileCIno);
  overlay->saveOverlayDir(rootIno, root);

  overlay->createOverlayFile(fileAIno, folly::ByteRange{""_sp});
  overlay->createOverlayFile(fileBIno, folly::ByteRange{""_sp});
  overlay->createOverlayFile(fileCIno, folly::ByteRange{""_sp});

  EXPECT_EQ(
      "/\n"
      "  Inode number: 1\n"
      "  Entries (3 total):\n"
      "            2 f  644 file_a\n"
      "            3 f  644 file_b\n"
      "            4 f  644 file_c\n",
      debugDumpOverlayInodes(*overlay, rootIno));
}

TEST_F(DebugDumpOverlayInodesTest, dump_directory_with_an_empty_subdirectory) {
  auto rootIno = kRootNodeId;
  EXPECT_EQ(1_ino, rootIno);
  auto subdirIno = overlay->allocateInodeNumber();
  EXPECT_EQ(2_ino, subdirIno);

  DirContents root;
  root.emplace("subdir"_pc, S_IFDIR | 0755, subdirIno);
  overlay->saveOverlayDir(rootIno, root);

  overlay->saveOverlayDir(subdirIno, DirContents{});

  EXPECT_EQ(
      "/\n"
      "  Inode number: 1\n"
      "  Entries (1 total):\n"
      "            2 d  755 subdir\n"
      "/subdir\n"
      "  Inode number: 2\n"
      "  Entries (0 total):\n",
      debugDumpOverlayInodes(*overlay, rootIno));
}

TEST_F(DebugDumpOverlayInodesTest, dump_directory_with_unsaved_subdirectory) {
  auto rootIno = kRootNodeId;
  EXPECT_EQ(1_ino, rootIno);
  auto directoryDoesNotExistIno = overlay->allocateInodeNumber();
  EXPECT_EQ(2_ino, directoryDoesNotExistIno);

  DirContents root;
  root.emplace(
      "directory_does_not_exist"_pc, S_IFDIR | 0755, directoryDoesNotExistIno);
  overlay->saveOverlayDir(rootIno, root);

  EXPECT_EQ(
      "/\n"
      "  Inode number: 1\n"
      "  Entries (1 total):\n"
      "            2 d  755 directory_does_not_exist\n"
      "/directory_does_not_exist\n"
      "  Inode number: 2\n",
      debugDumpOverlayInodes(*overlay, rootIno));
}

TEST_F(DebugDumpOverlayInodesTest, dump_directory_with_unsaved_regular_file) {
  auto rootIno = kRootNodeId;
  EXPECT_EQ(1_ino, rootIno);
  auto regularFileDoesNotExistIno = overlay->allocateInodeNumber();
  EXPECT_EQ(2_ino, regularFileDoesNotExistIno);

  DirContents root;
  root.emplace(
      "regular_file_does_not_exist"_pc,
      S_IFREG | 0644,
      regularFileDoesNotExistIno);
  overlay->saveOverlayDir(rootIno, root);

  EXPECT_EQ(
      "/\n"
      "  Inode number: 1\n"
      "  Entries (1 total):\n"
      "            2 f  644 regular_file_does_not_exist\n",
      debugDumpOverlayInodes(*overlay, rootIno));
}

TEST_F(DebugDumpOverlayInodesTest, directories_are_dumped_depth_first) {
  auto rootIno = kRootNodeId;
  EXPECT_EQ(1_ino, rootIno);
  auto subdirAIno = overlay->allocateInodeNumber();
  EXPECT_EQ(2_ino, subdirAIno);
  auto subdirAXIno = overlay->allocateInodeNumber();
  EXPECT_EQ(3_ino, subdirAXIno);
  auto subdirAYIno = overlay->allocateInodeNumber();
  EXPECT_EQ(4_ino, subdirAYIno);
  auto subdirBIno = overlay->allocateInodeNumber();
  EXPECT_EQ(5_ino, subdirBIno);
  auto subdirBXIno = overlay->allocateInodeNumber();
  EXPECT_EQ(6_ino, subdirBXIno);

  DirContents root;
  root.emplace("subdir_a"_pc, S_IFDIR | 0755, subdirAIno);
  root.emplace("subdir_b"_pc, S_IFDIR | 0755, subdirBIno);
  overlay->saveOverlayDir(rootIno, root);

  DirContents subdirA;
  subdirA.emplace("x"_pc, S_IFDIR | 0755, subdirAXIno);
  subdirA.emplace("y"_pc, S_IFDIR | 0755, subdirAYIno);
  overlay->saveOverlayDir(subdirAIno, subdirA);

  DirContents subdirB;
  subdirB.emplace("x"_pc, S_IFDIR | 0755, subdirBXIno);
  overlay->saveOverlayDir(subdirBIno, subdirB);

  overlay->saveOverlayDir(subdirAXIno, DirContents{});
  overlay->saveOverlayDir(subdirAYIno, DirContents{});
  overlay->saveOverlayDir(subdirBXIno, DirContents{});

  EXPECT_EQ(
      "/\n"
      "  Inode number: 1\n"
      "  Entries (2 total):\n"
      "            2 d  755 subdir_a\n"
      "            5 d  755 subdir_b\n"
      "/subdir_a\n"
      "  Inode number: 2\n"
      "  Entries (2 total):\n"
      "            3 d  755 x\n"
      "            4 d  755 y\n"
      "/subdir_a/x\n"
      "  Inode number: 3\n"
      "  Entries (0 total):\n"
      "/subdir_a/y\n"
      "  Inode number: 4\n"
      "  Entries (0 total):\n"
      "/subdir_b\n"
      "  Inode number: 5\n"
      "  Entries (1 total):\n"
      "            6 d  755 x\n"
      "/subdir_b/x\n"
      "  Inode number: 6\n"
      "  Entries (0 total):\n",
      debugDumpOverlayInodes(*overlay, rootIno));
}

namespace {
void debugDumpOverlayInodes(
    Overlay& overlay,
    InodeNumber rootInode,
    AbsolutePathPiece path,
    std::ostringstream& out) {
  out << path << "\n";
  out << "  Inode number: " << rootInode << "\n";

  auto dir = overlay.loadOverlayDir(rootInode);
  if (dir) {
    auto& dirContents = *dir;
    out << "  Entries (" << dirContents.size() << " total):\n";

    auto dtypeToString = [](dtype_t dtype) noexcept->const char* {
      switch (dtype) {
        case dtype_t::Dir:
          return "d";
        case dtype_t::Regular:
          return "f";
        default:
          return "?";
      }
    };

    for (const auto& [entryPath, entry] : dirContents) {
      auto permissions = entry.getInitialMode() & ~S_IFMT;
      out << "  " << std::dec << std::setw(11) << entry.getInodeNumber() << " "
          << dtypeToString(entry.getDtype()) << " " << std::oct << std::setw(4)
          << permissions << " " << entryPath << "\n";
    }
    for (const auto& [entryPath, entry] : dirContents) {
      if (entry.getDtype() == dtype_t::Dir) {
        debugDumpOverlayInodes(
            overlay, entry.getInodeNumber(), path + entryPath, out);
      }
    }
  }
}

std::string debugDumpOverlayInodes(Overlay& overlay, InodeNumber rootInode) {
  std::ostringstream out;
  debugDumpOverlayInodes(overlay, rootInode, AbsolutePathPiece{}, out);
  return out.str();
}

} // namespace

} // namespace eden
} // namespace facebook
