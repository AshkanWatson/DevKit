/// Riverpod wiring for the domain repositories and their read models.
///
/// Tests override the repository providers with fakes (widget tests must
/// never touch real FFI or timers they do not own). When the Rust FFI
/// repositories land (P1-08+), only these provider defaults change.
library;

import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../data/fake_repositories.dart';
import '../domain/domain_models.dart';
import '../domain/repository_contracts.dart';

/// Catalog source: fixtures in Phase 1, the SQLite-backed bridge later.
final catalogRepositoryProvider = Provider<CatalogRepository>(
  (ref) => FixtureCatalogRepository(),
);

/// Installed registry source (mock states in Phase 1).
final installedRepositoryProvider = Provider<InstalledRepository>(
  (ref) => FakeInstalledRepository(),
);

/// Job center source (Dart simulator in Phase 1, `devkit-jobs` via FFI later).
final jobsRepositoryProvider = Provider<JobsRepository>(
  (ref) => FakeJobsRepository(),
);

/// All catalog packages.
final catalogPackagesProvider = FutureProvider<List<CatalogPackage>>(
  (ref) => ref.watch(catalogRepositoryProvider).packages(),
);

/// One package by stable ID.
final packageDetailsProvider = FutureProvider.family<CatalogPackage?, String>(
  (ref, id) => ref.watch(catalogRepositoryProvider).package(id),
);

/// All catalog profiles.
final catalogProfilesProvider = FutureProvider<List<ProfileInfo>>(
  (ref) => ref.watch(catalogRepositoryProvider).profiles(),
);

/// Installed packages.
final installedPackagesProvider = FutureProvider<List<InstalledPackage>>(
  (ref) => ref.watch(installedRepositoryProvider).installed(),
);

/// Live job snapshots (re-attachable event stream).
final jobsProvider = StreamProvider<List<JobInfo>>(
  (ref) => ref.watch(jobsRepositoryProvider).watchJobs(),
);
