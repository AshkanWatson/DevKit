/// Repository contracts the UI consumes (DESIGN.md §11: "The UI should
/// call a fake repository/service that conforms to the planned domain
/// contract so it can later be replaced by the Rust FFI implementation
/// without redesigning every screen").
///
/// These interfaces mirror the planned `devkit-bridge` surface
/// (docs/ffi/bridge-contract.md): catalog reads, installed registry, and
/// the job center with a re-attachable event stream.
library;

import 'domain_models.dart';

/// Catalog browsing (future FFI: catalog reads from the local SQLite cache).
abstract interface class CatalogRepository {
  /// All packages available in the catalog.
  Future<List<CatalogPackage>> packages();

  /// One package by stable ID, or null when unknown.
  Future<CatalogPackage?> package(String id);

  /// All catalog profiles.
  Future<List<ProfileInfo>> profiles();
}

/// The installed registry (future FFI: `list_installed_packages`,
/// `list_installed_versions` and registry writes).
abstract interface class InstalledRepository {
  Future<List<InstalledPackage>> installed();

  /// Compute what activating `version` would change — the consent step's
  /// data (DESIGN.md journey C). Pure read; future FFI: `plan_activation`.
  Future<ActivationPlan> planActivation(String packageId, String version);

  /// Set the global active version of a package (version-management doc
  /// §3). May only be called after the user consented to the plan.
  Future<void> setActiveVersion(String packageId, String version);

  /// Uninstall one concrete version (destructive; the UI confirms first).
  Future<void> uninstall(String packageId, String version);
}

/// The job center (future FFI: `start_install`, `cancel_job`, `get_job`,
/// `watch_job_events` replayed from SQLite).
abstract interface class JobsRepository {
  /// Current job list (active jobs first, then history).
  Future<List<JobInfo>> jobs();

  /// Snapshot stream for live progress; emits on every state change.
  Stream<List<JobInfo>> watchJobs();

  /// Start a simulated install job; returns the job ID.
  Future<String> startInstall({
    required String packageId,
    required String packageLabel,
    required String version,
  });

  /// Request cooperative cancellation. The job stops at the next safe
  /// boundary; this returns immediately.
  Future<void> cancelJob(String jobId);
}
