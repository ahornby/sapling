/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

#include <folly/Exception.h>
#include <folly/Random.h>
#include <folly/executors/ManualExecutor.h>
#include <folly/portability/GFlags.h>
#include <folly/portability/GMock.h>
#include <folly/portability/GTest.h>
#include <folly/test/TestUtils.h>
#ifdef _WIN32
#include "eden/fs/prjfs/Enumerator.h"
#else
#include "eden/fs/fuse/DirList.h"
#endif // _WIN32
#include "eden/fs/inodes/FileInode.h"
#include "eden/fs/inodes/InodeMap.h"
#include "eden/fs/inodes/TreeInode.h"
#include "eden/fs/model/Tree.h"
#include "eden/fs/model/TreeEntry.h"
#include "eden/fs/store/ObjectFetchContext.h"
#include "eden/fs/testharness/FakeTreeBuilder.h"
#include "eden/fs/testharness/InodeUnloader.h"
#include "eden/fs/testharness/TestChecks.h"
#include "eden/fs/testharness/TestMount.h"
#include "eden/fs/utils/CaseSensitivity.h"
#include "eden/fs/utils/FaultInjector.h"
#include "eden/fs/utils/StatTimes.h"

using namespace facebook::eden;
using namespace std::chrono_literals;

namespace {
using ContainedType = InodeOrTreeOrEntry::ContainedType;
std::string to_string(const ContainedType& ctype) {
  switch (ctype) {
    case ContainedType::Inode:
      return "Inode";
    case ContainedType::DirEntry:
      return "DirEntry";
    case ContainedType::Tree:
      return "Tree";
    case ContainedType::TreeEntry:
      return "TreeEntry";
    default:
      return fmt::format("Unknown<{}>", folly::to_underlying(ctype));
  }
}

enum Flags {
  FLAG_M = 0x01, // Materialized
  FLAG_L = 0x02, // Loaded
};

/**
 * A class that tracks models the expected state of files in the mount, for
 * comparison with the actual mount.
 */
struct TestFileInfo {
  TestFileInfo(
      dtype_t dtype_,
      TreeEntryType treeEntryType_,
      ContainedType ctype_,
      mode_t mode_,
      std::string path_,
      int flags_)
      : dtype(dtype_),
        treeEntryType(treeEntryType_),
        containedType(ctype_),
        mode(mode_),
        path(std::move(path_)),
        flags(flags_),
        contents(dtype_ == dtype_t::Regular ? path.stringPiece() : "") {}

  bool operator==(const TestFileInfo& rhs) const {
    return dtype == rhs.dtype && containedType == rhs.containedType &&
        path == rhs.path && flags == rhs.flags;
  }

  bool isLoaded() const {
    return checkFlag<FLAG_L>();
  }

  bool isMaterialized() const {
    return checkFlag<FLAG_M>();
  }

  bool isRegularFile() const {
    return dtype == dtype_t::Regular;
  }
  bool isDirectory() const {
    return dtype == dtype_t::Dir;
  }
  bool isSymlink() const {
    return dtype == dtype_t::Symlink;
  }

  TreeEntryType getTreeEntryType() const {
    return treeEntryType;
  }

  std::string getLogPath() const {
    return "\"" + pathStr() + "\"";
  }

  std::string pathStr() const {
    return path.stringPiece().str();
  }

  folly::StringPiece getContents() const {
    return contents;
  }

  void setContents(folly::StringPiece contents) {
    this->contents = contents;
  }

  Hash20 getSHA1() const {
    auto content = getContents();
    return Hash20::sha1(folly::ByteRange{content});
  }

  mode_t getMode() const {
    return mode;
  }

  struct timespec getMtime(const struct timespec& lastCheckoutTime) const {
    return mtime.value_or(lastCheckoutTime);
  }

  dtype_t dtype{dtype_t::Unknown};
  TreeEntryType treeEntryType;
  ContainedType containedType;
  mode_t mode;
  RelativePath path;
  folly::Optional<struct timespec> mtime;
  int flags{0};
  std::string contents;

 private:
  template <int flag>
  bool checkFlag() const {
    return 0 != (flags & flag);
  }
};

#ifdef _WIN32
// TODO: figure out how to share this among here, InodeOrTreeOrEntry, and
// FileInode/TreeInode
#define DEFAULT_MODE_DIR (0)
#define DEFAULT_MODE_REG (0)
#define DEFAULT_MODE_EXE (0)
#else
#define DEFAULT_MODE_DIR                                                 \
  (S_IFDIR | S_IRUSR | S_IWUSR | S_IXUSR | S_IRGRP | S_IXGRP | S_IROTH | \
   S_IXOTH)
#define DEFAULT_MODE_REG (S_IFREG | S_IRUSR | S_IWUSR | S_IRGRP | S_IROTH)
#define DEFAULT_MODE_EXE                                                 \
  (S_IFDIR | S_IRUSR | S_IWUSR | S_IXUSR | S_IRGRP | S_IXGRP | S_IROTH | \
   S_IXOTH)
#endif
class TestFileDatabase {
 public:
#define ENTRY(dtype_, etype_, ctype_, path, flags)                 \
  std::make_shared<const TestFileInfo>(                            \
      dtype_t::dtype_,                                             \
      TreeEntryType::etype_,                                       \
      ContainedType::ctype_,                                       \
      (TreeEntryType::etype_ == TreeEntryType::TREE                \
           ? DEFAULT_MODE_DIR                                      \
           : (TreeEntryType::etype_ == TreeEntryType::REGULAR_FILE \
                  ? DEFAULT_MODE_REG                               \
                  : DEFAULT_MODE_EXE)),                            \
      path,                                                        \
      flags)
#define ENTRIES                                                              \
  ENTRY(Dir, TREE, Inode, "", FLAG_M | FLAG_L),                              \
      ENTRY(Regular, REGULAR_FILE, DirEntry, "root_fileA", 0),               \
      ENTRY(Regular, REGULAR_FILE, DirEntry, "root_fileB", 0),               \
      ENTRY(Dir, TREE, Tree, "root_dirA", 0),                                \
      ENTRY(Regular, REGULAR_FILE, TreeEntry, "root_dirA/child1_fileA1", 0), \
      ENTRY(Regular, REGULAR_FILE, TreeEntry, "root_dirA/child1_fileA2", 0), \
      ENTRY(Dir, TREE, Tree, "root_dirB", 0),                                \
      ENTRY(Regular, REGULAR_FILE, TreeEntry, "root_dirB/child1_fileB1", 0), \
      ENTRY(Regular, REGULAR_FILE, TreeEntry, "root_dirB/child1_fileB2", 0), \
      ENTRY(Dir, TREE, Tree, "root_dirB/child1_dirB1", 0),                   \
      ENTRY(                                                                 \
          Regular,                                                           \
          REGULAR_FILE,                                                      \
          TreeEntry,                                                         \
          "root_dirB/child1_dirB1/child2_fileBB1",                           \
          0),                                                                \
      ENTRY(                                                                 \
          Regular,                                                           \
          REGULAR_FILE,                                                      \
          TreeEntry,                                                         \
          "root_dirB/child1_dirB1/child2_fileBB2",                           \
          0),                                                                \
      ENTRY(Dir, TREE, Tree, "root_dirB/child1_dirB2", 0),                   \
      ENTRY(                                                                 \
          Regular,                                                           \
          REGULAR_FILE,                                                      \
          TreeEntry,                                                         \
          "root_dirB/child1_dirB2/child2_fileBB3",                           \
          0),                                                                \
      ENTRY(                                                                 \
          Regular,                                                           \
          REGULAR_FILE,                                                      \
          TreeEntry,                                                         \
          "root_dirB/child1_dirB2/child2_fileBB4",                           \
          0)

  TestFileDatabase() : initialInfos_({ENTRIES}) {
    for (auto& info : initialInfos_) {
      modifiedInfos_[info->path] = std::make_shared<TestFileInfo>(*info);
    }
  }
#undef ENTRIES
#undef ENTRY

  void reset() {
    for (auto& info : initialInfos_) {
      *modifiedInfos_[info->path] = *info;
    }
  }

  void del(RelativePathPiece path) {
    auto& entry = getEntry(path);
    // TODO: support recursive removal of parents?
    XCHECK_NE(entry.dtype, dtype_t::Dir);

    entry.dtype = dtype_t::Unknown;
    entry.flags = 0;

    onDelete(RelativePathPiece(path));
  }

  void setContents(RelativePathPiece path, folly::StringPiece contents) {
    auto& entry = getEntry(path);
    bool contentsChanged = entry.getContents() != contents;
    entry.setContents(contents.str());
    if (contentsChanged) {
      onContentsChanged(path);
    }
  }

  void setFlags(RelativePathPiece path, int flags) {
    auto& entry = getEntry(path);
    // Loaded entries should transition to be an iNode
    bool becameLoaded = (!entry.isLoaded() && (flags & FLAG_L));
    bool becameMaterialized = (!entry.isMaterialized() && (flags & FLAG_M));
    entry.flags |= flags;

    if (becameLoaded) {
      entry.containedType = ContainedType::Inode;
      onLoaded(path);
    }
    if (becameMaterialized) {
      onMaterialized(path);
    }
  }

  void clearFlags(RelativePathPiece path, int flags) {
    auto& entry = getEntry(path);
    bool becameUnLoaded = (entry.isLoaded() && !(flags & FLAG_L));
    bool becameUnMaterialized = (entry.isMaterialized() && !(flags & FLAG_M));
    entry.flags &= ~flags;
    if (becameUnLoaded) {
      entry.containedType = ContainedType::Inode;
      onUnLoaded(path);
    }
    if (becameUnMaterialized) {
      onUnMaterialized(path);
    }
  }

  void setContainedType(RelativePathPiece path, ContainedType containedType) {
    auto& entry = getEntry(path);
    entry.containedType = containedType;
  }

  void build(FakeTreeBuilder& builder) {
    for (const auto& info : initialInfos_) {
      if (info->isRegularFile()) {
        auto path = info->pathStr();
        builder.setFile(path, path);
      }
    }
  }

  size_t size() const {
    return initialInfos_.size();
  }
  const TestFileInfo& getOriginalInfo(size_t i) {
    return *initialInfos_[i].get();
  }

  std::vector<std::shared_ptr<const TestFileInfo>> getOriginalItems() {
    std::vector<std::shared_ptr<const TestFileInfo>> ret{initialInfos_};
    return ret;
  }
  std::vector<std::shared_ptr<TestFileInfo>> getModifiedItems() {
    std::vector<std::shared_ptr<TestFileInfo>> ret;
    ret.reserve(modifiedInfos_.size());
    for (auto& pathInfo : modifiedInfos_) {
      ret.push_back(pathInfo.second);
    }
    return ret;
  }

  bool isModified(const TestFileInfo& lhs) {
    for (auto& rhs : initialInfos_) {
      if (lhs.path == rhs->path) {
        return lhs == *rhs;
      }
    }
    throw std::out_of_range("No path match for lhs");
  }

  std::vector<TestFileInfo*> getChildren(RelativePathPiece path) {
    std::vector<TestFileInfo*> kids;
    for (auto& info : initialInfos_) {
      if (info->path.stringPiece().size() && info->path.dirname() == path) {
        kids.emplace_back(&getEntry(info->path));
      }
    }
    return kids;
  }

 private:
  TestFileInfo& getEntry(RelativePathPiece path) {
    auto& info = modifiedInfos_[path];
    XCHECK_NE(info, nullptr);
    return *info.get();
  }

  void onContentsChanged(RelativePathPiece path) {
    // Load & Materialize ourselves
    setFlags(path, FLAG_L | FLAG_M);
  }

  void onDelete(RelativePathPiece path) {
    XCHECK_NE(path.stringPiece().size(), 0U);
    // Unlinking a file causes the parents to be
    // loaded/materialized
    setFlags(path.dirname(), FLAG_M | FLAG_L);
  }

  void onMaterialized(RelativePathPiece path) {
    // Materializing a child also materializes the parent
    setFlags(path.dirname(), FLAG_M);
  }

  void onLoaded(RelativePathPiece path) {
    // Loading an inode means that this node is converting to an Inode
    setContainedType(path, ContainedType::Inode);
    // Loading a child also loads the parent
    setFlags(path.dirname(), FLAG_L);
    // Children of loaded dirs Change from Tree/TreeEntry to Tree/DirEntry if
    // they aren't already loaded
    for (auto& kidInfo : getChildren(path)) {
      if (!kidInfo->isDirectory() && !kidInfo->isLoaded()) {
        setContainedType(kidInfo->path, ContainedType::DirEntry);
      }
    }
  }

  void onUnLoaded(RelativePathPiece /*path*/) {
    // TODO: right now we only ever unMaterialize the entire tree
    assert(false);
  }

  void onUnMaterialized(RelativePathPiece /*path*/) {
    // TODO: right now we only ever unMaterialize the entire tree
    assert(false);
  }

  std::vector<std::shared_ptr<const TestFileInfo>> initialInfos_;
  std::map<RelativePathPiece, std::shared_ptr<TestFileInfo>> modifiedInfos_;
};

FakeTreeBuilder MakeTestTreeBuilder(TestFileDatabase& files) {
  FakeTreeBuilder builder;
  files.build(builder);
  return builder;
}

enum VERIFY_FLAGS {
  VERIFY_SHA1 = 0x0001,
  VERIFY_BLOB_METADATA = 0x0002,
  VERIFY_STAT = 0x0004,

  VERIFY_WITH_MODIFICATIONS = 0x0008,

  VERIFY_DEFAULT = VERIFY_SHA1 | VERIFY_STAT | VERIFY_BLOB_METADATA |
      VERIFY_WITH_MODIFICATIONS,
  VERIFY_INITIAL = VERIFY_DEFAULT & ~VERIFY_WITH_MODIFICATIONS,
};

void verifyTreeState(
    const char* filename,
    int line,
    TestMount& mount,
    TestFileDatabase& files,
    int verify_flags = VERIFY_DEFAULT) {
  (void)filename;
  (void)line;

  std::vector<const TestFileInfo*> infos;
  if (0 == (verify_flags & VERIFY_WITH_MODIFICATIONS)) {
    for (auto info : files.getOriginalItems()) {
      infos.push_back(info.get());
    }
  } else {
    for (auto info : files.getModifiedItems()) {
      infos.push_back(info.get());
    }
  }

  for (auto expected_ : infos) {
    auto& expected = *expected_;
    const char* type = files.isModified(expected) ? "MOD" : "ORIG";

    std::string dbgMsg = std::string(" for file at \"") +
        expected.path.stringPiece().str() + "\" with " + type +
        " record and flags (";
    {
      std::string flags;
      if (expected.flags & FLAG_L) {
        flags += "loaded";
      }
      if (expected.flags & FLAG_M) {
        if (flags.size()) {
          flags += ' ';
        }
        flags += "materialized";
      }
      dbgMsg += flags + ")";
    }

    // TODO: the code below is equivalent to EXPECT_INODE_OR(), perhaps it
    // should be broken out so test failures appear within the line#/function
    // they are occuring in?
    auto inodeOrTry = mount.getEdenMount()
                          ->getInodeOrTreeOrEntry(
                              RelativePathPiece{expected.path},
                              ObjectFetchContext::getNullContext())
                          .getTry();
    if (inodeOrTry.hasValue()) {
      auto inodeOr = inodeOrTry.value();
      EXPECT_EQ(inodeOr.getDtype(), expected.dtype) << dbgMsg;
      bool isLoaded = false;
      bool isMaterialized = false;
      if (inodeOr.testGetContainedType() == ContainedType::Inode) {
        auto inode = inodeOr.asInodePtr();
        EXPECT_TRUE(!!inode);
        isLoaded = true;

        if (auto fileInode = inode.asFilePtrOrNull()) {
          // There is no exposed method to get the martialized status of a
          // FileInode. But we can get the parent inode, and ask about the
          // materialization status of it's children. There's ALWAYS a parent
          // for a file, because the root is a directory.
          auto renameLock = mount.getEdenMount()->acquireRenameLock();
          auto location = fileInode->getLocationInfo(renameLock);
          auto parent = fileInode->getParent(renameLock);
          auto contents = parent->getContents().rlock();
          auto entry = contents->entries.find(location.name);
          ASSERT_TRUE(entry != contents->entries.end());
          isMaterialized = entry->second.isMaterialized();
        } else if (auto treeInode = inode.asTreePtrOrNull()) {
          const auto& contents = treeInode->getContents().rlock();
          isMaterialized = contents->isMaterialized();
        } else {
          EXPECT_TRUE(false)
              << dbgMsg << " is neither a FileInode or TreeInode";
        }
      } else {
        // No inode, so it must not be loaded or materialized
        isLoaded = false;
        isMaterialized = false;
      }
      EXPECT_EQ(isLoaded, expected.isLoaded()) << dbgMsg;
      EXPECT_EQ(isMaterialized, expected.isMaterialized()) << dbgMsg;

      EXPECT_EQ(
          to_string(inodeOr.testGetContainedType()),
          to_string(expected.containedType))
          << dbgMsg;
      // SHA1s are only computed for files
      if ((verify_flags & VERIFY_SHA1) &&
          inodeOr.getDtype() == dtype_t::Regular) {
        auto sha1 = inodeOr
                        .getSHA1(
                            expected.path,
                            mount.getEdenMount()->getObjectStore(),
                            ObjectFetchContext::getNullContext())
                        .get();
        EXPECT_EQ(sha1, expected.getSHA1()) << dbgMsg << " expected.contents=\""
                                            << expected.getContents() << "\"";
      }

      if ((verify_flags & VERIFY_BLOB_METADATA) &&
          inodeOr.getDtype() == dtype_t::Regular) {
        auto metadata = inodeOr
                            .getEntryAttributes(
                                expected.path,
                                mount.getEdenMount()->getObjectStore(),
                                ObjectFetchContext::getNullContext())
                            .get();
        EXPECT_EQ(metadata.sha1.value(), expected.getSHA1()) << dbgMsg;
        EXPECT_EQ(metadata.size.value(), expected.getContents().size())
            << dbgMsg;
        EXPECT_EQ(metadata.type.value(), expected.getTreeEntryType()) << dbgMsg;
      }

      if ((verify_flags & VERIFY_STAT)) {
        // TODO: choose random?
        auto lastCheckoutTime =
            mount.getEdenMount()->getLastCheckoutTime().toTimespec();
        auto st = inodeOr
                      .stat(
                          lastCheckoutTime,
                          mount.getEdenMount()->getObjectStore(),
                          ObjectFetchContext::getNullContext())
                      .get();

        EXPECT_EQ(st.st_size, expected.getContents().size()) << dbgMsg;
#ifdef _WIN32
        EXPECT_EQ(st.st_mode, 0) << dbgMsg;
#else
        EXPECT_NE(st.st_mode, 0) << dbgMsg;
#endif
        // Note: string conversion makes this MUCH easier to comprehend in test
        // failures
        EXPECT_EQ(
            fmt::format("{:#o}", st.st_mode),
            fmt::format("{:#o}", expected.getMode()))
            << dbgMsg;

        EXPECT_EQ(
            stMtime(st).tv_sec, expected.getMtime(lastCheckoutTime).tv_sec)
            << dbgMsg;
        EXPECT_EQ(
            stMtime(st).tv_nsec, expected.getMtime(lastCheckoutTime).tv_nsec)
            << dbgMsg;
      }
    } else {
      EXPECT_EQ(expected.dtype, dtype_t::Unknown)
          << dbgMsg << " file was expected to be deleted, but was present";
    }
  }
}

#define VERIFY_TREE(flags) \
  verifyTreeState(__FILE__, __LINE__, mount, files, flags)
#define VERIFY_TREE_DEFAULT() \
  verifyTreeState(__FILE__, __LINE__, mount, files, VERIFY_DEFAULT)

// TODO: flesh this out, including deleted stuff, etc
#define EXPECT_INODE_OR(_inodeOr, _info)             \
  do {                                               \
    EXPECT_EQ((_inodeOr).getDtype(), (_info).dtype); \
  } while (0)
} // namespace

TEST(InodeOrTreeOrEntryTest, findDoesNotChangeState) {
  TestFileDatabase files;
  auto flags = VERIFY_DEFAULT & (~VERIFY_SHA1);
  auto mount = TestMount{MakeTestTreeBuilder(files)};
  VERIFY_TREE(flags);

  for (auto info : files.getOriginalItems()) {
    VERIFY_TREE(flags);
    auto inodeOr = mount.getInodeOrTreeOrEntry(info->path);
    EXPECT_INODE_OR(inodeOr, *info.get());
  }
  VERIFY_TREE(flags);
}

void testRootDirAChildren(TestMount& mount) {
  auto inodeOr = mount.getInodeOrTreeOrEntry(RelativePathPiece{"root_dirA"});
  EXPECT_TRUE(inodeOr.isDirectory());

  auto children = inodeOr.getChildren(
      RelativePathPiece{"root_dirA"},
      mount.getEdenMount()->getObjectStore(),
      ObjectFetchContext::getNullContext());
  EXPECT_EQ(2, children.value().size());
  EXPECT_THAT(
      children.value(), testing::Contains(testing::Key("child1_fileA1"_pc)));
  EXPECT_THAT(
      children.value(), testing::Contains(testing::Key("child1_fileA2"_pc)));
}

TEST(InodeOrTreeOrEntryTest, getChildrenSimple) {
  TestFileDatabase files;
  auto flags = VERIFY_DEFAULT & (~VERIFY_SHA1);
  auto mount = TestMount{MakeTestTreeBuilder(files)};
  VERIFY_TREE(flags);

  testRootDirAChildren(mount);
  VERIFY_TREE_DEFAULT();
}

TEST(InodeOrTreeOrEntryTest, getLoaded) {
  TestFileDatabase files;
  auto flags = VERIFY_DEFAULT & (~VERIFY_SHA1);
  auto mount = TestMount{MakeTestTreeBuilder(files)};
  VERIFY_TREE(flags);
  // load inode
  mount.getInode(RelativePathPiece{"root_dirA"});
  files.setFlags(RelativePathPiece{"root_dirA"}, FLAG_L);
  testRootDirAChildren(mount);
  VERIFY_TREE_DEFAULT();
}

TEST(InodeOrTreeOrEntryTest, getChildrenMaterialized) {
  TestFileDatabase files;
  auto flags = VERIFY_DEFAULT & (~VERIFY_SHA1);
  auto mount = TestMount{MakeTestTreeBuilder(files)};
  VERIFY_TREE(flags);
  // materialize inode
  std::string path = "root_dirA/child1_fileA1";
  std::string newContents = path + "~newContent";
  mount.overwriteFile(folly::StringPiece{path}, newContents);
  files.setContents(RelativePathPiece{path}, newContents);

  testRootDirAChildren(mount);
  VERIFY_TREE_DEFAULT();
}

TEST(InodeOrTreeOrEntryTest, getChildrenDoesNotChangeState) {
  TestFileDatabase files;
  auto flags = VERIFY_DEFAULT & (~VERIFY_SHA1);
  auto mount = TestMount{MakeTestTreeBuilder(files)};
  VERIFY_TREE(flags);

  for (auto info : files.getOriginalItems()) {
    VERIFY_TREE(flags);
    auto inodeOr = mount.getInodeOrTreeOrEntry(info->path);
    EXPECT_INODE_OR(inodeOr, *info.get());
    if (inodeOr.isDirectory()) {
      inodeOr.getChildren(
          info->path,
          mount.getEdenMount()->getObjectStore(),
          ObjectFetchContext::getNullContext());
    }
  }
  VERIFY_TREE(flags);
}

TEST(InodeOrTreeOrEntryTest, getChildrenAttributes) {
  TestFileDatabase files;
  auto flags = VERIFY_DEFAULT & (~VERIFY_SHA1);
  auto mount = TestMount{MakeTestTreeBuilder(files)};
  VERIFY_TREE(flags);

  for (auto info : files.getOriginalItems()) {
    VERIFY_TREE(flags);
    auto inodeOr = mount.getInodeOrTreeOrEntry(info->path);
    EXPECT_INODE_OR(inodeOr, *info.get());
    if (inodeOr.isDirectory()) {
      auto result = inodeOr
                        .getChildrenAttributes(
                            info->path,
                            mount.getEdenMount()->getObjectStore(),
                            ObjectFetchContext::getNullContext())
                        .get();

      for (auto child : files.getChildren(RelativePathPiece{info->path})) {
        auto childInodeOr = mount.getInodeOrTreeOrEntry(child->path);
        auto entryName = basename(child->path.stringPiece());
        EXPECT_THAT(
            result,
            testing::Contains(testing::Pair(
                entryName,
                childInodeOr
                    .getEntryAttributes(
                        child->path,
                        mount.getEdenMount()->getObjectStore(),
                        ObjectFetchContext::getNullContext())
                    .getTry())));
      }
    }
  }
  VERIFY_TREE(flags);
}

TEST(InodeOrTreeOrEntryTest, statDoesNotChangeState) {
  TestFileDatabase files;
  auto flags = VERIFY_DEFAULT | VERIFY_STAT;
  auto mount = TestMount{MakeTestTreeBuilder(files)};
  VERIFY_TREE(flags);

  for (auto info : files.getOriginalItems()) {
    VERIFY_TREE(flags);
    auto inodeOr = mount.getInodeOrTreeOrEntry(info->path);
    EXPECT_INODE_OR(inodeOr, *info.get());
  }
  VERIFY_TREE(flags);
}

TEST(InodeOrTreeOrEntryTest, fileOpsOnCorrectObjectsOnly) {
  TestFileDatabase files;
  auto mount = TestMount{MakeTestTreeBuilder(files)};

  VERIFY_TREE(VERIFY_INITIAL);
  for (auto info_ : files.getOriginalItems()) {
    auto& info = *info_;
    auto inodeOr = mount.getInodeOrTreeOrEntry(info.path);
    auto hashTry = inodeOr
                       .getSHA1(
                           info.path,
                           mount.getEdenMount()->getObjectStore(),
                           ObjectFetchContext::getNullContext())
                       .getTry();
    if (info.isRegularFile()) {
      EXPECT_EQ(true, hashTry.hasValue()) << " on path " << info.getLogPath();
      EXPECT_EQ(hashTry.value(), info.getSHA1())
          << " on path " << info.getLogPath();
    } else {
      EXPECT_EQ(true, hashTry.hasException())
          << " on path " << info.getLogPath();
    }

    auto metadataTry = inodeOr
                           .getEntryAttributes(
                               info.path,
                               mount.getEdenMount()->getObjectStore(),
                               ObjectFetchContext::getNullContext())
                           .getTry();
    if (info.isRegularFile()) {
      EXPECT_EQ(true, metadataTry.hasValue())
          << " on path " << info.getLogPath();
      if (metadataTry.hasValue()) {
        auto& metadata = metadataTry.value();
        EXPECT_EQ(metadata.sha1.value(), info.getSHA1())
            << " on path " << info.getLogPath();
        EXPECT_EQ(metadata.size.value(), info.getContents().size())
            << " on path " << info.getLogPath();
        EXPECT_EQ(metadata.type.value(), info.getTreeEntryType())
            << " on path " << info.getLogPath();
      }
    } else {
      EXPECT_EQ(true, metadataTry.hasValue())
          << " on path " << info.getLogPath();
      if (metadataTry.hasValue()) {
        auto& metadata = metadataTry.value();
        EXPECT_TRUE(metadata.sha1.hasException());
        EXPECT_TRUE(metadata.size.hasException());
        EXPECT_EQ(metadata.type.value(), info.getTreeEntryType())
            << " on path " << info.getLogPath();
      }
    }
    VERIFY_TREE(VERIFY_INITIAL);
  }
}

TEST(InodeOrTreeOrEntryTest, getEntryAttributesDoesNotChangeState) {
  TestFileDatabase files;
  auto mount = TestMount{MakeTestTreeBuilder(files)};

  for (auto info : files.getOriginalItems()) {
    VERIFY_TREE(VERIFY_DEFAULT & (~VERIFY_SHA1));
    auto inodeOr = mount.getInodeOrTreeOrEntry(info->path);
    EXPECT_INODE_OR(inodeOr, *info.get());
  }
  VERIFY_TREE(VERIFY_DEFAULT & (~VERIFY_SHA1));
}

TEST(InodeOrTreeOrEntryTest, getEntryAttributesAttributeError) {
  TestFileDatabase files;
  FakeTreeBuilder builder;
  files.build(builder);
  auto mount = TestMount{builder, false};

  builder.setReady("root_dirA");
  builder.setReady("root_dirA/child1_fileA2");

  auto inodeOr = mount.getInodeOrTreeOrEntry("root_dirA");

  auto attributesFuture = inodeOr.getEntryAttributes(
      RelativePathPiece{"root_dirA"},
      mount.getEdenMount()->getObjectStore(),
      ObjectFetchContext::getNullContext());

  builder.triggerError(
      "root_dirA/child1_fileA1", std::domain_error("fake error for testing"));

  auto attributes = std::move(attributesFuture).get();
  EXPECT_TRUE(attributes.sha1.hasException());
  EXPECT_TRUE(attributes.size.hasException());
  EXPECT_FALSE(attributes.type.hasException());
}

TEST(InodeOrTreeOrEntryTest, sha1DoesNotChangeState) {
  TestFileDatabase files;
  auto mount = TestMount{MakeTestTreeBuilder(files)};

  const std::vector<int> verify_flag_sets{
      VERIFY_DEFAULT & (~VERIFY_SHA1),
      VERIFY_DEFAULT,
  };
  for (auto verify_flags : verify_flag_sets) {
    VERIFY_TREE(verify_flags);
    for (auto info_ : files.getOriginalItems()) {
      auto& info = *info_;
      auto inodeOr = mount.getInodeOrTreeOrEntry(info.path);
      EXPECT_INODE_OR(inodeOr, info);

      if (info.isRegularFile()) {
        inodeOr
            .getSHA1(
                info.path,
                mount.getEdenMount()->getObjectStore(),
                ObjectFetchContext::getNullContext())
            .get();
      } else {
        EXPECT_THROW_ERRNO(
            inodeOr
                .getSHA1(
                    info.path,
                    mount.getEdenMount()->getObjectStore(),
                    ObjectFetchContext::getNullContext())
                .get(),
            EISDIR);
      }

      VERIFY_TREE(verify_flags);
    }
    VERIFY_TREE(verify_flags);
  }
}

TEST(InodeOrTreeOrEntryTest, unlinkMaterializesParents) {
  TestFileDatabase files;
  auto builder = MakeTestTreeBuilder(files);
  auto mount = TestMount(builder, true);

  VERIFY_TREE(VERIFY_INITIAL);

  auto root = mount.getEdenMount()->getRootInode();
  mount.deleteFile("root_fileA");
  files.del("root_fileA"_relpath);
  VERIFY_TREE_DEFAULT();

  mount.deleteFile("root_dirB/child1_dirB2/child2_fileBB4");
  files.del("root_dirB/child1_dirB2/child2_fileBB4"_relpath);
  VERIFY_TREE(VERIFY_DEFAULT & (~VERIFY_SHA1));
}

// Materialization is different on Windows vs other platforms...
TEST(InodeOrTreeOrEntryTest, materializationPropagation) {
  // One by one, start with something fresh, load the one, and check the state
  TestFileDatabase files;
  for (auto info_ : files.getOriginalItems()) {
    auto& info = *info_;
    if (!info.isRegularFile()) {
      continue;
    }

    auto builder = MakeTestTreeBuilder(files);
    auto mount = TestMount(builder, true);
    auto edenMount = mount.getEdenMount();
    VERIFY_TREE(VERIFY_INITIAL);

    // Materialize this one file
    std::string oldContents = info.pathStr();
    std::string newContents = oldContents + "~newContent";
    mount.overwriteFile(info.path.stringPiece(), newContents);
    files.setContents(info.path, newContents);
    VERIFY_TREE_DEFAULT();

    // TODO: how do we reset the state of the TestMount() back to initial? Some
    // resetParentCommit() or something on the edenMount?
    files.reset();
  }

  /* TODO: Until we can reliable reset a mount back to the initial state, these
   * tests are hard to do quickly */
  // Now do a set of random sets
  for (size_t iteration = 0; iteration < 20; ++iteration) {
    auto builder = MakeTestTreeBuilder(files);
    auto mount = TestMount(builder, true);
    auto edenMount = mount.getEdenMount();

    // TestFileDatabase files;
    VERIFY_TREE(VERIFY_INITIAL);
    // Materialize a random set of files
    size_t N = folly::Random::rand32() % files.size();
    for (size_t i = 0; i < N; ++i) {
      auto& info = files.getOriginalInfo(i);
      if (!info.isRegularFile()) {
        continue;
      }

      std::string oldContents = info.pathStr();
      std::string newContents = oldContents + "~newContent";
      mount.overwriteFile(info.path.stringPiece(), newContents);
      files.setContents(info.path, newContents);
      VERIFY_TREE_DEFAULT();
    }

    // TODO: how do we reset the state of the TestMount() back to initial? Some
    // resetParentCommit() or something on the edenMount?
    files.reset();
  }
}

TEST(InodeOrTreeOrEntryTest, loadPropagation) {
  // One by one, start with something fresh, load the one, and check the state
  TestFileDatabase files;
  auto builder = MakeTestTreeBuilder(files);
  auto mount = TestMount(builder, true);
  auto edenMount = mount.getEdenMount();
  for (auto info_ : files.getOriginalItems()) {
    auto& info = *info_;
    VERIFY_TREE(VERIFY_INITIAL);

    // Load this one file
    mount.getInode(info.path);
    files.setFlags(info.path, FLAG_L);
    VERIFY_TREE_DEFAULT();

    // Reset the state of the mount and the file list
    UnconditionalUnloader::unload(*edenMount->getRootInode());
    edenMount->getRootInode()->unloadChildrenUnreferencedByFs();
    files.reset();
  }

  // Now do a set of random sets
  for (size_t iteration = 0; iteration < 1000; ++iteration) {
    // TestFileDatabase files;
    VERIFY_TREE(VERIFY_INITIAL);
    // Load a random set of files
    size_t N = folly::Random::rand32() % files.size();
    for (size_t i = 0; i < N; ++i) {
      auto& info = files.getOriginalInfo(i);
      mount.getInode(info.path);
      files.setFlags(info.path, FLAG_L);
      VERIFY_TREE_DEFAULT();
    }

    // Reset the state of the mount and the file list
    UnconditionalUnloader::unload(*edenMount->getRootInode());
    edenMount->getRootInode()->unloadChildrenUnreferencedByFs();
    files.reset();
  }
  VERIFY_TREE(VERIFY_INITIAL);
}
