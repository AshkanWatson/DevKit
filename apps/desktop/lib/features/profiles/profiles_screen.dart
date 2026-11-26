import 'dart:convert';

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
        data: (list) => ListView(
          padding: const EdgeInsets.all(DesignTokens.space5),
          children: [
            const _PersonalProfilesSection(),
            if (list.isEmpty)
              const EmptyState(
                title: 'No profiles yet',
                message:
                    'Developer Profiles group packages into ready-to-install '
                    'role setups. Catalog profiles will appear here.',
              )
            else
              for (final profile in list) _ProfileCard(profile: profile),
            const _ProjectPinningSection(),
          ],
        ),
      ),
    );
  }
}

/// Personal (user-created/edited) profiles with delete — P4-04. They are
/// persisted locally (origin = 'user') and survive catalog re-sync.
class _PersonalProfilesSection extends ConsumerWidget {
  const _PersonalProfilesSection();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final theme = Theme.of(context);
    final personal = ref.watch(personalProfilesProvider);
    return personal.when(
      loading: () => const SizedBox(height: 24),
      error: (_, _) => const SizedBox.shrink(),
      data: (list) {
        if (list.isEmpty) return const SizedBox.shrink();
        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Padding(
              padding: const EdgeInsets.only(bottom: DesignTokens.space2),
              child: Text('My profiles', style: theme.textTheme.titleMedium),
            ),
            for (final profile in list)
              ListTile(
                dense: true,
                contentPadding: EdgeInsets.zero,
                leading: const Icon(Icons.person_outline),
                title: Text(profile.name),
                subtitle: Text('${profile.packages.length} packages'),
                trailing: IconButton(
                  icon: const Icon(Icons.delete_outline),
                  tooltip: 'Delete personal profile',
                  onPressed: () async {
                    final ok = await ref
                        .read(catalogRepositoryProvider)
                        .deletePersonalProfile(profile.id);
                    if (ok) ref.invalidate(personalProfilesProvider);
                  },
                ),
              ),
            const Divider(),
          ],
        );
      },
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
    // Resolve against the engine first: concrete versions are authoritative
    // (never UI-supplied), and unresolvable optionals are already skipped.
    final plan = await ref
        .read(catalogRepositoryProvider)
        .resolveProfile(widget.profile.id);
    if (!mounted) return;
    final selectedIds = [for (final p in _plannedPackages) p.packageId];

    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text('Apply profile — ${widget.profile.name}'),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('Will install:', style: theme.textTheme.bodyMedium),
            const SizedBox(height: 8),
            for (final package in plan.packages)
              Text(
                '• ${package.packageId} ${package.version}'
                ' (${package.policy})',
                style: theme.textTheme.bodySmall,
              ),
            if (plan.skipped.isNotEmpty) ...[
              const SizedBox(height: 8),
              Text(
                'Skipped: ${plan.skipped.map((s) => s.packageId).join(', ')}',
                style: theme.textTheme.bodySmall?.copyWith(
                  color: theme.colorScheme.onSurfaceVariant,
                ),
              ),
            ],
            const SizedBox(height: 12),
            Text(
              'Installing does not switch the active version or change PATH. '
              'You can track each package in Downloads.',
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
            child: const Text('Apply'),
          ),
        ],
      ),
    );
    if (confirmed != true || !mounted) return;

    final result = await ref
        .read(jobsRepositoryProvider)
        .applyProfile(widget.profile.id, selectedIds);
    if (!mounted) return;
    await showDialog<void>(
      context: context,
      builder: (context) => _ProfileApplyProgressDialog(result: result),
    );
  }

  /// Saves the current edited plan (with optional deselections applied) as a
  /// personal profile. A fresh stable ID is derived from the name + a suffix
  /// so catalog profiles are never overwritten; the manifest is validated in
  /// Rust before it is persisted.
  Future<void> _saveAsPersonal() async {
    final nameController = TextEditingController(
      text: '${widget.profile.name} (personal)',
    );
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Save as personal profile'),
        content: TextField(
          controller: nameController,
          decoration: const InputDecoration(labelText: 'Profile name'),
          autofocus: true,
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: const Text('Save'),
          ),
        ],
      ),
    );
    if (confirmed != true || !mounted) return;
    final name = nameController.text.trim();
    if (name.isEmpty) return;

    final suffix = DateTime.now().microsecondsSinceEpoch.toString();
    final id = 'devkit.profile.personal.$suffix';
    final manifest = <String, dynamic>{
      'schema_version': 1,
      'id': id,
      'name': name,
      'description': widget.profile.description,
      'maintainer': 'user',
      'supported_platforms': widget.profile.platforms
          .map((p) => p.wire)
          .toList(),
      'packages': [
        for (final package in _plannedPackages)
          <String, dynamic>{
            'package_id': package.packageId,
            'required': package.required,
            'version_policy': package.policy.wire,
            if (package.version != null) 'version': package.version,
            if (package.channel != null) 'channel': package.channel,
            'reason': package.reason,
          },
      ],
    };
    try {
      await ref
          .read(catalogRepositoryProvider)
          .savePersonalProfile(jsonEncode(manifest));
      if (!mounted) return;
      ref.invalidate(personalProfilesProvider);
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text('Saved “$name” to My profiles')));
    } catch (error) {
      if (!mounted) return;
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text('Could not save profile: $error')));
    }
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
                const SizedBox(width: DesignTokens.space2),
                OutlinedButton(
                  onPressed: _saveAsPersonal,
                  child: const Text('Save as personal'),
                ),
                const SizedBox(width: DesignTokens.space2),
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

/// Project-level version pinning (P4-05, version-management.md §5). Paste a
/// project manifest to preview the version changes it suggests. Planning is a
/// pure read — nothing is applied. Applying a suggestion uses the existing
/// consent-gated install/activation flow (never automatic).
class _ProjectPinningSection extends ConsumerStatefulWidget {
  const _ProjectPinningSection();

  @override
  ConsumerState<_ProjectPinningSection> createState() =>
      _ProjectPinningSectionState();
}

class _ProjectPinningSectionState
    extends ConsumerState<_ProjectPinningSection> {
  final _controller = TextEditingController();
  ProjectPlan? _plan;
  String? _error;
  bool _expanded = false;

  static const _sampleProject = '''
{
  "schema_version": 1,
  "name": "My Project",
  "packages": [
    {"package_id": "devkit.lang.python", "required": true, "version_policy": "exact", "version": "3.12.7"},
    {"package_id": "devkit.utility.git", "required": false, "version_policy": "stable"}
  ]
}
''';

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  Future<void> _preview() async {
    setState(() {
      _error = null;
      _plan = null;
    });
    try {
      final plan = await ref
          .read(catalogRepositoryProvider)
          .planProject(
            _controller.text.trim().isEmpty ? _sampleProject : _controller.text,
          );
      if (mounted) setState(() => _plan = plan);
    } catch (error) {
      if (mounted) setState(() => _error = '$error');
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Card(
      elevation: 0,
      margin: const EdgeInsets.only(bottom: DesignTokens.space4),
      child: Padding(
        padding: const EdgeInsets.all(DesignTokens.space4),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            InkWell(
              onTap: () => setState(() => _expanded = !_expanded),
              child: Row(
                children: [
                  const Icon(Icons.folder_open_outlined),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Text(
                      'Project version pins',
                      style: theme.textTheme.titleMedium,
                    ),
                  ),
                  Icon(_expanded ? Icons.expand_less : Icons.expand_more),
                ],
              ),
            ),
            if (_expanded) ...[
              const SizedBox(height: 4),
              Text(
                'Preview the versions a project suggests. Nothing is applied '
                'automatically — changes use the same consent flow as activation.',
                style: theme.textTheme.bodySmall?.copyWith(
                  color: theme.colorScheme.onSurfaceVariant,
                ),
              ),
              const SizedBox(height: 8),
              TextField(
                controller: _controller,
                maxLines: 4,
                minLines: 2,
                decoration: const InputDecoration(
                  labelText: 'Project manifest JSON (blank = sample)',
                  border: OutlineInputBorder(),
                ),
              ),
              const SizedBox(height: 8),
              FilledButton.icon(
                onPressed: _preview,
                icon: const Icon(Icons.visibility_outlined),
                label: const Text('Preview changes'),
              ),
              if (_error != null)
                Padding(
                  padding: const EdgeInsets.only(top: 8),
                  child: Text(
                    'Could not preview: $_error',
                    style: TextStyle(color: theme.colorScheme.error),
                  ),
                ),
              if (_plan != null) ...[
                const SizedBox(height: 12),
                Text(
                  'Suggested for ${_plan!.projectName}',
                  style: theme.textTheme.titleSmall,
                ),
                for (final pin in _plan!.pins)
                  ListTile(
                    dense: true,
                    contentPadding: EdgeInsets.zero,
                    title: Text('${pin.packageId} → ${pin.version}'),
                    subtitle: Text(
                      '${pin.source.label}'
                      '${pin.currentActive != null ? ' · current ${pin.currentActive}' : ''}'
                      '${pin.changeNeeded ? ' · change needed' : ''}',
                    ),
                  ),
                if (_plan!.unresolved.isNotEmpty)
                  Text(
                    'Unresolved: ${_plan!.unresolved.join(', ')}',
                    style: theme.textTheme.bodySmall,
                  ),
                if (_plan!.changes.isEmpty)
                  Text(
                    'No changes needed — already on the pinned versions.',
                    style: theme.textTheme.bodySmall,
                  ),
              ],
            ],
          ],
        ),
      ),
    );
  }
}

/// Watches the enqueued jobs and shows per-package progress plus an
/// installed/skipped summary once every job reaches a terminal stage
/// (DESIGN.md journey B steps 7–8). No fake percentages: indeterminate
/// stages are labeled, and only real job progress is shown as a bar.
class _ProfileApplyProgressDialog extends ConsumerWidget {
  const _ProfileApplyProgressDialog({required this.result});

  final ProfileApplyResult result;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final theme = Theme.of(context);
    final jobsAsync = ref.watch(jobsProvider);
    final skippedLabel = result.skipped.isEmpty
        ? 'none'
        : result.skipped.map((s) => s.packageId).join(', ');

    return AlertDialog(
      title: const Text('Applying profile'),
      content: SizedBox(
        width: 360,
        child: jobsAsync.when(
          loading: () => const Center(child: CircularProgressIndicator()),
          error: (error, _) => Text('Could not load job progress: $error'),
          data: (jobs) {
            final byId = {for (final job in jobs) job.id: job};
            final done = result.allJobsTerminal(jobs);
            return Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                for (final apply in result.jobs)
                  Padding(
                    padding: const EdgeInsets.symmetric(vertical: 4),
                    child: Row(
                      children: [
                        Icon(
                          byId[apply.jobId]?.stage.isTerminal == true
                              ? Icons.check_circle_outline
                              : Icons.hourglass_empty,
                          size: 18,
                        ),
                        const SizedBox(width: 8),
                        Expanded(
                          child: Text(
                            '${apply.packageId} ${apply.version} — '
                            '${byId[apply.jobId]?.stage.label ?? 'Queued'}',
                            style: theme.textTheme.bodySmall,
                          ),
                        ),
                      ],
                    ),
                  ),
                const Divider(),
                if (done)
                  Text(
                    'Installed ${result.completedCount(jobs)} of '
                    '${result.jobs.length} · skipped: $skippedLabel',
                    style: theme.textTheme.bodyMedium,
                  )
                else
                  Text(
                    'Installing in the background — you can close this '
                    'dialog and follow progress in Downloads.',
                    style: theme.textTheme.bodySmall,
                  ),
              ],
            );
          },
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('Close'),
        ),
      ],
    );
  }
}
