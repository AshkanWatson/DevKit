import 'package:material_ui/material_ui.dart';

import '../../app/theme/design_tokens.dart';

/// Sidebar navigation (DESIGN.md §2, §7).
///
/// Destinations match [AppDestination] order in app/router.dart; keep the
/// two lists in sync.
class SidebarNavigation extends StatelessWidget {
  const SidebarNavigation({
    super.key,
    required this.selectedIndex,
    required this.onDestinationSelected,
    this.compact = false,
  });

  final int selectedIndex;
  final ValueChanged<int> onDestinationSelected;

  /// Compact mode shows icons only (medium windows, DESIGN.md §9).
  final bool compact;

  static const _destinations = [
    (Icons.home_outlined, Icons.home, 'Home'),
    (Icons.explore_outlined, Icons.explore, 'Explore'),
    (Icons.people_outline, Icons.people, 'Profiles'),
    (Icons.inventory_2_outlined, Icons.inventory_2, 'Installed'),
    (Icons.download_outlined, Icons.download, 'Downloads'),
    (Icons.settings_outlined, Icons.settings, 'Settings'),
  ];

  @override
  Widget build(BuildContext context) {
    return NavigationRail(
      selectedIndex: selectedIndex,
      onDestinationSelected: onDestinationSelected,
      extended: !compact,
      minExtendedWidth: DesignTokens.sidebarWidth,
      labelType: compact
          ? NavigationRailLabelType.none
          : NavigationRailLabelType.none,
      leading: Padding(
        padding: const EdgeInsets.all(DesignTokens.space4),
        child: Text('DevKit', style: Theme.of(context).textTheme.titleMedium),
      ),
      destinations: [
        for (final (icon, selectedIcon, label) in _destinations)
          NavigationRailDestination(
            icon: Icon(icon),
            selectedIcon: Icon(selectedIcon),
            label: Text(label),
          ),
      ],
    );
  }
}
