import 'package:material_ui/material_ui.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/bridge/api.dart';
import '../../core/bridge/bridge_providers.dart';
import '../../core/widgets/empty_state.dart';
import '../../app/theme/design_tokens.dart';

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
          const EmptyState(
            title: 'Home — coming in Phase 1',
            message:
                'The dashboard with installed versions, downloads, and '
                'profiles is built in Phase 1 (DESIGN.md §11).',
          ),
        ],
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
