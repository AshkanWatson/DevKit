/// Riverpod wiring for the domain repositories and their read models.
///
/// Defaults: the real Rust-engine repositories (P3-05) — except under
/// `flutter test`, where the Phase 1 fakes are kept so widget tests
/// never touch real FFI or timers they do not own (the FLUTTER_TEST
/// environment variable is set by the Flutter test runner itself).
library;

import 'dart:io' show Platform;

import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../data/fake_repositories.dart';
import '../data/ffi_repositories.dart';
import '../domain/domain_models.dart';
import '../domain/repository_contracts.dart';

/// True under `flutter test`: widget tests must never touch real FFI,
/// so the provider defaults fall back to the Phase 1 fakes there.
bool get _runningInFlutterTest =>
    Platform.environment.containsKey('FLUTTER_TEST');

/// Catalog source: the SQLite-backed bridge (offline browsing after the
/// first sync); fixtures under `flutter test`.
final catalogRepositoryProvider = Provider<CatalogRepository>((ref) {
  if (_runningInFlutterTest) return FixtureCatalogRepository();
  return FfiCatalogRepository();
});

/// Installed registry source: the real SQLite registry; mock states
/// under `flutter test`.
final installedRepositoryProvider = Provider<InstalledRepository>((ref) {
  if (_runningInFlutterTest) return FakeInstalledRepository();
  return FfiInstalledRepository();
});

/// Job center source: the real `devkit-jobs` engine via FFI; the Dart
/// simulator under `flutter test`.
///
/// The engine records install receipts itself, so unlike the Phase 1
/// wiring there is no `onInstallCompleted` hook: a completed job shows
/// up in the Installed destination through the registry, and the next
/// `watchJobs` emission refreshes the read models.
final jobsRepositoryProvider = Provider<JobsRepository>((ref) {
  if (_runningInFlutterTest) {
    final installed = ref.watch(installedRepositoryProvider);
    return FakeJobsRepository(
      onInstallCompleted: (packageId, packageLabel, version) async {
        if (installed case final FakeInstalledRepository fake) {
          await fake.addManagedInstall(
            packageId: packageId,
            name: packageLabel,
            version: version,
          );
          // Refresh the Installed read model so the new version shows up
          // without a manual reload.
          ref.invalidate(installedPackagesProvider);
        }
      },
    );
  }
  return FfiJobsRepository();
});

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

/// User-created/user-edited personal profiles (P4-04).
final personalProfilesProvider = FutureProvider<List<ProfileInfo>>(
  (ref) => ref.watch(catalogRepositoryProvider).personalProfiles(),
);

/// Installed packages.
final installedPackagesProvider = FutureProvider<List<InstalledPackage>>(
  (ref) => ref.watch(installedRepositoryProvider).installed(),
);

/// Live job snapshots (re-attachable event stream).
final jobsProvider = StreamProvider<List<JobInfo>>(
  (ref) => ref.watch(jobsRepositoryProvider).watchJobs(),
);
