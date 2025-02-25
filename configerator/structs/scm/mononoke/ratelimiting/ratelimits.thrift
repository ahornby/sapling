// @generated SignedSource<<424b38db5f0b4a9fb575e86a1a9237cc>>
// DO NOT EDIT THIS FILE MANUALLY!
// This file is a mechanical copy of the version in the configerator repo. To
// modify it, edit the copy in the configerator repo instead and copy it over by
// running the following in your fbcode directory:
//
// configerator-thrift-updater scm/mononoke/ratelimiting/ratelimits.thrift
# @oncall scm_server_infra
include "thrift/annotation/cpp.thrift"
include "thrift/annotation/rust.thrift"

typedef i64 RepoId

// Which clients should the rate limiting or load shedding rule apply to?
union Target {
  @cpp.Ref{type = cpp.RefType.Unique}
  @rust.Box
  // Deprecated
  // 1: Target not_target;
  // Deprecated, use several identities in the identities filed to achieve the same
  // 2: list<Target> and_target;
  // Deprecated, create a limit for each target to achieve the same effect
  // 3: list<Target> or_target;
  // Deprecated, use a single identity in the identities field for the same effect
  // 4: string identity;
  // A static slice of hosts that are chosen by hashing the client's hostname
  5: StaticSlice static_slice;
  // ClientMainId, intended for use like SR client_id
  6: string main_client_id;
  // A list of client's identities, such as ["MACHINE_TIER:sandcastle"]
  // All must be present for the rule to apply
  7: list<string> identities;
}

// We do not want static slices to use other StaticSlices as a target.
// At the same time I want to avoid verbosity when defining Targets
union StaticSliceTarget {
  1: list<string> identities;
  2: string main_client_id;
}

@rust.Exhaustive
struct StaticSlice {
  // The percentage of hosts this slice applies to. 0 <= slice_pct <= 100
  1: i32 slice_pct;
  // The nonce can be used to rotate hosts in a slice
  2: string nonce;
  // Target this should apply to
  3: StaticSliceTarget target;
}

enum RateLimitStatus {
  // Don't run this code at all.
  Disabled = 0,
  // Track this limit, but don't enforce it.
  Tracked = 1,
  // Enforce this limit.
  Enforced = 2,
}

// FciMetricKey are tracked and contributed to by all servers.
// The counters are explicitly bumped by Mononoke's code and are backed by FCI.
enum FciMetricKey {
  // The amount of bytes egressed by Mononoke servers
  EgressBytes = 0,
  // The number of manifests served
  TotalManifests = 1,
  // The number of files served
  GetpackFiles = 2,
  // The number of commits served
  Commits = 3,
  // The number of commits created by an author (wireproto)
  CommitsPerAuthor = 4,
  // The number of commits pushed by a user
  CommitsPerUser = 5,
  // The number of EdenAPI requests
  EdenApiQps = 6,
}

enum FciMetricScope {
  // Global
  Global = 0,
  // Regional, the datacenter prefix is insterted into the key
  Regional = 1,
}

@rust.Exhaustive
struct FciMetric {
  // The key that will be used in FCI. Region or other parameters may be inserted by the code later
  1: FciMetricKey metric;
  // The window over which to count the metric
  2: i64 window;
  // What scope does the ratelimit apply to, it will influence the final FCI metric key that it's compared to
  3: FciMetricScope scope;
}

@rust.Exhaustive
struct RateLimitBody {
  // Whether the rate limit is enabled
  1: RateLimitStatus status;
  // The limit above which requests will be rate limited
  2: double limit;
// Deprecated
// 3: optional i64 window;
}

@rust.Exhaustive
struct RateLimit {
  // Deprecated
  // 1: RegionalMetric metric;
  // The target of the RateLimit. If this is null then the RateLimit will
  // apply to all clients
  2: optional Target target;
  3: RateLimitBody limit;
  // New style metric
  4: FciMetric fci_metric;
}

@rust.Exhaustive
struct LoadShedLimit {
  // Deprecated
  // 1: string metric;
  // Whether the rate limit is enabled
  2: RateLimitStatus status;
  // The target of the RateLimit. If this is null then the RateLimit will
  // apply to all clients
  3: optional Target target;
  // The limit above which requests will be rate limited
  4: i64 limit;
  // The metric used to load shed
  5: LoadSheddingMetric load_shedding_metric;
}

// The metric used for load shedding.
union LoadSheddingMetric {
  // A counter exposed by the binary itself, i.e. starting with "mononoke.".
  1: string local_fb303_counter;
  // A counter not exposed by the binary.
  2: ExternalOdsCounter external_ods_counter;
}
@rust.Exhaustive
struct ExternalOdsCounter {
  // The entity of the ODS counter.
  1: string entity;
  // The key of the ODS counter.
  2: string key;
  // An expression to convert multiple timeseries into one. See https://fburl.com/wiki/c32jyv09
  3: optional string reduce;
}

@rust.Exhaustive
struct MononokeRateLimits {
  // The RateLimits that should be checked
  1: list<RateLimit> rate_limits;
  // The LoadShedLimits that should be checked
  2: list<LoadShedLimit> load_shed_limits;
// Deprecated
// 3: map<string, i32> datacenter_prefix_capacity;
// Deprecated
// 4: RateLimitBody commits_per_author;
// Deprecated
// 5: optional RateLimitBody total_file_changes;
}
