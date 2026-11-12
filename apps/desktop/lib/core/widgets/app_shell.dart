import 'package:flutter/material.dart';
import 'package:go_router/go_router.dart';

import '../../app/theme/design_tokens.dart';
import 'sidebar_navigation.dart';

/// Responsive desktop layout (DESIGN.md §2, §9):
/// - Wide windows: persistent sidebar plus content.
/// - Medium windows: compact collapsible sidebar.
/// - Narrow windows: navigation drawer and single-column content.
class AppShell extends StatelessWidget {
  const AppShell({super.key, required this.shell});

  final StatefulNavigationShell shell;

  @override
  Widget build(BuildContext context) {
    final width = MediaQuery.sizeOf(context).width;
    if (width < DesignTokens.breakpointNarrow) {
      return _NarrowShell(shell: shell);
    }
    return Scaffold(
      body: SafeArea(
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            SidebarNavigation(
              selectedIndex: shell.currentIndex,
              onDestinationSelected: (index) => _go(shell, index),
              compact: width < DesignTokens.breakpointWide,
            ),
            const VerticalDivider(width: 1),
            Expanded(child: shell),
          ],
        ),
      ),
    );
  }
}

/// Narrow windows: navigation drawer and single-column content.
class _NarrowShell extends StatelessWidget {
  const _NarrowShell({required this.shell});

  final StatefulNavigationShell shell;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      drawer: Drawer(
        child: SafeArea(
          child: SidebarNavigation(
            selectedIndex: shell.currentIndex,
            onDestinationSelected: (index) {
              Navigator.of(context).pop(); // close the drawer
              _go(shell, index);
            },
          ),
        ),
      ),
      appBar: AppBar(title: const Text('DevKit')),
      body: shell,
    );
  }
}

void _go(StatefulNavigationShell shell, int index) {
  shell.goBranch(index, initialLocation: index == shell.currentIndex);
}
