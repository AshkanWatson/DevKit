// P5-02: golden tests for a small, stable set of high-value screens/widgets
// (testing-strategy.md §2). Only deterministic components are captured:
// - the navigation rail (shell identity),
// - a package card (Explore list item),
// - the job progress stage (Downloads), at measurable and indeterminate states.
//
// The theme is pinned (fixed color scheme, single fallback font, fixed text
// scale) and test/flutter_test_config.dart provides a cross-platform tolerant
// comparator so the committed baselines work on Linux/macOS/Windows CI runners.
// No network, no engine — all data is constructed locally.

import 'package:devkit/app/theme/app_theme.dart';
import 'package:devkit/core/domain/domain_models.dart';
import 'package:devkit/core/widgets/common_widgets.dart';
import 'package:devkit/core/widgets/package_card.dart';
import 'package:devkit/core/widgets/sidebar_navigation.dart';
import 'package:material_ui/material_ui.dart';
import 'package:flutter_test/flutter_test.dart';

Widget _harness(Widget child) {
  return MediaQuery(
    data: const MediaQueryData(
      size: Size(800, 600),
      textScaler: TextScaler.linear(1.0),
      devicePixelRatio: 1.0,
    ),
    child: MaterialApp(
      debugShowCheckedModeBanner: false,
      theme: AppTheme.light(),
      home: Scaffold(body: SizedBox(width: 480, height: 400, child: child)),
    ),
  );
}

void main() {
  testWidgets('navigation rail golden', (tester) async {
    await tester.pumpWidget(
      _harness(
        SidebarNavigation(
          selectedIndex: 0,
          onDestinationSelected: (_) {},
          compact: true,
        ),
      ),
    );
    await tester.pumpAndSettle();
    await expectLater(
      find.byType(SidebarNavigation),
      matchesGoldenFile('goldens/navigation_rail.png'),
    );
  });

  testWidgets('progress stage measurable golden', (tester) async {
    await tester.pumpWidget(
      _harness(const ProgressStage(stage: JobStage.downloading, progress: 0.5)),
    );
    await expectLater(
      find.byType(ProgressStage),
      matchesGoldenFile('goldens/progress_stage_measurable.png'),
    );
  });

  testWidgets('progress stage indeterminate golden', (tester) async {
    await tester.pumpWidget(
      _harness(const ProgressStage(stage: JobStage.verifying)),
    );
    await expectLater(
      find.byType(ProgressStage),
      matchesGoldenFile('goldens/progress_stage_indeterminate.png'),
    );
  });

  testWidgets('package card golden', (tester) async {
    const package = CatalogPackage(
      id: 'devkit.lang.python',
      name: 'Python',
      description: 'The Python programming language.',
      publisher: 'Python Software Foundation',
      homepage: 'https://www.python.org',
      category: PackageCategory.language,
      platforms: [PlatformTarget.windowsX64],
      defaultPolicy: VersionPolicyName.stable,
      versions: [],
      license: 'PSF-2.0',
      updatedAt: '2026-01-01T00:00:00Z',
    );
    await tester.pumpWidget(
      _harness(PackageCard(package: package, onOpen: () {})),
    );
    await expectLater(
      find.byType(PackageCard),
      matchesGoldenFile('goldens/package_card.png'),
    );
  });
}
