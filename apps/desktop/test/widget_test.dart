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

  testWidgets('placeholder screens state they are Phase 1 work', (
    tester,
  ) async {
    await tester.pumpWidget(buildApp());
    await tester.pumpAndSettle();
    expect(find.text('Home — coming in Phase 1'), findsOneWidget);
  });

  testWidgets('home shows the live Rust engine status from the bridge', (
    tester,
  ) async {
    await tester.pumpWidget(buildApp());
    await tester.pumpAndSettle();
    expect(find.text('Rust engine connected'), findsOneWidget);
    expect(find.text('Engine windows/x86_64 · 2 capabilities'), findsOneWidget);
  });

  testWidgets('navigating to Explore shows the Explore placeholder', (
    tester,
  ) async {
    await tester.pumpWidget(buildApp());
    await tester.pumpAndSettle();
    await tester.tap(find.text('Explore'));
    await tester.pumpAndSettle();
    expect(find.text('Explore — coming in Phase 1'), findsOneWidget);
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
