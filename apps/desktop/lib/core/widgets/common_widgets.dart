import 'package:material_ui/material_ui.dart';

import '../domain/domain_models.dart';

/// Confirmation dialog for destructive actions (DESIGN.md §7/§8:
/// "destructive actions require confirmation").
///
/// Returns true when the user confirmed the action.
Future<bool> confirmDestructive(
  BuildContext context, {
  required String title,
  required String message,
  String confirmLabel = 'Confirm',
}) async {
  final result = await showDialog<bool>(
    context: context,
    builder: (context) => AlertDialog(
      title: Text(title),
      content: Text(message),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(false),
          child: const Text('Cancel'),
        ),
        FilledButton(
          style: FilledButton.styleFrom(
            backgroundColor: Theme.of(context).colorScheme.error,
            foregroundColor: Theme.of(context).colorScheme.onError,
          ),
          onPressed: () => Navigator.of(context).pop(true),
          child: Text(confirmLabel),
        ),
      ],
    ),
  );
  return result ?? false;
}

/// Install plan review (DESIGN.md §7 `InstallPlanReview`, §8: "Install
/// actions show a review step before execution"). Shows exactly what will
/// happen before the (simulated) job starts.
Future<bool> showInstallPlanReview(
  BuildContext context, {
  required String packageName,
  required String version,
  required String policyLabel,
  required String sourceDomain,
  required String sizeLabel,
  required String note,
}) async {
  final theme = Theme.of(context);
  final result = await showDialog<bool>(
    context: context,
    builder: (context) => AlertDialog(
      title: Text('Review install — $packageName $version'),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            'Version policy: $policyLabel',
            style: theme.textTheme.bodyMedium,
          ),
          const SizedBox(height: 4),
          Text('Source: $sourceDomain', style: theme.textTheme.bodyMedium),
          const SizedBox(height: 4),
          Text('Download size: $sizeLabel', style: theme.textTheme.bodyMedium),
          const SizedBox(height: 4),
          Text('License note: $note', style: theme.textTheme.bodySmall),
          const SizedBox(height: 12),
          Text(
            'Phase 1 runs a simulated install job; nothing is downloaded '
            'or installed.',
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
          child: const Text('Start install'),
        ),
      ],
    ),
  );
  return result ?? false;
}

/// Error state (DESIGN.md §7: components must support error states).
class ErrorState extends StatelessWidget {
  const ErrorState({
    super.key,
    required this.title,
    required this.error,
    this.onRetry,
  });

  final String title;
  final Object error;
  final VoidCallback? onRetry;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(24),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(Icons.error_outline, color: theme.colorScheme.error),
            const SizedBox(height: 8),
            Text(title, style: theme.textTheme.titleMedium),
            const SizedBox(height: 4),
            Text(
              'Could not load data: $error',
              style: theme.textTheme.bodySmall,
              textAlign: TextAlign.center,
            ),
            if (onRetry != null) ...[
              const SizedBox(height: 16),
              OutlinedButton(onPressed: onRetry, child: const Text('Retry')),
            ],
          ],
        ),
      ),
    );
  }
}

/// Version selector (DESIGN.md §4/§7): stable/latest/exact policy plus a
/// version dropdown for the exact case. Exact selection never silently
/// resolves to another version — the chosen version string is shown
/// verbatim in the plan review.
class VersionSelector extends StatelessWidget {
  const VersionSelector({
    super.key,
    required this.policy,
    required this.onPolicyChanged,
    required this.exactVersion,
    required this.versions,
    required this.onExactVersionChanged,
    required this.resolvedVersion,
  });

  final VersionPolicyName policy;
  final ValueChanged<VersionPolicyName> onPolicyChanged;
  final String exactVersion;
  final List<PackageVersionInfo> versions;
  final ValueChanged<String> onExactVersionChanged;

  /// The version the current policy resolves to (preview only; never
  /// silently changes after the plan is confirmed).
  final String resolvedVersion;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text('Version policy', style: theme.textTheme.titleSmall),
        const SizedBox(height: 8),
        SegmentedButton<VersionPolicyName>(
          segments: const [
            ButtonSegment(
              value: VersionPolicyName.stable,
              label: Text('Stable'),
              icon: Icon(Icons.verified_outlined),
            ),
            ButtonSegment(
              value: VersionPolicyName.latest,
              label: Text('Latest'),
              icon: Icon(Icons.arrow_upward),
            ),
            ButtonSegment(
              value: VersionPolicyName.exact,
              label: Text('Exact'),
              icon: Icon(Icons.push_pin_outlined),
            ),
          ],
          selected: {policy},
          onSelectionChanged: (selection) => onPolicyChanged(selection.first),
        ),
        const SizedBox(height: 12),
        if (policy == VersionPolicyName.exact)
          DropdownMenu<String>(
            initialSelection: exactVersion,
            width: 240,
            label: const Text('Exact version'),
            dropdownMenuEntries: [
              for (final version in versions)
                DropdownMenuEntry(
                  value: version.version,
                  label: version.isPrerelease
                      ? '${version.version} (${version.channel.wire})'
                      : version.version,
                ),
            ],
            onSelected: (value) {
              if (value != null) onExactVersionChanged(value);
            },
          )
        else
          Text(
            'Resolves to: $resolvedVersion',
            style: theme.textTheme.bodySmall?.copyWith(
              color: theme.colorScheme.onSurfaceVariant,
            ),
          ),
      ],
    );
  }
}

/// Progress stage row (DESIGN.md §7 `ProgressStage`, §8: no fake
/// percentages for indeterminate stages).
class ProgressStage extends StatelessWidget {
  const ProgressStage({super.key, required this.stage, this.progress});

  final JobStage stage;

  /// 0.0..1.0 when measurable; null shows an indeterminate bar.
  final double? progress;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Expanded(
              child: Text(stage.label, style: theme.textTheme.bodyMedium),
            ),
            // Percentage ONLY when progress is measurable.
            if (progress != null)
              Text(
                '${(progress! * 100).round()}%',
                style: theme.textTheme.bodySmall,
              ),
          ],
        ),
        const SizedBox(height: 4),
        LinearProgressIndicator(
          value: progress,
          minHeight: 4,
          borderRadius: BorderRadius.circular(2),
        ),
      ],
    );
  }
}
