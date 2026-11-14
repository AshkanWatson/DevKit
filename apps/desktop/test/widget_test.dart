import 'package:devkit/core/bridge/api.dart';
import 'package:devkit/core/bridge/bridge_providers.dart';
import 'package:devkit/core/widgets/empty_state.dart';
import 'package:devkit/main.dart';
import 'package:material_ui/material_ui.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  // Pumps the real composition root (DevKitRoot) so a missing
  // ProviderScope or equivalent composition error fails here, not in
  // release builds as a white screen.
  //
  // The Rust bridge provider is overridden: the FFI dynamic library is not
  // loadable in the plain test environment, and widget tests must never
  // depend on it (repo rule: no real toolchain work in tests).
  const fakeSystemInfo = SystemInfo(
    os: 'windows',
    arch: 'x86_64',
    capabilities: ['sqlite_registry', 'catalog_client'],
  );

  Widget buildApp() {
    return DevKitRoot(
      overrides: [
        systemInfoProvider.overrideWith((ref) => Future.value(fakeSystemInfo)),
      ],
    );
  }

  /// The NavigationRail shows the same icons as body content on some
  /// screens; scope nav taps to the rail so finders stay unambiguous.
  Finder railIcon(IconData icon) => find.descendant(
    of: find.byType(NavigationRail),
    matching: find.byIcon(icon),
  );

  testWidgets('app shell shows all six navigation destinations', (
    tester,
  ) async {
    await tester.pumpWidget(buildApp());
    await tester.pumpAndSettle();
    expect(find.text('Home'), findsOneWidget);
    expect(find.text('Explore'), findsOneWidget);
    expect(find.text('Profiles'), findsOneWidget);
    expect(find.text('Installed'), findsOneWidget);
    expect(find.text('Downloads'), findsOneWidget);
    expect(find.text('Settings'), findsOneWidget);
  });

  testWidgets('settings shows appearance, storage, and diagnostics sections', (
    tester,
  ) async {
    await tester.pumpWidget(buildApp());
    await tester.pumpAndSettle();
    await tester.tap(railIcon(Icons.settings_outlined));
    await tester.pumpAndSettle();
    expect(find.text('Appearance'), findsOneWidget);
    expect(find.text('Storage'), findsOneWidget);
    expect(find.text('Diagnostics'), findsOneWidget);
  });

  testWidgets('home shows the live Rust engine status from the bridge', (
    tester,
  ) async {
    await tester.pumpWidget(buildApp());
    await tester.pumpAndSettle();
    expect(find.text('Rust engine connected'), findsOneWidget);
    expect(find.text('Engine windows/x86_64 · 2 capabilities'), findsOneWidget);
  });

  testWidgets('home shows a fixture-data summary linking to destinations', (
    tester,
  ) async {
    await tester.pumpWidget(buildApp());
    await tester.pumpAndSettle();
    expect(find.text('Installed packages'), findsOneWidget);
    expect(find.text('Active jobs'), findsOneWidget);
  });

  testWidgets('navigating to Explore shows the fixture catalog', (
    tester,
  ) async {
    await tester.pumpWidget(buildApp());
    await tester.pumpAndSettle();
    await tester.tap(railIcon(Icons.explore_outlined));
    await tester.pumpAndSettle();
    expect(find.text('Python'), findsOneWidget);
    expect(find.text('Flutter'), findsOneWidget);
  });

  testWidgets('responsive layout: drawer below 700px, compact rail at '
      '700–1100px, extended rail above 1100px', (tester) async {
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);

    // Narrow: navigation drawer with an app bar, single column. The
    // closed drawer is not built until opened, so assert the app bar and
    // the absence of a persistent rail.
    tester.view.physicalSize = const Size(640, 800);
    await tester.pumpWidget(buildApp());
    await tester.pumpAndSettle();
    expect(find.byType(AppBar), findsOneWidget);
    expect(find.byType(NavigationRail), findsNothing);

    // Medium: persistent rail, compact (icons only, not extended).
    tester.view.physicalSize = const Size(900, 700);
    await tester.pumpWidget(buildApp());
    await tester.pumpAndSettle();
    expect(find.byType(AppBar), findsNothing);
    expect(
      tester.widget<NavigationRail>(find.byType(NavigationRail)).extended,
      isFalse,
    );

    // Wide: persistent extended rail.
    tester.view.physicalSize = const Size(1280, 800);
    await tester.pumpWidget(buildApp());
    await tester.pumpAndSettle();
    expect(
      tester.widget<NavigationRail>(find.byType(NavigationRail)).extended,
      isTrue,
    );
  });

  testWidgets('EmptyState renders title, message, and action', (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: EmptyState(
            title: 'No packages',
            message: 'Search to get started.',
            actionLabel: 'Browse',
            onAction: () {},
          ),
        ),
      ),
    );
    expect(find.text('No packages'), findsOneWidget);
    expect(find.text('Search to get started.'), findsOneWidget);
    expect(find.text('Browse'), findsOneWidget);
  });
}
