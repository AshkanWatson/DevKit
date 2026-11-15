/// Domain models for the catalog, installed packages, and jobs.
///
/// These mirror the documented contracts (schemas/package.schema.json,
/// schemas/profile.schema.json, docs/architecture/installation-lifecycle.md
/// §3, docs/ffi/bridge-contract.md § Event model) so the Phase 1 fake
/// repositories and the later Rust FFI implementation speak the same shape
/// (DESIGN.md §11: screens must not need redesign when FFI lands).
library;

/// Package categories (schemas/package.schema.json `category`).
enum PackageCategory {
  language('language', 'Languages'),
  framework('framework', 'Frameworks'),
  sdk('sdk', 'SDKs'),
  ide('ide', 'IDEs'),
  buildTool('build_tool', 'Build Tools'),
  database('database', 'Databases'),
  utility('utility', 'Utilities'),
  aiTool('ai_tool', 'AI Tools'),
  other('other', 'Other');

  const PackageCategory(this.wire, this.label);

  final String wire;
  final String label;

  static PackageCategory fromWire(String wire) => PackageCategory.values
      .firstWhere((c) => c.wire == wire, orElse: () => PackageCategory.other);
}

/// Supported platform/architecture pairs (ADR 0004).
enum PlatformTarget {
  windowsX64('windows-x64', 'Windows', 'x64'),
  windowsArm64('windows-arm64', 'Windows', 'ARM64'),
  macosX64('macos-x64', 'macOS', 'x64'),
  macosArm64('macos-arm64', 'macOS', 'ARM64'),
  linuxX64('linux-x64', 'Linux', 'x64'),
  linuxArm64('linux-arm64', 'Linux', 'ARM64');

  const PlatformTarget(this.wire, this.os, this.arch);

  final String wire;
  final String os;
  final String arch;

  static PlatformTarget fromWire(String wire) => PlatformTarget.values
      .firstWhere((p) => p.wire == wire, orElse: () => PlatformTarget.other);

  static const PlatformTarget other = windowsX64;

  /// Platform this build runs on. Phase 1 ships Windows-first; dynamic
  /// detection from the bridge lands with the FFI repositories.
  static const PlatformTarget current = windowsX64;
}

/// Release channels (schemas `channel`).
enum ReleaseChannel {
  stable('stable'),
  beta('beta'),
  dev('dev'),
  nightly('nightly');

  const ReleaseChannel(this.wire);

  final String wire;

  static ReleaseChannel fromWire(String wire) => ReleaseChannel.values
      .firstWhere((c) => c.wire == wire, orElse: () => ReleaseChannel.stable);

  bool get isPrerelease => this != ReleaseChannel.stable;
}

/// How an artifact installs (schemas `installer`).
enum InstallerKind {
  archive('archive'),
  exe('exe'),
  msi('msi'),
  dmg('dmg'),
  pkg('pkg'),
  appimage('appimage'),
  externalPackageManager('external_package_manager');

  const InstallerKind(this.wire);

  final String wire;

  static InstallerKind fromWire(String wire) => InstallerKind.values.firstWhere(
    (k) => k.wire == wire,
    orElse: () => InstallerKind.archive,
  );

  String get label => switch (this) {
    InstallerKind.externalPackageManager => 'External package manager',
    _ => wire,
  };
}

/// Manifest version-policy names (schemas `default_version_policy`).
enum VersionPolicyName {
  stable('stable'),
  latest('latest'),
  exact('exact'),
  channel('channel');

  const VersionPolicyName(this.wire);

  final String wire;

  static VersionPolicyName fromWire(String wire) =>
      VersionPolicyName.values.firstWhere(
        (p) => p.wire == wire,
        orElse: () => VersionPolicyName.stable,
      );

  String get label => switch (this) {
    VersionPolicyName.stable => 'Stable (newest non-prerelease)',
    VersionPolicyName.latest => 'Latest (including prereleases)',
    VersionPolicyName.exact => 'Exact version',
    VersionPolicyName.channel => 'Newest in channel',
  };
}

/// One downloadable artifact of a version, reduced to what the UI shows:
/// the SOURCE DOMAIN only (never the full URL — messages and logs must not
/// leak private query parameters) and the checksum RECORD status. A
/// recorded checksum is verified at download time, never before.
class ArtifactInfo {
  const ArtifactInfo({
    required this.os,
    required this.architecture,
    required this.installer,
    required this.sourceDomain,
    required this.sizeBytes,
    required this.hasChecksum,
  });

  final String os;
  final String architecture;
  final InstallerKind installer;
  final String sourceDomain;
  final int sizeBytes;
  final bool hasChecksum;

  String get sizeLabel {
    if (sizeBytes >= 1000000000) {
      return '${(sizeBytes / 1000000000).toStringAsFixed(1)} GB';
    }
    if (sizeBytes >= 1000000) {
      return '${(sizeBytes / 1000000).toStringAsFixed(1)} MB';
    }
    if (sizeBytes >= 1000) return '${(sizeBytes / 1000).toStringAsFixed(0)} kB';
    return '$sizeBytes B';
  }
}

/// A declared dependency of a version.
class DependencyRef {
  const DependencyRef({
    required this.packageId,
    required this.required,
    required this.policy,
    this.version,
    this.channel,
  });

  final String packageId;
  final bool required;
  final VersionPolicyName policy;
  final String? version;
  final String? channel;
}

/// One release of a package.
class PackageVersionInfo {
  const PackageVersionInfo({
    required this.version,
    required this.channel,
    this.releaseDate,
    this.releaseNotesUrl,
    this.dependencies = const [],
    this.artifacts = const [],
  });

  final String version;
  final ReleaseChannel channel;
  final String? releaseDate;
  final String? releaseNotesUrl;
  final List<DependencyRef> dependencies;
  final List<ArtifactInfo> artifacts;

  bool get isPrerelease => channel.isPrerelease;

  ArtifactInfo? artifactFor(PlatformTarget platform) {
    for (final artifact in artifacts) {
      if (artifact.os == platform.os.toLowerCase() &&
          artifact.architecture == platform.arch.toLowerCase()) {
        return artifact;
      }
    }
    return null;
  }
}

/// A catalog package (schemas/package.schema.json).
class CatalogPackage {
  const CatalogPackage({
    required this.id,
    required this.name,
    required this.description,
    required this.publisher,
    required this.homepage,
    required this.category,
    required this.platforms,
    required this.defaultPolicy,
    required this.versions,
    required this.license,
    required this.updatedAt,
  });

  final String id;
  final String name;
  final String description;
  final String publisher;
  final String homepage;
  final PackageCategory category;
  final List<PlatformTarget> platforms;
  final VersionPolicyName defaultPolicy;
  final List<PackageVersionInfo> versions;
  final String license;
  final String updatedAt;

  bool supportsPlatform(PlatformTarget platform) =>
      platforms.contains(platform);

  /// Newest stable version (null when the package only ships prereleases).
  PackageVersionInfo? get newestStable {
    for (final version in versions) {
      if (!version.isPrerelease) return version;
    }
    return null;
  }
}

/// A package selection inside a profile.
class ProfilePackageRef {
  const ProfilePackageRef({
    required this.packageId,
    required this.required,
    required this.policy,
    this.version,
    this.channel,
    this.reason = '',
  });

  final String packageId;
  final bool required;
  final VersionPolicyName policy;
  final String? version;
  final String? channel;
  final String reason;
}

/// A developer profile (schemas/profile.schema.json).
class ProfileInfo {
  const ProfileInfo({
    required this.id,
    required this.name,
    required this.description,
    required this.maintainer,
    required this.platforms,
    required this.packages,
  });

  final String id;
  final String name;
  final String description;
  final String maintainer;
  final List<PlatformTarget> platforms;
  final List<ProfilePackageRef> packages;
}

/// Installation status shown on the Installed screen ("mock version
/// states", DESIGN.md §11).
enum InstalledStatus {
  installed('Installed'),
  broken('Broken');

  const InstalledStatus(this.label);

  final String label;
}

/// Who owns an installation. Externally detected software is never
/// uninstallable through DevKit (HANDOFF.md §10).
enum InstallOwnership {
  devkitManaged('DevKit-managed'),
  externalDetected('Externally detected');

  const InstallOwnership(this.label);

  final String label;
}

class InstalledPackage {
  const InstalledPackage({
    required this.packageId,
    required this.name,
    required this.version,
    required this.status,
    required this.isActive,
    required this.ownership,
    required this.locationLabel,
  });

  final String packageId;
  final String name;
  final String version;
  final InstalledStatus status;
  final bool isActive;
  final InstallOwnership ownership;
  final String locationLabel;
}

/// What activating a version would change (P3-04, DESIGN.md journey C
/// step 4: "Show what activation changes"). Mirrors the Rust
/// `plan_activation` output; the consent dialog renders exactly this.
class ActivationPlan {
  const ActivationPlan({
    required this.shimFiles,
    required this.pathChange,
    required this.reversible,
    this.previousActiveVersion,
  });

  /// Managed-root-relative shim files that will be (re)written, e.g.
  /// `shims\git.cmd`. Empty for library packages.
  final List<String> shimFiles;

  /// What will happen to the user PATH, phrased for the consent dialog
  /// (appended once / already present / unavailable on this platform).
  final String pathChange;

  /// Whether activation can be reverted (the previous mapping and shim
  /// are kept for the operation's duration).
  final bool reversible;

  /// The version that is active now, and that a revert would restore.
  final String? previousActiveVersion;
}

/// Job stages: the documented state machine
/// (docs/architecture/installation-lifecycle.md §3). Wire forms match the
/// Rust `Stage` exactly so the FFI swap needs no screen changes.
enum JobStage {
  queued('QUEUED'),
  resolving('RESOLVING'),
  preparing('PREPARING'),
  downloading('DOWNLOADING'),
  verifying('VERIFYING'),
  extracting('EXTRACTING'),
  installing('INSTALLING'),
  configuring('CONFIGURING'),
  validating('VALIDATING'),
  completed('COMPLETED'),
  failed('FAILED'),
  cancelled('CANCELLED'),
  paused('PAUSED'),
  rollingBack('ROLLING_BACK'),
  rolledBack('ROLLED_BACK');

  const JobStage(this.wire);

  final String wire;

  static JobStage fromWire(String wire) => JobStage.values.firstWhere(
    (s) => s.wire == wire,
    orElse: () => JobStage.queued,
  );

  /// Terminal states reject further cancellation
  /// (bridge-contract.md § Cancellation semantics).
  bool get isTerminal =>
      this == JobStage.completed ||
      this == JobStage.failed ||
      this == JobStage.cancelled ||
      this == JobStage.rolledBack;

  /// A user-facing stage label.
  String get label => switch (this) {
    JobStage.queued => 'Queued',
    JobStage.resolving => 'Resolving version',
    JobStage.preparing => 'Preparing',
    JobStage.downloading => 'Downloading',
    JobStage.verifying => 'Verifying checksum',
    JobStage.extracting => 'Extracting',
    JobStage.installing => 'Installing',
    JobStage.configuring => 'Configuring',
    JobStage.validating => 'Validating',
    JobStage.completed => 'Completed',
    JobStage.failed => 'Failed',
    JobStage.cancelled => 'Cancelled',
    JobStage.paused => 'Paused',
    JobStage.rollingBack => 'Rolling back',
    JobStage.rolledBack => 'Rolled back',
  };
}

/// One entry of a job's structured log (bridge-contract.md § Event model).
class JobLogEntry {
  const JobLogEntry({
    required this.stage,
    required this.message,
    this.progress,
    this.errorCode,
  });

  final JobStage stage;
  final String message;

  /// 0.0..1.0 where measurable; null means indeterminate.
  final double? progress;
  final String? errorCode;
}

/// A job snapshot as rendered by the Downloads screen.
class JobInfo {
  const JobInfo({
    required this.id,
    required this.packageId,
    required this.packageLabel,
    required this.version,
    required this.stage,
    required this.log,
    this.errorCode,
  });

  final String id;
  final String packageId;
  final String packageLabel;
  final String version;

  /// Current stage (last log entry's stage is kept in sync).
  final JobStage stage;
  final List<JobLogEntry> log;
  final String? errorCode;

  /// Progress fraction ONLY where the stage can measure it; null means
  /// the UI must show an indeterminate stage — never a fake percentage
  /// (DESIGN.md §8).
  double? get progress {
    final current = log.where((e) => e.stage == stage).lastOrNull;
    return current?.progress;
  }

  bool get isActive => !stage.isTerminal;
  bool get canBeCancelled => !stage.isTerminal && stage != JobStage.rollingBack;
  bool get canBeRetried =>
      stage == JobStage.failed || stage == JobStage.rolledBack;
}
