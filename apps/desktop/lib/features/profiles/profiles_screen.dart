import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:material_ui/material_ui.dart';

import '../../app/theme/design_tokens.dart';
import '../../core/domain/domain_models.dart';
import '../../core/providers/repository_providers.dart';
import '../../core/widgets/common_widgets.dart';
import '../../core/widgets/empty_state.dart';

/// Profiles destination (DESIGN.md §4): profile cards with role, package
/// count, supported platforms; an editable preview showing required/
/// optional packages and per-package policies, plus dependency additions
/// before applying.
class ProfilesScreen extends ConsumerWidget {
  const ProfilesScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final profiles = ref.watch(catalogProfilesProvider);

    return Scaffold(
      body: profiles.when(
        loading: () => const Center(child: CircularProgressIndicator()),
        error: (error, _) => ErrorState(
          title: 'Profiles could not be loaded',
          error: error,
          onRetry: () => ref.invalidate(catalogProfilesProvider),
        ),
        data: (list) => list.isEmpty
            ? const EmptyState(
                title: 'No profiles yet',
                message:
                    'Developer Profiles group packages into ready-to-install '
                    'role setups. Catalog profiles will appear here.',
              )
            : ListView(
                padding: const EdgeInsets.all(DesignTokens.space5),
                children: [
                  for (final profile in list) _ProfileCard(profile: profile),
                ],
              ),
      ),
    );
  }
}

class _ProfileCard extends ConsumerStatefulWidget {
  const _ProfileCard({required this.profile});

  final ProfileInfo profile;

  @override
  ConsumerState<_ProfileCard> createState() => _ProfileCardState();
}

class _ProfileCardState extends ConsumerState<_ProfileCard> {
  /// Editable preview state: optional packages can be deselected before
  /// applying (DESIGN.md §4: "editable profile preview").
  late final Set<String> _excluded =
      {}; // package IDs the user removed from the plan

  List<ProfilePackageRef> get _plannedPackages => [
    for (final package in widget.profile.packages)
      if (!_excluded.contains(package.packageId)) package,
  ];

  Future<void> _applyProfile() async {
    final theme = Theme.of(context);
    // Dependency additions before applying (DESIGN.md §4).
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text('Apply profile — ${widget.profile.name}'),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              'Will install (simulated):',
              style: theme.textTheme.bodyMedium,
            ),
            const SizedBox(height: 8),
            for (final package in _plannedPackages)
              Text(
                '• ${package.packageId}'
                ' (${package.policy.wire}'
                '${package.version != null ? ' ${package.version}' : ''})',
                style: theme.textTheme.bodySmall,
              ),
            const SizedBox(height: 12),
            Text(
              'Phase 1 simulates profile application; nothing is downloaded '
              'or installed. Saving personal profiles arrives in Phase 4.',
              style: theme.textTheme.bodySmall?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: const Text('Apply (simulated)'),
          ),
        ],
      ),
    );
    if (confirmed != true || !mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(
      const SnackBar(
        content: Text(
          'Profile application is simulated in Phase 1 — no '
          'packages were installed.',
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final profile = widget.profile;

    return Card(
      elevation: 0,
      margin: const EdgeInsets.only(bottom: DesignTokens.space4),
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(12),
        side: BorderSide(color: theme.colorScheme.outlineVariant),
      ),
      child: Padding(
        padding: const EdgeInsets.all(DesignTokens.space4),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(profile.name, style: theme.textTheme.titleMedium),
                      const SizedBox(height: DesignTokens.space1),
                      Text(
                        profile.description,
                        style: theme.textTheme.bodyMedium?.copyWith(
                          color: theme.colorScheme.onSurfaceVariant,
                        ),
                      ),
                    ],
                  ),
                ),
                const SizedBox(width: DesignTokens.space3),
                FilledButton(
                  onPressed: _plannedPackages.isEmpty ? null : _applyProfile,
                  child: const Text('Apply'),
                ),
              ],
            ),
            const SizedBox(height: DesignTokens.space3),
            Text(
              '${_plannedPackages.length} of ${profile.packages.length} '
              'packages · supports '
              '${profile.platforms.map((p) => p.wire).join(', ')}',
              style: theme.textTheme.bodySmall,
            ),
            const Divider(height: DesignTokens.space5),
            // Editable preview: required packages are fixed; optional ones
            // can be removed from the plan.
            for (final package in profile.packages)
              SwitchListTile(
                dense: true,
                contentPadding: EdgeInsets.zero,
                value: !_excluded.contains(package.packageId),
                onChanged: package.required
                    ? null // required packages cannot be deselected
                    : (included) => setState(() {
                        if (included) {
                          _excluded.remove(package.packageId);
                        } else {
                          _excluded.add(package.packageId);
                        }
                      }),
                title: Text(
                  '${package.packageId}'
                  '${package.required ? '  ·  required' : '  ·  optional'}',
                  style: theme.textTheme.bodyMedium,
                ),
                subtitle: Text(
                  '${package.policy.wire}'
                  '${package.version != null ? ' ${package.version}' : ''}'
                  '${package.channel != null ? ' · channel ${package.channel}' : ''}'
                  ' — ${package.reason}',
                  style: theme.textTheme.bodySmall,
                ),
              ),
            if (_plannedPackages.isEmpty)
              Padding(
                padding: const EdgeInsets.only(top: DesignTokens.space2),
                child: Text(
                  'Nothing to install: all optional packages are removed and '
                  'required packages cannot be deselected.',
                  style: theme.textTheme.bodySmall,
                ),
              ),
          ],
        ),
      ),
    );
  }
}
