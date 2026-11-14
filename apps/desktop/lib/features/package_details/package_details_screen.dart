import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';

import '../../app/theme/design_tokens.dart';
import '../../core/domain/domain_models.dart';
import '../../core/providers/repository_providers.dart';
import '../../core/widgets/common_widgets.dart';
import '../../core/widgets/empty_state.dart';
import '../../core/widgets/package_card.dart';

/// Package details (DESIGN.md §4): header, version selector with policy,
/// trust status, platform compatibility, artifacts, dependencies, and the
/// install plan review before starting a (simulated) install.
class PackageDetailsScreen extends ConsumerWidget {
  const PackageDetailsScreen({super.key, required this.packageId});

  final String packageId;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final details = ref.watch(packageDetailsProvider(packageId));

    return details.when(
      loading: () => const Center(child: CircularProgressIndicator()),
      error: (error, _) => ErrorState(
        title: 'Package could not be loaded',
        error: error,
        onRetry: () => ref.invalidate(packageDetailsProvider(packageId)),
      ),
      data: (package) {
        if (package == null) {
          return EmptyState(
            title: 'Package not found',
            message: 'The catalog does not contain "$packageId".',
            actionLabel: 'Back to Explore',
            onAction: () => context.go('/explore'),
          );
        }
        return _DetailsBody(package: package);
      },
    );
  }
}

class _DetailsBody extends ConsumerStatefulWidget {
  const _DetailsBody({required this.package});

  final CatalogPackage package;

  @override
  ConsumerState<_DetailsBody> createState() => _DetailsBodyState();
}

class _DetailsBodyState extends ConsumerState<_DetailsBody> {
  VersionPolicyName _policy = VersionPolicyName.stable;
  String _exactVersion = '';

  CatalogPackage get package => widget.package;

  @override
  void initState() {
    super.initState();
    _policy = package.defaultPolicy;
    _exactVersion =
        package.newestStable?.version ?? package.versions.first.version;
  }

  /// The version the current policy resolves to (preview only).
  String _resolveVersion() {
    switch (_policy) {
      case VersionPolicyName.exact:
        return _exactVersion;
      case VersionPolicyName.latest:
        return package.versions.first.version;
      case VersionPolicyName.stable:
        return package.newestStable?.version ?? package.versions.first.version;
      case VersionPolicyName.channel:
        return package.newestStable?.version ?? package.versions.first.version;
    }
  }

  Future<void> _startInstall() async {
    final version = _resolveVersion();
    final versionInfo = package.versions.firstWhere(
      (v) => v.version == version,
      orElse: () => package.versions.first,
    );
    final artifact = versionInfo.artifactFor(PlatformTarget.current);
    final confirmed = await showInstallPlanReview(
      context,
      packageName: package.name,
      version: version,
      policyLabel: _policy.label,
      sourceDomain: artifact?.sourceDomain ?? '—',
      sizeLabel: artifact?.sizeLabel ?? 'unknown',
      note: package.license.isEmpty
          ? 'License: not recorded'
          : 'License: ${package.license}',
    );
    if (!confirmed || !mounted) return;
    final jobId = await ref
        .read(jobsRepositoryProvider)
        .startInstall(
          packageId: package.id,
          packageLabel: package.name,
          version: version,
        );
    if (!mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(content: Text('Install job started: $jobId (simulated)')),
    );
    context.go('/downloads');
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final version = _resolveVersion();
    final versionInfo = package.versions.firstWhere(
      (v) => v.version == version,
      orElse: () => package.versions.first,
    );
    final artifact = versionInfo.artifactFor(PlatformTarget.current);
    final supported = package.supportsPlatform(PlatformTarget.current);

    return Scaffold(
      body: ListView(
        padding: const EdgeInsets.all(DesignTokens.space5),
        children: [
          // PackageHeader (DESIGN.md §7).
          Row(
            children: [
              IconButton(
                tooltip: 'Back to Explore',
                onPressed: () => context.go('/explore'),
                icon: const Icon(Icons.arrow_back),
              ),
              const SizedBox(width: DesignTokens.space2),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(package.name, style: theme.textTheme.headlineSmall),
                    Text(
                      '${package.publisher} · ${package.category.label} · '
                      '${package.id}',
                      style: theme.textTheme.bodySmall?.copyWith(
                        color: theme.colorScheme.onSurfaceVariant,
                      ),
                    ),
                  ],
                ),
              ),
            ],
          ),
          const SizedBox(height: DesignTokens.space4),
          Text(package.description, style: theme.textTheme.bodyMedium),
          const SizedBox(height: DesignTokens.space2),
          TrustStatus(sourceDomain: artifact?.sourceDomain ?? '—'),
          const SizedBox(height: DesignTokens.space4),

          // Unsupported platform warning (DESIGN.md §4: clear warnings).
          if (!supported) ...[
            _WarningBanner(
              message:
                  'This package does not list ${PlatformTarget.current.wire} '
                  'as a supported platform. Installation is disabled until a '
                  'compatible artifact is published.',
            ),
            const SizedBox(height: DesignTokens.space4),
          ],

          // Version selector (policy + exact version).
          Card(
            elevation: 0,
            shape: RoundedRectangleBorder(
              borderRadius: BorderRadius.circular(12),
              side: BorderSide(color: theme.colorScheme.outlineVariant),
            ),
            child: Padding(
              padding: const EdgeInsets.all(DesignTokens.space4),
              child: VersionSelector(
                policy: _policy,
                onPolicyChanged: (policy) => setState(() => _policy = policy),
                exactVersion: _exactVersion,
                versions: package.versions,
                onExactVersionChanged: (value) =>
                    setState(() => _exactVersion = value),
                resolvedVersion: version,
              ),
            ),
          ),
          const SizedBox(height: DesignTokens.space4),

          // Compatibility + artifact info for the selected version.
          _SectionTitle('Compatibility'),
          PlatformCompatibility(
            platforms: package.platforms,
            highlightUnsupported: !supported,
          ),
          const SizedBox(height: DesignTokens.space4),
          _SectionTitle('Selected version ${versionInfo.version}'),
          _ArtifactTable(artifact: artifact),
          const SizedBox(height: DesignTokens.space4),

          // Dependencies and conflicts.
          if (versionInfo.dependencies.isNotEmpty) ...[
            _SectionTitle('Dependencies'),
            for (final dependency in versionInfo.dependencies)
              ListTile(
                dense: true,
                leading: Icon(
                  dependency.required ? Icons.link : Icons.link_outlined,
                  size: 18,
                ),
                title: Text(dependency.packageId),
                subtitle: Text(
                  '${dependency.required ? 'required' : 'optional'} · '
                  '${dependency.policy.wire}'
                  '${dependency.version != null ? ' ${dependency.version}' : ''}'
                  '${dependency.channel != null ? ' (${dependency.channel})' : ''}',
                ),
              ),
            const SizedBox(height: DesignTokens.space4),
          ],

          // Primary action.
          FilledButton.icon(
            onPressed: supported ? _startInstall : null,
            icon: const Icon(Icons.download_outlined),
            label: Text(supported ? 'Install $version' : 'Install unavailable'),
          ),
          if (!supported) ...[
            const SizedBox(height: DesignTokens.space2),
            Text(
              'Disabled: no artifact for ${PlatformTarget.current.wire} in '
              'the selected version.',
              style: theme.textTheme.bodySmall?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ),
          ],
          const SizedBox(height: DesignTokens.space5),
        ],
      ),
    );
  }
}

class _SectionTitle extends StatelessWidget {
  const _SectionTitle(this.title);

  final String title;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(bottom: DesignTokens.space2),
      child: Text(title, style: Theme.of(context).textTheme.titleSmall),
    );
  }
}

class _WarningBanner extends StatelessWidget {
  const _WarningBanner({required this.message});

  final String message;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Container(
      padding: const EdgeInsets.all(DesignTokens.space3),
      decoration: BoxDecoration(
        color: theme.colorScheme.errorContainer,
        borderRadius: BorderRadius.circular(12),
      ),
      child: Row(
        children: [
          Icon(Icons.warning_amber_rounded, color: theme.colorScheme.error),
          const SizedBox(width: DesignTokens.space3),
          Expanded(child: Text(message, style: theme.textTheme.bodyMedium)),
        ],
      ),
    );
  }
}

/// Artifact facts for the selected version + current platform: source
/// domain, checksum record status, install size, installer kind.
class _ArtifactTable extends StatelessWidget {
  const _ArtifactTable({required this.artifact});

  final ArtifactInfo? artifact;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    if (artifact == null) {
      return Text(
        'No artifact for ${PlatformTarget.current.wire} in this version.',
        style: theme.textTheme.bodyMedium?.copyWith(
          color: theme.colorScheme.onSurfaceVariant,
        ),
      );
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _FactRow(label: 'Installer', value: artifact!.installer.label),
        _FactRow(label: 'Source domain', value: artifact!.sourceDomain),
        _FactRow(
          label: 'Checksum',
          value: artifact!.hasChecksum
              ? 'SHA-256 recorded (verified at download time)'
              : 'Not recorded',
        ),
        _FactRow(label: 'Download size', value: artifact!.sizeLabel),
      ],
    );
  }
}

class _FactRow extends StatelessWidget {
  const _FactRow({required this.label, required this.value});

  final String label;
  final String value;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 2),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          SizedBox(
            width: 140,
            child: Text(
              label,
              style: theme.textTheme.bodySmall?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ),
          ),
          Expanded(child: Text(value, style: theme.textTheme.bodyMedium)),
        ],
      ),
    );
  }
}
