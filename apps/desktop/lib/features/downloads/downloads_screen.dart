import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';

import '../../app/theme/design_tokens.dart';
import '../../core/domain/domain_models.dart';
import '../../core/providers/repository_providers.dart';
import '../../core/widgets/common_widgets.dart';
import '../../core/widgets/empty_state.dart';

/// Downloads destination (DESIGN.md §4): queue and active jobs with stage
/// labels and progress (never a fake percentage — indeterminate stages
/// show an indeterminate bar), expandable structured logs, cancel where
/// safe, and completed/failed history with retry.
///
/// NOTE: this screen must not use `tester.pumpAndSettle` in widget tests
/// while any job is active — the indeterminate progress bar animates
/// continuously by design.
class DownloadsScreen extends ConsumerWidget {
  const DownloadsScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final jobs = ref.watch(jobsProvider);

    return Scaffold(
      body: jobs.when(
        loading: () => const Center(child: CircularProgressIndicator()),
        error: (error, _) => ErrorState(
          title: 'Job history could not be loaded',
          error: error,
          onRetry: () => ref.invalidate(jobsProvider),
        ),
        data: (list) {
          final active = list.where((job) => job.isActive).toList();
          final history = list.where((job) => !job.isActive).toList();
          if (list.isEmpty) {
            return EmptyState(
              title: 'Nothing downloading yet.',
              message:
                  'Explore developer tools and install your first package.',
              actionLabel: 'Explore',
              onAction: () => context.go('/explore'),
            );
          }
          return ListView(
            padding: const EdgeInsets.all(DesignTokens.space5),
            children: [
              if (active.isNotEmpty) ...[
                Text('Active', style: Theme.of(context).textTheme.titleMedium),
                const SizedBox(height: DesignTokens.space3),
                for (final job in active) _JobCard(job: job),
                const SizedBox(height: DesignTokens.space5),
              ],
              if (history.isNotEmpty) ...[
                Text('History', style: Theme.of(context).textTheme.titleMedium),
                const SizedBox(height: DesignTokens.space3),
                for (final job in history) _JobCard(job: job),
              ],
            ],
          );
        },
      ),
    );
  }
}

class _JobCard extends ConsumerWidget {
  const _JobCard({required this.job});

  final JobInfo job;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final theme = Theme.of(context);

    return Card(
      elevation: 0,
      margin: const EdgeInsets.only(bottom: DesignTokens.space3),
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
                      Text(
                        '${job.packageLabel} ${job.version}',
                        style: theme.textTheme.titleMedium,
                      ),
                      Text(
                        job.id,
                        style: theme.textTheme.bodySmall?.copyWith(
                          color: theme.colorScheme.onSurfaceVariant,
                        ),
                      ),
                    ],
                  ),
                ),
                _StageChip(stage: job.stage, errorCode: job.errorCode),
              ],
            ),
            const SizedBox(height: DesignTokens.space3),
            // Active jobs get a progress row; an indeterminate stage shows
            // an indeterminate bar and NO percentage (§8). Terminal jobs
            // show their final state as plain text — a completed or failed
            // job has no meaningful progress bar to animate forever.
            if (job.isActive)
              ProgressStage(stage: job.stage, progress: job.progress)
            else
              Text(
                job.stage.label,
                style: theme.textTheme.bodyMedium?.copyWith(
                  color: theme.colorScheme.onSurfaceVariant,
                ),
              ),
            const SizedBox(height: DesignTokens.space3),
            Row(
              children: [
                if (job.canBeCancelled)
                  OutlinedButton.icon(
                    onPressed: () async {
                      // Cancellation is cooperative and safe; no
                      // confirmation needed (DESIGN.md §4: cancel where
                      // safe).
                      await ref.read(jobsRepositoryProvider).cancelJob(job.id);
                    },
                    icon: const Icon(Icons.close),
                    label: const Text('Cancel'),
                  ),
                if (job.canBeRetried) ...[
                  FilledButton.icon(
                    onPressed: () {
                      // Retry creates a NEW job (idempotent retry rule,
                      // installation-lifecycle.md §3.5); the failed job's
                      // history remains.
                      ref
                          .read(jobsRepositoryProvider)
                          .startInstall(
                            packageId: job.packageId,
                            packageLabel: job.packageLabel,
                            version: job.version,
                          );
                    },
                    icon: const Icon(Icons.refresh),
                    label: const Text('Retry'),
                  ),
                  const SizedBox(width: DesignTokens.space2),
                ],
                if (job.errorCode != null)
                  Expanded(
                    child: Text(
                      'Failed with ${job.errorCode} — see the log for the '
                      'simulated failure details.',
                      style: theme.textTheme.bodySmall?.copyWith(
                        color: theme.colorScheme.error,
                      ),
                    ),
                  ),
              ],
            ),
            // Expandable structured log (DESIGN.md §4, §7).
            if (job.log.isNotEmpty)
              Theme(
                data: theme.copyWith(dividerColor: Colors.transparent),
                child: ExpansionTile(
                  tilePadding: EdgeInsets.zero,
                  childrenPadding: const EdgeInsets.only(
                    bottom: DesignTokens.space3,
                  ),
                  title: Text(
                    'Structured log (${job.log.length} events)',
                    style: theme.textTheme.bodyMedium,
                  ),
                  children: [
                    Container(
                      width: double.infinity,
                      padding: const EdgeInsets.all(DesignTokens.space3),
                      decoration: BoxDecoration(
                        color: theme.colorScheme.surfaceContainerHighest,
                        borderRadius: BorderRadius.circular(8),
                      ),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          for (final entry in job.log)
                            Padding(
                              padding: const EdgeInsets.symmetric(vertical: 1),
                              child: Text(
                                '${entry.stage.wire}'
                                '${entry.progress != null ? ' ${(entry.progress! * 100).round()}%' : ''}'
                                ' — ${entry.message}'
                                '${entry.errorCode != null ? ' [${entry.errorCode}]' : ''}',
                                style: theme.textTheme.bodySmall?.copyWith(
                                  fontFeatures: const [
                                    FontFeature.tabularFigures(),
                                  ],
                                ),
                              ),
                            ),
                        ],
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

class _StageChip extends StatelessWidget {
  const _StageChip({required this.stage, this.errorCode});

  final JobStage stage;
  final String? errorCode;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final (icon, color) = switch (stage) {
      JobStage.completed => (Icons.check_circle_outline, Colors.green),
      JobStage.failed => (Icons.error_outline, theme.colorScheme.error),
      JobStage.cancelled ||
      JobStage.rollingBack ||
      JobStage.rolledBack => (Icons.undo, theme.colorScheme.onSurfaceVariant),
      _ => (Icons.schedule, theme.colorScheme.primary),
    };
    return Tooltip(
      message: stage.wire,
      child: Chip(
        avatar: Icon(icon, size: 16, color: color),
        label: Text(stage.label),
        labelStyle: theme.textTheme.labelMedium,
        visualDensity: VisualDensity.compact,
      ),
    );
  }
}
