import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:material_ui/material_ui.dart';

import '../../app/theme/design_tokens.dart';
import '../../core/domain/domain_models.dart';
import '../../core/providers/repository_providers.dart';
import '../../core/widgets/common_widgets.dart';
import '../../core/widgets/empty_state.dart';
import '../../core/widgets/global_search.dart';

/// Installed destination (DESIGN.md §4): search and filter, rows with
/// active version and status, distinguishing DevKit-managed installations
/// from externally detected software. Uninstall is destructive and
/// requires confirmation; Set Active only applies to managed installs.
class InstalledScreen extends ConsumerStatefulWidget {
  const InstalledScreen({super.key});

  @override
  ConsumerState<InstalledScreen> createState() => _InstalledScreenState();
}

class _InstalledScreenState extends ConsumerState<InstalledScreen> {
  String _query = '';

  @override
  Widget build(BuildContext context) {
    final installed = ref.watch(installedPackagesProvider);

    return Scaffold(
      body: installed.when(
        loading: () => const Center(child: CircularProgressIndicator()),
        error: (error, _) => ErrorState(
          title: 'Installed packages could not be loaded',
          error: error,
          onRetry: () => ref.invalidate(installedPackagesProvider),
        ),
        data: (packages) {
          final lowerQuery = _query.trim().toLowerCase();
          final filtered = [
            for (final package in packages)
              if (lowerQuery.isEmpty ||
                  package.name.toLowerCase().contains(lowerQuery) ||
                  package.packageId.toLowerCase().contains(lowerQuery))
                package,
          ];
          return Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Padding(
                padding: const EdgeInsets.fromLTRB(
                  DesignTokens.space5,
                  DesignTokens.space5,
                  DesignTokens.space5,
                  DesignTokens.space3,
                ),
                child: GlobalSearch(
                  value: _query,
                  onChanged: (value) => setState(() => _query = value),
                ),
              ),
              Expanded(
                child: filtered.isEmpty
                    ? const EmptyState(
                        title: 'Nothing installed matches',
                        message:
                            'No installed package matches the search. Browse '
                            'Explore to install packages.',
                      )
                    : ListView(
                        padding: const EdgeInsets.fromLTRB(
                          DesignTokens.space5,
                          0,
                          DesignTokens.space5,
                          DesignTokens.space5,
                        ),
                        children: [
                          for (final package in filtered)
                            _InstalledRow(package: package),
                        ],
                      ),
              ),
            ],
          );
        },
      ),
    );
  }
}

class _InstalledRow extends ConsumerWidget {
  const _InstalledRow({required this.package});

  final InstalledPackage package;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final theme = Theme.of(context);
    final managed = package.ownership == InstallOwnership.devkitManaged;
    final canSetActive =
        managed &&
        package.status == InstalledStatus.installed &&
        !package.isActive;

    return Card(
      elevation: 0,
      margin: const EdgeInsets.only(bottom: DesignTokens.space3),
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(12),
        side: BorderSide(color: theme.colorScheme.outlineVariant),
      ),
      child: Padding(
        padding: const EdgeInsets.all(DesignTokens.space3),
        child: Row(
          children: [
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    children: [
                      Flexible(
                        child: Text(
                          package.name,
                          style: theme.textTheme.titleMedium,
                          overflow: TextOverflow.ellipsis,
                        ),
                      ),
                      const SizedBox(width: DesignTokens.space2),
                      if (package.isActive)
                        Tooltip(
                          message: 'This is the version on PATH / launch env.',
                          child: Chip(
                            label: const Text('Active'),
                            labelStyle: theme.textTheme.labelSmall,
                            visualDensity: VisualDensity.compact,
                            backgroundColor: theme.colorScheme.primaryContainer,
                          ),
                        ),
                      if (package.status == InstalledStatus.broken)
                        Chip(
                          label: const Text('Broken'),
                          labelStyle: theme.textTheme.labelSmall,
                          visualDensity: VisualDensity.compact,
                          backgroundColor: theme.colorScheme.errorContainer,
                        ),
                    ],
                  ),
                  const SizedBox(height: DesignTokens.space1),
                  Text(
                    '${package.version} · ${package.ownership.label} · '
                    '${package.locationLabel}',
                    style: theme.textTheme.bodySmall?.copyWith(
                      color: theme.colorScheme.onSurfaceVariant,
                    ),
                  ),
                ],
              ),
            ),
            const SizedBox(width: DesignTokens.space2),
            // Actions (DESIGN.md §4): Set Active / Repair (stub) / Uninstall.
            if (managed) ...[
              TextButton(
                onPressed: canSetActive
                    ? () async {
                        await ref
                            .read(installedRepositoryProvider)
                            .setActiveVersion(
                              package.packageId,
                              package.version,
                            );
                        ref.invalidate(installedPackagesProvider);
                      }
                    : null,
                child: Text(
                  'Set Active',
                  style: canSetActive
                      ? null
                      : theme.textTheme.bodyMedium?.copyWith(
                          color: theme.colorScheme.onSurfaceVariant,
                        ),
                ),
              ),
              IconButton(
                tooltip: managed
                    ? 'Uninstall ${package.name} ${package.version}'
                    : 'Not available for externally detected software',
                onPressed: managed ? () => _uninstall(context, ref) : null,
                icon: const Icon(Icons.delete_outline),
              ),
            ] else ...[
              Tooltip(
                message:
                    'Externally detected software is owned outside DevKit; '
                    'uninstall it with its own installer.',
                child: const Icon(Icons.lock_outline),
              ),
            ],
          ],
        ),
      ),
    );
  }

  Future<void> _uninstall(BuildContext context, WidgetRef ref) async {
    // Destructive action → explicit confirmation (DESIGN.md §8).
    final confirmed = await confirmDestructive(
      context,
      title: 'Uninstall ${package.name} ${package.version}?',
      message:
          'This removes the DevKit-managed installation at '
          '${package.locationLabel}. The active version mapping is removed '
          'too. This action cannot be undone from DevKit.',
      confirmLabel: 'Uninstall',
    );
    if (!confirmed) return;
    await ref
        .read(installedRepositoryProvider)
        .uninstall(package.packageId, package.version);
    ref.invalidate(installedPackagesProvider);
  }
}
