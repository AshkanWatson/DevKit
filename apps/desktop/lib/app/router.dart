import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../core/widgets/app_shell.dart';
import '../features/downloads/downloads_screen.dart';
import '../features/explore/explore_screen.dart';
import '../features/home/home_screen.dart';
import '../features/installed/installed_screen.dart';
import '../features/package_details/package_details_screen.dart';
import '../features/profiles/profiles_screen.dart';
import '../features/settings/settings_screen.dart';

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
          StatefulShellBranch(
            routes: [
              GoRoute(
                path: '/home',
                builder: (context, state) => const HomeScreen(),
              ),
            ],
          ),
          StatefulShellBranch(
            routes: [
              GoRoute(
                path: '/explore',
                builder: (context, state) => const ExploreScreen(),
              ),
              GoRoute(
                path: '/explore/:packageId',
                builder: (context, state) => PackageDetailsScreen(
                  packageId: state.pathParameters['packageId']!,
                ),
              ),
            ],
          ),
          StatefulShellBranch(
            routes: [
              GoRoute(
                path: '/profiles',
                builder: (context, state) => const ProfilesScreen(),
              ),
            ],
          ),
          StatefulShellBranch(
            routes: [
              GoRoute(
                path: '/installed',
                builder: (context, state) => const InstalledScreen(),
              ),
            ],
          ),
          StatefulShellBranch(
            routes: [
              GoRoute(
                path: '/downloads',
                builder: (context, state) => const DownloadsScreen(),
              ),
            ],
          ),
          StatefulShellBranch(
            routes: [
              GoRoute(
                path: '/settings',
                // P1-07: real Settings screen (theme mode, storage
                // location, diagnostics stub).
                builder: (context, state) => const SettingsScreen(),
              ),
            ],
          ),
        ],
      ),
    ],
  );
});
