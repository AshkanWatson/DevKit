import 'package:material_ui/material_ui.dart';
import 'package:go_router/go_router.dart';

import '../../app/router.dart';
import 'empty_state.dart';

/// Phase 0 placeholder for the six navigation destinations.
///
/// DESIGN.md §11 assigns real screens to Phase 1; each placeholder states
/// what will live here so navigation can be tested now.
class PlaceholderScreen extends StatelessWidget {
  const PlaceholderScreen({super.key, required this.destination});

  final AppDestination destination;

  String get _title => switch (destination) {
    AppDestination.home => 'Home',
    AppDestination.explore => 'Explore',
    AppDestination.profiles => 'Profiles',
    AppDestination.installed => 'Installed',
    AppDestination.downloads => 'Downloads',
    AppDestination.settings => 'Settings',
  };

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: EmptyState(
        title: '$_title — coming in Phase 1',
        message: 'This navigation destination is part of the app shell. Its screen is built in Phase 1 (DESIGN.md §11).',
        actionLabel: destination == AppDestination.home ? null : 'Go to Home',
        onAction: destination == AppDestination.home
            ? null
            : () => context.go('/home'),
      ),
    );
  }
}
