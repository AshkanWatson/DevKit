/// P3-05: the real FFI repositories (the P1-08 final proof).
///
/// These implement the same domain contracts as the Phase 1 fakes
/// (`fake_repositories.dart`) over the generated Rust bridge surface
/// (`../bridge/api/*`), so no screen changes when the engine takes over:
/// - `FfiCatalogRepository`: offline browsing over the local SQLite
///   cache (the cache is filled by `sync_catalog` against the Django API).
/// - `FfiInstalledRepository`: the SQLite registry + the P3-04
///   activation consent pair and exact-version uninstall.
/// - `FfiJobsRepository`: real install jobs (`devkit-jobs`), persisted
///   event logs, and the live snapshot stream. No `onInstallCompleted`
///   hook — the engine records the receipt itself.
///
/// Every operation initializes the engine lazily (`init_engine` is
/// idempotent; the default paths follow HANDOFF.md §6). Bridge errors
/// propagate as `BridgeError` exceptions carrying the stable contract
/// code — Flutter maps codes to copy, never the message text.
///
/// Widget tests never touch this file: the provider defaults fall back
/// to the fakes when `FLUTTER_TEST` is set (repository_providers.dart).
library;

import '../bridge/api/catalog.dart' as bridge_catalog;
import '../bridge/api/engine.dart' as bridge_engine;
import '../bridge/api/installed.dart' as bridge_installed;
import '../bridge/api/jobs.dart' as bridge_jobs;
import '../bridge/api/profiles.dart' as bridge_profiles;
import '../bridge/bridge_providers.dart' show ensureBridgeInitialized;
import '../domain/domain_models.dart';
import '../domain/repository_contracts.dart';
import 'fake_repositories.dart' show FixtureCatalogRepository;

/// One-time engine startup: bridge init + idempotent `init_engine`
/// (default managed root per HANDOFF.md §6; migrations and orphan
/// recovery run inside the Rust engine).
Future<void>? _engineReady;

Future<void> _ensureEngine() {
  return _engineReady ??= () async {
    await ensureBridgeInitialized();
    await bridge_engine.initEngine();
  }();
}

// ---------------------------------------------------------------------------
// Catalog (offline browsing over the SQLite cache)
// ---------------------------------------------------------------------------

/// Maps one cached manifest to the UI read model. Versions are
/// normalized to newest-first exactly like the fake repository, so
/// `newestStable` and the detail screens behave identically.
CatalogPackage catalogPackageFromDto(bridge_catalog.CatalogPackageDto dto) {
  return CatalogPackage(
    id: dto.id,
    name: dto.name,
    description: dto.description,
    publisher: dto.publisher,
    homepage: dto.homepage,
    category: PackageCategory.fromWire(dto.category),
    platforms: [
      for (final platform in dto.platforms) PlatformTarget.fromWire(platform),
    ],
    defaultPolicy: VersionPolicyName.fromWire(dto.defaultPolicy),
    versions: [
      for (final version in dto.versions) _packageVersionFromDto(version),
    ]..sort(FixtureCatalogRepository.compareVersionsDesc),
    license: dto.license,
    updatedAt: dto.updatedAt,
  );
}

PackageVersionInfo _packageVersionFromDto(
  bridge_catalog.CatalogVersionDto version,
) {
  return PackageVersionInfo(
    version: version.version,
    channel: ReleaseChannel.fromWire(version.channel),
    releaseDate: version.releaseDate,
    releaseNotesUrl: version.releaseNotesUrl,
    dependencies: [
      for (final dependency in version.dependencies)
        DependencyRef(
          packageId: dependency.packageId,
          required: dependency.required_,
          policy: VersionPolicyName.fromWire(dependency.policy),
          version: dependency.version,
          channel: dependency.channel,
        ),
    ],
    artifacts: [
      for (final artifact in version.artifacts) _artifactFromDto(artifact),
    ],
  );
}

/// The UI shows the SOURCE DOMAIN only — never the full URL (no private
/// query parameters in UI copy or logs).
ArtifactInfo _artifactFromDto(bridge_catalog.CatalogArtifactDto artifact) {
  return ArtifactInfo(
    os: artifact.os,
    architecture: artifact.architecture,
    installer: InstallerKind.fromWire(artifact.installer),
    sourceDomain: Uri.tryParse(artifact.url)?.host ?? '',
    sizeBytes: artifact.sizeBytes.toInt(),
    hasChecksum: artifact.sha256.length == 64,
  );
}

ProfileInfo profileInfoFromDto(bridge_catalog.CatalogProfileDto dto) {
  return ProfileInfo(
    id: dto.id,
    name: dto.name,
    description: dto.description,
    maintainer: dto.maintainer,
    platforms: [
      for (final platform in dto.platforms) PlatformTarget.fromWire(platform),
    ],
    packages: [
      for (final package in dto.packages)
        ProfilePackageRef(
          packageId: package.packageId,
          required: package.required_,
          policy: VersionPolicyName.fromWire(package.policy),
          version: package.version,
          channel: package.channel,
          reason: package.reason,
        ),
    ],
  );
}

class FfiCatalogRepository implements CatalogRepository {
  @override
  Future<List<CatalogPackage>> packages() async {
    await _ensureEngine();
    final dtos = await bridge_catalog.searchCatalog();
    return [for (final dto in dtos) catalogPackageFromDto(dto)];
  }

  @override
  Future<CatalogPackage?> package(String id) async {
    await _ensureEngine();
    final dto = await bridge_catalog.getPackageDetails(packageId: id);
    return dto == null ? null : catalogPackageFromDto(dto);
  }

  @override
  Future<List<ProfileInfo>> profiles() async {
    await _ensureEngine();
    final dtos = await bridge_catalog.listCatalogProfiles();
    return [for (final dto in dtos) profileInfoFromDto(dto)];
  }

  @override
  Future<ProfileResolution> resolveProfile(String profileId) async {
    await _ensureEngine();
    final dto = await bridge_profiles.resolveProfile(profileId: profileId);
    return ProfileResolution(
      profileId: dto.profileId,
      packages: [
        for (final p in dto.packages)
          ResolvedProfilePackage(
            packageId: p.packageId,
            required: p.required_,
            policy: p.policy,
            version: p.version,
          ),
      ],
      skipped: [
        for (final s in dto.skipped)
          SkippedProfilePackage(
            packageId: s.packageId,
            reason: ProfileSkipReason.fromWire(s.reason.name),
          ),
      ],
    );
  }

  @override
  Future<List<ProfileInfo>> personalProfiles() async {
    await _ensureEngine();
    final dtos = await bridge_profiles.listPersonalProfiles();
    return [for (final dto in dtos) profileInfoFromDto(dto)];
  }

  @override
  Future<ProfileInfo> savePersonalProfile(String manifestJson) async {
    await _ensureEngine();
    final dto = await bridge_profiles.savePersonalProfile(
      manifestJson: manifestJson,
    );
    return profileInfoFromDto(dto);
  }

  @override
  Future<bool> deletePersonalProfile(String id) async {
    await _ensureEngine();
    return bridge_profiles.deletePersonalProfile(id: id);
  }

  @override
  Future<ProjectPlan> planProject(
    String projectJson, {
    String? profileId,
  }) async {
    await _ensureEngine();
    final dto = await bridge_profiles.planProject(
      projectJson: projectJson,
      profileId: profileId,
    );
    return ProjectPlan(
      projectName: dto.projectName,
      pins: [
        for (final pin in dto.pins)
          ProjectPin(
            packageId: pin.packageId,
            version: pin.version,
            source: PinSource.fromWire(pin.source),
            currentActive: pin.currentActive,
            changeNeeded: pin.changeNeeded,
          ),
      ],
      unresolved: List<String>.from(dto.unresolved),
    );
  }
}

// ---------------------------------------------------------------------------
// Installed registry + activation consent pair (P3-04 semantics)
// ---------------------------------------------------------------------------

/// Maps the registry status wire string. Only settled states reach the
/// list ops (`installing`/`removing` rows belong to a running job).
InstalledStatus _installedStatusFromWire(String status) {
  switch (status) {
    case 'broken':
      return InstalledStatus.broken;
    case 'installed':
    default:
      return InstalledStatus.installed;
  }
}

InstalledPackage installedPackageFromDto(
  bridge_installed.InstalledPackageDto dto,
) {
  return InstalledPackage(
    packageId: dto.packageId,
    name: dto.name,
    version: dto.version,
    status: _installedStatusFromWire(dto.status),
    isActive: dto.isActive,
    // Everything in this registry is DevKit-managed; externally
    // detected software never enters it (HANDOFF.md §10).
    ownership: InstallOwnership.devkitManaged,
    locationLabel: dto.locationLabel,
  );
}

/// Phrases the PATH part of an activation plan for the consent dialog.
/// The append/already-present copy mirrors the Phase 1 fake word for
/// word; the unavailable branch surfaces the engine's reason.
String pathChangeLabel(bridge_installed.PathChangeDto change) {
  switch (change.kind) {
    case bridge_installed.PathChangeKind.alreadyPresent:
      return 'The DevKit shims folder is already on your user PATH — '
          'no PATH change is needed.';
    case bridge_installed.PathChangeKind.append:
      return 'Add the DevKit shims folder to your user PATH (user scope '
          'only; no administrator rights required).';
    case bridge_installed.PathChangeKind.unavailable:
      return change.reason ??
          'PATH integration is unavailable on this '
              'platform; shims still work when invoked by absolute path.';
  }
}

ActivationPlan activationPlanFromDto(bridge_installed.ActivationPlanDto dto) {
  return ActivationPlan(
    shimFiles: [for (final shim in dto.shims) shim.shimRelativePath],
    pathChange: pathChangeLabel(dto.pathChange),
    // The previous mapping and shims are kept for the operation's
    // duration, so activation can always be reverted (P3-04).
    reversible: true,
    previousActiveVersion: dto.previousActiveVersion,
  );
}

class FfiInstalledRepository implements InstalledRepository {
  @override
  Future<List<InstalledPackage>> installed() async {
    await _ensureEngine();
    final dtos = await bridge_installed.listInstalledPackages();
    return [for (final dto in dtos) installedPackageFromDto(dto)];
  }

  @override
  Future<ActivationPlan> planActivation(
    String packageId,
    String version,
  ) async {
    await _ensureEngine();
    final dto = await bridge_installed.planActivationFor(
      packageId: packageId,
      version: version,
    );
    return activationPlanFromDto(dto);
  }

  @override
  Future<void> setActiveVersion(String packageId, String version) async {
    await _ensureEngine();
    // The UI calls this only after the user accepted the plan, so
    // consent is granted here; the engine refuses `consent: false` with
    // PERMISSION_REQUIRED and writes nothing (the dialog data comes
    // from [planActivation]).
    await bridge_installed.setActiveVersion(
      packageId: packageId,
      version: version,
      consent: true,
    );
  }

  @override
  Future<void> uninstall(String packageId, String version) async {
    await _ensureEngine();
    // Receipt-driven exact-version removal; a missing version is
    // reported as `found: false` and is effectively idempotent.
    await bridge_installed.removeManagedVersion(
      packageId: packageId,
      version: version,
    );
  }
}

// ---------------------------------------------------------------------------
// Jobs (real engine: devkit-jobs through the bridge)
// ---------------------------------------------------------------------------

JobLogEntry jobLogEntryFromDto(bridge_jobs.JobEventDto event) {
  return JobLogEntry(
    stage: JobStage.fromWire(event.stage),
    message: event.message,
    progress: event.progress,
    errorCode: event.errorCode,
  );
}

JobInfo jobInfoFromDto(bridge_jobs.JobDto job) {
  return JobInfo(
    id: job.id,
    packageId: job.packageId ?? '',
    // The engine resolves the display name from the catalog cache;
    // unknown packages fall back to the stable ID.
    packageLabel: job.packageName ?? job.packageId ?? '',
    version: job.version ?? '',
    stage: JobStage.fromWire(job.state),
    log: [for (final event in job.events) jobLogEntryFromDto(event)],
    errorCode: job.errorCode,
  );
}

class FfiJobsRepository implements JobsRepository {
  @override
  Future<List<JobInfo>> jobs() async {
    await _ensureEngine();
    final dtos = await bridge_jobs.listJobs();
    return [for (final dto in dtos) jobInfoFromDto(dto)];
  }

  @override
  Stream<List<JobInfo>> watchJobs() {
    // The Rust poller emits the full snapshot whenever the persisted
    // store changes and stays alive until the Dart stream is cancelled
    // (new installs appear without re-subscribing).
    return bridge_jobs.watchJobs().map(
      (dtos) => [for (final dto in dtos) jobInfoFromDto(dto)],
    );
  }

  @override
  Future<String> startInstall({
    required String packageId,
    required String packageLabel,
    required String version,
  }) async {
    await _ensureEngine();
    // `packageLabel` is accepted for contract parity with the fake; the
    // engine resolves the display name from the catalog cache itself
    // (single source of truth, no UI-provided strings in the registry).
    return bridge_jobs.startInstall(packageId: packageId, version: version);
  }

  @override
  Future<void> cancelJob(String jobId) async {
    await _ensureEngine();
    // Accepted / already-finished / unknown are all "nothing to do" for
    // the UI (terminal states reject cancellation; the control is
    // disabled for them) — the contract method returns void.
    await bridge_jobs.cancelJob(jobId: jobId);
  }

  @override
  Future<ProfileApplyResult> applyProfile(
    String profileId,
    List<String> selectedPackageIds,
  ) async {
    await _ensureEngine();
    final dto = await bridge_profiles.applyProfile(
      profileId: profileId,
      selectedPackageIds: selectedPackageIds,
    );
    return ProfileApplyResult(
      profileId: dto.profileId,
      jobs: [
        for (final job in dto.jobs)
          ProfileApplyJob(
            packageId: job.packageId,
            version: job.version,
            jobId: job.jobId,
          ),
      ],
      skipped: [
        for (final skip in dto.skipped)
          SkippedProfilePackage(
            packageId: skip.packageId,
            reason: ProfileSkipReason.fromWire(skip.reason.name),
          ),
      ],
    );
  }
}
