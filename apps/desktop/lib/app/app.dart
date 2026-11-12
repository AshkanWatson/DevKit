import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'router.dart';
import 'theme/app_theme.dart';

/// Root widget: theme configuration and navigation only.
///
/// Phase 0 scope (DESIGN.md §11): app shell and navigation. Screens are
/// placeholders; real features arrive in Phase 1.
class DevKitApp extends ConsumerWidget {
  const DevKitApp({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final router = ref.watch(appRouterProvider);
    return MaterialApp.router(
      title: 'DevKit',
      debugShowCheckedModeBanner: false,
      theme: AppTheme.light(),
      darkTheme: AppTheme.dark(),
      // Phase 1 adds a settings-driven theme mode (system/light/dark).
      themeMode: ThemeMode.system,
      routerConfig: router,
    );
  }
}
