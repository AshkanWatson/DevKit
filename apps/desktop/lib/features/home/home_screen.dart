import 'package:material_ui/material_ui.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../core/bridge/api.dart';
import '../../core/bridge/bridge_providers.dart';
import '../../app/theme/design_tokens.dart';
import '../../core/providers/repository_providers.dart';

/// Home destination (P1-02 milestone): shows the live Rust engine status —
/// the first real bridge round trip — plus the Phase 1 roadmap placeholder.
///
/// The full dashboard (installed overview, download progress) lands in
/// P1-06 once the local catalog and job manager exist (DESIGN.md §11).
class HomeScreen extends ConsumerWidget {
  const HomeScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final systemInfo = ref.watch(systemInfoProvider);

    return Scaffold(
      body: ListView(
        padding: const EdgeInsets.all(DesignTokens.space5),
        children: [
          systemInfo.when(
            loading: () => const _EngineStatusCard.loading(),
            error: (error, stackTrace) => _EngineStatusCard.error(error),
            data: (info) => _EngineStatusCard.data(info),
          ),
          const SizedBox(height: DesignTokens.space5),
          const _SummarySection(),
        ],
      ),
    );
  }
}

/// Home summary from the local fixture catalog and job center
/// (DESIGN.md §4/§11: compact summary of installed packages and active
/// jobs; empty state directs the user to Explore).
class _SummarySection extends ConsumerWidget {
  const _SummarySection();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final installed = ref.watch(installedPackagesProvider);
    final jobs = ref.watch(jobsProvider);

    final installedCount = installed.value?.length ?? 0;
    final activeJobs = jobs.value?.where((job) => job.isActive).length ?? 0;

    return Wrap(
      spacing: DesignTokens.space3,
      runSpacing: DesignTokens.space3,
      children: [
        _SummaryCard(
          title: '$installedCount',
          subtitle: 'Installed packages',
          icon: Icons.inventory_2_outlined,
          onTap: () => context.go('/installed'),
        ),
        _SummaryCard(
          title: '$activeJobs',
          subtitle: 'Active jobs',
          icon: Icons.downloading_outlined,
          onTap: () => context.go('/downloads'),
        ),
        _SummaryCard(
          title: 'Browse',
          subtitle: 'Explore the catalog',
          icon: Icons.explore_outlined,
          onTap: () => context.go('/explore'),
        ),
      ],
    );
  }
}

class _SummaryCard extends StatelessWidget {
  const _SummaryCard({
    required this.title,
    required this.subtitle,
    required this.icon,
    required this.onTap,
  });

  final String title;
  final String subtitle;
  final IconData icon;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Card(
      elevation: 0,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(12),
        side: BorderSide(color: theme.colorScheme.outlineVariant),
      ),
      child: InkWell(
        borderRadius: BorderRadius.circular(12),
        onTap: onTap,
        child: Padding(
          padding: const EdgeInsets.all(DesignTokens.space4),
          child: SizedBox(
            width: 180,
            child: Row(
              children: [
                Icon(icon, color: theme.colorScheme.primary),
                const SizedBox(width: DesignTokens.space3),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(title, style: theme.textTheme.titleLarge),
                      Text(
                        subtitle,
                        style: theme.textTheme.bodySmall?.copyWith(
                          color: theme.colorScheme.onSurfaceVariant,
                        ),
                      ),
                    ],
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// Engine status card: loading / failure / live data from the bridge.
class _EngineStatusCard extends StatelessWidget {
  const _EngineStatusCard.loading() : info = null, error = null;

  const _EngineStatusCard.data(this.info) : error = null;

  const _EngineStatusCard.error(this.error) : info = null;

  final SystemInfo? info;
  final Object? error;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colorScheme = theme.colorScheme;

    final (icon, iconColor, title, subtitle) = switch ((info, error)) {
      (final SystemInfo data, _) => (
        Icons.verified_outlined,
        colorScheme.primary,
        'Rust engine connected',
        'Engine ${data.os}/${data.arch} · ${data.capabilities.length} '
            'capabilities',
      ),
      (null, null) => (
        Icons.hourglass_empty,
        colorScheme.onSurfaceVariant,
        'Starting Rust engine…',
        'Loading bridge and host information.',
      ),
      // Any other state is a load failure (the card states are disjoint).
      (_, _) => (
        Icons.error_outline,
        colorScheme.error,
        'Rust engine unavailable',
        'The engine could not be loaded. Catalog browsing stays unavailable '
            'until this is fixed; diagnostics will show details.',
      ),
    };

    return Card(
      elevation: 0,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(12),
        side: BorderSide(color: colorScheme.outlineVariant),
      ),
      child: Padding(
        padding: const EdgeInsets.all(DesignTokens.space4),
        child: Row(
          children: [
            Icon(icon, color: iconColor),
            const SizedBox(width: DesignTokens.space3),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(title, style: theme.textTheme.titleMedium),
                  const SizedBox(height: DesignTokens.space1),
                  Text(
                    subtitle,
                    style: theme.textTheme.bodySmall?.copyWith(
                      color: colorScheme.onSurfaceVariant,
                    ),
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}
