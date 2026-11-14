import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:material_ui/material_ui.dart';

import '../../app/theme/design_tokens.dart';
import '../../core/bridge/api.dart';
import '../../core/bridge/bridge_providers.dart';
import '../../core/settings/settings_providers.dart';

/// Settings destination (DESIGN.md §4, P1-07): theme mode
/// (system/light/dark) persisted across restarts, storage location
/// display, and a diagnostics stub that shows only redacted,
/// non-sensitive facts (acceptance criterion: no secrets displayed).
class SettingsScreen extends ConsumerWidget {
  const SettingsScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final theme = Theme.of(context);
    final themeMode = ref.watch(themeModeProvider);
    final storageLocation = ref.watch(storageLocationProvider);
    final appInfo = ref.watch(appInfoProvider);
    final systemInfo = ref.watch(systemInfoProvider);

    final currentMode = themeMode.value ?? ThemeMode.system;

    return Scaffold(
      body: ListView(
        padding: const EdgeInsets.all(DesignTokens.space5),
        children: [
          Text('Settings', style: theme.textTheme.headlineSmall),
          const SizedBox(height: DesignTokens.space4),

          // Appearance — persisted across restarts (P1-07). RadioGroup
          // owns the group value (RadioListTile group API is deprecated).
          _Section(
            title: 'Appearance',
            children: [
              RadioGroup<ThemeMode>(
                groupValue: currentMode,
                onChanged: (value) {
                  if (value != null) {
                    ref.read(themeModeProvider.notifier).setMode(value);
                  }
                },
                child: Column(
                  children: [
                    for (final mode in ThemeMode.values)
                      RadioListTile<ThemeMode>(
                        value: mode,
                        title: Text(_modeLabel(mode)),
                        secondary: Icon(_modeIcon(mode)),
                      ),
                  ],
                ),
              ),
            ],
          ),
          const SizedBox(height: DesignTokens.space4),

          // Storage — display only (local-database.md §1).
          _Section(
            title: 'Storage',
            children: [
              Text(
                'DevKit stores all local state in a single SQLite database '
                'at the OS-standard user data location:',
                style: theme.textTheme.bodyMedium,
              ),
              const SizedBox(height: DesignTokens.space2),
              SelectableText(
                storageLocation,
                style: theme.textTheme.bodyMedium?.copyWith(
                  fontFamily: 'Consolas',
                  fontFeatures: const [FontFeature.tabularFigures()],
                ),
              ),
              const SizedBox(height: DesignTokens.space2),
              Text(
                'The authoritative location is resolved by the DevKit '
                'engine; this is the Windows default shown for planning. '
                'Managed packages live under the same data root and are '
                'never placed outside it.',
                style: theme.textTheme.bodySmall?.copyWith(
                  color: theme.colorScheme.onSurfaceVariant,
                ),
              ),
            ],
          ),
          const SizedBox(height: DesignTokens.space4),

          // Diagnostics stub (P1-07): redacted, non-sensitive facts only.
          _Section(
            title: 'Diagnostics',
            children: [
              Text(
                'A diagnostics export for bug reports arrives with the '
                'bridge. This Phase 1 preview shows only facts that are '
                'safe to share — no environment variables, no file '
                'contents, no credentials.',
                style: theme.textTheme.bodyMedium,
              ),
              const SizedBox(height: DesignTokens.space3),
              Theme(
                data: theme.copyWith(dividerColor: Colors.transparent),
                child: ExpansionTile(
                  title: Text(
                    'Diagnostics preview (redacted)',
                    style: theme.textTheme.bodyMedium,
                  ),
                  childrenPadding: const EdgeInsets.only(
                    bottom: DesignTokens.space3,
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
                          Text(
                            'DevKit 0.1.0+1 (Phase 1)',
                            style: theme.textTheme.bodyMedium,
                          ),
                          Text(
                            'Engine: ${_engineDescription(appInfo, systemInfo)}',
                            style: theme.textTheme.bodyMedium,
                          ),
                          Text(
                            'Catalog source: local fixtures (offline)',
                            style: theme.textTheme.bodyMedium,
                          ),
                          Text(
                            'Job history: local SQLite (simulated installs '
                            'only)',
                            style: theme.textTheme.bodyMedium,
                          ),
                        ],
                      ),
                    ),
                  ],
                ),
              ),
            ],
          ),
        ],
      ),
    );
  }

  String _modeLabel(ThemeMode mode) => switch (mode) {
    ThemeMode.system => 'Follow system',
    ThemeMode.light => 'Light',
    ThemeMode.dark => 'Dark',
  };

  IconData _modeIcon(ThemeMode mode) => switch (mode) {
    ThemeMode.system => Icons.settings_suggest_outlined,
    ThemeMode.light => Icons.light_mode_outlined,
    ThemeMode.dark => Icons.dark_mode_outlined,
  };

  String _engineDescription(
    AsyncValue<AppInfo> appInfo,
    AsyncValue<SystemInfo> systemInfo,
  ) {
    final version = appInfo.value?.version ?? 'unavailable';
    final system = systemInfo.value;
    final platform = system == null
        ? 'unavailable'
        : '${system.os}/${system.arch} '
              '(${system.capabilities.length} capabilities)';
    return '$version · $platform';
  }
}

class _Section extends StatelessWidget {
  const _Section({required this.title, required this.children});

  final String title;
  final List<Widget> children;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Card(
      elevation: 0,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(12),
        side: BorderSide(color: theme.colorScheme.outlineVariant),
      ),
      child: Padding(
        padding: const EdgeInsets.all(DesignTokens.space4),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(title, style: theme.textTheme.titleMedium),
            const SizedBox(height: DesignTokens.space2),
            ...children,
          ],
        ),
      ),
    );
  }
}
