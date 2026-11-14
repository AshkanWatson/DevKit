/// Phase 1 fake repositories (DESIGN.md §11, P1-06/P1-08).
///
/// - `FixtureCatalogRepository` parses the embedded fixture manifests.
/// - `FakeInstalledRepository` seeds mock version states.
/// - `FakeJobsRepository` simulates install jobs with the same state
///   machine, progress rules, and rollback semantics as the Rust
///   `devkit-jobs` crate, so the swap to FFI later needs no screen changes.
///
/// No network, no FFI, no real installation work in Phase 1.
library;

import 'dart:async';
import 'dart:convert';

import '../domain/domain_models.dart';
import '../domain/repository_contracts.dart';
import 'fixture_catalog_data.dart';

// ---------------------------------------------------------------------------
// Catalog
// ---------------------------------------------------------------------------

class FixtureCatalogRepository implements CatalogRepository {
  FixtureCatalogRepository();

  late final List<CatalogPackage> _packages =
      fixturePackageDocuments.map(_parsePackage).toList()
        ..sort((a, b) => a.id.compareTo(b.id));
  late final List<ProfileInfo> _profiles =
      fixtureProfileDocuments.map(_parseProfile).toList()
        ..sort((a, b) => a.id.compareTo(b.id));

  @override
  Future<List<CatalogPackage>> packages() async => List.unmodifiable(_packages);

  @override
  Future<CatalogPackage?> package(String id) async {
    for (final package in _packages) {
      if (package.id == id) return package;
    }
    return null;
  }

  @override
  Future<List<ProfileInfo>> profiles() async => List.unmodifiable(_profiles);

  /// Parse a package manifest. Throws [FormatException] on missing/invalid
  /// fields — screens render an error state for corrupt catalog data.
  static CatalogPackage _parsePackage(String json) {
    final map = jsonDecode(json) as Map<String, dynamic>;
    return CatalogPackage(
      id: map['id'] as String,
      name: map['name'] as String,
      description: map['description'] as String? ?? '',
      publisher: map['publisher'] as String,
      homepage: map['homepage'] as String? ?? '',
      category: PackageCategory.fromWire(map['category'] as String),
      platforms: (map['supported_platforms'] as List<dynamic>? ?? const [])
          .cast<String>()
          .map(PlatformTarget.fromWire)
          .toList(),
      defaultPolicy: VersionPolicyName.fromWire(
        map['default_version_policy'] as String,
      ),
      versions:
          (map['versions'] as List<dynamic>? ?? const [])
              .cast<Map<String, dynamic>>()
              .map(_parseVersion)
              .toList()
            // Manifests do not guarantee ordering; normalize to newest-first so
            // "latest" resolves to the newest listed version and "stable" to
            // the newest non-prerelease.
            ..sort((a, b) => _compareVersionsDesc(a, b)),
      license: map['license_expression'] as String? ?? '',
      updatedAt: map['updated_at'] as String? ?? '',
    );
  }

  /// Newest-first version comparison over dotted numeric cores
  /// (`3.13.1` > `3.12.7`, `1.99.0-insider` > `1.98.0`). Equal cores put
  /// stable releases before prereleases.
  static int _compareVersionsDesc(PackageVersionInfo a, PackageVersionInfo b) {
    final ca = _versionCore(a.version);
    final cb = _versionCore(b.version);
    for (var i = 0; i < ca.length && i < cb.length; i++) {
      final cmp = cb[i].compareTo(ca[i]); // descending
      if (cmp != 0) return cmp;
    }
    final lengthCmp = cb.length.compareTo(ca.length);
    if (lengthCmp != 0) return lengthCmp;
    if (a.isPrerelease != b.isPrerelease) return a.isPrerelease ? 1 : -1;
    return 0;
  }

  static List<int> _versionCore(String version) {
    final core = version.split('-').first.split('+').first;
    return [for (final part in core.split('.')) int.tryParse(part) ?? 0];
  }

  static PackageVersionInfo _parseVersion(Map<String, dynamic> map) {
    return PackageVersionInfo(
      version: map['version'] as String,
      channel: ReleaseChannel.fromWire(map['channel'] as String),
      releaseDate: map['release_date'] as String?,
      releaseNotesUrl: map['release_notes_url'] as String?,
      dependencies: (map['dependencies'] as List<dynamic>? ?? const [])
          .cast<Map<String, dynamic>>()
          .map(
            (d) => DependencyRef(
              packageId: d['package_id'] as String,
              required: d['required'] as bool? ?? false,
              policy: VersionPolicyName.fromWire(d['version_policy'] as String),
              version: d['version'] as String?,
              channel: d['channel'] as String?,
            ),
          )
          .toList(),
      artifacts: (map['artifacts'] as List<dynamic>? ?? const [])
          .cast<Map<String, dynamic>>()
          .map(_parseArtifact)
          .toList(),
    );
  }

  static ArtifactInfo _parseArtifact(Map<String, dynamic> map) {
    final url = map['url'] as String? ?? '';
    // Show only the source domain — never the full URL (no private query
    // parameters in UI or logs).
    final host = Uri.tryParse(url)?.host ?? '';
    return ArtifactInfo(
      os: map['os'] as String? ?? '',
      architecture: map['architecture'] as String? ?? '',
      installer: InstallerKind.fromWire(
        map['installer'] as String? ?? 'archive',
      ),
      sourceDomain: host,
      sizeBytes: map['size_bytes'] as int? ?? 0,
      hasChecksum: (map['sha256'] as String? ?? '').length == 64,
    );
  }

  static ProfileInfo _parseProfile(String json) {
    final map = jsonDecode(json) as Map<String, dynamic>;
    return ProfileInfo(
      id: map['id'] as String,
      name: map['name'] as String,
      description: map['description'] as String? ?? '',
      maintainer: map['maintainer'] as String? ?? '',
      platforms: (map['supported_platforms'] as List<dynamic>? ?? const [])
          .cast<String>()
          .map(PlatformTarget.fromWire)
          .toList(),
      packages: (map['packages'] as List<dynamic>? ?? const [])
          .cast<Map<String, dynamic>>()
          .map(
            (p) => ProfilePackageRef(
              packageId: p['package_id'] as String,
              required: p['required'] as bool? ?? false,
              policy: VersionPolicyName.fromWire(p['version_policy'] as String),
              version: p['version'] as String?,
              channel: p['channel'] as String?,
              reason: p['reason'] as String? ?? '',
            ),
          )
          .toList(),
    );
  }
}

// ---------------------------------------------------------------------------
// Installed registry (mock states, DESIGN.md §11)
// ---------------------------------------------------------------------------

class FakeInstalledRepository implements InstalledRepository {
  FakeInstalledRepository() {
    _installed = [
      const InstalledPackage(
        packageId: 'devkit.lang.python',
        name: 'Python',
        version: '3.12.7',
        status: InstalledStatus.installed,
        isActive: true,
        ownership: InstallOwnership.devkitManaged,
        locationLabel: 'devkit\\state\\packages\\python\\3.12.7',
      ),
      const InstalledPackage(
        packageId: 'devkit.utility.git',
        name: 'Git',
        version: '2.51.0',
        status: InstalledStatus.installed,
        isActive: false,
        ownership: InstallOwnership.devkitManaged,
        locationLabel: 'devkit\\state\\packages\\git\\2.51.0',
      ),
      const InstalledPackage(
        packageId: 'devkit.ide.vscode',
        name: 'Visual Studio Code',
        version: '1.96.2',
        status: InstalledStatus.installed,
        isActive: false,
        ownership: InstallOwnership.externalDetected,
        locationLabel: 'C:\\Program Files\\Microsoft VS Code',
      ),
      const InstalledPackage(
        packageId: 'devkit.lang.rust',
        name: 'Rust (toolchain)',
        version: '1.97.1',
        status: InstalledStatus.broken,
        isActive: false,
        ownership: InstallOwnership.devkitManaged,
        locationLabel: 'devkit\\state\\packages\\rust\\1.97.1',
      ),
    ];
  }

  late List<InstalledPackage> _installed;

  @override
  Future<List<InstalledPackage>> installed() async =>
      List.unmodifiable(_installed);

  @override
  Future<void> setActiveVersion(String packageId, String version) async {
    _installed = [
      for (final package in _installed)
        if (package.packageId == packageId &&
            package.ownership == InstallOwnership.devkitManaged)
          InstalledPackage(
            packageId: package.packageId,
            name: package.name,
            version: version,
            status: InstalledStatus.installed,
            isActive: package.version == version,
            ownership: package.ownership,
            locationLabel: package.locationLabel,
          )
        else if (package.packageId == packageId)
          // Only one active version per package.
          InstalledPackage(
            packageId: package.packageId,
            name: package.name,
            version: package.version,
            status: package.status,
            isActive: false,
            ownership: package.ownership,
            locationLabel: package.locationLabel,
          )
        else
          package,
    ];
  }

  @override
  Future<void> uninstall(String packageId, String version) async {
    _installed = [
      for (final package in _installed)
        if (!(package.packageId == packageId && package.version == version))
          package,
    ];
  }

  /// Registers a completed (simulated) install as a DevKit-managed,
  /// active version. The wired counterpart of the real engine recording
  /// the install receipt in the SQLite registry (Phase 3 does this for
  /// real, including PATH integration with explicit consent).
  Future<void> addManagedInstall({
    required String packageId,
    required String name,
    required String version,
  }) async {
    final shortId = packageId.split('.').last;
    final location = 'devkit\\state\\packages\\$shortId\\$version';
    final updated = InstalledPackage(
      packageId: packageId,
      name: name,
      version: version,
      status: InstalledStatus.installed,
      isActive: true,
      ownership: InstallOwnership.devkitManaged,
      locationLabel: location,
    );
    _installed = [
      updated,
      // Keep other packages, and keep external (non-managed) entries for
      // the same package; the managed entry is superseded by the new
      // active version.
      for (final package in _installed)
        if (package.packageId != packageId ||
            package.ownership != InstallOwnership.devkitManaged)
          package,
    ];
  }
}

// ---------------------------------------------------------------------------
// Simulated jobs (mirrors crates/devkit-jobs semantics)
// ---------------------------------------------------------------------------

class _SimStep {
  const _SimStep(this.stage, this.progress, this.message);
  final JobStage stage;
  final double? progress;
  final String message;
}

class _FakeJob {
  _FakeJob({
    required this.id,
    required this.packageId,
    required this.packageLabel,
    required this.version,
    required this.steps,
    this.errorCode,
  }) : log = [
         JobLogEntry(
           stage: JobStage.queued,
           message: 'Job queued (install) — simulated in Phase 1',
         ),
       ],
       stage = JobStage.queued;

  final String id;
  final String packageId;
  final String packageLabel;
  final String version;
  final List<_SimStep> steps;
  int index = 0;
  bool cancelRequested = false;
  bool finished = false;
  Timer? timer;
  JobStage stage;
  final List<JobLogEntry> log;
  String? errorCode;
}

/// Deterministic simulated job engine. Stage order, progress rules
/// (measurable only in DOWNLOADING/EXTRACTING), the rollback chain
/// (terminal → ROLLING_BACK → ROLLED_BACK), and cancellation semantics all
/// mirror `crates/devkit-jobs`.
class FakeJobsRepository implements JobsRepository {
  /// Optional hook invoked when a job reaches COMPLETED: the Phase 1
  /// provider wiring uses it to register the managed install in the
  /// installed registry (in the real engine this happens inside the same
  /// Rust process, so the bridge needs no equivalent).
  FakeJobsRepository({this.onInstallCompleted}) {
    _seedHistory();
  }

  final Future<void> Function(
    String packageId,
    String packageLabel,
    String version,
  )?
  onInstallCompleted;

  final List<_FakeJob> _jobs = [];
  final _changes = StreamController<List<JobInfo>>.broadcast();
  int _counter = 0;
  static const _tick = Duration(milliseconds: 250);

  List<JobInfo> _snapshot() => [
    // Active jobs first, then history (newest first).
    ..._jobs.where((j) => !j.finished).map(_toInfo),
    ..._jobs.where((j) => j.finished).map(_toInfo),
  ];

  JobInfo _toInfo(_FakeJob job) => JobInfo(
    id: job.id,
    packageId: job.packageId,
    packageLabel: job.packageLabel,
    version: job.version,
    stage: job.stage,
    log: List.unmodifiable(job.log),
    errorCode: job.errorCode,
  );

  void _emit() {
    if (!_changes.isClosed) _changes.add(_snapshot());
  }

  /// Full simulated plan (the Dart twin of `SimulatedPlan::install`).
  static List<_SimStep> _installSteps(String packageLabel, String version) {
    return [
      _SimStep(
        JobStage.resolving,
        null,
        'Resolving $packageLabel $version (simulated)',
      ),
      const _SimStep(
        JobStage.preparing,
        null,
        'Preparing staging directory (simulated)',
      ),
      const _SimStep(
        JobStage.downloading,
        0.25,
        'Simulating download chunk 1/4',
      ),
      const _SimStep(
        JobStage.downloading,
        0.5,
        'Simulating download chunk 2/4',
      ),
      const _SimStep(
        JobStage.downloading,
        0.75,
        'Simulating download chunk 3/4',
      ),
      const _SimStep(
        JobStage.downloading,
        1.0,
        'Simulating download chunk 4/4',
      ),
      const _SimStep(JobStage.verifying, null, 'Verifying SHA-256 (simulated)'),
      const _SimStep(
        JobStage.extracting,
        0.33,
        'Simulating archive extraction 1/3',
      ),
      const _SimStep(
        JobStage.extracting,
        0.67,
        'Simulating archive extraction 2/3',
      ),
      const _SimStep(
        JobStage.extracting,
        1.0,
        'Simulating archive extraction 3/3',
      ),
      const _SimStep(
        JobStage.configuring,
        null,
        'Writing install receipt (simulated)',
      ),
      const _SimStep(
        JobStage.validating,
        null,
        'Validating installation (simulated)',
      ),
      const _SimStep(
        JobStage.completed,
        null,
        'Installation complete (simulated) — registered in Installed; '
        'PATH integration arrives with real installation (Phase 3)',
      ),
    ];
  }

  void _seedHistory() {
    // One completed and one failed job so the history section is populated
    // (DESIGN.md §4: "Completed and failed history").
    final completed = _FakeJob(
      id: 'sim-history-1',
      packageId: 'devkit.utility.git',
      packageLabel: 'Git',
      version: '2.50.1',
      steps: const [],
      errorCode: null,
    );
    completed
      ..stage = JobStage.completed
      ..finished = true
      // The constructor already appended the QUEUED entry.
      ..log.addAll(const [
        JobLogEntry(
          stage: JobStage.completed,
          message:
              'Installation complete (simulated) — registered in '
              'Installed; PATH integration arrives with real '
              'installation (Phase 3)',
        ),
      ]);
    final failed = _FakeJob(
      id: 'sim-history-2',
      packageId: 'devkit.lang.python',
      packageLabel: 'Python',
      version: '3.13.1',
      steps: const [],
      errorCode: 'INTEGRITY_CHECK_FAILED',
    );
    failed
      ..stage = JobStage.failed
      ..finished = true
      // The constructor already appended the QUEUED entry.
      ..log.addAll(const [
        JobLogEntry(
          stage: JobStage.downloading,
          progress: 1.0,
          message: 'Simulating download chunk 4/4',
        ),
        JobLogEntry(
          stage: JobStage.failed,
          message:
              'Checksum mismatch — artifact discarded, never extracted '
              '(simulated)',
          errorCode: 'INTEGRITY_CHECK_FAILED',
        ),
        JobLogEntry(
          stage: JobStage.rollingBack,
          message: 'Removing partial artifacts from staging (simulated)',
        ),
        JobLogEntry(
          stage: JobStage.rolledBack,
          message: 'Managed root restored to pre-install state (simulated)',
        ),
      ]);
    _jobs.addAll([completed, failed]);
  }

  @override
  Future<List<JobInfo>> jobs() async => _snapshot();

  @override
  Stream<List<JobInfo>> watchJobs() async* {
    yield _snapshot();
    yield* _changes.stream;
  }

  @override
  Future<String> startInstall({
    required String packageId,
    required String packageLabel,
    required String version,
  }) async {
    _counter += 1;
    final job = _FakeJob(
      id: 'sim-job-$_counter',
      packageId: packageId,
      packageLabel: packageLabel,
      version: version,
      steps: _installSteps(packageLabel, version),
    );
    _jobs.insert(0, job);
    job.timer = Timer.periodic(_tick, (_) => _advance(job));
    _emit();
    return job.id;
  }

  void _advance(_FakeJob job) {
    if (job.finished) return;

    // Cooperative cancellation: honored at the next step boundary.
    if (job.cancelRequested) {
      _rollback(job, JobStage.cancelled, 'Cancelled at a safe boundary');
      return;
    }

    if (job.index >= job.steps.length) {
      job.timer?.cancel();
      job.finished = true;
      _emit();
      return;
    }

    final step = job.steps[job.index];
    job.index += 1;
    job.stage = step.stage;
    job.log.add(
      JobLogEntry(
        stage: step.stage,
        progress: step.progress,
        message: step.message,
      ),
    );
    if (job.stage.isTerminal) {
      job.timer?.cancel();
      job.finished = true;
      // Completed installs register in the installed registry (Phase 1
      // simulator wiring; the real engine records the receipt itself).
      if (job.stage == JobStage.completed && onInstallCompleted != null) {
        unawaited(
          onInstallCompleted!(job.packageId, job.packageLabel, job.version),
        );
      }
    }
    _emit();
  }

  void _rollback(_FakeJob job, JobStage terminal, String message) {
    job.stage = terminal;
    job.log.add(JobLogEntry(stage: terminal, message: message));
    // The rollback chain is recorded as further ticks so it is visible in
    // the UI log, matching the Rust runner's event order.
    job.log.add(
      const JobLogEntry(
        stage: JobStage.rollingBack,
        message: 'Removing partial artifacts from staging (simulated)',
      ),
    );
    job.log.add(
      const JobLogEntry(
        stage: JobStage.rolledBack,
        message: 'Managed root restored to pre-install state (simulated)',
      ),
    );
    job.stage = JobStage.rolledBack;
    job.timer?.cancel();
    job.finished = true;
    _emit();
  }

  @override
  Future<void> cancelJob(String jobId) async {
    for (final job in _jobs) {
      if (job.id == jobId && !job.finished) {
        job.cancelRequested = true;
        return;
      }
    }
    // Terminal states reject cancellation (bridge contract § Cancellation
    // semantics); the UI disables the control for them.
  }

  /// Test seam: advance one simulated tick manually (no real timers).
  void debugAdvanceOnce() {
    for (final job in _jobs.where((j) => !j.finished)) {
      _advance(job);
    }
  }
}
