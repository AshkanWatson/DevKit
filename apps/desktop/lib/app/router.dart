import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../core/widgets/app_shell.dart';
import '../core/widgets/placeholder_screen.dart';

/// Primary navigation destinations (DESIGN.md §2). The order defines the
/// sidebar order and must stay in sync with AppShell.
enum AppDestination { home, explore, profiles, installed, downloads, settings }

final appRouterProvider = Provider<GoRouter>((ref) {
  return GoRouter(
    initialLocation: '/home',
    routes: [
      StatefulShellRoute.indexedStack(
        builder: (context, state, shell) => AppShell(shell: shell),
        branches: [
          for (final destination in AppDestination.values)
            StatefulShellBranch(
              routes: [
                GoRoute(
                  path: '/${destination.name}',
                  builder: (context, state) =>
                      PlaceholderScreen(destination: destination),
                ),
              ],
            ),
        ],
      ),
    ],
  );
});
