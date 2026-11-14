import 'package:devkit/core/bridge/api.dart';
import 'package:devkit/core/bridge/bridge_providers.dart';
import 'package:devkit/core/settings/settings_providers.dart';
import 'package:devkit/core/settings/settings_repository.dart';
import 'package:devkit/main.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';

/// P1-07 Settings tests: theme mode persistence, storage location
/// display, redacted diagnostics stub.
///
/// The settings repository is overridden with an in-memory fake so tests
/// never write real user preferences; the bridge providers are overridden
/// so widget tests never touch real FFI.
/// In-memory settings: survives across "restarts" within a test.
class _InMemorySettings implements SettingsRepository {
  final Map<String, String> values = {};

  @override
  Future<String?> read(String key) async => values[key];

  @override
  Future<void> write(String key, String value) async {
    values[key] = value;
  }
}

void main() {
  const fakeSystemInfo = SystemInfo(
    os: 'windows',
    arch: 'x86_64',
    capabilities: ['sqlite_registry', 'catalog_client'],
  );
  const fakeAppInfo = AppInfo(version: '1.2.3', engineName: 'devkit-bridge');

  Widget buildApp({SettingsRepository? settings}) {
    // KeyedSubtree with a fresh key forces a full unmount when the test
    // pumps a second "app instance": pumpWidget would otherwise update
    // the existing element tree in place (same root type), keeping the
    // old router and provider container alive.
    return KeyedSubtree(
      key: UniqueKey(),
      child: DevKitRoot(
        overrides: [
          systemInfoProvider.overrideWith(
            (ref) => Future.value(fakeSystemInfo),
          ),
          appInfoProvider.overrideWith((ref) => Future.value(fakeAppInfo)),
          settingsRepositoryProvider.overrideWithValue(
            settings ?? _InMemorySettings(),
          ),
        ],
      ),
    );
  }

  Future<void> goToSettings(WidgetTester tester) async {
    await tester.tap(
      find.descendant(
        of: find.byType(NavigationRail),
        matching: find.byIcon(Icons.settings_outlined),
      ),
    );
    await tester.pumpAndSettle();
  }

  testWidgets('theme mode selection persists across a restart', (tester) async {
    final settings = _InMemorySettings();
    await tester.pumpWidget(buildApp(settings: settings));
    await tester.pumpAndSettle();
    await goToSettings(tester);

    // Default: follow system.
    expect(
      tester.widget<MaterialApp>(find.byType(MaterialApp)).themeMode,
      ThemeMode.system,
    );

    await tester.tap(find.text('Light'));
    await tester.pumpAndSettle();
    expect(
      tester.widget<MaterialApp>(find.byType(MaterialApp)).themeMode,
      ThemeMode.light,
    );
    expect(settings.values['theme_mode'], 'light');

    // "Restart": a brand-new root reading the same persisted settings.
    await tester.pumpWidget(buildApp(settings: settings));
    await tester.pumpAndSettle();
    expect(
      tester.widget<MaterialApp>(find.byType(MaterialApp)).themeMode,
      ThemeMode.light,
    );

    // Dark persists the same way.
    await goToSettings(tester);
    await tester.tap(find.text('Dark'));
    await tester.pumpAndSettle();
    expect(
      tester.widget<MaterialApp>(find.byType(MaterialApp)).themeMode,
      ThemeMode.dark,
    );
    await tester.pumpWidget(buildApp(settings: settings));
    await tester.pumpAndSettle();
    expect(
      tester.widget<MaterialApp>(find.byType(MaterialApp)).themeMode,
      ThemeMode.dark,
    );
  });

  testWidgets('storage location is displayed with the OS-standard default', (
    tester,
  ) async {
    await tester.pumpWidget(buildApp());
    await tester.pumpAndSettle();
    await goToSettings(tester);

    expect(find.textContaining('devkit.sqlite'), findsOneWidget);
    expect(find.textContaining('state'), findsWidgets);
  });

  testWidgets('diagnostics preview shows redacted facts only', (tester) async {
    await tester.pumpWidget(buildApp());
    await tester.pumpAndSettle();
    await goToSettings(tester);

    final diagnosticsTile = find.text('Diagnostics preview (redacted)');
    await tester.ensureVisible(diagnosticsTile);
    await tester.pumpAndSettle();
    await tester.tap(diagnosticsTile);
    await tester.pumpAndSettle();

    // Whitelisted facts only.
    expect(find.textContaining('DevKit 0.1.0+1'), findsOneWidget);
    expect(find.textContaining('windows/x86_64'), findsOneWidget);
    expect(find.textContaining('local fixtures'), findsOneWidget);

    // Never an environment dump, credentials, or file contents.
    expect(
      find.textContaining(RegExp('TOKEN|PASSWORD|API_KEY|SECRET')),
      findsNothing,
    );
    expect(
      find.textContaining('='),
      findsNothing,
      reason: 'key=value pairs would imply unredacted data',
    );
  });
}
