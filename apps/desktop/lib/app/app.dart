import 'package:material_ui/material_ui.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../core/settings/settings_providers.dart';
import 'router.dart';
import 'theme/app_theme.dart';

/// Root widget: theme configuration and navigation only.
class DevKitApp extends ConsumerWidget {
  const DevKitApp({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final router = ref.watch(appRouterProvider);
    // The persisted theme mode (P1-07); follows the system until the
    // persisted value (or a test override) resolves.
    final themeMode = ref.watch(themeModeProvider).value ?? ThemeMode.system;
    return MaterialApp.router(
      title: 'DevKit',
      debugShowCheckedModeBanner: false,
      theme: AppTheme.light(),
      darkTheme: AppTheme.dark(),
      themeMode: themeMode,
      routerConfig: router,
    );
  }
}
